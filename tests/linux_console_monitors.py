#!/usr/bin/env python3
"""Opt-in Linux GUI regression on an isolated Xorg dummy display.

Requires Xorg with the dummy driver, Openbox, xterm, xrandr, xprop, xwininfo, xdotool and
xkbcomp. Does not install packages or change the current desktop. Run e.g.:
  python3 tests/linux_console_monitors.py --binary target/debug/avl-basic
--tools-root can point at an extracted package tree containing usr/bin and
usr/lib/xorg. A real terminal/PTY is required because piped stdin bypasses the
interpreter's terminal-ownership check. This is simulated monitor coverage,
not validation of a physical desktop or every window manager.
"""

from pathlib import Path
import argparse
import json
import os
import re
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time


XORG_CONFIG = """Section "ServerFlags"
 Option "AutoAddDevices" "false"
 Option "AutoEnableDevices" "false"
 Option "AutoAddGPU" "false"
EndSection
Section "Device"
 Identifier "DummyDevice"
 Driver "dummy"
 VideoRam 256000
EndSection
Section "Monitor"
 Identifier "DummyMonitor"
 HorizSync 5.0-1000.0
 VertRefresh 5.0-200.0
 Modeline "1280x720" 74.50 1280 1344 1472 1664 720 723 728 748 -hsync +vsync
EndSection
Section "Screen"
 Identifier "DummyScreen"
 Device "DummyDevice"
 Monitor "DummyMonitor"
 DefaultDepth 24
 SubSection "Display"
  Depth 24
  Modes "1280x720"
  Virtual 2560 720
 EndSubSection
EndSection
"""

BASIC_PROGRAM = """10 MODE 960 : SCREEN
20 INPUT "Position both windows, then Enter: ",R$
30 SCREEN FULL ON : LOCATE 1,1 : GPRINT "Press N for console INPUT"
40 IF INKEY$="" THEN GOTO 40
50 INPUT "VALUE: ",A$
60 LOCATE 1,3 : GPRINT A$
70 STOP
80 LOCATE 1,5 : GPRINT "FINISHED"
90 END
"""

DEBUG_PROGRAM = """10 SCREEN FULL ON
20 GPRINT "DEBUG STEP"
30 SCREEN FULL OFF
40 SCREEN FULL ON
50 MODE 1280
60 SCREEN FULL OFF
70 END
"""

POSITION_PROGRAM = """10 SCREEN
20 MODE 960
30 SCREEN FULL ON
40 GPRINT "DEBUG SECOND MONITOR"
50 SCREEN FULL OFF
60 SCREEN FULL ON
70 MODE 1280
80 SCREEN FULL OFF
90 END
"""


