import 'dart:async';
import 'dart:math';

import 'package:audio_session/audio_session.dart';
import 'package:flutter/foundation.dart';
import 'package:just_audio/just_audio.dart';

import '../bell.dart';
import 'bell_synth.dart';

/// Which sequence of strikes to ring.
enum BellSequence {
  /// Three strikes, ringing out. Opens a period of zazen.
  opening,

  /// Two strikes, the second stopped by the striker laid on the bowl.
  /// Closes a period.
  closing,

  /// One strike, for the free-play bell and for previewing a setting.
  single,
}

/// Plays the bells, and keeps the audio pipeline busy in between.
///
/// The bells are synthesized here rather than loaded from a file — see
/// `bell_synth.dart` for the model and why — which buys the thing a recording
/// cannot give: every strike is a little different, because the striker never
/// lands in quite the same place twice.
///
/// Synthesis takes long enough to drop frames if it happens when the bell is
/// wanted, so it never does. Everything is rendered ahead, on a background
/// isolate, and what a strike does is hand an already-finished buffer to a
/// player. After each strike the next rendering starts, so the bell is always
/// one ahead of the user.
///
/// The finished bells stay in memory. Handing just_audio a buffer means its
/// `StreamAudioSource`, which is marked experimental — the alternative is
/// writing a megabyte or more to the cache for every strike, and the
/// free-play bell is a thing people tap repeatedly. Latency there is the
/// whole experience, so the experimental API is the right trade; see the
/// ignore on [_WavSource] for what to watch on a just_audio upgrade.
///
/// The players are deliberately several. The keep-alive player runs a
/// near-silent loop for the whole sitting and is never stopped or re-pointed;
/// sharing one player would mean a gap while the source is swapped, and a gap
/// in output is exactly the moment iOS looks for when deciding whether a
/// backgrounded app still needs to be running. The free-play pool is separate
/// again, so that tapping the bell twice lets the two rings overlap the way
/// they would on a real one.
class BellAudio {
  BellAudio({Random? random}) : _random = random ?? Random();

  final AudioPlayer _bellPlayer = AudioPlayer();
  final AudioPlayer _keepAlivePlayer = AudioPlayer();

  /// Players for the free-play bell. Four is enough that a person tapping as
  /// fast as is meaningful never cuts off their own last strike.
  static const int _poolSize = 4;
  final List<AudioPlayer> _pool =
      List<AudioPlayer>.generate(_poolSize, (_) => AudioPlayer());
  int _nextPlayer = 0;

  final Random _random;

  static const String _keepAliveAsset = 'assets/audio/keepalive.wav';

  bool _keepAliveWanted = false;
  StreamSubscription<AudioInterruptionEvent>? _interruptions;

  /// Renderings waiting to be played, one per sequence. Replaced as they are
  /// used, so the next strike is never the same as the last.
  final Map<BellSequence, _Rendering> _ready = <BellSequence, _Rendering>{};
  final Set<BellSequence> _rendering = <BellSequence>{};

  Bell _bell = Bell.inkin;
  double _size = 1.0;

  /// Configures the audio session and renders the first bells.
  ///
  /// Call this before the user could plausibly press start: the opening bell
  /// should answer the button immediately, and it can only do that if it has
  /// already been synthesized.
  Future<void> initialize(Bell bell, double size) async {
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
    await load(bell, size);
  }

  /// Points the synthesizer at [bell] at [size], and renders it.
  ///
  /// Cheap to call with what is already loaded — it returns immediately.
  Future<void> load(Bell bell, double size) async {
    if (_bell == bell && _size == size && _ready.isNotEmpty) {
      return;
    }
    if (_bell != bell || _size != size) {
      _bell = bell;
      _size = size;
      // Everything in hand is the wrong bell now.
      _ready.clear();
    }
    // The opening is the one the user waits on, so it is rendered first and
    // awaited; the others catch up in the background.
    await _render(BellSequence.opening);
    unawaited(_render(BellSequence.closing));
    unawaited(_render(BellSequence.single));
  }

  /// Rings [sequence] on the given bell, from the top.
  ///
  /// Returns as soon as playback has started — the caller is running a clock
  /// and should not be blocked for the length of the ring.
  Future<void> strike(
    Bell bell,
    double size,
    BellSequence sequence,
  ) async {
    await load(bell, size);
    final rendering = _ready.remove(sequence) ?? await _renderNow(sequence);
    // Start the replacement before playing, so that a second strike close
    // behind this one still finds something ready.
    unawaited(_render(sequence));

    await _bellPlayer.setVolume(1);
    await _bellPlayer.setAudioSource(_WavSource(rendering.bytes));
    await _bellPlayer.play();
  }

  /// Strikes the free-play bell, letting it overlap whatever is still
  /// ringing, the way a second strike on a real bell does.
  Future<void> tap(Bell bell, double size) async {
    await load(bell, size);
    final rendering =
        _ready.remove(BellSequence.single) ?? await _renderNow(BellSequence.single);
    unawaited(_render(BellSequence.single));

    final player = _pool[_nextPlayer];
    _nextPlayer = (_nextPlayer + 1) % _poolSize;
    await player.setVolume(1);
    await player.setAudioSource(_WavSource(rendering.bytes));
    await player.play();
  }

