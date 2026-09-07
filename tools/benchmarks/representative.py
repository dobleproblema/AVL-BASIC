"""Compare two release executables on a fixed, finite, deterministic workload suite.

Standard library only. Programs are generated from the named samples; samples
are never modified. The default primary metric is subprocess wall time.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import re
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CONFIG = Path(__file__).with_suffix(".json")
TIMING = re.compile(r"^__BENCH_SECONDS__\s*([0-9.eE+\-]+)\s*$", re.MULTILINE)
GRAPHICS = {"raytracer", "jelly", "mandelbrot"}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def generate(name: str, spec: dict) -> tuple[str, str]:
    source = (ROOT / spec["source"]).read_bytes()
    lines = {}
    for line in source.decode("utf-8-sig").splitlines():
        if line.strip():
            number, statement = line.split(maxsplit=1)
            lines[int(number)] = statement

    def replace(number: int, expected: str, statement: str) -> None:
        if expected not in lines.get(number, ""):
            raise ValueError(f"{name}: source line {number} changed; review workload adaptation")
        lines[number] = statement

    marker = 'PRINT "__BENCH_SECONDS__";TIME-BENCHSTART'
    if name == "raytracer":
        lines[1] = "BENCHSTART=TIME"
        replace(1040, "WID=640", f'WID={spec["width"]} : HEI={spec["height"]}')
        replace(1390, "Render finished", marker)
        lines[1392] = 'BSAVE "frame.png"'
    elif name in ("pimachin", "modern"):
        lines[1] = "BENCHSTART=TIME"
        first, last = (105, 185) if name == "pimachin" else (2510, 2670)
        replace(first, "P=1000", f'P={spec["digits"]}')
        replace(last, '" SEG."', "PRINT : " + marker)
    elif name == "nqueens":
        lines[1] = "BENCHSTART=TIME"
        lines[105] = f'FOR BENCHREPEAT=1 TO {spec["repeats"]}'
        replace(110, "N=16", f'N={spec["size"]}')
        replace(390, "Elapsed time:", "NEXT BENCHREPEAT")
        lines[400] = marker
        lines[410] = "END"
    elif name == "jelly":
        lines[1] = "BENCHSTART=TIME"
        replace(100, "PTS=10000", f'PTS={spec["points"]}')
        replace(210, "fps=t=fc=0", "t=0 : BENCHFRAME=0")
        replace(310, "TIME-t0", "t=t+0.1 : BENCHFRAME=BENCHFRAME+1")
        replace(325, "fps", 'GPRINT "AVL-BASIC JELLY  FRAME: ";BENCHFRAME : INK &H44ccff')
        replace(340, "KEYDOWN", f'IF BENCHFRAME<{spec["frames"]} THEN 220')
        replace(350, "GOTO 220", marker)
        lines[360] = 'BSAVE "frame.png"'
        lines[370] = "END"
    elif name == "mandelbrot":
        lines[1] = "BENCHSTART=TIME"
        replace(115, "W=WIDTH", f'W={spec["width"]} : H={spec["height"]}')
        replace(120, "MaxIter=50", f'ZoomFactor=8 : MaxIter={spec["iterations"]}')
        replace(155, "ON MOUSE", "REM Mouse input disabled for finite rendering")
        replace(310, "Elapsed time:", marker)
        replace(320, "PAUSE", 'BSAVE "frame.png"')
        replace(325, "CLICKED", "END")
    elif name == "strings":
        replace(30, "RECORDS=256", f'RECORDS={spec["records"]} : PASSES={spec["passes"]}')
    elif name == "matrices":
        replace(30, "REPEATS=30000", f'REPEATS={spec["repeats"]}')
    elif name == "powermul":
        lines[1] = "BENCHSTART=TIME"
        replace(1110, "EXPONENT=1000", f'POWERBASE=7 : EXPONENT={spec["exponent"]}')
        replace(1270, '" SEG."', "PRINT : " + marker)
    elif name == "unicode":
        replace(30, "PASSES=10000", f'PASSES={spec["passes"]}')
    else:
        raise ValueError(f"No generator for {name}")
    return "\n".join(f"{n} {s}" for n, s in sorted(lines.items())) + "\n", digest(source)


def run(executable: Path, program: Path, timeout: float, graphics: bool, window: str) -> dict:
    artifact = program.parent / "frame.png"
    if graphics:
        artifact.unlink(missing_ok=True)
    start = time.perf_counter()
    result = subprocess.run(
        [str(executable), str(program)], cwd=program.parent,
        env={**os.environ, "AVL_BASIC_WINDOW": window},
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        timeout=timeout, check=False,
    )
    elapsed = time.perf_counter() - start
    stdout = result.stdout.decode("utf-8", errors="replace").replace("\r\n", "\n")
    stderr = result.stderr.decode("utf-8", errors="replace").replace("\r\n", "\n")
    if result.returncode:
        raise RuntimeError(f"{executable.name} failed ({result.returncode}): {stdout}\n{stderr}")
    markers = TIMING.findall(stdout)
    if len(markers) != 1:
        raise RuntimeError(f"Expected one timing marker in {program.name}: {stdout}")
    normalized = TIMING.sub("__BENCH_SECONDS__<timing>", stdout)
    artifact_hash = digest(artifact.read_bytes()) if graphics else None
    correctness = digest(json.dumps([normalized, stderr, artifact_hash]).encode())
    return {"elapsed_s": elapsed, "body_s": float(markers[0]),
            "stdout": stdout, "stderr": stderr,
            "correctness_sha256": correctness, "frame_sha256": artifact_hash}


def summarize(report: dict, metric: str) -> dict:
    summary = {}
    for name, workload in report["workloads"].items():
        row = {"group": workload["spec"]["group"]}
        for label in report["executables"]:
            samples = [r[metric] for r in report["runs"]
                       if r["workload"] == name and r["label"] == label and not r["warmup"]]
            if samples:
                median = statistics.median(samples)
                row[label] = {"median_s": median, "min_s": min(samples), "max_s": max(samples),
                              "mad_s": statistics.median(abs(s - median) for s in samples),
                              "samples": len(samples)}
        if "baseline" in row and "candidate" in row:
            ratio = row["candidate"]["median_s"] / row["baseline"]["median_s"]
            row["time_reduction_pct"] = 100 * (1 - ratio)
            row["speedup"] = 1 / ratio
        summary[name] = row
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--config", type=Path, default=CONFIG)
    parser.add_argument("--output", type=Path, required=True, help="New directory for programs, PNGs and raw JSON")
    parser.add_argument("--group", choices=("tuning", "validation", "all"), default="all")
    parser.add_argument("--workloads", help="Comma-separated workload names, optionally restricted by --group")
    parser.add_argument("--runs", type=int, default=6, help="Even count gives equal AB/BA order")
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--metric", choices=("elapsed_s", "body_s"), default="elapsed_s")
    parser.add_argument("--window", choices=("0", "1"), default="0",
                        help="0=headless, 1=real graphics window; use body_s for windowed spot checks")
    parser.add_argument("--prepare-only", action="store_true")
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0 or args.timeout <= 0:
        parser.error("runs and timeout must be positive; warmups cannot be negative")
    if not args.prepare_only and not args.baseline:
        parser.error("--baseline is required unless --prepare-only is used")
    config = json.loads(args.config.read_text(encoding="utf-8"))
    requested = set(args.workloads.split(",")) if args.workloads else set(config)
    if unknown := requested - config.keys():
        parser.error(f"Unknown workloads: {sorted(unknown)}")
    selected = {name: spec for name, spec in config.items()
                if name in requested and (args.group == "all" or spec["group"] == args.group)}
    if not selected:
        parser.error("No workloads selected")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {"created_utc": datetime.now(timezone.utc).isoformat(), "platform": platform.platform(),
              "python": sys.version, "metric": args.metric,
              "configuration": {"runs": args.runs, "warmups": args.warmups, "timeout": args.timeout,
                                "AVL_BASIC_WINDOW": args.window, "order": "rotating workloads; alternating AB/BA"},
              "executables": {}, "workloads": {}, "runs": []}
    for label in ("baseline", "candidate"):
        path = getattr(args, label)
        if path:
            path = path.resolve()
            report["executables"][label] = {"path": str(path), "sha256": digest(path.read_bytes())}
    for name, spec in selected.items():
        program_text, source_hash = generate(name, spec)
        directory = output / name
        directory.mkdir()
        program = directory / f"{name}.bas"
        program.write_text(program_text, encoding="utf-8", newline="\n")
        report["workloads"][name] = {"spec": spec, "program": str(program),
                                      "source_sha256": source_hash,
                                      "program_sha256": digest(program.read_bytes())}
    report_path = output / "results.json"

    def save() -> None:
        report["summary"] = summarize(report, args.metric)
        report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    save()
    if args.prepare_only:
        print(f"Prepared {len(selected)} workloads in {output}")
        return 0
    expected = {}
    names = list(selected)
    try:
        for warmup, count in ((True, args.warmups), (False, args.runs)):
            for repetition in range(count):
                order = names[repetition % len(names):] + names[:repetition % len(names)]
                for name in order:
                    labels = list(report["executables"])
                    if (repetition + names.index(name)) % 2:
                        labels.reverse()
                    for label in labels:
                        workload = report["workloads"][name]
                        result = run(Path(report["executables"][label]["path"]),
                                     Path(workload["program"]), args.timeout, name in GRAPHICS, args.window)
                        result.update(workload=name, label=label, repetition=repetition, warmup=warmup)
                        report["runs"].append(result)
                        actual = result["correctness_sha256"]
                        if actual != expected.setdefault(name, actual):
                            raise RuntimeError(f"Correctness mismatch: {name}, {label}, repetition {repetition}")
                        save()
                        phase = "warmup" if warmup else f"{repetition + 1}/{count}"
                        print(f"{name:<12} {label:<9} {phase:<7} wall={result['elapsed_s']:.4f}s body={result['body_s']:.4f}s", flush=True)
    except (RuntimeError, subprocess.TimeoutExpired, OSError) as error:
        report["error"] = str(error)
        save()
        print(str(error), file=sys.stderr)
        return 1
    summary = report["summary"]
    for name, row in summary.items():
        if "time_reduction_pct" in row:
            print(f"{name:<12} reduction={row['time_reduction_pct']:+.2f}% speedup={row['speedup']:.3f}x")
    for group in ("tuning", "validation", "all"):
        ratios = [1 / row["speedup"] for row in summary.values()
                  if "speedup" in row and (group == "all" or row["group"] == group)]
        if ratios:
            ratio = math.exp(statistics.fmean(math.log(r) for r in ratios))
            report.setdefault("aggregate", {})[group] = {"time_reduction_pct": 100 * (1 - ratio), "speedup": 1 / ratio}
            print(f"{group:<12} geometric-mean time reduction={100 * (1 - ratio):+.2f}%")
    save()
    print(f"Raw results: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