class Desktop:
    def __init__(self, binary, directory, tools_root):
        self.binary = binary
        self.directory = directory
        self.children = []
        self.logs = []
        self.resume_timer = None
        self.wm = None
        self.env = os.environ.copy()
        self.env.pop("WAYLAND_DISPLAY", None)
        self.env["XDG_SESSION_TYPE"] = "x11"
        self.tools_root = tools_root
        if tools_root:
            private_libs = str(tools_root / "usr/lib/x86_64-linux-gnu")
            self.env["LD_LIBRARY_PATH"] = private_libs + ":" + self.env.get("LD_LIBRARY_PATH", "")
            self.env["XDG_DATA_DIRS"] = str(tools_root / "usr/share") + ":/usr/local/share:/usr/share"

    def tool(self, name):
        relative = "usr/lib/xorg/Xorg" if name == "Xorg" else "usr/bin/" + name
        if self.tools_root and (self.tools_root / relative).is_file():
            return str(self.tools_root / relative)
        path = shutil.which(name)
        if not path:
            raise RuntimeError(f"Missing prerequisite: {name}")
        return path

    def run(self, *args, check=True):
        result = subprocess.run(
            [str(arg) for arg in args], env=self.env, cwd=self.directory,
            text=True, capture_output=True, timeout=10,
        )
        if check and result.returncode:
            raise RuntimeError(f"{args!r}: {result.stdout}\n{result.stderr}")
        return result.stdout

    def start(self, name, *args, pass_fds=()):
        log = (self.directory / (name + ".log")).open("w")
        self.logs.append(log)
        child = subprocess.Popen(
            [str(arg) for arg in args], env=self.env, cwd=self.directory,
            stdout=log, stderr=subprocess.STDOUT, pass_fds=pass_fds,
        )
        self.children.append(child)
        return child

    def prepare(self):
        config = self.directory / "xorg.conf"
        config.write_text(XORG_CONFIG)
        read_fd, write_fd = os.pipe()
        # Explicit display selection also works with WSLg's read-only socket
        # directory, where Xorg's auto-allocation otherwise retries forever.
        for number in range(90, 190):
            if Path(f"/tmp/.X{number}-lock").exists() or Path(f"/tmp/.X11-unix/X{number}").exists():
                continue
            with socket.socket(socket.AF_UNIX) as connection:
                if connection.connect_ex(f"\0/tmp/.X11-unix/X{number}") == 0:
                    continue
            break
        else:
            raise RuntimeError("No unused isolated X11 display found")
        args = [self.tool("Xorg"), f":{number}", "-displayfd", str(write_fd), "-config", config,
                "-logfile", self.directory / "Xorg.log", "-nolisten", "tcp",
                "-ac", "-noreset", "-novtswitch", "-sharevts"]
        if self.tools_root:
            args.extend(["-modulepath", self.tools_root / "usr/lib/xorg/modules"])
        try:
            server = self.start("xserver", *args, pass_fds=(write_fd,))
            os.close(write_fd)
            write_fd = None
            if not select.select([read_fd], [], [], 10)[0]:
                raise RuntimeError("Isolated Xorg did not start within 10 seconds")
            display_number = os.read(read_fd, 64).decode().strip()
            if not display_number.isdigit() or server.poll() is not None:
                raise RuntimeError("Isolated Xorg failed; inspect xserver.log/Xorg.log")
            self.env["DISPLAY"] = ":" + display_number
        finally:
            os.close(read_fd)
            if write_fd is not None:
                os.close(write_fd)
        self.run(self.tool("xrandr"), "--addmode", "DUMMY1", "1280x720")
        self.run(self.tool("xrandr"), "--output", "DUMMY0", "--mode", "1280x720", "--pos", "0x0",
                 "--output", "DUMMY1", "--mode", "1280x720", "--pos", "1280x0")
        topology = self.run(self.tool("xrandr"), "--listmonitors")
        assert "Monitors: 2" in topology, topology
        (self.directory / "topology.txt").write_text(topology)
        config = self.directory / "openbox.xml"
        config.write_text('<openbox_config xmlns="http://openbox.org/3.4/rc">'
                          '<focus><focusNew>yes</focusNew></focus>'
                          '<theme><name>Clearlooks</name></theme></openbox_config>')
        self.wm = self.start("openbox", self.tool("openbox"), "--config-file", config)
        for _ in range(50):
            if "window id # 0x" in self.run(self.tool("xprop"), "-root", "_NET_SUPPORTING_WM_CHECK"):
                return
            if self.wm.poll() is not None:
                raise RuntimeError("Openbox exited before becoming the window manager")
            time.sleep(.1)
        raise RuntimeError("Openbox did not become the window manager")

    def xdo(self, *args):
        return self.run(self.tool("xdotool"), *args)

    def find_window(self, name):
        for _ in range(50):
            ids = self.run(self.tool("xdotool"), "search", "--name", name, check=False).split()
            if ids:
                assert len(ids) == 1, f"Ambiguous test window: {name} {ids}"
                return ids[0]
            time.sleep(.1)
        raise RuntimeError(f"Test window did not appear: {name}")

    def command(self, text):
        self.xdo("type", "--clearmodifiers", "--delay", "1", text)
        self.xdo("key", "Return")
        time.sleep(.3)

    def wait_terminal_text(self, text, count=1):
        path = self.directory / "terminal.log"
        for _ in range(100):
            if path.exists() and path.read_text(errors="replace").count(text) >= count:
                time.sleep(.1)  # Let the raw console reader enter its input loop.
                return
            time.sleep(.1)
        raise RuntimeError(f"Interpreter did not produce {text!r} x{count}")

    def snapshot(self, phase, graphics, terminal):
        time.sleep(.5)
        state = self.run(self.tool("xprop"), "-id", graphics, "_NET_WM_STATE", "_NET_FRAME_EXTENTS")
        geometry = self.xdo("getwindowgeometry", "--shell", graphics)
        info = self.run(self.tool("xwininfo"), "-id", graphics)
        native_rect = {key: int(re.search(pattern, info)[1]) for key, pattern in (
            ("X", r"Absolute upper-left X:\s+(-?\d+)"),
            ("Y", r"Absolute upper-left Y:\s+(-?\d+)"),
            ("WIDTH", r"Width:\s+(\d+)"), ("HEIGHT", r"Height:\s+(\d+)"))}
        return {"phase": phase, "fullscreen": "_NET_WM_STATE_FULLSCREEN" in state,
                "graphics_state": state, "graphics_geometry": geometry,
                "graphics_rect": native_rect,
                "terminal_geometry": self.xdo("getwindowgeometry", "--shell", terminal),
                "active_window": self.run(self.tool("xprop"), "-root", "_NET_ACTIVE_WINDOW")}

    def all_desktops_without_current(self, terminal):
        # Reproduce XWayland's all-desktops CARDINAL/32 with no current-desktop
        # property. These properties belong only to our isolated Xorg server.
        self.xdo("set_desktop_for_window", terminal, "4294967295")
        for _ in range(50):
            state = self.run(self.tool("xprop"), "-id", terminal, "_NET_WM_DESKTOP")
            if "= 4294967295" in state:
                break
            time.sleep(.1)
        assert "= 4294967295" in state, state
        self.run(self.tool("xprop"), "-root", "-remove", "_NET_CURRENT_DESKTOP")
        current = self.run(self.tool("xprop"), "-root", "_NET_CURRENT_DESKTOP")
        assert "not found" in current, current
        (self.directory / "desktop-properties.txt").write_text(state + current)

    def unknown_terminal_location(self, scenario, terminal):
        if scenario == "missing-current-desktop":
            self.run(self.tool("xprop"), "-root", "-remove", "_NET_CURRENT_DESKTOP")
            state = self.run(self.tool("xprop"), "-id", terminal, "_NET_WM_DESKTOP")
            current = self.run(self.tool("xprop"), "-root", "_NET_CURRENT_DESKTOP")
            assert "= 0" in state, state
            assert "not found" in current, current
        else:
            # The terminal can no longer be associated with our process, as
            # with a native Wayland terminal absent from X11's client list.
            self.run(self.tool("xprop"), "-id", terminal, "-remove", "_NET_WM_PID")
            state = self.run(self.tool("xprop"), "-id", terminal, "_NET_WM_PID")
            assert "not found" in state, state
            current = ""
        (self.directory / "desktop-properties.txt").write_text(state + current)

    def scenario(self, scenario):
        if scenario.startswith("debugger-"):
            return self.debugger_scenario(scenario)
        (self.directory / "probe.bas").write_text(BASIC_PROGRAM)
        trace = self.directory / "trace.log"
        (self.directory / "terminal.log").write_text("")
        self.start("xterm", self.tool("xterm"), "-T", "AVL_REPRO_TERMINAL",
                   "-l", "-lf", self.directory / "terminal.log",
                   "-geometry", "85x28+1320+35", "-e", sys.executable,
                   Path(__file__).resolve(), "--terminal-child", self.binary, trace)
        terminal = self.find_window("^AVL_REPRO_TERMINAL$")
        self.wait_terminal_text("Ready")
        self.xdo("windowactivate", "--sync", terminal)
        self.command('LOAD "probe.bas"')
        self.wait_terminal_text("Ready", 2)
        self.command("RUN")
        graphics = self.find_window("^AVL BASIC Graphics$")
        self.xdo("windowmove", graphics, "100", "80")
        same_monitor = scenario in ("same-monitor", "all-desktops-same-monitor")
        x = "2300" if scenario == "clipped-terminal" else ("500" if same_monitor else "1340")
        self.xdo("windowmove", terminal, x, "50")
        self.xdo("windowactivate", "--sync", terminal)
        self.command("")
        results = [self.snapshot("before INPUT", graphics, terminal)]
        self.xdo("windowactivate", "--sync", graphics)
        if scenario.startswith("all-desktops"):
            self.all_desktops_without_current(terminal)
        unknown_location = scenario in ("missing-current-desktop", "unknown-terminal")
        if unknown_location:
            self.unknown_terminal_location(scenario, terminal)
        if scenario == "slow-wm":
            os.kill(self.wm.pid, signal.SIGSTOP)
            self.resume_timer = threading.Timer(.35, lambda: os.kill(self.wm.pid, signal.SIGCONT))
            self.resume_timer.start()
        self.xdo("key", "n")
        results.append(self.snapshot("during INPUT", graphics, terminal))
        self.xdo("windowactivate", "--sync", terminal)
        self.command("native-terminal-value")
        results.append(self.snapshot("STOP", graphics, terminal))
        self.xdo("windowactivate", "--sync", terminal)
        self.command("CONT")
        results.append(self.snapshot("END", graphics, terminal))
        self.xdo("windowactivate", "--sync", terminal)
        self.command("SCREEN CLOSE")
        assert not self.run(self.tool("xdotool"), "search", "--name", "^AVL BASIC Graphics$", check=False).strip()
        self.command("QUIT")
        (self.directory / "results.json").write_text(json.dumps(results, indent=2))
        observed = [record["fullscreen"] for record in results]
        expected = [True] + [not (same_monitor or unknown_location)] * 3
        assert observed == expected, f"{scenario}: fullscreen {observed}, expected {expected}"
        for record in results:
            if record["fullscreen"]:
                geometry = dict(re.findall(r"^(\w+)=(\d+)$", record["graphics_geometry"], re.MULTILINE))
                assert [geometry[k] for k in ("X", "Y", "WIDTH", "HEIGHT")] == ["0", "0", "1280", "720"], record
        if scenario == "slow-wm":
            assert "activation not confirmed within 200 ms" in trace.read_text(), "Slow-WM condition was not exercised"
        if scenario.startswith("all-desktops"):
            # A pass caused by a generic unknown-terminal fallback would hide
            # regressions in the actual desktop-property interpretation.
            assert f"terminal_separate={str(not same_monitor).lower()}" in trace.read_text(), trace.read_text()
        if unknown_location:
            text = trace.read_text()
            assert "terminal/location unavailable" in text, text
            assert "suspending fullscreen" in text, text
            assert "terminal_separate=" not in text, text
            if scenario == "unknown-terminal":
                assert "missing or invalid _NET_WM_PID" in text, text
            else:
                assert "(Some(0), None)" in text, text
        return {"scenario": scenario, "fullscreen": observed, "status": "PASS"}

    def debugger_scenario(self, scenario):
        position_case = scenario == "debugger-secondary-monitor"
        (self.directory / "probe.bas").write_text(POSITION_PROGRAM if position_case else DEBUG_PROGRAM)
        (self.directory / "terminal.log").write_text("")
        trace = self.directory / "trace.log"
        self.start("xterm", self.tool("xterm"), "-T", "AVL_DEBUG_TERMINAL",
                   "-l", "-lf", self.directory / "terminal.log", "-geometry", "85x28+1340+50",
                   "-e", sys.executable, Path(__file__).resolve(),
                   "--terminal-child", self.binary, trace)
        terminal = self.find_window("^AVL_DEBUG_TERMINAL$")
        self.wait_terminal_text("Ready")
        self.xdo("windowactivate", "--sync", terminal)
        if not position_case:
            self.command("MODE 960")
            self.wait_terminal_text("Ready", 2)
            graphics = self.find_window("^AVL BASIC Graphics$")
            self.xdo("windowmove", graphics, "100", "80")
        separate = scenario != "debugger-same-monitor"
        self.xdo("windowmove", terminal, "150" if position_case or not separate else "1340", "50")
        self.xdo("windowactivate", "--sync", terminal)
        self.command('LOAD "probe.bas"')
        self.wait_terminal_text("Ready", 2 if position_case else 3)
        self.command("DEBUG")
        self.wait_terminal_text("F6")  # The debugger's step controls are visible.
        unknown_terminal = scenario == "debugger-unknown-terminal"
        if unknown_terminal:
            self.unknown_terminal_location("unknown-terminal", terminal)
        results = []
        if position_case:
            self.xdo("key", "F6")  # Execute SCREEN before moving the new window.
            graphics = self.find_window("^AVL BASIC Graphics$")
            self.xdo("windowmove", graphics, "1320", "50")
            self.xdo("windowactivate", "--sync", terminal)
            results.append(self.snapshot("SCREEN_MOVED", graphics, terminal))
        phases = (["MODE_960"] if position_case else []) + ["ON", "DRAW", "OFF", "ON_AGAIN", "MODE", "OFF_AGAIN"]
        for phase in phases:
            assert self.xdo("getactivewindow").strip() == terminal, "Debugger lost terminal focus"
            self.xdo("key", "F6")
            time.sleep(.35)
            graphics = self.find_window("^AVL BASIC Graphics$")
            record = self.snapshot(phase, graphics, terminal)
            record["terminal_focused"] = self.xdo("getactivewindow").strip() == terminal
            results.append(record)
            if unknown_terminal:
                # An unidentified terminal cannot be activated automatically;
                # it must at least remain reachable through windowed graphics.
                self.xdo("windowactivate", "--sync", terminal)
            else:
                assert record["terminal_focused"], record
        observed = [record["fullscreen"] for record in results]
        keep_fullscreen = separate and not unknown_terminal
        expected = ([False, False] if position_case else []) + [keep_fullscreen, keep_fullscreen, False, keep_fullscreen, keep_fullscreen, False]
        (self.directory / "results.json").write_text(json.dumps(results, indent=2))
        assert observed == expected, f"{scenario}: fullscreen {observed}, expected {expected}"
        if position_case:
            self.assert_secondary_monitor_placement(results)
        # After MODE and OFF, the normal-size window must use the new canvas.
        geometry = dict(re.findall(r"^(\w+)=(\d+)$", results[-1]["graphics_geometry"], re.MULTILINE))
        assert [geometry[k] for k in ("WIDTH", "HEIGHT")] == ["1280", "720"], results[-1]
        if unknown_terminal:
            assert "suspending fullscreen" in trace.read_text()
        self.xdo("key", "Escape")
        self.wait_terminal_text("Ready", 3 if position_case else 4)
        self.command("SCREEN CLOSE")
        assert not self.run(self.tool("xdotool"), "search", "--name", "^AVL BASIC Graphics$", check=False).strip()
        self.command("QUIT")
        return {"scenario": scenario, "fullscreen": observed, "status": "PASS"}

    def assert_secondary_monitor_placement(self, results):
        def geometry(record):
            return record["graphics_rect"]

        def outer_origin(record):
            rect = geometry(record)
            extents = re.search(r"_NET_FRAME_EXTENTS\(CARDINAL\) = (\d+), (\d+), (\d+), (\d+)", record["graphics_state"])
            assert extents, record
            return (rect["X"] - int(extents[1]), rect["Y"] - int(extents[3]))

        initial = outer_origin(results[0])
        for record in results:
            rect = geometry(record)
            if record["fullscreen"]:
                assert [rect[k] for k in ("X", "Y", "WIDTH", "HEIGHT")] == [1280, 0, 1280, 720], record
            else:
                # The 1280 canvas plus decorations may need clamping, but it
                # must never return to the first monitor with the terminal.
                assert 1280 <= rect["X"] + rect["WIDTH"] / 2 < 2560, record
                if record["phase"] in ("MODE_960", "OFF"):
                    current = outer_origin(record)
                    assert all(abs(a - b) <= 2 for a, b in zip(initial, current)), (initial, current, record)

    def close(self):
        if self.resume_timer:
            self.resume_timer.cancel()
            self.resume_timer.join()
        if self.wm and self.wm.poll() is None:
            os.kill(self.wm.pid, signal.SIGCONT)
        for child in reversed(self.children):
            if child.poll() is None:
                child.terminate()
        for child in reversed(self.children):
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
        for log in self.logs:
            log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/debug/avl-basic"))
    parser.add_argument("--tools-root", type=Path)
    parser.add_argument("--output-dir", type=Path, help="Keep native state and logs in this directory")
    scenarios = ["normal", "slow-wm", "clipped-terminal", "same-monitor",
                 "all-desktops", "all-desktops-same-monitor",
                 "missing-current-desktop", "unknown-terminal",
                 "debugger-same-monitor", "debugger-separate-monitors", "debugger-unknown-terminal",
                 "debugger-secondary-monitor"]
    parser.add_argument("--scenario", choices=["all", *scenarios], default="all")
    args = parser.parse_args()
    binary = args.binary.resolve()
    assert binary.is_file(), f"Interpreter does not exist: {binary}"
    tools_root = args.tools_root.resolve() if args.tools_root else None
    with tempfile.TemporaryDirectory(prefix="avl-console-monitors-") as temporary:
        # Keep execution on the Linux filesystem: xterm correctly refuses to
        # log to a file that DrvFS reports as owned by another uid.
        root = Path(temporary)
        output = args.output_dir.resolve() if args.output_dir else None
        if output:
            output.mkdir(parents=True, exist_ok=True)
        for scenario in (scenarios if args.scenario == "all" else [args.scenario]):
            directory = root / scenario
            directory.mkdir(exist_ok=True)
            desktop = Desktop(binary, directory, tools_root)
            try:
                desktop.prepare()
                print(json.dumps(desktop.scenario(scenario)), flush=True)
            except Exception:
                for name in ("trace.log", "terminal.log", "xserver.log", "openbox.log", "xterm.log"):
                    path = directory / name
                    if path.exists():
                        print(f"--- {path}\n{path.read_text()[-12000:]}", file=sys.stderr)
                raise
            finally:
                desktop.close()
                if output:
                    shutil.copytree(directory, output / scenario, dirs_exist_ok=True)


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--terminal-child":
        os.environ["AVL_BASIC_CONSOLE_TRACE"] = "1"
        os.environ["AVL_BASIC_WINDOW"] = "1"
        with open(sys.argv[3], "w") as trace:
            os.dup2(trace.fileno(), 2)
        os.execv(sys.argv[2], [sys.argv[2]])
    main()
