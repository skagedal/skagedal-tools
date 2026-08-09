import 'dart:async';

import 'package:audio_session/audio_session.dart';
import 'package:just_audio/just_audio.dart';

import '../bell.dart';

/// Plays the bells, and keeps the audio pipeline busy in between.
///
/// Two players, deliberately. The keep-alive player runs a near-silent loop
/// for the whole sitting and is never stopped or re-pointed; the bell player
/// is the one that actually rings. Sharing a single player would mean a gap
/// while the source is swapped, and a gap in output is exactly the moment
/// iOS looks for when deciding whether a backgrounded app still needs to be
/// running.
class BellAudio {
  BellAudio();

  final AudioPlayer _bellPlayer = AudioPlayer();
  final AudioPlayer _keepAlivePlayer = AudioPlayer();

  static const String _keepAliveAsset = 'assets/audio/keepalive.wav';

  Bell? _loadedBell;
  bool _keepAliveWanted = false;
  StreamSubscription<AudioInterruptionEvent>? _interruptions;

  /// Configures the audio session and pre-loads [bell].
  ///
  /// Call this before the user could plausibly press start: loading an asset
  /// takes long enough to be audible as a delay, and the opening bell should
  /// answer the button immediately.
  Future<void> initialize(Bell bell) async {
    final session = await AudioSession.instance;
    await session.configure(
      const AudioSessionConfiguration(
        // `playback` is what lets audio continue when the app is in the
        // background, and — the reason it matters here — it ignores the
        // ring/silent switch. A closing bell that the mute switch can
        // silence is not a closing bell.
        avAudioSessionCategory: AVAudioSessionCategory.playback,
        avAudioSessionCategoryOptions: AVAudioSessionCategoryOptions.duckOthers,
        avAudioSessionMode: AVAudioSessionMode.defaultMode,
        androidAudioAttributes: AndroidAudioAttributes(
          contentType: AndroidAudioContentType.music,
          // The alarm usage, rather than media. It plays at alarm volume and
          // survives Do Not Disturb, and because the opening bell uses it
          // too, the user hears at the start of the sitting exactly how loud
          // the closing bell will be.
          usage: AndroidAudioUsage.alarm,
        ),
        androidAudioFocusGainType:
            AndroidAudioFocusGainType.gainTransientMayDuck,
      ),
    );

    // A phone call or a timer from another app pauses our players. Nothing
    // brings them back on its own, and a keep-alive that quietly stopped
    // half an hour ago is worse than none at all.
    _interruptions ??= session.interruptionEventStream.listen((event) {
      if (!event.begin && _keepAliveWanted && !_keepAlivePlayer.playing) {
        unawaited(_keepAlivePlayer.play());
      }
    });

    await _keepAlivePlayer.setAsset(_keepAliveAsset);
    await _keepAlivePlayer.setLoopMode(LoopMode.one);
    await load(bell);
  }

  /// Loads [bell] into the bell player, if it is not loaded already.
  Future<void> load(Bell bell) async {
    if (_loadedBell == bell) {
      return;
    }
    await _bellPlayer.setAsset(bell.assetPath);
    _loadedBell = bell;
  }

  /// Strikes [bell] three times, from the top.
  ///
  /// Returns as soon as playback has started — the caller is running a clock
  /// and should not be blocked for the length of the ring.
  Future<void> strike(Bell bell) async {
    await load(bell);
    await _bellPlayer.seek(Duration.zero);
    await _bellPlayer.play();
  }

  /// Stops a bell that is still ringing.
  Future<void> silence() async {
    await _bellPlayer.pause();
    await _bellPlayer.seek(Duration.zero);
  }

  /// Whether a bell is ringing right now.
  bool get isRinging => _bellPlayer.playing;

  /// Starts the near-silent loop that holds the audio session open.
  Future<void> startKeepAlive() async {
    _keepAliveWanted = true;
    await _keepAlivePlayer.seek(Duration.zero);
    await _keepAlivePlayer.play();
  }

  Future<void> stopKeepAlive() async {
    _keepAliveWanted = false;
    await _keepAlivePlayer.pause();
  }

  Future<void> dispose() async {
    await _interruptions?.cancel();
    _interruptions = null;
    await _bellPlayer.dispose();
    await _keepAlivePlayer.dispose();
  }
}
