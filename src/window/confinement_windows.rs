//! Optional HWND-local bookkeeping for the shared Win32 cursor clip.
//! Properties contain only integers, never pointers into Rust-owned objects.
use super::{GetClientRect, GetCursorPos, GetPropW, Point, Rect, RemovePropW, SetPropW};
use crate::window::PresentationViewport;
use std::ffi::c_void;
use std::ptr;
use std::sync::OnceLock;

const fn wide<const N: usize>(ascii: [u8; N]) -> [u16; N] {
    let mut result = [0; N];
    let mut i = 0;
    while i < N {
        result[i] = ascii[i] as u16;
        i += 1;
    }
    result
}

const WIDTH: &[u16] = &wide(*b"AVLBASIC.Clip.Width\0");
const HEIGHT: &[u16] = &wide(*b"AVLBASIC.Clip.Height\0");
const OWNED: &[u16] = &wide(*b"AVLBASIC.Clip.Owned\0");
const LEFT: &[u16] = &wide(*b"AVLBASIC.Clip.Left\0");
const TOP: &[u16] = &wide(*b"AVLBASIC.Clip.Top\0");
const RIGHT: &[u16] = &wide(*b"AVLBASIC.Clip.Right\0");
const BOTTOM: &[u16] = &wide(*b"AVLBASIC.Clip.Bottom\0");
const RECT_PROPERTIES: [&[u16]; 5] = [OWNED, LEFT, TOP, RIGHT, BOTTOM];

type SetThreadDpi = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
type ConvertPoint = unsafe extern "system" fn(*mut c_void, *mut Point) -> i32;

struct DpiApi {
    set_thread: SetThreadDpi,
    to_physical: ConvertPoint,
    to_logical: ConvertPoint,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

fn dpi_api() -> Result<&'static DpiApi, String> {
    static API: OnceLock<Option<DpiApi>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let module = GetModuleHandleW(wide(*b"user32.dll\0").as_ptr());
        if module.is_null() {
            return None;
        }
        let set = GetProcAddress(module, c"SetThreadDpiAwarenessContext".as_ptr().cast());
        let convert = GetProcAddress(
            module,
            c"LogicalToPhysicalPointForPerMonitorDPI".as_ptr().cast(),
        );
        let inverse = GetProcAddress(
            module,
            c"PhysicalToLogicalPointForPerMonitorDPI".as_ptr().cast(),
        );
        if set.is_null() || convert.is_null() || inverse.is_null() {
            None
        } else {
            Some(DpiApi {
                set_thread: std::mem::transmute::<*mut c_void, SetThreadDpi>(set),
                to_physical: std::mem::transmute::<*mut c_void, ConvertPoint>(convert),
                to_logical: std::mem::transmute::<*mut c_void, ConvertPoint>(inverse),
            })
        }
    })
    .as_ref()
    .ok_or_else(|| "Safe DPI-aware pointer confinement is unavailable".into())
}

struct PhysicalCoordinates {
    previous: *mut c_void,
    restore: SetThreadDpi,
}

impl PhysicalCoordinates {
    fn enter() -> Result<Self, String> {
        let set_thread = dpi_api()?.set_thread;
        // Only the immediate ClipCursor/GetClipCursor/system-metrics call is
        // physical. Do not create windows, pump events or read minifb input in
        // this scope, and never change process/window DPI awareness.
        let previous = unsafe { set_thread(-3isize as *mut c_void) }; // PER_MONITOR_AWARE
        if previous.is_null() {
            Err("Could not select physical pointer coordinates".into())
        } else {
            Ok(Self {
                previous,
                restore: set_thread,
            })
        }
    }
}

impl Drop for PhysicalCoordinates {
    fn drop(&mut self) {
        unsafe { (self.restore)(self.previous) };
    }
}

#[link(name = "user32")]
extern "system" {
    fn GetClipCursor(rect: *mut Rect) -> i32;
    fn ClipCursor(rect: *const Rect) -> i32;
    fn GetForegroundWindow() -> *mut c_void;
    fn GetFocus() -> *mut c_void;
    fn IsIconic(window: *mut c_void) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn ClientToScreen(window: *mut c_void, point: *mut Point) -> i32;
}

