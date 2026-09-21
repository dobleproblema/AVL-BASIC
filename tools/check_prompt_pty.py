#!/usr/bin/env python3
"""Check immediate input and EDIT line through a real Linux/WSL PTY.

Uses only the standard library and check_inspector_pty.py beside this script.
Pass the exact executable with --binary; repeat with --cols 40, 80 and 120.
Programs remain in memory and the child terminal is always reaped.
"""
import argparse
import codecs
from pathlib import Path
import re
import sys
import termios
import time

args = argparse.ArgumentParser()
args.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[1])
args.add_argument('--binary', type=Path, required=True)
args.add_argument('--cols', type=int, default=40)
args = args.parse_args()
sys.path.insert(0, str(args.repo / 'tools'))
from check_inspector_pty import PtySession, plain, ENTER_ALT, LEAVE_ALT, F4, F5, F6, HOME, END, LEFT, ESC

class Screen:
    def __init__(self, cols=120, rows=32):
        self.cols, self.rows = cols, rows
        self.cells = [[' '] * cols for _ in range(rows)]
        self.row = self.col = 0
        self.pending_wrap = False
        self.wrap = True
        self.escape = ''
        self.osc = False
        self.osc_esc = False
        self.decoder = codecs.getincrementaldecoder('utf-8')('replace')
        self.primary = None
        self.cursor_visible = True

    def resize(self, cols, rows):
        self.cells = [(line + [' '] * cols)[:cols] for line in self.cells[:rows]]
        self.cells += [[' '] * cols for _ in range(rows - len(self.cells))]
        self.cols, self.rows = cols, rows
        self.row, self.col = min(self.row, rows - 1), min(self.col, cols - 1)
        self.pending_wrap = False

    def down(self):
        self.row += 1
        if self.row >= self.rows:
            self.cells.pop(0)
            self.cells.append([' '] * self.cols)
            self.row = self.rows - 1

    def feed(self, data):
        for char in self.decoder.decode(data):
            if self.osc:
                if char == '\a' or (self.osc_esc and char == '\\'):
                    self.osc = False
                self.osc_esc = char == '\x1b'
                continue
            if self.escape:
                self.escape += char
                if len(self.escape) == 2:
                    if char == ']':
                        self.osc = True
                        self.escape = ''
                    elif char != '[':
                        self.escape = ''
                    continue
                if '@' <= char <= '~':
                    self.csi(self.escape[2:-1], char)
                    self.escape = ''
                continue
            if char == '\x1b':
                self.escape = char
            elif char == '\r':
                self.col = 0
                self.pending_wrap = False
            elif char == '\n':
                self.down()
                self.pending_wrap = False
            elif char == '\b':
                self.col = max(0, self.col - 1)
                self.pending_wrap = False
            elif char >= ' ':
                if self.pending_wrap and self.wrap:
                    self.col = 0
                    self.down()
                self.pending_wrap = False
                self.cells[self.row][self.col] = char
                if self.col == self.cols - 1:
                    self.pending_wrap = True
                else:
                    self.col += 1

    def csi(self, body, command):
        if body == '?25':
            self.cursor_visible = command == 'h'
            return
        if body == '?7':
            self.wrap = command == 'h'
            return
        if body == '?1049':
            if command == 'h':
                self.primary = (self.cells, self.row, self.col)
                self.cells = [[' '] * self.cols for _ in range(self.rows)]
                self.row = self.col = 0
            elif self.primary is not None:
                self.cells, self.row, self.col = self.primary
                self.primary = None
            self.pending_wrap = False
            return
        if body.startswith(('?', '>', '<', '=')) or command in 'mnchlt':
            return
        try:
            p = [int(part or '0') for part in body.split(';')]
        except ValueError:
            return
        n = p[0] or 1
        if command in 'Hf':
            self.row = max(0, min(self.rows - 1, n - 1))
            self.col = max(0, min(self.cols - 1, (p[1] or 1) - 1 if len(p) > 1 else 0))
        elif command == 'G': self.col = max(0, min(self.cols - 1, n - 1))
        elif command == 'A': self.row = max(0, self.row - n)
        elif command == 'B': self.row = min(self.rows - 1, self.row + n)
        elif command == 'C': self.col = min(self.cols - 1, self.col + n)
        elif command == 'D': self.col = max(0, self.col - n)
        elif command == 'E': self.row, self.col = min(self.rows - 1, self.row + n), 0
        elif command == 'F': self.row, self.col = max(0, self.row - n), 0
        elif command == 'K':
            start, stop = (0, self.cols) if p[0] == 2 else ((0, self.col + 1) if p[0] == 1 else (self.col, self.cols))
            self.cells[self.row][start:stop] = [' '] * (stop - start)
        elif command == 'J':
            if p[0] in (2, 3): self.cells = [[' '] * self.cols for _ in range(self.rows)]
            elif p[0] == 0:
                self.cells[self.row][self.col:] = [' '] * (self.cols - self.col)
                for row in range(self.row + 1, self.rows): self.cells[row] = [' '] * self.cols
        self.pending_wrap = False

    def text(self):
        return '\n'.join(''.join(line).rstrip() for line in self.cells)

