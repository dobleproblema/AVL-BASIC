Original sound effects for g-arkanoid.bas
=======================================

wall.wav       Soft boundary tap (75 ms).
paddle.wav     Elastic paddle impact (120 ms).
brick.wav      Bright, percussive brick impact (180 ms).
start.wav      Ascending launch chime (620 ms).
life.wav       Falling pitch when a life is lost (420 ms).
game-over.wav  Descending minor ending (950 ms).
win.wav        Ascending major ending (1180 ms).

All files are mono, 16-bit PCM WAV at 44100 Hz. They are original procedural
effects, with no third-party recordings or borrowed melodies. Regenerate them
with: python tools/generate_arkanoid_audio.py
Python is only a development tool; it is not needed to run the game.

The game loads these files once, uses eight overlapping impact voices, pans
impacts with the ball, and raises the brick pitch for higher rows. Channel 9
plays the start/life/ending cues. Playback never waits in the game loop.
Keep this directory beside the other samples/assets files when copying the
game. If audio output is unavailable or disabled, the game continues silently.

License: MIT. See COPYING in the project root.