/// Read the integer physical position before converting it to this HWND's
/// logical screen coordinates. GetCursorPos alone can transiently round a
/// clipped relative-motion packet outside the client at non-100% desktop DPI.
pub(super) fn canonical_screen_pointer(window: *mut c_void) -> Option<Point> {
    let inverse = dpi_api().ok()?.to_logical;
    let mut point = Point::default();
    {
        // The same physical coordinate convention as ClipCursor/GetClipCursor.
        // Do not dispatch events or read minifb state within this scope.
        let _coordinates = PhysicalCoordinates::enter().ok()?;
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return None;
        }
    }
    (unsafe { inverse(window, &mut point) } != 0).then_some(point)
}

pub(super) fn pointer_is_confined_inside(window: *mut c_void) -> bool {
    let Some(rect) = owned(window) else {
        return false;
    };
    if unsafe { GetForegroundWindow() } != window
        || unsafe { GetFocus() } != window
        || unsafe { IsIconic(window) } != 0
        || current_clip().ok() != Some(rect)
    {
        return false;
    }
    let Ok(_coordinates) = PhysicalCoordinates::enter() else {
        return false;
    };
    let mut point = Point::default();
    (unsafe { GetCursorPos(&mut point) } != 0)
        && point.x >= rect.left
        && point.x < rect.right
        && point.y >= rect.top
        && point.y < rect.bottom
}

fn current_clip() -> Result<Rect, String> {
    let _coordinates = PhysicalCoordinates::enter()?;
    let mut rect = Rect::default();
    if unsafe { GetClipCursor(&mut rect) } == 0 {
        Err("Could not inspect pointer confinement".into())
    } else {
        Ok(rect)
    }
}

fn desktop() -> Result<Rect, String> {
    let _coordinates = PhysicalCoordinates::enter()?;
    unsafe {
        let left = GetSystemMetrics(76); // SM_XVIRTUALSCREEN
        let top = GetSystemMetrics(77);
        Ok(Rect {
            left,
            top,
            right: left.saturating_add(GetSystemMetrics(78)),
            bottom: top.saturating_add(GetSystemMetrics(79)),
        })
    }
}

fn set_clip(rect: Option<&Rect>) -> Result<(), String> {
    let _coordinates = PhysicalCoordinates::enter()?;
    if unsafe { ClipCursor(rect.map_or(ptr::null(), |rect| rect)) } == 0 {
        Err("Could not update physical pointer confinement".into())
    } else {
        Ok(())
    }
}

fn owned(window: *mut c_void) -> Option<Rect> {
    unsafe {
        if GetPropW(window, OWNED.as_ptr()).is_null() {
            None
        } else {
            Some(Rect {
                left: GetPropW(window, LEFT.as_ptr()) as isize as i32,
                top: GetPropW(window, TOP.as_ptr()) as isize as i32,
                right: GetPropW(window, RIGHT.as_ptr()) as isize as i32,
                bottom: GetPropW(window, BOTTOM.as_ptr()) as isize as i32,
            })
        }
    }
}

fn forget_owned(window: *mut c_void) {
    for name in RECT_PROPERTIES {
        unsafe { RemovePropW(window, name.as_ptr()) };
    }
}

fn save_owned(window: *mut c_void, rect: Rect) -> Result<(), String> {
    // No clip is held while these properties are replaced. Mark ownership last,
    // so a failed SetProp cannot leave an incomplete rectangle marked as ours.
    for (name, value) in [
        (LEFT, rect.left as isize),
        (TOP, rect.top as isize),
        (RIGHT, rect.right as isize),
        (BOTTOM, rect.bottom as isize),
        (OWNED, 1),
    ] {
        if unsafe { SetPropW(window, name.as_ptr(), value as *mut c_void) } == 0 {
            forget_owned(window);
            return Err("Could not retain pointer confinement bounds".into());
        }
    }
    Ok(())
}

pub(super) fn release(window: *mut c_void) -> Result<(), String> {
    if let Some(previous) = owned(window) {
        // ClipCursor has no owner token. Equality is the conservative available
        // check: never clear a different rectangle installed by another app.
        if current_clip()? == previous {
            set_clip(None)?;
        }
        forget_owned(window);
    }
    Ok(())
}

pub(super) fn disable(window: *mut c_void) -> Result<(), String> {
    unsafe {
        RemovePropW(window, WIDTH.as_ptr());
        RemovePropW(window, HEIGHT.as_ptr());
    }
    release(window)
}

pub(super) fn cleanup(window: *mut c_void) {
    let _ = disable(window);
    forget_owned(window);
}

