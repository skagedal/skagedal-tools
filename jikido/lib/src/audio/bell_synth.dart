/// The bell, synthesized.
///
/// This is a port of `tool/synthesize_bells.py`, which is the reference
/// implementation and carries the full account of where the numbers come
/// from. The short version: a struck bowl bell is a sum of exponentially
/// decaying inharmonic partials, and the values in [partials] and
/// [dominantQ] are measured from recordings of real inkin and rin bells.
///
/// The app synthesizes on the device rather than playing a bundled recording
/// so that every strike can differ a little from the last. Three strikes of
/// the same sample in a row is the one thing that gives a synthetic bell
/// away, and the fix is not noise: it is that a real striker never lands in
/// quite the same place twice. [BellStrike.contact] is that place, and
/// everything else about a strike's character follows from it.
///
/// Nothing in here touches a plugin, so it all runs under `flutter test`.
library;

import 'dart:math';
import 'dart:typed_data';

/// One partial of the bell, as a pair of modes.
class Partial {
  const Partial({
    required this.ratio,
    required this.amplitude,
    required this.decay,
    required this.beatHz,
  });

  /// Frequency as a multiple of the hum mode.
  final double ratio;

  /// Level relative to the partial that carries the pitch.
  final double amplitude;

  /// Decay time as a fraction of the pitch partial's.
  final double decay;

  /// Hz between the two modes of the pair, whose interference is the warble.
  final double beatHz;
}

/// Measured from a traditional inkin, a flattop inkin, a portable inkin and
/// two rin bowls, which agree to within a couple of percent.
///
/// The partial at 2.70 is the one heard as the bell's pitch, and it is more
/// than ten times the hum. That is the whole shape of an inkin: modelling the
/// hum as the loudest partial gives something far darker than any real one.
const List<Partial> partials = <Partial>[
  Partial(ratio: 1.000, amplitude: 0.08, decay: 1.00, beatHz: 1.7),
  Partial(ratio: 2.700, amplitude: 1.00, decay: 1.00, beatHz: 4.5),
  Partial(ratio: 4.930, amplitude: 0.12, decay: 0.19, beatHz: 6.5),
  Partial(ratio: 7.670, amplitude: 0.02, decay: 0.06, beatHz: 8.0),
];

/// Index into [partials] of the one that carries the pitch.
const int dominantIndex = 1;

/// Quality factor of the pitch partial: `tau = Q / (pi * f)`.
///
/// Holding this constant is what makes one size control physical. A bigger
/// bell of the same shape sounds lower and rings longer in proportion, and a
/// constant Q says exactly that. Measured 20300 on the traditional inkin.
const double dominantQ = 20300;

/// The striker laid on the bowl rather than lifted away: how long the bell
/// rings before the hand arrives, and how fast it dies once it does. Together
/// these put a strike 20 dB down in 0.17 s, against 2.5-4.7 s for one left to
/// ring.
const double dampedAfter = 0.06;
const double dampedTau = 0.05;

const int sampleRate = 32000;

/// The longest ring worth rendering.
///
/// A keisu at the largest size has a decay time of 26 seconds, and the 2.5 of
/// those a full ring wants is over a minute of audio — for a bell that is
/// 20 dB down before half of it has played. Holding a minute of samples in
/// memory to play something nobody can hear any more is not worth the
/// megabytes, so the tail stops here and the fade takes it the rest of the
/// way. No bell at any offered size reaches this except the largest keisu.
const double maxTailSeconds = 30.0;

/// A tiny linear congruential generator.
///
/// The synthesizer needs a repeatable spray of numbers for the mode phases
/// and the contact noise, and it uses this rather than [Random] so that it
/// agrees with the Python reference implementation: Dart's `Random` and
/// Python's Mersenne Twister do not produce the same sequence, and a model
/// that cannot be compared across the two cannot be kept in step with them.
class Noise {
  Noise(int seed) : _state = seed & 0xFFFFFFFF;

