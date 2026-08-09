import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/alarm/closing_bell_notification.dart';
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

  SittingController makeController() => SittingController(
        audio: audio,
        notification: notification,
        service: service,
        screen: screen,
        clock: clock.call,
      );

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
}
