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
  Timer? _ticker;
  bool _closingBellStruck = false;
  bool _permissionsRequested = false;
  String? _notice;
  int _notifiedMinutesLeft = -1;

  Settings get settings => _settings;
  SittingStatus get status => _status;
  MeditationSession? get session => _session;

  /// A message about something that did not go to plan — currently only the
  /// case where the app was killed and came back after the sitting ended.
  String? get notice => _notice;

  Duration get remaining =>
      _session?.remainingAt(_now()) ?? _settings.duration;

  double get progress => _session?.progressAt(_now()) ?? 0;

  SessionPhase? get phase => _session?.phaseAt(_now());

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
      await _audio.initialize(_settings.bell);
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
    await _audio.load(bell);
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
    await _audio.strike(_settings.bell);
  }

  Future<void> start() async {
    if (_status == SittingStatus.running) {
      return;
    }

    if (!_permissionsRequested) {
      _permissionsRequested = true;
      await _notification.requestPermissions();
      await _service.requestPermissions();
    }

    final session = MeditationSession(
      startedAt: _now(),
      duration: _settings.duration,
      bell: _settings.bell,
    );
    _session = session;
    _status = SittingStatus.running;
    _closingBellStruck = false;
    _notice = null;
    _notifiedMinutesLeft = -1;
    notifyListeners();

    // Ring first. Everything below is housekeeping, and the user pressed a
    // button expecting a bell.
    await _audio.strike(session.bell);
    await _audio.startKeepAlive();

    if (_settings.keepScreenOn) {
      await _screen.set(enabled: true);
    }

    final remaining = session.remainingAt(_now());
    _notifiedMinutesLeft = _wholeMinutes(remaining);
    await _service.start(text: _serviceText(remaining));
    await _notification.schedule(
      // Deliberately a little after the app would ring the bell itself; see
      // ClosingBellNotification.backstopDelay.
      at: session.closingBellAt.add(ClosingBellNotification.backstopDelay),
      bell: session.bell,
      sittingLength: session.duration,
    );

    _ticker?.cancel();
    _ticker = Timer.periodic(_tickInterval, (_) => _tick());
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
    if (session == null || _status != SittingStatus.running) {
      return;
    }
    final now = _now();

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
      unawaited(_audio.strike(session.bell));
    }

    if (_closingBellStruck && !now.isBefore(session.endsAt)) {
      unawaited(_finish());
      return;
    }

    _updateServiceText(session.remainingAt(now));
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

  void _updateServiceText(Duration remaining) {
    // Once a minute is plenty for a notification, and rewriting it five times
    // a second would be an odd thing to do to someone's battery.
    final minutesLeft = _wholeMinutes(remaining);
    if (minutesLeft == _notifiedMinutesLeft) {
      return;
    }
    _notifiedMinutesLeft = minutesLeft;
    unawaited(_service.update(text: _serviceText(remaining)));
  }

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
    _ticker?.cancel();
    unawaited(_audio.dispose());
    super.dispose();
  }
}
