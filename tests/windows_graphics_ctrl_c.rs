#![cfg(windows)]

use std::ffi::c_void;
use std::io::{BufRead, BufReader, Write};
use std::mem::size_of;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

type Hwnd = *mut c_void;
type Bool = i32;
type Dword = u32;
type Lparam = isize;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DesktopRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_SCANCODE: u32 = 0x0008;
const CREATE_NEW_CONSOLE: u32 = 0x00000010;
const SCAN_A: u16 = 0x1E;
const SCAN_ENTER: u16 = 0x1C;
const SCAN_ESC: u16 = 0x01;
const SCAN_N: u16 = 0x31;

#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    kind: u32,
    data: InputData,
}

#[repr(C)]
#[derive(Clone, Copy)]
union InputData {
    keyboard: KeyboardInput,
    _padding: [usize; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyboardInput {
    virtual_key: u16,
    scan_code: u16,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[link(name = "user32")]
extern "system" {
    fn AttachThreadInput(id_attach: Dword, id_attach_to: Dword, attach: Bool) -> Bool;
    fn BringWindowToTop(hwnd: Hwnd) -> Bool;
    fn EnumWindows(callback: extern "system" fn(Hwnd, Lparam) -> Bool, lparam: Lparam) -> Bool;
    fn EnumDisplayMonitors(
        dc: Hwnd,
        clip: *const DesktopRect,
        callback: extern "system" fn(Hwnd, Hwnd, *mut DesktopRect, Lparam) -> Bool,
        lparam: Lparam,
    ) -> Bool;
    fn GetForegroundWindow() -> Hwnd;
    fn GetClassNameW(hwnd: Hwnd, class_name: *mut u16, max_count: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, max_count: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut Dword) -> Dword;
    fn IsWindowVisible(hwnd: Hwnd) -> Bool;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut DesktopRect) -> Bool;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    fn SetThreadDpiAwarenessContext(context: Hwnd) -> Hwnd;
    fn SetWindowPos(
        hwnd: Hwnd,
        after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> Bool;
    fn PostMessageW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> Bool;
    fn SetActiveWindow(hwnd: Hwnd) -> Hwnd;
    fn SetFocus(hwnd: Hwnd) -> Hwnd;
    fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn ShowWindow(hwnd: Hwnd, cmd_show: i32) -> Bool;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> Dword;
}

#[test]
#[ignore = "opens a real console and graphics window and verifies focus after SCREEN CLOSE"]
fn screen_close_waits_for_escape_release_and_restores_console_focus() {
    let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_WINDOW", "1")
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn avl-basic");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");

    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    let console = foreground_console();

    stdin
        .write_all(
            b"10 SCREEN\n\
20 FRAME\n\
30 PRINT \"WAITING-ESC\"\n\
40 IF KEYDOWN(27) THEN PRINT \"CLOSING-ESC\":SCREEN CLOSE:END\n\
50 GOTO 40\n\
RUN\n",
        )
        .unwrap();
    let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
    wait_for_line(&rx, "WAITING-ESC", Duration::from_secs(5));
    let mut held_escape = focus_and_hold_key(hwnd, SCAN_ESC);
    wait_for_line(&rx, "CLOSING-ESC", Duration::from_secs(5));
    assert_no_line_containing(&rx, "Ready", Duration::from_millis(200));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        hwnd,
        "focus changed before Esc was released"
    );
    held_escape.release();
    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        console,
        "SCREEN CLOSE did not restore console focus"
    );
    assert_no_graphics_window(child.child.id(), Duration::from_millis(200), hwnd);

    // Reopening must not inherit the Esc state that closed the previous window.
    stdin
        .write_all(b"NEW\n10 SCREEN\n20 FRAME\n30 PRINT \"ESC-STATE=\";KEYDOWN(27)\n40 SCREEN CLOSE:END\nRUN\n")
        .unwrap();
    wait_for_line(&rx, "ESC-STATE= 0", Duration::from_secs(5));
    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        console,
        "second SCREEN CLOSE did not restore console focus"
    );

    let _ = stdin.write_all(b"QUIT\n");
    let _ = child.child.wait();
    child.finished = true;
}

