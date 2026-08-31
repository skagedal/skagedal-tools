import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/alarm/closing_bell_notification.dart';
import 'package:jikido/src/bell.dart';
import 'package:jikido/src/session.dart';

void main() {
  final start = DateTime.utc(2026, 3, 1, 7, 0, 0);

  MeditationSession sessionOf({
    Duration duration = const Duration(minutes: 15),
    Bell bell = Bell.inkin,
    Duration prepare = Duration.zero,
  }) =>
      MeditationSession(
        beganAt: start,
        duration: duration,
        bell: bell,
        prepare: prepare,
      );

  group('instants', () {
    test('the closing bell is due when the configured time has passed', () {
      final session = sessionOf();
      expect(session.closingBellAt, start.add(const Duration(minutes: 15)));
    });

    test('the sitting is not over until the closing bell has rung out', () {
      final session = sessionOf();
      expect(
        session.endsAt,
        start.add(const Duration(minutes: 15) + Bell.inkin.closingRingAt(1)),
      );
    });

    test('the closing rings for a fraction of what the opening does', () {
      // The closing ends with the striker laid on the bowl, so there is no
      // tail to wait out. It is the reason a sitting now ends three seconds
      // after the bell rather than twelve.
      final session = sessionOf();
      expect(session.closingRing, lessThan(session.openingRing ~/ 3));
      expect(session.closingRing, lessThan(const Duration(seconds: 3)));
    });
  });

  group('the settling time', () {
    test('delays the opening bell without shortening the sitting', () {
      final session = sessionOf(prepare: const Duration(minutes: 1));
      expect(session.startedAt, start.add(const Duration(minutes: 1)));
      expect(
        session.closingBellAt,
        start.add(const Duration(minutes: 16)),
        reason: 'fifteen minutes still means fifteen minutes of sitting',
      );
    });

    test('counts down, and stops at zero', () {
      final session = sessionOf(prepare: const Duration(minutes: 1));
      expect(session.prepareRemainingAt(start), const Duration(minutes: 1));
      expect(
        session.prepareRemainingAt(start.add(const Duration(seconds: 40))),
        const Duration(seconds: 20),
      );
      expect(
        session.prepareRemainingAt(start.add(const Duration(minutes: 5))),
        Duration.zero,
      );
    });

    test('leaves the ring empty until the sitting actually starts', () {
      // A ring that crept round during the settling time would say the
      // sitting had begun when it had not.
      final session = sessionOf(prepare: const Duration(minutes: 1));
      expect(session.progressAt(start), 0);
      expect(session.progressAt(start.add(const Duration(seconds: 59))), 0);
      expect(
        session.progressAt(start.add(const Duration(minutes: 1, seconds: 30))),
        greaterThan(0),
      );
    });

    test('is a phase of its own', () {
      final session = sessionOf(prepare: const Duration(minutes: 1));
      expect(session.phaseAt(start), SessionPhase.preparing);
      expect(
        session.phaseAt(start.add(const Duration(seconds: 59))),
        SessionPhase.preparing,
      );
      expect(
        session.phaseAt(start.add(const Duration(minutes: 1))),
        SessionPhase.opening,
      );
    });

    test('is skipped entirely when it is zero', () {
      final session = sessionOf();
      expect(session.startedAt, start);
      expect(session.phaseAt(start), SessionPhase.opening);
    });
  });

  group('the opening bell', () {
    test('is due the moment the settling time is up', () {
      final session = sessionOf(prepare: const Duration(minutes: 1));
      expect(
        session.openingBellIsDueAt(start.add(const Duration(seconds: 59))),
        isFalse,
      );
      expect(
        session.openingBellIsDueAt(start.add(const Duration(minutes: 1))),
        isTrue,
      );
    });

    test('is not rung at all once the app has missed it by seconds', () {
      // The sitting is timed from the instant whether or not anyone heard
      // the bell, so an app suspended through the settling time comes back to
      // a period already under way. Ringing it open now would be a lie.
      final session = sessionOf(prepare: const Duration(minutes: 1));
      expect(
        session.openingBellIsStaleAt(
            start.add(const Duration(minutes: 1, seconds: 2))),
        isFalse,
      );
      expect(
        session.openingBellIsStaleAt(
            start.add(const Duration(minutes: 1, seconds: 4))),
        isTrue,
      );
    });
  });

  group('remainingAt', () {
    test('counts down from the full duration', () {
      final session = sessionOf();
      expect(session.remainingAt(start), const Duration(minutes: 15));
      expect(
        session.remainingAt(start.add(const Duration(minutes: 4, seconds: 30))),
        const Duration(minutes: 10, seconds: 30),
      );
    });

    test('never goes negative, however late the caller is', () {
      final session = sessionOf();
      expect(
        session.remainingAt(start.add(const Duration(hours: 3))),
        Duration.zero,
      );
    });
  });

  group('progressAt', () {
    test('runs from zero to one across the sitting', () {
      final session = sessionOf();
      expect(session.progressAt(start), 0);
      expect(
        session.progressAt(start.add(const Duration(minutes: 5))),
        closeTo(1 / 3, 1e-9),
      );
      expect(session.progressAt(start.add(const Duration(minutes: 15))), 1);
    });

    test('is clamped rather than extrapolated', () {
      final session = sessionOf();
      expect(session.progressAt(start.subtract(const Duration(minutes: 1))), 0);
      expect(session.progressAt(start.add(const Duration(hours: 1))), 1);
    });
  });

  group('phaseAt', () {
    test('walks through the phases of a sitting', () {
      final session = sessionOf();
      expect(session.phaseAt(start), SessionPhase.opening);
      expect(
        session.phaseAt(start.add(const Duration(seconds: 5))),
        SessionPhase.opening,
      );
      expect(
        session.phaseAt(start.add(const Duration(seconds: 30))),
        SessionPhase.sitting,
      );
      expect(
        session.phaseAt(start.add(const Duration(minutes: 15))),
        SessionPhase.closing,
      );
      expect(
        session.phaseAt(start.add(const Duration(minutes: 15, seconds: 2))),
        SessionPhase.closing,
      );
      expect(
        session.phaseAt(start.add(const Duration(minutes: 15, seconds: 3))),
        SessionPhase.complete,
      );
    });

    test('the opening phase lasts as long as the chosen bell rings', () {
      final keisu = sessionOf(bell: Bell.keisu);
      expect(
        keisu.phaseAt(start.add(const Duration(seconds: 40))),
        SessionPhase.opening,
      );
      expect(
        keisu.phaseAt(start.add(const Duration(seconds: 43))),
        SessionPhase.sitting,
      );
    });

    test('a bigger bell rings for longer', () {
      final small = MeditationSession(
          beganAt: start, duration: const Duration(minutes: 15),
          bell: Bell.inkin, bellSize: 0.6);
      final large = MeditationSession(
          beganAt: start, duration: const Duration(minutes: 15),
          bell: Bell.inkin, bellSize: 2.0);
      expect(large.openingRing, greaterThan(small.openingRing));
    });

    test('a sitting shorter than the bell still reaches every phase', () {
      // A one-minute sitting with the keisu: the opening bell has not
      // finished ringing when the closing one is due. Closing must win.
      final session = sessionOf(
        duration: const Duration(minutes: 1),
        bell: Bell.keisu,
      );
      expect(session.phaseAt(start), SessionPhase.opening);
      expect(
        session.phaseAt(start.add(const Duration(seconds: 61))),
        SessionPhase.closing,
      );
    });
  });

  group('the closing bell', () {
    test('is due from the moment the sitting time is up', () {
      final session = sessionOf();
      expect(
        session.closingBellIsDueAt(
            start.add(const Duration(minutes: 14, seconds: 59))),
        isFalse,
      );
      expect(
        session.closingBellIsDueAt(start.add(const Duration(minutes: 15))),
        isTrue,
      );
    });

    test('is worth striking a moment late', () {
      final session = sessionOf();
      expect(
        session.closingBellIsStaleAt(
            start.add(const Duration(minutes: 15, seconds: 2))),
        isFalse,
      );
    });

    test('is left to the backstop once the app is seconds behind', () {
      // The case this guards: the app was killed, the scheduled notification
      // rang, and the user only reopens Jikido later. Exactly one bell.
      final session = sessionOf();
      expect(
        session.closingBellIsStaleAt(
            start.add(const Duration(minutes: 15, seconds: 4))),
        isTrue,
      );
      expect(
        session.closingBellIsStaleAt(start.add(const Duration(minutes: 20))),
        isTrue,
      );
    });

    test('hands over to the backstop before the backstop fires', () {
      // The invariant the whole two-bells-or-none argument rests on. If
      // these ever cross, a sitting could end with both bells or neither.
      expect(
        ClosingBellNotification.backstopDelay,
        greaterThan(MeditationSession.staleAfter),
      );
    });
  });
}
