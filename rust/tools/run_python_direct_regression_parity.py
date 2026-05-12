"""Compare Rust with direct non-graphics session regressions from Python tests.

The large parametrized program table is handled by run_python_text_parity.py.
This runner extracts additional tests in tests/test_basic.py that call the
run_basic_interpreter fixture with literal command streams, including simple
pytest.mark.parametrize expansions, then compares the Python oracle output with
the Rust interpreter output.
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
from typing import Any


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
    r"Secuencias de escape ANSI no soportadas\.|"
    r"Saliendo del int.rprete BASIC\.|"
    r"Ready"
    r")$"
)

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


@dataclass(frozen=True)
class DirectCase:
    name: str
    source_line: int
    commands: list[str]
    graphics: bool
    requires_tmp_path: bool

    @property
    def label(self) -> str:
        return f"{self.name}:{self.source_line}"


def python_repo_from_args(value: str | None) -> Path:
    if value:
        return Path(value).resolve()
    return Path(os.environ.get("AVL_BASIC_PY_REPO", DEFAULT_PY_REPO)).resolve()


def rust_bin_from_args(value: str | None) -> Path:
    if value:
        return Path(value).resolve()
    return Path(os.environ.get("AVL_BASIC_RUST_BIN", DEFAULT_RUST_BIN)).resolve()


def literal_value(node: ast.AST, env: dict[str, Any]) -> Any:
    if isinstance(node, ast.Name) and node.id in env:
        return env[node.id]
    if isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, (ast.List, ast.Tuple)):
        values = []
        for item in node.elts:
            try:
                values.append(literal_value(item, env))
            except ValueError as exc:
                raise ValueError from exc
        return values
    try:
        return ast.literal_eval(node)
    except Exception as exc:
        raise ValueError from exc


def dotted_name(node: ast.AST) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        parent = dotted_name(node.value)
        return f"{parent}.{node.attr}" if parent else node.attr
    return ""


def parameter_envs(function: ast.FunctionDef) -> list[tuple[str, dict[str, Any]]]:
    envs: list[tuple[str, dict[str, Any]]] = [("", {})]
    for decorator in function.decorator_list:
        if not isinstance(decorator, ast.Call):
            continue
        if dotted_name(decorator.func) != "pytest.mark.parametrize":
            continue
        if len(decorator.args) < 2:
            continue
        raw_names = literal_value(decorator.args[0], {})
        if isinstance(raw_names, str):
            names = [name.strip() for name in raw_names.split(",")]
        elif isinstance(raw_names, list) and all(isinstance(name, str) for name in raw_names):
            names = raw_names
        else:
            continue
        raw_values = literal_value(decorator.args[1], {})
        if not isinstance(raw_values, list):
            continue

        expanded: list[tuple[str, dict[str, Any]]] = []
        for suffix, base_env in envs:
            for index, raw_value in enumerate(raw_values, start=1):
                if len(names) == 1:
                    values = [raw_value]
                elif isinstance(raw_value, list) and len(raw_value) == len(names):
                    values = raw_value
                else:
                    continue
                next_env = dict(base_env)
                for name, value in zip(names, values):
                    next_env[name] = value
                next_suffix = f"{suffix}[{index}]" if suffix else f"[{index}]"
                expanded.append((next_suffix, next_env))
        envs = expanded or envs
    return envs


def commands_from_call(call: ast.Call, env: dict[str, Any]) -> list[str] | None:
    if not call.args:
        return None
    try:
        value = literal_value(call.args[0], env)
    except ValueError:
        return None
    if isinstance(value, list) and all(isinstance(item, str) for item in value):
        return list(value)
    return None


def extract_calls_from_body(
    function: ast.FunctionDef, suffix: str, initial_env: dict[str, Any]
) -> list[DirectCase]:
    env = dict(initial_env)
    cases: list[DirectCase] = []
    requires_tmp_path = any(arg.arg == "tmp_path" for arg in function.args.args)

    for statement in function.body:
        if (
            isinstance(statement, ast.Assign)
            and len(statement.targets) == 1
            and isinstance(statement.targets[0], ast.Name)
        ):
            try:
                env[statement.targets[0].id] = literal_value(statement.value, env)
            except ValueError:
                env.pop(statement.targets[0].id, None)

        for node in ast.walk(statement):
            if (
                isinstance(node, ast.Call)
                and isinstance(node.func, ast.Name)
                and node.func.id == "run_basic_interpreter"
            ):
                commands = commands_from_call(node, env)
                if commands is None:
                    continue
                program_text = "\n".join(commands)
                cases.append(
                    DirectCase(
                        name=f"{function.name}{suffix}",
                        source_line=node.lineno,
                        commands=commands,
                        graphics=bool(GRAPHICS_RE.search(program_text)),
                        requires_tmp_path=requires_tmp_path,
                    )
                )
    return cases


def extract_direct_cases(py_repo: Path) -> list[DirectCase]:
    test_file = py_repo / "tests" / "test_basic.py"
    if not test_file.exists():
        raise FileNotFoundError(f"Python test file not found: {test_file}")

    module = ast.parse(test_file.read_text(encoding="utf-8"), filename=str(test_file))
    cases: list[DirectCase] = []
    for node in module.body:
        if not isinstance(node, ast.FunctionDef):
            continue
        if node.name == "test_basic_program":
            continue
        if not any(arg.arg == "run_basic_interpreter" for arg in node.args.args):
            continue
        for suffix, env in parameter_envs(node):
            cases.extend(extract_calls_from_body(node, suffix, env))
    return cases


def normalize_session_output(text: str, *, strip_prompt_space: bool) -> str:
    lines: list[str] = []
    prompt_pending = False

    for raw in text.replace("\r\n", "\n").replace("\f", "").splitlines():
        unprompted = (
            raw[1:]
            if strip_prompt_space and prompt_pending and raw.startswith(" ")
            else raw
        )
        stripped = unprompted.strip()
        if SESSION_NOISE_RE.fullmatch(stripped):
            prompt_pending = True
            continue
        if unprompted.endswith("Ready"):
            unprompted = unprompted[: -len("Ready")]
            prompt_pending = True
            if unprompted == "":
                continue
        lines.append(unprompted)
        prompt_pending = False

    return "\n".join(lines).rstrip("\n")


def run_process(
    command: list[str],
    stdin: str,
    cwd: Path,
    timeout: float,
    *,
    strip_prompt_space: bool,
) -> str:
    proc = subprocess.run(
        command,
        input=stdin,
        cwd=str(cwd),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=timeout,
    )
    return normalize_session_output(proc.stdout, strip_prompt_space=strip_prompt_space)


def selected_cases(cases: list[DirectCase], mode: str) -> list[DirectCase]:
    text_cases = [case for case in cases if not case.graphics]
    if mode == "all-text":
        return text_cases
    if mode == "summary":
        return []
    raise ValueError(f"Unknown mode: {mode}")


def print_summary(cases: list[DirectCase]) -> None:
    graphics = sum(1 for case in cases if case.graphics)
    tmp_path = sum(1 for case in cases if case.requires_tmp_path)
    selected = len(selected_cases(cases, "all-text"))
    print(f"direct_session_cases={len(cases)}")
    print(f"graphics_cases={graphics}")
    print(f"tmp_path_cases={tmp_path}")
    print(f"selected_non_graphics_cases={selected}")


def setup_tmp_path_case(case: DirectCase, cwd: Path) -> None:
    name = case.name.split("[", 1)[0]
    if name == "test_run_supports_subdirectories_and_cd_root":
        (cwd / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding="utf-8")
        examples = cwd / "ejemplos"
        examples.mkdir()
        (examples / "demo.bas").write_text('10 PRINT "SUBDIR"\n', encoding="utf-8")
        return
    if name == "test_files_follow_virtual_current_directory_and_cd_parent":
        (cwd / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding="utf-8")
        examples = cwd / "ejemplos"
        examples.mkdir()
        (examples / "demo.bas").write_text('10 PRINT "SUBDIR"\n', encoding="utf-8")
        return
    if name == "test_files_lists_subdirectories_with_trailing_slash":
        (cwd / "ejemplos").mkdir()
        return
    if name == "test_cd_and_files_accept_directory_junctions_inside_virtual_root":
        shared = cwd / "shared"
        shared.mkdir()
        (shared / "demo.bas").write_text('10 PRINT "DEMO"\n', encoding="utf-8")
        samples = cwd / "samples"
        if os.name == "nt":
            subprocess.run(
                ["cmd", "/C", "mklink", "/J", str(samples), str(shared)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=True,
            )
        else:
            os.symlink(shared, samples, target_is_directory=True)
        return
    if name == "test_cd_parent_is_clamped_at_virtual_root":
        (cwd / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding="utf-8")
        return
    if name == "test_running_file_does_not_change_current_directory":
        (cwd / "raiz.bas").write_text('10 PRINT "RAIZ"\n', encoding="utf-8")
        samples = cwd / "samples"
        samples.mkdir()
        (samples / "demo.bas").write_text('10 PRINT "DEMO"\n', encoding="utf-8")
        return
    if name == "test_program_dir_is_used_for_chain_inside_loaded_program":
        samples = cwd / "samples"
        samples.mkdir()
        (samples / "parent.bas").write_text('10 CHAIN "child.bas"\n', encoding="utf-8")
        (samples / "child.bas").write_text('10 PRINT "CHILD"\n', encoding="utf-8")
        return
    if name == "test_load_and_run_ignore_blank_lines_in_source_file":
        (cwd / "blank.bas").write_text(
            '10 PRINT "A"\n\n20 PRINT "B"\n\n', encoding="utf-8"
        )
        return
    raise RuntimeError(f"No tmp_path setup implemented for {case.name}")


def run_selected_cases(args: argparse.Namespace, cases: list[DirectCase], py_repo: Path) -> int:
    rust_bin = rust_bin_from_args(args.rust_bin)
    if not rust_bin.exists():
        print(f"Rust binary not found: {rust_bin}", file=sys.stderr)
        return 2

    chosen = selected_cases(cases, args.mode)
    failures: list[tuple[DirectCase, str, str]] = []

    for case in chosen:
        stdin = "\n".join(case.commands) + "\n"
        tmp_context = tempfile.TemporaryDirectory(prefix="avl_basic_direct_parity_")
        try:
            cwd = PROJECT_DIR
            if case.requires_tmp_path:
                cwd = Path(tmp_context.name)
                setup_tmp_path_case(case, cwd)
            else:
                tmp_context.cleanup()
                tmp_context = None
        except Exception as exc:
            if tmp_context is not None:
                tmp_context.cleanup()
            failures.append((case, f"<setup failed: {exc}>", ""))
            continue
        try:
            expected = run_process(
                [sys.executable, "-X", "utf8", str(py_repo / "basic.py")],
                stdin,
                cwd,
                args.timeout,
                strip_prompt_space=False,
            )
        except subprocess.TimeoutExpired:
            failures.append((case, "<python timeout>", ""))
            if tmp_context is not None:
                tmp_context.cleanup()
            continue
        try:
            actual = run_process(
                [str(rust_bin)],
                stdin,
                cwd,
                args.timeout,
                strip_prompt_space=False,
            )
        except subprocess.TimeoutExpired:
            failures.append((case, expected, "<rust timeout>"))
            if tmp_context is not None:
                tmp_context.cleanup()
            continue
        if tmp_context is not None:
            tmp_context.cleanup()
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
            print(f"case={case.label}")
            print("--- commands ---")
            print("\n".join(case.commands))
            print("--- python oracle ---")
            print(expected)
            print("--- rust ---")
            print(actual)
        return 1

    print(f"ok mode={args.mode} selected={len(chosen)}")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--mode",
        choices=("summary", "all-text"),
        default="all-text",
        help="summary only, or every direct non-graphics session case",
    )
    parser.add_argument("--py-repo", help="Path to the Python BASIC repository")
    parser.add_argument("--rust-bin", help="Path to the avl-basic Rust binary")
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--max-failures", type=int, default=20)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    py_repo = python_repo_from_args(args.py_repo)
    try:
        cases = extract_direct_cases(py_repo)
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 2

    print_summary(cases)
    if args.mode == "summary":
        return 0
    return run_selected_cases(args, cases, py_repo)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