fn bounds(window: *mut c_void, logical: (usize, usize)) -> Option<Rect> {
    let mut client = Rect::default();
    if unsafe { GetClientRect(window, &mut client) } == 0 {
        return None;
    }
    let viewport = PresentationViewport::fit(
        logical,
        (
            usize::try_from(client.right).ok()?,
            usize::try_from(client.bottom).ok()?,
        ),
    )?;
    // Convert the first and LAST VALID pixel, not the exclusive boundary.
    // At e.g. 150%, virtualizing an exclusive right edge then subtracting one
    // physical pixel can round GetCursorPos back OUTSIDE the logical canvas.
    let mut first = Point {
        x: i32::try_from(viewport.x).ok()?,
        y: i32::try_from(viewport.y).ok()?,
    };
    let mut last = Point {
        x: i32::try_from(viewport.x + viewport.width - 1).ok()?,
        y: i32::try_from(viewport.y + viewport.height - 1).ok()?,
    };
    let convert = dpi_api().ok()?.to_physical;
    for point in [&mut first, &mut last] {
        if unsafe { ClientToScreen(window, point) } == 0 || unsafe { convert(window, point) } == 0 {
            return None;
        }
    }
    let desktop = desktop().ok()?;
    let rect = Rect {
        left: first.x.max(desktop.left),
        top: first.y.max(desktop.top),
        right: last.x.checked_add(1)?.min(desktop.right),
        bottom: last.y.checked_add(1)?.min(desktop.bottom),
    };
    (rect.left < rect.right && rect.top < rect.bottom).then_some(rect)
}

pub(super) fn update(window: *mut c_void) -> Result<(), String> {
    let logical = unsafe {
        (
            GetPropW(window, WIDTH.as_ptr()) as usize,
            GetPropW(window, HEIGHT.as_ptr()) as usize,
        )
    };
    if logical.0 == 0
        || logical.1 == 0
        || unsafe { GetForegroundWindow() } != window
        || unsafe { GetFocus() } != window
        || unsafe { IsIconic(window) } != 0
    {
        return release(window);
    }
    let Some(wanted) = bounds(window, logical) else {
        return release(window);
    };
    let current = current_clip()?;
    let previous = owned(window);
    if previous == Some(wanted) && current == wanted {
        return Ok(());
    }
    if previous.is_some() {
        release(window)?;
    }
    // This also declines an existing clip when we first become active. Losing
    // focus and encountering another application's clip never confiscates it.
    if current_clip()? != desktop()? {
        return Ok(());
    }
    save_owned(window, wanted)?;
    if let Err(error) = set_clip(Some(&wanted)) {
        forget_owned(window);
        return Err(error);
    }
    Ok(())
}

pub(super) fn set(
    window: *mut c_void,
    logical: (usize, usize),
    enabled: bool,
) -> Result<(), String> {
    if !enabled || logical.0 == 0 || logical.1 == 0 {
        return disable(window);
    }
    for (name, value) in [(WIDTH, logical.0), (HEIGHT, logical.1)] {
        if unsafe { GetPropW(window, name.as_ptr()) } as usize != value
            && unsafe { SetPropW(window, name.as_ptr(), value as *mut c_void) } == 0
        {
            let _ = disable(window);
            return Err("Could not configure pointer confinement".into());
        }
    }
    update(window)
}

#[cfg(test)]
mod tests {
    use super::super::CursorController;
    use super::*;
    use minifb::{MouseMode, Window, WindowOptions};
    use std::time::{Duration, Instant};

    #[link(name = "user32")]
    extern "system" {
        fn SetForegroundWindow(window: *mut c_void) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn ShowWindow(window: *mut c_void, command: i32) -> i32;
    }

    struct RestoreCursor {
        rect: Rect,
        point: Point,
    }

    fn move_physical(x: i32, y: i32) -> i32 {
        let _coordinates = PhysicalCoordinates::enter().unwrap();
        unsafe { SetCursorPos(x, y) }
    }

    fn physical_position() -> Point {
        let _coordinates = PhysicalCoordinates::enter().unwrap();
        let mut point = Point::default();
        assert_ne!(unsafe { GetCursorPos(&mut point) }, 0);
        point
    }

    impl Drop for RestoreCursor {
        fn drop(&mut self) {
            let _ = set_clip(Some(&self.rect));
            move_physical(self.point.x, self.point.y);
        }
    }

