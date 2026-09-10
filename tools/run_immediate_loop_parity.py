"""Check prompt loop fixtures against both real runtimes and expected output."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

BEGIN = "__AVL_LOOPS_BEGIN__"
END = "__AVL_LOOPS_END__"
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def run_case(command: list[str], case: dict) -> list[str]:
    environment = {**os.environ, "PYTHONIOENCODING": "utf-8", "PYTHONUTF8": "1",
                   "NO_COLOR": "1", "AVL_BASIC_COLOR": "0", "AVL_BASIC_WINDOW": "0"}
    source = [f'PRINT "{BEGIN}"', *case['commands'], f'PRINT "{END}"', 'QUIT', '']
    with tempfile.TemporaryDirectory(prefix='avl-immediate-loops-') as folder:
        result = subprocess.run(command, input='\n'.join(source), capture_output=True,
                                text=True, encoding='utf-8', errors='replace',
                                cwd=folder, env=environment, timeout=15)
    if result.returncode:
        raise AssertionError(f'exit {result.returncode}: {result.stderr}')
    lines = []
    for line in ANSI.sub('', result.stdout).splitlines():
        while line.startswith('>> '):
            line = line[3:]
        lines.append(line)
    if BEGIN not in lines or END not in lines:
        raise AssertionError(f'incomplete prompt session: {result.stdout!r}')
    body = lines[lines.index(BEGIN) + 1:lines.index(END)]
    return [line for line in body if line != 'Ready']


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--python-repo', default=os.environ.get('AVL_BASIC_PY_REPO'))
    parser.add_argument('--rust-bin', required=True, type=Path)
    parser.add_argument('--case', default='', help='Filter case names')
    args = parser.parse_args()
    if not args.python_repo:
        parser.error('--python-repo or AVL_BASIC_PY_REPO is required')
    root = Path(__file__).resolve().parents[1]
    fixtures = json.loads((root / 'tests/fixtures/immediate_loops.json').read_text(encoding='utf-8'))
    cases = [case for case in fixtures if args.case in case['name']]
    runtimes = [('Python', [sys.executable, '-X', 'utf8', str(Path(args.python_repo).resolve() / 'basic.py')]),
                ('Rust', [str(args.rust_bin.resolve())])]
    failures = []
    for case in cases:
        for label, command in runtimes:
            try:
                actual = run_case(command, case)
                assert actual == case['output'], f"expected {case['output']!r}; actual {actual!r}"
                print(f"PASS [{label}] {case['name']}")
            except (AssertionError, subprocess.TimeoutExpired) as error:
                failures.append((label, case['name']))
                print(f"FAIL [{label}] {case['name']}: {error}")
    total = len(cases) * len(runtimes)
    print(f'{total - len(failures)}/{total} checks passed; {len(cases)} shared immediate-loop cases')
    return int(bool(failures))


if __name__ == '__main__':
    raise SystemExit(main())
