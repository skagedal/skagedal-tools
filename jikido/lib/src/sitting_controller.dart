import 'dart:async';

import 'package:flutter/foundation.dart';

import 'alarm/closing_bell_notification.dart';
import 'alarm/sitting_service.dart';
import 'audio/bell_audio.dart';
import 'bell.dart';
import 'screen_awake.dart';
import 'session.dart';
import 'settings.dart';

enum SittingStatus {
  /// No sitting in progress; the user is choosing a length.
  idle,

  /// A sitting is under way.
  running,

  /// The closing bell has finished, and the completion screen is showing.
  complete,
}

/// Drives a sitting from the opening bell to the closing one.
///
/// The design rule here is that nothing about *when* things happen is
/// accumulated. [MeditationSession] holds the wall-clock instants, the ticker
/// only asks "what should have happened by now?", and every layer that could
/// keep the app alive is engaged in parallel rather than relied on
/// individually.
class SittingController extends ChangeNotifier {
  SittingController({
    BellAudio? audio,
    ClosingBellNotification? notification,
    SittingService? service,
    ScreenAwake? screen,
    DateTime Function()? clock,
  })  : _audio = audio ?? BellAudio(),
        _notification = notification ?? ClosingBellNotification(),
        _service = service ?? const SittingService(),
        _screen = screen ?? const ScreenAwake(),
        _now = clock ?? DateTime.now;

  final BellAudio _audio;
  final ClosingBellNotification _notification;
  final SittingService _service;
  final ScreenAwake _screen;

  /// Where the time comes from. Injectable so that the one piece of logic
  /// that must never be wrong — striking the closing bell when it is due —
  /// can be tested without waiting twenty minutes for it.
  final DateTime Function() _now;

  /// How often to re-check the clock. Fast enough that the bell is never
  /// audibly late, cheap enough to leave running for an hour.
  static const Duration _tickInterval = Duration(milliseconds: 200);

  Settings _settings = const Settings();
  SittingStatus _status = SittingStatus.idle;
  MeditationSession? _session;
  DateTime? _pausedAt;
  Timer? _ticker;
  bool _openingBellStruck = false;
  bool _closingBellStruck = false;
  bool _permissionsRequested = false;
  String? _notice;
  int _notifiedMinutesLeft = _unwritten;

  Settings get settings => _settings;
  SittingStatus get status => _status;
  MeditationSession? get session => _session;

  /// A message about something that did not go to plan — currently only the
  /// case where the app was killed and came back after the sitting ended.
  String? get notice => _notice;

  /// Whether the sitting is held. A paused sitting is still a sitting: the
  /// audio session and the foreground service stay up, because letting the
  /// process be reclaimed while the user is answering the door would lose
  /// the sitting they meant to come back to.
  bool get isPaused => _pausedAt != null;

  /// Whether there is anything left to hold. Available from pressing Sit
  /// until the closing bell is due; past that the sitting is over bar the
  /// ring, and pausing it would only mean a bell left hanging.
  bool get canPause =>
      _status == SittingStatus.running &&
      !isPaused &&
      !(_session?.closingBellIsDueAt(_now()) ?? true);

  /// The instant everything shown is worked out from. While paused it is the
  /// instant the user paused at, so the countdown, the ensō and the phase all
  /// hold still rather than the sitting running on quietly underneath.
  DateTime _shownNow() => _pausedAt ?? _now();

  Duration get remaining =>
      _session?.remainingAt(_shownNow()) ?? _settings.duration;

  double get progress => _session?.progressAt(_shownNow()) ?? 0;

  /// How long until the opening bell. Only meaningful while settling.
  Duration get prepareRemaining =>
      _session?.prepareRemainingAt(_shownNow()) ?? _settings.prepare;

  /// Whether the sitting has not started yet because the settling time is
  /// still running.
  bool get isPreparing => phase == SessionPhase.preparing;

  SessionPhase? get phase => _session?.phaseAt(_shownNow());

  /// Set when something the sitting depends on could not be set up. The user
  /// is told: a meditation timer that has quietly lost the ability to make a
  /// sound is worse than one that says so.
  String? get initializationError => _initializationError;
  String? _initializationError;

  Future<void> initialize() async {
    try {
      _settings = await Settings.load();
      _service.configure();
      await _notification.initialize();
      await _audio.initialize(_settings.bell, _settings.bellSize);
      _initializationError = null;
    } catch (error) {
      _initializationError = 'Jikido could not set up its audio: $error';
    }
    notifyListeners();
  }