#[test]
#[ignore = "opens a real graphics window and verifies graphical PAUSE input"]
fn graphics_pause_ignores_click_and_exits_for_consumed_key() {
    let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_WINDOW", "1")
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn avl-basic");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");

    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    stdin
        .write_all(
            b"10 SCREEN\n\
20 GINPUT \"VALUE: \",A$\n\
30 PRINT \"PAUSE-START\"\n\
40 PAUSE\n\
50 PRINT \"PAUSE-END\"\n\
60 IF INKEY$<>\"\" THEN PRINT \"LEAK\" : GOTO 100\n\
70 PRINT \"CONSUMED\"\n\
100 END\n\
RUN\n",
        )
        .unwrap();

    let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
    thread::sleep(Duration::from_millis(100));
    focus_graphics_window(hwnd);
    tap_key(SCAN_A);
    tap_key(SCAN_ENTER);
    wait_for_line(&rx, "PAUSE-START", Duration::from_secs(5));
    assert_no_line_containing(&rx, "PAUSE-END", Duration::from_millis(350));

    click_window(hwnd, 20, 20);
    assert_no_line_containing(&rx, "PAUSE-END", Duration::from_millis(350));

    let mut held_n = focus_and_hold_key(hwnd, SCAN_N);
    wait_for_line(&rx, "PAUSE-END", Duration::from_secs(5));
    wait_for_line(&rx, "CONSUMED", Duration::from_secs(5));
    held_n.release();
    wait_for_line(&rx, "Ready", Duration::from_secs(5));

    let _ = stdin.write_all(b"SCREEN CLOSE\nQUIT\n");
    let _ = child.child.wait();
    child.finished = true;
}

#[test]
#[ignore = "opens a real graphics window and holds N across the RUN boundary"]
fn graphics_end_waits_for_key_release_and_does_not_poison_next_run() {
    let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_WINDOW", "1")
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn avl-basic");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");

    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    stdin
        .write_all(
            b"10 SCREEN\n\
20 FRAME\n\
30 PRINT \"WAITING-N\"\n\
40 R$=UPPER$(INKEY$)\n\
50 IF R$=\"\" THEN 40\n\
60 PRINT \"GOT=\";R$\n\
70 END\n\
RUN\n",
        )
        .unwrap();

    let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
    wait_for_line(&rx, "WAITING-N", Duration::from_secs(5));

    let mut held_n = focus_and_hold_key(hwnd, SCAN_N);
    wait_for_line(&rx, "GOT=N", Duration::from_secs(5));
    assert_no_line_containing(&rx, "Ready", Duration::from_millis(200));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        hwnd,
        "console received focus before N was released"
    );
    held_n.release();
    wait_for_line(&rx, "Ready", Duration::from_secs(5));

    stdin.write_all(b"RUN\n").unwrap();
    wait_for_line(&rx, "WAITING-N", Duration::from_secs(5));

    let mut held_n = focus_and_hold_key(hwnd, SCAN_N);
    wait_for_line(&rx, "GOT=N", Duration::from_secs(5));
    assert_no_line_containing(&rx, "Ready", Duration::from_millis(200));
    held_n.release();
    wait_for_line(&rx, "Ready", Duration::from_secs(5));

    let _ = stdin.write_all(b"SCREEN CLOSE\nQUIT\n");
    let _ = child.child.wait();
    child.finished = true;
}

#[test]
#[ignore = "opens a real graphics window and sends Ctrl-C with Win32 SendInput"]
fn repeated_graphics_ctrl_c_does_not_poison_next_run() {
    let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_WINDOW", "1")
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn avl-basic");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");

    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    let console = foreground_console();
    stdin
        .write_all(b"10 SCREEN\n20 FRAME\n30 GOTO 20\nRUN\n")
        .unwrap();

    let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
    assert_no_line_containing(
        &rx,
        "Execution interrupted by user.",
        Duration::from_millis(300),
    );
    send_ctrl_c_to_window(hwnd);
    wait_for_line(
        &rx,
        "Execution interrupted by user.",
        Duration::from_secs(5),
    );
    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        console,
        "Ctrl+C did not restore console focus"
    );

    stdin.write_all(b"RUN\n").unwrap();
    assert_no_line_containing(
        &rx,
        "Execution interrupted by user.",
        Duration::from_millis(500),
    );
    send_ctrl_c_to_window(hwnd);
    wait_for_line(
        &rx,
        "Execution interrupted by user.",
        Duration::from_secs(5),
    );
    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    assert_eq!(
        unsafe { GetForegroundWindow() },
        console,
        "second Ctrl+C did not restore console focus"
    );

    let _ = stdin.write_all(b"SCREEN CLOSE\nQUIT\n");
    let _ = child.child.wait();
    child.finished = true;
}

