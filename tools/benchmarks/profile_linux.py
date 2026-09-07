"""Profile the fixed representative suite with Linux perf; standard library only.

Run under Linux/WSL. Samples describe CPU time in user space, not elapsed time.
Profiled durations are diagnostic and must not be used as speed comparisons.
"""
from __future__ import annotations

import argparse
import collections
import gzip
import json
import os
import platform
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import representative


HEADER = re.compile(r"^\s*(\d+\.\d+):\s+(\d+)\s+(\S+):\s*(.*)$")
FRAME = re.compile(r"^\s*((?:0x)?[0-9a-fA-F]+)\s+(.+)\s+\((.+)\)\s*$")
OFFSET = re.compile(r"\+0x[0-9a-fA-F]+$")


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def perf_output(command: list[str], output: Path, timeout: float, env: dict) -> str:
    result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, timeout=timeout, env=env, check=False)
    stdout = result.stdout.decode("utf-8", errors="replace")
    output.write_text(stdout, encoding="utf-8")
    output.with_suffix(output.suffix + ".stderr.txt").write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(f"Command failed ({result.returncode}): {command!r}; see {output}")
    return stdout


def parse_report(text: str, children: bool = False) -> list[dict]:
    """Parse only data rows from the saved, explicitly delimited perf report."""
    rows = []
    for line in text.splitlines():
        columns = [column.strip() for column in line.strip().split("|", 5 if children else 4)]
        if len(columns) != (6 if children else 5) or not columns[0].endswith("%"):
            continue
        try:
            if children:
                inclusive, own, period, samples, dso, symbol = columns
            else:
                own, period, samples, dso, symbol = columns
            row = {"self_period_pct": float(own.strip().rstrip("%")),
                   "period": int(period.strip()), "self_samples": int(samples.strip()),
                   "dso": dso.strip(), "symbol": symbol.strip()}
            if children:
                row["inclusive_period_pct"] = float(inclusive.strip().rstrip("%"))
        except ValueError as error:
            raise RuntimeError(f"Unexpected perf report data row: {line}") from error
        rows.append(row)
    if not rows:
        raise RuntimeError("No symbol rows parsed from perf report; inspect saved raw report")
    return rows


def parse_stacks(text: str) -> dict:
    """Aggregate observed stacks, leaf first, counting recursion once inclusively.

    perf script fields are fixed to time,period,event,ip,sym,dso. Raw text is
    retained separately so parser assumptions remain auditable.
    """
    events = []
    current = None
    unparsed = []
    for line in text.splitlines():
        if not line.strip():
            continue
        match = HEADER.match(line)
        if match:
            current = {"time_s": match[1], "period": int(match[2]),
                       "event": match[3], "frames": []}
            events.append(current)
            line = match[4]
            if not line:
                continue
        frame = FRAME.match(line)
        if frame and current is not None:
            current["frames"].append({"ip": frame[1],
                                      "symbol": OFFSET.sub("", frame[2].strip()),
                                      "dso": frame[3]})
        elif line.strip() and not line.lstrip().startswith("#"):
            unparsed.append(line)
    if not events:
        raise RuntimeError("No samples parsed from perf script; inspect stacks.txt.gz")
    leaf = collections.Counter()
    inclusive = collections.Counter()
    leaf_period = collections.Counter()
    inclusive_period = collections.Counter()
    stacks = collections.Counter()
    stack_periods = collections.Counter()
    missing = 0
    for event in events:
        frames = event["frames"]
        if not frames:
            missing += 1
            continue
        keys = tuple((frame["dso"], frame["symbol"]) for frame in frames)
        leaf[keys[0]] += 1
        leaf_period[keys[0]] += event["period"]
        for key in set(keys):
            inclusive[key] += 1
            inclusive_period[key] += event["period"]
        stacks[keys] += 1
        stack_periods[keys] += event["period"]
    total = len(events)
    total_period = sum(event["period"] for event in events)

    def rows(counts: collections.Counter, periods: collections.Counter) -> list[dict]:
        return [{"dso": key[0], "symbol": key[1], "samples": counts[key],
                 "sample_pct": 100 * counts[key] / total,
                 "period": periods[key],
                 "period_pct": 100 * periods[key] / total_period if total_period else None}
                for key in sorted(counts, key=lambda key: periods[key], reverse=True)]

    return {"samples": total, "total_period": total_period,
            "samples_without_frames": missing, "unparsed_lines": unparsed,
            "first_sample_time_s": events[0]["time_s"],
            "last_sample_time_s": events[-1]["time_s"],
            "leaf": rows(leaf, leaf_period), "inclusive": rows(inclusive, inclusive_period),
            "stack_counts": [{"samples": count, "period": stack_periods[key],
                              "frames_leaf_first": [{"dso": dso, "symbol": symbol}
                                                     for dso, symbol in key]}
                             for key, count in stacks.most_common()]}


