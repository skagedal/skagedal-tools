import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/alarm/closing_bell_notification.dart';
import 'package:jikido/src/bell.dart';
import 'package:jikido/src/session.dart';

void main() {
  final start = DateTime.utc(2026, 3, 1, 7, 0, 0);

  MeditationSession sessionOf({
    Duration duration = const Duration(minutes: 15),
    Bell bell = Bell.inkin,
  }) =>
      MeditationSession(startedAt: start, duration: duration, bell: bell);

  group('instants', () {
    test('the closing bell is due when the configured time has passed', () {
      final session = sessionOf();
      expect(session.closingBellAt, start.add(const Duration(minutes: 15)));
    });

    test('the sitting is not over until the closing bell has rung out', () {
      final session = sessionOf();
      expect(
        session.endsAt,
        start.add(const Duration(minutes: 15) + Bell.inkin.duration),
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
        session.phaseAt(start.add(const Duration(minutes: 15, seconds: 11))),
        SessionPhase.closing,
      );
      expect(
        session.phaseAt(start.add(const Duration(minutes: 15, seconds: 12))),
        SessionPhase.complete,
      );
    });

    test('the opening phase lasts as long as the chosen bell rings', () {
      final keisu = sessionOf(bell: Bell.keisu);
      expect(
        keisu.phaseAt(start.add(const Duration(seconds: 20))),
        SessionPhase.opening,
      );
      expect(
        keisu.phaseAt(start.add(const Duration(seconds: 26))),
        SessionPhase.sitting,
      );
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
