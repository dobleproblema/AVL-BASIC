"""Check shared MAT behavior and Rust's compound-expression extension.

Each numbered BASIC fixture runs in a fresh isolated virtual root. Assertions
live in BASIC and produce one stable CASE:<name>:OK marker. No benchmark timing,
random values, or runtime-specific numerical formatting appears in the output.

Run Rust alone with --rust PATH --rust-only. Normal mode takes --rust PATH and
--python PATH_TO_BASIC_PY_OR_ITS_REPOSITORY: shared fixtures run on both runtimes,
while Rust extension fixtures run on Rust only. Python keeps its existing MAT
expression grammar and is not expected to accept the new compound expressions.
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


def close(expression: str, expected: float) -> str:
    return check(f"ABS(({expression})-({expected:.17g}))<1E-10")


def success(name: str, statements: list[str]) -> Case:
    marker = f"CASE:{name}:OK"
    return Case(name, program(statements + [f'PRINT "{marker}"', "END"]), output=marker)


def vector_checks(name: str, expected: list[int | float]) -> list[str]:
    return [check(f"UBOUND({name})={len(expected)-1}")] + [
        close(f"{name}({i})", value) for i, value in enumerate(expected)
    ]


def matrix_checks(name: str, expected: list[list[int | float]]) -> list[str]:
    return [check(f"UBOUND({name},1)={len(expected)-1} AND UBOUND({name},2)={len(expected[0])-1}")] + [
        close(f"{name}({i},{j})", value)
        for i, row in enumerate(expected) for j, value in enumerate(row)
    ]


def rejected(name: str, setup: list[str], expression: str, code: int,
             unchanged: list[str]) -> Case:
    """Resume after a failed statement, then independently check atomicity."""
    prefix = setup + ["CAUGHT=0", "ON ERROR GOTO 9000", expression]
    error_line = len(prefix) * 10
    statements = prefix + ["ON ERROR GOTO 0", check("CAUGHT=1")] + unchanged
    marker = f"CASE:{name}:OK"
    source = program(statements + [f'PRINT "{marker}"', "END"]).removesuffix("RUN\n")
    source += f'9000 IF ERR<>{code} OR ERL<>{error_line} THEN PRINT "WRONG ERROR":END\n'
    source += "9010 CAUGHT=CAUGHT+1\n9020 RESUME NEXT\nRUN\n"
    return Case(name, source, output=marker)


def expression_cases() -> list[Case]:
    result: list[Case] = []
    vector = ["MAT BASE 0", "DIM A(2),B(2)", "MAT A=4", "MAT B=2"]
    matrix = ["MAT BASE 0", "DIM A(1,1),B(1,1),D(1,1)",
              "A(0,0)=1:A(0,1)=2:A(1,0)=3:A(1,1)=4",
              "B(0,0)=2:B(1,1)=3", "MAT D=1"]
    for name, expression, expected in [
        ("precedence-scale-plus", "A*2+3", 11),
        ("precedence-plus-scale", "3+A*2", 11),
        ("parentheses-plus-scale", "(A+3)*2", 14),
        ("subtraction-left-associative", "A*2-3-1", 4),
        ("division-left-associative", "A*8/4/2", 4),
        ("wrapped-scale-division", "(A*2)/4", 2),
        ("wrapped-scale-power", "(A*2)^3", 512),
        ("wrapped-array-division", "A/(2+2)", 1),
        ("scalar-over-array", "8/(A+4)", 1),
        ("element-as-scalar", "A+A(0)", 8),
        ("unary-power-precedence", "-A^2", -16),
        ("unary-power-parentheses", "(-A)^2", 16),
        ("negative-power", "A^-2", 0.0625),
        ("positive-unary", "+A^2", 16),
        ("power-left-associative", "2^A^2", 256),
    ]:
        result.append(success(name, vector + [f"MAT C={expression}"] + vector_checks("C", [expected]*3)))

    # Exercise both operand orders rather than only a commutative final value.
    for label, scalar, value in [("abs", "ABS(-1)", 1), ("min", "MIN(1,2)", 1),
                                 ("sqr", "SQR(4)", 2), ("compound", "ABS(-1)+COS(0)", 2)]:
        for side, expression in [("right", f"A+({scalar})"), ("left", f"({scalar})+A")]:
            result.append(success(f"scalar-function-{label}-{side}", vector + [f"MAT C={expression}"] +
                                  vector_checks("C", [4+value]*3)))
    result += [
        success("bare-abs-right", vector + ["MAT C=A+ABS(-1)"] + vector_checks("C", [5]*3)),
        success("bare-abs-left", vector + ["MAT C=ABS(-1)+A"] + vector_checks("C", [5]*3)),
        success("matrix-product-precedence", matrix + ["MAT C=D+A*B"] + matrix_checks("C", [[3,7],[7,13]])),
        success("matrix-product-parentheses", matrix + ["MAT C=(D+A)*B"] + matrix_checks("C", [[4,9],[8,15]])),
        success("matrix-product-self-destination", matrix + ["MAT A=A+B*A"] + matrix_checks("A", [[3,6],[12,16]])),
        success("scalar-only-fill", ["DIM C(2)", "MAT C=2^3^2+1"] + vector_checks("C", [65]*3)),
        success("variable-e-and-dye-minus", vector + ["E=3:DYE=5", "MAT C=A*(DYE-2)+E"] + vector_checks("C", [15]*3)),
        success("scientific-literal-signs", vector + ["MAT C=A*1E-2+1E+1"] + vector_checks("C", [10.04]*3)),
        success("scientific-literal-after-name", vector + ["DYE=5", "MAT C=A+DYE-2+1E-2"] + vector_checks("C", [7.01]*3)),
        success("indexed-numeric-vector-fill", vector + ["DIM C(2)", "MAT C=A(0)"] + vector_checks("C", [4]*3)),
        success("indexed-numeric-matrix-fill", matrix + ["DIM C(2)", "MAT C=A(0,1)"] + vector_checks("C", [2]*3)),
        success("indexed-numeric-self-fill", vector + ["MAT A=A(0)+1"] + vector_checks("A", [5]*3)),
        success("transpose-composite", matrix + ["MAT C=TRN(A+D)*2"] + matrix_checks("C", [[4,8],[6,10]])),
        success("transpose-nested", matrix + ["MAT C=TRN(TRN(A+D))+A"] + matrix_checks("C", [[3,5],[7,9]])),
        success("inverse-composite", matrix + ["MAT C=INV(A+A)*A"] + matrix_checks("C", [[0.5,0],[0,0.5]])),
        success("inverse-transpose-composite", ["MAT BASE 0", "DIM A(1,1)", "A(0,0)=2:A(1,1)=4",
                "MAT C=TRN(INV(A))*2"] + matrix_checks("C", [[1,0],[0,0.5]])),
    ]

    # Existing statistics remain scalar leaves. Their arguments keep the
    # pre-existing contract: this feature extends MAT assignments, not general
    # scalar expressions outside MAT.
    stats = ["MAT BASE 0", "DIM A(2),B(2)", "A(0)=1:A(1)=-2:A(2)=3", "B(0)=4:B(1)=5:B(2)=6"]
    result.append(success("statistics-as-mat-scalars", stats + ["MAT C=A+SUM(A)+AMAX(B)*2"] +
                          vector_checks("C", [15,12,17]) + [check("AMAXROW=2 AND AMAXCOL=0")]))
    result.append(success("norms-and-dot-as-mat-scalars", stats + ["MAT C=A+DOT(A,B)+ABSUM(A)+FNORM(A)-SQR(14)"] +
                          vector_checks("C", [19,16,21])))
    result.append(success("determinant-as-mat-scalar", matrix + [close("DET(A)", -2), "MAT C=A+DET(A)*2"] +
                          matrix_checks("C", [[-3,-2],[-1,0]])))

    result += [
        success("lazy-iif-omits-error", vector + ["MAT C=A+IIF(1,ABS(-1),1/0)"] + vector_checks("C", [5]*3)),
        success("lazy-iif-omits-side-effect", ["DEF FNSIDE()", "COUNT=COUNT+1", "FNSIDE=9", "FNEND"] +
                vector + ["COUNT=0", "MAT C=IIF(0,FNSIDE(),2)+A", check("COUNT=0")] + vector_checks("C", [6]*3)),
        success("scalar-functions-once-left-to-right", ["DEF FNNEXT()", "COUNT=COUNT+1", "FNNEXT=COUNT", "FNEND"] +
                vector + ["COUNT=0", "MAT C=A+FNNEXT()*FNNEXT()", check("COUNT=2")] + vector_checks("C", [6]*3)),
        success("scalar-array-name-coexistence", vector + ["A=100", "MAT C=A+ABS(A)"] + vector_checks("C", [104]*3)),
        success("empty-active-evaluates-functions-once", ["DEF FNNEXT()", "COUNT=COUNT+1", "FNNEXT=COUNT", "FNEND",
                "MAT BASE 1", "DIM A(0)", "MAT A=99", "COUNT=0", "MAT C=A+FNNEXT()+FNNEXT()", check("COUNT=2")] +
                vector_checks("C", [102])),
        success("random-leaves-evaluate-once", vector + ["RANDOMIZE 42", "X=RND:Y=RND:Z=RND", "RANDOMIZE 42",
                "MAT C=A*RND+RND", "Z2=RND", check("Z2=Z"), check("C(0)=4*X+Y AND C(1)=C(0) AND C(2)=C(0)")]),
        success("lazy-iif-omits-random", vector + ["RANDOMIZE 42", "X=RND", "RANDOMIZE 42", "MAT C=A+IIF(1,2,RND)",
                "Y=RND", check("X=Y")] + vector_checks("C", [6]*3)),
        success("array-functions-snapshot-order", [
            "DEF FNLEFT(X)", "ORDER=ORDER*10+1:A(0)=3", "MAT FNLEFT=A", "FNEND",
            "DEF FNRIGHT(X)", "ORDER=ORDER*10+2:A(0)=7", "MAT FNRIGHT=A", "FNEND",
            "DIM A(1):ORDER=0", "MAT C=FNLEFT(0)+FNRIGHT(0)*2", check("ORDER=12 AND A(0)=7"),
        ] + vector_checks("C", [17,0])),
        success("named-left-array-snapshot", [
            "DEF FNMUT()", "MAT A=9", "MAT FNMUT=A", "FNEND", "DIM A(2)", "MAT A=4",
            "MAT C=A+FNMUT()", check("A(0)=9 AND A(1)=9 AND A(2)=9"),
        ] + vector_checks("C", [13]*3)),
        success("array-parameter-scalar-function", [
            "DEF FNSCAL(X)", "COUNT=COUNT+1", "FNSCAL=SUM(X)", "FNEND", "DIM A(2)", "MAT A=2",
            "COUNT=0", "MAT C=FNSCAL(A)+A", check("COUNT=1"),
        ] + vector_checks("C", [8]*3)),
    ]

    for base in [0, 1]:
        result.append(success(f"active-base-{base}", [f"MAT BASE {base}", "DIM A(3)", "MAT A=4", "MAT C=A*2+1"] +
                              vector_checks("C", [9]*4)))
        result.append(success(f"vector-column-equivalence-base-{base}", [f"MAT BASE {base}", f"DIM A(2,{base}),B(2)",
                "MAT A=4", "MAT B=2", "MAT C=A+B*2"] +
                matrix_checks("C", [[8]*(base+1) for _ in range(3)])))

    result += [
        success("compound-text-concatenation", ["MAT BASE 0", "DIM A$(1),B$(1)",
                'A$(0)="Ávila":A$(1)="":B$(0)="!":B$(1)="."', 'MAT C$="["+A$+"]"+B$',
                check('UBOUND(C$)=1 AND C$(0)="[Ávila]!" AND C$(1)="[]."')]),
        success("compound-text-parentheses", ["MAT BASE 0", "DIM A$(1),B$(1)",
                'A$(0)="a":A$(1)="b":B$(0)="!":B$(1)="?"', 'MAT C$=(A$+B$)+"x"',
                check('UBOUND(C$)=1 AND C$(0)="a!x" AND C$(1)="b?x"')]),
        success("indexed-text-matrix-fill", ["MAT BASE 0", "DIM A$(1,2),C$(2)",
                'A$(1,2)="Ávila"', 'MAT C$=A$(1,2)',
                check('UBOUND(C$)=2 AND C$(0)="Ávila" AND C$(1)="Ávila" AND C$(2)="Ávila"')]),
        success("indexed-text-self-fill", ["MAT BASE 0", "DIM A$(1)",
                'A$(0)="a":A$(1)="b"', 'MAT A$=A$(0)+"!"',
                check('UBOUND(A$)=1 AND A$(0)="a!" AND A$(1)="a!"')]),
        success("subarray-copy-stays-available", ["MAT BASE 0", "DIM A(3),C(0)", "FOR I=0 TO 3", "A(I)=I+1", "NEXT I",
                "MAT C=A(1:2)"] + vector_checks("C", [2,3])),
    ]

    unchanged = vector_checks("C", [37,37])
    for name, setup, expression, code in [
        ("late-division-preserves-destination", ["DIM A(2),C(1)", "MAT A=4", "A(2)=0", "MAT C=37"], "MAT C=A+2/A", 6),
        ("intermediate-shape-preserves-destination", ["DIM A(2),B(3),C(1)", "MAT A=4", "MAT B=2", "MAT C=37"], "MAT C=A+B*2", 52),
        ("mixed-types-preserve-destination", ["DIM A(2),S$(2),C(1)", "MAT A=4", 'MAT S$="x"', "MAT C=37"], "MAT C=A+S$", 5),
        ("text-scaling-rejected", ["DIM A(2),S$(2),C(1)", "MAT A=4", 'MAT S$="x"', "MAT C=37"], "MAT C=A+S$*2", 37),
        ("slice-arithmetic-rejected", ["DIM A(3),C(1)", "MAT A=4", "MAT C=37"], "MAT C=A(0:1)+A(0:1)", 37),
        ("slice-scaled-expression-rejected", ["DIM A(3),C(1)", "MAT A=4", "MAT C=37"], "MAT C=A(0:1)*2+1", 37),
        ("selected-target-arithmetic-rejected", ["DIM A(1),C(1)", "MAT A=4", "MAT C=37"], "MAT C(0:1)=A+A", 26),
    ]:
        result.append(rejected(name, ["MAT BASE 0"] + setup, expression, code, unchanged))
    result.append(rejected("singular-intermediate-preserves-destination", ["MAT BASE 0", "DIM A(1,1),C(1)",
                           "MAT A=1", "MAT C=37"], "MAT C=INV(A+A)+2", 4, unchanged))
    return result


SHARED_EXPRESSION_CASES = frozenset({
    # Python already supports these numeric scalar subexpressions. Counting
    # operators is insufficient: only one array operation is performed here.
    "wrapped-array-division", "negative-power", "bare-abs-right", "bare-abs-left",
    "scalar-only-fill", "lazy-iif-omits-error", "lazy-iif-omits-side-effect",
    "scalar-functions-once-left-to-right", "lazy-iif-omits-random",
    "indexed-text-matrix-fill", "indexed-text-self-fill", "subarray-copy-stays-available",
    "mixed-types-preserve-destination", "slice-arithmetic-rejected", "slice-scaled-expression-rejected",
    *(f"scalar-function-{label}-{side}" for label in ("abs", "min", "sqr", "compound")
      for side in ("right", "left")),
})


def shared_cases() -> list[Case]:
    """Portable behavior supported by the unchanged Python interpreter."""
    result = [case for case in expression_cases() if case.name in SHARED_EXPRESSION_CASES]
    vector = ["MAT BASE 0", "DIM A(2),B(2)", "MAT A=4", "MAT B=2"]
    for name, expression, expected in [
        ("shared-array-copy", "A", 4),
        ("shared-array-plus-array", "A+B", 6),
        ("shared-array-minus-array", "A-B", 2),
        ("shared-array-minus-scalar", "A-3", 1),
        ("shared-scalar-minus-array", "3-A", -1),
        ("shared-array-scale", "A*3", 12),
        ("shared-scalar-scale", "3*A", 12),
        ("shared-array-divide", "A/2", 2),
        ("shared-array-power", "A^2", 16),
        ("shared-scalar-power", "2^A", 16),
        ("shared-array-unary-minus", "-A", -4),
        ("shared-complex-scalar-left", "(5-2+ABS(-1))*A", 16),
        ("shared-complex-scalar-right", "A*(5-2+ABS(-1))", 16),
    ]:
        result.append(success(name, vector + [f"MAT C={expression}"] + vector_checks("C", [expected]*3)))
    matrix = ["MAT BASE 0", "DIM A(1,1),B(1,1)",
              "A(0,0)=1:A(0,1)=2:A(1,0)=3:A(1,1)=4", "B(0,0)=2:B(1,1)=3"]
    result += [
        success("shared-matrix-product", matrix + ["MAT C=A*B"] + matrix_checks("C", [[2,6],[6,12]])),
        success("shared-transpose", matrix + ["MAT C=TRN(A)"] + matrix_checks("C", [[1,3],[2,4]])),
        success("shared-inverse", matrix + ["MAT C=INV(A)"] + matrix_checks("C", [[-2,1],[1.5,-0.5]])),
        success("shared-numeric-fill", ["DIM C(2)", "MAT C=7"] + vector_checks("C", [7]*3)),
        success("shared-text-single-concatenation", ["MAT BASE 0", "DIM A$(1)", 'A$(0)="a":A$(1)="b"',
                'MAT C$=A$+"!"', check('UBOUND(C$)=1 AND C$(0)="a!" AND C$(1)="b!"')]),
        success("shared-array-function-copy", ["DEF FNCOPY(X)", "MAT FNCOPY=X", "FNEND"] + vector +
                ["MAT C=FNCOPY(A)"] + vector_checks("C", [4]*3)),
    ]
    for base in [0, 1]:
        result.append(success(f"shared-identity-base-{base}", [f"MAT BASE {base}", "DIM A(2,2)", "MAT A=IDN"] +
                              matrix_checks("A", [[1 if i == j and i >= base else 0 for j in range(3)] for i in range(3)])))
        numeric = [f"MAT BASE {base}", f"DIM V(2),M(2,{base})", "V(0)=-1:V(1)=2:V(2)=3",
                   f"M(0,{base})=5:M(1,{base})=7:M(2,{base})=11"]
        if base == 1:
            numeric.append("M(0,0)=100:M(1,0)=200:M(2,0)=300")
        result.append(success(f"shared-vector-plus-column-base-{base}", numeric + ["MAT C=V+M"] +
                              vector_checks("C", [4,9,14])))
        expected = [[4],[9],[14]] if base == 0 else [[99,4],[202,9],[303,14]]
        result.append(success(f"shared-column-plus-vector-base-{base}", numeric + ["MAT C=M+V"] +
                              matrix_checks("C", expected)))
        transpose = [[5,7,11]] if base == 0 else [[0,0,0],[0,7,11]]
        result.append(success(f"shared-transpose-column-rank-base-{base}", numeric + ["MAT C=TRN(M)"] +
                              matrix_checks("C", transpose)))

        text = [f"MAT BASE {base}", f"DIM V$(2),M$(2,{base})"]
        for row in range(3):
            text.append(f'V$({row})="v{row}":M$({row},{base})="m{row}"')
            if base == 1:
                text.append(f'M$({row},0)="z{row}"')
        vector_text = [check("UBOUND(C$)=2")] + [check(f'C$({row})="v{row}m{row}"') for row in range(3)]
        result.append(success(f"shared-text-vector-plus-column-base-{base}", text + ["MAT C$=V$+M$"] + vector_text))
        column_text = [check(f"UBOUND(C$,1)=2 AND UBOUND(C$,2)={base}")]
        for row in range(3):
            column_text.append(check(f'C$({row},{base})="m{row}v{row}"'))
            if base == 1:
                column_text.append(check(f'C$({row},0)="z{row}v{row}"'))
        result.append(success(f"shared-text-column-plus-vector-base-{base}", text + ["MAT C$=M$+V$"] + column_text))

        product = [f"MAT BASE {base}", f"DIM A({base+1},{base+1}),B({base+1},{base})", "MAT A=999", "MAT B=999",
                   f"A({base},{base})=1:A({base},{base+1})=2:A({base+1},{base})=3:A({base+1},{base+1})=4",
                   f"B({base},{base})=5:B({base+1},{base})=6", "MAT C=A*B"]
        expected = [[17],[39]] if base == 0 else [[0,0],[0,17],[0,39]]
        result.append(success(f"shared-product-column-rank-base-{base}", product + matrix_checks("C", expected)))

    stored = [[11,4,4],[4,4,4],[4,4,4]]
    base_one = ["MAT BASE 1", "DIM A(2,2),B(2,2)", "MAT A=4", "MAT B=2", "A(0,0)=11"]
    for name, expression, expected in [
        ("shared-base-one-stored-unary", "-A", [[-value for value in row] for row in stored]),
        ("shared-base-one-stored-scale", "A*2", [[value*2 for value in row] for row in stored]),
        ("shared-base-one-stored-plus", "A+B", [[value+2 for value in row] for row in stored]),
    ]:
        result.append(success(name, base_one + [f"MAT C={expression}"] + matrix_checks("C", expected)))
    result += [
        success("shared-inverse-single-cell-rank", ["MAT BASE 0", "DIM A(0,0)", "A(0,0)=2", "MAT C=INV(A)"] +
                matrix_checks("C", [[0.5]])),
        success("shared-inverse-base-one-preserves-border", ["MAT BASE 1", "DIM A(2,2)", "MAT A=999",
                "A(1,1)=2:A(1,2)=0:A(2,1)=0:A(2,2)=4", "MAT C=INV(A)"] +
                matrix_checks("C", [[999,999,999],[999,0.5,0],[999,0,0.25]])),
        rejected("shared-base-one-stored-zero-divisor", ["MAT BASE 1", "DIM A(0),C(1)", "MAT A=99", "MAT C=37"],
                 "MAT C=A/0", 6, vector_checks("C", [37,37])),
    ]
    for function in ("TRN", "INV"):
        result.append(rejected(f"shared-{function.lower()}-rejects-vector", ["MAT BASE 0", "DIM A(2),C(1)", "MAT A=4", "MAT C=37"],
                               f"MAT C={function}(A)", 52, vector_checks("C", [37,37])))
    return result


def rust_extension_cases() -> list[Case]:
    """New Rust expression grammar plus its evaluation/error contracts."""
    return [case for case in expression_cases() if case.name not in SHARED_EXPRESSION_CASES]


def cases() -> list[Case]:
    """All fixtures, retained as a convenience for external runners."""
    return shared_cases() + rust_extension_cases()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rust", type=Path, help="Rust interpreter executable")
    parser.add_argument("--python", type=Path, default=os.environ.get("AVL_BASIC_PY_REPO"),
                        help="Python basic.py file or its repository")
    parser.add_argument("--rust-only", action="store_true", help="Validate shared behavior and extensions on Rust only")
    parser.add_argument("--case", help="Only case names containing this text")
    parser.add_argument("--dump", type=Path, help="Write numbered BASIC fixtures for review")
    args = parser.parse_args()
    groups = [("shared", shared_cases()), ("rust-extension", rust_extension_cases())]
    selected = [(group, [case for case in group_cases if not args.case or args.case in case.name])
                for group, group_cases in groups]
    if not any(group_cases for _, group_cases in selected):
        parser.error("No case names matched --case")
    if args.dump:
        args.dump.mkdir(parents=True, exist_ok=True)
        for group, group_cases in selected:
            folder = args.dump / group
            folder.mkdir(parents=True, exist_ok=True)
            for case in group_cases:
                (folder / (case.name + ".bas")).write_text(case.commands.removesuffix("RUN\n"), encoding="utf-8")
    if args.rust_only and not args.rust:
        parser.error("--rust-only requires --rust")
    if not args.rust:
        if args.dump:
            return 0
        parser.error("Specify --rust or --dump")
    commands = [("Rust", [str(args.rust.resolve())])]
    if not args.rust_only:
        if not args.python:
            parser.error("Specify --python, AVL_BASIC_PY_REPO, or --rust-only")
        entry = args.python.resolve()
        if entry.is_dir():
            entry /= "basic.py"
        commands.append(("Python", [sys.executable, "-X", "utf8", str(entry)]))
    total = failed = 0
    for group, group_cases in selected:
        active_commands = commands if group == "shared" else commands[:1]
        group_total = len(group_cases) * len(active_commands)
        group_failed = 0
        if group_cases:
            label = "Shared MAT behavior" if group == "shared" else "Rust MAT expression extension (Rust only)"
            print(f"{label}: {len(group_cases)} cases")
        for case in group_cases:
            outputs = []
            for name, command in active_commands:
                try:
                    outputs.append(run_case(case, command))
                    print(f"PASS {group} {name} {case.name}")
                except (AssertionError, OSError, subprocess.TimeoutExpired) as error:
                    group_failed += 1
                    print(f"FAIL {group} {name} {case.name}: {error}")
            if len(outputs) == 2 and outputs[0] != outputs[1]:
                group_failed += 1
                print(f"FAIL shared parity {case.name}: generated outputs or files differ")
        if group_cases:
            print(f"{group_total-group_failed}/{group_total} {group} runtime checks passed")
        total += group_total
        failed += group_failed
    print(f"{total-failed}/{total} MAT runtime checks passed (Python is tested on shared fixtures only)")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