def profile_once(args: argparse.Namespace, name: str, program_text: str,
                 directory: Path, expected: str, env: dict) -> dict:
    directory.mkdir()
    program = directory / f"{name}.bas"
    program.write_text(program_text, encoding="utf-8", newline="\n")
    raw = directory / "perf.data"
    stderr = directory / "program.stderr.txt"
    record_command = [str(args.perf), "record", "--no-buildid-cache", "-e", "cpu-clock:u",
                      "-F", str(args.frequency), "--call-graph", f"dwarf,{args.stack_size}",
                      "-o", str(raw), "--", "sh", "-c",
                      'exec "$@" 2>"$PROFILE_PROGRAM_STDERR"', "profile-target",
                      str(args.executable), str(program)]
    start = time.perf_counter()
    result = subprocess.run(record_command, cwd=directory, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            env={**env, "PROFILE_PROGRAM_STDERR": str(stderr)},
                            timeout=args.timeout, check=False)
    elapsed = time.perf_counter() - start
    (directory / "program.stdout.txt").write_bytes(result.stdout)
    (directory / "perf-record.stderr.txt").write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(f"perf record failed ({result.returncode}): {directory}")
    stdout = result.stdout.decode("utf-8", errors="replace").replace("\r\n", "\n")
    program_stderr = stderr.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n")
    markers = representative.TIMING.findall(stdout)
    if len(markers) != 1:
        raise RuntimeError(f"Expected one BASIC timing marker: {directory}")
    frame = directory / "frame.png"
    frame_hash = representative.digest(frame.read_bytes()) if name in representative.GRAPHICS else None
    normalized = representative.TIMING.sub("__BENCH_SECONDS__<timing>", stdout)
    correctness = representative.digest(json.dumps([normalized, program_stderr, frame_hash]).encode())
    if correctness != expected:
        raise RuntimeError(f"Output differs from normal baseline: {directory}")

    commands = {"record": record_command}
    rows = {}
    lost_samples = None
    for label, children in (("self", False), ("inclusive", True)):
        fields = ("overhead_children," if children else "") + "overhead,period,sample,dso,symbol"
        command = [str(args.perf), "report", "-i", str(raw), "--no-inline", "--stdio", "--stdio-color", "never",
                   "--children" if children else "--no-children", "--call-graph", "none",
                   "--percent-limit", "0", "--percent-type", "global-period",
                   "-t", "|", "-F", fields]
        commands[label] = command
        report = perf_output(command, directory / f"report-{label}.txt", args.timeout, env)
        lost_match = re.search(r"^# Total Lost Samples:\s*(\d+)", report, re.MULTILINE)
        if lost_match:
            lost_samples = int(lost_match[1])
        rows[label] = parse_report(report, children)
        write_json(directory / f"report-{label}.json", rows[label])

    command = [str(args.perf), "script", "-i", str(raw), "--no-inline", "--ns", "--demangle",
               "--show-lost-events", "-F", "time,period,event,ip,sym,dso"]
    commands["script"] = command
    result = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, env=env, timeout=args.timeout, check=False)
    (directory / "perf-script.stderr.txt").write_bytes(result.stderr)
    with gzip.open(directory / "stacks.txt.gz", "wb") as stream:
        stream.write(result.stdout)
    if result.returncode:
        raise RuntimeError(f"perf script failed ({result.returncode}): {directory}")
    stack_summary = parse_stacks(result.stdout.decode("utf-8", errors="replace"))
    write_json(directory / "stacks.json", stack_summary)
    write_json(directory / "commands.json", commands)
    if (sum(row["self_samples"] for row in rows["self"]) != stack_summary["samples"]
            or sum(row["period"] for row in rows["self"]) != stack_summary["total_period"]):
        raise RuntimeError(f"perf report and parsed stacks disagree on sample/period totals: {directory}")
    return {"workload": name, "directory": str(directory),
            "profiled_elapsed_s": elapsed, "profiled_body_s": float(markers[0]),
            "correctness_sha256": correctness, "frame_sha256": frame_hash,
            "perf_data_sha256": representative.digest(raw.read_bytes()),
            "perf_data_bytes": raw.stat().st_size, "samples": stack_summary["samples"],
            "lost_samples": lost_samples,
            "total_period": stack_summary["total_period"],
            "samples_without_frames": stack_summary["samples_without_frames"],
            "unparsed_stack_lines": len(stack_summary["unparsed_lines"]),
            "self": stack_summary["leaf"], "inclusive": stack_summary["inclusive"]}


