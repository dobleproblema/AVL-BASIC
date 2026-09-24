#!/usr/bin/env python3
"""Execute fenced BASIC manual examples in isolated package directories.

Python's standard library is sufficient. Native graphics and audio are disabled;
graphics commands still render into the real interpreter's pixel buffer. Keyboard
loops are bounded and mouse handlers receive synthetic calls, reported explicitly
in the results. This is not a test of physical input, display timing, or speakers.

Usage: python tools/check_manual_examples.py --report manual-check.json
"""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time


@dataclass(frozen=True)
class Example:
    manual: Path
    section: str
    ordinal: int
    line: int
    source: str

    @property
    def key(self) -> str:
        return f"s{self.section}-e{self.ordinal}"


@dataclass(frozen=True)
class Scenario:
    name: str
    source: str
    adaptation: str = "none; executes the published program unchanged"
    expected_exit: int = 0
    expected_stdout: str | None = None
    expected_stderr: str = ""
    contains: str | None = None


def extract_examples(path: Path) -> list[Example]:
    examples = []
    section = "0"
    counts: dict[str, int] = {}
    fence = None
    start = 0
    body: list[str] = []
    for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
        if line.startswith("```"):
            if fence is None:
                fence = line[3:].strip().lower()
                start, body = number + 1, []
            else:
                if fence == "basic":
                    counts[section] = counts.get(section, 0) + 1
                    examples.append(Example(path, section, counts[section], start,
                                            "\n".join(body).strip() + "\n"))
                fence = None
            continue
        if fence is not None:
            body.append(line)
            continue
        # The period is mandatory: rows such as "15 mediumpurple 31 ivory"
        # in the palette table are not section headings.
        heading = re.match(r"^(\d+\.\d*)\s+\S", line)
        if heading:
            section = heading[1].rstrip(".")
    if fence is not None:
        raise ValueError(f"{path}:{start}: unclosed code fence")
    if not examples:
        raise ValueError(f"{path}: no fenced basic examples found")
    return examples


def executable_text(line: str) -> str:
    """Mask strings, comments, and DATA fields before scanning literal jumps."""
    result = []
    quoted = data = False
    index = 0
    while index < len(line):
        char = line[index]
        if char == '"':
            quoted = not quoted
            result.append(" ")
            index += 1
            continue
        if quoted:
            result.append(" ")
            index += 1
            continue
        if char == "'":
            break
        if char == ":":
            data = False
        word = re.match(r"[A-Za-z][A-Za-z0-9_$]*", line[index:])
        if word and (index == 0 or not (line[index - 1].isalnum() or line[index - 1] == "_")):
            token = word[0]
            if token.upper() == "REM":
                break
            if token.upper() == "DATA":
                data = True
            result.append(" " * len(token) if data else token)
            index += len(token)
            continue
        result.append(" " if data else char)
        index += 1
    return "".join(result)


def check_lines(example: Example) -> list[str]:
    numbered: dict[int, str] = {}
    problems = []
    previous = 0
    for raw in example.source.splitlines():
        match = re.fullmatch(r"(\d+)\s+(.+)", raw)
        if not match:
            problems.append(f"not a numbered program line: {raw!r}")
            continue
        number = int(match[1])
        if number <= previous:
            problems.append(f"duplicate or unordered line {number}")
        previous = number
        numbered[number] = executable_text(match[2])
    for number, code in numbered.items():
        for match in re.finditer(r"\b(GOTO|GOSUB|THEN|ELSE|RESTORE|RESUME)\s+(\d+(?:\s*,\s*\d+)*)\b", code, re.I):
            for target in map(int, re.findall(r"\d+", match[2])):
                prefix = code[:match.start()].upper()
                if target == 0 and ("ON ERROR" in prefix or "ON MOUSE" in prefix):
                    continue
                # Section 8.2 deliberately demonstrates an unhandled missing jump.
                if example.key == "s8.2-e1" and number == 40 and target == 100:
                    continue
                if target not in numbered:
                    problems.append(f"line {number}: {match[1]} refers to missing line {target}")
    return problems


def replace_line(source: str, number: int, statement: str) -> str:
    updated, count = re.subn(rf"^{number}\s+.*$", f"{number} {statement}", source, flags=re.M)
    if count != 1:
        raise ValueError(f"expected one line {number} for validation adaptation, found {count}")
    return updated


