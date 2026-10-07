"""Generate the offline cloth clip using the distributed BASIC renderer.

Physics, lighting, shadows and rasterization run in AVL BASIC. This helper
creates output directories, configures a temporary BASIC copy, checks the
result and publishes complete PNGs. Physical checkpoints stay under target.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import uuid

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "samples" / "g-cloth-render.bas"


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release" / ("avl-basic.exe" if os.name == "nt" else "avl-basic"))
    parser.add_argument("--frames", type=int, default=120)
    parser.add_argument("--fps", type=int, choices=(30, 60), default=30)
    parser.add_argument("--material", choices=("teal", "copper", "ivory"), default="teal")
    parser.add_argument("--output", type=Path, default=ROOT / "samples/assets/cloth")
    args = parser.parse_args()
    if not 1 <= args.frames <= 1000:
        parser.error("frames must be between 1 and 1000")
    executable = args.executable.resolve()
    if not executable.is_file():
        parser.error(f"missing interpreter: {executable}")

    stage = ROOT / "target/cloth-offline" / f"generation-{uuid.uuid4().hex[:8]}"
    images = stage / "images"
    states = stage / "states"
    images.mkdir(parents=True)
    states.mkdir()
    lines = {int(line.split(" ", 1)[0]): line.split(" ", 1)[1] for line in SOURCE.read_text(encoding="utf-8").splitlines() if line.strip()}
    lines[150] = lines[150].replace("FPSOUT=30", f"FPSOUT={args.fps}").replace("LAST=119", f"LAST={args.frames - 1}")
    material = ("teal", "copper", "ivory").index(args.material)
    lines[180] = lines[180].replace("MATERIAL=0", f"MATERIAL={material}")
    lines[185] = "SAVEFRAMES=1 : PROGRESS=0"
    lines[186] = "REM Export mode is configured by the helper above."
    image_ref = "/" + images.relative_to(ROOT).as_posix() + "/"
    state_ref = "/" + states.relative_to(ROOT).as_posix() + "/"
    lines[190] = f'OUT$="{image_ref}" : STATEIN$="" : STATEOUT$="{state_ref}"'
    program = stage / "render.bas"
    program.write_text("\n".join(f"{n} {lines[n]}" for n in sorted(lines)) + "\n", encoding="utf-8")

    started = time.perf_counter()
    completed_frames: list[int] = []
    done = False
    with (stage / "render.log").open("w", encoding="utf-8") as log:
        process = subprocess.Popen([str(executable), str(program)], cwd=ROOT, env=dict(os.environ, AVL_BASIC_WINDOW="0"), stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
        try:
            assert process.stdout is not None
            for line in process.stdout:
                log.write(line)
                log.flush()
                match = re.search(r"CLOTH_FRAME\s+(\d+)\s+", line)
                if match:
                    frame = int(match[1])
                    completed_frames.append(frame)
                    if frame % 10 == 0 or frame == args.frames - 1:
                        print(f"Rendered {frame + 1}/{args.frames} frames ({time.perf_counter() - started:.1f} s)", flush=True)
                elif "CLOTH_RENDER_DONE" in line:
                    done = True
                elif "CLOTH_RENDER_BEGIN" not in line:
                    print(line.rstrip(), flush=True)
            returncode = process.wait()
        except BaseException:
            process.terminate()
            process.wait(timeout=10)
            raise
    if returncode or not done or completed_frames != list(range(args.frames)):
        raise RuntimeError(f"Incomplete generation; see {stage / 'render.log'}")

    records = []
    for frame in range(args.frames):
        image = images / f"frame-{frame:03d}.png"
        state = states / f"state-{frame:03d}.csv"
        with Image.open(image) as picture:
            picture.load()
            if picture.size != (800, 600) or picture.mode != "RGB":
                raise RuntimeError(f"Unexpected image format: {image}")
        if not state.is_file():
            raise RuntimeError(f"Missing physical state: {state}")
        records.append({"frame": frame, "time": frame / args.fps, "png": image.name, "png_bytes": image.stat().st_size, "png_sha256": digest(image), "state_sha256": digest(state)})
    elapsed = time.perf_counter() - started
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    for record in records:
        shutil.copyfile(images / record["png"], output / record["png"])
    manifest = {"format_version": 1, "frames": args.frames, "fps": args.fps, "duration_seconds": args.frames / args.fps, "resolution": [800, 600], "mesh": [40, 34], "physical_step": 1 / 180, "substeps_per_frame": 180 // args.fps, "scene": "DRAPE", "material": args.material, "source_sha256": digest(SOURCE), "configured_source_sha256": digest(program), "executable_sha256": digest(executable), "generation_seconds": elapsed, "physical_states": states.relative_to(ROOT).as_posix(), "images": records}
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (stage / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Published {args.frames} frames to {output}", flush=True)
    print(f"Physical states: {states}", flush=True)
    print(f"Generation completed in {elapsed:.1f} s; clip duration {args.frames / args.fps:.1f} s.", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