  int _state;

  /// The next value, in [0, 1).
  double unit() {
    _state = (_state * 1664525 + 1013904223) & 0xFFFFFFFF;
    return _state / 4294967296.0;
  }

  /// The next value, in [-1, 1).
  double signed() => unit() * 2.0 - 1.0;
}

/// The noise seed a strike at [contact] uses.
///
/// Varying the contact point varies the phases and the contact noise along
/// with the mode balance, so one number is all a caller has to choose to get
/// a different strike.
int contactSeed(double contact) =>
    (contact.abs() * 1000003).toInt() & 0xFFFFFFFF;

/// A bell, as the synthesizer sees it.
class BellVoice {
  const BellVoice({required this.dominantHz});

  /// The pitch you hear, in Hz — the frequency of `partials[dominantIndex]`.
  final double dominantHz;

  double get humHz => dominantHz / partials[dominantIndex].ratio;

  /// Decay time of the pitch partial, from the constant-Q rule.
  double get dominantTau => dominantQ / (pi * dominantHz);

  /// This bell made larger ([size] > 1) or smaller ([size] < 1).
  ///
  /// Frequency goes inversely with size, and because Q is held constant the
  /// ring time follows on its own — which is why there is one control here
  /// and not two.
  BellVoice scaled(double size) => BellVoice(dominantHz: dominantHz / size);

  @override
  bool operator ==(Object other) =>
      other is BellVoice && other.dominantHz == dominantHz;

  @override
  int get hashCode => dominantHz.hashCode;
}

/// One strike within a sequence.
class BellStrike {
  const BellStrike({
    required this.at,
    this.gain = 1.0,
    this.dampAfter,
    this.contact = 0.0,
  });

  /// Seconds from the start of the sequence.
  final double at;

  final double gain;

  /// Seconds after this strike at which the bell is damped, or null to let it
  /// ring out.
  final double? dampAfter;

  /// Where on the rim the bell was struck, as a phase in turns.
  final double contact;
}

/// Renders a single strike into a buffer of [length] samples.
///
/// Deterministic: the same voice, length and contact always give the same
/// samples. Variation between strikes comes from the caller choosing a
/// different [contact], which is the physical variable anyway.
Float64List renderStrike(
  BellVoice voice,
  int length,
  double contact, {
  int rate = sampleRate,
}) {
  final out = Float64List(length);
  final noise = Noise(contactSeed(contact));
  final nyquist = rate * 0.45;
  final tau0 = voice.dominantTau;

  for (final partial in partials) {
    final frequency = voice.humHz * partial.ratio;
    if (frequency >= nyquist) {
      continue;
    }
    final tau = tau0 * partial.decay;

    // The two modes of the pair, and how the strike divides its energy
    // between them. Where the bell is struck decides which comes out on top.
    //
    // The weaker is kept between 15% and 45% of the stronger. That is the
    // 1-2.5 dB of envelope ripple measured on the real bells, and the bound
    // matters in both directions: two modes of equal level beat all the way
    // down to silence, which sounds like a tremolo pedal rather than a bell,
    // and one mode alone sounds dead.
    final theta = 2.0 * pi * contact * partial.ratio;
    final weaker = 0.15 + 0.30 * cos(theta).abs();
    final split = sin(theta) < 0
        ? <double>[weaker, 1.0]
        : <double>[1.0, weaker];
    final norm = sqrt(split[0] * split[0] + split[1] * split[1]);

    for (var mode = 0; mode < 2; mode++) {
      final detuned = frequency + (mode - 0.5) * partial.beatHz;
      final level = partial.amplitude * split[mode] / norm;

      // An exponentially decaying sinusoid is a rotation and a shrink applied
      // over and over, so it costs two multiplies a sample rather than a
      // sin() and an exp().
      final shrink = exp(-1.0 / (tau * rate));
      final omega = 2.0 * pi * detuned / rate;
      final stepRe = shrink * cos(omega);
      final stepIm = shrink * sin(omega);

      final phase = 2.0 * pi * noise.unit();
      var re = cos(phase);
      var im = sin(phase);
      for (var n = 0; n < length; n++) {
        out[n] += level * im;
        final nextRe = re * stepRe - im * stepIm;
        im = re * stepIm + im * stepRe;
        re = nextRe;
      }
    }
  }

  // The striker hitting the metal: a very short burst of noise, differenced
  // to tilt it towards the high end where contact noise actually lives.
  const noiseTau = 0.006;
  final noiseLength = min(length, (rate * noiseTau * 8).toInt());
  var previous = 0.0;
  for (var n = 0; n < noiseLength; n++) {
    final sample = noise.signed();
    out[n] += 0.10 * (sample - previous) * exp(-n / (noiseTau * rate));
    previous = sample;
  }

  // A couple of milliseconds of attack ramp removes the click that a hard
  // onset would otherwise put at the start.
  final attack = (rate * 0.002).toInt();
  for (var n = 0; n < min(attack, length); n++) {
    out[n] *= n / attack;
  }

  return out;
}

