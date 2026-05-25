"""Compare Rust PRINT USING formatting with the Python reference.

This exercises the formatting function through real BASIC programs so that the
Rust parser, PRINT USING execution path, and formatter are compared together.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path


PROJECT_DIR = Path(__file__).resolve().parents[1]


def default_python_repo() -> Path:
    parent = PROJECT_DIR.parent
    if (parent / "basic.py").exists():
        return parent
    return parent / "AVL-BASIC"


DEFAULT_PY_REPO = default_python_repo()
DEFAULT_RUST_BIN = PROJECT_DIR / "target" / "debug" / (
    "avl-basic.exe" if os.name == "nt" else "avl-basic"
)


BASE_FORMATS = [
    "#,###,###",
    "#,###,###.##",
    ",#,###,###.##",
    ",#,###,###,###.##",
    "0.00^^^^",
    ",0.00^^^^",
    "0.00^^^^^",
    "#.##^^^^",
    "##.##^^^^",
    "+0.00^^^^",
    "0##.##",
    "0##.##*",
    "0*##.##",
    "0##.##-",
    "*###.##",
    "##.##",
    "*##.##",
    "0######.##",
    "**###.##",
    "**0##.##",
    "*0##.##",
    "$0##.##",
    "##:##",
    "0#:##",
    "#:##",
    "#-#",
    "A##B",
    "##-##",
    "**##.##",
    "#,###  ",
    "##.#",
    ".###",
    "0.###",
    "0.###############",
    "0.####################",
    "############.##",
    "######.##",
]

INTEGER_MASKS = [
    "#",
    "##",
    "###",
    "0#",
    "0##",
    "#,###",
    "##,###",
    "A##B",
    "*###",
    "$0##",
]

FRACTION_MASKS = ["", ".#", ".##", ".###"]

VALUES = [
    -12345.67,
    -345.2,
    -3.2,
    -0.0001,
    -0.0,
    0,
    0.0049,
    0.005,
    0.0149,
    0.015,
    0.05,
    0.15,
    0.5,
    1,
    1.2345,
    3.2,
    12.34,
    45,
    234,
    536.82,
    999.9,
    1234,
    4567,
    12345,
    123456.7892,
    1234567.23,
    123245435234,
    1e-15,
    1.234567890123456e-10,
    1e20,
    1e100,
    1e308,
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--python-repo",
        type=Path,
        default=Path(os.environ.get("AVL_BASIC_PY_REPO", DEFAULT_PY_REPO)),
    )
    parser.add_argument(
        "--rust-bin",
        type=Path,
        default=Path(os.environ.get("AVL_BASIC_RUST_BIN", DEFAULT_RUST_BIN)),
    )
    return parser.parse_args()


def generated_formats() -> list[str]:
    formats = list(BASE_FORMATS)
    for int_mask in INTEGER_MASKS:
        for frac in FRACTION_MASKS:
            formats.append(int_mask + frac)
            formats.append("+" + int_mask + frac)
            formats.append("," + int_mask + frac)
            if frac:
                formats.append(int_mask + frac + "^^^^")
                formats.append(int_mask + frac + "^^^^^")
    return formats


def basic_number(value: float) -> str:
    if value == 0:
        return "0"
    return format(value, ".17g").replace("e", "E")


def load_python_reference(py_repo: Path):
    sys.path.insert(0, str(py_repo))
    from basic import BasicInterpreter  # pylint: disable=import-outside-toplevel

    return BasicInterpreter


def build_cases(basic_interpreter) -> list[tuple[str, float, str]]:
    cases: list[tuple[str, float, str]] = []
    seen: set[tuple[str, float]] = set()
    for fmt in generated_formats():
        for value in VALUES:
            key = (fmt, value)
            if key in seen:
                continue
            seen.add(key)
            basic_interpreter.format_using.cache_clear()
            expected = basic_interpreter.format_using(value, fmt)
            if expected is not None:
                cases.append((fmt, value, expected))
    return cases


def run_rust_program(rust_bin: Path, cases: list[tuple[str, float, str]]) -> list[str]:
    program_lines = [
        f'{idx * 10} PRINT USING "{fmt}"; {basic_number(value)}'
        for idx, (fmt, value, _) in enumerate(cases, start=1)
    ]
    program_lines.append(f"{(len(cases) + 1) * 10} END")
    program = "\n".join(program_lines) + "\n"

    with tempfile.NamedTemporaryFile("w", suffix=".bas", delete=False) as handle:
        handle.write(program)
        program_path = Path(handle.name)

    try:
        completed = subprocess.run(
            [str(rust_bin), str(program_path)],
            cwd=PROJECT_DIR,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=120,
            check=False,
        )
    finally:
        program_path.unlink(missing_ok=True)

    if completed.returncode != 0:
        raise RuntimeError(
            "Rust interpreter failed\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed.stdout.splitlines()


def main() -> int:
    args = parse_args()
    basic_interpreter = load_python_reference(args.python_repo.resolve())
    cases = build_cases(basic_interpreter)
    actual = run_rust_program(args.rust_bin.resolve(), cases)
    expected = [expected for _, _, expected in cases]

    print(f"print_using_cases={len(cases)}")
    if len(actual) != len(expected):
        print(f"line count mismatch: expected {len(expected)}, got {len(actual)}")
        return 1

    for idx, (got, want) in enumerate(zip(actual, expected), start=1):
        if got == want:
            continue
        fmt, value, _ = cases[idx - 1]
        print(f"case={idx} format={fmt!r} value={value!r}")
        print(f"expected={want!r}")
        print(f"actual={got!r}")
        return 1

    print("all matched")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