def scenarios(example: Example) -> list[Scenario]:
    source, key = example.source, example.key
    if key == "s6.7-e1":
        updated, count = re.subn(r"\bPAUSE\s*(?=(?:'.*)?$)", "PAUSE 1", source, flags=re.M)
        if count != 1:
            raise ValueError("sprite transparency example must have one final key wait")
        return [Scenario("timed-final-wait", updated, "replace final indefinite PAUSE by PAUSE 1")]
    if key == "s6.7-e2":
        statement = "FRAME 60 : N=N+1 : IF N>=180 THEN SCREEN CLOSE : END ELSE GOTO 230"
        return [Scenario("180-frames", replace_line(source, 510, statement),
                         "stop animation after 180 frames; all collision/background/sprite code retained")]
    if example.section == "6.8":
        updated = replace_line(source, 30, "GOSUB 35 : GOSUB 45 : GOTO 60")
        return [Scenario("synthetic-mouse-handlers", updated,
                         "call press and release/drag handlers once, then exit; no physical mouse events")]
    if example.section == "10":
        updated, count = re.subn(r"FRAME\s+60\s*:\s*GOTO\s+20", "FRAME 60 : N=N+1 : IF N<3 THEN GOTO 20", source, flags=re.I)
        if count != 1:
            raise ValueError("keyboard example must contain one FRAME 60 : GOTO 20 loop")
        updated += '120 PRINT "CHECK_X=";X : SCREEN CLOSE : END\n'
        result = [Scenario("three-idle-frames", updated,
                           "stop after three frames without key input and check the initial position",
                           expected_stdout="CHECK_X= 320\n")]
        if key == "s10-e1":
            synthetic, replaced = re.subn(r"\bINKEY\$", "KTEST$", source)
            if replaced != 1 or "\n20 " not in synthetic:
                raise ValueError("discrete-key example must read INKEY$ once on line 20")
            synthetic = synthetic.replace("\n20 ", '\n15 N=N+1 : KTEST$="" : IF N=1 THEN KTEST$=CHR$(28)\n20 ', 1)
            synthetic = re.sub(r"FRAME\s+60\s*:\s*GOTO\s+20", "FRAME 60 : IF N<3 THEN GOTO 15", synthetic, flags=re.I)
            synthetic += '120 PRINT "CHECK_X=";X : SCREEN CLOSE : END\n'
            result.append(Scenario("one-left-key-then-idle", synthetic,
                                   "supply one left-arrow character followed by two empty key reads; original key processing retained",
                                   expected_stdout="CHECK_X= 312\n"))
        return result
    if key == "s8.2-e1":
        return [Scenario("intentional-unhandled-error", source, expected_exit=1,
                         expected_stdout="Error 200 occurred on line 20\n",
                         expected_stderr="Line 40. Target line does not exist.\n")]
    if example.section == "7":
        # Fix the generated value only in this scenario, so timer/handler behavior
        # is tested without depending on a particular pseudo-random sequence.
        seeded = replace_line(source, 90, "RANDOMIZE 0")
        success = replace_line(seeded, 100, 'DEF FNSOL$="a"')
        timeout = replace_line(seeded, 110, 's$=FNSOL$ : s$="!"')
        return [Scenario("published-random-game", source),
                Scenario("guess-success", success,
                         "replace FNSOL$ by constant lowercase a so the generated guess matches", contains="is correct. I win!"),
                Scenario("timer-expiry", timeout,
                         "seed RNG and choose a non-letter target to force timer expiry", contains="Too late. I still win!")]
    expected = {
        "s3-e1": "OK\n",
        "s4.1-e1": "det=16\n\n 14   18\n 38   50\n\n 14   38\n 18   50\n\n 1   0\n 0   1\n",
        "s5.2-e1": "PLAYER ONE:  1250\nPLAYER TWO:  980\n",
        "s8-e1": "[10][20][30][40] 1  10  20\n[50][60][30][40] 2  20  30\n[50][60][70]\n",
        "s11-e1": " 31.415926535898\n",
        "s11-e2": "Hello AVL\n",
        "s11-e3": " 120\n",
        "s11-e4": " 8\n 3\n",
        "s11-e5": " 3   3\n 3   3\n",
        "s11-e6": " 5\n 4\n 9\n",
    }.get(key)
    result = [Scenario("published", source, expected_stdout=expected)]
    if key == "s3-e1":
        result.extend([
            Scenario("zero-branch", replace_line(source, 10, "V=0"), "set V=0 to exercise ELSEIF", expected_stdout="no data\n"),
            Scenario("else-branch", replace_line(source, 10, "V=25"), "set V=25 to exercise ELSE", expected_stdout="value 25\n"),
        ])
    return result