    fn focus(window: &mut Window) {
        let hwnd = window.get_window_handle();
        unsafe { SetForegroundWindow(hwnd) };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            window.update();
            if unsafe { GetForegroundWindow() } == hwnd && unsafe { GetFocus() } == hwnd {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "native test needs foreground permission on an unlocked desktop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "moves the shared Windows pointer in two minifb windows; restores initial state"]
    fn native_confinement_respects_viewport_focus_other_clips_and_drop() {
        // Deliberately do not change DPI awareness: exercise minifb's actual
        // process/thread coordinate convention, including Windows scaling.
        let original = current_clip().unwrap();
        assert_eq!(
            original,
            desktop().unwrap(),
            "another application already confines the pointer"
        );
        let point = physical_position();
        let restore = RestoreCursor {
            rect: original,
            point,
        };
        let mut window = Window::new(
            "AVL native pointer confinement test",
            960,
            540,
            WindowOptions::default(),
        )
        .unwrap();
        window.set_position(50, 50);
        let hwnd = window.get_window_handle();
        let mut cursor = CursorController::new(&window).unwrap();
        cursor.set_visible(false).unwrap();
        focus(&mut window);
        cursor.set_confinement((640, 480), true).unwrap();
        let expected = bounds(hwnd, (640, 480)).unwrap();
        assert_eq!(current_clip().unwrap(), expected);
        for (x, y, canvas_expected) in [
            (expected.left - 50, expected.top - 50, (0, 0)),
            (expected.right + 50, expected.bottom + 50, (639, 479)),
            (expected.left - 50, expected.bottom + 50, (0, 479)),
            (expected.right + 50, expected.top - 50, (639, 0)),
        ] {
            assert_ne!(move_physical(x, y), 0);
            let actual = physical_position();
            assert_eq!(
                (actual.x, actual.y),
                (
                    x.clamp(expected.left, expected.right - 1),
                    y.clamp(expected.top, expected.bottom - 1)
                )
            );
            // Read through minifb's unchanged DPI context and its actual
            // GetCursorPos -> ScreenToClient path. No outside pixel is allowed.
            window.update();
            let point = window.get_unscaled_mouse_pos(MouseMode::Pass).unwrap();
            let viewport = PresentationViewport::fit((640, 480), window.get_size()).unwrap();
            assert_eq!(
                viewport.canvas_pixel(point, (640, 480)),
                Some(canvas_expected)
            );
        }

        // A real focus transfer produces WM_KILLFOCUS/WA_INACTIVE; no manual
        // release call is made before checking the shared clip.
        let mut other = Window::new(
            "AVL native focus target",
            320,
            240,
            WindowOptions::default(),
        )
        .unwrap();
        focus(&mut other);
        window.update();
        assert_eq!(current_clip().unwrap(), original);
        focus(&mut window);
        cursor.set_confinement((640, 480), true).unwrap();
        assert_eq!(current_clip().unwrap(), bounds(hwnd, (640, 480)).unwrap());
        cursor.set_confinement((1280, 720), true).unwrap();
        assert_eq!(current_clip().unwrap(), bounds(hwnd, (1280, 720)).unwrap());
        cursor.set_visible(true).unwrap();
        assert_eq!(current_clip().unwrap(), original);

        cursor.set_visible(false).unwrap();
        cursor.set_confinement((640, 480), true).unwrap();
        let another = Rect {
            left: original.left + 20,
            top: original.top + 20,
            right: original.left + 120,
            bottom: original.top + 120,
        };
        set_clip(Some(&another)).unwrap();
        cursor.release_confinement().unwrap();
        assert_eq!(
            current_clip().unwrap(),
            another,
            "must not release a newer foreign clip"
        );
        cursor.set_confinement((640, 480), true).unwrap();
        assert_eq!(
            current_clip().unwrap(),
            another,
            "must not acquire over a foreign clip"
        );
        set_clip(None).unwrap(); // the foreign test clip is ours
        cursor.set_confinement((640, 480), true).unwrap();
        unsafe { ShowWindow(hwnd, 6) }; // SW_MINIMIZE
        window.update();
        assert_eq!(current_clip().unwrap(), original);
        unsafe { ShowWindow(hwnd, 9) }; // SW_RESTORE
        focus(&mut window);
        cursor.set_confinement((640, 480), true).unwrap();
        assert_eq!(current_clip().unwrap(), bounds(hwnd, (640, 480)).unwrap());
        drop(cursor);
        assert_eq!(current_clip().unwrap(), original);
        drop(other);
        drop(window);
        drop(restore);
        let restored = physical_position();
        assert_eq!((restored.x, restored.y), (point.x, point.y));
        assert_eq!(current_clip().unwrap(), original);
    }
}