/// Renders a sequence of strikes, tails overlapping as on a real bell.
Float64List renderSequence(
  BellVoice voice,
  List<BellStrike> strikes, {
  required double tailSeconds,
  int rate = sampleRate,
  bool normalize = true,
}) {
  final last = strikes.map((s) => s.at).reduce(max);
  final total = ((last + tailSeconds) * rate).toInt();
  final out = Float64List(total);

  for (final strike in strikes) {
    final offset = (strike.at * rate).toInt();
    final rendered = renderStrike(voice, total - offset, strike.contact,
        rate: rate);
    for (var n = 0; n < rendered.length; n++) {
      out[offset + n] += strike.gain * rendered[n];
    }
  }

  // Damping is applied to the mix, and after every strike has been added,
  // because a hand laid on the bowl stops the whole bell — including whatever
  // is still ringing from the strike before. Damping a strike's own buffer
  // instead leaves the previous one sounding on underneath, which in a
  // two-strike closing is very nearly the entire sound.
  for (final strike in strikes) {
    final dampAfter = strike.dampAfter;
    if (dampAfter != null) {
      _damp(out, ((strike.at + dampAfter) * rate).toInt(), rate);
    }
  }

  // Fade the last half second so the buffer cannot end on a discontinuity.
  final fade = min((rate * 0.5).toInt(), total);
  for (var n = 0; n < fade; n++) {
    out[total - fade + n] *= 1.0 - n / fade;
  }

  if (normalize) {
    var peak = 0.0;
    for (final value in out) {
      peak = max(peak, value.abs());
    }
    if (peak > 0) {
      final scale = 0.89 / peak; // Leave a little headroom below full scale.
      for (var n = 0; n < total; n++) {
        out[n] *= scale;
      }
    }
  }
  return out;
}

/// Lays the striker on the bowl: everything from [fromSample] dies fast.
void _damp(Float64List samples, int fromSample, int rate) {
  final decay = exp(-1.0 / (dampedTau * rate));
  var gain = 1.0;
  for (var n = max(0, fromSample); n < samples.length; n++) {
    samples[n] *= gain;
    gain *= decay;
  }
}

/// How long a sequence takes, from the first strike to silence.
///
/// A damped ending changes this completely, which is why it is worth
/// computing rather than assuming: a keisu left to ring needs half a minute,
/// and the same keisu stopped by the hand needs half a second. The sitting is
/// not over until this has elapsed.
double sequenceSeconds(BellVoice voice, List<BellStrike> strikes) {
  final lastStrike = strikes.map((s) => s.at).reduce(max);
  final dampings = <double>[
    for (final strike in strikes)
      if (strike.dampAfter != null) strike.at + strike.dampAfter!,
  ];
  // A damping only ends the sequence if nothing is struck after it.
  final finalDampings = dampings.where((d) => d >= lastStrike);
  if (finalDampings.isNotEmpty) {
    // Eight time constants is 70 dB down, which is silence.
    return finalDampings.reduce(max) + dampedTau * 8;
  }
  return lastStrike + min(voice.dominantTau * 2.5, maxTailSeconds);
}

