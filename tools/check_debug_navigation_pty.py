#!/usr/bin/env python3
"""Exercise debugger Set Next (F3) and Restart (F9) in a real Linux/WSL PTY.

Uses the standard-library driver beside this file. The BASIC programs exist only
in memory, and the terminal child is always reaped.
"""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import sys
import time

from check_inspector_pty import (
    DOWN, END, ENTER_ALT, ESC, F2, F4, F5, F6, HOME, LEAVE_ALT, LEFT,
    RIGHT, UP, PtySession, assert_heading, assert_value, last_frame, plain,
)


F3, F9 = b"\x1bOR", b"\x1b[20~"


def location(frame: str, line: int, statement: int = 1) -> None:
    assert f"Ln {line} Stmt {statement}" in frame, frame[-1200:]
    assert not re.search(r"\b(?:View|Sel)\s", frame), frame[-1200:]


def underlined_text(terminal: PtySession) -> str:
    """Collect the latest paint's underlined characters across syntax resets."""
    raw = bytes(terminal.transcript).rsplit(b"\x1b[1;1H", 1)[-1]
    rendered = raw.decode("utf-8", "replace")
    underlined = False
    parts = []
    for part in re.split(r"(\x1b\[[0-?]*[ -/]*[@-~])", rendered):
        if part.startswith("\x1b["):
            # The renderer uses dedicated SGR 4/24 and reset sequences. Do not
            # mistake a palette index inside 38;5;n for an underline attribute.
            if part == "\x1b[4m":
                underlined = True
            elif part in ("\x1b[m", "\x1b[0m", "\x1b[24m"):
                underlined = False
        elif underlined:
            parts.append(part)
    return "".join(parts)


def step(terminal: PtySession, line: int, statement: int = 1) -> str:
    terminal.send(F6)
    frame = last_frame(terminal.wait_for(f"Ln {line} Stmt {statement}"))
    location(frame, line, statement)
    return frame


def restart(terminal: PtySession, first_line: int = 10) -> str:
    terminal.send(F9)
    frame = last_frame(terminal.wait_for(f"Ln {first_line} Stmt 1"))
    location(frame, first_line)
    return frame


def load(terminal: PtySession, lines: tuple[str, ...]) -> None:
    terminal.command("NEW")
    for line in lines:
        terminal.command(line)
    terminal.send(b"DEBUG\r")
    terminal.wait_for("Ln 10 Stmt 1")