#[test]
#[ignore = "opens a real graphics window and exercises the raytracer Ctrl-C path"]
fn raytracer_ctrl_c_does_not_poison_subsequent_runs() {
    let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_WINDOW", "1")
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn avl-basic");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");

    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    stdin
        .write_all(b"CD \"samples\"\nRUN \"g-raytracer.bas\"\n")
        .unwrap();

    let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
    assert_no_line_containing(
        &rx,
        "Execution interrupted by user.",
        Duration::from_millis(500),
    );
    send_ctrl_c_to_window(hwnd);
    wait_for_line(
        &rx,
        "Execution interrupted by user.",
        Duration::from_secs(5),
    );
    wait_for_line(&rx, "Ready", Duration::from_secs(5));

    stdin.write_all(b"RUN\n").unwrap();
    assert_no_line_containing(
        &rx,
        "Execution interrupted by user.",
        Duration::from_secs(2),
    );
    send_ctrl_c_to_window(hwnd);
    wait_for_line(
        &rx,
        "Execution interrupted by user.",
        Duration::from_secs(5),
    );
    wait_for_line(&rx, "Ready", Duration::from_secs(5));

    let _ = stdin.write_all(b"SCREEN CLOSE\nQUIT\n");
    let _ = child.child.wait();
    child.finished = true;
}

#[test]
#[ignore = "opens g-balls, closes its graphics window, and checks for reopen regressions"]
fn g_balls_window_close_does_not_reopen_blank_window() {
    for delay_ms in [100_u64, 250, 500, 900, 1400] {
        let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
            .env("AVL_BASIC_WINDOW", "1")
            .current_dir(project_root())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn avl-basic");
        let mut child = ChildGuard {
            child,
            finished: false,
        };
        let stdout = child.child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let mut stdin = child.child.stdin.take().expect("stdin");

        wait_for_line(&rx, "Ready", Duration::from_secs(5));
        stdin
            .write_all(b"CD \"samples\"\nRUN \"g-balls.bas\"\n")
            .unwrap();

        let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
        thread::sleep(Duration::from_millis(delay_ms));
        close_window(hwnd);
        wait_for_line(
            &rx,
            "Execution interrupted by user.",
            Duration::from_secs(5),
        );
        wait_for_line(&rx, "Ready", Duration::from_secs(5));
        assert_no_graphics_window(child.child.id(), Duration::from_secs(2), hwnd);

        let _ = stdin.write_all(b"QUIT\n");
        let _ = child.child.wait();
        child.finished = true;
    }
}

#[test]
#[ignore = "opens a real graphics window, closes it, and checks that the implicit end frame does not reopen it"]
fn closing_dirty_no_frame_program_does_not_reopen_window() {
    let child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .env("AVL_BASIC_WINDOW", "1")
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn avl-basic");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");

    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    stdin
        .write_all(
            b"10 CLG : GPRINT \"escalera\"\n\
20 MOVE 0,400 : FOR n=1 TO 8\n\
30 DRAWR 50,0\n\
40 DRAWR 0,-50\n\
50 NEXT : MOVE 348,0 : FILL 3\n\
60 GOTO 60\n\
RUN\n",
        )
        .unwrap();

    let hwnd = wait_for_graphics_window(child.child.id(), Duration::from_secs(5));
    close_window(hwnd);
    wait_for_line(
        &rx,
        "Execution interrupted by user.",
        Duration::from_secs(5),
    );
    wait_for_line(&rx, "Ready", Duration::from_secs(5));
    assert_no_graphics_window(child.child.id(), Duration::from_secs(2), hwnd);

    let _ = stdin.write_all(b"QUIT\n");
    let _ = child.child.wait();
    child.finished = true;
}

#[test]
#[ignore = "opens its own console and graphics window and tests fullscreen console transitions"]
fn fullscreen_console_transitions_on_same_monitor() {
    fullscreen_console_transitions(false);
}

#[test]
#[ignore = "requires two real monitors and opens its own console and graphics window"]
fn fullscreen_console_transitions_on_separate_monitors() {
    fullscreen_console_transitions(true);
}

#[test]
#[ignore = "opens its own console and exercises native fullscreen while stepping the debugger"]
fn fullscreen_debugger_steps_on_same_monitor() {
    fullscreen_debugger_steps(false, false);
}

#[test]
#[ignore = "requires two real monitors and exercises native fullscreen while stepping the debugger"]
fn fullscreen_debugger_steps_on_separate_monitors() {
    fullscreen_debugger_steps(true, false);
}

#[test]
#[ignore = "requires two real monitors and moves graphics before MODE to verify placement"]
fn fullscreen_debugger_preserves_secondary_monitor_position() {
    fullscreen_debugger_steps(true, true);
}

