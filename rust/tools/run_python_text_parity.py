"""Run Rust AVL BASIC against textual program cases from the Python test suite.

The Python repository remains read-only. This script parses tests/test_basic.py,
extracts the large pytest parameter table used by test_basic_program, excludes
graphics cases, and compares Rust CLI output with the expected text embedded in
the Python tests.

Default paths assume the monorepo layout:
    AVL-BASIC/       Python reference implementation
    AVL-BASIC/rust/  Rust implementation

Examples:
    python tools/run_python_text_parity.py --mode summary
    python tools/run_python_text_parity.py --mode supported
    python tools/run_python_text_parity.py --mode all-text --rust-bin target/release/avl-basic.exe
"""

from __future__ import annotations

import argparse
import ast
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
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

SESSION_NOISE_RE = re.compile(
    r"^(?:"
    r"AVL BASIC v1\.5|"
    r"BASIC interpreter written in (?:Python|Rust)|"
    r"Copyright 2024-2026 .+|"
    r"License: GPLv3 or later \(see COPYING\)|"
    r"This is free software under GPLv3 or later\. You may redistribute it under its terms\.|"
    r"This program comes with ABSOLUTELY NO WARRANTY\. See COPYING\.|"
    r"Ready|"
    r"Saliendo del interprete BASIC\."
    r")$"
)

# This is intentionally conservative. The parametrized Python program cases are
# currently textual, but the classifier keeps the separation explicit as the
# Python test suite grows.
GRAPHICS_RE = re.compile(
    r"\b("
    r"SCREEN|CLG|PLOT|PLOTR|DRAW|DRAWR|MOVE|MOVER|MODE|INK|PAPER|"
    r"SPRITE|BLOAD|BSAVE|CIRCLE|FCIRCLE|CIRCLER|FCIRCLER|RECTANGLE|"
    r"FRECTANGLE|TRIANGLE|FTRIANGLE|FILL|FRAME|GDISP|LDIR|PENWIDTH|"
    r"MASK|GRAPH|GRAPHRANGE|XAXIS|YAXIS|CROSSAT|SCALE|ORIGIN|"
    r"MOUSE|KEYDOWN|COLMODE|COLCOLOR|COLRESET|HIT|HITCOLOR|"
    r"HITSPRITE|HITID"
    r")\b|SCREEN\$|SPRITE\$|INKEY\$|TEST\s*\(",
    re.IGNORECASE,
)

# These are the Python text parity cases that the current Rust interpreter
# matches exactly. The AVL-BASIC oracle currently exposes 391 text cases.
SUPPORTED_TEXT_CASE_IDS = set(range(1, 392))


@dataclass(frozen=True)
class ProgramCase:
    case_id: int
    source_line: int
    program: str
    expected: str
    graphics: bool

    @property
    def first_source_line(self) -> str:
        for line in self.program.strip().splitlines():
            if line.strip():
                return line.strip()
        return "<blank program>"


def python_repo_from_args(value: str | None) -> Path:
    if value:
        return Path(value).resolve()
    return Path(os.environ.get("AVL_BASIC_PY_REPO", DEFAULT_PY_REPO)).resolve()


def rust_bin_from_args(value: str | None) -> Path:
    if value:
        return Path(value).resolve()
    return Path(os.environ.get("AVL_BASIC_RUST_BIN", DEFAULT_RUST_BIN)).resolve()


def extract_program_cases(py_repo: Path) -> list[ProgramCase]:
    test_file = py_repo / "tests" / "test_basic.py"
    if not test_file.exists():
        raise FileNotFoundError(f"Python test file not found: {test_file}")

    module = ast.parse(test_file.read_text(encoding="utf-8"), filename=str(test_file))
    cases: list[ProgramCase] = []

    for node in ast.walk(module):
        if not isinstance(node, ast.FunctionDef) or node.name != "test_basic_program":
            continue
        for decorator in node.decorator_list:
            if not isinstance(decorator, ast.Call) or len(decorator.args) < 2:
                continue
            arg0 = decorator.args[0]
            if not isinstance(arg0, ast.Constant) or arg0.value != "program_code, expected_output":
                continue
            case_list = decorator.args[1]
            if not isinstance(case_list, ast.List):
                continue
            for case_id, item in enumerate(case_list.elts, start=1):
                if not isinstance(item, ast.Tuple) or len(item.elts) != 2:
                    continue
                program = ast.literal_eval(item.elts[0])
                expected = ast.literal_eval(item.elts[1])
                cases.append(
                    ProgramCase(
                        case_id=case_id,
                        source_line=item.lineno,
                        program=program,
                        expected=expected,
                        graphics=bool(GRAPHICS_RE.search(program)),
                    )
                )

    if not cases:
        raise RuntimeError("No test_basic_program parameter cases were found")
    return cases