def check_set_next(terminal: PtySession, no_color: bool) -> list[str]:
    load(terminal, (
        "10 X=2", "20 Y=X*3:Z=99",
        "30 IF X>0 THEN T=1:U=2 ELSE T=3:U=4", "40 K$=INKEY$",
        '50 PRINT "NAV=";X;":";Y;":";Z;":";T;":";U;":";LEN(K$)', "60 END",
    ))
    step(terminal, 20)
    frame = step(terminal, 20, 2)
    assert_value(frame, "X", "2")
    assert_value(frame, "Y", "6")

    # Output, inspector browsing and inline editing each consume F3/F9.
    terminal.send(F4)
    terminal.wait_for(LEAVE_ALT)
    assert not terminal.keys(F3 + F9)
    terminal.send(ESC)
    frame = last_frame(terminal.wait_for("Ln 20 Stmt 2"))
    location(frame, 20, 2)
    terminal.keys(b"\t" + HOME + DOWN + DOWN)
    terminal.keys(b" ")
    ignored_start = len(terminal.transcript)
    frame = terminal.keys(F3 + F9)
    location(frame, 20, 2)
    assert_heading(frame, "PINNED", 1)
    assert_value(frame, "X", "2", occurrences=2)
    terminal.keys(b"\r")
    frame = terminal.keys(b"5" + F3 + F9)
    assert "Esc Cancel" in frame, frame[-1200:]
    frame = terminal.keys(b"\r")
    location(frame, 20, 2)
    assert_value(frame, "X", "5", occurrences=2)
    assert_value(frame, "Y", "6")
    ignored = bytes(terminal.transcript[ignored_start:])
    assert ENTER_ALT not in ignored and LEAVE_ALT not in ignored

    terminal.keys(b"\t")
    frame = terminal.keys(LEFT)
    location(frame, 20, 2)
    if not no_color:
        selection = underlined_text(terminal)
        assert "Y=X*3" in selection and "Z=99" not in selection, repr(selection)
    assert re.search(r"▶\s+Z=99", frame), frame[-2400:]
    for scroll_key in (b"\x1b[1;5C", b"\x1b[1;5D"):
        frame = terminal.keys(scroll_key)
        location(frame, 20, 2)
    if not no_color:
        assert "Y=X*3" in underlined_text(terminal), repr(underlined_text(terminal))
    frame = terminal.keys(F3)
    location(frame, 20)
    assert "Next statement set" in frame, frame[-1200:]
    assert_value(frame, "Y", "6")
    assert "Z = " not in frame, frame[-2400:]
    assert re.search(r"▶\s+Y=X\*3", frame), frame[-2400:]
    frame = step(terminal, 20, 2)
    assert_value(frame, "Y", "15")
    assert "Z = " not in frame, frame[-2400:]

    # Skip Z entirely and select a whole inline IF. Right must not enter its
    # THEN/ELSE leaves; those remain independently available to F6 stepping.
    frame = terminal.keys(DOWN + F3)
    location(frame, 30)
    assert "Z = " not in frame and "T = " not in frame, frame[-2400:]
    frame = terminal.keys(RIGHT)
    location(frame, 30)
    if not no_color:
        selection = underlined_text(terminal)
        assert "IF X>0 THEN T=1:U=2 ELSE T=3:U=4" in selection, repr(selection)
    frame = terminal.keys(F3)
    location(frame, 30)
    assert "T = " not in frame and "U = " not in frame, frame[-2400:]
    step(terminal, 30, 2)
    step(terminal, 30, 3)
    frame = step(terminal, 40)
    assert_value(frame, "T", "1")
    assert_value(frame, "U", "2")
    assert "Z = " not in frame, frame[-2400:]
    terminal.send(F5)
    output = plain(terminal.wait_for("Ready"))
    assert re.search(r"NAV=\s*5\s*:\s*15\s*:\s*0\s*:\s*1\s*:\s*2\s*:\s*0(?:\s|$)", output), repr(output)
    return [
        "F3/F9 are inert in output, inspector and editing modes",
        "Left/Right selects a candidate without moving execution; F3 applies it without running",
        "An edited X feeds a repeated calculation; skipping a statement leaves it unexecuted",
        "Set Next treats inline IF as one target while F6 still steps its selected branch",
        "Continued BASIC receives no navigation keys through INKEY$",
    ]


def check_loop_rejections(terminal: PtySession) -> str:
    load(terminal, (
        "10 FOR I=1 TO 2", "20 A=A+1", "30 NEXT I", "40 END",
    ))
    frame = terminal.keys(DOWN + F3)
    location(frame, 10)
    assert "Cannot set next:" in frame, frame[-1200:]
    assert "A = " not in frame and "ERR=0 ERL=0" in frame, frame[-2400:]
    frame = step(terminal, 20)
    assert_value(frame, "I", "1")
    frame = terminal.keys(END + F3)
    location(frame, 20)
    assert "Cannot set next:" in frame, frame[-1200:]
    assert_value(frame, "I", "1")
    assert "A = " not in frame and "ERR=0 ERL=0" in frame, frame[-2400:]
    terminal.send(ESC)
    terminal.wait_for("Ready")
    return "Set Next rejects entry into an unentered FOR and exit from an active FOR without changing state"


def check_sub_restart(terminal: PtySession) -> list[str]:
    load(terminal, (
        "10 DEF SUB WORK", "20 LOCAL L", "30 L=7", "40 L=L+1", "50 SUBEND",
        "60 ON ERROR RESUME NEXT", "70 READ D", "80 X=10",
        "90 AFTER 500,1 GOSUB 800", "100 Q=1/0", "110 CALL WORK", "120 END",
        "800 RETURN", "900 DATA 11,22",
    ))
    frame = terminal.keys(HOME + DOWN * 3 + F2)
    assert "Breakpoint set at 40" in frame, frame[-1200:]
    terminal.send(F5)
    frame = last_frame(terminal.wait_for("Ln 40 Stmt 1"))
    assert_heading(frame, "STACK", 2)
    assert_heading(frame, "TIMERS", 1)
    # ON ERROR RESUME NEXT clears ERR/ERL after handling the main-program error.
    assert "ERR=0 ERL=0" in frame, frame[-2400:]
    assert "Ln 900 Item 2: 22" in frame, frame[-2400:]

    terminal.keys(b"\t" + HOME + DOWN + DOWN + b" ")
    frame = terminal.keys(HOME + DOWN * 2 + b" ")
    assert_heading(frame, "PINNED", 1)
    assert re.search(r"▸\s+\[?VARIABLES\]?", frame), frame[-2400:]
    assert_value(frame, "D", "11")
    terminal.keys(b"\t")
    frame = terminal.keys(HOME + DOWN * 7 + F3)
    location(frame, 40)
    assert "Cannot set next:" in frame, frame[-1200:]
    assert_heading(frame, "STACK", 2)
    assert_value(frame, "D", "11")

    for _ in range(2):
        frame = restart(terminal)
        assert_heading(frame, "PINNED", 1)
        assert_heading(frame, "VARIABLES", 0, collapsed=True)
        assert_value(frame, "D", "<unavailable>")
        assert_heading(frame, "STACK", 1)
        assert_heading(frame, "TIMERS", 0)
        assert "ERR=0 ERL=0" in frame, frame[-2400:]
        assert "Ln 900 Item 1: 11" in frame, frame[-2400:]
    terminal.send(F5)
    frame = last_frame(terminal.wait_for("Ln 40 Stmt 1"))
    assert_heading(frame, "PINNED", 1)
    assert_heading(frame, "STACK", 2)
    assert_value(frame, "D", "11")
    assert re.search(r"▸\s+\[?VARIABLES\]?", frame), frame[-2400:]
    terminal.send(ESC)
    terminal.wait_for("Ready")
    return [
        "Set Next cannot cross from a SUB into the main program",
        "Restart unwinds SUB and resets values, DATA, timers and stacks before the first statement",
        "Repeated Restart preserves pinned names, collapsed categories and effective breakpoints",
    ]