  Future<void> setDuration(Duration duration) async {
    _settings = _settings.copyWith(duration: Settings.clampDuration(duration));
    notifyListeners();
    await _settings.save();
  }

  Future<void> setBell(Bell bell) async {
    _settings = _settings.copyWith(bell: bell);
    notifyListeners();
    await _audio.load(bell, _settings.bellSize);
    await _settings.save();
  }

  /// Casts the bell larger or smaller. One control, because a real bell's
  /// pitch and ring length are not independent — see [Settings.bellSize].
  Future<void> setBellSize(double size) async {
    _settings = _settings.copyWith(bellSize: Settings.clampBellSize(size));
    notifyListeners();
    // Re-casting the bell for every pixel of a slider drag would start a
    // background rendering per frame. Wait for the finger to settle.
    _bellReload?.cancel();
    _bellReload = Timer(_bellReloadDelay, () {
      unawaited(_audio.load(_settings.bell, _settings.bellSize));
    });
    await _settings.save();
  }

  Timer? _bellReload;
  static const Duration _bellReloadDelay = Duration(milliseconds: 250);

  /// Sets the silence between pressing Sit and the opening bell.
  Future<void> setPrepare(Duration prepare) async {
    _settings = _settings.copyWith(prepare: Settings.clampPrepare(prepare));
    notifyListeners();
    await _settings.save();
  }

  Future<void> setKeepScreenOn(bool keepScreenOn) async {
    _settings = _settings.copyWith(keepScreenOn: keepScreenOn);
    notifyListeners();
    await _settings.save();
    if (_status == SittingStatus.running) {
      await _screen.set(enabled: keepScreenOn);
    }
  }

  /// Rings the currently selected bell, so the user can hear what they are
  /// choosing. Only available when no sitting is in progress.
  Future<void> previewBell() async {
    if (_status == SittingStatus.running) {
      return;
    }
    if (_audio.isRinging) {
      await _audio.silence();
      return;
    }
    await strikeBell();
  }

  /// Strikes the bell once, letting it overlap whatever is still ringing.
  /// This is the free-play bell, and the overlap is the point: struck twice
  /// in quick succession a real bell does not start over, it adds.
  Future<void> strikeBell() async {
    if (_status == SittingStatus.running) {
      return;
    }
    await _audio.tap(_settings.bell, _settings.bellSize);
  }

  /// Lays the striker on the bowl, stopping whatever is ringing.
  Future<void> dampBell() async {
    if (_status == SittingStatus.running) {
      return;
    }
    await _audio.damp();
  }

  Future<void> start() async {
    if (_status == SittingStatus.running) {
      return;
    }

    if (!_permissionsRequested) {
      _permissionsRequested = true;
      await _engage(() => _notification.requestPermissions());
      await _engage(() => _service.requestPermissions());
    }

    final session = MeditationSession(
      beganAt: _now(),
      duration: _settings.duration,
      bell: _settings.bell,
      bellSize: _settings.bellSize,
      prepare: _settings.prepare,
    );
    _session = session;
    _status = SittingStatus.running;
    _pausedAt = null;
    _openingBellStruck = false;
    _closingBellStruck = false;
    _notice = null;
    _notifiedMinutesLeft = _wholeMinutes(session.duration);

    // The ticker goes up before the layers below, not after them. It only
    // ever asks the wall clock what should have happened by now, so it is
    // right from the instant the session exists — and starting it first
    // means a plugin that throws or takes its time cannot leave the user
    // watching a sitting whose clock never moves.
    _startTicker();
    notifyListeners();

    await _engageLayers(session);
  }

  void _startTicker() {
    _ticker?.cancel();
    _ticker = Timer.periodic(_tickInterval, (_) => _tick());
  }

