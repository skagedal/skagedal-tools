import 'dart:async';

import 'package:fake_async/fake_async.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/alarm/closing_bell_notification.dart';
import 'package:jikido/src/audio/bell_audio.dart';
import 'package:jikido/src/bell.dart';
import 'package:jikido/src/settings.dart';
import 'package:jikido/src/sitting_controller.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'fakes.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  late FakeBellAudio audio;
  late FakeClosingBellNotification notification;
  late FakeSittingService service;
  late FakeScreenAwake screen;
  late TestClock clock;

  /// A controller with no settling time, which is what most of these tests
  /// are about. The default is a minute — see [Settings.defaultPrepare] — and
  /// the tests that care about it ask for it explicitly.
  SittingController makeController({Duration prepare = Duration.zero}) {
    final controller = SittingController(
      audio: audio,
      notification: notification,
      service: service,
      screen: screen,
      clock: clock.call,
    );
    // Applies to _settings before it awaits anything, so the value is in
    // place by the time this returns.
    controller.setPrepare(prepare);
    return controller;
  }

  setUp(() {
    SharedPreferences.setMockInitialValues(<String, Object>{});
    audio = FakeBellAudio();
    notification = FakeClosingBellNotification();
    service = FakeSittingService();
    screen = FakeScreenAwake();
    clock = TestClock(DateTime.utc(2026, 3, 1, 7, 0, 0));
  });

  test('starting a sitting rings the bell and arms every backstop', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 15));
      controller.start();
      async.flushMicrotasks();

      expect(controller.status, SittingStatus.running);
      expect(audio.strikes, <Bell>[Bell.inkin], reason: 'the opening bell');
      expect(audio.keepAliveRunning, isTrue,
          reason: 'the audio session must be held open for the whole sitting');
      expect(service.running, isTrue);
      expect(
        notification.scheduledFor,
        DateTime.utc(2026, 3, 1, 7, 15, 0)
            .add(ClosingBellNotification.backstopDelay),
        reason: 'the backstop is set a little past the app\'s own bell, so '
            'that only one of the two can ring',
      );

      controller.dispose();
    });
  });

  test('the closing bell is struck when the sitting time is up', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 5));
      controller.start();
      async.flushMicrotasks();

      // Ticks are driven by a periodic timer; advance the fake clock in step
      // with fake time so the two agree.
      for (var i = 0; i < 5 * 60 * 5; i++) {
        clock.advance(const Duration(milliseconds: 200));
        async.elapse(const Duration(milliseconds: 200));
      }

      expect(audio.strikes, <Bell>[Bell.inkin, Bell.inkin]);
      expect(notification.isScheduled, isFalse,
          reason: 'the backstop is cancelled once the app has rung the bell');
      expect(controller.status, SittingStatus.running,
          reason: 'the sitting is not over until the bell has rung out');

      controller.dispose();
    });
  });

  test('the sitting completes once the closing bell has rung out', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 1));
      controller.start();
      async.flushMicrotasks();

      for (var i = 0; i < (60 + 13) * 5; i++) {
        clock.advance(const Duration(milliseconds: 200));
        async.elapse(const Duration(milliseconds: 200));
      }

      expect(controller.status, SittingStatus.complete);
      expect(audio.keepAliveRunning, isFalse);
      expect(service.running, isFalse);
      expect(controller.notice, isNull);

      controller.dispose();
    });
  });

  test('a bell missed while the app was gone is reported, not rung late', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 20));
      controller.start();
      async.flushMicrotasks();
      expect(audio.strikes.length, 1);

      // The process was killed and the user reopened Jikido half an hour
      // later. Fake time does not move, so no tick ran in between — which is
      // exactly the situation being reproduced.
      clock.advance(const Duration(minutes: 50));
      controller.onResumed();
      async.flushMicrotasks();

      expect(audio.strikes.length, 1, reason: 'no bell three decades late');
      expect(controller.status, SittingStatus.complete);
      expect(controller.notice, isNotNull);

      controller.dispose();
    });
  });

  test('a bell a few seconds late is still worth striking', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 20));
      controller.start();
      async.flushMicrotasks();

      clock.advance(const Duration(minutes: 20, seconds: 2));
      controller.onResumed();
      async.flushMicrotasks();

      expect(audio.strikes, <Bell>[Bell.inkin, Bell.inkin]);
      expect(controller.notice, isNull);

      controller.dispose();
    });
  });

  test('ending early rings nothing and clears the scheduled bell', () {
    fakeAsync((async) {
      final controller = makeController();
      controller.start();
      async.flushMicrotasks();

      clock.advance(const Duration(minutes: 3));
      controller.cancel();
      async.flushMicrotasks();

      expect(audio.strikes.length, 1, reason: 'only the opening bell');
      expect(notification.isScheduled, isFalse);
      expect(audio.keepAliveRunning, isFalse);
      expect(service.running, isFalse);
      expect(controller.status, SittingStatus.idle);

      controller.dispose();
    });
  });

  test('the service notification counts down in whole minutes', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 3));
      controller.start();
      async.flushMicrotasks();

      for (var i = 0; i < 90 * 5; i++) {
        clock.advance(const Duration(milliseconds: 200));
        async.elapse(const Duration(milliseconds: 200));
      }

      expect(service.texts, <String>[
        '3 minutes left',
        '2 minutes left',
      ]);

      controller.cancel();
      async.flushMicrotasks();
      controller.dispose();
    });
  });

  test('the chosen bell is the one that rings', () {
    fakeAsync((async) {
      final controller = makeController()..setBell(Bell.keisu);
      async.flushMicrotasks();
      controller.start();
      async.flushMicrotasks();

      expect(audio.strikes, <Bell>[Bell.keisu]);
      expect(notification.scheduledBell, Bell.keisu);

      controller.cancel();
      async.flushMicrotasks();
      controller.dispose();
    });
  });

  test('permissions are asked for once, not before every sitting', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(Settings.minimumDuration);
      controller.start();
      async.flushMicrotasks();
      expect(notification.permissionsRequested, isTrue);
      expect(service.permissionsRequested, isTrue);

      notification.permissionsRequested = false;
      service.permissionsRequested = false;

      controller.cancel();
      async.flushMicrotasks();
      controller.start();
      async.flushMicrotasks();

      expect(notification.permissionsRequested, isFalse);
      expect(service.permissionsRequested, isFalse);

      controller.cancel();
      async.flushMicrotasks();
      controller.dispose();
    });
  });

  test('a layer that fails is reported, and does not stop the clock', () {
    fakeAsync((async) {
      // Android 14 refuses to start a foreground service while the app is
      // not in the foreground, which is exactly where the first-run
      // notification dialog puts it. That used to abandon the rest of the
      // setup, ticker included, and the countdown sat frozen on the second
      // the sitting began.
      service.failToStart = true;

      final errors = <FlutterErrorDetails>[];
      final previousOnError = FlutterError.onError;
      FlutterError.onError = errors.add;
      addTearDown(() => FlutterError.onError = previousOnError);

      final controller = makeController()
        ..setDuration(const Duration(minutes: 15));
      controller.start();
      async.flushMicrotasks();

      expect(errors, hasLength(1),
          reason: 'the failure reaches the log rather than being swallowed');
      expect(service.running, isFalse);
      expect(notification.isScheduled, isTrue,
          reason: 'the layers are independent: the service failing must not '
              'cost the sitting its backstop bell');

      // What the user sees is a redraw per tick, so that is what is counted
      // here. `remaining` reads the clock directly and would look right even
      // with no ticker at all.
      var redraws = 0;
      controller.addListener(() => redraws++);
      for (var i = 0; i < 15; i++) {
        clock.advance(const Duration(milliseconds: 200));
        async.elapse(const Duration(milliseconds: 200));
      }

      expect(redraws, 15);
      expect(controller.remaining, const Duration(minutes: 14, seconds: 57));

      controller.cancel();
      async.flushMicrotasks();
      controller.dispose();
    });
  });

  test('a layer that hangs does not hold up the clock', () {
    fakeAsync((async) {
      final blocked = Completer<void>();
      service.startBlocker = blocked;

      final controller = makeController()
        ..setDuration(const Duration(minutes: 15));
      controller.start();
      async.flushMicrotasks();

      var redraws = 0;
      controller.addListener(() => redraws++);
      for (var i = 0; i < 15; i++) {
        clock.advance(const Duration(milliseconds: 200));
        async.elapse(const Duration(milliseconds: 200));
      }

      expect(redraws, 15,
          reason: 'the ticker is up before the platform calls, so one of '
              'them taking its time cannot freeze the countdown');
      expect(controller.remaining, const Duration(minutes: 14, seconds: 57));
      expect(notification.isScheduled, isFalse,
          reason: 'the layers after the blocked one are genuinely still '
              'waiting — this is what the countdown is being kept clear of');

      blocked.complete();
      async.flushMicrotasks();
      expect(service.running, isTrue);
      expect(notification.isScheduled, isTrue);

      controller.cancel();
      async.flushMicrotasks();
      controller.dispose();
    });
  });

  group('the settling time', () {
    test('holds the opening bell until it has run out', () {
      fakeAsync((async) {
        final controller = makeController(prepare: const Duration(minutes: 1))
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        expect(controller.status, SittingStatus.running);
        expect(audio.strikes, isEmpty,
            reason: 'pressing Sit starts the settling, not the sitting');
        expect(audio.keepAliveRunning, isTrue,
            reason: 'the audio session is held from the moment Sit is '
                'pressed — the settling time counts too');

        // Just short of a minute: still nothing.
        for (var i = 0; i < 59 * 5; i++) {
          clock.advance(const Duration(milliseconds: 200));
          async.elapse(const Duration(milliseconds: 200));
        }
        expect(audio.strikes, isEmpty);

        for (var i = 0; i < 5; i++) {
          clock.advance(const Duration(milliseconds: 200));
          async.elapse(const Duration(milliseconds: 200));
        }
        expect(audio.sequences, <BellSequence>[BellSequence.opening],
            reason: 'the settling is over, so the period opens');

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });

    test('does not come out of the sitting', () {
      fakeAsync((async) {
        final controller = makeController(prepare: const Duration(minutes: 1))
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        expect(
          notification.scheduledFor,
          DateTime.utc(2026, 3, 1, 7, 16, 0)
              .add(ClosingBellNotification.backstopDelay),
          reason: 'fifteen minutes of sitting still means fifteen minutes, '
              'starting when the bell rings rather than when Sit was pressed',
        );

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });

    test('an opening bell missed while the app was gone is not rung late', () {
      fakeAsync((async) {
        final controller = makeController(prepare: const Duration(minutes: 1))
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        // The app was suspended and comes back well past the opening bell.
        clock.advance(const Duration(minutes: 2));
        controller.onResumed();
        async.flushMicrotasks();

        expect(audio.strikes, isEmpty,
            reason: 'the period has been under way for a minute; opening it '
                'now would be a lie about where things are');
        expect(controller.status, SittingStatus.running,
            reason: 'the sitting itself carries on regardless — it is timed '
                'from the instant, not from the bell being heard');

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });
  });

  group('pausing', () {
    test('holds the countdown, and gives the time back on resume', () {
      fakeAsync((async) {
        final controller = makeController()
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        clock.advance(const Duration(minutes: 4));
        async.elapse(const Duration(minutes: 4));
        expect(controller.remaining, const Duration(minutes: 11));

        controller.pause();
        async.flushMicrotasks();
        expect(controller.isPaused, isTrue);
        expect(controller.status, SittingStatus.running,
            reason: 'a paused sitting is still a sitting');

        clock.advance(const Duration(minutes: 7));
        async.elapse(const Duration(minutes: 7));
        expect(controller.remaining, const Duration(minutes: 11),
            reason: 'the sitting is held, not quietly running underneath');
        expect(audio.strikes.length, 1, reason: 'only the opening bell');

        controller.resume();
        async.flushMicrotasks();
        expect(controller.isPaused, isFalse);
        expect(controller.remaining, const Duration(minutes: 11));

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });

    test('leaves the layers that keep the sitting alive up', () {
      fakeAsync((async) {
        final controller = makeController();
        controller.start();
        async.flushMicrotasks();

        controller.pause();
        async.flushMicrotasks();

        expect(audio.keepAliveRunning, isTrue,
            reason: 'a process reclaimed during a pause is a sitting lost');
        expect(service.running, isTrue);
        expect(service.text, 'Paused');
        expect(notification.isScheduled, isFalse,
            reason: 'the backstop is set for an instant that is no longer '
                'the end of anything');

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });

    test('re-arms the backstop for the closing bell\'s new time', () {
      fakeAsync((async) {
        final controller = makeController()
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        clock.advance(const Duration(minutes: 4));
        async.elapse(const Duration(minutes: 4));
        controller.pause();
        async.flushMicrotasks();

        clock.advance(const Duration(minutes: 7));
        async.elapse(const Duration(minutes: 7));
        controller.resume();
        async.flushMicrotasks();

        expect(
          notification.scheduledFor,
          DateTime.utc(2026, 3, 1, 7, 22, 0)
              .add(ClosingBellNotification.backstopDelay),
          reason: 'the pause moved the closing bell seven minutes later',
        );
        expect(service.text, '11 minutes left',
            reason: 'the notification says where the sitting actually is');

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });

    test('the closing bell waits for the pause to end', () {
      fakeAsync((async) {
        final controller = makeController()
          ..setDuration(const Duration(minutes: 5));
        controller.start();
        async.flushMicrotasks();

        clock.advance(const Duration(minutes: 1));
        async.elapse(const Duration(minutes: 1));
        controller.pause();
        async.flushMicrotasks();

        // Well past when the bell would have been due, and past the point
        // where the app would give up and leave it to the backstop.
        clock.advance(const Duration(minutes: 30));
        async.elapse(const Duration(minutes: 30));
        expect(audio.strikes.length, 1, reason: 'no bell into a held sitting');
        expect(controller.status, SittingStatus.running);
        expect(controller.notice, isNull,
            reason: 'nothing was missed — the sitting was paused');

        controller.resume();
        async.flushMicrotasks();

        // The four minutes that were left when the pause began.
        for (var i = 0; i < 4 * 60 * 5; i++) {
          clock.advance(const Duration(milliseconds: 200));
          async.elapse(const Duration(milliseconds: 200));
        }
        expect(audio.sequences,
            <BellSequence>[BellSequence.opening, BellSequence.closing]);

        controller.dispose();
      });
    });

    test('holds the settling time, and opens the period on the way out', () {
      fakeAsync((async) {
        final controller = makeController(prepare: const Duration(minutes: 1))
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        clock.advance(const Duration(seconds: 20));
        async.elapse(const Duration(seconds: 20));
        controller.pause();
        async.flushMicrotasks();
        expect(controller.prepareRemaining, const Duration(seconds: 40));

        clock.advance(const Duration(minutes: 5));
        async.elapse(const Duration(minutes: 5));
        expect(audio.strikes, isEmpty,
            reason: 'the settling time is held along with everything else');
        expect(controller.prepareRemaining, const Duration(seconds: 40));

        controller.resume();
        async.flushMicrotasks();
        expect(audio.strikes, isEmpty,
            reason: 'forty seconds of settling still to go, and no stale '
                'bell for the five minutes the app was holding');

        for (var i = 0; i < 40 * 5; i++) {
          clock.advance(const Duration(milliseconds: 200));
          async.elapse(const Duration(milliseconds: 200));
        }
        expect(audio.sequences, <BellSequence>[BellSequence.opening]);
        expect(controller.remaining, const Duration(minutes: 15));

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });

    test('is not offered once the closing bell is due', () {
      fakeAsync((async) {
        final controller = makeController()
          ..setDuration(const Duration(minutes: 1));
        controller.start();
        async.flushMicrotasks();
        expect(controller.canPause, isTrue);

        for (var i = 0; i < 60 * 5; i++) {
          clock.advance(const Duration(milliseconds: 200));
          async.elapse(const Duration(milliseconds: 200));
        }

        expect(controller.canPause, isFalse,
            reason: 'the sitting is over bar the ring; there is nothing '
                'left to hold');
        controller.pause();
        async.flushMicrotasks();
        expect(controller.isPaused, isFalse);

        controller.dispose();
      });
    });

    test('is not offered when no sitting is running', () {
      fakeAsync((async) {
        final controller = makeController();
        expect(controller.canPause, isFalse);
        controller.pause();
        controller.resume();
        async.flushMicrotasks();
        expect(controller.isPaused, isFalse);
        expect(controller.status, SittingStatus.idle);

        controller.dispose();
      });
    });
  });

  group('the free-play bell', () {
    test('strikes overlap rather than cutting each other off', () {
      fakeAsync((async) {
        final controller = makeController();
        controller.strikeBell();
        controller.strikeBell();
        controller.strikeBell();
        async.flushMicrotasks();

        expect(audio.taps, 3);
        expect(audio.strikes, isEmpty,
            reason: 'the free-play bell is a single strike, not a sequence');

        controller.dampBell();
        async.flushMicrotasks();
        expect(audio.damps, 1);

        controller.dispose();
      });
    });

    test('is unavailable mid-sitting', () {
      fakeAsync((async) {
        final controller = makeController()
          ..setDuration(const Duration(minutes: 15));
        controller.start();
        async.flushMicrotasks();

        controller.strikeBell();
        controller.dampBell();
        async.flushMicrotasks();

        expect(audio.taps, 0);
        expect(audio.damps, 0,
            reason: 'a bell to play with during zazen is a bell to get '
                'distracted by, and damping would silence the real one');

        controller.cancel();
        async.flushMicrotasks();
        controller.dispose();
      });
    });
  });

  test('the closing bell is the damped two-strike sequence', () {
    fakeAsync((async) {
      final controller = makeController()
        ..setDuration(const Duration(minutes: 5));
      controller.start();
      async.flushMicrotasks();

      for (var i = 0; i < 5 * 60 * 5; i++) {
        clock.advance(const Duration(milliseconds: 200));
        async.elapse(const Duration(milliseconds: 200));
      }

      expect(audio.sequences,
          <BellSequence>[BellSequence.opening, BellSequence.closing]);

      controller.dispose();
    });
  });
}