def check_function_restart(terminal: PtySession) -> str:
    load(terminal, (
        "10 DEF FNF(X)", "20 FNF=X+1", "30 FNEND", "40 A=FNF(2)", "50 END",
    ))
    frame = terminal.keys(DOWN + F2)
    assert "Breakpoint set at 20" in frame, frame[-1200:]
    for _ in range(2):
        terminal.send(F5)
        frame = last_frame(terminal.wait_for("Ln 20 Stmt 1"))
        assert_heading(frame, "STACK", 2)
        assert_value(frame, "X", "2")
        frame = restart(terminal)
        assert_heading(frame, "STACK", 1)
        assert_heading(frame, "VARIABLES", 0)
    terminal.send(ESC)
    terminal.wait_for("Ready")
    return "Restart unwinds a multiline function repeatedly without completing its pending expression"


def check_error_restart(terminal: PtySession) -> list[str]:
    load(terminal, (
        "10 ON ERROR GOTO 100", "20 A=7", "30 ERROR 5",
        '40 PRINT "HANDLER=";H;":";ERR;":";ERL', "50 END",
        "100 H=ERR", "110 H=H+1", "120 RESUME NEXT",
    ))
    frame = terminal.keys(HOME + DOWN * 6 + F2)
    assert "Breakpoint set at 110" in frame, frame[-1200:]
    terminal.send(F5)
    frame = last_frame(terminal.wait_for("Ln 110 Stmt 1"))
    assert "ERR=5 ERL=30" in frame, frame[-2400:]
    assert_value(frame, "A", "7")
    assert_value(frame, "H", "5")
    frame = terminal.keys(UP + F3)
    location(frame, 100)
    assert "Next statement set" in frame, frame[-1200:]
    assert "ERR=5 ERL=30" in frame, frame[-2400:]
    assert_value(frame, "H", "5")
    frame = step(terminal, 110)
    assert "ERR=5 ERL=30" in frame, frame[-2400:]
    assert_value(frame, "H", "5")
    frame = terminal.keys(END + F3)
    location(frame, 120)
    assert "Next statement set" in frame, frame[-1200:]
    assert "ERR=5 ERL=30" in frame, frame[-2400:]
    assert_value(frame, "H", "5")
    frame = terminal.keys(HOME + F3)
    location(frame, 120)
    assert "Cannot set next:" in frame, frame[-1200:]
    assert "ERR=5 ERL=30" in frame, frame[-2400:]
    frame = restart(terminal)
    assert "ERR=0 ERL=0" in frame, frame[-2400:]
    assert_heading(frame, "VARIABLES", 0)
    # The preserved breakpoint catches a fresh handler. Skipping its increment
    # must keep the original RESUME NEXT continuation and clear ERR only there.
    terminal.send(F5)
    frame = last_frame(terminal.wait_for("Ln 110 Stmt 1"))
    assert "ERR=5 ERL=30" in frame, frame[-2400:]
    frame = terminal.keys(END + F3)
    location(frame, 120)
    frame = step(terminal, 40)
    assert "ERR=0 ERL=0" in frame, frame[-2400:]
    assert_value(frame, "H", "5")
    terminal.send(F5)
    output = plain(terminal.wait_for("Ready"))
    assert re.search(r"HANDLER=\s*5\s*:\s*0\s*:\s*0(?:\s|$)", output), repr(output)
    return [
        "Set Next moves within an error handler without changing ERR/ERL or its RESUME NEXT continuation",
        "Cross-handler moves are rejected; Restart clears the active error and preserves its breakpoint",
    ]


