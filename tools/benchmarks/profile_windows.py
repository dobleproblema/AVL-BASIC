"""Capture native Windows CPU profiles with installed Visual Studio tools.

Each interpreter child is created suspended with stdout/stderr redirected, then
attached to the collector and resumed. Captures are serialized, headless and
checked against an existing representative.py correctness report. Instrumented
durations are diagnostic only; they are not benchmark speedup measurements.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time

from representative import GRAPHICS, TIMING

HERE = Path(__file__).resolve().parent
VS_ROOT = Path(r"C:\Program Files\Microsoft Visual Studio\2022\Community")
COLLECTOR = VS_ROOT / "Team Tools/DiagnosticsHub/Collector/VSDiagnostics.exe"
CREATE_SUSPENDED = 0x00000004
CREATE_NO_WINDOW = 0x08000000


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


class THREADENTRY32(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD),
                ("th32ThreadID", wintypes.DWORD), ("th32OwnerProcessID", wintypes.DWORD),
                ("tpBasePri", wintypes.LONG), ("tpDeltaPri", wintypes.LONG),
                ("dwFlags", wintypes.DWORD)]


def kernel_api():
    api = ctypes.WinDLL("kernel32", use_last_error=True)
    api.CreateToolhelp32Snapshot.argtypes = [wintypes.DWORD, wintypes.DWORD]
    api.CreateToolhelp32Snapshot.restype = wintypes.HANDLE
    api.Thread32First.argtypes = [wintypes.HANDLE, ctypes.POINTER(THREADENTRY32)]
    api.Thread32First.restype = wintypes.BOOL
    api.Thread32Next.argtypes = [wintypes.HANDLE, ctypes.POINTER(THREADENTRY32)]
    api.Thread32Next.restype = wintypes.BOOL
    api.OpenThread.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    api.OpenThread.restype = wintypes.HANDLE
    api.ResumeThread.argtypes = [wintypes.HANDLE]
    api.ResumeThread.restype = wintypes.DWORD
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    api.CloseHandle.restype = wintypes.BOOL
    return api


def initial_thread(api, process_id: int):
    snapshot = api.CreateToolhelp32Snapshot(0x4, 0)
    if snapshot == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry = THREADENTRY32()
        entry.dwSize = ctypes.sizeof(entry)
        thread_ids = []
        valid = api.Thread32First(snapshot, ctypes.byref(entry))
        while valid:
            if entry.th32OwnerProcessID == process_id:
                thread_ids.append(entry.th32ThreadID)
            valid = api.Thread32Next(snapshot, ctypes.byref(entry))
    finally:
        api.CloseHandle(snapshot)
    if len(thread_ids) != 1:
        raise RuntimeError(f"Expected one initial suspended thread in PID {process_id}, got {thread_ids}")
    handle = api.OpenThread(0x2, False, thread_ids[0])  # THREAD_SUSPEND_RESUME
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    return handle, thread_ids[0]


def command(argv: list[str], log: Path, timeout: float = 90, cwd: Path | None = None) -> dict:
    started = time.perf_counter()
    with log.open("wb") as output:
        result = subprocess.run(argv, stdout=output, stderr=subprocess.STDOUT,
                                stdin=subprocess.DEVNULL, cwd=cwd, timeout=timeout,
                                creationflags=CREATE_NO_WINDOW)
    text = log.read_bytes().decode("utf-8-sig", errors="replace")
    return {"argv": argv, "returncode": result.returncode, "wall_s": time.perf_counter() - started,
            "log": str(log), "text": text}


def checked(result: dict) -> None:
    if result["returncode"]:
        raise RuntimeError(f"Command failed ({result['returncode']}): {result['argv']}\n{result['text']}")


def normalized_output(directory: Path, graphics: bool) -> dict:
    stdout = (directory / "stdout.txt").read_bytes().decode("utf-8", errors="replace").replace("\r\n", "\n")
    stderr = (directory / "stderr.txt").read_bytes().decode("utf-8", errors="replace").replace("\r\n", "\n")
    markers = TIMING.findall(stdout)
    if len(markers) != 1:
        raise RuntimeError(f"Expected exactly one timing marker, found {len(markers)} in {directory}")
    frame_hash = sha(directory / "frame.png") if graphics else None
    normalized = TIMING.sub("__BENCH_SECONDS__<timing>", stdout)
    correctness = hashlib.sha256(json.dumps([normalized, stderr, frame_hash]).encode()).hexdigest()
    return {"stdout": stdout, "stderr": stderr, "body_s_diagnostic_only": float(markers[0]),
            "frame_sha256": frame_hash, "correctness_sha256": correctness}


def capture(args, program: Path, run_dir: Path, expected: str) -> dict:
    api = kernel_api()
    result = {"directory": str(run_dir), "commands": [], "session_id": args.session_id}
    capture_started = time.perf_counter()
    status = command([str(args.collector), "status", str(args.session_id)], run_dir / "status-before.log")
    result["commands"].append(status)
    checked(status)
    if "does not exist" not in status["text"]:
        raise RuntimeError(f"Collector session {args.session_id} is not confirmed absent; will not alter it")
    child = None
    thread_handle = None
    start_attempted = False
    diag = run_dir / "capture.diagsession"
    try:
        with (run_dir / "stdout.txt").open("wb") as out, (run_dir / "stderr.txt").open("wb") as err:
            child = subprocess.Popen([str(args.exe), str(program)], cwd=run_dir,
                                     env={**os.environ, "AVL_BASIC_WINDOW": "0"},
                                     stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                     creationflags=CREATE_SUSPENDED | CREATE_NO_WINDOW)
            result["pid"] = child.pid
            thread_handle, thread_id = initial_thread(api, child.pid)
            result["main_thread_id"] = thread_id
            start_attempted = True
            started = command([str(args.collector), "start", str(args.session_id),
                               f"/attach:{child.pid}", f"/loadConfig:{args.collector_config}"], run_dir / "start.log")
            result["commands"].append(started)
            checked(started)
            if "Running" not in started["text"]:
                raise RuntimeError(f"Collector did not confirm a running session: {started['text']}")
            execution_start = time.perf_counter()
            previous_count = api.ResumeThread(thread_handle)
            if previous_count != 1:
                raise RuntimeError(f"Unexpected initial suspension count {previous_count} for PID {child.pid}")
            result["returncode"] = child.wait(timeout=args.timeout)
            result["execution_wall_s_diagnostic_only"] = time.perf_counter() - execution_start
    finally:
        # Always reap our child and stop our exact session, including attach or
        # program failures. No unrelated process/session is killed or stopped.
        if child is not None and child.poll() is None:
            child.kill()
            child.wait(timeout=10)
        if thread_handle is not None:
            api.CloseHandle(thread_handle)
        if start_attempted:
            stopped = command([str(args.collector), "stop", str(args.session_id), f"/output:{diag}"], run_dir / "stop.log")
            result["commands"].append(stopped)
            checked(stopped)
            status_after = command([str(args.collector), "status", str(args.session_id)], run_dir / "status-after.log")
            result["commands"].append(status_after)
            checked(status_after)
            if "does not exist" not in status_after["text"]:
                raise RuntimeError(f"Collector session {args.session_id} was not confirmed stopped; see {run_dir}")
        result["capture_wall_s_diagnostic_only"] = time.perf_counter() - capture_started
        write_json(run_dir / "capture-state.json", result)
    if result["returncode"]:
        raise RuntimeError(f"Interpreter returned {result['returncode']}: {run_dir}")
    result["output"] = normalized_output(run_dir, program.stem in GRAPHICS)
    if result["output"]["correctness_sha256"] != expected:
        raise RuntimeError(f"Captured output differs from unprofiled oracle: {run_dir}")
    if not diag.is_file() or not diag.stat().st_size:
        raise RuntimeError(f"Collector produced no diagnostic archive: {diag}")
    result["diagsession_sha256"] = sha(diag)
    expanded = command([str(args.collector), "expandDiagSession", str(diag)], run_dir / "expand.log")
    result["commands"].append(expanded)
    checked(expanded)
    etls = list(diag.with_suffix("").rglob("*.etl"))
    if len(etls) != 1:
        raise RuntimeError(f"Expected one ETL payload, found {etls}")
    result["etl"] = str(etls[0])
    result["etl_sha256"] = sha(etls[0])
    analysis = run_dir / "analysis"
    extract = command([str(args.pwsh), "-NoProfile", "-File", str(HERE / "extract-cpu-trace.ps1"),
                       "-Etl", str(etls[0]), "-ExactImage", str(args.exe), "-ProcessId", str(result["pid"]),
                       "-SymbolDirectory", str(args.pdb.parent), "-OutputDirectory", str(analysis)],
                      run_dir / "extract.log", timeout=180)
    result["commands"].append(extract)
    checked(extract)
    profile = json.loads((analysis / "profile.json").read_text(encoding="utf-8-sig"))
    result["profile"] = str(analysis / "profile.json")
    result["quality"] = {key: profile[key] for key in ("Samples", "SamplesWithStack", "UnknownLeafSamples",
                                                       "SamplesWithUnknownFrame", "EventsLost", "Truncated", "CountSum", "Warnings")}
    if profile["Samples"] == 0 or profile["EventsLost"] or profile["Truncated"]:
        raise RuntimeError(f"Capture quality failed: {result['quality']}")
    if len(profile["Observations"]) != profile["Samples"] or sum(r["Exclusive"] for r in profile["Functions"]) != profile["Samples"]:
        raise RuntimeError("Sample accounting is inconsistent")
    result["functions"] = profile["Functions"]
    result["lines"] = profile["Lines"]
    write_json(run_dir / "capture-state.json", result)
    return result


def aggregate(runs: list[dict]) -> dict:
    summary = {}
    for name in dict.fromkeys(run["workload"] for run in runs):
        selected = [run for run in runs if run["workload"] == name]
        row = {"captures": len(selected), "samples": sum(run["quality"]["Samples"] for run in selected),
               "lost_events": sum(run["quality"]["EventsLost"] for run in selected)}
        for kind in ("functions", "lines"):
            maps = [{f["Name"]: f for f in run[kind]} for run in selected]
            keys = set().union(*(mapping.keys() for mapping in maps))
            entries = []
            for key in keys:
                entry = {"name": key}
                for field in ("ExclusivePct", "InclusivePct"):
                    values = [mapping.get(key, {}).get(field, 0) for mapping in maps]
                    entry[field] = {"median": statistics.median(values), "min": min(values), "max": max(values)}
                entries.append(entry)
            row[kind] = sorted(entries, key=lambda r: r["ExclusivePct"]["median"], reverse=True)
        summary[name] = row
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--pdb", type=Path, required=True)
    parser.add_argument("--programs", type=Path, required=True, help="Prepared representative.py output directory")
    parser.add_argument("--oracle-results", type=Path, required=True, help="Successful unprofiled representative.py results.json")
    parser.add_argument("--output", type=Path, required=True, help="New output directory")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--workloads", help="Comma-separated tuning workloads; default all prepared tuning workloads")
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--session-id", type=int, default=173)
    parser.add_argument("--collector", type=Path, default=COLLECTOR)
    parser.add_argument("--collector-config", type=Path)
    parser.add_argument("--pwsh", type=Path, default=Path(shutil.which("pwsh") or "pwsh.exe"))
    parser.add_argument("--prepare-only", action="store_true", help="Validate inputs/write manifest without launching any process")
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Windows is required")
    if args.runs < 1 or args.timeout <= 0 or not 1 <= args.session_id <= 255:
        parser.error("runs/timeout must be positive; session-id must be 1..255")
    for key in ("exe", "pdb", "programs", "oracle_results", "output", "collector", "pwsh"):
        setattr(args, key, getattr(args, key).resolve())
    args.collector_config = (args.collector_config or args.collector.parent / "AgentConfigs/CpuUsageBase.json").resolve()
    for path in (args.exe, args.pdb, args.collector, args.collector_config, args.oracle_results, args.pwsh):
        if not path.is_file():
            parser.error(f"Missing file: {path}")
    prepared_path = args.programs / "results.json"
    prepared = json.loads(prepared_path.read_text(encoding="utf-8-sig"))
    oracle = json.loads(args.oracle_results.read_text(encoding="utf-8-sig"))
    if oracle.get("error"):
        parser.error("Oracle report contains an error")
    exe_hash = sha(args.exe)
    if exe_hash not in {row["sha256"] for row in oracle["executables"].values()}:
        parser.error("Profiled executable has no matching unprofiled executable hash in oracle report")
    requested = set(args.workloads.split(",")) if args.workloads else set(prepared["workloads"])
    if requested - prepared["workloads"].keys():
        parser.error("Unknown workloads")
    selected = {name: workload for name, workload in prepared["workloads"].items()
                if name in requested and workload["spec"]["group"] == "tuning"}
    if not selected:
        parser.error("No tuning workloads selected")
    expected = {}
    programs = {}
    for name, workload in selected.items():
        program = args.programs / name / f"{name}.bas"
        program_hash = sha(program)
        if program_hash != workload["program_sha256"] or program_hash != oracle["workloads"][name]["program_sha256"]:
            parser.error(f"Prepared program or oracle changed: {name}")
        hashes = {run["correctness_sha256"] for run in oracle["runs"] if run["workload"] == name}
        if len(hashes) != 1:
            parser.error(f"Oracle is absent or inconsistent: {name}")
        expected[name] = hashes.pop()
        programs[name] = program
    args.output.mkdir(parents=True, exist_ok=False)
    report = {"created_utc": datetime.now(timezone.utc).isoformat(), "platform": platform.platform(), "python": sys.version,
              "executable": {"path": str(args.exe), "sha256": exe_hash}, "pdb": {"path": str(args.pdb), "sha256": sha(args.pdb)},
              "tools": {str(path): sha(path) for path in (args.collector, args.collector_config, HERE / "ExtractCpuTrace.cs", HERE / "extract-cpu-trace.ps1", Path(__file__))},
              "oracle": {"path": str(args.oracle_results), "sha256": sha(args.oracle_results)},
              "configuration": {"runs": args.runs, "AVL_BASIC_WINDOW": "0", "session_id": args.session_id,
                                "method": "suspended child, exact-PID attach, resume, wait, stop, offline extraction",
                                "order": "rotating workloads; captures and conversions sequential",
                                "metric_warning": "Profiled timings are not speedup measurements"},
              "workloads": selected, "runs": []}

    def save():
        report["summary"] = aggregate(report["runs"])
        write_json(args.output / "results.json", report)

    save()
    if args.prepare_only:
        print(f"Inputs validated; no process launched. Manifest: {args.output / 'results.json'}")
        return 0
    names = list(selected)
    try:
        for repetition in range(args.runs):
            order = names[repetition % len(names):] + names[:repetition % len(names)]
            for name in order:
                run_dir = args.output / name / f"run-{repetition + 1:02d}"
                run_dir.mkdir(parents=True, exist_ok=False)
                program = run_dir / f"{name}.bas"
                shutil.copyfile(programs[name], program)
                print(f"Capturing {name} {repetition + 1}/{args.runs}", flush=True)
                result = capture(args, program, run_dir, expected[name])
                result.update(workload=name, repetition=repetition)
                report["runs"].append(result)
                save()
                quality = result["quality"]
                print(f"Verified {name}: {quality['Samples']} samples, {quality['EventsLost']} lost; stdout/stderr/PNG match", flush=True)
    except (Exception, KeyboardInterrupt) as error:
        report["error"] = str(error) or type(error).__name__
        save()
        print(f"Failed: {report['error']}", file=sys.stderr, flush=True)
        return 1
    print(f"Completed profiles: {args.output / 'results.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
