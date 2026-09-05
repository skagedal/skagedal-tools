import 'bell.dart';

/// Where a sitting has got to.
enum SessionPhase {
  /// Settling onto the cushion. The opening bell has not rung yet.
  preparing,

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
/// Everything here is derived from [beganAt] rather than accumulated by a
/// ticker, which is the whole point: a phone suspends timers when it feels
/// like it, but it does not forget what time it is. Whenever the app gets
/// control back it can ask this object what should have happened by now.
class MeditationSession {
  const MeditationSession({
    required this.beganAt,
    required this.duration,
    required this.bell,
    this.bellSize = 1.0,
    this.prepare = Duration.zero,
  });

  /// When the user pressed Sit.
  final DateTime beganAt;

  /// Silence between pressing Sit and the opening bell, for settling.
  final Duration prepare;

  /// The configured length of the sitting, measured from [startedAt] to the
  /// first strike of the closing bell.
  final Duration duration;

  final Bell bell;
  final double bellSize;

  /// The same sitting, every instant of it moved later by [by].
  ///
  /// This is what pausing does. A paused sitting is not a sitting with a
  /// counter that has stopped being decremented — there is no counter — it is
  /// the same sitting, postponed. Everything here hangs off [beganAt], so
  /// moving that moves the opening bell, the closing bell and the end
  /// together, and whatever was left when the user paused is exactly what is
  /// left when they carry on.
  MeditationSession delayedBy(Duration by) => MeditationSession(
        beganAt: beganAt.add(by),
        duration: duration,
        bell: bell,
        bellSize: bellSize,
        prepare: prepare,
      );

  /// When the opening bell is struck, and the sitting proper begins.
  DateTime get startedAt => beganAt.add(prepare);

  /// When the closing bell should be struck.
  DateTime get closingBellAt => startedAt.add(duration);

  /// How long the closing bell rings for. Short, because it ends under the
  /// striker rather than fading out.
  Duration get closingRing => bell.closingRingAt(bellSize);

  /// How long the opening bell rings for.
  Duration get openingRing => bell.openingRingAt(bellSize);

  /// When the closing bell has finished ringing and the sitting is over.
  DateTime get endsAt => closingBellAt.add(closingRing);

  /// How long until the opening bell, never negative. Zero once it is due.
  Duration prepareRemainingAt(DateTime now) {
    final remaining = startedAt.difference(now);
    return remaining.isNegative ? Duration.zero : remaining;
  }

  /// How long until the closing bell, never negative.
  Duration remainingAt(DateTime now) {
    final remaining = closingBellAt.difference(now);
    return remaining.isNegative ? Duration.zero : remaining;
  }

  /// Progress through the sitting, from 0.0 at the opening bell to 1.0 at the
  /// closing one. Flat at zero while preparing: the sitting has not started,
  /// and a ring that crept round during the settling time would say it had.
  double progressAt(DateTime now) {
    if (duration <= Duration.zero) {
      return 1;
    }
    final elapsed = now.difference(startedAt).inMicroseconds;
    if (elapsed <= 0) {
      return 0;
    }
    return (elapsed / duration.inMicroseconds).clamp(0.0, 1.0);
  }

  /// Progress through the settling time, 0.0 to 1.0. Zero when there is none.
  double prepareProgressAt(DateTime now) {
    if (prepare <= Duration.zero) {
      return 1;
    }
    final elapsed = now.difference(beganAt).inMicroseconds;
    return (elapsed / prepare.inMicroseconds).clamp(0.0, 1.0);
  }

  SessionPhase phaseAt(DateTime now) {
    if (!now.isBefore(endsAt)) {
      return SessionPhase.complete;
    }
    if (!now.isBefore(closingBellAt)) {
      return SessionPhase.closing;
    }
    if (now.isBefore(startedAt)) {
      return SessionPhase.preparing;
    }
    if (now.isBefore(startedAt.add(openingRing))) {
      return SessionPhase.opening;
    }
    return SessionPhase.sitting;
  }

  /// Whether the opening bell is due at [now] but has not been struck yet.
  bool openingBellIsDueAt(DateTime now) => !now.isBefore(startedAt);

  /// Whether striking the opening bell at [now] would be too late to bother.
  ///
  /// The sitting is timed from [startedAt] whether or not anyone was there to
  /// hear the bell, so an app that was suspended through the settling time
  /// comes back to a sitting already under way. Ringing the opening bell
  /// minutes into it would be a lie about where things are.
  bool openingBellIsStaleAt(DateTime now) =>
      now.difference(startedAt) > staleAfter;

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

  /// How late the app may be and still ring a bell itself.
  ///
  /// While the app is running it ticks several times a second, so it is late
  /// by milliseconds. Being seconds late means it was suspended, and being
  /// suspended is what the backstop is for.
  static const Duration staleAfter = Duration(seconds: 3);
}
