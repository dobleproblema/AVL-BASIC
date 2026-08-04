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

const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_SCANCODE: u32 = 0x0008;
const CREATE_NEW_CONSOLE: u32 = 0x00000010;
const SCAN_A: u16 = 0x1E;
const SCAN_ENTER: u16 = 0x1C;
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
    fn GetForegroundWindow() -> Hwnd;
    fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, max_count: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut Dword) -> Dword;
    fn IsWindowVisible(hwnd: Hwnd) -> Bool;
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