def aggregate(runs: list[dict]) -> dict:
    result = {}
    for name in dict.fromkeys(run["workload"] for run in runs):
        selected = [run for run in runs if run["workload"] == name]
        samples = sum(run["samples"] for run in selected)
        period = sum(run["total_period"] for run in selected)
        result[name] = {"runs": len(selected), "samples": samples, "total_period": period}
        for category in ("self", "inclusive"):
            counts = collections.Counter()
            periods = collections.Counter()
            for run in selected:
                for row in run[category]:
                    key = row["dso"], row["symbol"]
                    counts[key] += row["samples"]
                    periods[key] += row["period"]
            result[name][category] = [
                {"dso": key[0], "symbol": key[1], "samples": counts[key],
                 "sample_pct": 100 * counts[key] / samples, "period": periods[key],
                 "period_pct": 100 * periods[key] / period if period else None}
                for key in sorted(counts, key=lambda key: periods[key], reverse=True)]
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--perf", type=Path, required=True)
    parser.add_argument("--executable", type=Path, required=True, help="Optimized executable with symbols")
    parser.add_argument("--baseline", type=Path, required=True, help="Normal release used as output oracle")
    parser.add_argument("--config", type=Path, default=representative.CONFIG)
    parser.add_argument("--output", type=Path, required=True,
                        help="New directory; on WSL use Linux ext4 storage for perf.data, not /mnt/c")
    parser.add_argument("--workloads", default="raytracer,pimachin,nqueens,jelly,mandelbrot,strings")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--frequency", type=int, default=997)
    parser.add_argument("--stack-size", type=int, default=16384)
    parser.add_argument("--timeout", type=float, default=180)
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("Run this tool under Linux/WSL")
    if min(args.runs, args.frequency, args.stack_size, args.timeout) <= 0:
        parser.error("runs, frequency, stack-size and timeout must be positive")
    for field in ("perf", "executable", "baseline", "config", "output"):
        setattr(args, field, getattr(args, field).resolve())
    config = json.loads(args.config.read_text(encoding="utf-8"))
    names = args.workloads.split(",")
    if len(set(names)) != len(names) or any(name not in config for name in names):
        parser.error("workloads must be unique names present in config")
    args.output.mkdir(parents=True, exist_ok=False)
    env = {**os.environ, "AVL_BASIC_WINDOW": "0", "LC_ALL": "C", "PERF_PAGER": "cat"}
    metadata = {"created_utc": datetime.now(timezone.utc).isoformat(),
                "platform": platform.platform(), "python": sys.version,
                "configuration": {"event": "cpu-clock:u", "frequency_hz": args.frequency,
                                  "call_graph": f"dwarf,{args.stack_size}", "runs": args.runs,
                                  "AVL_BASIC_WINDOW": "0", "workload_order": "rotating"},
                "scope": "User-space CPU samples over whole process including startup and final PNG; profiled times are not benchmark comparisons",
                "counting": "Physical functions, inline expansion disabled. Self is the leaf; inclusive counts each distinct symbol at most once per sampled stack; period-weighted percentages and raw sample counts are both retained",
                "perf_version": subprocess.check_output([str(args.perf), "--version"], env=env, text=True).strip(),
                "files": {field: {"path": str(getattr(args, field)),
                                    "sha256": representative.digest(getattr(args, field).read_bytes())}
                          for field in ("perf", "executable", "baseline", "config")},
                "cpu_affinity": sorted(os.sched_getaffinity(0)), "workloads": {}, "runs": []}
    for procfile in ("/proc/sys/kernel/perf_event_paranoid", "/proc/sys/kernel/perf_event_max_sample_rate"):
        path = Path(procfile)
        if path.exists():
            metadata.setdefault("kernel_perf_settings", {})[procfile] = path.read_text().strip()

    def save() -> None:
        metadata["aggregate"] = aggregate(metadata["runs"])
        write_json(args.output / "results.json", metadata)

    programs = {}
    expected = {}
    save()
    try:
        for name in names:
            program_text, source_hash = representative.generate(name, config[name])
            programs[name] = program_text
            directory = args.output / name
            directory.mkdir()
            baseline_dir = directory / "baseline-oracle"
            baseline_dir.mkdir()
            program = baseline_dir / f"{name}.bas"
            program.write_text(program_text, encoding="utf-8", newline="\n")
            reference = representative.run(args.baseline, program, args.timeout,
                                           name in representative.GRAPHICS, "0")
            write_json(baseline_dir / "result.json", reference)
            expected[name] = reference["correctness_sha256"]
            metadata["workloads"][name] = {"spec": config[name], "source_sha256": source_hash,
                                           "program_sha256": representative.digest(program.read_bytes()),
                                           "baseline_oracle": reference}
            save()
            print(f"{name}: normal baseline output captured", flush=True)
        for repetition in range(args.runs):
            order = names[repetition % len(names):] + names[:repetition % len(names)]
            for name in order:
                directory = args.output / name / f"run-{repetition + 1:02d}"
                result = profile_once(args, name, programs[name], directory, expected[name], env)
                result["repetition"] = repetition
                metadata["runs"].append(result)
                save()
                print(f"{name} {repetition + 1}/{args.runs}: {result['samples']} samples; outputs match", flush=True)
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        metadata["error"] = str(error)
        save()
        print(str(error), file=sys.stderr)
        return 1
    metadata["complete"] = True
    save()
    print(f"Profiles and summaries: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