// Run the real debugger execution hooks in a child with a separate native
// console. A synchronous callback replaces only the terminal UI, allowing each
// actual debugger pause to be inspected deterministically by the parent.
#[test]
#[ignore = "helper process for fullscreen_debugger_steps; requires its parent harness"]
fn fullscreen_debugger_child() {
    assert_eq!(
        std::env::var("AVL_BASIC_DEBUGGER_GUI_CHILD").as_deref(),
        Ok("1")
    );
    // The test executable does not carry the interpreter's DPI-aware manifest.
    // Use physical monitor coordinates in the child as well as in the parent.
    assert!(!unsafe { SetThreadDpiAwarenessContext(-4isize as Hwnd) }.is_null());
    use avl_basic::{DebugAction, Debugger, Interpreter};
    let mut interpreter = Interpreter::new();
    let position_case = std::env::var("AVL_BASIC_DEBUGGER_POSITION_CHILD").as_deref() == Ok("1");
    interpreter
        .process_immediate(if position_case { "SCREEN" } else { "MODE 960" })
        .unwrap();
    interpreter
        .program
        .load_text(if position_case {
            "10 MODE 960\n20 SCREEN FULL ON\n30 GPRINT \"SECOND MONITOR\"\n\
40 SCREEN FULL OFF\n50 SCREEN FULL ON\n60 MODE 1280\n70 SCREEN FULL OFF\n80 END"
        } else {
            "10 SCREEN FULL ON\n20 GPRINT \"DEBUG STEP\"\n30 SCREEN FULL OFF\n\
40 SCREEN FULL ON\n50 MODE 1280\n60 SCREEN FULL OFF\n70 END"
        })
        .unwrap();
    let mut debugger = Debugger::interactive(|snapshot, _, idle| {
        println!("DEBUG-PAUSE={}", snapshot.location.line);
        std::io::stdout().flush()?;
        // Keep native repaint/event processing alive while the parent examines
        // fullscreen geometry and focus, just like the interactive debugger.
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut command = String::new();
            let result = std::io::stdin().read_line(&mut command).map(|_| command);
            let _ = tx.send(result);
        });
        loop {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(command) => {
                    assert_eq!(command?.trim(), "STEP");
                    return Ok(DebugAction::StepInto);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => idle()?,
                Err(error) => return Err(std::io::Error::other(error)),
            }
        }
    });
    debugger.request_pause();
    interpreter.set_debugger(debugger);
    interpreter.run_loaded().unwrap();
    interpreter.process_immediate("SCREEN CLOSE").unwrap();
}

