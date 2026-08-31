#!/usr/bin/env python3
"""Synthesizes the bell assets in `assets/audio/`.

Jikido ships generated bell sounds rather than recordings, so that the audio
is redistributable and reproducible.

This is also the reference implementation of the bell model. The app
synthesizes its bells on the device, in Dart, so that each strike can differ
a little from the last; `lib/src/audio/bell_synth.dart` is a port of what is
here and the two are kept in step by `test/bell_synth_test.dart`. What this
script still renders on its own is the closing-bell notification, which has
to be a file on disk for the operating system to play when Jikido is not
running.

## The model

A struck bowl bell is a sum of exponentially decaying inharmonic partials.
The numbers below are measured from recordings of real inkin and rin bells
rather than guessed; see `PARTIALS` and `DOMINANT_Q` for what was measured
and how it is used.

Two things about a real bell matter more than they might seem to:

Each partial is really a *pair* of modes a few Hz apart, because no bowl is
perfectly circular. The interference between them is the slow warble that
keeps a bell from sounding like an organ pipe. Which of the pair is louder
depends on where the bell is struck, so it changes from strike to strike.

The upper partials die away very fast — the third partial of an inkin is
gone in a third of a second while the second is still ringing after two.
That collapse from a bright clang to a pure tone is most of what makes the
attack sound like metal.

Run from the `jikido/` directory:

    python3 tool/synthesize_bells.py

Only the standard library is used, so there is nothing to install.
"""

from __future__ import annotations

import cmath
import math
import os
import random
import shutil
import struct
import wave
from dataclasses import dataclass

# The partials of a bowl bell, measured from the recordings in
# ~/icloud/projects/jikido-bells: a traditional inkin, a flattop inkin, a
# portable inkin and two rin bowls, which agree to within a couple of percent.
#
# Ratios are relative to the hum mode, amplitudes relative to the *second*
# partial, and that is the whole point: the partial at 2.70 is the one you
# hear as the bell's pitch, and it is more than ten times the hum. Modelling
# the hum as the loudest partial — which is the obvious thing to do, and what
# this script used to do — produces something far darker and duller than any
# real inkin.
#
# `decay` is each partial's decay time as a fraction of the second partial's.
# Measured: 0.34 s against 1.99 s, and 0.10 s against 1.99 s. A partial an
# octave up dies in a fifth of the time, not four fifths.
@dataclass(frozen=True)
class Partial:
    ratio: float
    amplitude: float
    decay: float
    # Hz between the two modes of the pair. Measured from the ripple on the
    # decay envelope: 2.8 Hz on one bell, 5.6 Hz on two others.
    beat_hz: float


PARTIALS = [
    Partial(ratio=1.000, amplitude=0.08, decay=1.00, beat_hz=1.7),
    Partial(ratio=2.700, amplitude=1.00, decay=1.00, beat_hz=4.5),
    Partial(ratio=4.930, amplitude=0.12, decay=0.19, beat_hz=6.5),
    Partial(ratio=7.670, amplitude=0.02, decay=0.06, beat_hz=8.0),
]

# The partial that carries the pitch, as an index into PARTIALS.
DOMINANT = 1

# Quality factor of the pitch partial: tau = Q / (pi * f).
#
# This is what makes one size knob physical. A bigger bell of the same shape
# sounds lower and rings longer, in proportion, and a constant Q is exactly
# that relationship. Measured 20300 on the traditional inkin (3245 Hz ringing
# with tau 1.99 s); the same constant puts a keisu at tau 12 s, which is the
# half-minute of ring a real one has.
DOMINANT_Q = 20300

# The striker laid on the bowl rather than lifted away. `DAMPED_AFTER` is how
# long the bell is allowed to ring before the hand arrives, and `DAMPED_TAU`
# how fast it dies once it does. Together they put the strike 20 dB down in
# 0.17 s, at the slow end of the 0.05-0.17 s measured on real damped strikes
# and against 2.5-4.7 s for one left to ring.
DAMPED_AFTER = 0.06
DAMPED_TAU = 0.05

SAMPLE_RATE = 32000

# The longest ring worth rendering. A keisu at the largest size decays over
# 26 seconds, and the 2.5 of those a full ring wants is more than a minute of
# audio for a bell that is 20 dB down before half of it has played.
MAX_TAIL_SECONDS = 30.0