def normalize_expected(text: str) -> str:
    return text.replace("\r\n", "\n").rstrip("\n")


def normalize_rust_session_output(text: str) -> str:
    lines: list[str] = []

    for raw in text.replace("\r\n", "\n").splitlines():
        unprompted = raw
        stripped = unprompted.strip()
        if SESSION_NOISE_RE.fullmatch(stripped):
            continue
        if unprompted.endswith("Ready"):
            unprompted = unprompted[: -len("Ready")]
            if unprompted == "":
                continue
        lines.append(unprompted)

    return "\n".join(lines).rstrip("\n")


def run_rust_case(rust_bin: Path, case: ProgramCase, temp_dir: Path, timeout: float) -> str:
    del temp_dir
    stdin = "NEW\n" + case.program.strip() + "\nRUN\nEXIT\n"
    proc = subprocess.run(
        [str(rust_bin)],
        input=stdin,
        cwd=str(PROJECT_DIR),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    return normalize_rust_session_output(proc.stdout)


def selected_cases(cases: list[ProgramCase], mode: str) -> list[ProgramCase]:
    text_cases = [case for case in cases if not case.graphics]
    if mode == "all-text":
        return text_cases
    if mode == "supported":
        present = {case.case_id for case in text_cases}
        missing = sorted(SUPPORTED_TEXT_CASE_IDS - present)
        if missing:
            raise RuntimeError(f"Supported case ids missing from Python suite: {missing}")
        return [case for case in text_cases if case.case_id in SUPPORTED_TEXT_CASE_IDS]
    raise ValueError(f"Unknown mode: {mode}")


def print_summary(cases: list[ProgramCase]) -> None:
    text_count = sum(1 for case in cases if not case.graphics)
    graphics_count = len(cases) - text_count
    supported_count = sum(
        1
        for case in cases
        if not case.graphics and case.case_id in SUPPORTED_TEXT_CASE_IDS
    )
    print(f"program_cases={len(cases)}")
    print(f"text_cases={text_count}")
    print(f"graphics_cases={graphics_count}")
    print(f"supported_text_cases={supported_count}")


def run_selected_cases(args: argparse.Namespace, cases: list[ProgramCase]) -> int:
    rust_bin = rust_bin_from_args(args.rust_bin)
    if not rust_bin.exists():
        print(f"Rust binary not found: {rust_bin}", file=sys.stderr)
        return 2

    chosen = selected_cases(cases, args.mode)
    failures: list[tuple[ProgramCase, str, str]] = []

    with tempfile.TemporaryDirectory(prefix="avl_basic_text_parity_") as tmp:
        temp_dir = Path(tmp)
        for case in chosen:
            try:
                actual = run_rust_case(rust_bin, case, temp_dir, args.timeout)
            except subprocess.TimeoutExpired:
                actual = "<timeout>"
            expected = normalize_expected(case.expected)
            if actual != expected:
                failures.append((case, expected, actual))
                if len(failures) >= args.max_failures:
                    break

    if failures:
        print(
            f"{len(failures)} mismatch(es) before stopping "
            f"(mode={args.mode}, selected={len(chosen)})"
        )
        for case, expected, actual in failures:
            print()
            print(
                f"case={case.case_id} source_line={case.source_line} "
                f"first_line={case.first_source_line}"
            )
            print("--- expected ---")
            print(expected)
            print("--- actual ---")
            print(actual)
        return 1

    print(f"ok mode={args.mode} selected={len(chosen)}")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--mode",
        choices=("summary", "supported", "all-text"),
        default="supported",
        help="summary only, current exact-pass set, or every non-graphics program case",
    )
    parser.add_argument("--py-repo", help="Path to the Python BASIC repository")
    parser.add_argument("--rust-bin", help="Path to the avl-basic Rust binary")
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--max-failures", type=int, default=20)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        cases = extract_program_cases(python_repo_from_args(args.py_repo))
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 2

    print_summary(cases)
    if args.mode == "summary":
        return 0
    return run_selected_cases(args, cases)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
