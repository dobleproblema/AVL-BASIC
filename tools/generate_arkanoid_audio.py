#!/usr/bin/env python3
"""Regenerate the original Arkanoid WAV effects using only Python's stdlib.

Python is needed only to regenerate assets. The game plays the included WAVs
directly through AVL-BASIC's AUDIO commands, with no external audio tools.
"""

from array import array
import math
from pathlib import Path
import random
import sys
import wave


RATE = 44100
TAU = 2 * math.pi
OUTPUT = Path(__file__).resolve().parents[1] / "samples/assets/audio/arkanoid"


def envelope(t, duration, decay, attack=0.002):
    """Short smooth attack and a tail that reaches zero, without hard clicks."""
    fade = min(1.0, max(0.0, (duration - t) / 0.025))
    return min(1.0, t / attack) * math.exp(-t / decay) * fade * fade


def impact(duration, frequency, bend, decay, brightness, noise, seed):
    rng = random.Random(seed)
    result = []
    for i in range(round(duration * RATE)):
        t = i / RATE
        # Exponentially falling pitch lends the mallet a little elasticity.
        phase = TAU * (frequency * t + bend * 0.012 * (1 - math.exp(-t / 0.012)))
        tone = math.sin(phase) + brightness * math.sin(2.76 * phase) * math.exp(-t / 0.02)
        transient = noise * rng.uniform(-1, 1) * math.exp(-t / 0.008)
        result.append((tone + transient) * envelope(t, duration, decay))
    return result


def chime(notes, duration):
    """Layer soft sine partials; notes are (onset, frequency, length, gain)."""
    result = [0.0] * round(duration * RATE)
    for onset, frequency, length, gain in notes:
        start = round(onset * RATE)
        for i in range(min(round(length * RATE), len(result) - start)):
            t = i / RATE
            tone = math.sin(TAU * frequency * t)
            tone += 0.23 * math.sin(TAU * frequency * 2 * t) * math.exp(-t / 0.12)
            tone += 0.08 * math.sin(TAU * frequency * 3 * t) * math.exp(-t / 0.06)
            result[start + i] += gain * tone * envelope(t, length, length / 4, 0.006)
    return result


def falling_ball():
    duration = 0.42
    result = []
    phase = 0.0
    for i in range(round(duration * RATE)):
        t = i / RATE
        frequency = 110 + 470 * math.exp(-t / 0.1)
        phase += TAU * frequency / RATE
        tone = math.sin(phase) + 0.2 * math.sin(2 * phase)
        result.append(tone * envelope(t, duration, 0.16, 0.004))
    return result


def write_wav(name, samples, peak):
    scale = peak / max(abs(value) for value in samples)
    pcm = array("h", (round(value * scale * 32767) for value in samples))
    if sys.byteorder != "little":
        pcm.byteswap()
    path = OUTPUT / (name + ".wav")
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(RATE)
        wav.writeframes(pcm.tobytes())
    print(f"{path.name}: {len(samples) / RATE:.3f} s, {path.stat().st_size} bytes")


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    write_wav("wall", impact(0.075, 430, 120, 0.017, 0.12, 0.16, 1), 0.45)
    write_wav("paddle", impact(0.12, 680, 280, 0.027, 0.25, 0.09, 2), 0.6)
    write_wav("brick", impact(0.18, 960, 90, 0.038, 0.5, 0.32, 3), 0.58)
    write_wav("start", chime([
        (0.00, 523.25, 0.24, 0.7),
        (0.10, 659.25, 0.28, 0.8),
        (0.20, 783.99, 0.40, 1.0),
    ], 0.62), 0.64)
    write_wav("life", falling_ball(), 0.58)
    write_wav("game-over", chime([
        (0.00, 392.00, 0.30, 0.8),
        (0.18, 311.13, 0.32, 0.8),
        (0.36, 261.63, 0.56, 1.0),
        (0.36, 130.81, 0.56, 0.4),
    ], 0.95), 0.64)
    write_wav("win", chime([
        (0.00, 523.25, 0.35, 0.7),
        (0.12, 659.25, 0.35, 0.7),
        (0.24, 783.99, 0.38, 0.8),
        (0.40, 1046.5, 0.75, 1.0),
        (0.40, 659.25, 0.75, 0.35),
        (0.40, 783.99, 0.75, 0.35),
    ], 1.18), 0.67)


if __name__ == "__main__":
    main()
