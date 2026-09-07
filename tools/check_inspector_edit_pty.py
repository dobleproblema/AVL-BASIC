#!/usr/bin/env python3
"""Verify inline debugger edits through a real Linux/WSL PTY (stdlib only).

Run beside check_inspector_pty.py, passing the exact Linux executable to test.
Programs live only in the interpreter's memory; no BASIC files are written.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import sys
import time

from check_inspector_pty import (
    BACK_TAB, DOWN, END, ENTER_ALT, ESC, F1, F2, F4, F5, F6, F7, F8,
    HOME, LEAVE_ALT, LEFT, PAGE_DOWN, PAGE_UP, RIGHT, PtySession,
    assert_heading, assert_value, last_frame, plain,
)


def editing(frame: str) -> None:
    assert "Esc Cancel" in frame, frame[-700:]


def timer_remaining(frame: str) -> int:
    match = re.search(r"#1 AFTER -> 900 active (\d+)/(\d+)ms", frame)
    assert match, frame[-2000:]
    assert int(match.group(2)) == 1000, match.group(0)
    return int(match.group(1))


def assert_pause(frame: str) -> None:
    # The compact EDIT footer replaces the normal location label. The source
    # execution marker and MAIN frame still identify the pending instruction.
    assert re.search(r"70\s+▶\s+A=A\+1", frame), frame[-2000:]
    assert "0: MAIN @70" in frame, frame[-2000:]
    assert "ERR=0 ERL=0" in frame, frame[-2000:]


def run(binary: Path, theme: str, no_color: bool) -> list[str]:
    terminal = PtySession(binary, theme, no_color)
    passed = []
    try:
        terminal.wait_for("Ready")
        for line in (
            "10 ON ERROR GOTO 800", "20 A=2", '30 S$="OLD"', "40 DIM M(2)",
            "45 M(2)=9", "50 M(1)=7", "60 AFTER 50,1 GOSUB 900", "70 A=A+1",
            "80 K$=INKEY$",
            '90 PRINT "EDITRESULT=";A;":LEN=";LEN(S$);":CTRL=";ASC(MID$(S$,7,1));":KEY=";LEN(K$)',
            '95 PRINT "EDITARRAY=";M(1);":";M(2)', "100 END",
            '800 PRINT "BADERROR":RESUME NEXT', '900 PRINT "BADTIMER":RETURN',
        ):
            terminal.command(line)
        terminal.send(b"DEBUG\r")
        terminal.wait_for("Ln 10 Stmt 1")
        for line in (20, 30, 40, 45, 50, 60, 70):
            terminal.send(F6)
            frame = last_frame(terminal.wait_for(f"Ln {line} Stmt 1"))
        baseline_timer = timer_remaining(frame)
        editing_started = time.monotonic()

        # Pin A, S$ and the currently displayed M(1). Their insertion order is
        # intentionally different from any inferred expression evaluation.
        terminal.keys(b"\t" + HOME + DOWN + DOWN)
        terminal.keys(b" ")
        terminal.keys(DOWN + b" ")
        frame = terminal.keys(DOWN + b" ")
        assert_heading(frame, "PINNED", 3)
        terminal.keys(HOME + DOWN)
        frame = terminal.keys(b"\r")
        editing(frame)
        frame = terminal.keys(b"12.5")
        editing(frame)
        assert_value(frame, "A", "2")  # VARIABLES retains the committed value.

        ignored_start = len(terminal.transcript)
        for key in (b"\t", BACK_TAB, F1, F2, F4, F5, F6, F7, F8, PAGE_UP, PAGE_DOWN):
            terminal.keys(key)
        ignored = bytes(terminal.transcript[ignored_start:])
        assert ENTER_ALT not in ignored and LEAVE_ALT not in ignored, ignored
        frame = terminal.keys(b"\r")
        assert_pause(frame)
        assert_heading(frame, "PINNED", 3)
        assert_value(frame, "A", "12.5", occurrences=2)
        assert "Esc Cancel" not in frame, frame[-700:]
        passed.append("Numeric edit replaces the selection, commits both lists in place and ignores execution/focus keys")

        for invalid in (b"RND", b"1e999"):
            terminal.keys(b"\r")
            frame = terminal.keys(invalid + b"\r")
            editing(frame)
            assert_pause(frame)
            assert_value(frame, "A", "12.5")
            frame = terminal.keys(ESC)
            assert_pause(frame)
            assert_value(frame, "A", "12.5", occurrences=2)
        terminal.keys(b"\r")
        terminal.keys(b"999")
        frame = terminal.keys(ESC)
        assert_value(frame, "A", "12.5", occurrences=2)
        passed.append("Invalid/non-finite literals leave BASIC and ON ERROR unchanged; Esc cancels without aborting")

        # Select S$ in VARIABLES, exercising editing from the other copy.
        terminal.keys(HOME + DOWN * 6)
        frame = terminal.keys(b"\r")
        editing(frame)
        string_input = b'A "B"\\n\\x1b[31m'
        commit_start = len(terminal.transcript)
        terminal.keys(string_input)
        frame = terminal.keys(b"\r")
        escaped_value = '"A ""B""\\n\\x1B[31m"'
        assert_pause(frame)
        assert_heading(frame, "PINNED", 3)
        assert_value(frame, "S$", escaped_value, occurrences=2)
        assert b"\x1b[31m" not in bytes(terminal.transcript[commit_start:])
        # Reopening and accepting the text must preserve every decoded control.
        terminal.keys(b"\r")
        frame = terminal.keys(b"\r")
        assert_value(frame, "S$", escaped_value, occurrences=2)
        passed.append("String edits accept spaces/quotes and reversible escapes without emitting terminal controls")

        terminal.keys(b"\r")
        frame = terminal.keys(b"oops\\q\r")
        editing(frame)
        assert_value(frame, "S$", escaped_value)
        frame = terminal.keys(ESC)
        assert_value(frame, "S$", escaped_value, occurrences=2)

        terminal.keys(b"\r")
        paste_start = len(terminal.transcript)
        terminal.keys(b'\x1b[200~PASTE\n\tPRINT "LEAK"\n\x1b[201~')
        frame = terminal.keys(HOME)
        editing(frame)
        assert "PASTE\\n\\t" in frame, frame[-2000:]
        paste_output = bytes(terminal.transcript[paste_start:])
        assert ENTER_ALT not in paste_output and LEAVE_ALT not in paste_output
        frame = terminal.keys(ESC)
        assert_value(frame, "S$", escaped_value, occurrences=2)
        passed.append("Bracketed paste inserts escaped text without applying newlines or leaking BASIC commands")

        terminal.keys(b"\r")
        frame = terminal.keys(b"L" * 100 + b"TAIL")
        editing(frame)
        assert "TAIL" in frame, frame[-2000:]
        assert 1 <= terminal.terminal.row < terminal.terminal.rows
        assert 1 <= terminal.terminal.col <= terminal.terminal.cols
        frame = terminal.keys(HOME)
        assert "LLLLLLLL" in frame and "TAIL" not in frame, frame[-2000:]
        frame = terminal.keys(END)
        assert "TAIL" in frame, frame[-2000:]
        for width in (80, 100, 120):
            frame = terminal.resize(width, 32)
            editing(frame)
            assert "TAIL" in frame, frame[-2000:]
            assert 1 <= terminal.terminal.row < terminal.terminal.rows
            assert 1 <= terminal.terminal.col <= width
        frame = terminal.keys(LEFT + b"X" + RIGHT)
        assert "TAIXL" in frame, frame[-2000:]
        terminal.resize(120, 4)
        frame = terminal.resize(120, 32)
        editing(frame)
        assert "TAIXL" in frame, frame[-2000:]
        terminal.resize(120, 4)
        terminal.keys(ESC)
        frame = terminal.resize(120, 32)
        assert_value(frame, "S$", escaped_value, occurrences=2)
        assert_pause(frame)
        assert "Esc Cancel" not in frame, frame[-700:]
        if "Tab Inspect" in frame[-120:]:
            frame = terminal.keys(b"\t")
        assert "INSPECT" in frame[-120:], frame[-120:]
        passed.append("Long edits scroll, survive width changes and a hidden inspector, and Esc still cancels when hidden")

        # M(1) is selected explicitly. Committing must not touch M(2), even
        # though the inspector stores one last-modified entry for the array.
        terminal.keys(DOWN)
        frame = terminal.keys(b"\r")
        editing(frame)
        frame = terminal.keys(b"17\r")
        assert_pause(frame)
        assert_value(frame, "M(1)", "17", occurrences=2)
        terminal.keys(HOME + DOWN * 3)
        frame = terminal.keys(b"\r")
        editing(frame)
        terminal.keys(b"88")
        frame = terminal.keys(ESC)
        assert_value(frame, "M(1)", "17", occurrences=2)
        passed.append("Array edits target the displayed fixed element and update PINNED/VARIABLES together")

        assert time.monotonic() - editing_started > 1.0
        assert abs(timer_remaining(frame) - baseline_timer) <= 2, (baseline_timer, timer_remaining(frame))
        passed.append("The one-second timer remains frozen across all edits and snapshot refreshes")

        terminal.keys(b"\t")
        terminal.send(F5)
        output = plain(terminal.wait_for("Ready"))
        assert re.search(r"EDITRESULT=\s*13\.5\s*:LEN=\s*11\s*:CTRL=\s*27\s*:KEY=\s*0(?:\s|$)", output), repr(output)
        assert re.search(r"EDITARRAY=\s*17\s*:\s*9(?:\s|$)", output), repr(output)
        assert "BADERROR" not in output and "BADTIMER" not in output, repr(output)
        passed.append("Continued BASIC uses edited values, preserves other array cells and receives no editor keys")

        terminal.send(b"DEBUG\r")
        terminal.wait_for("Ln 10 Stmt 1")
        for line in (20, 30):
            terminal.send(F6)
            terminal.wait_for(f"Ln {line} Stmt 1")
        terminal.keys(b"\t" + HOME + DOWN + DOWN + b"\r")
        frame = terminal.keys(b"999")
        editing(frame)
        terminal.send(b"\x03")
        terminal.wait_for("Ready")
        output = terminal.command('PRINT "EDITAFTER=";2+2')
        assert re.search(r"EDITAFTER=\s*4(?:\s|$)", output), repr(output)
        passed.append("Ctrl+C aborts during inline editing and restores the REPL")

        terminal.send(b"QUIT\r")
        deadline = time.monotonic() + 3
        while terminal.child.poll() is None and time.monotonic() < deadline:
            terminal.read_once(0.05)
        assert terminal.child.poll() == 0, terminal.child.poll()
        return passed
    except BaseException:
        print("PTY transcript tail:", repr(bytes(terminal.transcript[-8000:])), file=sys.stderr)
        raise
    finally:
        terminal.close()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path, help="exact Linux avl-basic executable to verify")
    parser.add_argument("--theme", choices=("dark", "light"), default="dark")
    parser.add_argument("--no-color", action="store_true")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not os.access(binary, os.X_OK):
        parser.error(f"not executable: {binary}")
    checks = run(binary, args.theme, args.no_color)
    for check in checks:
        print(f"PASS: {check}")
    mode = "NO_COLOR" if args.no_color else args.theme
    print(f"All {len(checks)} inline-edit PTY checks passed ({mode}): {binary}")


if __name__ == "__main__":
    main()
