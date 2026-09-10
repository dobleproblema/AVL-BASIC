"""Check sequential-file behavior and exact bytes against both real runtimes.

Run with --python-repo PATH --rust-bin PATH (or AVL_BASIC_PY_REPO).
All programs and files live in fresh temporary virtual roots. No repository
samples or user data are opened for writing.
"""

from __future__ import annotations

import argparse
import math
import os
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field

from run_python_text_parity import normalize_rust_session_output


@dataclass
class Case:
    name: str
    commands: str
    inputs: dict[str, bytes] = field(default_factory=dict)
    files: dict[str, bytes] = field(default_factory=dict)
    output: str = "OK"
    linked_dirs: tuple[str, ...] = ()


def program(source: str, after: str = "") -> str:
    return source.strip() + "\nRUN\n" + after


def cases() -> list[Case]:
    result = [
        Case("unicode-records", program('''
10 N$="Ávila, 東京 "+CHR$(34)+"quoted"+CHR$(34)+CHR$(13)+CHR$(10)+"next"
20 OPEN "records.any" FOR OUTPUT AS #1
30 WRITE #1,N$,0.1,"",-0
40 CLOSE #1
50 OPEN "records.any" FOR INPUT AS #1
60 IF EOF(1) THEN PRINT "EARLY EOF":END
70 INPUT #1,A$,B,C$,D
80 IF A$<>N$ OR B<>0.1 OR C$<>"" OR D<>0 THEN PRINT "BAD DATA":END
90 IF EOF(1)=0 THEN PRINT "MISSING EOF":END
100 PRINT "OK"
'''), files={"records.any": '"Ávila, 東京 ""quoted""\r\nnext",0.10000000000000001,"",0\n'.encode()}),
        Case("bom-lines-and-physical-eof", program('''
10 OPEN "lines.txt" FOR INPUT AS #1
20 IF EOF(1) OR EOF(1) THEN PRINT "EARLY EOF":END
30 LINE INPUT #1,A$
40 LINE INPUT #1,B$
50 LINE INPUT #1,C$
60 LINE INPUT #1,D$
70 IF A$<>"first" OR B$<>"" OR C$<>"third" OR D$<>"last"+CHR$(26) THEN PRINT "BAD LINE":END
80 IF EOF(1) THEN PRINT "OK"
'''), inputs={"lines.txt": b"\xef\xbb\xbffirst\r\n\rthird\nlast\x1a"}),
        Case("empty-and-bom-only", program('''
10 OPEN "empty" FOR INPUT AS #1
20 OPEN "bom" FOR INPUT AS #2
30 IF EOF(1) AND EOF(2) THEN PRINT "OK"
'''), inputs={"empty": b"", "bom": b"\xef\xbb\xbf"}),
        Case("append-and-truncate", program('''
10 OPEN "out" FOR APPEND AS #1
20 PRINT #1,"tail"
30 CLOSE #1
40 OPEN "new" FOR APPEND AS #1
50 PRINT #1,"created";
60 CLOSE
70 OPEN "old" FOR OUTPUT AS #1
80 PRINT #1,"fresh"
90 PRINT "OK"
'''), inputs={"out": b"head", "old": b"obsolete"}, files={"out": b"headtail\n", "new": b"created", "old": b"fresh\n"}),
        Case("independent-print-columns", program('''
10 ZONE 8
20 OPEN "a" FOR OUTPUT AS #1
30 OPEN "b" FOR OUTPUT AS #2
40 PRINT #1,"abc";
50 PRINT #2,"x";
60 PRINT #1,TAB(6);"z"
70 PRINT #2,TAB(4);"y"
80 PRINT #1,USING "###.##";12.5
90 PRINT #2,"a","b"
100 PRINT "OK"
'''), files={"a": b"abc  z\n 12.50\n", "b": b"x  y\na       b\n"}),
        Case("arrays-and-channel-expressions", program('''
10 DIM A$(2),N(2):F=2:P$="array.csv"
20 OPEN P$ FOR OUTPUT AS #(F+1)
30 WRITE #F+1,"entry",12
40 CLOSE #(F+1)
50 OPEN P$ FOR INPUT AS #F
60 INPUT #F,A$(1),N(2)
70 IF A$(1)="entry" AND N(2)=12 THEN PRINT "OK"
'''), files={"array.csv": b'"entry",12\n'}),
        Case("quoted-fields-and-blank-record", program('''
10 OPEN "csv" FOR INPUT AS #1
20 INPUT #1,A$,B$,C,D$
30 IF A$<>" x " OR B$<>"a,b" OR C<>125 OR D$<>"say "+CHR$(34)+"hi"+CHR$(34) THEN PRINT "BAD CSV":END
40 INPUT #1,E$
50 IF E$="" AND EOF(1) THEN PRINT "OK"
'''), inputs={"csv": b'" x " ,"a,b", +1.25e2 ,"say ""hi"""\r\n\n'}),
        Case("stop-cont-keeps-channel", program('''
10 OPEN "stopped" FOR OUTPUT AS #1
20 PRINT #1,"before"
30 STOP
40 PRINT #1,"after"
50 PRINT "OK"
''', 'CONT\n'), files={"stopped": b"before\nafter\n"}, output="Line 30. Program stopped.\nOK"),
        Case("chain-keeps-channel-and-program-path", 'LOAD "sub/start.bas"\nRUN\n', inputs={
            "sub/start.bas": b'10 OPEN "data.csv" FOR OUTPUT AS #1\n20 WRITE #1,"before"\n30 CHAIN "next.bas"\n',
            "sub/next.bas": b'10 WRITE #1,"after"\n20 CLOSE #1\n30 OPEN "data.csv" FOR INPUT AS #1\n40 INPUT #1,A$\n50 INPUT #1,B$\n60 IF A$="before" AND B$="after" AND EOF(1) THEN PRINT "OK"\n',
        }, files={"sub/data.csv": b'"before"\n"after"\n'}),
        Case("end-closes-and-releases-number", program('''
10 OPEN "first" FOR OUTPUT AS #1
20 PRINT #1,"one"
30 END
''', 'OPEN "second" FOR OUTPUT AS #1\nPRINT #1,"two"\nCLOSE\nPRINT "OK"\n'), files={"first": b"one\n", "second": b"two\n"}),
        Case("clear-and-new-close", 'OPEN "first" FOR OUTPUT AS #1\nCLEAR\nOPEN "second" FOR OUTPUT AS #1\nNEW\nOPEN "third" FOR OUTPUT AS #1\nCLOSE #1,#1\nCLOSE\nPRINT "OK"\n', files={"first": b"", "second": b"", "third": b""}),
        Case("handled-errors-preserve-channel", program('''
10 OPEN "safe" FOR OUTPUT AS #1
20 ON ERROR GOTO 100
30 OPEN "safe" FOR OUTPUT AS #2
40 PRINT #1,"kept"
50 PRINT "OK":END
100 IF ERR<>57 OR ERL<>30 THEN PRINT "WRONG ERROR":END
110 RESUME NEXT
'''), inputs={"safe": b"before"}, files={"safe": b"kept\n"}),
        Case("open-after-cont-uses-program-directory", 'LOAD "sub/start.bas"\nRUN\nCONT\n', inputs={
            "sub/start.bas": b'10 STOP\n20 OPEN "data" FOR INPUT AS #1\n30 LINE INPUT #1,A$\n40 IF A$="subdirectory" THEN PRINT "OK"\n',
            "sub/data": b"subdirectory\n", "data": b"wrong directory\n",
        }, output="Line 10. Program stopped.\nOK"),
        Case("fresh-run-closes-stopped-channel", program('''
10 OPEN "restart" FOR APPEND AS #1
20 PRINT #1,"run"
30 STOP
40 PRINT "OK"
''', 'RUN\nCONT\n'), files={"restart": b"run\nrun\n"},
             output="Line 30. Program stopped.\nLine 30. Program stopped.\nOK"),
        Case("load-closes-immediate-channel", 'OPEN "old" FOR OUTPUT AS #1\nLOAD "loaded.bas"\nOPEN "new" FOR OUTPUT AS #1\nCLOSE\nPRINT "OK"\n',
             inputs={"loaded.bas": b'10 PRINT "LOADED"\n'}, files={"old": b"", "new": b""}),
        Case("close-list-validates-before-closing", program('''
10 OPEN "out" FOR OUTPUT AS #1
20 ON ERROR GOTO 100
30 CLOSE #1,#2
40 PRINT #1,"preserved"
50 PRINT "OK":END
100 IF ERR<>58 THEN PRINT "WRONG ERROR":END
110 RESUME NEXT
'''), files={"out": b"preserved\n"}),
        Case("input-validates-before-assigning", program('''
10 OPEN "data" FOR INPUT AS #1
20 A=7:B=8:ON ERROR GOTO 100
30 INPUT #1,A,B
40 IF A<>7 OR B<>8 THEN PRINT "PARTIAL ASSIGN":END
50 INPUT #1,A,B
60 IF A=11 AND B=12 THEN PRINT "OK"
70 END
100 IF ERR<>62 THEN PRINT "WRONG ERROR":END
110 RESUME NEXT
'''), inputs={"data": b"9,nonsense\n11,12\n"}),
        Case("independent-input-positions", program('''
10 OPEN "data" FOR INPUT AS #1
20 OPEN "data" FOR INPUT AS #2
30 INPUT #1,A
40 INPUT #1,B
50 INPUT #2,C
60 IF A=1 AND B=2 AND C=1 AND EOF(1) AND EOF(2)=0 THEN PRINT "OK"
'''), inputs={"data": b"1\n2\n"}),
        Case("subroutine-file-io", program('''
10 DEF SUB SAVEVALUE(P$,N)
20 OPEN P$ FOR OUTPUT AS #1
30 WRITE #1,N
40 CLOSE #1
50 SUBEND
60 CALL SAVEVALUE("subroutine",42)
70 OPEN "subroutine" FOR INPUT AS #1
80 INPUT #1,A
90 IF A=42 AND EOF(1) THEN PRINT "OK"
'''), files={"subroutine": b"42\n"}),
        Case("while-eof-is-live", program('''
10 OPEN "data" FOR INPUT AS #1
20 N=0
30 WHILE NOT EOF(1)
40 INPUT #1,A
50 N=N+A
60 WEND
70 IF N=6 THEN PRINT "OK"
'''), inputs={"data": b"1\n2\n3\n"}),
        Case("recover-malformed-record", program('''
10 OPEN "data" FOR INPUT AS #1
20 ON ERROR GOTO 100
30 INPUT #1,A$
40 INPUT #1,B$
50 IF B$="good" THEN PRINT "OK"
60 END
100 IF ERR<>62 THEN PRINT "WRONG ERROR":END
110 RESUME NEXT
'''), inputs={"data": b'"bad"junk\n"good"\n'}),
    ]
    errors = [
        ("unopened", 'INPUT #1,A$', {}, 58),
        ("fractional-channel", 'OPEN "x" FOR OUTPUT AS #1.5', {}, 26),
        ("zero-channel", 'OPEN "x" FOR OUTPUT AS #0', {}, 26),
        ("large-channel", 'OPEN "x" FOR OUTPUT AS #256', {}, 26),
        ("string-channel", 'OPEN "x" FOR OUTPUT AS #"1"', {}, 26),
        ("string-eof-channel", 'A=EOF("1")', {}, 26),
        ("root-escape", 'OPEN "../outside" FOR OUTPUT AS #1', {}, 26),
        ("absent-input", 'OPEN "missing" FOR INPUT AS #1', {}, 40),
        ("missing-parent", 'OPEN "absent/out" FOR OUTPUT AS #1', {}, 61),
        ("wrong-read-mode", 'OPEN "x" FOR OUTPUT AS #1:INPUT #1,A$', {}, 59),
        ("wrong-eof-mode", 'OPEN "x" FOR OUTPUT AS #1:A=EOF(1)', {}, 59),
        ("past-eof", 'OPEN "x" FOR INPUT AS #1:LINE INPUT #1,A$', {"x": b""}, 60),
        ("invalid-utf8", 'OPEN "x" FOR INPUT AS #1:LINE INPUT #1,A$', {"x": b"\xff\n"}, 62),
        ("unclosed-quote", 'OPEN "x" FOR INPUT AS #1:INPUT #1,A$', {"x": b'"unfinished'}, 62),
        ("quote-junk", 'OPEN "x" FOR INPUT AS #1:INPUT #1,A$', {"x": b'"a"x\n'}, 62),
        ("invalid-number", 'OPEN "x" FOR INPUT AS #1:INPUT #1,A', {"x": b'1+2\n'}, 62),
        ("unicode-number", 'OPEN "x" FOR INPUT AS #1:INPUT #1,A', {"x": '１２\n'.encode()}, 62),
        ("missing-field", 'OPEN "x" FOR INPUT AS #1:INPUT #1,A,B', {"x": b'1\n'}, 30),
        ("excess-field", 'OPEN "x" FOR INPUT AS #1:INPUT #1,A', {"x": b'1,2\n'}, 30),
        ("missing-input-target", 'OPEN "x" FOR INPUT AS #1:INPUT #1,', {"x": b''}, 15),
        ("missing-line-target", 'OPEN "x" FOR INPUT AS #1:LINE INPUT #1,', {"x": b''}, 15),
        ("excess-line-targets", 'OPEN "x" FOR INPUT AS #1:LINE INPUT #1,A$,B$', {"x": b''}, 31),
        ("write-mode-before-list", 'OPEN "x" FOR INPUT AS #1:WRITE #1,', {"x": b''}, 59),
        ("close-validates-left-to-right", 'CLOSE #2,#(1/0)', {}, 58),
        ("reserved-input-target", 'OPEN "x" FOR INPUT AS #1:INPUT #1,PRINT', {"x": b'1\n'}, 26),
        ("reserved-eof-target", 'OPEN "x" FOR INPUT AS #1:INPUT #1,EOF', {"x": b'1\n'}, 26),
    ]
    for name, statement, inputs, code in errors:
        result.append(Case(name, program(f'''10 ON ERROR GOTO 100
20 {statement}
30 PRINT "NO ERROR":END
100 IF ERR={code} THEN PRINT "OK" ELSE PRINT ERR
110 END'''), inputs=inputs))

    for target in ('A()', 'A(1,)', 'A(1+)'):
        result.append(Case('invalid-index-' + target, program(f'''10 DIM A(2)
20 OPEN "data" FOR INPUT AS #1
30 ON ERROR GOTO 100
40 INPUT #1,{target}
50 LINE INPUT #1,S$
60 IF S$="untouched" THEN PRINT "OK"
70 END
100 IF ERR<>15 THEN PRINT "WRONG ERROR":END
110 RESUME NEXT'''), inputs={"data": b"untouched\n"}))

    # Development builds expose samples through a Windows junction (or a Unix
    # symlink). Exercise the shipped programs with that real directory layout.
    sample_root = Path(__file__).resolve().parents[1] / 'samples'
    for sample, output, data_name, data in [
        ('f-scores', 'SAVED SCORES\nPLAYER ONE        1250\nPLAYER TWO        980\nPLAYER THREE      1420',
         'f-scores.csv', b'"PLAYER ONE",1250\n"PLAYER TWO",980\n"PLAYER THREE",1420\n'),
        ('f-text', 'SESSION REPORT\nTotal: 12.50\nStatus: complete',
         'f-text.txt', b'SESSION REPORT\nTotal: 12.50\nStatus: complete\n'),
        ('f-records', 'Text preserved: -1\nNumber preserved: -1\nEmpty field preserved: -1\nEnd of file: -1',
         'f-records.csv', b'"Hello, ""reader""\r\nSecond line",0.14285714285714285,""\n'),
    ]:
        result.append(Case('linked-samples-' + sample, f'LOAD "samples/{sample}.bas"\nRUN',
                           inputs={f'samples/{sample}.bas': (sample_root / (sample + '.bas')).read_bytes()},
                           files={'samples/' + data_name: data}, output=output,
                           linked_dirs=('samples',)))

    rng = random.Random(728451)
    numbers = [0.0, -0.0, 0.1, 1e-5, 1e-4, 1e16, 1e17, 1e100, 1e-100, 5e-324,
               sys.float_info.max, -sys.float_info.min]
    while len(numbers) < 180:
        value = struct.unpack('>d', rng.randbytes(8))[0]
        if math.isfinite(value):
            numbers.append(value)
    # Seed file bypasses the existing source formatter's 15-digit literal
    # normalization: this case tests file precision, including max f64.
    numeric_program = '''
10 OPEN "seed" FOR INPUT AS #1
20 OPEN "numbers" FOR OUTPUT AS #2
30 WHILE NOT EOF(1)
40 INPUT #1,N
50 WRITE #2,N
60 WEND
70 CLOSE
80 OPEN "seed" FOR INPUT AS #1
90 OPEN "numbers" FOR INPUT AS #2
100 WHILE NOT EOF(1)
110 INPUT #1,A
120 INPUT #2,B
130 IF A<>B THEN PRINT "ROUNDTRIP":END
140 WEND
150 IF EOF(2) THEN PRINT "OK"
'''
    expected = ''.join((format(v, '.17g') if v else '0') + '\n' for v in numbers).encode()
    seed = ''.join(repr(v) + '\n' for v in numbers).encode()
    result.append(Case("finite-number-roundtrips", program(numeric_program), inputs={"seed": seed}, files={"numbers": expected}))
    return result