class Session(PtySession):
    def __init__(self, binary):
        self.screen = Screen()
        super().__init__(binary, 'dark')
    def read_once(self, timeout):
        data = super().read_once(timeout)
        self.screen.feed(data)
        return data
    def keys(self, data):
        self.send(data)
        output = plain(self.drain(timeout=15.0))
        assert self.child.poll() is None, 'child unexpectedly exited'
        return output
    def resize(self, cols, rows):
        self.screen.resize(cols, rows)
        return super().resize(cols, rows)

s = Session(args.binary.resolve())
passed = []
try:
    s.wait_for('Ready')
    s.resize(args.cols, 24)
    for length in (args.cols - 1, args.cols, args.cols + 1, 3 * args.cols + 5):
        start_row, start_col = s.screen.row, s.screen.col
        assert start_col == 0, s.screen.text()
        s.keys(b'A' * length)
        expected_row = min(s.screen.rows - 1, start_row + length // args.cols)
        visible_origin = expected_row - length // args.cols
        expected_col = length % args.cols
        assert (s.screen.row, s.screen.col) == (expected_row, expected_col), (length, (s.screen.row,s.screen.col), (expected_row,expected_col), s.screen.text())
        assert ''.join(''.join(line) for line in s.screen.cells[visible_origin:expected_row+1]).startswith('A' * length), s.screen.text()
        s.keys(END + b'\x7f' * length)
        assert (s.screen.row, s.screen.col) == (visible_origin, 0), s.screen.text()
        assert all(not ''.join(line).strip() for line in s.screen.cells[visible_origin:expected_row+1]), s.screen.text()
    passed.append('exact width, next row, three wrapped rows, backspace clears stale rows')

    # Use a short terminal to exercise viewport movement with a bounded key batch.
    s.resize(args.cols, min(12, max(3, 900 // args.cols - 3)))
    length = args.cols * (s.screen.rows + 3) + 5
    payload = ''.join(chr(65 + (index // args.cols) % 26) for index in range(length))
    s.keys(payload.encode())
    assert (s.screen.row, s.screen.col) == (s.screen.rows - 1, 5), s.screen.text()
    first_visible_row = length // args.cols + 1 - s.screen.rows
    assert ''.join(''.join(line) for line in s.screen.cells).startswith(payload[first_visible_row * args.cols:]), s.screen.text()
    s.keys(HOME)
    assert (s.screen.row, s.screen.col) == (0, 0), s.screen.text()
    assert ''.join(s.screen.cells[0]) == payload[:args.cols], s.screen.text()
    s.keys(b'\x1b[3~')  # Delete the character at Home, preserving the long suffix.
    payload = payload[1:]
    assert ''.join(s.screen.cells[0]) == payload[:args.cols], s.screen.text()
    s.keys(END)
    assert (s.screen.row, s.screen.col) == (s.screen.rows - 1, 4), s.screen.text()
    s.keys(b'\x7f' * len(payload))
    assert s.screen.col == 0 and not s.screen.text().strip(), s.screen.text()
    passed.append('viewport taller than terminal, Home/End/Delete, complete clearing')
    s.resize(args.cols, 24)

    before = len(s.transcript)
    s.keys(HOME + LEFT + b'\x1b[21~')
    assert len(s.transcript) == before, repr(bytes(s.transcript[before:]))
    passed.append('Home/Left at start and unhandled F10 produce no redraw')

    s.command('PRINT "HISTORY_MARKER"')
    s.keys(b'\x1b[A')
    assert 'PRINT "HISTORY_MARKER"' in s.screen.text(), s.screen.text()
    s.keys(b'\x1b[B')
    assert not ''.join(s.screen.cells[s.screen.row]).strip(), s.screen.text()
    assert 'HISTORY_MARKER' in s.keys(b'\x1b[A\r')
    passed.append('Up recalls history, Down restores blank draft, recalled Enter executes')

    s.keys(b'PRINT "ABX')
    output = s.keys(b'\x7fC"\r')
    assert re.search(r'(?:^|[\r\n])ABC[\r\n]', output), repr(output)
    output = s.command('PRINT "áñΩ"')
    assert 'áñΩ' in output and 'Ready' in output, repr(output)
    passed.append('ASCII, UTF-8, backspace and Enter execution')

    def current_input():
        return ''.join(s.screen.cells[s.screen.row]).rstrip()

    s.keys(b'val')
    assert current_input() == 'VAL', s.screen.text()
    s.keys(b'or=7')
    assert current_input() == 'valor=7', s.screen.text()
    s.keys(b'\r')
    s.keys(b'printer=9')
    assert current_input() == 'printer=9', s.screen.text()
    s.keys(b'\r')
    output = s.command('print valor;printer')
    assert re.search(r'(?:^|[\r\n])\s*7\s+9\s*(?:[\r\n]|$)', output), repr(output)

    s.keys(b'VALor=7')
    s.keys(HOME + b'\x1b[3~' * 3)
    assert current_input() == 'OR=7', s.screen.text()
    s.keys(b'prueba')
    assert current_input() == 'pruebaor=7', s.screen.text()
    s.keys(b'\r')

    s.keys(b'for mat=9')
    s.keys(HOME + b'\x1b[C' * 4 + b'\x7f')
    assert current_input() == 'format=9', s.screen.text()
    s.keys(ESC)

    s.keys(b'load"demo')
    assert current_input() == 'LOAD "demo.bas"', s.screen.text()
    s.keys(b'.txt"')
    assert current_input() == 'LOAD "demo.txt"', s.screen.text()
    s.keys(ESC)

    s.command('10 valor=7')
    s.command('EDIT 10')
    s.keys(HOME + b'\x1b[C' * 3 + b'\x1b[3~' * 3)
    assert current_input() == '10 OR=7', s.screen.text()
    s.keys(b'prueba')
    assert current_input() == '10 pruebaor=7', s.screen.text()
    s.keys(b'\r')
    assert '10 pruebaor=7' in s.command('LIST'), s.screen.text()
    passed.append('keyword prefixes, Delete/Backspace joins, lowercase file assistance and EDIT preserve typed case')

    s.command('10 PRINT "EDIT_OLD"')
    s.command('EDIT 10')
    s.keys(END + LEFT + b'_NEW\r')
    assert 'EDIT_OLD_NEW' in s.command('RUN')
    s.command('EDIT 10')
    s.keys(END + LEFT + b'_CANCEL')
    s.keys(ESC)
    output = s.command('RUN')
    assert 'EDIT_OLD_NEW' in output and 'EDIT_OLD_NEW_CANCEL' not in output, repr(output)
    passed.append('EDIT line commits Enter and restores the original on Esc')

    s.resize(120, 32)
    s.send(b'EDIT\r')
    s.wait_for(ENTER_ALT)
    s.send(ESC)
    s.wait_for(LEAVE_ALT)
    assert 'AFTER_EDITOR' in s.command('PRINT "AFTER_EDITOR"')
    s.command('20 END')
    s.send(b'DEBUG\r')
    s.wait_for('Ln 10 Stmt 1')
    s.send(F4)
    s.wait_for(LEAVE_ALT)
    s.send(ESC)
    s.wait_for('Ln 10 Stmt 1')
    s.send(F5)
    s.wait_for('Ready')
    assert 'AFTER_DEBUG' in s.command('PRINT "AFTER_DEBUG"')
    passed.append('fullscreen editor, debugger output toggle and return to immediate input')

    s.keys(b'PRINT "UNEXECUTED"')
    s.send(b'\x03')
    s.child.wait(timeout=5)
    s.drain()
    flags = termios.tcgetattr(s.master)[3]
    assert flags & termios.ICANON and flags & termios.ECHO, flags
    assert s.screen.cursor_visible, 'cursor hidden after Ctrl+C'
    passed.append('Ctrl+C exits and restores canonical input, terminal echo and visible cursor')
    for item in passed: print(f'PASS ({args.cols} columns):', item)
except BaseException:
    print('SCREEN:\n' + s.screen.text(), file=sys.stderr)
    print('TAIL:', repr(bytes(s.transcript[-3000:])), file=sys.stderr)
    raise
finally:
    s.close()
