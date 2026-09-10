"""Exercise real Ctrl+C events in isolated interpreter processes, without UI focus."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import queue
import signal
import subprocess
import sys
import tempfile
import threading
import time


def send_windows_ctrl_c(pid: int) -> None:
    # This helper joins only the fresh, hidden console created for the child.
    # Never broadcast a control event into the calling terminal's console.
    import ctypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.FreeConsole()
    if not kernel.AttachConsole(pid):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        processes = (ctypes.c_ulong * 16)()
        count = kernel.GetConsoleProcessList(processes, len(processes))
        members = set(processes[:count])
        if not count or count > len(processes) or members != {pid, os.getpid()}:
            raise RuntimeError(f'unexpected console membership: {members}')
        if not kernel.SetConsoleCtrlHandler(None, True):
            raise ctypes.WinError(ctypes.get_last_error())
        if not kernel.GenerateConsoleCtrlEvent(0, 0):
            raise ctypes.WinError(ctypes.get_last_error())
        time.sleep(0.1)
    finally:
        kernel.FreeConsole()


class Session:
    def __init__(self, command: list[str], root: Path):
        options = {}
        if os.name == 'nt':
            startup = subprocess.STARTUPINFO()
            startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
            startup.wShowWindow = 0
            options = {'creationflags': subprocess.CREATE_NEW_CONSOLE, 'startupinfo': startup}
        else:
            options = {'start_new_session': True}
        self.child = subprocess.Popen(command, cwd=root, stdin=subprocess.PIPE,
                                      stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                      text=True, encoding='utf-8', errors='replace', bufsize=1,
                                      env={**os.environ, 'PYTHONUTF8': '1', 'PYTHONIOENCODING': 'utf-8',
                                           'NO_COLOR': '1', 'AVL_BASIC_WINDOW': '0'}, **options)
        self.lines = queue.Queue()
        self.history = []
        threading.Thread(target=self._read, daemon=True).start()
        self.wait('Ready')

    def _read(self):
        for line in self.child.stdout:
            self.lines.put(line.rstrip('\r\n'))
        self.lines.put(None)

    def wait(self, expected: str, timeout: float = 5.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                line = self.lines.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break
            if line is None:
                raise AssertionError(f'interpreter exited: {self.history[-15:]}')
            self.history.append(line)
            if line == expected:
                return
        raise AssertionError(f'timeout waiting for {expected!r}: {self.history[-15:]}')

    def send(self, command: str):
        self.child.stdin.write(command + '\n')
        self.child.stdin.flush()

    def command(self, command: str, expected: str | None = None):
        self.send(command)
        if expected is not None:
            self.wait(expected)
        self.wait('Ready')

    def interrupt_loop(self, loop: str):
        self.send('PRINT "BUSY":' + loop + ':PRINT "UNREACHABLE"')
        self.wait('BUSY')
        time.sleep(0.1)  # Let the interpreter enter the loop before signalling.
        started = time.monotonic()
        if os.name == 'nt':
            subprocess.run([sys.executable, str(Path(__file__).resolve()),
                            '--signal-child', str(self.child.pid)], check=True,
                           capture_output=True, timeout=5)
        else:
            self.child.send_signal(signal.SIGINT)
        self.wait('Execution interrupted by user.')
        self.wait('Ready')
        assert 'UNREACHABLE' not in self.history
        return time.monotonic() - started

    def program(self, source: list[str]):
        self.send('\n'.join([*source, 'PRINT "LOADED"']))
        self.wait('LOADED')
        self.wait('Ready')

    def close(self):
        if self.child.poll() is None:
            self.child.terminate()
        self.child.wait(timeout=5)
        self.child.stdin.close()
        self.child.stdout.close()


def check(label: str, command: list[str]):
    with tempfile.TemporaryDirectory(prefix='avl-ctrl-c-') as folder:
        root = Path(folder).resolve()
        assert root.parent == Path(tempfile.gettempdir()).resolve()
        session = Session(command, root)
        try:
            times = []
            for loop in ('WHILE 1:A=A+1:WEND', 'FOR I=1 TO 1E12:A=A+1:NEXT',
                         'WHILE 1:WEND', 'FOR I=1 TO 1E12:NEXT'):
                times.append(session.interrupt_loop(loop))
                session.command('PRINT A>0', '-1')
                session.command('A=0:FOR I=1 TO 3:A=A+1:NEXT:PRINT A', ' 3')
            session.command('CONT', 'There is no stopped program to continue.')
            session.program([
                '10 OPEN "kept.csv" FOR OUTPUT AS #1', '20 WRITE #1,"before"',
                '30 S=0', '40 FOR K=1 TO 2', '50 S=S+K', '60 IF K=1 THEN STOP',
                '70 NEXT K', '80 WRITE #1,"after"', '90 CLOSE #1', '100 PRINT "RESUMED";S',
            ])
            session.command('RUN', 'Line 60. Program stopped.')
            times.append(session.interrupt_loop('WHILE -1:A=A+1:WEND'))
            session.command('WRITE #1,"prompt"')
            session.command('CONT', 'RESUMED 3')
            assert (root / 'kept.csv').read_bytes() == b'"before"\n"prompt"\n"after"\n'
            session.command('NEW')
            session.program(['10 PRINT "FRESH RUN"', '20 END'])
            session.command('RUN', 'FRESH RUN')
            session.send('QUIT')
            assert session.child.wait(timeout=5) == 0
            print(f'PASS [{label}] WHILE, FOR, prompt recovery, STOP/CONT, files and fresh RUN; '
                  f'max signal-to-prompt check {max(times):.3f}s')
        finally:
            session.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--signal-child', type=int)
    parser.add_argument('--rust-bin', type=Path)
    parser.add_argument('--python-repo', type=Path)
    args = parser.parse_args()
    if args.signal_child is not None:
        if os.name != 'nt':
            parser.error('--signal-child is Windows-only')
        send_windows_ctrl_c(args.signal_child)
        return 0
    if not args.rust_bin and not args.python_repo:
        parser.error('specify --rust-bin and/or --python-repo')
    if args.python_repo:
        check('Python', [sys.executable, '-X', 'utf8', str(args.python_repo.resolve() / 'basic.py')])
    if args.rust_bin:
        check('Rust', [str(args.rust_bin.resolve())])
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
