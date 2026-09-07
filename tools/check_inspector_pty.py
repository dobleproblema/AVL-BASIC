#!/usr/bin/env python3
"""Check the debugger inspector through a real Linux/WSL terminal.

Pass the exact Linux executable to test. This uses only the Python standard
library, answers terminal queries, and always reaps its child process. The BASIC
program is entered in memory; no program or application files are created.
"""

from __future__ import annotations

import argparse
import errno
import fcntl
import os
from pathlib import Path
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time


ENTER_ALT = b"\x1b[?1049h"
LEAVE_ALT = b"\x1b[?1049l"
F2, F4, F5, F6 = b"\x1bOQ", b"\x1bOS", b"\x1b[15~", b"\x1b[17~"
F1, F7, F8 = b"\x1bOP", b"\x1b[18~", b"\x1b[19~"
UP, DOWN, LEFT, RIGHT = b"\x1b[A", b"\x1b[B", b"\x1b[D", b"\x1b[C"
HOME, END = b"\x1b[H", b"\x1b[F"
PAGE_UP, PAGE_DOWN = b"\x1b[5~", b"\x1b[6~"
BACK_TAB, ESC = b"\x1b[Z", b"\x1b"
CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
OSC = re.compile(rb"\x1b\].*?(?:\x07|\x1b\\)", re.DOTALL)


def plain(data: bytes) -> str:
    return CSI.sub(b"", OSC.sub(b"", data)).decode("utf-8", "replace")


def last_frame(data: bytes) -> str:
    # Each debugger paint begins at the upper-left code cell. A key batch may
    # produce several complete paints; assertions must inspect the final one.
    return plain(data.rsplit(b"\x1b[1;1H", 1)[-1])


