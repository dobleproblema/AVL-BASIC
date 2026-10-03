"""Freeze a portable Windows CPU comparison without rebuilding AVL-BASIC."""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from datetime import datetime, timezone
from pathlib import Path

import representative


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def zoomer(frames: int) -> tuple[str, str]:
    source = representative.ROOT / "samples/g-zoomer.bas"
    lines = {}
    for line in source.read_text(encoding="utf-8-sig").splitlines():
        if line.strip():
            number, statement = line.split(maxsplit=1)
            lines[int(number)] = statement
    expected = {150: "S=2", 400: "T=TIME-T0", 720: "FCOUNT=FCOUNT+1",
                800: 'GPRINT "AVL-BASIC ZOOMER', 810: "FRAME", 830: "GOTO 400"}
    for number, text in expected.items():
        if text not in lines.get(number, ""):
            raise ValueError(f"Zoomer source changed at line {number}; review adaptation")
    lines[1] = "BENCHSTART=TIME"
    lines[400] = "T=FCOUNT/60"
    for number in (730, 740, 750, 760, 770):
        del lines[number]
    lines[800] = 'GPRINT "AVL-BASIC ZOOMER-ROTATOR  FRAME: ";FCOUNT'
    lines[820] = f"IF FCOUNT<{frames} THEN 400"
    lines[830] = 'PRINT "__BENCH_SECONDS__";TIME-BENCHSTART'
    lines[840] = 'BSAVE "frame.png"'
    lines[850] = "END"
    return "\n".join(f"{n} {s}" for n, s in sorted(lines.items())) + "\n", sha256(source)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    shutil.copy2(args.executable, output / "avl-basic.exe")
    config = json.loads(representative.CONFIG.read_text(encoding="utf-8"))
    cases = [
        ("pi-5000", "pimachin", {"digits": 5000}),
        ("pi-10000", "pimachin", {"digits": 10000}),
        ("pi-20000", "pimachin", {"digits": 20000}),
        ("jelly", "jelly", {"frames": 960}),
        ("zoomer", "zoomer", {"frames": 240, "width": 640, "height": 480, "S": 2}),
        ("raytracer", "raytracer", {}),
    ]
    workloads = []
    for name, generator, overrides in cases:
        spec = {**config.get(generator, {}), **overrides}
        if generator == "zoomer":
            program, source_hash = zoomer(spec["frames"])
        else:
            program, source_hash = representative.generate(generator, spec)
        directory = output / "programs" / name
        directory.mkdir(parents=True)
        path = directory / "program.bas"
        path.write_text(program, encoding="utf-8", newline="\n")
        files = []
        if generator == "zoomer":
            (directory / "assets").mkdir()
            image = directory / "assets/anime.png"
            shutil.copy2(representative.ROOT / "samples/assets/anime.png", image)
            files.append({"path": "assets/anime.png", "sha256": sha256(image)})
        workloads.append({
            "name": name, "program": path.relative_to(output).as_posix(),
            "sha256": sha256(path), "source_sha256": source_hash,
            "graphics": generator in ("jelly", "zoomer", "raytracer"),
            "frames": spec.get("frames"), "timeout_seconds": 120,
            "files": files, "spec": spec,
        })
    manifest = {
        "schema_version": 1, "created_utc": datetime.now(timezone.utc).isoformat(),
        "executable_sha256": sha256(output / "avl-basic.exe"),
        "workloads": workloads,
        "notes": [
            "Exact same executable and generated programs on both machines; no rebuild.",
            "Primary metric is monotonic process wall time; BASIC TIME is diagnostic.",
            "Includes process startup, loading, stdout, and final PNG encoding.",
            "Jelly and Zoomer follow fixed frame sequences independent of real time.",
            "Pi size sweep is an interpreter workload, not an isolated cache latency test.",
        ],
    }
    (output / "workloads.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "executable_sha256": manifest["executable_sha256"],
                      "workloads": [w["name"] for w in workloads]}, indent=2))


if __name__ == "__main__":
    main()
