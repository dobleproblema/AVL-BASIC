"""Check HP-85 MAT subarray copies against both runtimes and explicit results.

Uses the existing isolated-session runner. The BASIC checks are independent of
either implementation and cover the nine examples on HP manual pages 42-43.
Numeric cell expressions preserve Python's legacy error; Rust's compound MAT
grammar fills the destination with the scalar instead. Both are checked.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys

from run_data_file_parity import Case, run_case


def program(statements: list[str]) -> str:
    return "\n".join(f"{10 * (i + 1)} {statement}" for i, statement in enumerate(statements)) + "\nRUN\n"


def check(expression: str) -> str:
    return f'IF NOT ({expression}) THEN PRINT "BAD RESULT":END'


def success(name: str, statements: list[str]) -> Case:
    return Case(name, program(statements + ['PRINT "OK"', 'END']))


def matrix_checks(name: str, expected: list[list[int]]) -> list[str]:
    return [check(f'UBOUND({name},1)={len(expected)-1} AND UBOUND({name},2)={len(expected[0])-1}')] + [
        check(f'{name}({i},{j})={value}')
        for i, row in enumerate(expected) for j, value in enumerate(row)
    ]


def vector_checks(name: str, expected: list[int]) -> list[str]:
    return [check(f'UBOUND({name})={len(expected)-1}')] + [
        check(f'{name}({i})={value}') for i, value in enumerate(expected)
    ]


def cases() -> list[Case]:
    common = [
        'MAT BASE 1', 'DIM A(5,5),B(5,5),C(5,5),D(5)',
        'FOR I=1 TO 5', 'D(I)=I', 'FOR J=1 TO 5',
        'A(I,J)=10*I+J', 'NEXT J', 'NEXT I', 'MAT B=ZER',
    ]
    zero = lambda: [[0] * 6 for _ in range(6)]
    result: list[Case] = []
    a = zero()
    for i in range(1, 6):
        for j in range(1, 6):
            a[i][j] = 10*i+j
    b = [[0] * 4 for _ in range(4)]
    for i in range(1, 4):
        for j in range(1, 4):
            b[i][j] = a[i][j]
    result += [success('hp-example-1-full-copy', common + ['MAT B=A'] + matrix_checks('B', a)),
               success('hp-example-2-block-rebase', common + ['MAT B=A(1:3,1:3)'] + matrix_checks('B', b))]
    for name, command, cells in [
        ('3-single-cell', 'MAT B(3,2)=D(3)', [(3, 2, 3)]),
        ('4-row-segment', 'MAT B(3,1:3)=A(1,2:4)', [(3, j, 11+j) for j in range(1, 4)]),
        ('5-column-segment', 'MAT B(2:3,5)=A(4:5,1)', [(2, 5, 41), (3, 5, 51)]),
        ('6-vector-into-row', 'MAT B(3,)=D', [(3, j, j) for j in range(1, 6)]),
        ('8-block-into-block', 'MAT B(2:3,1:4)=A(1:2,2:5)', [(i, j, 10*(i-1)+j+1) for i in range(2, 4) for j in range(1, 5)]),
    ]:
        b = zero()
        for i, j, value in cells:
            b[i][j] = value
        result.append(success('hp-example-' + name, common + [command] + matrix_checks('B', b)))
    result.append(success('hp-example-7-column-into-vector', common + ['MAT D=A(,2)'] + vector_checks('D', [0,12,22,32,42,52])))
    c = [[0]*4 for _ in range(3)]
    for i in range(1, 3):
        for j in range(1, 4):
            c[i][j] = 10*i+j+2
    result.append(success('hp-example-9-offset-block', common + ['MAT C=A(1:2,3:5)'] + matrix_checks('C', c)))

    result += [
        success('row-column-vector-bridge', common + ['MAT D=A(1,)', 'MAT B(,3)=D', 'MAT D=A(,4)', 'MAT B(2,)=D'] +
                [check('B(1,3)=11 AND B(5,3)=15 AND B(2,1)=14 AND B(2,5)=54 AND B(3,2)=0')]),
        success('vector-into-full-column-matrix', common + ['MAT B=D(2:4)'] + matrix_checks('B', [[0,0],[0,2],[0,3],[0,4]])),
        success('base-zero-offset-rebase', ['MAT BASE 0','DIM A(3,4),V(0)','FOR I=0 TO 3','FOR J=0 TO 4','A(I,J)=10*I+J','NEXT J','NEXT I',
                'MAT B=A(1:2,2:4)','MAT V=A(2,)'] + matrix_checks('B', [[12,13,14],[22,23,24]]) + vector_checks('V', [20,21,22,23,24])),
        success('base-one-explicit-zero', common + ['A(0,0)=99','MAT B=A(0:1,0:1)'] + matrix_checks('B', [[0,0,0],[0,99,0],[0,0,11]])),
        success('variable-expression-selectors', common + ['K=2','MAT C=A(K-1:K,MAX(1,K):K+2)'] + matrix_checks('C', [[0,0,0,0],[0,12,13,14],[0,22,23,24]])),
        success('overlap-forward-snapshot', ['DIM V(5)','FOR I=0 TO 5','V(I)=I','NEXT I','MAT V(1:5)=V(0:4)'] + vector_checks('V',[0,0,1,2,3,4])),
        success('overlap-backward-snapshot', ['DIM V(5)','FOR I=0 TO 5','V(I)=I','NEXT I','MAT V(0:4)=V(1:5)'] + vector_checks('V',[1,2,3,4,5,5])),
        success('overlap-full-resize-snapshot', ['DIM V(5)','FOR I=0 TO 5','V(I)=I','NEXT I','MAT V=V(2:4)'] + vector_checks('V',[2,3,4])),
        success('overlap-block-snapshot', common + ['MAT A(2:5,2:5)=A(1:4,1:4)'] + [check(f'A({i},{j})={10*(i-1)+j-1}') for i in range(2,6) for j in range(2,6)]),
        success('reverse-source-with-sentinels', ['DIM V(3)','FOR I=0 TO 3','V(I)=I+10','NEXT I','MAT W=V(4:-1)'] + vector_checks('W',[13,12,11,10])),
        success('reverse-target-with-sentinels', ['DIM V(3),W(3)','FOR I=0 TO 3','V(I)=I+10','NEXT I','MAT W(4:-1)=V'] + vector_checks('W',[13,12,11,10])),
        success('reverse-hp-matrix-example', common + ['MAT B(2:4,5:0)=A(6:2,2:5)'] + [check(f'B({i},{j})={10*(7-i)+6-j}') for i in range(2,5) for j in range(1,5)]),
        success('empty-selected-is-noop', common + ['MAT B=7','MAT B(1:0,2)=A(1:0,3)'] + matrix_checks('B',[[7]*6 for _ in range(6)])),
        success('empty-full-base-one', common + ['MAT B=A(1:0,2)','MAT C=A(,2:1)','MAT D=A(1:0,2)'] +
                [check('UBOUND(B,1)=0 AND UBOUND(B,2)=1 AND UBOUND(C,1)=5 AND UBOUND(C,2)=0 AND UBOUND(D)=0')]),
        success('explicit-single-cell-copy', ['DIM A(2),B(2,2)','B(1,2)=7','MAT A=B(1:1,2:2)'] + vector_checks('A',[7])),
        success('fractional-bounds-truncate', ['DIM A(3,3),B(3,3)','MAT A=3','A(1,1)=9','MAT B=7','MAT B(0:1.9,0:1.2)=A(0:1.8,0:1.1)'] +
                matrix_checks('B',[[3,3,7,7],[3,9,7,7],[7,7,7,7],[7,7,7,7]])),
        success('string-row-and-overlap', ['DIM A$(1,2),V$(0)','A$(0,0)="Ávila"','A$(0,1)=""','A$(0,2)="smoke"','MAT V$=A$(0,)','MAT A$(1,)=V$',
                'MAT V$(1:2)=V$(0:1)',check('A$(1,0)="Ávila" AND A$(1,1)="" AND A$(1,2)="smoke" AND V$(0)="Ávila" AND V$(1)="Ávila" AND V$(2)=""')]),
        success('zero-argument-functions-legacy', ['DEF FNSCAL()=7','DEF FNALL()','MAT FNALL=A','FNEND','DIM A(2),B(2)','MAT A=3',
                'MAT B=FNSCAL()',check('B(0)=7 AND B(1)=7 AND B(2)=7'),'MAT B=FNALL()',check('B(0)=3 AND B(1)=3 AND B(2)=3')]),
        success('array-function-into-selection', ['DEF FNCROP(A)','MAT FNCROP=A(1:3)','FNEND','DIM V(4),W(4)','FOR I=0 TO 4','V(I)=I+1','NEXT I',
                'MAT W=-1','MAT W(1:3)=FNCROP(V)'] + vector_checks('W',[-1,2,3,4,-1])),
        success('string-array-function-into-selection', ['DEF FNROW$()','MAT FNROW$=A$(1,)','FNEND','DIM A$(1,2),W$(3)',
                'A$(1,0)="zero"','A$(1,1)="one"','A$(1,2)="two"','MAT W$="keep"','MAT W$(1:3)=FNROW$()',
                check('W$(0)="keep" AND W$(1)="zero" AND W$(2)="one" AND W$(3)="two"')]),
    ]

    # Catch errors inside BASIC, then independently prove destination atomicity.
    errors = [
        ('numeric-cell-expression-legacy', 'MAT B=A(1,2)', 37),
        ('shape-mismatch', 'MAT B(0:1,0:1)=A(0:2,0:1)', 52),
        ('row-column-needs-vector', 'MAT B(0,0:2)=A(0:2,0)', 52),
        ('out-of-bounds-source', 'MAT B=A(0:6,0:1)', 34),
        ('out-of-bounds-target', 'MAT B(0:6,0:1)=A(0:6,0:1)', 34),
        ('negative-source', 'MAT B=A(-1:1,0:1)', 34),
        ('nonfinite-source', 'MAT B=A(0:INF,0:1)', 32),
        ('nonfinite-target', 'MAT B(0:INF,0:1)=A(0:1,0:1)', 32),
        ('wrong-source-rank', 'MAT B=A(0:1)', 52),
        ('wrong-target-rank', 'MAT B(0:1)=A(0:1,0:1)', 52),
        ('type-mismatch', 'MAT B=S$(0:1,0:1)', 5),
        ('empty-full-base-zero', 'MAT B=A(1:0,0:1)', 52),
        ('no-range-arithmetic', 'MAT B=A(0:1,0:1)+A(0:1,0:1)', 37),
        ('missing-upper-bound', 'MAT B=A(0:,0:1)', 15),
        ('missing-lower-bound', 'MAT B=A(:1,0:1)', 15),
        ('scalar-selected-destination', 'MAT B(0:1,0:1)=7', 26),
    ]
    for name, statement, code in errors:
        setup = ['MAT BASE 0','DIM A(5,5),B(2,2),S$(5,5)','MAT A=3','MAT B=7','ON ERROR GOTO 9990',statement,
                 'PRINT "MISSING ERROR"','END']
        source = program(setup).removesuffix('RUN\n')
        source += f'9980 IF ERR<>{code} THEN PRINT "WRONG ERROR";ERR:END\n9990 ON ERROR GOTO 0\n'
        source = source.replace('ON ERROR GOTO 9990', 'ON ERROR GOTO 9980')
        checks = matrix_checks('B',[[7]*3 for _ in range(3)]) + ['PRINT "OK"','END']
        source += '\n'.join(f'{10000+i*10} {line}' for i,line in enumerate(checks)) + '\nRUN\n'
        result.append(Case('error-preserves-' + name,source))
    return result


def runtime_case(case: Case, runtime: str) -> Case:
    if runtime == 'Rust' and case.name == 'error-preserves-numeric-cell-expression-legacy':
        return success('numeric-cell-expression-scalar-rust', [
            'MAT BASE 0', 'DIM A(2,3),B(1,1)', 'MAT A=4', 'MAT B=9', 'MAT B=A(1,2)',
        ] + matrix_checks('B', [[4,4],[4,4]]))
    return case


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--python-repo',default=os.environ.get('AVL_BASIC_PY_REPO'))
    parser.add_argument('--rust-bin',type=Path)
    parser.add_argument('--case',help='Only case names containing this text')
    parser.add_argument('--dump',type=Path,help='Write numbered BASIC cases for review')
    args = parser.parse_args()
    selected = [c for c in cases() if not args.case or args.case in c.name]
    if args.dump:
        args.dump.mkdir(parents=True,exist_ok=True)
        for case in selected:
            (args.dump/(case.name+'.bas')).write_text(case.commands.removesuffix('RUN\n'),encoding='utf-8')
    commands = []
    if args.python_repo:
        commands.append(('Python',[sys.executable,'-X','utf8',str(Path(args.python_repo).resolve()/'basic.py')]))
    if args.rust_bin:
        commands.append(('Rust',[str(args.rust_bin.resolve())]))
    if not commands:
        if args.dump:
            return 0
        parser.error('Specify --python-repo, --rust-bin, or --dump')
    failed = 0
    for case in selected:
        for name,command in commands:
            active_case = runtime_case(case, name)
            try:
                run_case(active_case,command)
                print(f'PASS {name} {active_case.name}')
            except (AssertionError,subprocess.TimeoutExpired) as error:
                failed += 1
                print(f'FAIL {name} {active_case.name}: {error}')
    total = len(selected)*len(commands)
    print(f'{total-failed}/{total} MAT subarray runtime checks passed')
    return bool(failed)


if __name__=='__main__':
    raise SystemExit(main())