class TerminalRequests:
    """Answer terminal requests with a small cursor tracker, not screen replay."""

    def __init__(self, rows: int, cols: int):
        self.rows, self.cols = rows, cols
        self.row, self.col = 1, 1
        self.saved = self.primary = (1, 1)
        self.escape = bytearray()
        self.in_osc = self.osc_escape = False

    def feed(self, data: bytes) -> bytes:
        replies = bytearray()
        for byte in data:
            if self.in_osc:
                if byte == 7 or (self.osc_escape and byte == ord("\\")):
                    self.in_osc = False
                self.osc_escape = byte == 27
                continue
            if self.escape:
                self.escape.append(byte)
                if len(self.escape) == 2:
                    if byte == ord("]"):
                        self.in_osc = True
                        self.escape.clear()
                    elif byte != ord("["):
                        if byte == ord("7"):
                            self.saved = (self.row, self.col)
                        elif byte == ord("8"):
                            self.row, self.col = self.saved
                        self.escape.clear()
                    continue
                if 0x40 <= byte <= 0x7E:
                    body = bytes(self.escape[2:-1]).decode("ascii", "replace")
                    if byte == ord("n") and body in ("6", "?6"):
                        prefix = "?" if body.startswith("?") else ""
                        replies.extend(f"\x1b[{prefix}{self.row};{self.col}R".encode())
                    elif byte == ord("n") and body == "5":
                        replies.extend(b"\x1b[0n")
                    elif byte == ord("c") and body in ("", "0"):
                        replies.extend(b"\x1b[?1;2c")
                    else:
                        self._csi(body, chr(byte))
                    self.escape.clear()
                elif len(self.escape) > 128:
                    self.escape.clear()
                continue
            if byte == 27:
                self.escape.append(byte)
            elif byte == 13:
                self.col = 1
            elif byte == 10:
                self.row = min(self.rows, self.row + 1)
            elif byte == 8:
                self.col = max(1, self.col - 1)
            elif byte == 9:
                self.col = min(self.cols, ((self.col - 1) // 8 + 1) * 8 + 1)
            elif byte >= 32 and byte & 0xC0 != 0x80:
                self.col += 1
                if self.col > self.cols:
                    self.col = 1
                    self.row = min(self.rows, self.row + 1)
        return bytes(replies)

    def _csi(self, body: str, final: str) -> None:
        if body == "?1049":
            if final == "h":
                self.primary = (self.row, self.col)
                self.row, self.col = 1, 1
            elif final == "l":
                self.row, self.col = self.primary
            return
        if body.startswith(("?", ">", "<", "=")):
            return
        try:
            values = [int(value or "1") for value in body.split(";")]
        except ValueError:
            return
        amount = values[0] or 1
        if final in "Hf":
            self.row = min(self.rows, max(1, amount))
            self.col = min(self.cols, max(1, values[1] if len(values) > 1 else 1))
        elif final == "G":
            self.col = min(self.cols, max(1, amount))
        elif final == "d":
            self.row = min(self.rows, max(1, amount))
        elif final == "A":
            self.row = max(1, self.row - amount)
        elif final == "B":
            self.row = min(self.rows, self.row + amount)
        elif final == "C":
            self.col = min(self.cols, self.col + amount)
        elif final == "D":
            self.col = max(1, self.col - amount)
        elif final == "s":
            self.saved = (self.row, self.col)
        elif final == "u":
            self.row, self.col = self.saved


class PtySession:
    def __init__(self, binary: Path, theme: str, no_color: bool = False):
        master, slave = os.openpty()
        self.master = master
        self.transcript = bytearray()
        self.terminal = TerminalRequests(32, 120)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 120, 0, 0))

        def own_terminal() -> None:
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        environment = {**os.environ, "TERM": "xterm-256color", "AVL_BASIC_WINDOW": "0",
                       "AVL_BASIC_THEME": theme, "AVL_BASIC_COLOR": "1"}
        if no_color:
            environment["NO_COLOR"] = "1"
        else:
            environment.pop("NO_COLOR", None)
        try:
            self.child = subprocess.Popen(
                [str(binary)], stdin=slave, stdout=slave, stderr=slave,
                close_fds=True, preexec_fn=own_terminal, cwd=str(binary.parent),
                env=environment,
            )
        except BaseException:
            os.close(master)
            raise
        finally:
            os.close(slave)

    def close(self) -> None:
        if self.child.poll() is None:
            os.killpg(self.child.pid, signal.SIGTERM)
            try:
                self.child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(self.child.pid, signal.SIGKILL)
                self.child.wait(timeout=2)
        os.close(self.master)

    def send(self, data: bytes) -> None:
        offset = 0
        while offset < len(data):
            offset += os.write(self.master, data[offset:])

    def read_once(self, timeout: float) -> bytes:
        if not select.select([self.master], [], [], max(0.0, timeout))[0]:
            return b""
        try:
            data = os.read(self.master, 65536)
        except OSError as error:
            if error.errno == errno.EIO:
                return b""
            raise
        self.transcript.extend(data)
        replies = self.terminal.feed(data)
        if replies:
            self.send(replies)
        return data

    def drain(self, quiet: float = 0.12, timeout: float = 3.0) -> bytes:
        start = len(self.transcript)
        deadline = time.monotonic() + timeout
        quiet_deadline = time.monotonic() + quiet
        while time.monotonic() < min(deadline, quiet_deadline):
            data = self.read_once(min(deadline, quiet_deadline) - time.monotonic())
            if data:
                quiet_deadline = time.monotonic() + quiet
            elif self.child.poll() is not None:
                break
        if time.monotonic() >= deadline:
            raise AssertionError("PTY output did not settle within timeout")
        return bytes(self.transcript[start:])

    def wait_for(self, expected: str | bytes, timeout: float = 5.0) -> bytes:
        start = len(self.transcript)
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            data = bytes(self.transcript[start:])
            found = expected in data if isinstance(expected, bytes) else expected in plain(data)
            if found:
                self.drain()
                return bytes(self.transcript[start:])
            if self.child.poll() is not None:
                break
            self.read_once(min(0.1, deadline - time.monotonic()))
        raise AssertionError(f"Timed out waiting for {expected!r}; child={self.child.poll()}")

    def keys(self, data: bytes) -> str:
        self.send(data)
        frame = last_frame(self.drain())
        assert self.child.poll() is None, "Child unexpectedly exited"
        return frame

    def resize(self, cols: int, rows: int) -> str:
        self.terminal.cols, self.terminal.rows = cols, rows
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        return last_frame(self.drain())

    def command(self, command: str) -> str:
        return self.keys(command.encode() + b"\r")


