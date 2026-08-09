import 'bell.dart';

/// Where a sitting has got to.
enum SessionPhase {
  /// The opening bell is still ringing. Sitting has begun — the countdown
  /// starts with the first strike, as it does in a zendo, where the bell
  /// rings out over the beginning of the period rather than before it.
  opening,

  /// The silent part of the sitting.
  sitting,

  /// The closing bell is ringing.
  closing,

  /// The closing bell has finished.
  complete,
}

/// A sitting, described entirely in terms of wall-clock instants.
///
/// Everything here is derived from [startedAt] rather than accumulated by a
/// ticker, which is the whole point: a phone suspends timers when it feels
/// like it, but it does not forget what time it is. Whenever the app gets
/// control back it can ask this object what should have happened by now.
class MeditationSession {
  const MeditationSession({
    required this.startedAt,
    required this.duration,
    required this.bell,
  });

  /// When the opening bell was struck.
  final DateTime startedAt;

  /// The configured length of the sitting, measured from [startedAt] to the
  /// first strike of the closing bell.
  final Duration duration;

  final Bell bell;

  /// When the closing bell should be struck.
  DateTime get closingBellAt => startedAt.add(duration);

  /// When the closing bell has finished ringing and the sitting is over.
  DateTime get endsAt => closingBellAt.add(bell.duration);

  /// How long until the closing bell, never negative.
  Duration remainingAt(DateTime now) {
    final remaining = closingBellAt.difference(now);
    return remaining.isNegative ? Duration.zero : remaining;
  }

  /// Progress through the sitting, from 0.0 at the opening bell to 1.0 at the
  /// closing one.
  double progressAt(DateTime now) {
    if (duration <= Duration.zero) {
      return 1;
    }
    final elapsed = now.difference(startedAt).inMicroseconds;
    return (elapsed / duration.inMicroseconds).clamp(0.0, 1.0);
  }

  SessionPhase phaseAt(DateTime now) {
    if (!now.isBefore(endsAt)) {
      return SessionPhase.complete;
    }
    if (!now.isBefore(closingBellAt)) {
      return SessionPhase.closing;
    }
    if (now.isBefore(startedAt.add(bell.duration))) {
      return SessionPhase.opening;
    }
    return SessionPhase.sitting;
  }

  /// Whether the closing bell is due at [now] but has not been struck yet.
  bool closingBellIsDueAt(DateTime now) => !now.isBefore(closingBellAt);

  /// Whether striking the closing bell at [now] would be too late to be the
  /// app's job any more.
  ///
  /// Past this point the scheduled backstop notification is assumed to have
  /// taken over — it fires shortly after [staleAfter] — and ringing as well
  /// would mean two bells for one sitting. Which of the two rings is decided
  /// by this one comparison, so there is no window in which both do.
  bool closingBellIsStaleAt(DateTime now) =>
      now.difference(closingBellAt) > staleAfter;

  /// How late the app may be and still ring the bell itself.
  ///
  /// While the app is running it ticks several times a second, so it is late
  /// by milliseconds. Being seconds late means it was suspended, and being
  /// suspended is what the backstop is for.
  static const Duration staleAfter = Duration(seconds: 3);
}