  /// Lays the striker on the bowl: everything ringing dies away fast.
  ///
  /// The synthesizer can bake this into a rendering, but only when it knows
  /// in advance where the damping falls — which it does for the closing bell
  /// and cannot for a person tapping a screen. So this is done to the players
  /// instead, as a fall in level following the same time constant the
  /// synthesizer uses, and it sounds the same because damping a bell mostly
  /// *is* the level falling off a cliff.
  Future<void> damp() async {
    final playing = <AudioPlayer>[
      ..._pool.where((player) => player.playing),
      if (_bellPlayer.playing) _bellPlayer,
    ];
    if (playing.isEmpty) {
      return;
    }

    const step = Duration(milliseconds: 20);
    // Eight time constants is 70 dB down, which is silence.
    final steps = (dampedTau * 8 * 1000 / step.inMilliseconds).round();
    for (var i = 1; i <= steps; i++) {
      final level = exp(-i * step.inMilliseconds / 1000 / dampedTau);
      await Future.wait(playing.map((p) => p.setVolume(level)));
      await Future<void>.delayed(step);
    }
    await Future.wait(playing.map((player) async {
      await player.stop();
      await player.setVolume(1);
    }));
  }

  /// Stops a bell that is still ringing, at once and without ceremony.
  Future<void> silence() async {
    await _bellPlayer.stop();
    await Future.wait(_pool.map((player) => player.stop()));
  }

  /// Whether a bell is ringing right now.
  bool get isRinging =>
      _bellPlayer.playing || _pool.any((player) => player.playing);

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
    await Future.wait(_pool.map((player) => player.dispose()));
  }

  /// Renders [sequence] in the background and files it away, unless one is
  /// already in hand or already being rendered.
  Future<void> _render(BellSequence sequence) async {
    if (_ready.containsKey(sequence) || _rendering.contains(sequence)) {
      return;
    }
    _rendering.add(sequence);
    final bell = _bell;
    final size = _size;
    try {
      final bytes = await compute(_synthesize, _Request(
        dominantHz: bell.voiceAt(size).dominantHz,
        sequence: sequence.index,
        seed: _random.nextInt(1 << 31),
      ));
      // The bell may have been changed while this was rendering.
      if (bell == _bell && size == _size) {
        _ready[sequence] = _Rendering(bytes);
      }
    } finally {
      _rendering.remove(sequence);
    }
  }

  /// Renders [sequence] right now, on this isolate, because it is wanted and
  /// nothing is ready. Only reached if a strike arrives before the render
  /// that was started ahead of it has finished.
  Future<_Rendering> _renderNow(BellSequence sequence) async {
    final bytes = _synthesize(_Request(
      dominantHz: _bell.voiceAt(_size).dominantHz,
      sequence: sequence.index,
      seed: _random.nextInt(1 << 31),
    ));
    return _Rendering(bytes);
  }
}

/// A finished bell, waiting to be played.
class _Rendering {
  const _Rendering(this.bytes);

  final Uint8List bytes;
}

/// What one rendering needs to know. Kept to plain fields so that it can
/// cross to another isolate without ceremony.
class _Request {
  const _Request({
    required this.dominantHz,
    required this.sequence,
    required this.seed,
  });

  final double dominantHz;
  final int sequence;
  final int seed;
}

/// Renders a sequence to WAV bytes. Top-level, because [compute] runs it on
/// another isolate.
Uint8List _synthesize(_Request request) {
  final voice = BellVoice(dominantHz: request.dominantHz);
  final random = Random(request.seed);
  final strikes = switch (BellSequence.values[request.sequence]) {
    BellSequence.opening => openingStrikes(voice, random),
    BellSequence.closing => closingStrikes(voice, random),
    BellSequence.single => singleStrike(random),
  };
  final last = strikes.map((strike) => strike.at).reduce(max);
  final samples = renderSequence(
    voice,
    strikes,
    tailSeconds: sequenceSeconds(voice, strikes) - last,
  );
  return wavBytes(samples);
}

/// Feeds an in-memory WAV to just_audio, which otherwise wants a file or a
/// URL and we have neither.
///
/// `StreamAudioSource` is the only way in, and just_audio marks it
/// experimental. That is a deliberate exception to the house rule against
/// suppressing analyzer complaints rather than fixing them: there is nothing
/// to fix, the alternative is writing every rendered bell to disk before it
/// can be played, and this is the one place a just_audio upgrade could break
/// the app. If it ever does, the fallback is a file in the cache directory.
// ignore: experimental_member_use
class _WavSource extends StreamAudioSource {
  _WavSource(this._bytes);

  final Uint8List _bytes;

  @override
  // ignore: experimental_member_use
  Future<StreamAudioResponse> request([int? start, int? end]) async {
    final from = start ?? 0;
    final to = end ?? _bytes.length;
    // ignore: experimental_member_use
    return StreamAudioResponse(
      sourceLength: _bytes.length,
      contentLength: to - from,
      offset: from,
      stream: Stream<List<int>>.value(_bytes.sublist(from, to)),
      contentType: 'audio/wav',
    );
  }
}