def assert_heading(frame: str, name: str, count: int, collapsed: bool = False) -> None:
    marker = "▸" if collapsed else "▾"
    assert re.search(rf"{marker}\s+\[?{name}\]?\s+\({count}\)", frame), frame[-1800:]
    assert not re.search(r"[▶>]\s*[▾▸]", frame), frame[-1800:]


def category_body(frame: str, name: str, next_name: str) -> str:
    match = re.search(
        rf"[▾▸]\s+\[?{name}\]?\s+\(\d+\)(.*?)[▾▸]\s+\[?{next_name}\]?\s+\(\d+\)",
        frame, re.DOTALL,
    )
    assert match, frame[-2200:]
    return match.group(1)


def assert_value(frame: str, name: str, value: str, occurrences: int = 1) -> None:
    found = re.findall(rf"(?<![A-Za-z0-9_]){re.escape(name)} = {re.escape(value)}(?=\s|$)", frame)
    assert len(found) == occurrences, (name, value, occurrences, frame[-2200:])


def check_array_pins(terminal: PtySession) -> None:
    terminal.command("NEW")
    for line in (
        "10 A=42", "20 DIM A(12)", "30 A(7)=9", "40 A(12)=8",
        "50 CLEAR", "60 A(3)=6", "70 END",
    ):
        terminal.command(line)
    terminal.send(b"DEBUG\r")
    terminal.wait_for("Ln 10 Stmt 1")
    for line in (20, 30, 40):
        terminal.send(F6)
        terminal.wait_for(f"Ln {line} Stmt 1")
    terminal.keys(b"\t" + HOME + DOWN + DOWN)
    terminal.keys(b" ")
    terminal.keys(DOWN)
    frame = terminal.keys(b" ")
    assert_heading(frame, "PINNED", 2)
    assert_value(frame, "A", "42", occurrences=2)
    assert_value(frame, "A(7)", "9", occurrences=2)
    pinned = category_body(frame, "PINNED", "VARIABLES")
    variables = category_body(frame, "VARIABLES", "ARRAYS")
    assert "[+]" not in pinned, pinned
    assert variables.count("[+]") == 2, variables
    assert pinned.index("A = 42") < pinned.index("A(7) = 9"), pinned
    assert re.search(r"▶[ *]*\[\+\] A\(7\) = 9", variables), variables

    terminal.keys(b"\t")
    terminal.send(F6)
    frame = last_frame(terminal.wait_for("Ln 50 Stmt 1"))
    assert_heading(frame, "PINNED", 2)
    assert_value(frame, "A", "42", occurrences=2)
    assert_value(frame, "A(12)", "8", occurrences=2)
    assert "A(7) = 9" not in frame, frame[-2200:]
    assert "[+]" not in category_body(frame, "PINNED", "VARIABLES")
    frame = terminal.keys(b"\t")
    assert "Space Unpin" in frame, frame[-600:]

    terminal.keys(b"\t")
    terminal.send(F6)
    frame = last_frame(terminal.wait_for("Ln 60 Stmt 1"))
    assert_heading(frame, "PINNED", 2)
    assert_heading(frame, "VARIABLES", 0)
    assert_value(frame, "A", "<unavailable>")
    assert_value(frame, "A()", "<unavailable>")
    terminal.keys(b"\t")
    frame = terminal.keys(HOME + DOWN + b"\r")
    assert "Esc Cancel" not in frame, frame[-600:]
    assert_value(frame, "A", "<unavailable>")
    terminal.keys(b"\t")
    terminal.send(F6)
    frame = last_frame(terminal.wait_for("Ln 70 Stmt 1"))
    assert_value(frame, "A", "<unavailable>")
    assert_value(frame, "A(3)", "6", occurrences=2)
    terminal.keys(b"\t")
    terminal.send(ESC)
    terminal.wait_for("Ready")


