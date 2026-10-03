#!/usr/bin/env python3
"""Render AVL Dungeon's original locked-gate effect using only the stdlib.

Two short attempts against a stuck latch: dry body impacts, inharmonic metal
resonances, and irregular smaller rebounds. This is not a pitched error tone.
Only gate-locked.wav is written; other assets stay as-is.
"""

import math
from pathlib import Path
import random
import struct
import wave


ASSETS = Path(__file__).resolve().parents[1] / "samples/assets/audio/dungeon"
NAME = "gate-locked.wav"
RATE = 44100
DURATION = 0.46
PEAK = 0.23


def synthesize():
    """Return deterministic mono PCM16 samples, with silent endpoints."""
    rng = random.Random(20261003)
    samples = [0.0] * round(DURATION * RATE)
    # The two main pulls each end in several softer, uneven latch rebounds.
    strikes = [(0.010, 1.0), (0.045, 0.23), (0.071, 0.13),
               (0.172, 0.90), (0.202, 0.29), (0.226, 0.18),
               (0.253, 0.10)]
    modes = [(617, 0.26, 0.030), (1031, 0.20, 0.048),
             (1789, 0.13, 0.023), (2861, 0.07, 0.016)]
    for onset, level in strikes:
        start = round(onset * RATE)
        low_noise = 0.0
        for offset in range(min(round(0.18 * RATE), len(samples) - start)):
            t = offset / RATE
            noise = rng.uniform(-1.0, 1.0)
            low_noise += 0.18 * (noise - low_noise)
            attack = min(1.0, t / 0.0009)
            release = min(1.0, (0.18 - t) / 0.015)
            # A low door-body thud plus a bright, fast metal contact.
            body = 0.62 * math.sin(2 * math.pi * 116 * t) * math.exp(-t / 0.017)
            contact = (0.40 * noise + 0.35 * low_noise) * math.exp(-t / 0.006)
            ring = sum(weight * math.sin(2 * math.pi * freq * t)
                       * math.exp(-t / decay) for freq, weight, decay in modes)
            samples[start + offset] += level * attack * release * (body + contact + ring)

    # Remove subsonic drift while retaining the low door-body impact.
    alpha = math.exp(-2 * math.pi * 30 / RATE)
    previous_input = previous_output = 0.0
    for i, value in enumerate(samples):
        filtered = alpha * (previous_output + value - previous_input)
        samples[i] = filtered
        previous_input, previous_output = value, filtered
    # Smooth global tail into 10 ms of silence, with no abrupt clip or DC step.
    for i in range(len(samples)):
        remaining = (len(samples) - 1 - i) / RATE
        samples[i] *= min(1.0, max(0.0, (remaining - 0.010) / 0.020))
    scale = PEAK * 32767 / max(abs(value) for value in samples)
    pcm = [round(value * scale) for value in samples]
    assert pcm[0] == pcm[-1] == 0
    assert max(abs(value) for value in pcm) / 32768 <= PEAK
    return pcm


def generate():
    pcm = synthesize()
    path = ASSETS / NAME
    with wave.open(str(path), "wb") as stream:
        stream.setparams((1, 2, RATE, len(pcm), "NONE", "not compressed"))
        stream.writeframes(struct.pack(f"<{len(pcm)}h", *pcm))
    print(f"Generated {path}")


if __name__ == "__main__":
    generate()