  /// Engages everything that keeps a sitting alive to the closing bell: the
  /// opening bell itself, the held-open audio session, the wakelock, the
  /// foreground service and the backstop notification.
  ///
  /// The layers are independent by design — each covers a different way for
  /// the others to fail, see the README — so they are engaged independently
  /// too, and one that throws costs the sitting that layer and nothing else.
  Future<void> _engageLayers(MeditationSession session) async {
    // Ring first — unless there is settling time to sit through, in which
    // case the ticker rings it when it comes due. Everything below is
    // housekeeping, and a user who pressed a button expecting a bell should
    // not wait on it.
    if (session.openingBellIsDueAt(_now())) {
      _openingBellStruck = true;
      await _engage(() => _audio.strike(
          session.bell, session.bellSize, BellSequence.opening));
    }
    await _engage(() => _audio.startKeepAlive());

    if (_settings.keepScreenOn) {
      await _engage(() => _screen.set(enabled: true));
    }

    await _engage(() {
      final now = _now();
      final settling = session.phaseAt(now) == SessionPhase.preparing;
      if (settling) {
        _notifiedMinutesLeft = _settling;
      }
      return _service.start(
        text: settling
            ? 'Settling — the bell is coming'
            : _serviceText(session.remainingAt(now)),
      );
    });
    await _armBackstop(session);
  }

  /// Hands the operating system the bell to ring if Jikido is not around to
  /// ring it. Re-armed rather than adjusted whenever the closing bell moves,
  /// which pausing makes it do.
  Future<void> _armBackstop(MeditationSession session) => _engage(
        () => _notification.schedule(
          // Deliberately a little after the app would ring the bell itself;
          // see ClosingBellNotification.backstopDelay.
          at: session.closingBellAt.add(ClosingBellNotification.backstopDelay),
          bell: session.bell,
          sittingLength: session.duration,
        ),
      );

  /// Engages one layer, and carries on if it throws.
  ///
  /// The error is reported rather than swallowed, so it still reaches the
  /// log, but it is not allowed to abandon the rest of the setup. It used
  /// to: these calls were a straight run of awaits ending in the ticker, so
  /// a plugin throwing part-way through skipped everything after it — the
  /// ticker included, which left a sitting running with its clock frozen on
  /// the second it began. Android 14 makes that a live risk rather than a
  /// theoretical one: starting a foreground service is refused outright
  /// while the app is not in the foreground, which is exactly where the
  /// first-run permission dialog puts it.
  Future<void> _engage(Future<void> Function() layer) async {
    try {
      await layer();
    } catch (error, stack) {
      FlutterError.reportError(FlutterErrorDetails(
        exception: error,
        stack: stack,
        library: 'jikido',
        context: ErrorDescription('engaging a layer of a sitting'),
      ));
    }
  }

  /// Holds the sitting where it is. Works during the settling time too: what
  /// is held is whichever countdown is running.
  ///
  /// Nothing is ended here. The audio session, the foreground service and the
  /// wakelock all stay as they were, so a pause is a pause rather than a
  /// quiet way of losing the sitting to the process being reclaimed. What
  /// does go is the backstop notification: it is set for an instant that is
  /// no longer the end of anything, and it is armed again on the way back.
  Future<void> pause() async {
    if (!canPause) {
      return;
    }
    _pausedAt = _now();
    _ticker?.cancel();
    _ticker = null;
    // Force the notification to be rewritten on the first tick after
    // resuming, whatever it said before.
    _notifiedMinutesLeft = _unwritten;
    notifyListeners();

    await _engage(() => _notification.cancel());
    // A bell still ringing into a paused sitting would be the app carrying
    // on without the user.
    await _engage(() => _audio.silence());
    await _engage(() => _service.update(text: 'Paused'));
  }

  /// Carries on, giving back exactly as long as the pause lasted.
  ///
  /// The sitting is moved rather than credited: see
  /// [MeditationSession.delayedBy]. Everything derived from it — the opening
  /// bell if the pause was during the settling time, the closing bell, the
  /// end — moves with it, and the wall clock stays the only source of when.
  Future<void> resume() async {
    final session = _session;
    final pausedAt = _pausedAt;
    if (session == null ||
        pausedAt == null ||
        _status != SittingStatus.running) {
      return;
    }
    final resumed = session.delayedBy(_now().difference(pausedAt));
    _session = resumed;
    _pausedAt = null;
    _startTicker();
    // Puts the countdown, the ensō and the service notification back where
    // they belong now rather than up to a tick later.
    _tick();

    await _armBackstop(resumed);
  }

  /// Ends the sitting early, at the user's request. No bell: the sitting did
  /// not finish, and pretending otherwise would be a small lie told by a
  /// tool whose whole job is to mark time honestly.
  Future<void> cancel() async {
    if (_status == SittingStatus.idle) {
      return;
    }
    await _teardown();
    _session = null;
    _pausedAt = null;
    _status = SittingStatus.idle;
    _notice = null;
    notifyListeners();
  }

