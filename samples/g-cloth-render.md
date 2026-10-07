# Offline cloth rendering

[g-cloth-render.bas](g-cloth-render.bas) simulates cloth and renders satin lighting
and filtered shadows entirely in AVL BASIC, using `GTRIANGLE` to draw triangles
with interpolated colors and depth. By default it shows a progressive preview
and writes no PNG files or physical checkpoints. PNG export is optional. The
companion [g-animation3.bas](g-animation3.bas) preloads the supplied PNGs and plays
them at 30 FPS; the expensive physics and rendering have already happened.

The supplied clip contains 120 frames at 800 × 600: a four-second sheet of teal
cloth falling over a fixed sphere. Playback stops on the last frame. SPACE
pauses, R replays, C toggles the presentation cap, and ESC exits. A clock keeps
the motion at its recorded speed when the cap is off.

The player uses `SCREEN` to restore each image in the drawing buffer,
adds the HUD, then presents both together with `FRAME`. This avoids displaying
an intermediate image without text.

## Preview without exporting

Run the BASIC program to watch the cloth simulation and the progressive
triangle rendering:

```basic
RUN "/samples/g-cloth-render.bas"
```

The default settings are `SAVEFRAMES=0 : PROGRESS=1` on line 185. No output
directory is required. Each triangle is presented as it is drawn, and a final
`FRAME` presents each completed simulation frame. When the last image is
complete, the preview waits for a key with that image visible. Set
`PROGRESS=0` to present only completed frames; this also leaves file output
disabled while `SAVEFRAMES=0`.

## Generate the default clip

The ready-made PNGs are in `assets/cloth/`. To regenerate them, run the helper
from the Rust repository root with a built interpreter and Pillow installed:

```console
python tools/generate_cloth_animation.py
```

The helper explicitly enables export with `SAVEFRAMES=1 : PROGRESS=0` in a
temporary BASIC copy. It creates output directories, renders without
per-triangle presentation, verifies the complete set of 120 RGB PNGs, and
publishes them with `manifest.json`. Simulation checkpoints and the generation
log stay in a new `target/cloth-offline/generation-.../` directory. The Python
helper coordinates files and validation; the BASIC program performs the
simulation and rendering.

The helper also accepts `--executable`, `--frames`, `--fps` (30 or 60),
`--material` (`teal`, `copper`, or `ivory`), and `--output`. For example:

```console
python tools/generate_cloth_animation.py --frames 120 --fps 30 --material copper
```

Changing the number of frames or the output rate also requires matching `F`
(the last frame index) and `FRATE` in the playback example. Its `MODE` must match
the exported image dimensions.

## Configure the BASIC renderer

Edit the settings at lines 130–190 to configure the preview or export.
Relative paths are resolved beside the BASIC program, in `samples/`.
Output directories need to exist only when exporting.

| Setting | Default | Purpose |
|---|---|---|
| `MODE` | `800` | Image size: 800 × 600 pixels. |
| `NX`, `NY` | `40`, `34` | Cloth cells; the mesh has 41 × 35 particles. |
| `H` | `1/180` | Fixed duration of one physical substep, in seconds. |
| `FPSOUT` | `30` | Simulated frames per second, independent of preview or generation speed. |
| `FIRST`, `LAST` | `0`, `119` | Inclusive frame range, with indices from 0 to 999. |
| `SCENE`, `PINMODE` | `1`, `0` | The draped sheet with no pinned corners. |
| `WIND` | `1` | Wind strength; set it to 0 to remove the applied wind. |
| `MATERIAL` | `0` | Teal satin; 1 selects copper and 2 selects ivory. |
| `WARMUP` | `0` | Physical substeps before frame 0 when starting a fresh simulation. |
| `BX0`, `BY0`, `BZ0`, `BRAD` | `0`, `1.45`, `-.25`, `1.03` | Sphere center and radius. |
| `AZ`, `EL`, `DIST`, `TARGETY` | `.48`, `.45`, `7.2`, `1.6` | Camera azimuth, elevation, distance, and target height. |
| `SAVEFRAMES` | `0` | Preview only; set to 1 to save PNGs and any requested checkpoints. |
| `PROGRESS` | `1` | Present after each triangle; 0 presents only completed frames. |
| `OUT$` | `"assets/cloth/"` | PNG directory/prefix when exporting; files are named `frame-000.png`, etc. |
| `STATEIN$` | `""` | Optional complete physical checkpoint to load. |
| `STATEOUT$` | `""` | Optional directory/prefix for `state-000.csv`, etc.; used only with `SAVEFRAMES=1`. |

## Enable PNG export

Remove `REM` from line 186 to activate its export settings:

```basic
186 SAVEFRAMES=1 : PROGRESS=0
```

This overrides line 185: PNGs are saved, and only completed frames are
presented with `FRAME`. Create the `OUT$` directory before running the program.
The images contain no HUD. You can set `PROGRESS=1` while exporting if you want
to watch individual triangles being drawn; it changes presentation, not the
saved image or simulation.

Set `SAVEFRAMES=0` for a preview with no file writes, even when `STATEOUT$` is
nonempty.

At 30 FPS, each output frame advances six substeps of `1/180` second. At 60 FPS,
it advances three. The program checks that the chosen output rate contains a
whole number of physical substeps. Rendering can take much longer than the
recorded frame duration without changing the simulated motion.

Frames must be simulated in order. `FIRST` selects the first frame to preview
or save; it does not skip the physical work needed to reach that frame.
With no warmup, frame 0 shows the initial state and frame 119 shows time
`119/30` seconds. The 120 images occupy four seconds when played at 30 FPS.

## Reuse physical checkpoints

With `SAVEFRAMES=1`, set `STATEOUT$` to an existing directory to save positions,
velocities, and scene state for every exported frame. For example, enable
export on line 186, create `target/cloth-cache/`, and replace line 190 with:

```basic
190 OUT$="assets/cloth/" : STATEIN$="" : STATEOUT$="/target/cloth-cache/"
```

The leading `/` addresses AVL BASIC's virtual root, which is the repository root
when running there. To render frame 60 again from that checkpoint, set
`FIRST=60 : LAST=60` on line 150 and use:

```basic
190 OUT$="assets/cloth/" : STATEIN$="/target/cloth-cache/state-060.csv" : STATEOUT$=""
```

The checkpoint stores the state **at** frame 60. The renderer draws that frame
directly, without advancing its physics again. Use `SAVEFRAMES=0` to preview it
without writing any files, or `SAVEFRAMES=1` to export it. You can change the
camera or material to render the same geometry differently. To continue the clip, use
`FIRST=60 : LAST=119`; subsequent frames advance normally from the saved state.

Keep `NX`, `NY`, `H`, and `FPSOUT` unchanged when loading a checkpoint; the
renderer validates them. It restores the saved scene, pinning, wind, and sphere
state, while keeping your camera and material settings. Warmup is skipped when
a checkpoint is loaded, and the requested range cannot start before its frame.
The helper's manifest records the location of its local `states/` directory,
whose checkpoints work in the same way.

To preview just one image without a checkpoint, set `FIRST=LAST` to its frame
index. Enable `SAVEFRAMES=1` to save that image as well. The simulation still
advances through all preceding frames before rendering it. A checkpoint avoids
repeating that work.