fn fullscreen_debugger_steps(separate: bool, position_case: bool) {
    struct DpiGuard(Hwnd);
    impl Drop for DpiGuard {
        fn drop(&mut self) {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
    let old_dpi = unsafe { SetThreadDpiAwarenessContext(-4isize as Hwnd) };
    assert!(!old_dpi.is_null());
    let _dpi = DpiGuard(old_dpi);
    let monitors = desktop_monitors();
    assert!(!monitors.is_empty());
    assert!(
        !separate || monitors.len() >= 2,
        "two real monitors required"
    );
    let (graphics_monitor, console_monitor) = if position_case {
        let primary = monitors
            .iter()
            .position(|monitor| monitor.left == 0 && monitor.top == 0)
            .expect("the primary monitor must include desktop origin");
        let secondary = (0..monitors.len()).find(|index| *index != primary).unwrap();
        (monitors[secondary], monitors[primary])
    } else {
        (monitors[0], monitors[usize::from(separate)])
    };
    let previous_windows = visible_window_handles();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fullscreen_debugger_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AVL_BASIC_WINDOW", "1")
        .env("AVL_BASIC_DEBUGGER_GUI_CHILD", "1")
        .env(
            "AVL_BASIC_DEBUGGER_POSITION_CHILD",
            if position_case { "1" } else { "0" },
        )
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn native debugger helper");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().unwrap();
    wait_for_line(&rx, "DEBUG-PAUSE=10", Duration::from_secs(10));
    let console = foreground_console();
    assert!(
        !previous_windows.contains(&console),
        "refusing to move an existing console"
    );
    place_test_window(console, console_monitor, 60, 700, 400);
    let mut graphics = wait_for_graphics_window(child.child.id(), Duration::from_secs(10));
    let original = desktop_window_rect(graphics);
    place_test_window(
        graphics,
        graphics_monitor,
        20,
        original.right - original.left,
        original.bottom - original.top,
    );
    let placed = desktop_window_rect(graphics);
    wait_for_console_focus(console);
    let phases: &[(i32, bool, &str)] = if position_case {
        &[
            (20, false, "debugger changed MODE after moving SCREEN"),
            (
                30,
                true,
                "debugger entered fullscreen on the second monitor",
            ),
            (40, true, "debugger drew on the second monitor"),
            (50, false, "debugger restored the previous normal position"),
            (60, true, "debugger entered fullscreen again"),
            (70, true, "debugger changed MODE while fullscreen"),
            (
                80,
                false,
                "debugger restored MODE 1280 on the second monitor",
            ),
        ]
    } else {
        &[
            (20, separate, "debugger stepped SCREEN FULL ON"),
            (30, separate, "debugger stepped drawing"),
            (40, false, "debugger stepped SCREEN FULL OFF"),
            (50, separate, "debugger stepped SCREEN FULL ON again"),
            (60, separate, "debugger recreated window with MODE"),
            (70, false, "debugger stepped SCREEN FULL OFF after MODE"),
        ]
    };
    for &(next_line, expected, phase) in phases {
        stdin.write_all(b"STEP\n").unwrap();
        wait_for_line(
            &rx,
            &format!("DEBUG-PAUSE={next_line}"),
            Duration::from_secs(10),
        );
        graphics = wait_for_graphics_window(child.child.id(), Duration::from_secs(10));
        assert_window_presentation(graphics, graphics_monitor, expected, phase);
        if position_case {
            let rect = desktop_window_rect(graphics);
            let center_x = (rect.left + rect.right) / 2;
            let center_y = (rect.top + rect.bottom) / 2;
            assert!(
                center_x >= graphics_monitor.left
                    && center_x < graphics_monitor.right
                    && center_y >= graphics_monitor.top
                    && center_y < graphics_monitor.bottom,
                "graphics changed monitor at {phase}: {rect:?}"
            );
            if !expected && matches!(next_line, 20 | 50) {
                assert!(
                    (rect.left - placed.left).abs() <= 2 && (rect.top - placed.top).abs() <= 2,
                    "normal position changed at {phase}: before={placed:?}, after={rect:?}"
                );
            }
        }
        wait_for_console_focus(console);
    }
    stdin.write_all(b"STEP\n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "debugger child did not exit");
        thread::sleep(Duration::from_millis(20));
    }
    child.finished = true;
}

fn fullscreen_console_transitions(separate: bool) {
    // Match the interpreter's physical desktop coordinates even with mixed DPI.
    struct DpiGuard(Hwnd);
    impl Drop for DpiGuard {
        fn drop(&mut self) {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
    let old_dpi = unsafe { SetThreadDpiAwarenessContext(-4isize as Hwnd) };
    assert!(
        !old_dpi.is_null(),
        "could not enable physical desktop coordinates"
    );
    let _dpi = DpiGuard(old_dpi);
    let monitors = desktop_monitors();
    assert!(!monitors.is_empty(), "no desktop monitor available");
    assert!(
        !separate || monitors.len() >= 2,
        "two-monitor verification requires two real monitors; this is not a passing skip"
    );
    let graphics_monitor = monitors[0];
    let console_monitor = monitors[usize::from(separate)];
    // The same compiled test can verify an installed PGO executable without a
    // release-profile test build replacing that executable with a non-PGO one.
    let binary = std::env::var_os("AVL_BASIC_TEST_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_avl-basic").into());
    assert!(
        binary.is_absolute(),
        "test binary must have an absolute local path"
    );
    assert!(
        binary.is_file(),
        "test binary does not exist: {}",
        binary.display()
    );
    let binary = binary.canonicalize().expect("resolve test binary path");
    assert!(
        matches!(binary.components().next(), Some(std::path::Component::Prefix(prefix))
            if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))),
        "test binary must be on a local drive: {}",
        binary.display()
    );
    let previous_windows = visible_window_handles();
    let child = Command::new(binary)
        .env("AVL_BASIC_WINDOW", "1")
        .creation_flags(CREATE_NEW_CONSOLE)
        .current_dir(project_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn avl-basic with a separate console");
    let mut child = ChildGuard {
        child,
        finished: false,
    };
    let stdout = child.child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let mut stdin = child.child.stdin.take().expect("stdin");
    wait_for_line(&rx, "Ready", Duration::from_secs(10));
    let console = foreground_console();
    assert!(
        !previous_windows.contains(&console),
        "the new process did not expose its own console; refusing to move an existing window"
    );
    place_test_window(console, console_monitor, 60, 700, 400);
    stdin
        .write_all(
            b"10 MODE 960:SCREEN:FRAME\n\
20 PRINT \"MONITOR-WINDOW-READY\"\n\
30 IF INKEY$=\"\" THEN GOTO 30\n\
40 SCREEN FULL ON:FRAME\n\
50 PRINT \"MONITOR-FULL-BEFORE-INPUT\"\n\
60 IF INKEY$=\"\" THEN GOTO 60\n\
70 INPUT \"VALUE\";A$\n\
80 PRINT \"MONITOR-INPUT-DONE\"\n\
90 SCREEN FULL ON:FRAME\n\
100 PRINT \"MONITOR-FULL-BEFORE-STOP\"\n\
110 IF INKEY$=\"\" THEN GOTO 110\n\
120 STOP\n\
130 FRAME\n\
140 PRINT \"MONITOR-FULL-BEFORE-END\"\n\
150 IF INKEY$=\"\" THEN GOTO 150\n\
160 END\n\
RUN\n",
        )
        .unwrap();
    let graphics = wait_for_graphics_window(child.child.id(), Duration::from_secs(10));
    wait_for_line(&rx, "MONITOR-WINDOW-READY", Duration::from_secs(10));
    // Preserve the fixed client size; only its position changes.
    let original = desktop_window_rect(graphics);
    place_test_window(
        graphics,
        graphics_monitor,
        20,
        original.right - original.left,
        original.bottom - original.top,
    );
    focus_graphics_window(graphics);
    tap_key(SCAN_N);
    wait_for_line(&rx, "MONITOR-FULL-BEFORE-INPUT", Duration::from_secs(10));
    assert_window_presentation(graphics, graphics_monitor, true, "before INPUT");
    focus_graphics_window(graphics);
    tap_key(SCAN_N);
    wait_for_console_focus(console);
    assert_window_presentation(graphics, graphics_monitor, separate, "during INPUT");
    stdin.write_all(b"monitor-answer\n").unwrap();
    wait_for_line(&rx, "MONITOR-INPUT-DONE", Duration::from_secs(10));
    wait_for_line(&rx, "MONITOR-FULL-BEFORE-STOP", Duration::from_secs(10));
    assert_window_presentation(graphics, graphics_monitor, true, "before STOP");
    focus_graphics_window(graphics);
    tap_key(SCAN_N);
    wait_for_line(&rx, "Ready", Duration::from_secs(10));
    wait_for_console_focus(console);
    assert_window_presentation(graphics, graphics_monitor, separate, "at STOP");
    stdin.write_all(b"CONT\n").unwrap();
    wait_for_line(&rx, "MONITOR-FULL-BEFORE-END", Duration::from_secs(10));
    assert_window_presentation(graphics, graphics_monitor, true, "after CONT and FRAME");
    focus_graphics_window(graphics);
    tap_key(SCAN_N);
    wait_for_line(&rx, "Ready", Duration::from_secs(10));
    wait_for_console_focus(console);
    assert_window_presentation(graphics, graphics_monitor, separate, "after END");
    stdin.write_all(b"SCREEN CLOSE\n").unwrap();
    wait_for_line(&rx, "Ready", Duration::from_secs(10));
    assert_no_graphics_window(child.child.id(), Duration::from_millis(300), graphics);
    wait_for_console_focus(console);
    stdin.write_all(b"QUIT\n").unwrap();
    assert!(child.child.wait().unwrap().success());
    child.finished = true;
}

fn desktop_monitors() -> Vec<DesktopRect> {
    extern "system" fn collect(_: Hwnd, _: Hwnd, rect: *mut DesktopRect, data: Lparam) -> Bool {
        unsafe { (&mut *(data as *mut Vec<DesktopRect>)).push(*rect) };
        1
    }
    let mut monitors = Vec::new();
    assert_ne!(
        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                collect,
                &mut monitors as *mut Vec<DesktopRect> as Lparam,
            )
        },
        0
    );
    monitors
}