def check_stack_paging(terminal: PtySession) -> None:
    terminal.command("NEW")
    lines = ["10 GOSUB 100", "20 END"]
    lines.extend(f"{line} GOSUB {line + 10}:RETURN" for line in range(100, 550, 10))
    lines.extend(("550 A=1", "560 RETURN"))
    for line in lines:
        terminal.command(line)
    terminal.send(b"DEBUG\r")
    terminal.wait_for("Ln 10 Stmt 1")
    frame = terminal.keys(END + UP + F2)
    assert "Breakpoint set at 550" in frame, frame[-800:]
    terminal.send(F5)
    frame = last_frame(terminal.wait_for("Ln 550 Stmt 1"))
    assert_heading(frame, "STACK", 47)

    # Empty categories have no selectable values. Three Down presses from
    # PINNED therefore reach STACK without selecting decorative '(none)' rows.
    frame = terminal.keys(b"\t" + HOME + DOWN + DOWN + DOWN)
    assert "Space Collapse" in frame, frame[-600:]
    frame = terminal.keys(PAGE_DOWN)
    assert "46: GOSUB" in frame, frame[-2200:]
    assert not re.search(r"[▾▸]\s+\[?STACK\]?", frame), frame[-2200:]
    assert "Space Collapse" not in frame, frame[-600:]

    # The selected STACK heading is above the current page: Enter and Space
    # must not fold it, activate hidden content, or snap scrolling back to it.
    frame = terminal.keys(b"\r ")
    assert "46: GOSUB" in frame, frame[-2200:]
    assert "Space Collapse" not in frame, frame[-600:]
    frame = terminal.keys(PAGE_UP)
    assert_heading(frame, "STACK", 47)
    frame = terminal.keys(b"\r")
    assert_heading(frame, "STACK", 47)
    frame = terminal.keys(b" ")
    assert_heading(frame, "STACK", 47, collapsed=True)
    assert "0: PROGRAM" not in frame and "46: GOSUB" not in frame, frame[-2200:]
    terminal.send(b"\x03")
    terminal.wait_for("Ready")


