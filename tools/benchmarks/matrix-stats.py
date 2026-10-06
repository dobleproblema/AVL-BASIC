"""Frozen AB/BA comparisons of numeric matrix reductions; standard library only.

Examples:
  py -3 tools/benchmarks/matrix-stats.py --baseline old.exe --candidate new.exe --output target/stats
  py -3 tools/benchmarks/matrix-stats.py --python --baseline old-basic.py --candidate basic.py --repeats 100 --output target/stats-python

Timers exclude initialization and output. Each reduction is warmed before its
timer. Complete numerical output, including positions, must match for both
implementations. No sample or runtime source is modified.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

TIMING = re.compile(r'^__STAT__([A-Z]+)\s+([0-9.eE+\-]+)\s*$', re.MULTILINE)
FUNCTIONS = ('SUM', 'ABSUM', 'AMAX', 'AMIN', 'MAXAB', 'FNORM', 'RNORM', 'CNORM', 'DOT')
CONTEXT = {'AMAX': 'AMAXROW;AMAXCOL', 'AMIN': 'AMINROW;AMINCOL',
           'MAXAB': 'MAXABROW;MAXABCOL', 'RNORM': 'RNORMROW', 'CNORM': 'CNORMCOL'}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def generate(shape: str, repeats: int) -> str:
    base = int(shape[-1])
    kind = shape[:-1]
    lines = [f'MAT BASE {base}']
    if kind == 'vector':
        last = 8191 + base
        lines += [f'DIM A({last}),B({last})', 'MAT A=1E100:MAT B=-1E100',
                  f'FOR I={base} TO {last}', 'A(I)=((I*17) MOD 97-48)/8',
                  'B(I)=((I*13) MOD 89-44)/16', 'NEXT I']
        functions = FUNCTIONS
    elif kind == 'matrix':
        row, col = 95 + base, 127 + base
        lines += [f'DIM A({row},{col})', 'MAT A=1E100',
                  f'FOR I={base} TO {row}:FOR J={base} TO {col}',
                  'A(I,J)=((I*17+J*13) MOD 97-48)/8', 'NEXT J:NEXT I']
        functions = FUNCTIONS[:-1]
    elif kind == 'column':
        last = 8191 + base
        lines += [f'DIM A({last},{base}),B({last})', 'MAT A=1E100:MAT B=-1E100',
                  f'FOR I={base} TO {last}', f'A(I,{base})=((I*17) MOD 97-48)/8',
                  'B(I)=((I*13) MOD 89-44)/16', 'NEXT I']
        functions = ('DOT',)
    elif kind == 'det':
        last = 15 + base
        lines += [f'DIM A({last},{last})', 'MAT A=1E100',
                  f'FOR I={base} TO {last}:FOR J={base} TO {last}',
                  'A(I,J)=((I*17+J*13) MOD 7-3)/8',
                  'IF I=J THEN A(I,J)=A(I,J)+8', 'NEXT J:NEXT I']
        functions = ('DET',)
    else:
        raise ValueError(shape)
    for function in functions:
        expression = f'{function}(A,B)' if function == 'DOT' else f'{function}(A)'
        lines += [f'FOR K=1 TO 5:R={expression}:NEXT K', 'T=TIME',
                  f'FOR K=1 TO {repeats}', f'R={expression}', 'NEXT K',
                  f'PRINT "__STAT__{function}";TIME-T', 'PRINT R']
        if function in CONTEXT:
            lines.append('PRINT ' + CONTEXT[function])
    lines.append('END')
    return ''.join(f'{10*(n+1)} {statement}\n' for n, statement in enumerate(lines))


def python_worker(module_path: Path, program: Path) -> None:
    spec = importlib.util.spec_from_file_location('matrix_stats_runtime', module_path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    interpreter = module.BasicInterpreter()
    interpreter.execute_immediate(f'LOAD "{program.name}"')
    interpreter.execute_immediate('RUN')


def run(runtime: Path, program: Path, python: bool, timeout: float) -> dict:
    command = ([sys.executable, str(Path(__file__).resolve()), '--worker', str(runtime), str(program)]
               if python else [str(runtime), str(program)])
    start = time.perf_counter()
    result = subprocess.run(command, cwd=program.parent, capture_output=True, timeout=timeout,
                            env={**os.environ, 'AVL_BASIC_WINDOW': '0', 'PYTHONIOENCODING': 'utf-8'})
    stdout = result.stdout.decode('utf-8', errors='replace').replace('\r\n', '\n')
    stderr = result.stderr.decode('utf-8', errors='replace')
    timings = dict((name, float(value)) for name, value in TIMING.findall(stdout))
    expected = (('DET',) if program.stem.startswith('det') else
                ('DOT',) if program.stem.startswith('column') else
                FUNCTIONS[:-1] if program.stem.startswith('matrix') else FUNCTIONS)
    if result.returncode or set(timings) != set(expected) or stderr:
        raise RuntimeError(f'{runtime}: {stdout}\n{stderr}')
    normalized = TIMING.sub(lambda m: f'__STAT__{m[1]} <time>', stdout)
    return {'wall_s': time.perf_counter()-start, 'times': timings,
            'correctness_sha256': digest(normalized.encode()), 'stdout': stdout}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--python', action='store_true')
    parser.add_argument('--repeats', type=int, default=2000)
    parser.add_argument('--runs', type=int, default=4)
    parser.add_argument('--warmups', type=int, default=1)
    parser.add_argument('--shapes', default='vector0,matrix1,column1,det0,det1')
    parser.add_argument('--timeout', type=float, default=180)
    args = parser.parse_args()
    if args.runs < 1 or args.repeats < 1 or args.warmups < 0:
        parser.error('runs/repeats must be positive and warmups nonnegative')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    runtimes = {'baseline': args.baseline.resolve(), 'candidate': args.candidate.resolve()}
    programs = {}
    for shape in args.shapes.split(','):
        path = output / (shape+'.bas')
        path.write_text(generate(shape, args.repeats), encoding='utf-8')
        programs[shape] = path
    report = {'runtime_kind': 'python' if args.python else 'native', 'configuration': vars(args).copy(),
              'runtimes': {n: {'path': str(p), 'sha256': digest(p.read_bytes())} for n,p in runtimes.items()},
              'programs': {n: digest(p.read_bytes()) for n,p in programs.items()}, 'runs': []}
    report['configuration'] = {k:str(v) if isinstance(v,Path) else v for k,v in report['configuration'].items()}
    signatures = {}
    for iteration in range(-args.warmups, args.runs):
        for shape, program in programs.items():
            for name in (('baseline','candidate') if iteration%2==0 else ('candidate','baseline')):
                result = run(runtimes[name], program, args.python, args.timeout)
                signature = result['correctness_sha256']
                if shape in signatures and signature != signatures[shape]:
                    raise AssertionError(f'Output changed: {shape} {name}')
                signatures[shape] = signature
                result.update(iteration=iteration, warmup=iteration<0, shape=shape, name=name)
                report['runs'].append(result)
                (output/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
                print(f'{iteration} {shape} {name}: {sum(result["times"].values()):.4f} s',flush=True)
    summary = {}
    for shape in programs:
        functions = report['runs'][next(i for i,r in enumerate(report['runs']) if r['shape']==shape)]['times']
        for function in functions:
            entry = {}
            for name in runtimes:
                samples = [r['times'][function] for r in report['runs'] if r['shape']==shape and r['name']==name and not r['warmup']]
                median = statistics.median(samples)
                entry[name] = {'median_s':median,'mad_s':statistics.median(abs(v-median) for v in samples)}
            entry['speedup'] = entry['baseline']['median_s']/entry['candidate']['median_s']
            summary[f'{shape}/{function}'] = entry
    report['summary'] = summary
    (output/'results.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    print(json.dumps(summary,indent=2))


if __name__ == '__main__':
    if len(sys.argv)>1 and sys.argv[1]=='--worker':
        python_worker(Path(sys.argv[2]),Path(sys.argv[3]))
    else:
        main()
