"""Compare Smoke (formerly Ink Reactor) samples with fixed input.

Standard library only. Each process simulates five untimed frames, then the
requested number of measured frames, always at DT=1/30. PNG and CSV exports
happen after the BASIC timer stops; process wall time includes all startup and
export work. External warmup processes are excluded from timing statistics.

The CSV contains U,V,R,G,B for every cell, including the ghost border, in
row-major order using BASIC WRITE's numeric serialization. Equality checks
cover those serialized values, PNG bytes, and stdout with its timing line
removed. They do not claim a comparison of the internal f64 bit patterns.

Example, run from the repository root:
    python tools/benchmarks/ink_reactor.py --baseline old-ink.bas \
        --output target/ink-comparison --grids 64,96,128 --window 0

Use a separate output directory and invocation for windowed measurements.
Use --baseline-executable to compare an interpreter change as well as the sample.
No source sample or executable is modified.
Requested widths override each sample's quality preset; heights retain each
sample's 16:10 or 4:3 formula. Use --allow-differences across aspect ratios.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
import platform
import re
import statistics
import struct
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
GRID_LEVELS = {36: 0, 40: 0, 56: 1, 64: 1, 72: 2, 80: 2,
               84: 3, 96: 3, 116: 4, 128: 4}
INTERNAL_WARMUP = 5
NUMBER = r"[+\-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+\-]?\d+)?"
TIMING = re.compile(rf"__INK_SECONDS__\s+({NUMBER})")
EQUALITY_KEYS = ("semantic_stdout_sha256", "fields_sha256", "frame_sha256")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_json(path: Path, data: object) -> None:
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def parse_source(source: bytes) -> dict[int, str]:
    result: dict[int, str] = {}
    for line in source.decode("utf-8-sig").splitlines():
        if not line.strip():
            continue
        number, statement = line.split(maxsplit=1)
        number = int(number)
        if number in result:
            raise ValueError(f"Duplicate BASIC line {number}")
        result[number] = statement
    return result


def grid_height(lines: dict[int, str], width: int) -> int:
    formula = lines.get(2050, "").split(":", 1)[0].strip()
    match = re.fullmatch(r"NY\s*=\s*NX\s*\*\s*(\d+)\s*\\\s*(\d+)", formula)
    if match is None or tuple(map(int, match.groups())) not in ((5, 8), (3, 4)):
        raise ValueError("Expected NY=NX*5\\8 or NY=NX*3\\4 on source line 2050")
    numerator, denominator = map(int, match.groups())
    return width * numerator // denominator


def generate(source: bytes, grid: int, frames: int, smooth: int | None, scenario: str, hud: int = 0) -> str:
    lines = parse_source(source)
    if grid not in GRID_LEVELS:
        raise ValueError(f"Unsupported grid width {grid}")
    grid_height(lines, grid)

    def replace(number: int, expected: str, statement: str) -> None:
        if expected not in lines.get(number, ""):
            raise ValueError(f"Source line {number} changed; review workload adaptation ({expected!r})")
        lines[number] = statement

    def insert(number: int, statement: str) -> None:
        if number in lines:
            raise ValueError(f"Benchmark line {number} would overwrite source code")
        lines[number] = statement

    # Solver calls remain untouched so an intentionally different integration
    # or projection strategy can be measured with --allow-differences.
    auto = int(scenario == "auto")
    settings = lines[200]
    automatic_only = not re.search(r"\bAUTO\s*=", settings)
    if automatic_only:
        if lines.get(510) != "GOSUB 1000" or 440 in lines:
            raise ValueError("Unrecognized automatic-only sample; review workload adaptation")
        if scenario != "auto":
            raise ValueError("This sample is automatic-only and has no manual mouse scenario")
    overrides = {"LEVEL": GRID_LEVELS[grid], "VORT": 1, "SMOOTH": smooth,
                 "HUD": hud, "FROZEN": 0, "CAP": 0}
    if not automatic_only:
        overrides["AUTO"] = auto
    for name, value in overrides.items():
        if value is None:
            continue
        settings, count = re.subn(rf"\b{name}\s*=[^:]+", f"{name}={value} ", settings)
        if count != 1:
            raise ValueError(f"Expected one {name} setting on source line 200")
    # Each source retains its own solver and pressure-iteration default.
    replace(200, "LEVEL=", settings)
    # The same LEVEL may select a different width in older/newer samples.
    insert(2045, f"NX={grid}")
    insert(290, 'PRINT "__INK_BEGIN__" : BENCHSTART=0 : FRAMES=0')
    replace(310, "DT=", "DT=1/30")
    replace(320, "K$=", 'K$=""')
    replace(330, "KEYDOWN", "REM Deterministic workload; no keyboard exit")
    if scenario == "auto":
        mouse = "MX=-1 : MY=-1 : ML=0 : MR=0"
    else:
        # Continuous curved drag, cycling ink/stir/release and five colors.
        # It approaches all four walls to exercise ghost-cell updates.
        mouse = (
            "MX=320+310*SIN(FRAMES*0.11) : MY=240+195*COS(FRAMES*0.07)"
            " : ML=0 : MR=0 : COLORID=INT(FRAMES/30) MOD 5"
            " : IF FRAMES MOD 60<40 THEN ML=1 ELSE IF FRAMES MOD 60<50 THEN MR=1"
        )
    if not automatic_only:
        replace(440, "MOUSEX", mouse)
    # Remove the HUD's own timers and FPS updates in both variants. There are
    # no timers around individual stages of the measured loop.
    for number, expected in ((480, "TS=TIME"), (580, "SIMMS="), (630, "DRAWMS="), (670, "TIME-TLAST")):
        if automatic_only and number not in lines:
            continue
        replace(number, expected, "REM HUD timing disabled for benchmark")
    replace(610, "CLG OFFSCREEN", "CLG OFFSCREEN")
    replace(650, "FRAME", "FRAME")
    replace(660, "COUNT=COUNT+1", "FRAMES=FRAMES+1")
    insert(665, f"IF FRAMES={INTERNAL_WARMUP} THEN BENCHSTART=TIME")
    replace(680, "GOTO 300", f"IF FRAMES<{frames + INTERNAL_WARMUP} THEN GOTO 300")
    insert(690, 'BENCHSECONDS=TIME-BENCHSTART : PRINT "__INK_SECONDS__ ";BENCHSECONDS')
    insert(700, 'BSAVE "frame.png"')
    insert(710, 'OPEN "fields.csv" FOR OUTPUT AS #1')
    insert(720, "FOR Y=0 TO NY+1 : FOR X=0 TO NX+1 : I=Y*S+X : WRITE #1,U(I),V(I),R(Y,X),G(Y,X),B(Y,X) : NEXT X : NEXT Y")
    insert(730, "CLOSE #1")
    insert(740, 'PRINT "__INK_END__ ";FRAMES')
    insert(790, "SCREEN CLOSE : END")
    return "".join(f"{number} {statement}\n" for number, statement in sorted(lines.items()))


def check_exports(directory: Path, grid: int, height: int | None = None) -> tuple[str, str]:
    frame = (directory / "frame.png").read_bytes()
    if len(frame) < 24 or frame[:8] != b"\x89PNG\r\n\x1a\n" or frame[12:16] != b"IHDR":
        raise ValueError("Missing or invalid PNG snapshot")
    if struct.unpack(">II", frame[16:24]) != (640, 480):
        raise ValueError("Unexpected PNG dimensions")
    fields = (directory / "fields.csv").read_bytes()
    records = list(csv.reader(fields.decode("utf-8-sig").splitlines()))
    expected = (grid + 2) * ((grid * 5 // 8 if height is None else height) + 2)
    if len(records) != expected:
        raise ValueError(f"Expected {expected} field records, received {len(records)}")
    for index, row in enumerate(records):
        if len(row) != 5 or not all(math.isfinite(float(value)) for value in row):
            raise ValueError(f"Invalid or non-finite field record {index}")
    return sha256(fields), sha256(frame)


def run_one(executable: Path, program: str, directory: Path, *, grid: int,
            frames: int, window: int, timeout: float, variant: str,
            phase: str, pair: int) -> dict:
    lines = parse_source(program.encode("utf-8"))
    width = re.fullmatch(r"NX\s*=\s*(\d+)", lines.get(2045, ""))
    if width is None or int(width[1]) != grid:
        raise ValueError("Benchmark line 2045 must set NX to the requested grid width")
    height = grid_height(lines, int(width[1]))
    directory.mkdir()
    program_path = directory / "bench.bas"
    program_bytes = program.encode("utf-8")
    program_path.write_bytes(program_bytes)
    command = [str(executable), str(program_path)]
    env = {**os.environ, "AVL_BASIC_WINDOW": str(window)}
    started = time.perf_counter()
    try:
        process = subprocess.run(command, cwd=directory, env=env, capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        (directory / "stdout.txt").write_bytes(error.stdout or b"")
        (directory / "stderr.txt").write_bytes(error.stderr or b"")
        raise RuntimeError(f"Timed out after {timeout:g}s: {directory}") from error
    process_seconds = time.perf_counter() - started
    (directory / "stdout.txt").write_bytes(process.stdout)
    (directory / "stderr.txt").write_bytes(process.stderr)
    result = {
        "variant": variant, "phase": phase, "pair": pair,
        "directory": str(directory), "grid": grid, "grid_height": height, "frames": frames,
        "returncode": process.returncode, "process_seconds": process_seconds,
        "program_sha256": sha256(program_bytes),
        "stdout_sha256": sha256(process.stdout),
        "stderr_sha256": sha256(process.stderr),
    }
    try:
        if process.returncode != 0:
            raise ValueError(f"Interpreter exited with status {process.returncode}")
        if process.stderr.strip():
            raise ValueError("Unexpected interpreter stderr; inspect stderr.txt")
        stdout = process.stdout.decode("utf-8-sig")
        output_lines = [line.strip() for line in stdout.splitlines() if line.strip()]
        if len(output_lines) != 3 or output_lines[0] != "__INK_BEGIN__":
            raise ValueError("Missing start marker or unexpected interpreter output")
        timing = TIMING.fullmatch(output_lines[1])
        ending = re.fullmatch(r"__INK_END__\s+(\d+)", output_lines[2])
        if timing is None or ending is None or int(ending[1]) != frames + INTERNAL_WARMUP:
            raise ValueError("Invalid timing/completion markers or wrong frame count")
        seconds = float(timing[1])
        if not math.isfinite(seconds) or seconds <= 0 or seconds > process_seconds + 0.1:
            raise ValueError(f"Invalid BASIC loop duration {seconds!r}")
        # Preserve every other byte, including line endings, for the stdout
        # equivalence hash. Only the inherently variable timing line is removed.
        semantic_stdout = b"".join(
            line for line in process.stdout.splitlines(keepends=True)
            if not line.lstrip().startswith(b"__INK_SECONDS__")
        )
        fields_hash, frame_hash = check_exports(directory, grid, height)
        result.update({
            "loop_seconds": seconds, "fps": frames / seconds,
            "milliseconds_per_frame": 1000 * seconds / frames,
            "semantic_stdout_sha256": sha256(semantic_stdout),
            "fields_sha256": fields_hash, "frame_sha256": frame_hash,
            "validated": True,
        })
    except (ValueError, OSError, UnicodeError) as error:
        result.update({"validated": False, "error": str(error)})
        write_json(directory / "result.json", result)
        raise RuntimeError(f"{directory}: {error}") from error
    write_json(directory / "result.json", result)
    return result


def median_mad(values: list[float]) -> dict[str, float]:
    median = statistics.median(values)
    return {"median": median, "mad": statistics.median(abs(value - median) for value in values)}


def positive_int(value: str) -> int:
    result = int(value)
    if result < 1:
        raise argparse.ArgumentTypeError("Must be a positive integer")
    return result


def grids_arg(value: str) -> list[int]:
    try:
        grids = [int(item) for item in value.split(",")]
    except ValueError as error:
        raise argparse.ArgumentTypeError("Use comma-separated grid widths") from error
    if not grids or any(grid not in GRID_LEVELS for grid in grids) or len(set(grids)) != len(grids):
        raise argparse.ArgumentTypeError("Choose distinct widths from " + ",".join(map(str, GRID_LEVELS)))
    return grids


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", type=Path, default=ROOT / "samples/g-smoke.bas")
    parser.add_argument("--executable", type=Path, default=ROOT / "target/release/avl-basic.exe")
    parser.add_argument("--baseline-executable", type=Path, help="Defaults to --executable")
    parser.add_argument("--output", required=True, type=Path, help="New output directory; existing directories are rejected")
    parser.add_argument("--grids", type=grids_arg, default=[64, 96, 128],
                        help="Requested grid widths; each source retains its own height formula")
    parser.add_argument("--frames", type=positive_int, default=120)
    parser.add_argument("--runs", type=positive_int, default=4, help="Measured AB/BA pairs per grid")
    parser.add_argument("--warmups", type=int, default=1, help="Discarded AB/BA pairs per grid")
    parser.add_argument("--window", type=int, choices=(0, 1), default=0)
    parser.add_argument("--smooth", type=int, choices=(0, 1), default=1)
    parser.add_argument("--hud", type=int, choices=(0, 1), default=0, help="Draw the HUD with its FPS timer disabled")
    parser.add_argument("--source-smoothing", action="store_true", help="Keep each source's default SMOOTH setting instead of overriding it with --smooth")
    parser.add_argument("--scenario", choices=("auto", "manual"), default="auto")
    parser.add_argument("--allow-differences", action="store_true", help="Allow different outputs between variants; still require each variant to be deterministic")
    parser.add_argument("--timeout", type=float, default=300, help="Maximum seconds for each process")
    args = parser.parse_args()
    if args.warmups < 0 or not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("warmups must be nonnegative and timeout must be finite and positive")
    sources = {"baseline": args.baseline.resolve(), "candidate": args.candidate.resolve()}
    executable = args.executable.resolve()
    executables = {"baseline": (args.baseline_executable or executable).resolve(), "candidate": executable}
    for path in [*sources.values(), *executables.values()]:
        if not path.is_file():
            parser.error(f"File not found: {path}")
    output = args.output.resolve()
    if output.exists():
        parser.error(f"Output already exists: {output}")
    source_bytes = {name: path.read_bytes() for name, path in sources.items()}
    try:
        programs = {
            (grid, name): generate(data, grid, args.frames, None if args.source_smoothing else args.smooth, args.scenario, args.hud)
            for grid in args.grids for name, data in source_bytes.items()
        }
    except ValueError as error:
        parser.error(str(error))
    output.mkdir(parents=True)
    for name, data in source_bytes.items():
        (output / f"source-{name}.bas").write_bytes(data)
    report = {
        "schema_version": 1, "started_utc": datetime.now(timezone.utc).isoformat(),
        "platform": platform.platform(), "python": platform.python_version(),
        "processor": platform.processor(), "executable": str(executable),
        "executable_sha256": sha256(executable.read_bytes()),
        "executables": {name: {"path": str(path), "sha256": sha256(path.read_bytes())} for name, path in executables.items()},
        "sources": {name: {"path": str(sources[name]), "sha256": sha256(data)} for name, data in source_bytes.items()},
        "settings": {"grids": args.grids, "frames": args.frames, "runs": args.runs,
                     "external_warmup_pairs": args.warmups, "internal_warmup_frames": INTERNAL_WARMUP,
                     "window": args.window, "smooth": "source" if args.source_smoothing else args.smooth,
                     "source_smoothing": {
                         name: int(re.search(r"\bSMOOTH\s*=\s*(\d+)", parse_source(data)[200])[1])
                         for name, data in source_bytes.items()
                     }, "scenario": args.scenario,
                     "fixed_dt": "1/30", "pressure_iterations": {
                         name: int(re.search(r"\bITER\s*=\s*(\d+)", parse_source(data)[200])[1])
                         for name, data in source_bytes.items()
                     }, "vorticity": 1, "hud": args.hud,
                     "cap": 0, "timeout_seconds": args.timeout},
        "allow_differences": args.allow_differences,
        "equality_scope": "BASIC WRITE serialized full fields including ghost cells; PNG bytes; stdout excluding timing line",
        "timing_scope": "BASIC loop excludes initialization, five warmup frames and exports; process wall time includes them",
        "runs": [], "summaries": [], "status": "running",
    }
    report_path = output / "report.json"
    write_json(report_path, report)
    try:
        for grid in args.grids:
            reference: dict | None = None
            variant_references: dict[str, dict] = {}
            measured: dict[str, list[dict]] = {"baseline": [], "candidate": []}
            for phase, pair_count in (("warmup", args.warmups), ("measured", args.runs)):
                for pair in range(pair_count):
                    order = ("baseline", "candidate") if pair % 2 == 0 else ("candidate", "baseline")
                    for position, variant in enumerate(order):
                        directory = output / f"{grid}-{phase}-{pair + 1:02d}-{position + 1}-{variant}"
                        result = run_one(executables[variant], programs[grid, variant], directory,
                                         grid=grid, frames=args.frames, window=args.window,
                                         timeout=args.timeout, variant=variant, phase=phase, pair=pair + 1)
                        report["runs"].append(result)
                        if reference is None:
                            reference = result
                        if variant not in variant_references:
                            variant_references[variant] = result
                        comparison = variant_references[variant] if args.allow_differences else reference
                        mismatches = [key for key in EQUALITY_KEYS if result[key] != comparison[key]]
                        if mismatches:
                            raise ValueError(f"Output mismatch against {comparison['directory']}: {directory}: {', '.join(mismatches)}")
                        if phase == "measured":
                            measured[variant].append(result)
                        write_json(report_path, report)
                        print(f"{grid} {phase} {pair + 1}/{pair_count} {variant}: {result['fps']:.2f} FPS, {result['milliseconds_per_frame']:.3f} ms/frame", flush=True)
            equal = all(variant_references["baseline"][key] == variant_references["candidate"][key] for key in EQUALITY_KEYS)
            summary = {"grid": grid, "all_output_hashes_equal": equal, "deterministic_within_each_variant": True}
            for variant, results in measured.items():
                summary[variant] = {metric: median_mad([result[metric] for result in results])
                                    for metric in ("loop_seconds", "milliseconds_per_frame", "fps", "process_seconds")}
            baseline_seconds = summary["baseline"]["loop_seconds"]["median"]
            candidate_seconds = summary["candidate"]["loop_seconds"]["median"]
            summary["speedup"] = baseline_seconds / candidate_seconds
            summary["loop_time_reduction_percent"] = 100 * (1 - candidate_seconds / baseline_seconds)
            summary["hashes"] = {variant: {key: result[key] for key in EQUALITY_KEYS}
                                 for variant, result in variant_references.items()}
            report["summaries"].append(summary)
            equality = "equal outputs" if equal else "different outputs; each variant deterministic"
            print(f"{grid}: {summary['speedup']:.3f}x speedup; {equality}", flush=True)
            write_json(report_path, report)
        for name, path in executables.items():
            if sha256(path.read_bytes()) != report["executables"][name]["sha256"]:
                raise ValueError(f"Executable changed during the benchmark: {path}")
        for name, path in sources.items():
            if sha256(path.read_bytes()) != report["sources"][name]["sha256"]:
                raise ValueError(f"Source changed during the benchmark: {path}")
        report["status"] = "passed"
        report["finished_utc"] = datetime.now(timezone.utc).isoformat()
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        report["status"] = "failed"
        report["error"] = str(error)
        write_json(report_path, report)
        print(f"Benchmark failed: {error}\nEvidence: {report_path}", file=sys.stderr)
        return 1
    write_json(report_path, report)
    print(f"Report: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
