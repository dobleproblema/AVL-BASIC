#!/usr/bin/env python3
"""Run the exact block-IF/GOSUB Rust fixtures through real Rust/Python prompts.

Each runtime is checked against the fixture's expected result, not against the
other runtime as an infallible oracle. Programs live only in child-process memory.
No packages beyond Python's standard library are required by this runner.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import subprocess
import sys


BEGIN, END = "__IF_CALL_BEGIN__", "__IF_CALL_END__"
STRING = r'"((?:[^"\\]|\\.)*)"'
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


@dataclass(frozen=True)
class Case:
    name: str
    source: str
    immediate: str
    expected: str
    stopped: bool = False
    continuation: str = "CONT"


def unquote(value: str) -> str:
    # This corpus uses the common JSON/Rust string escapes only.
    return json.loads('"' + value + '"')


def cases_from_rust(path: Path) -> list[Case]:
    text = path.read_text(encoding="utf-8-sig")
    cases = []
    program = re.compile(r'assert_program\(\s*r#"(.*?)"#\s*,\s*' + STRING, re.S)
    for match in program.finditer(text):
        names = re.findall(r"\bfn\s+(\w+)\s*\(", text[:match.start()])
        cases.append(Case(names[-1], match[1], "", unquote(match[2])))
    prompts = re.search(r"const STOP_PROMPT_CASES:.*?=\s*&\[(.*?)\];", text, re.S)
    if not prompts:
        raise ValueError("STOP_PROMPT_CASES fixture was not found")
    prompt_cases = re.findall(r"\(\s*" + STRING + r"\s*,\s*" + STRING + r"\s*,?\s*\)", prompts[1])
    for name in ("STOP_IN_TOP_IF", "STOP_IN_GOSUB_IF"):
        source = re.search(r"const " + name + r':\s*&str\s*=\s*r#"(.*?)"#;', text, re.S)
        if not source:
            raise ValueError(f"{name} fixture was not found")
        for index, (immediate, expected) in enumerate(prompt_cases):
            cases.append(Case(f"{name.lower()}_prompt_{index}", source[1],
                              unquote(immediate), unquote(expected), stopped=True))
    goto = re.search(r"const IMMEDIATE_GOTO_CASES:.*?=\s*&\[(.*?)\];", text, re.S)
    if not goto:
        raise ValueError("IMMEDIATE_GOTO_CASES fixture was not found")
    goto_cases = re.findall(r"\(\s*(\w+)\s*,\s*" + STRING + r"\s*,\s*" + STRING + r"\s*,?\s*\)", goto[1])
    for name, command, expected in goto_cases:
        source = re.search(r"const " + name + r':\s*&str\s*=\s*r#"(.*?)"#;', text, re.S)
        if not source:
            raise ValueError(f"{name} fixture was not found")
        continuation = unquote(command)
        cases.append(Case(f"{name.lower()}_{continuation.replace(' ', '_').lower()}",
                          source[1], "", unquote(expected), stopped=True,
                          continuation=continuation))
    if not cases or not prompt_cases or not goto_cases:
        raise ValueError(f"Unexpected fixture matrix: {len(cases)} cases")
    return cases


def comparable(output: str) -> list[str]:
    lines = []
    for raw in ANSI.sub("", output).splitlines():
        line = raw
        while line.startswith(">> "):
            line = line[3:]
        if line == "Ready" or re.fullmatch(r"(?:Line \d+\. )?Program stopped\.", line):
            continue
        # Preserve program whitespace, including numeric padding and blank lines.
        lines.append(line)
    return lines


def check(label: str, command: list[str], case: Case, timeout: float) -> tuple[str, str, str | None]:
    commands = [case.source, f'PRINT "{BEGIN}"', "RUN"]
    if case.stopped:
        commands.extend([case.immediate, case.continuation])
    commands.extend([f'PRINT "{END}"', "QUIT", ""])
    environment = {
        **os.environ, "PYTHONIOENCODING": "utf-8", "PYTHONUTF8": "1",
        "NO_COLOR": "1", "AVL_BASIC_COLOR": "0", "AVL_BASIC_WINDOW": "0",
        "SDL_VIDEODRIVER": "dummy", "SDL_AUDIODRIVER": "dummy",
        "PYGAME_HIDE_SUPPORT_PROMPT": "1",
    }
    try:
        result = subprocess.run(command, input="\n".join(commands), capture_output=True,
                                text=True, encoding="utf-8", errors="replace", env=environment,
                                timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return label, case.name, f"timed out after {timeout:g}s"
    output = ANSI.sub("", result.stdout)
    if result.returncode or BEGIN not in output or END not in output:
        return label, case.name, f"exit={result.returncode}; stdout={output[-1200:]!r}; stderr={result.stderr[-500:]!r}"
    body = output.split(BEGIN, 1)[1].split(END, 1)[0]
    # Only the marker PRINT's own line terminator is outside the program output.
    body = re.sub(r"^\r?\n", "", body, count=1)
    if case.stopped and "Program stopped." not in body:
        return label, case.name, f"STOP was not reached: {body!r}"
    actual, expected = comparable(body), comparable(case.expected)
    if actual != expected:
        return label, case.name, f"expected={expected!r}; actual={actual!r}"
    return label, case.name, None


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", type=Path, required=True, help="exact Rust executable to check")
    parser.add_argument("--python-basic", type=Path, help="optional Python basic.py to check too")
    parser.add_argument("--python-exe", default=sys.executable, help="Python interpreter for basic.py")
    parser.add_argument("--fixtures", type=Path,
                        default=Path(__file__).resolve().parents[1] / "tests" / "block_if_gosub.rs")
    parser.add_argument("--jobs", type=int, default=4, choices=range(1, 9))
    parser.add_argument("--timeout", type=float, default=10)
    args = parser.parse_args()
    cases = cases_from_rust(args.fixtures.resolve(strict=True))
    runtimes = [("Rust", [str(args.rust.resolve(strict=True))])]
    if args.python_basic:
        runtimes.append(("Python", [args.python_exe, str(args.python_basic.resolve(strict=True))]))
    tasks = [(label, command, case, args.timeout) for label, command in runtimes for case in cases]
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(lambda task: check(*task), tasks))
    failures = 0
    for label, name, error in results:
        print(f"{'FAIL' if error else 'PASS'} [{label}] {name}" + (f": {error}" if error else ""))
        failures += error is not None
    for label, _ in runtimes:
        passed = sum(owner == label and error is None for owner, _, error in results)
        print(f"{label}: {passed}/{len(cases)} semantic cases passed")
    print(f"Total: {len(results) - failures}/{len(results)} checks passed; {len(cases)} shared semantic cases")
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