fn visible_window_handles() -> Vec<Hwnd> {
    extern "system" fn collect(hwnd: Hwnd, data: Lparam) -> Bool {
        if unsafe { IsWindowVisible(hwnd) } != 0 {
            unsafe { (&mut *(data as *mut Vec<Hwnd>)).push(hwnd) };
        }
        1
    }
    let mut windows = Vec::new();
    unsafe { EnumWindows(collect, &mut windows as *mut Vec<Hwnd> as Lparam) };
    windows
}

fn desktop_window_rect(hwnd: Hwnd) -> DesktopRect {
    let mut rect = DesktopRect::default();
    assert_ne!(
        unsafe { GetWindowRect(hwnd, &mut rect) },
        0,
        "GetWindowRect failed"
    );
    rect
}

fn place_test_window(hwnd: Hwnd, monitor: DesktopRect, offset: i32, width: i32, height: i32) {
    assert!(
        monitor.right - monitor.left > width + offset,
        "test window {width}x{height} at offset {offset} does not fit {monitor:?}"
    );
    assert!(
        monitor.bottom - monitor.top > height + offset,
        "test window {width}x{height} at offset {offset} does not fit {monitor:?}"
    );
    const SWP_NOZORDER: u32 = 4;
    const SWP_NOACTIVATE: u32 = 16;
    unsafe { ShowWindow(hwnd, 9) }; // Restore only the console created by this test.
    assert_ne!(
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                monitor.left + offset,
                monitor.top + offset,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        },
        0
    );
}