class Noise:
    """A tiny linear congruential generator.

    The synthesizer needs a repeatable spray of numbers for the mode phases
    and the contact noise. It uses this rather than `random` so that the Dart
    port on the device produces the identical waveform: Python's Mersenne
    Twister and Dart's `Random` do not agree, and a model that cannot be
    compared across the two implementations cannot be kept in step with them.

    Integer arithmetic throughout, masked to 32 bits, so both languages
    compute the same sequence exactly.
    """

    def __init__(self, seed: int) -> None:
        self.state = seed & 0xFFFFFFFF

    def unit(self) -> float:
        """The next value, in [0, 1)."""
        self.state = (self.state * 1664525 + 1013904223) & 0xFFFFFFFF
        return self.state / 4294967296.0

    def signed(self) -> float:
        """The next value, in [-1, 1)."""
        return self.unit() * 2.0 - 1.0


def contact_seed(contact: float) -> int:
    """The noise seed a strike at `contact` uses. Varying the contact point
    varies the phases and the contact noise along with the mode balance, so
    one number is all a caller has to choose to get a different strike."""
    return int(abs(contact) * 1_000_003) & 0xFFFFFFFF


@dataclass(frozen=True)
class Voice:
    """A bell, as the synthesizer sees it."""

    name: str
    # The pitch you hear, in Hz — the frequency of PARTIALS[DOMINANT].
    dominant_hz: float

    @property
    def hum_hz(self) -> float:
        return self.dominant_hz / PARTIALS[DOMINANT].ratio

    @property
    def dominant_tau(self) -> float:
        """Decay time of the pitch partial, from the constant-Q rule."""
        return DOMINANT_Q / (math.pi * self.dominant_hz)

    def scaled(self, size: float) -> "Voice":
        """This bell made larger (`size` > 1) or smaller (`size` < 1).

        Frequency goes inversely with size, and because Q is held constant
        the ring time follows on its own — which is why there is one knob
        here and not two.
        """
        return Voice(name=self.name, dominant_hz=self.dominant_hz / size)


# The default sizes. The inkin's pitch is the traditional inkin measured from
# the monastery-store demonstration; the keisu is set where a large standing
# bowl sits, and inherits its half-minute of ring from the constant Q.
VOICES = [
    Voice(name="inkin", dominant_hz=3245.0),
    Voice(name="keisu", dominant_hz=500.0),
]


@dataclass(frozen=True)
class Strike:
    """One strike within a sequence."""

    at: float                   # seconds from the start of the sequence
    gain: float = 1.0
    # Seconds after this strike at which the bell is damped, or None to let
    # it ring out.
    damp_after: float | None = None
    # Where on the rim the bell was struck, as a phase in turns. It decides
    # the balance within each mode pair, and so how the warble sits. Varying
    # it is what keeps three strikes from sounding like one strike pasted
    # three times.
    contact: float = 0.0


def render_strike(
    voice: Voice,
    length: int,
    contact: float,
    sample_rate: int = SAMPLE_RATE,
) -> list[float]:
    """Renders a single strike of `voice` into a buffer of `length` samples.

    Deterministic: the same voice, length and contact always give the same
    samples. Variation between strikes comes from the caller choosing a
    different `contact`, which is the physical variable anyway — where the
    striker lands on the rim.
    """
    noise = Noise(contact_seed(contact))
    out = [0.0] * length
    nyquist = sample_rate * 0.45
    tau0 = voice.dominant_tau

    for partial in PARTIALS:
        frequency = voice.hum_hz * partial.ratio
        if frequency >= nyquist:
            continue
        tau = tau0 * partial.decay

        # The two modes of the pair, and how the strike divides its energy
        # between them. Where the bell is struck decides which of the pair
        # comes out on top, so the split varies from strike to strike, and
        # the warble varies with it.
        #
        # The weaker of the pair is kept between 15% and 45% of the stronger.
        # That is the 1-2.5 dB of envelope ripple measured on the real bells,
        # and the bound matters in both directions: two modes of equal level
        # beat all the way down to silence, which sounds like a tremolo pedal
        # rather than a bell, and one mode alone sounds dead.
        theta = 2.0 * math.pi * contact * partial.ratio
        weaker = 0.15 + 0.30 * abs(math.cos(theta))
        split = (weaker, 1.0) if math.sin(theta) < 0 else (1.0, weaker)
        norm = math.hypot(*split)

        for mode, weight in enumerate(split):
            detuned = frequency + (mode - 0.5) * partial.beat_hz
            level = partial.amplitude * weight / norm
            # An exponentially decaying sinusoid is a geometric sequence in
            # the complex plane, so it can be generated with one complex
            # multiply per sample instead of a sin() and an exp().
            step = cmath.exp(complex(-1.0 / (tau * sample_rate),
                                     2.0 * math.pi * detuned / sample_rate))
            phasor = cmath.exp(complex(0.0, 2.0 * math.pi * noise.unit()))
            for n in range(length):
                out[n] += level * phasor.imag
                phasor *= step

    # The striker hitting the metal: a very short burst of noise, differenced
    # to tilt it towards the high end where contact noise actually lives.
    noise_tau = 0.006
    noise_length = min(length, int(sample_rate * noise_tau * 8))
    previous = 0.0
    for n in range(noise_length):
        sample = noise.signed()
        out[n] += 0.10 * (sample - previous) * math.exp(-n / (noise_tau * sample_rate))
        previous = sample

    # A couple of milliseconds of attack ramp removes the click that a hard
    # onset would otherwise put at the start.
    attack = int(sample_rate * 0.002)
    for n in range(min(attack, length)):
        out[n] *= n / attack

    return out


