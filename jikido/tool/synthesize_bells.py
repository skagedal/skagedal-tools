#!/usr/bin/env python3
"""Synthesizes the bell assets in `assets/audio/`.

Jikido ships generated bell sounds rather than recordings, so that the audio
is redistributable and reproducible. A struck bowl bell (inkin, keisu) is
modelled well by a sum of exponentially decaying inharmonic partials, plus a
short noise transient for the mallet contact. Each partial is rendered twice,
slightly detuned, which gives the slow beating that makes a real bell sound
alive rather than like an organ pipe.

Run from the `jikido/` directory:

    python3 tool/synthesize_bells.py

Only the standard library is used, so there is nothing to install.
"""

from __future__ import annotations

import cmath
import math
import os
import random
import struct
import wave
from dataclasses import dataclass, field

# Frequency ratios of the lowest modes of a thin-walled bowl bell, and how
# loud each one is relative to the fundamental. These are inharmonic — that
# inharmonicity is what makes a bell a bell rather than a note.
PARTIAL_RATIOS = [1.00, 2.71, 5.18, 8.52, 12.50, 17.20]
PARTIAL_AMPLITUDES = [1.00, 0.62, 0.34, 0.20, 0.11, 0.055]


@dataclass
class Bell:
    """The parameters of one synthesized bell."""

    name: str
    sample_rate: int
    fundamental_hz: float
    # Decay time constant of the fundamental, in seconds. Higher partials
    # decay faster; see `partial_decay`.
    decay_seconds: float
    # Seconds between the start of one strike and the start of the next.
    strike_interval: float
    # Seconds of ring left after the final strike.
    tail_seconds: float
    strike_count: int = 3
    # Slight per-strike level variation, so three strikes sound struck by a
    # person rather than pasted from a clipboard.
    strike_gains: list[float] = field(default_factory=lambda: [0.94, 1.0, 0.90])


BELLS = [
    # Inkin: a small hand bell on a stick, bright and quick to fade. This is
    # the bell most commonly used to open and close a period of zazen.
    Bell(
        name="inkin",
        sample_rate=32000,
        fundamental_hz=1318.5,
        decay_seconds=3.2,
        strike_interval=2.4,
        tail_seconds=4.0,
    ),
    # Keisu: the large standing bowl gong. Much lower, and it rings for a
    # long time, so the strikes are spaced further apart.
    Bell(
        name="keisu",
        sample_rate=32000,
        fundamental_hz=196.0,
        decay_seconds=11.0,
        strike_interval=4.0,
        tail_seconds=7.0,
    ),
]


def partial_decay(bell: Bell, ratio: float) -> float:
    """Decay time of a partial. Higher modes radiate energy faster."""
    return bell.decay_seconds / ratio**0.55


def render_strike(bell: Bell, length: int) -> list[float]:
    """Renders a single strike of `bell` into a buffer of `length` samples."""
    out = [0.0] * length
    nyquist = bell.sample_rate * 0.45

    for index, (ratio, amplitude) in enumerate(zip(PARTIAL_RATIOS, PARTIAL_AMPLITUDES)):
        frequency = bell.fundamental_hz * ratio
        if frequency >= nyquist:
            continue
        tau = partial_decay(bell, ratio)
        # The detuned twin that produces the beating. A fixed offset in Hz
        # (rather than in cents) keeps the beat rate audible on the low
        # partials, which is where the ear notices it.
        # Keeping the twin quiet matters: two partials of equal level beat
        # all the way down to silence, which sounds like a tremolo pedal
        # rather than a bell. At 0.85/0.15 the modulation is about 3 dB.
        beat_hz = 0.6 + 0.15 * index
        for detuned, gain in ((frequency, 0.85), (frequency + beat_hz, 0.15)):
            # An exponentially decaying sinusoid is a geometric sequence in
            # the complex plane, so it can be generated with one complex
            # multiply per sample instead of a sin() and an exp().
            step = cmath.exp(complex(-1.0 / (tau * bell.sample_rate),
                                     2.0 * math.pi * detuned / bell.sample_rate))
            phasor = cmath.exp(complex(0.0, random.uniform(0.0, 2.0 * math.pi)))
            level = amplitude * gain
            for n in range(length):
                out[n] += level * phasor.imag
                phasor *= step

    # The mallet hitting the metal: a very short burst of noise, differenced
    # to tilt it towards the high end where the contact noise actually lives.
    noise_tau = 0.006
    noise_length = min(length, int(bell.sample_rate * noise_tau * 8))
    previous = 0.0
    for n in range(noise_length):
        sample = random.uniform(-1.0, 1.0)
        out[n] += 0.10 * (sample - previous) * math.exp(-n / (noise_tau * bell.sample_rate))
        previous = sample

    # A couple of milliseconds of attack ramp removes the click that a hard
    # onset would otherwise put at the start of the file.
    attack = int(bell.sample_rate * 0.002)
    for n in range(min(attack, length)):
        out[n] *= n / attack

    return out


def render(bell: Bell) -> list[float]:
    """Renders the full three-strike sequence, tails overlapping."""
    interval_samples = int(bell.strike_interval * bell.sample_rate)
    total = interval_samples * (bell.strike_count - 1) + int(
        (bell.decay_seconds + bell.tail_seconds) * bell.sample_rate
    )

    # Each strike rings until it is inaudible; strikes overlap by design,
    # which is exactly how three strikes on a real bell sound.
    strike_length = total
    out = [0.0] * total
    for strike in range(bell.strike_count):
        offset = strike * interval_samples
        gain = bell.strike_gains[strike % len(bell.strike_gains)]
        rendered = render_strike(bell, strike_length - offset)
        for n, value in enumerate(rendered):
            out[offset + n] += gain * value

    # Fade the last half second so the file cannot end on a discontinuity.
    fade = int(bell.sample_rate * 0.5)
    for n in range(fade):
        out[total - fade + n] *= 1.0 - n / fade

    peak = max(abs(value) for value in out)
    scale = 0.89 / peak  # Leave a little headroom below full scale.
    return [value * scale for value in out]


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


def main() -> None:
    random.seed(20250809)  # Reproducible output.
    directory = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                             "assets", "audio")
    os.makedirs(directory, exist_ok=True)
    for bell in BELLS:
        write_wav(os.path.join(directory, f"{bell.name}.wav"), bell.sample_rate, render(bell))
    write_keepalive(os.path.join(directory, "keepalive.wav"))


if __name__ == "__main__":
    main()