def run_case(case: Case, command: list[str]) -> tuple[str, dict[str, bytes]]:
    with tempfile.TemporaryDirectory(prefix="avl-file-parity-") as folder:
        stage = Path(folder)
        root = stage / 'runtime'
        root.mkdir()
        for index, name in enumerate(case.linked_dirs):
            target = stage / f'linked-{index}'
            target.mkdir()
            alias = root / name
            if os.name == 'nt':
                subprocess.run(['cmd.exe', '/c', 'mklink', '/J', str(alias), str(target)],
                               capture_output=True, check=True)
            else:
                alias.symlink_to(target, target_is_directory=True)
        for name, contents in case.inputs.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
        env = {**os.environ, "PYTHONIOENCODING": "utf-8", "PYTHONUTF8": "1",
               "AVL_BASIC_PRINT_ZONE_DEFAULT": "8", "NO_COLOR": "1"}
        proc = subprocess.run(command, input=(case.commands.rstrip() + '\nQUIT\n').encode(),
                              cwd=root, env=env, capture_output=True, timeout=25)
        if proc.returncode:
            raise AssertionError(f"process exited {proc.returncode}: {proc.stderr.decode(errors='replace')}")
        output = normalize_rust_session_output(proc.stdout.decode('utf-8'))
        output = '\n'.join(line for line in output.splitlines() if line not in (
            'Saliendo del intérprete BASIC.', 'Secuencias de escape ANSI no soportadas.'))
        files = {p.relative_to(root).as_posix(): p.read_bytes()
                 for p in root.rglob('*') if p.is_file()}
        # Path.rglob does not follow directory symlinks on all platforms.
        for name in case.linked_dirs:
            alias = root / name
            files.update({name + '/' + p.relative_to(alias).as_posix(): p.read_bytes()
                          for p in alias.rglob('*') if p.is_file()})
        assert output == case.output, f"stdout expected {case.output!r}, got {output!r}; stderr={proc.stderr!r}"
        for name, expected in case.files.items():
            assert files.get(name) == expected, f"{name}: expected {expected!r}, got {files.get(name)!r}"
        return output, files


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--python-repo', default=os.environ.get('AVL_BASIC_PY_REPO'))
    parser.add_argument('--rust-bin', required=True, type=Path)
    parser.add_argument('--case', help='Only case names containing this text')
    args = parser.parse_args()
    if not args.python_repo:
        parser.error('--python-repo or AVL_BASIC_PY_REPO is required')
    python = [sys.executable, '-X', 'utf8', str(Path(args.python_repo).resolve() / 'basic.py')]
    rust = [str(args.rust_bin.resolve())]
    selected = [c for c in cases() if not args.case or args.case in c.name]
    failures = []
    for case in selected:
        try:
            py_result = run_case(case, python)
            rs_result = run_case(case, rust)
            assert py_result == rs_result, 'Python/Rust generated file trees differ'
            print(f'PASS {case.name}')
        except (AssertionError, subprocess.TimeoutExpired) as error:
            failures.append(case.name)
            print(f'FAIL {case.name}: {error}')
    print(f'{len(selected)-len(failures)}/{len(selected)} data-file parity cases passed')
    return 1 if failures else 0


if __name__ == '__main__':
    raise SystemExit(main())