fn wait_for_console_focus(console: Hwnd) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while unsafe { GetForegroundWindow() } != console && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        unsafe { GetForegroundWindow() },
        console,
        "terminal did not regain focus"
    );
}

fn assert_window_presentation(hwnd: Hwnd, monitor: DesktopRect, fullscreen: bool, phase: &str) {
    const GWL_STYLE: i32 = -16;
    const WS_CAPTION: isize = 0x00c0_0000;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
        let rect = desktop_window_rect(hwnd);
        let correct = if fullscreen {
            style & WS_CAPTION == 0 && rect == monitor
        } else {
            style & WS_CAPTION != 0 && rect != monitor
        };
        if correct {
            return;
        }
        assert!(Instant::now() < deadline,
            "wrong presentation {phase}: fullscreen={fullscreen}, style={style:#x}, rect={rect:?}, monitor={monitor:?}");
        thread::sleep(Duration::from_millis(20));
    }
}

struct ChildGuard {
    child: Child,
    finished: bool,
}

struct HeldKeyGuard {
    scan_code: u16,
    released: bool,
}

impl HeldKeyGuard {
    fn release(&mut self) {
        if self.released {
            return;
        }
        send_inputs(&[scan_code_input(self.scan_code, KEYEVENTF_KEYUP)]);
        self.released = true;
    }
}

impl Drop for HeldKeyGuard {
    fn drop(&mut self) {
        if !self.released {
            let input = scan_code_input(self.scan_code, KEYEVENTF_KEYUP);
            unsafe {
                let _ = SendInput(1, &input, size_of::<Input>() as i32);
            }
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn project_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn foreground_console() -> Hwnd {
    let console = unsafe { GetForegroundWindow() };
    let mut class_name = [0u16; 64];
    let class_len =
        unsafe { GetClassNameW(console, class_name.as_mut_ptr(), class_name.len() as i32) };
    let class_name = String::from_utf16_lossy(&class_name[..class_len.max(0) as usize]);
    assert!(
        matches!(
            class_name.as_str(),
            "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS"
        ),
        "new console did not become foreground: class={class_name:?}"
    );
    console
}

fn wait_for_line(rx: &Receiver<String>, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        if let Ok(line) = rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            if line.contains(needle) {
                return;
            }
            seen.push(line);
        }
    }
    panic!("timed out waiting for line containing {needle:?}; saw {seen:?}");
}

fn assert_no_line_containing(rx: &Receiver<String>, needle: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(line) if line.contains(needle) => {
                panic!("unexpected line containing {needle:?}: {line}");
            }
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

fn wait_for_graphics_window(pid: u32, timeout: Duration) -> Hwnd {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(hwnd) = find_graphics_window(pid) {
            return hwnd;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for AVL BASIC Graphics window");
}

fn assert_no_graphics_window(pid: u32, timeout: Duration, original: Hwnd) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(hwnd) = find_graphics_window(pid) {
            panic!(
                "unexpected graphics window after close: {hwnd:p}; same_as_original={}",
                hwnd == original
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn find_graphics_window(pid: u32) -> Option<Hwnd> {
    struct FindData {
        pid: u32,
        hwnd: Hwnd,
    }

    extern "system" fn enum_proc(hwnd: Hwnd, lparam: Lparam) -> Bool {
        let data = unsafe { &mut *(lparam as *mut FindData) };
        let mut window_pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut window_pid);
        }
        if window_pid != data.pid || unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }
        if window_title(hwnd) == "AVL BASIC Graphics" {
            data.hwnd = hwnd;
            return 0;
        }
        1
    }

    let mut data = FindData {
        pid,
        hwnd: std::ptr::null_mut(),
    };
    unsafe {
        EnumWindows(enum_proc, &mut data as *mut FindData as Lparam);
    }
    (!data.hwnd.is_null()).then_some(data.hwnd)
}

fn window_title(hwnd: Hwnd) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; len as usize + 1];
        let copied = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer[..copied as usize])
    }
}