def render_sequence(
    voice: Voice,
    strikes: list[Strike],
    tail_seconds: float,
    sample_rate: int = SAMPLE_RATE,
    normalize: bool = True,
) -> list[float]:
    """Renders a sequence of strikes, tails overlapping as on a real bell."""
    last = max(strike.at for strike in strikes)
    total = int((last + tail_seconds) * sample_rate)
    out = [0.0] * total

    for strike in strikes:
        offset = int(strike.at * sample_rate)
        rendered = render_strike(voice, total - offset, strike.contact,
                                 sample_rate)
        for n, value in enumerate(rendered):
            out[offset + n] += strike.gain * value

    # Damping is applied to the mix, and after every strike has been added,
    # because a hand laid on the bowl stops the whole bell — including
    # whatever is still ringing from the strike before. Damping each strike's
    # own buffer instead leaves the previous one sounding on underneath,
    # which is both wrong and, in a two-strike closing where the first strike
    # is the loud one, almost the entire sound.
    for strike in strikes:
        if strike.damp_after is not None:
            _damp(out, int((strike.at + strike.damp_after) * sample_rate),
                  sample_rate)

    # Fade the last half second so the file cannot end on a discontinuity.
    fade = int(sample_rate * 0.5)
    for n in range(min(fade, total)):
        out[total - fade + n] *= 1.0 - n / fade

    if normalize:
        peak = max(abs(value) for value in out)
        if peak > 0:
            scale = 0.89 / peak  # Leave a little headroom below full scale.
            out = [value * scale for value in out]
    return out


def _damp(samples: list[float], from_sample: int, sample_rate: int) -> None:
    """Lays the striker on the bowl: everything after `from_sample` dies fast.

    In place, and applied to the whole mix rather than to one strike, because
    the hand stops whatever the bell happens to be doing.
    """
    decay = math.exp(-1.0 / (DAMPED_TAU * sample_rate))
    gain = 1.0
    for n in range(from_sample, len(samples)):
        samples[n] *= gain
        gain *= decay


def opening(voice: Voice, interval: float, rng: random.Random) -> list[Strike]:
    """Three strikes, ringing out. This opens a period of zazen."""
    return [
        Strike(at=i * interval,
               gain=gain,
               contact=rng.uniform(0.0, 1.0))
        for i, gain in enumerate((0.94, 1.0, 0.90))
    ]


def closing(voice: Voice, interval: float, rng: random.Random) -> list[Strike]:
    """Two strikes, the second damped. This closes a period of zazen.

    The striker is laid on the bowl straight after the second strike rather
    than lifted away, so the ring is stopped rather than allowed to fade.
    It is how a period ends in a zendo, and it is unmistakable: the sitting
    is over, not fading out.
    """
    return [
        Strike(at=0.0, gain=1.0, contact=rng.uniform(0.0, 1.0)),
        Strike(at=interval, gain=0.95, damp_after=DAMPED_AFTER,
               contact=rng.uniform(0.0, 1.0)),
    ]


