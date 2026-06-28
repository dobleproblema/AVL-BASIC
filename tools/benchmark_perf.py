"""Benchmark AVL BASIC Rust and Python on the same program.

Rust benchmarking works from this checkout. Python comparison requires
AVL_BASIC_PY_REPO or --python-repo pointing at the external reference checkout.

Example:
    python tools/benchmark_perf.py --runs 7
"""

from __future__ import annotations

import argparse
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path


PROJECT_DIR = Path(__file__).resolve().parents[1]


DEFAULT_RUST_BIN = PROJECT_DIR / "target" / "release" / (
    "avl-basic.exe" if os.name == "nt" else "avl-basic"
)
DEFAULT_PROGRAM = PROJECT_DIR / "samples" / "pimachin.bas"


def command_name(command: list[str]) -> str:
    return Path(command[0]).name


def python_program_arg(program: Path, py_repo: Path) -> str:
    try:
        return str(program.relative_to(py_repo))
    except ValueError:
        return str(program)


def run_once(
    command: list[str],
    cwd: Path,
    timeout: float,
) -> float:
    start = time.perf_counter()
    result = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        timeout=timeout,
        check=False,
    )
    elapsed = time.perf_counter() - start
    if result.returncode != 0:
        stderr = result.stderr.strip()
        detail = f": {stderr}" if stderr else ""
        raise SystemExit(
            f"{command_name(command)} exited with {result.returncode}{detail}"
        )
    return elapsed


def measure(
    label: str,
    command: list[str],
    cwd: Path,
    runs: int,
    warmups: int,
    timeout: float,
) -> list[float]:
    for _ in range(warmups):
        run_once(command, cwd, timeout)
    samples = [run_once(command, cwd, timeout) for _ in range(runs)]
    avg = statistics.fmean(samples)
    print(
        f"{label:<8} min={min(samples):.4f}s "
        f"avg={avg:.4f}s max={max(samples):.4f}s runs={runs}"
    )
    return samples


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python-repo", type=Path, default=None)
    parser.add_argument("--rust-bin", type=Path, default=DEFAULT_RUST_BIN)
    parser.add_argument("--program", type=Path, default=DEFAULT_PROGRAM)
    parser.add_argument("--runs", type=int, default=7)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument(
        "--only",
        choices=("both", "rust", "python"),
        default="both",
        help="Select which interpreter to measure.",
    )
    args = parser.parse_args()
    if args.python_repo is None and args.only in ("both", "python"):
        if repo := os.environ.get("AVL_BASIC_PY_REPO"):
            args.python_repo = Path(repo)
        else:
            raise SystemExit("Set AVL_BASIC_PY_REPO or pass --python-repo to benchmark Python")
    return args


def main() -> int:
    args = parse_args()
    program = args.program.resolve()
    py_repo = args.python_repo.resolve() if args.python_repo else None
    rust_bin = args.rust_bin.resolve()
    if not program.exists():
        raise SystemExit(f"Program not found: {program}")
    if args.only in ("both", "python") and (py_repo is None or not (py_repo / "basic.py").exists()):
        raise SystemExit(f"Python oracle not found: {py_repo / 'basic.py' if py_repo else '<unset>'}")
    if args.only in ("both", "rust") and not rust_bin.exists():
        raise SystemExit(f"Rust binary not found: {rust_bin}")
    if args.runs <= 0 or args.warmups < 0:
        raise SystemExit("runs must be positive and warmups cannot be negative")

    print(f"program  {program}")
    rust_samples: list[float] | None = None
    python_samples: list[float] | None = None
    py_program_arg = python_program_arg(program, py_repo) if py_repo else ""
    if args.only in ("both", "rust"):
        rust_samples = measure(
            "rust",
            [str(rust_bin), str(program)],
            PROJECT_DIR,
            args.runs,
            args.warmups,
            args.timeout,
        )
    if args.only in ("both", "python"):
        python_samples = measure(
            "python",
            [sys.executable, str(py_repo / "basic.py"), py_program_arg],
            py_repo,
            args.runs,
            args.warmups,
            args.timeout,
        )
    if rust_samples and python_samples:
        rust_min = min(rust_samples)
        python_min = min(python_samples)
        if rust_min > 0.0:
            print(f"ratio    python_min/rust_min={python_min / rust_min:.2f}x")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