fn send_ctrl_c_to_window(hwnd: Hwnd) {
    focus_graphics_window(hwnd);

    const VK_CONTROL: u16 = 0x11;
    const VK_C: u16 = 0x43;

    send_inputs(&[
        key_input(INPUT_KEYBOARD, VK_CONTROL, 0),
        key_input(INPUT_KEYBOARD, VK_C, 0),
    ]);
    thread::sleep(Duration::from_millis(350));
    send_inputs(&[
        key_input(INPUT_KEYBOARD, VK_C, KEYEVENTF_KEYUP),
        key_input(INPUT_KEYBOARD, VK_CONTROL, KEYEVENTF_KEYUP),
    ]);
    thread::sleep(Duration::from_millis(100));
}

fn focus_and_hold_key(hwnd: Hwnd, scan_code: u16) -> HeldKeyGuard {
    let deadline = Instant::now() + Duration::from_secs(2);
    while unsafe { GetForegroundWindow() } != hwnd && Instant::now() < deadline {
        focus_graphics_window_once(hwnd);
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        unsafe { GetForegroundWindow() },
        hwnd,
        "graphics window did not become foreground; refusing to hold a key"
    );

    send_inputs(&[scan_code_input(scan_code, 0)]);
    HeldKeyGuard {
        scan_code,
        released: false,
    }
}

fn tap_key(scan_code: u16) {
    send_inputs(&[scan_code_input(scan_code, 0)]);
    thread::sleep(Duration::from_millis(50));
    send_inputs(&[scan_code_input(scan_code, KEYEVENTF_KEYUP)]);
    thread::sleep(Duration::from_millis(50));
}

fn focus_graphics_window(hwnd: Hwnd) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while unsafe { GetForegroundWindow() } != hwnd && Instant::now() < deadline {
        focus_graphics_window_once(hwnd);
        thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        unsafe { GetForegroundWindow() },
        hwnd,
        "graphics window did not become foreground; refusing to send input"
    );
}

fn focus_graphics_window_once(hwnd: Hwnd) {
    unsafe {
        let current_thread = GetCurrentThreadId();
        let target_thread = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let foreground = GetForegroundWindow();
        let foreground_thread = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, std::ptr::null_mut())
        };

        let attached_target = target_thread != 0
            && target_thread != current_thread
            && AttachThreadInput(current_thread, target_thread, 1) != 0;
        let attached_foreground = foreground_thread != 0
            && foreground_thread != current_thread
            && foreground_thread != target_thread
            && AttachThreadInput(current_thread, foreground_thread, 1) != 0;

        const SW_SHOW: i32 = 5;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetActiveWindow(hwnd);
        let _ = SetFocus(hwnd);

        if attached_foreground {
            let _ = AttachThreadInput(current_thread, foreground_thread, 0);
        }
        if attached_target {
            let _ = AttachThreadInput(current_thread, target_thread, 0);
        }
    }
}

fn close_window(hwnd: Hwnd) {
    const WM_CLOSE: u32 = 0x0010;
    let ok = unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) };
    assert_ne!(ok, 0, "PostMessageW(WM_CLOSE) failed");
}

fn click_window(hwnd: Hwnd, x: u16, y: u16) {
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const MK_LBUTTON: usize = 0x0001;
    let position = ((y as u32) << 16 | x as u32) as isize;

    let down = unsafe { PostMessageW(hwnd, WM_LBUTTONDOWN, MK_LBUTTON, position) };
    let up = unsafe { PostMessageW(hwnd, WM_LBUTTONUP, 0, position) };
    assert_ne!(down, 0, "PostMessageW(WM_LBUTTONDOWN) failed");
    assert_ne!(up, 0, "PostMessageW(WM_LBUTTONUP) failed");
}

fn send_inputs(inputs: &[Input]) {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<Input>() as i32,
        )
    };
    assert_eq!(sent, inputs.len() as u32, "SendInput failed");
}

fn key_input(kind: u32, virtual_key: u16, flags: u32) -> Input {
    Input {
        kind,
        data: InputData {
            keyboard: KeyboardInput {
                virtual_key,
                scan_code: 0,
                flags,
                time: 0,
                extra_info: 0,
            },
        },
    }
}

fn scan_code_input(scan_code: u16, flags: u32) -> Input {
    Input {
        kind: INPUT_KEYBOARD,
        data: InputData {
            keyboard: KeyboardInput {
                virtual_key: 0,
                scan_code,
                flags: flags | KEYEVENTF_SCANCODE,
                time: 0,
                extra_info: 0,
            },
        },
    }
}