def sequence_seconds(voice: Voice, strikes: list[Strike]) -> float:
    """How long a sequence takes, from the first strike to silence.

    A damped ending changes this completely, which is the reason it is worth
    computing rather than assuming: a keisu left to ring needs half a minute
    of tail, and the same keisu stopped by the hand needs half a second. The
    app uses this to know when a sitting is actually over, and the notification
    asset needs it to stay under the 30 seconds iOS will accept.
    """
    last_strike = max(strike.at for strike in strikes)
    dampings = [strike.at + strike.damp_after for strike in strikes
                if strike.damp_after is not None]
    # A damping only ends the sequence if nothing is struck after it.
    final = [d for d in dampings if d >= last_strike]
    if final:
        # Eight time constants is 70 dB down, which is silence.
        return max(final) + DAMPED_TAU * 8
    return last_strike + min(voice.dominant_tau * 2.5, MAX_TAIL_SECONDS)


def strike_interval(voice: Voice) -> float:
    """Seconds between strikes.

    The recordings sit at 1.4-1.8 s, but those are demonstrations rather than
    zazen. Struck this close together the three strikes run into one gesture;
    at 2.4 s each is given room to be its own, which is what opening a period
    of sitting wants. A bigger bell is given longer still, because the ring it
    is leaving room for is longer.
    """
    return 2.4 * max(1.0, (3245.0 / voice.dominant_hz) ** 0.5)


def write_wav(path: str, sample_rate: int, samples: list[float]) -> None:
    frames = bytearray()
    for value in samples:
        clamped = max(-1.0, min(1.0, value))
        frames += struct.pack("<h", int(clamped * 32767.0))
    with wave.open(path, "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(sample_rate)
        handle.writeframes(bytes(frames))
    print(f"{path}: {len(samples) / sample_rate:.1f}s, {len(frames) / 1024:.0f} KiB")


def write_keepalive(path: str) -> None:
    """Writes the near-silent loop that holds the audio session open.

    It is not digital silence: a handful of platforms optimise away buffers
    that are entirely zero, and the whole point of this file is to keep the
    audio pipeline demonstrably busy. At -84 dBFS and 40 Hz it is inaudible.
    """
    sample_rate = 8000
    length = sample_rate * 10
    amplitude = 2.0 / 32767.0
    samples = [
        amplitude * math.sin(2.0 * math.pi * 40.0 * n / sample_rate) for n in range(length)
    ]
    write_wav(path, sample_rate, samples)


# Where each bell has to be copied to besides `assets/audio`.
#
# A notification sound has to be a platform resource — Flutter assets are not
# visible to the notification system — so the same file is needed in three
# places. This used to be a step in the README that someone had to remember,
# which is a thing that goes wrong quietly: the app rings one bell and the
# notification rings a stale one.
PLATFORM_DESTINATIONS = [
    os.path.join("android", "app", "src", "main", "res", "raw"),
    os.path.join("ios", "Runner"),
]


def main() -> None:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    directory = os.path.join(root, "assets", "audio")
    os.makedirs(directory, exist_ok=True)

    # The notification bell: what the operating system plays if Jikido has
    # been killed and the app is not there to ring the closing bell itself.
    # It is the only bell that still has to be a file, and it is always the
    # default size — the app cannot hand a customized bell to the OS, and
    # this is a backstop for a case the user is not there for anyway.
    for voice in VOICES:
        rng = random.Random(20250809)  # Reproducible output.
        interval = strike_interval(voice)
        strikes = closing(voice, interval, rng)
        samples = render_sequence(
            voice,
            strikes,
            tail_seconds=sequence_seconds(voice, strikes) - max(s.at for s in strikes),
        )
        path = os.path.join(directory, f"{voice.name}.wav")
        write_wav(path, SAMPLE_RATE, samples)
        for destination in PLATFORM_DESTINATIONS:
            target = os.path.join(root, destination)
            os.makedirs(target, exist_ok=True)
            shutil.copyfile(path, os.path.join(target, f"{voice.name}.wav"))
            print(f"  -> {os.path.join(destination, voice.name)}.wav")

    # Not copied anywhere: the keep-alive loop is played by the app, never by
    # the notification system.
    write_keepalive(os.path.join(directory, "keepalive.wav"))


if __name__ == "__main__":
    main()
