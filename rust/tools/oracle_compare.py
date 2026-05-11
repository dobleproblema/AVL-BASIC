"""Compare avl-basic Rust output with the read-only Python oracle.

Usage:
    python tools/oracle_compare.py path/to/program.bas

Environment:
    AVL_BASIC_PY_ORACLE defaults to the Python implementation in the monorepo.
    AVL_BASIC_RUST_BIN defaults to the debug avl-basic binary.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


PROJECT = Path(__file__).resolve().parents[1]


def default_python_repo() -> Path:
    parent = PROJECT.parent
    if (parent / "basic.py").exists():
        return parent
    return parent / "AVL-BASIC"


DEFAULT_ORACLE = default_python_repo() / "basic.py"
DEFAULT_RUST = PROJECT / "target" / "debug" / ("avl-basic.exe" if os.name == "nt" else "avl-basic")


NOISE_PREFIXES = (
    "AVL BASIC v",
    "BASIC interpreter written",
    "Copyright ",
    "License:",
    "This is free software",
    "This program comes with",
    "Ready",
    "Saliendo",
)


def clean(text: str) -> str:
    lines = []
    for raw in text.replace("\r\n", "\n").splitlines():
        if any(raw.startswith(prefix) for prefix in NOISE_PREFIXES):
            continue
        lines.append(raw)
    return "\n".join(lines).strip() + "\n"


def run(cmd: list[str], stdin: str = "") -> str:
    proc = subprocess.run(
        cmd,
        input=stdin,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        encoding="utf-8",
        errors="replace",
        timeout=30,
    )
    return proc.stdout


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2

    program_arg = Path(sys.argv[1])
    program = program_arg.resolve()
    oracle = Path(os.environ.get("AVL_BASIC_PY_ORACLE", DEFAULT_ORACLE)).resolve()
    rust_bin = Path(os.environ.get("AVL_BASIC_RUST_BIN", DEFAULT_RUST)).resolve()

    try:
        py_program_arg = str(program.relative_to(PROJECT))
    except ValueError:
        py_program_arg = str(program_arg)

    py_out = clean(run([sys.executable, "-X", "utf8", str(oracle), py_program_arg]))
    rust_out = clean(run([str(rust_bin), str(program)]))

    if py_out != rust_out:
        print("MISMATCH")
        print("--- python oracle ---")
        print(py_out)
        print("--- rust ---")
        print(rust_out)
        return 1

    print("OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