def check_handler_reprint(terminal: PtySession, timer: bool) -> str:
    caller = (
        ("20 AFTER 1,1 GOSUB 190", "30 PAUSE 250", "40 K$=INKEY$")
        if timer else ("20 GOSUB 190", "40 K$=INKEY$")
    )
    load(terminal, (
        "10 X=7", *caller,
        '50 PRINT "RETURNED=";X;":";LEN(K$)', "60 END",
        '190 PRINT "REPRINT=";X', "200 RETURN",
    ))
    frame = terminal.keys(END + F2)
    assert "Breakpoint set at 200" in frame, frame[-1200:]
    terminal.send(F5)
    first_print = terminal.wait_for("Ln 200 Stmt 1")
    frame = last_frame(first_print)
    assert_heading(frame, "STACK", 2)
    assert_value(frame, "X", "7")
    assert re.search(r"REPRINT=\s*7(?:\s|$)", plain(first_print)), repr(plain(first_print))

    frame = terminal.keys(HOME + F3)
    location(frame, 200)
    assert "Cannot set next:" in frame, frame[-1200:]
    assert_heading(frame, "STACK", 2)
    terminal.keys(b"\t" + HOME + DOWN + DOWN + b"\r")
    frame = terminal.keys(b"11\r")
    location(frame, 200)
    assert_value(frame, "X", "11")
    terminal.keys(b"\t")
    move_start = len(terminal.transcript)
    frame = terminal.keys(END + UP + F3)
    location(frame, 190)
    assert "Next statement set" in frame, frame[-1200:]
    assert_heading(frame, "STACK", 2)
    assert_value(frame, "X", "11")
    # Moving the instruction pointer must neither print nor consume RETURN.
    moved = bytes(terminal.transcript[move_start:])
    assert LEAVE_ALT not in moved
    assert not re.search(r"REPRINT=\s*11(?:\s|$)", plain(moved)), repr(plain(moved))
    print_start = len(terminal.transcript)
    frame = step(terminal, 200)
    assert_heading(frame, "STACK", 2)
    repeated = plain(bytes(terminal.transcript[print_start:]))
    assert re.search(r"REPRINT=\s*11(?:\s|$)", repeated), repr(repeated)
    terminal.send(F5)
    output = plain(terminal.wait_for("Ready"))
    assert re.search(r"RETURNED=\s*11\s*:\s*0(?:\s|$)", output), repr(output)
    context = "timer handler with its caller's PAUSE" if timer else "GOSUB"
    return f"Set Next reprints an edited value from 200 to 190 inside {context}, preserving RETURN and consuming UI keys"


def run(binary: Path, theme: str, no_color: bool) -> list[str]:
    terminal = PtySession(binary, theme, no_color)
    try:
        terminal.wait_for("Ready")
        passed = check_set_next(terminal, no_color)
        passed.append(check_loop_rejections(terminal))
        passed.extend(check_sub_restart(terminal))
        passed.append(check_function_restart(terminal))
        passed.extend(check_error_restart(terminal))
        passed.append(check_handler_reprint(terminal, timer=False))
        passed.append(check_handler_reprint(terminal, timer=True))
        if no_color:
            styles = re.findall(rb"\x1b\[([0-9;]*)m", bytes(terminal.transcript))
            assert all(style in (b"", b"0") for style in styles), styles[-30:]
            passed.append("NO_COLOR preserves selection and Set Next behavior without fields View/Sel or styled text")
        output = terminal.command('PRINT "NAVAFTER=";2+2')
        assert re.search(r"NAVAFTER=\s*4(?:\s|$)", output), repr(output)
        terminal.send(b"QUIT\r")
        deadline = time.monotonic() + 3
        while terminal.child.poll() is None and time.monotonic() < deadline:
            terminal.read_once(0.05)
        assert terminal.child.poll() == 0, terminal.child.poll()
        return passed
    except BaseException:
        print("PTY transcript tail:", repr(bytes(terminal.transcript[-7000:])), file=sys.stderr)
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
    print(f"All {len(checks)} debugger navigation PTY checks passed ({mode}): {binary}")


if __name__ == "__main__":
    main()