def run_scenario(exe: Path, package: Path, output: Path, example: Example,
                 scenario: Scenario, timeout: float) -> dict:
    directory = output / example.manual.stem / example.key / scenario.name
    directory.mkdir(parents=True)
    # Copy only the named assets; avoid duplicating the entire audio/image tree
    # for every small numeric example. Computed asset paths fall back to a copy
    # of the whole assets folder. No writes can reach the actual package.
    assets = directory / "samples" / "assets"
    assets.mkdir(parents=True)
    names = {name for name in re.findall(r'"([^"\n]*)"', scenario.source)
             if name.startswith("samples/assets/")}
    for name in sorted(names):
        relative = Path(name)
        source_asset = package / relative
        if ".." in relative.parts:
            raise ValueError(f"invalid asset path in example: {name}")
        if not source_asset.is_file():
            shutil.copytree(package / "samples" / "assets", assets, dirs_exist_ok=True)
            break
        destination = directory / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source_asset, destination)
    (directory / "example.bas").write_text(scenario.source, encoding="utf-8")
    record = {"manual": example.manual.name, "example": example.key, "line": example.line,
              "scenario": scenario.name, "adaptation": scenario.adaptation,
              "published_sha256": hashlib.sha256(example.source.encode()).hexdigest()}
    env = dict(os.environ, AVL_BASIC_WINDOW="0", AVL_BASIC_AUDIO="0", NO_COLOR="1")
    env.pop("AVL_BASIC_PRINT_ZONE_DEFAULT", None)
    start = time.monotonic()
    try:
        process = subprocess.run([str(exe), "example.bas"], cwd=directory, env=env,
                                 input="", capture_output=True, text=True, encoding="utf-8",
                                 errors="replace", timeout=timeout)
        record.update(exit=process.returncode, stdout=process.stdout, stderr=process.stderr)
        problems = []
        if process.returncode != scenario.expected_exit:
            problems.append(f"expected exit {scenario.expected_exit}, got {process.returncode}")
        if process.stderr != scenario.expected_stderr:
            problems.append("unexpected stderr")
        if scenario.expected_stdout is not None and process.stdout != scenario.expected_stdout:
            problems.append(f"expected stdout {scenario.expected_stdout!r}")
        if scenario.contains and scenario.contains not in process.stdout:
            problems.append(f"stdout lacks {scenario.contains!r}")
        if example.key == "s6.6-e1":
            fps = re.fullmatch(r"\s*([\d.eE+-]+)\s+fps\s*", process.stdout)
            if not fps or float(fps[1]) <= 0:
                problems.append("image restoration benchmark did not report positive fps")
        if example.key == "s5.2-e1":
            expected_bytes = b'"PLAYER ONE",1250\n"PLAYER TWO",980\n'
            score_file = directory / "scores.csv"
            if not score_file.exists() or score_file.read_bytes() != expected_bytes:
                problems.append("scores.csv does not contain the expected UTF-8/LF records")
        record.update(ok=not problems, problems=problems)
    except subprocess.TimeoutExpired:
        record.update(ok=False, problems=[f"exceeded {timeout:g} seconds"])
    record["seconds"] = round(time.monotonic() - start, 3)
    return record


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, default=root / "target" / "release" / ("avl-basic.exe" if os.name == "nt" else "avl-basic"))
    parser.add_argument("--manual", type=Path, action="append", help="repeat to select manuals; defaults to EN and ES")
    parser.add_argument("--package-root", type=Path, default=root, help="directory containing samples/assets")
    parser.add_argument("--only", action="append", help="stable example ID such as s6.7-e2; repeat as needed")
    parser.add_argument("--timeout", type=float, default=15)
    parser.add_argument("--jobs", type=int, default=2)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--keep-temp", action="store_true")
    args = parser.parse_args()
    if args.timeout <= 0 or args.jobs < 1:
        parser.error("timeout and jobs must be positive")
    exe = args.exe.resolve()
    if not exe.is_file():
        parser.error(f"interpreter not found: {exe}; build with cargo build --release")
    manuals = [path.resolve() for path in (args.manual or [root / "MANUAL.txt", root / "MANUAL.es.txt"])]
    all_examples = [example for path in manuals for example in extract_examples(path)]
    problems = []
    if len(manuals) == 2:
        versions = [{e.key: e.source for e in all_examples if e.manual == path} for path in manuals]
        if versions[0] != versions[1]:
            problems.append("English and Spanish runnable example IDs or source differ")
    examples = [e for e in all_examples if not args.only or e.key in args.only]
    if not examples:
        problems.append("no selected examples")
    for example in examples:
        problems.extend(f"{example.manual.name}:{example.line} {example.key}: {p}" for p in check_lines(example))
    if problems:
        print("\n".join(problems))
        return 1
    jobs = [(e, scenario) for e in examples for scenario in scenarios(e)]
    temp_root = Path(tempfile.gettempdir()).resolve()
    temporary = Path(tempfile.mkdtemp(prefix="avl-manual-examples-", dir=temp_root)).resolve()
    records = []
    try:
        def run(job):
            return run_scenario(exe, args.package_root.resolve(), temporary, *job, args.timeout)
        with ThreadPoolExecutor(max_workers=args.jobs) as executor:
            for result in executor.map(run, jobs):
                records.append(result)
                state = "PASS" if result["ok"] else "FAIL"
                print(f"{state} {result['manual']} {result['example']} {result['scenario']} ({result['seconds']:.3f}s)", flush=True)
                if not result["ok"]:
                    print(json.dumps(result, ensure_ascii=True), flush=True)
        report = {"mode": "headless; silent audio; explicitly reported input/wait adaptations",
                  "examples": len(examples), "scenarios": len(records),
                  "passed": sum(r["ok"] for r in records), "results": records}
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"{report['passed']}/{len(records)} scenarios passed across {len(examples)} published examples.")
        return 0 if all(r["ok"] for r in records) else 1
    finally:
        if args.keep_temp:
            print(f"Kept test package directories: {temporary}")
        else:
            if temporary.parent != temp_root or not temporary.name.startswith("avl-manual-examples-"):
                raise RuntimeError(f"refusing to remove unexpected temporary directory: {temporary}")
            shutil.rmtree(temporary)


if __name__ == "__main__":
    raise SystemExit(main())
