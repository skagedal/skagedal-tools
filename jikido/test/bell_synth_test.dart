import 'dart:math';

import 'package:flutter_test/flutter_test.dart';
import 'package:jikido/src/audio/bell_synth.dart';

/// The synthesizer exists twice: here in Dart, for the bells the app rings,
/// and in `tool/synthesize_bells.py`, for the notification asset the OS plays
/// when the app is not running. Two implementations of one model drift apart
/// unless something holds them together, and these goldens are that.
///
/// They were produced by the Python reference and pasted in. Regenerate them
/// the same way if the model changes deliberately:
///
///     python3 -c "import sys; sys.path.insert(0, 'tool'); \
///       import synthesize_bells as sb; \
///       v = sb.Voice(name='inkin', dominant_hz=3245.0); \
///       s = sb.render_strike(v, sb.SAMPLE_RATE, contact=0.42); \
///       print([s[i] for i in (0, 1, 7, 64, 100, 1000, 5000, 12345, 20000, 31999)])"
///
/// The tolerance is there because `sin`, `cos` and `exp` are the platform's
/// and the two languages need not agree in the last bit. It is far tighter
/// than any change to the model could hide in.
void main() {
  const voice = BellVoice(dominantHz: 3245.0);
  const tolerance = 1e-9;

  test('a strike matches the Python reference sample for sample', () {
    final samples = renderStrike(voice, sampleRate, 0.42);

    const golden = <int, double>{
      0: -0.0,
      1: 0.0025875690063850007,
      7: -0.020732257457853995,
      64: 0.29112109840156575,
      100: 0.2734672732778063,
      1000: 0.9557312110378476,
      5000: -0.7883202943226862,
      12345: 0.17145129922083097,
      20000: -0.5413627951828298,
      31999: -0.6284157298023565,
    };

    golden.forEach((index, expected) {
      expect(samples[index], closeTo(expected, tolerance),
          reason: 'sample $index');
    });
  });

  test('a damped sequence matches the Python reference', () {
    final strikes = <BellStrike>[
      const BellStrike(at: 0, gain: 1.0, contact: 0.11),
      const BellStrike(
          at: 0.5, gain: 0.95, dampAfter: dampedAfter, contact: 0.73),
    ];
    final samples = renderSequence(voice, strikes, tailSeconds: 0.6);

    expect(samples.length, 35200);

    const golden = <int, double>{
      0: 0.0,
      100: -0.36085992006377066,
      8000: 0.3439348238316009,
      16000: 0.304944080960263,
      17000: 0.40833166796151854,
      20000: -0.04104001462317705,
      30000: -3.015517929114501e-05,
      34000: 1.9117221488859187e-06,
    };

    golden.forEach((index, expected) {
      expect(samples[index], closeTo(expected, tolerance),
          reason: 'sample $index');
    });
  });

  test('the derived quantities match the reference', () {
    expect(sequenceSeconds(voice, <BellStrike>[
      const BellStrike(at: 0, contact: 0.11),
      const BellStrike(at: 0.5, dampAfter: dampedAfter, contact: 0.73),
    ]), closeTo(0.9600000000000001, tolerance));
    expect(strikeInterval(voice), closeTo(2.4, tolerance));
    expect(voice.dominantTau, closeTo(1.9912760214271032, tolerance));
  });

  test('the same contact gives the same strike, a different one does not', () {
    final a = renderStrike(voice, 2000, 0.42);
    final b = renderStrike(voice, 2000, 0.42);
    final c = renderStrike(voice, 2000, 0.43);

    expect(a, orderedEquals(b),
        reason: 'the synthesizer must be deterministic, or the goldens above '
            'and the Python reference mean nothing');
    expect(a, isNot(orderedEquals(c)),
        reason: 'a striker landing somewhere else must sound like it');
  });

  test('the damping stops the whole bell, not just the strike that damped it',
      () {
    // The first strike is loud and left ringing; the second is quiet and
    // damped. If damping were applied per strike rather than to the mix, the
    // first strike would ring on underneath and dominate the tail.
    final strikes = <BellStrike>[
      const BellStrike(at: 0, gain: 1.0, contact: 0.2),
      const BellStrike(at: 0.5, gain: 0.3, dampAfter: 0.02, contact: 0.6),
    ];
    final samples = renderSequence(voice, strikes, tailSeconds: 1.0,
        normalize: false);

    double rms(double from, double to) {
      var sum = 0.0;
      final a = (from * sampleRate).toInt();
      final b = (to * sampleRate).toInt();
      for (var n = a; n < b; n++) {
        sum += samples[n] * samples[n];
      }
      return sqrt(sum / (b - a));
    }

    final before = rms(0.3, 0.5);
    final after = rms(0.8, 1.0);
    // Damped per strike instead, the first strike would still be ringing here
    // at roughly 0.6 of its level — its decay time is two seconds. Damped on
    // the mix it is 40 dB down. The two outcomes are nowhere near each other,
    // so the threshold does not have to be delicate.
    expect(after, lessThan(before / 100),
        reason: 'a hand on the bowl silences the bell');
  });

  test('a bigger bell sounds lower and rings longer, in proportion', () {
    const small = BellVoice(dominantHz: 3245.0);
    final big = small.scaled(2.0);

    expect(big.dominantHz, closeTo(small.dominantHz / 2, 1e-12));
    expect(big.dominantTau, closeTo(small.dominantTau * 2, 1e-12),
        reason: 'constant Q is what ties pitch and ring together, so that '
            'size is one control rather than two');
  });

  test('the WAV wrapper is a WAV', () {
    final bytes = wavBytes(renderStrike(voice, 1000, 0.1));

    expect(String.fromCharCodes(bytes.sublist(0, 4)), 'RIFF');
    expect(String.fromCharCodes(bytes.sublist(8, 12)), 'WAVE');
    expect(String.fromCharCodes(bytes.sublist(36, 40)), 'data');
    expect(bytes.length, 44 + 1000 * 2);
  });
}