  /// Dismisses the completion screen.
  void acknowledge() {
    if (_status != SittingStatus.complete) {
      return;
    }
    _session = null;
    _pausedAt = null;
    _status = SittingStatus.idle;
    _notice = null;
    notifyListeners();
  }

  /// Called when the app comes back to the foreground.
  ///
  /// Timers do not necessarily run while an app is suspended, so the first
  /// thing to do on the way back is ask the clock what was missed.
  void onResumed() {
    if (_status == SittingStatus.running) {
      _tick();
    }
  }

  void _tick() {
    final session = _session;
    if (session == null || _status != SittingStatus.running || isPaused) {
      return;
    }
    final now = _now();

    if (!_openingBellStruck && session.openingBellIsDueAt(now)) {
      _openingBellStruck = true;
      // If the app was suspended through the settling time it comes back to a
      // sitting already under way, and an opening bell minutes late would be
      // a lie about where things are. The sitting is timed from the instant
      // either way.
      if (!session.openingBellIsStaleAt(now)) {
        unawaited(_audio.strike(
            session.bell, session.bellSize, BellSequence.opening));
      }
    }

    if (!_closingBellStruck && session.closingBellIsDueAt(now)) {
      _closingBellStruck = true;
      unawaited(_notification.cancel());
      if (session.closingBellIsStaleAt(now)) {
        // The app was not running when the bell was due. The scheduled
        // notification will have rung; ringing again now would only be
        // confusing, so say what happened instead.
        _notice = 'Jikido was stopped while you were sitting. '
            'The closing bell rang as a notification.';
        unawaited(_finish());
        return;
      }
      unawaited(_audio.strike(
          session.bell, session.bellSize, BellSequence.closing));
    }

    if (_closingBellStruck && !now.isBefore(session.endsAt)) {
      unawaited(_finish());
      return;
    }

    _updateServiceText(now, session);
    notifyListeners();
  }

  Future<void> _finish() async {
    _status = SittingStatus.complete;
    notifyListeners();
    // The bell player is left alone here: it may still be ringing, and the
    // ring is the point.
    _ticker?.cancel();
    _ticker = null;
    await _audio.stopKeepAlive();
    await _service.stop();
    await _screen.set(enabled: false);
  }

  Future<void> _teardown() async {
    _ticker?.cancel();
    _ticker = null;
    await _notification.cancel();
    await _audio.silence();
    await _audio.stopKeepAlive();
    await _service.stop();
    await _screen.set(enabled: false);
  }

  void _updateServiceText(DateTime now, MeditationSession session) {
    if (session.phaseAt(now) == SessionPhase.preparing) {
      // The settling time counts down in its own right, and saying "15
      // minutes left" while the period has not opened would be wrong. It is
      // short enough that one line for the whole of it is fine.
      if (_notifiedMinutesLeft != _settling) {
        _notifiedMinutesLeft = _settling;
        unawaited(_service.update(text: 'Settling — the bell is coming'));
      }
      return;
    }
    // Once a minute is plenty for a notification, and rewriting it five times
    // a second would be an odd thing to do to someone's battery.
    final remaining = session.remainingAt(now);
    final minutesLeft = _wholeMinutes(remaining);
    if (minutesLeft == _notifiedMinutesLeft) {
      return;
    }
    _notifiedMinutesLeft = minutesLeft;
    unawaited(_service.update(text: _serviceText(remaining)));
  }

  /// Stands in for a minute count while settling, so that the notification is
  /// written once rather than on every tick. No real count collides with it.
  static const int _settling = -2;

  /// Stands in for "the notification does not say a minute count at all", so
  /// that the next tick writes one whatever it happens to be. No real count
  /// collides with it either.
  static const int _unwritten = -1;

  static int _wholeMinutes(Duration remaining) =>
      (remaining.inSeconds / 60).ceil();

  static String _serviceText(Duration remaining) {
    final minutesLeft = _wholeMinutes(remaining);
    return switch (minutesLeft) {
      <= 0 => 'Less than a minute left',
      1 => 'About a minute left',
      _ => '$minutesLeft minutes left',
    };
  }

  @override
  void dispose() {
    _bellReload?.cancel();
    _ticker?.cancel();
    unawaited(_audio.dispose());
    super.dispose();
  }
}