/// How long the opening sequence rings for at this size.
///
/// The contact points do not affect the timing, only the sound, so it does
/// not matter which ones these are.
double openingSeconds(BellVoice voice) =>
    sequenceSeconds(voice, openingStrikes(voice, Random(0)));

/// How long the closing sequence rings for at this size. Much shorter than
/// [openingSeconds], because it ends under the hand rather than fading.
double closingSeconds(BellVoice voice) =>
    sequenceSeconds(voice, closingStrikes(voice, Random(0)));

/// Seconds between strikes.
///
/// The recordings sit at 1.4-1.8 s, but those are demonstrations rather than
/// zazen. At 2.4 s each strike is given room to be its own, which is what
/// opening a period of sitting wants. A bigger bell is given longer still,
/// because the ring it is leaving room for is longer.
double strikeInterval(BellVoice voice) =>
    2.4 * max(1.0, sqrt(3245.0 / voice.dominantHz));

/// Three strikes, ringing out. This opens a period of zazen.
List<BellStrike> openingStrikes(BellVoice voice, Random random) {
  final interval = strikeInterval(voice);
  const gains = <double>[0.94, 1.0, 0.90];
  return <BellStrike>[
    for (var i = 0; i < gains.length; i++)
      BellStrike(
        at: i * interval,
        gain: gains[i],
        contact: random.nextDouble(),
      ),
  ];
}

/// Two strikes, the second damped. This closes a period of zazen.
///
/// The striker is laid on the bowl straight after the second strike rather
/// than lifted away, so the ring is stopped rather than allowed to fade. It
/// is how a period ends in a zendo, and it is unmistakable: the sitting is
/// over, not fading out.
List<BellStrike> closingStrikes(BellVoice voice, Random random) {
  final interval = strikeInterval(voice);
  return <BellStrike>[
    BellStrike(at: 0, gain: 1.0, contact: random.nextDouble()),
    BellStrike(
      at: interval,
      gain: 0.95,
      dampAfter: dampedAfter,
      contact: random.nextDouble(),
    ),
  ];
}

/// A single strike, for the free-play bell.
List<BellStrike> singleStrike(Random random) =>
    <BellStrike>[BellStrike(at: 0, gain: 1.0, contact: random.nextDouble())];

/// Wraps samples in a 16-bit mono WAV, which is what the player wants.
Uint8List wavBytes(Float64List samples, {int rate = sampleRate}) {
  final dataBytes = samples.length * 2;
  final bytes = ByteData(44 + dataBytes);

  void ascii(int offset, String text) {
    for (var i = 0; i < text.length; i++) {
      bytes.setUint8(offset + i, text.codeUnitAt(i));
    }
  }

  ascii(0, 'RIFF');
  bytes.setUint32(4, 36 + dataBytes, Endian.little);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  bytes.setUint32(16, 16, Endian.little); // PCM header length
  bytes.setUint16(20, 1, Endian.little); // uncompressed
  bytes.setUint16(22, 1, Endian.little); // mono
  bytes.setUint32(24, rate, Endian.little);
  bytes.setUint32(28, rate * 2, Endian.little); // bytes per second
  bytes.setUint16(32, 2, Endian.little); // bytes per frame
  bytes.setUint16(34, 16, Endian.little); // bits per sample
  ascii(36, 'data');
  bytes.setUint32(40, dataBytes, Endian.little);

  for (var n = 0; n < samples.length; n++) {
    final clamped = samples[n].clamp(-1.0, 1.0);
    bytes.setInt16(44 + n * 2, (clamped * 32767.0).toInt(), Endian.little);
  }
  return bytes.buffer.asUint8List();
}
