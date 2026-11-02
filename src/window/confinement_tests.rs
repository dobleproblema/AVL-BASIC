//! Explicit Windows integration checks, excluded from ordinary headless runs.
//! Run serially on an unlocked desktop; this test moves the shared pointer.
use super::GraphicsWindow;
use crate::graphics::Graphics;
use std::ffi::c_void;
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "user32")]
extern "system" {
    fn GetClipCursor(rect: *mut Rect) -> i32;
    fn ClipCursor(rect: *const Rect) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn SetThreadDpiAwarenessContext(context: *mut c_void) -> *mut c_void;
    fn LogicalToPhysicalPointForPerMonitorDPI(window: *mut c_void, point: *mut Point) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn GetForegroundWindow() -> *mut c_void;
    fn GetFocus() -> *mut c_void;
    fn ClientToScreen(window: *mut c_void, point: *mut Point) -> i32;
    fn GetCursorInfo(info: *mut CursorInfo) -> i32;
    fn SendInput(count: u32, inputs: *const MousePacket, size: i32) -> u32;
    fn GetAsyncKeyState(key: i32) -> i16;
}

#[repr(C)]
#[derive(Default)]
struct MouseInput {
    dx: i32,
    dy: i32,
    data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

// The mouse member is the largest member of Win32 INPUT's union.
#[repr(C)]
struct MousePacket {
    kind: u32,
    mouse: MouseInput,
}

fn relative_mouse_packet(dx: i32, dy: i32, flags: u32) {
    let packet = MousePacket {
        kind: 0, // INPUT_MOUSE
        mouse: MouseInput {
            dx,
            dy,
            flags,
            ..Default::default()
        },
    };
    assert_eq!(
        unsafe { SendInput(1, &packet, std::mem::size_of::<MousePacket>() as i32) },
        1
    );
}

struct HeldTestButtons;

impl Drop for HeldTestButtons {
    fn drop(&mut self) {
        // Release only the buttons this test pressed, before its HWND drops,
        // also when an assertion unwinds. Never leave shared input held down.
        let packet = MousePacket {
            kind: 0,
            mouse: MouseInput {
                flags: 0x0004 | 0x0010,
                ..Default::default()
            },
        };
        unsafe { SendInput(1, &packet, std::mem::size_of::<MousePacket>() as i32) };
    }
}

#[repr(C)]
#[derive(Default)]
struct CursorInfo {
    size: u32,
    flags: u32,
    cursor: *mut c_void,
    position: Point,
}

fn cursor_visible() -> bool {
    let mut info = CursorInfo {
        size: std::mem::size_of::<CursorInfo>() as u32,
        ..Default::default()
    };
    assert_ne!(unsafe { GetCursorInfo(&mut info) }, 0);
    info.flags & 1 != 0
}

struct PhysicalCoordinates(*mut c_void);

impl PhysicalCoordinates {
    fn enter() -> Self {
        let previous = unsafe { SetThreadDpiAwarenessContext(-3isize as *mut c_void) };
        assert!(
            !previous.is_null(),
            "native test requires scoped physical coordinates"
        );
        Self(previous)
    }
}

impl Drop for PhysicalCoordinates {
    fn drop(&mut self) {
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

fn move_physical(x: i32, y: i32) -> i32 {
    let _coordinates = PhysicalCoordinates::enter();
    unsafe { SetCursorPos(x, y) }
}

fn physical_position() -> Point {
    let _coordinates = PhysicalCoordinates::enter();
    let mut point = Point::default();
    assert_ne!(unsafe { GetCursorPos(&mut point) }, 0);
    point
}

fn current_clip() -> Rect {
    let _coordinates = PhysicalCoordinates::enter();
    let mut rect = Rect::default();
    assert_ne!(
        unsafe { GetClipCursor(&mut rect) },
        0,
        "GetClipCursor failed"
    );
    rect
}

fn desktop() -> Rect {
    let _coordinates = PhysicalCoordinates::enter();
    unsafe {
        let left = GetSystemMetrics(76);
        let top = GetSystemMetrics(77);
        Rect {
            left,
            top,
            right: left + GetSystemMetrics(78),
            bottom: top + GetSystemMetrics(79),
        }
    }
}

struct RestoreCursor {
    clip: Rect,
    position: Point,
}

impl RestoreCursor {
    fn save() -> Self {
        let clip = current_clip();
        assert_eq!(
            clip,
            desktop(),
            "another application already confines the pointer; do not run this test"
        );
        let position = physical_position();
        Self { clip, position }
    }
}

impl Drop for RestoreCursor {
    fn drop(&mut self) {
        // Also runs on assertion unwinding, after the later-created windows.
        unsafe {
            let coordinates = PhysicalCoordinates::enter();
            ClipCursor(&self.clip);
            drop(coordinates);
            move_physical(self.position.x, self.position.y);
        }
    }
}

fn foreground(window: &GraphicsWindow) -> bool {
    let hwnd = window.window.get_window_handle();
    unsafe { GetForegroundWindow() == hwnd && GetFocus() == hwnd }
}

fn activate(window: &mut GraphicsWindow) {
    window.focus();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        window.pump_events();
        if foreground(window) && window.is_active() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Windows denied foreground activation: run this native test on an unlocked desktop"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn expected_viewport(window: &GraphicsWindow) -> Rect {
    let viewport = window
        .presentation_viewport()
        .expect("nonempty native client");
    let hwnd = window.window.get_window_handle();
    let mut first = Point {
        x: viewport.x as i32,
        y: viewport.y as i32,
    };
    let mut last = Point {
        x: (viewport.x + viewport.width - 1) as i32,
        y: (viewport.y + viewport.height - 1) as i32,
    };
    for point in [&mut first, &mut last] {
        assert_ne!(unsafe { ClientToScreen(hwnd, point) }, 0);
        assert_ne!(
            unsafe { LogicalToPhysicalPointForPerMonitorDPI(hwnd, point) },
            0
        );
    }
    Rect {
        left: first.x,
        top: first.y,
        right: last.x + 1,
        bottom: last.y + 1,
    }
}

fn assert_canvas_corners(window: &mut GraphicsWindow) {
    let rect = expected_viewport(window);
    let (last_x, last_y) = (window.width as i32 - 1, window.height as i32 - 1);
    for (x, y, expected) in [
        (rect.left - 50, rect.top - 50, (0, last_y)),
        (rect.right + 50, rect.top - 50, (last_x, last_y)),
        (rect.left - 50, rect.bottom + 50, (0, 0)),
        (rect.right + 50, rect.bottom + 50, (last_x, 0)),
    ] {
        assert_ne!(move_physical(x, y), 0);
        let mouse = window.pump_events();
        assert_eq!(
            (mouse.x, mouse.y),
            expected,
            "actual BASIC mouse coordinates must reach the canvas extremes without -1"
        );
    }
}

#[test]
#[ignore = "native Windows fullscreen/pointer integration; run serially on an unlocked desktop"]
fn native_graphics_confinement_suspends_until_explicit_graphics_resume() {
    // Keep the same DPI-awareness context as the interpreter/minifb. The guard
    // is created first so its restoration runs after window cleanup on panic.
    let restore = RestoreCursor::save();
    let original_clip = restore.clip;
    let original_position = restore.position;
    let mut graphics = Graphics::new(640);
    let mut window = GraphicsWindow::new(&graphics).unwrap();
    window.set_title("AVL GRAPHICS POINTER INTEGRATION TEST");
    window.restore_position((50, 50)).unwrap();
    activate(&mut window);
    window.set_fullscreen(true).unwrap();
    window.set_mouse_cursor_visible(false).unwrap();
    activate(&mut window);
    assert_eq!((window.width, window.height), (640, 480));
    assert!(window.is_fullscreen());
    assert_eq!(
        current_clip(),
        expected_viewport(&window),
        "FULL + MOUSE OFF must capture the canvas"
    );
    assert_canvas_corners(&mut window);

    window.set_mouse_cursor_visible(true).unwrap();
    assert_eq!(
        current_clip(),
        original_clip,
        "MOUSE ON must release immediately"
    );
    window.set_mouse_cursor_visible(false).unwrap();
    assert_eq!(current_clip(), expected_viewport(&window));
    window.set_fullscreen(false).unwrap();
    assert_eq!(
        current_clip(),
        original_clip,
        "windowed MOUSE OFF must stay unconfined"
    );
    window.pump_events();
    assert_eq!(current_clip(), original_clip);
    window.set_fullscreen(true).unwrap();
    activate(&mut window);
    assert_eq!(current_clip(), expected_viewport(&window));

    // Deliberately keep graphics foreground and fullscreen. This rules out
    // focus loss as the explanation for release, as on two-monitor INPUT/DEBUG.
    window.suspend_pointer_confinement_for_console();
    assert!(window.is_fullscreen() && foreground(&window));
    assert_eq!(
        current_clip(),
        original_clip,
        "console suspension must release even while FULL remains active"
    );
    for _ in 0..3 {
        window.pump_events();
        assert!(window.pump_lifecycle(&graphics).unwrap());
        window.resume_fullscreen_for_console();
        assert!(window.is_fullscreen() && foreground(&window));
        assert_eq!(
            current_clip(),
            original_clip,
            "passive pumps/debugger presentation must not recapture"
        );
    }

    // Also cover the console path that temporarily leaves fullscreen, followed
    // by a debugger restoring the requested FULL presentation without capture.
    window.suspend_fullscreen_for_console();
    assert!(!window.is_fullscreen() && window.fullscreen_requested());
    assert_eq!(current_clip(), original_clip);
    window.resume_fullscreen_for_console();
    assert!(window.is_fullscreen());
    assert_eq!(current_clip(), original_clip);
    activate(&mut window); // activating alone must not remove suspension
    assert_eq!(current_clip(), original_clip);
    window.resume_pointer_confinement_for_graphics();
    assert_eq!(
        current_clip(),
        expected_viewport(&window),
        "explicit graphics resume must recapture"
    );

    // Exercise the GraphicsWindow lifecycle used by MODE recreation: release
    // the old HWND before constructing the replacement, then restore prefs.
    let position = window.position_for_recreation();
    let requested = window.fullscreen_requested();
    let visible = window.mouse_cursor_visible();
    drop(window);
    assert_eq!(
        current_clip(),
        original_clip,
        "old MODE window must release on Drop"
    );
    graphics.set_mode(1280).unwrap();
    let mut recreated = GraphicsWindow::new(&graphics).unwrap();
    recreated.set_title("AVL RECREATED POINTER INTEGRATION TEST");
    if let Some(position) = position {
        recreated.restore_position(position).unwrap();
    }
    recreated.set_mouse_cursor_visible(visible).unwrap();
    assert_eq!(
        current_clip(),
        original_clip,
        "replacement is initially windowed"
    );
    recreated.set_fullscreen(requested).unwrap();
    activate(&mut recreated);
    recreated.resume_pointer_confinement_for_graphics();
    assert_eq!((recreated.width, recreated.height), (1280, 720));
    assert_eq!(
        current_clip(),
        expected_viewport(&recreated),
        "MODE replacement must use its new viewport"
    );
    assert_canvas_corners(&mut recreated);
    drop(recreated);
    assert_eq!(current_clip(), original_clip);
    drop(restore);
    let restored_position = physical_position();
    assert_eq!(restored_position, original_position);
    assert_eq!(current_clip(), original_clip);
}

#[test]
#[ignore = "native fullscreen edge cursor regression; moves shared pointer and restores it"]
fn native_fullscreen_mouse_off_stays_hidden_between_event_pumps_at_all_edges() {
    let restore = RestoreCursor::save();
    for mode in [640, 960, 1280] {
        let graphics = Graphics::new(mode);
        let mut window = GraphicsWindow::new(&graphics).unwrap();
        window.set_title("AVL FULLSCREEN CURSOR EDGE TEST");
        activate(&mut window);
        window.set_fullscreen(true).unwrap();
        window.set_mouse_cursor_visible(false).unwrap();
        activate(&mut window);
        let rect = expected_viewport(&window);
        assert_eq!(current_clip(), rect);
        let middle_x = (rect.left + rect.right) / 2;
        let middle_y = (rect.top + rect.bottom) / 2;
        for (edge, x, y) in [
            ("top", middle_x, rect.top - 50),
            ("left", rect.left - 50, middle_y),
            ("bottom", middle_x, rect.bottom + 50),
            ("right", rect.right + 50, middle_y),
            ("top-left", rect.left - 50, rect.top - 50),
            ("top-right", rect.right + 50, rect.top - 50),
            ("bottom-left", rect.left - 50, rect.bottom + 50),
            ("bottom-right", rect.right + 50, rect.bottom + 50),
        ] {
            assert_ne!(move_physical(middle_x, middle_y), 0);
            let settle = Instant::now() + Duration::from_millis(300);
            loop {
                window.pump_events();
                if !cursor_visible() {
                    break;
                }
                assert!(
                    Instant::now() < settle,
                    "setup could not hide the test cursor"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(20));
            assert_ne!(move_physical(x, y), 0);
            // A game can spend this interval drawing or waiting for VSYNC.
            // Do not pump WM_SETCURSOR: that could hide a transient class arrow
            // and make the regression falsely pass after the flicker is over.
            let deadline = Instant::now() + Duration::from_millis(60);
            while Instant::now() < deadline {
                assert!(foreground(&window), "native test lost foreground ownership");
                assert!(
                    !cursor_visible(),
                    "MOUSE OFF flashed at mode={mode}, edge={edge}"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            let point = physical_position();
            assert_eq!(point.x, x.clamp(rect.left, rect.right - 1));
            assert_eq!(point.y, y.clamp(rect.top, rect.bottom - 1));
        }
        drop(window);
        assert_eq!(current_clip(), restore.clip);
    }
}

#[test]
#[ignore = "relative mouse input to own foreground HWND; restores pointer and test-held buttons"]
fn native_relative_mouse_keeps_cursor_hidden_coordinates_valid_and_buttons_held_at_edges() {
    let restore = RestoreCursor::save();
    assert_eq!(
        std::mem::size_of::<MousePacket>(),
        if cfg!(target_pointer_width = "64") {
            40
        } else {
            28
        }
    );
    for key in [0x01, 0x02] {
        // VK_LBUTTON / VK_RBUTTON
        assert_eq!(
            unsafe { GetAsyncKeyState(key) } as u16 & 0x8000,
            0,
            "release physical mouse buttons before this test"
        );
    }
    for mode in [640, 960, 1280] {
        let graphics = Graphics::new(mode);
        let mut window = GraphicsWindow::new(&graphics).unwrap();
        window.set_title("AVL RELATIVE MOUSE EDGE REGRESSION");
        activate(&mut window);
        window.set_fullscreen(true).unwrap();
        window.set_mouse_cursor_visible(false).unwrap();
        activate(&mut window);
        let rect = expected_viewport(&window);
        assert_eq!(current_clip(), rect);
        let middle_x = (rect.left + rect.right) / 2;
        let middle_y = (rect.top + rect.bottom) / 2;
        let last_x = graphics.width as i32 - 1;
        let last_y = graphics.height as i32 - 1;
        let buttons = HeldTestButtons;
        relative_mouse_packet(0, 0, 0x0002 | 0x0008); // LEFTDOWN | RIGHTDOWN
        window.pump_events();
        for (edge, x, y, dx, dy, expected_x, expected_y) in [
            ("top", middle_x, rect.top, 0, -10, None, Some(last_y)),
            ("left", rect.left, middle_y, -10, 0, Some(0), None),
            ("bottom", middle_x, rect.bottom - 1, 0, 10, None, Some(0)),
            ("right", rect.right - 1, middle_y, 10, 0, Some(last_x), None),
            (
                "top-left",
                rect.left,
                rect.top,
                -10,
                -10,
                Some(0),
                Some(last_y),
            ),
            (
                "top-right",
                rect.right - 1,
                rect.top,
                10,
                -10,
                Some(last_x),
                Some(last_y),
            ),
            (
                "bottom-left",
                rect.left,
                rect.bottom - 1,
                -10,
                10,
                Some(0),
                Some(0),
            ),
            (
                "bottom-right",
                rect.right - 1,
                rect.bottom - 1,
                10,
                10,
                Some(last_x),
                Some(0),
            ),
        ] {
            assert_ne!(move_physical(x, y), 0);
            window.pump_events();
            let settle = Instant::now() + Duration::from_millis(300);
            while cursor_visible() {
                window.pump_events();
                assert!(
                    Instant::now() < settle,
                    "test setup cursor did not become hidden"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            for step in 0..48 {
                assert!(
                    foreground(&window),
                    "do not send input to another application"
                );
                relative_mouse_packet(dx, dy, 1); // MOUSEEVENTF_MOVE, relative
                std::thread::sleep(Duration::from_millis(2));
                assert!(
                    !cursor_visible(),
                    "cursor reappeared before pump: mode={mode}, edge={edge}, step={step}"
                );
                let mouse = window.pump_events();
                assert!(
                    !cursor_visible(),
                    "cursor reappeared after pump: mode={mode}, edge={edge}, step={step}"
                );
                assert!(mouse.x >= 0 && mouse.y >= 0, "clipped physical pointer reported outside at mode={mode}, edge={edge}, step={step}: {mouse:?}");
                if let Some(x) = expected_x {
                    assert_eq!(mouse.x, x, "mode={mode}, edge={edge}, step={step}");
                }
                if let Some(y) = expected_y {
                    assert_eq!(mouse.y, y, "mode={mode}, edge={edge}, step={step}");
                }
                assert!(
                    mouse.left && mouse.right,
                    "held buttons lost at mode={mode}, edge={edge}: {mouse:?}"
                );
                assert!(!matches!(mouse.event.as_str(), "LEFTUP" | "RIGHTUP"));
                window
                    .window
                    .update_with_buffer(graphics.buffer(), graphics.width, graphics.height)
                    .unwrap();
            }
        }
        drop(buttons);
        let release_deadline = Instant::now() + Duration::from_millis(300);
        loop {
            let mouse = window.pump_events();
            if !mouse.left && !mouse.right {
                break;
            }
            assert!(
                Instant::now() < release_deadline,
                "test button-up packets were not delivered"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        // No movement or event pumping after ON: refresh must also handle a
        // stationary cursor whose transient virtual position is outside.
        window.set_mouse_cursor_visible(true).unwrap();
        let deadline = Instant::now() + Duration::from_millis(500);
        while !cursor_visible() {
            assert!(
                Instant::now() < deadline,
                "MOUSE ON failed at the stationary edge"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        window.set_mouse_cursor_visible(false).unwrap();
        let deadline = Instant::now() + Duration::from_millis(500);
        while cursor_visible() {
            assert!(
                Instant::now() < deadline,
                "MOUSE OFF failed at the stationary edge"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(window);
        assert_eq!(current_clip(), restore.clip);
    }
}