def run(binary: Path, theme: str, no_color: bool = False) -> list[str]:
    passed = []
    terminal = PtySession(binary, theme, no_color)
    try:
        terminal.wait_for("Ready")
        for line in (
            "10 A=0", "20 B=10", "30 C=20", "40 A=A+1", "50 B=B+1",
            "60 A=A+1", "70 K$=INKEY$", '80 PRINT "INSPECTKEY=";LEN(K$);":A=";A',
            "90 END",
        ):
            terminal.command(line)
        terminal.send(b"DEBUG\r")
        terminal.wait_for("Ln 10 Stmt 1")
        for line in (20, 30, 40):
            terminal.send(F6)
            frame = last_frame(terminal.wait_for(f"Ln {line} Stmt 1"))
        assert_heading(frame, "PINNED", 0)
        assert_heading(frame, "VARIABLES", 3)
        assert_value(frame, "A", "0")

        # Home establishes a known inspector position, independent of the code
        # cursor and the line most recently executed.
        frame = terminal.keys(b"\t" + HOME)
        assert_heading(frame, "PINNED", 0)
        if no_color:
            assert "[PINNED]" in frame, frame[-1800:]
        assert not re.search(r"\bF(?:1|2|4|5|6|7|8)\b", frame[-120:]), frame[-120:]
        ignored_start = len(terminal.transcript)
        for key in (b"\r", F1, F2, F4, F5, F6, F7, F8):
            terminal.keys(key)
        ignored_output = bytes(terminal.transcript[ignored_start:])
        assert ENTER_ALT not in ignored_output and LEAVE_ALT not in ignored_output, ignored_output
        assert "Breakpoint set" not in plain(ignored_output), plain(ignored_output)[-1000:]
        frame = terminal.keys(HOME)
        assert "Ln 40 Stmt 1" in frame and "INSPECT" in frame, frame[-600:]
        assert_heading(frame, "PINNED", 0)
        assert_value(frame, "A", "0")
        passed.append("Headings ignore Enter and the inspector consumes function keys without changing execution")
        terminal.keys(DOWN)
        terminal.keys(DOWN)
        frame = terminal.keys(b" ")
        assert_heading(frame, "PINNED", 1)
        assert_value(frame, "A", "0", occurrences=2)
        assert "INSPECT" in frame and "Space Unpin" in frame, frame[-600:]
        assert "[+]" not in category_body(frame, "PINNED", "VARIABLES")
        variables = category_body(frame, "VARIABLES", "ARRAYS")
        assert re.search(r"▶[ *]*\[\+\] A = 0", variables), variables
        passed.append("Tab focuses the inspector; Space pins a scalar without executing BASIC")

        frame = terminal.keys(b"\r")
        assert "Esc Cancel" in frame, frame[-600:]
        frame = terminal.keys(ESC)
        assert_heading(frame, "PINNED", 1)
        assert_value(frame, "A", "0", occurrences=2)
        frame = terminal.keys(UP + b" ")
        assert_heading(frame, "VARIABLES", 3, collapsed=True)
        assert_value(frame, "A", "0")
        assert "B = 10" not in frame and "C = 20" not in frame, frame[-1800:]
        passed.append("Space collapses VARIABLES while PINNED stays visible; Esc cancels scalar editing")

        for line, value in ((50, "1"), (60, "1"), (70, "2")):
            terminal.keys(b"\t")
            terminal.send(F6)
            frame = last_frame(terminal.wait_for(f"Ln {line} Stmt 1"))
            assert "CODE" in frame and "Tab Inspect" in frame, frame[-600:]
            assert_heading(frame, "PINNED", 1)
            assert_heading(frame, "VARIABLES", 3, collapsed=True)
            assert_value(frame, "A", value)
            frame = terminal.keys(b"\t")
            assert "Space Expand" in frame, frame[-600:]
        # The inspector selection is retained while stepping from CODE.
        frame = terminal.keys(b" ")
        assert_heading(frame, "VARIABLES", 3)
        assert_value(frame, "A", "2", occurrences=2)
        terminal.keys(b" ")
        passed.append("CODE stepping refreshes pinned values and Tab restores inspector selection and folding")

        terminal.keys(b"\t")
        terminal.send(F4)
        terminal.wait_for(LEAVE_ALT)
        terminal.send(ESC)
        frame = last_frame(terminal.wait_for("Ln 70 Stmt 1"))
        assert_heading(frame, "VARIABLES", 3, collapsed=True)
        assert "CODE" in frame and "Tab Inspect" in frame, frame[-600:]
        terminal.keys(b"\t")
        frame = terminal.keys(b" ")
        assert_heading(frame, "VARIABLES", 3)
        terminal.keys(b" ")
        passed.append("F4/Esc returns to CODE at the same pause with inspector selection preserved")

        # Narrow terminals relocate the inspector below the source, with two
        # columns at 80 and three at 100. Focus and pins survive both layouts.
        for width in (80, 100):
            frame = terminal.resize(width, 32)
            assert_heading(frame, "PINNED", 1)
            assert_heading(frame, "VARIABLES", 3, collapsed=True)
            assert_value(frame, "A", "2")
            assert "INSPECT" in frame, frame[-500:]
            for key in (RIGHT, LEFT, PAGE_DOWN, PAGE_UP, END, HOME):
                terminal.keys(key)
        terminal.resize(120, 32)
        frame = terminal.keys(HOME + b" ")
        assert_heading(frame, "PINNED", 1, collapsed=True)
        frame = terminal.keys(b" ")
        assert_heading(frame, "PINNED", 1)
        assert_value(frame, "A", "2")
        passed.append("80/100/120-column resize and navigation preserve pins, focus and folding")

        # With the first pinned value selected, toggling focus twice must return
        # to that value; Space then removes it instead of modifying source text.
        terminal.keys(DOWN)
        terminal.keys(BACK_TAB)
        terminal.keys(b"\t")
        frame = terminal.keys(b" ")
        assert_heading(frame, "PINNED", 0)
        assert_heading(frame, "VARIABLES", 3, collapsed=True)
        assert "A = 2" not in frame, frame[-1800:]
        passed.append("Shift+Tab/Tab preserves selection; Space removes a pinned scalar")

        # Leave an actual pin in place, as well as a collapsed category, so the
        # following DEBUG verifies session lifetime rather than an empty list.
        terminal.keys(HOME)
        terminal.keys(DOWN + b" ")
        terminal.keys(DOWN)
        frame = terminal.keys(b" ")
        assert_heading(frame, "PINNED", 1)
        terminal.keys(UP + b" ")
        terminal.keys(b"\t")
        terminal.send(F5)
        text = plain(terminal.wait_for("Ready"))
        assert re.search(r"INSPECTKEY=\s*0\s*:A=\s*2(?:\s|$)", text), repr(text)
        passed.append("Continue consumes navigation keys: BASIC INKEY$ receives none")

        terminal.send(b"DEBUG\r")
        frame = last_frame(terminal.wait_for("Ln 10 Stmt 1"))
        assert_heading(frame, "PINNED", 0)
        assert_heading(frame, "VARIABLES", 0)
        terminal.send(ESC)
        terminal.wait_for("Ready")
        text = terminal.command('PRINT "INSPECTAFTER=";2+2')
        assert re.search(r"INSPECTAFTER=\s*4(?:\s|$)", text), repr(text)
        passed.append("A new DEBUG resets categories; abort restores a usable REPL")

        check_array_pins(terminal)
        passed.append("Scalar/array pins remain distinct and follow the latest array write without duplicate markers")
        check_stack_paging(terminal)
        passed.append("Long-stack paging exposes all rows, hidden headings stay inert and Ctrl+C aborts INSPECT")
        if no_color:
            # Terminal cleanup can emit SGR reset, but no actual color, bold or
            # underline is allowed when syntax highlighting is disabled.
            styles = re.findall(rb"\x1b\[([0-9;]*)m", bytes(terminal.transcript))
            assert all(style in (b"", b"0") for style in styles), styles[-30:]
            passed.append("NO_COLOR uses bracketed headings and variable arrows with no styled text")

        terminal.send(b"QUIT\r")
        deadline = time.monotonic() + 3
        while terminal.child.poll() is None and time.monotonic() < deadline:
            terminal.read_once(0.05)
        assert terminal.child.poll() == 0, f"QUIT did not exit cleanly: {terminal.child.poll()}"
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
    parser.add_argument("--no-color", action="store_true", help="verify NO_COLOR plain-text selection")
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    if not os.access(binary, os.X_OK):
        parser.error(f"not executable: {binary}")
    checks = run(binary, args.theme, args.no_color)
    for check in checks:
        print(f"PASS: {check}")
    mode = "NO_COLOR" if args.no_color else args.theme
    print(f"All {len(checks)} inspector PTY checks passed ({mode}): {binary}")


if __name__ == "__main__":
    main()
