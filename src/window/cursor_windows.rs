//! Keep cursor visibility local to the graphics window. minifb's Windows
//! visibility setter changes ShowCursor's counter instead of a boolean state.
//! The same HWND-owned callback queues native maximize requests for fullscreen.
use minifb::Window;
use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "confinement_windows.rs"]
mod confinement;

const WM_SETCURSOR: u32 = 0x0020;
const WM_NCDESTROY: u32 = 0x0082;
const WM_NCHITTEST: u32 = 0x0084;
const WM_SYSCOMMAND: u32 = 0x0112;
const SC_MAXIMIZE: usize = 0xf030;
const WM_MOUSEMOVE: isize = 0x0200;
const HTCLIENT: isize = 1;
const GWLP_WNDPROC: i32 = -4;
const GWL_STYLE: i32 = -16;
const GCLP_HCURSOR: i32 = -12;
const WINDOW_DECORATIONS: isize = 0x00c0_0000 | 0x0004_0000; // CAPTION | THICKFRAME
const WS_MAXIMIZEBOX: isize = 0x0001_0000;
const FRAME_CHANGED: u32 = 0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020;
// These properties belong to the HWND, not to movable/droppable Rust state.
const PREVIOUS_PROC: &[u16] = &[
    65, 86, 76, 66, 65, 83, 73, 67, 46, 67, 117, 114, 115, 111, 114, 46, 80, 114, 101, 118, 105,
    111, 117, 115, 80, 114, 111, 99, 0,
];
const HIDDEN: &[u16] = &[
    65, 86, 76, 66, 65, 83, 73, 67, 46, 67, 117, 114, 115, 111, 114, 46, 72, 105, 100, 100, 101,
    110, 0,
];
const OWNER: &[u16] = &[
    65, 86, 76, 66, 65, 83, 73, 67, 46, 67, 117, 114, 115, 111, 114, 46, 79, 119, 110, 101, 114, 0,
];
// ENABLED is 1 when we added WS_MAXIMIZEBOX, 2 when it was already present.
// Both values enable interception; removing it makes a retained callback inert.
const MAXIMIZE_ENABLED: &[u16] = &[
    65, 86, 76, 66, 65, 83, 73, 67, 46, 77, 97, 120, 105, 109, 105, 122, 101, 46, 69, 110, 97, 98,
    108, 101, 100, 0,
];
const MAXIMIZE_REQUESTED: &[u16] = &[
    65, 86, 76, 66, 65, 83, 73, 67, 46, 77, 97, 120, 105, 109, 105, 122, 101, 46, 82, 101, 113,
    117, 101, 115, 116, 101, 100, 0,
];
static NEXT_OWNER: AtomicUsize = AtomicUsize::new(1);

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[link(name = "user32")]
extern "system" {
    #[cfg_attr(target_pointer_width = "32", link_name = "GetWindowLongW")]
    fn GetWindowLongPtrW(window: *mut c_void, index: i32) -> isize;
    #[cfg_attr(target_pointer_width = "32", link_name = "SetWindowLongW")]
    fn SetWindowLongPtrW(window: *mut c_void, index: i32, value: isize) -> isize;
    #[cfg_attr(target_pointer_width = "32", link_name = "SetClassLongW")]
    fn SetClassLongPtrW(window: *mut c_void, index: i32, value: isize) -> usize;
    fn SetWindowPos(
        window: *mut c_void,
        insert_after: *mut c_void,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn CallWindowProcW(
        previous: *mut c_void,
        window: *mut c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize;
    fn DefWindowProcW(window: *mut c_void, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetPropW(window: *mut c_void, name: *const u16, data: *mut c_void) -> i32;
    fn GetPropW(window: *mut c_void, name: *const u16) -> *mut c_void;
    fn RemovePropW(window: *mut c_void, name: *const u16) -> *mut c_void;
    fn SetCursor(cursor: *mut c_void) -> *mut c_void;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn WindowFromPoint(point: Point) -> *mut c_void;
    fn ScreenToClient(window: *mut c_void, point: *mut Point) -> i32;
    fn GetClientRect(window: *mut c_void, rect: *mut Rect) -> i32;
    fn SendMessageW(window: *mut c_void, message: u32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn SetLastError(code: u32);
    fn GetLastError() -> u32;
}

fn disable_default_class_cursor(window: *mut c_void) -> Result<(), String> {
    // minifb selects its visible cursor explicitly in WM_SETCURSOR. Its class
    // must have no default cursor: otherwise Windows can restore the arrow on
    // motion before this thread dispatches the message that hides it again.
    // Keep this policy for the process-local minifb class, shared by all its
    // HWNDs. Restoring the arrow on one controller's Drop would break another
    // hidden window. Visible clients/title bars still use their normal handlers.
    unsafe {
        SetLastError(0);
        if SetClassLongPtrW(window, GCLP_HCURSOR, 0) == 0 && GetLastError() != 0 {
            return Err("Could not disable the graphics-window default cursor".into());
        }
    }
    Ok(())
}

fn refresh_frame(window: *mut c_void) -> bool {
    // NOMOVE | NOSIZE | NOZORDER | NOACTIVATE | FRAMECHANGED. Enabling the
    // title-bar button must not move, resize, activate, or make borders sizable.
    unsafe { SetWindowPos(window, ptr::null_mut(), 0, 0, 0, 0, FRAME_CHANGED) != 0 }
}

fn enable_maximize_shortcut(window: *mut c_void) {
    let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) };
    if style == 0 {
        return;
    }
    let added = style & WS_MAXIMIZEBOX == 0;
    let state = if added { 1usize } else { 2usize };
    if unsafe { SetPropW(window, MAXIMIZE_ENABLED.as_ptr(), state as *mut c_void) } == 0 {
        return;
    }
    if added
        && (unsafe { SetWindowLongPtrW(window, GWL_STYLE, style | WS_MAXIMIZEBOX) } == 0
            || !refresh_frame(window))
    {
        // This is optional title-bar convenience, not a reason to reject an
        // otherwise usable graphics window. Restore only our attempted change.
        disable_maximize_shortcut(window);
    }
}

fn disable_maximize_shortcut(window: *mut c_void) {
    let state = unsafe {
        let state = RemovePropW(window, MAXIMIZE_ENABLED.as_ptr()) as usize;
        RemovePropW(window, MAXIMIZE_REQUESTED.as_ptr());
        state
    };
    if state == 1 {
        let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) };
        if style != 0 && style & WS_MAXIMIZEBOX != 0 {
            unsafe { SetWindowLongPtrW(window, GWL_STYLE, style & !WS_MAXIMIZEBOX) };
            let _ = refresh_frame(window);
        }
    }
}

fn pointer_in_client(window: *mut c_void) -> bool {
    fn contains(window: *mut c_void, mut point: Point) -> bool {
        let mut rect = Rect::default();
        unsafe {
            WindowFromPoint(point) == window
                && ScreenToClient(window, &mut point) != 0
                && GetClientRect(window, &mut rect) != 0
                && point.x >= rect.left
                && point.y >= rect.top
                && point.x < rect.right
                && point.y < rect.bottom
        }
    }
    let mut point = Point::default();
    (unsafe { GetCursorPos(&mut point) } != 0 && contains(window, point))
        || confinement::canonical_screen_pointer(window)
            .is_some_and(|point| contains(window, point))
}

fn refresh(window: *mut c_void) {
    // SetCursor affects the current cursor, so never change another window's
    // cursor, or the ordinary title-bar/resize cursor, as a side effect.
    if pointer_in_client(window) {
        unsafe {
            SendMessageW(
                window,
                WM_SETCURSOR,
                window as usize,
                HTCLIENT | (WM_MOUSEMOVE << 16),
            );
        }
    }
}

fn clipped_fullscreen_edge(window: *mut c_void, lparam: isize) -> bool {
    if unsafe { GetPropW(window, HIDDEN.as_ptr()) }.is_null()
        || unsafe { GetWindowLongPtrW(window, GWL_STYLE) } & WINDOW_DECORATIONS != 0
    {
        return false;
    }
    // GET_X/Y_LPARAM: signed coordinates also cover monitors left/above zero.
    let mut point = Point {
        x: lparam as u16 as i16 as i32,
        y: (lparam >> 16) as u16 as i16 as i32,
    };
    let mut rect = Rect::default();
    (unsafe { ScreenToClient(window, &mut point) } != 0)
        && unsafe { GetClientRect(window, &mut rect) } != 0
        && point.x >= rect.left.saturating_sub(1)
        && point.x <= rect.right
        && point.y >= rect.top.saturating_sub(1)
        && point.y <= rect.bottom
        && confinement::pointer_is_confined_inside(window)
}

unsafe extern "system" fn cursor_subclass(
    window: *mut c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let previous = GetPropW(window, PREVIOUS_PROC.as_ptr());
    if message == WM_SYSCOMMAND
        && wparam & 0xfff0 == SC_MAXIMIZE
        && !GetPropW(window, MAXIMIZE_ENABLED.as_ptr()).is_null()
    {
        // Block ordinary maximization before it can replace the normal
        // placement. The owner enters fullscreen after native dispatch ends;
        // changing presentation inside WNDPROC would re-enter window handling.
        // Duplicate commands represent ON, never a fullscreen toggle.
        let _ = SetPropW(window, MAXIMIZE_REQUESTED.as_ptr(), 1usize as *mut c_void);
        return 0;
    }
    // Release before forwarding messages that can transfer input or destroy
    // the HWND. Activation alone is insufficient: acquisition also verifies
    // GetForegroundWindow/GetFocus in the native confinement adapter.
    if message == 0x0008 // WM_KILLFOCUS
        || (message == 0x0006 && wparam & 0xffff == 0) // WA_INACTIVE
        || (message == 0x001c && wparam == 0)
    // WM_ACTIVATEAPP false
    {
        let _ = confinement::release(window);
    }
    if message == WM_NCDESTROY {
        confinement::cleanup(window);
    }
    if message == WM_SETCURSOR
        && !GetPropW(window, HIDDEN.as_ptr()).is_null()
        && ((lparam & 0xffff) == HTCLIENT
            || (wparam == window as usize
                && GetWindowLongPtrW(window, GWL_STYLE) & WINDOW_DECORATIONS == 0))
    {
        // Relative mouse packets at a clipped DPI-scaled edge can report
        // virtual -1/width and HTNOWHERE even though the physical pointer is
        // still inside. A fullscreen HWND has no non-client decorations:
        // honor Windows' own recipient instead of rechecking that bad point.
        SetCursor(ptr::null_mut());
        return 1;
    }
    if message == WM_NCDESTROY
        && GetWindowLongPtrW(window, GWLP_WNDPROC) == cursor_subclass as *const () as isize
        && !previous.is_null()
    {
        SetWindowLongPtrW(window, GWLP_WNDPROC, previous as isize);
    }
    let mut result = if previous.is_null() {
        DefWindowProcW(window, message, wparam, lparam)
    } else {
        // The previous value can be a Windows-internal procedure token. It
        // must go through CallWindowProc, never a direct Rust function call.
        CallWindowProcW(previous, window, message, wparam, lparam)
    };
    if message == WM_NCHITTEST && result == 0 && clipped_fullscreen_edge(window, lparam) {
        // The physical pointer is inside our owned clip, but Windows rounded
        // the virtual position one pixel outside. Keep the packet client-side
        // so button-up/move messages are not misrouted as non-client input.
        result = HTCLIENT;
    }
    if message == WM_NCDESTROY {
        RemovePropW(window, MAXIMIZE_ENABLED.as_ptr());
        RemovePropW(window, MAXIMIZE_REQUESTED.as_ptr());
        RemovePropW(window, HIDDEN.as_ptr());
        RemovePropW(window, PREVIOUS_PROC.as_ptr());
        RemovePropW(window, OWNER.as_ptr());
    } else if matches!(message, 0x0003 | 0x0005 | 0x0007 | 0x02e0)
        || (message == 0x0006 && wparam & 0xffff != 0)
        || (message == 0x001c && wparam != 0)
    {
        // WM_MOVE / SIZE / SETFOCUS / DPICHANGED, or activation. Use the
        // post-message client geometry and the same fit as presentation.
        let _ = confinement::update(window);
    }
    result
}

/// The owning GraphicsWindow drops this before its minifb Window. Callback
/// state lives in HWND properties, never in pointers into Rust-owned state.
pub(super) struct CursorController {
    window: *mut c_void,
    owner: usize,
}

impl CursorController {
    pub(super) fn new(window: &Window) -> Result<Self, String> {
        let window = window.get_window_handle();
        // Never recycle a generation: an old controller must not alter a new
        // graphics window if Windows reuses its destroyed HWND before Drop.
        let owner = NEXT_OWNER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| "Graphics-window cursor identifiers exhausted".to_string())?;
        let previous = unsafe { GetWindowLongPtrW(window, GWLP_WNDPROC) };
        if previous == 0
            || !unsafe { GetPropW(window, PREVIOUS_PROC.as_ptr()) }.is_null()
            || !unsafe { GetPropW(window, OWNER.as_ptr()) }.is_null()
        {
            return Err("Could not install graphics-window cursor handling".into());
        }
        // Installation and message dispatch run on the window's owning
        // thread. Store the chain before making our callback reachable.
        if unsafe { SetPropW(window, PREVIOUS_PROC.as_ptr(), previous as *mut c_void) } == 0 {
            return Err("Could not save graphics-window cursor handling".into());
        }
        if unsafe { SetPropW(window, OWNER.as_ptr(), owner as *mut c_void) } == 0 {
            unsafe { RemovePropW(window, PREVIOUS_PROC.as_ptr()) };
            return Err("Could not identify graphics-window cursor handling".into());
        }
        let replaced = unsafe {
            SetWindowLongPtrW(window, GWLP_WNDPROC, cursor_subclass as *const () as isize)
        };
        if replaced == 0 {
            // A valid WNDPROC is nonzero, so zero here unambiguously means
            // failure. No callback was installed and no property may remain.
            unsafe { RemovePropW(window, PREVIOUS_PROC.as_ptr()) };
            unsafe { RemovePropW(window, OWNER.as_ptr()) };
            return Err("Could not install graphics-window cursor handling".into());
        }
        // Construct the owner first so a failed class update unwinds our
        // properties and subclass through the ordinary controller cleanup.
        let controller = Self { window, owner };
        disable_default_class_cursor(window)?;
        enable_maximize_shortcut(window);
        Ok(controller)
    }

    pub(super) fn take_maximize_request(&mut self) -> bool {
        self.installed()
            && !unsafe { GetPropW(self.window, MAXIMIZE_ENABLED.as_ptr()) }.is_null()
            && !unsafe { RemovePropW(self.window, MAXIMIZE_REQUESTED.as_ptr()) }.is_null()
    }

    pub(super) fn set_visible(&mut self, visible: bool) -> Result<(), String> {
        if !self.installed() {
            return Err("Graphics-window cursor handling is no longer available".into());
        }
        if visible {
            let _ = self.release_confinement();
            unsafe { RemovePropW(self.window, HIDDEN.as_ptr()) };
        } else if unsafe { SetPropW(self.window, HIDDEN.as_ptr(), 1usize as *mut c_void) } == 0 {
            return Err("Could not update graphics-window cursor visibility".into());
        }
        self.refresh();
        Ok(())
    }

    pub(super) fn refresh(&self) {
        refresh(self.window);
    }

    pub(super) fn canonical_client_pointer(&self) -> Option<(f32, f32)> {
        if !self.installed() {
            return None;
        }
        let mut point = confinement::canonical_screen_pointer(self.window)?;
        (unsafe { ScreenToClient(self.window, &mut point) } != 0)
            .then_some((point.x as f32, point.y as f32))
    }

    /// Optional convenience; callers may continue without confinement when
    /// desktop access is unavailable or another application owns a clip.
    pub(super) fn set_confinement(
        &mut self,
        logical: (usize, usize),
        enabled: bool,
    ) -> Result<(), String> {
        if !self.installed() {
            return Err("Graphics-window cursor handling is no longer available".into());
        }
        confinement::set(self.window, logical, enabled)
    }

    pub(super) fn release_confinement(&mut self) -> Result<(), String> {
        if self.installed() {
            confinement::disable(self.window)
        } else {
            Ok(())
        }
    }

    fn installed(&self) -> bool {
        !self.window.is_null()
            && unsafe { GetPropW(self.window, OWNER.as_ptr()) } as usize == self.owner
            && !unsafe { GetPropW(self.window, PREVIOUS_PROC.as_ptr()) }.is_null()
    }

    pub(super) fn release(&mut self) {
        // WM_NCDESTROY may already have removed us. Do not restore a callback
        // on an unrelated window if Windows has since reused that HWND.
        if self.installed() {
            disable_maximize_shortcut(self.window);
            confinement::cleanup(self.window);
            // Restore the visible cursor before fullscreen restoration or
            // destruction can move this window away from the stationary mouse.
            unsafe { RemovePropW(self.window, HIDDEN.as_ptr()) };
            self.refresh();
            // A later subclass may have chained to us. In that case leave a
            // transparent callback and its HWND-owned chain until NCDESTROY,
            // rather than replacing that subclass or discarding its target.
            if unsafe { GetWindowLongPtrW(self.window, GWLP_WNDPROC) }
                == cursor_subclass as *const () as isize
            {
                let previous = unsafe { GetPropW(self.window, PREVIOUS_PROC.as_ptr()) };
                if unsafe { SetWindowLongPtrW(self.window, GWLP_WNDPROC, previous as isize) } != 0 {
                    unsafe { RemovePropW(self.window, PREVIOUS_PROC.as_ptr()) };
                    unsafe { RemovePropW(self.window, OWNER.as_ptr()) };
                }
            }
        }
        self.window = ptr::null_mut();
    }
}

impl Drop for CursorController {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::Graphics;
    use crate::window::GraphicsWindow;
    use std::mem::size_of;
    use std::time::{Duration, Instant};

    #[repr(C)]
    #[derive(Default)]
    struct CursorInfo {
        size: u32,
        flags: u32,
        cursor: *mut c_void,
        position: Point,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetCursorInfo(info: *mut CursorInfo) -> i32;
        #[cfg_attr(target_pointer_width = "32", link_name = "GetClassLongW")]
        fn GetClassLongPtrW(window: *mut c_void, index: i32) -> usize;
    }

    #[track_caller]
    fn assert_cursor_visible(visible: bool, window: Option<&GraphicsWindow>) {
        // Windows can publish the global cursor state shortly after the
        // window transition. Observe that state without changing it or moving
        // the pointer; a later result from another window must not count.
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            if let Some(window) = window {
                assert!(
                    pointer_in_client(window.window.get_window_handle()),
                    "cursor verification lost the test window under the mouse"
                );
            }
            let mut info = CursorInfo {
                size: size_of::<CursorInfo>() as u32,
                ..CursorInfo::default()
            };
            assert_ne!(unsafe { GetCursorInfo(&mut info) }, 0);
            if (info.flags & 1 != 0) == visible && (!visible || !info.cursor.is_null()) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "cursor did not become visible={visible} within 500 ms: flags={}, handle={:?}, position=({}, {})",
                info.flags,
                info.cursor,
                info.position.x,
                info.position.y
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn put_window_under_stationary_pointer(window: &mut GraphicsWindow) {
        let mut point = Point::default();
        assert_ne!(unsafe { GetCursorPos(&mut point) }, 0);
        window
            .restore_position((point.x as isize - 100, point.y as isize - 100))
            .unwrap();
        window.focus();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            window.window.update();
            if pointer_in_client(window.window.get_window_handle()) {
                window.cursor.refresh();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "test window must be under the mouse on an unlocked desktop"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "opens a Windows window under the stationary mouse and toggles fullscreen"]
    fn native_mouse_visibility_is_local_idempotent_and_survives_fullscreen() {
        let graphics = Graphics::default();
        let mut window = GraphicsWindow::new(&graphics).unwrap();
        window.set_title("AVL cursor visibility verification");
        put_window_under_stationary_pointer(&mut window);
        assert!(window.mouse_cursor_visible());
        assert_cursor_visible(true, Some(&window));

        // Previously these repeated ON calls incremented ShowCursor, so one
        // later OFF could not hide the pointer even though BASIC said OFF.
        for _ in 0..64 {
            window.set_mouse_cursor_visible(true).unwrap();
        }
        for fullscreen in [false, true, false, true] {
            window.set_fullscreen(fullscreen).unwrap();
            assert!(pointer_in_client(window.window.get_window_handle()));
            for visible in [false, false, true, true, false] {
                window.set_mouse_cursor_visible(visible).unwrap();
                assert_eq!(window.mouse_cursor_visible(), visible);
                assert_cursor_visible(visible, Some(&window));
                // Exercise the same message Windows sends after mouse motion;
                // neither this test nor the implementation moves the pointer.
                window.cursor.refresh();
                assert_cursor_visible(visible, Some(&window));
            }
            let handle = window.window.get_window_handle();
            unsafe {
                SendMessageW(handle, WM_SETCURSOR, handle as usize, 2); // HTCAPTION
            }
            assert_cursor_visible(!fullscreen, Some(&window));
            window.cursor.refresh();
            assert_cursor_visible(false, Some(&window));
        }
        // Destruction restores visibility before leaving fullscreen, even if
        // the restored window no longer covers the stationary mouse.
        drop(window);
        assert_cursor_visible(true, None);

        let mut reopened = GraphicsWindow::new(&graphics).unwrap();
        put_window_under_stationary_pointer(&mut reopened);
        assert!(reopened.mouse_cursor_visible());
        assert_cursor_visible(true, Some(&reopened));
        reopened.set_mouse_cursor_visible(false).unwrap();
        let handle = reopened.window.get_window_handle();
        unsafe { SendMessageW(handle, 0x0010, 0, 0) }; // WM_CLOSE, as with the X button
        assert!(!reopened.is_open());
        drop(reopened);
        assert_cursor_visible(true, None);
    }

    #[test]
    #[ignore = "opens a Windows window under the stationary mouse"]
    fn native_cursor_release_preserves_outer_subclass_and_owner() {
        const OUTER_PREVIOUS: &[u16] = &[
            65, 86, 76, 67, 117, 114, 115, 111, 114, 84, 101, 115, 116, 0,
        ];
        unsafe extern "system" fn outer(
            window: *mut c_void,
            message: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize {
            let previous = GetPropW(window, OUTER_PREVIOUS.as_ptr());
            let result = CallWindowProcW(previous, window, message, wparam, lparam);
            if message == WM_NCDESTROY {
                RemovePropW(window, OUTER_PREVIOUS.as_ptr());
            }
            result
        }

        let mut window = GraphicsWindow::new(&Graphics::default()).unwrap();
        put_window_under_stationary_pointer(&mut window);
        window.set_mouse_cursor_visible(false).unwrap();
        let handle = window.window.get_window_handle();
        // Model an old controller whose HWND has been reused: only the owner
        // differs. Its cleanup must leave the new owner's hidden state alone.
        let mut stale = CursorController {
            window: handle,
            owner: 0,
        };
        stale.release();
        assert!(window.cursor.installed());
        assert_cursor_visible(false, Some(&window));

        let previous = unsafe { GetWindowLongPtrW(handle, GWLP_WNDPROC) };
        assert_ne!(
            unsafe { SetPropW(handle, OUTER_PREVIOUS.as_ptr(), previous as *mut c_void) },
            0
        );
        assert_eq!(
            unsafe { SetWindowLongPtrW(handle, GWLP_WNDPROC, outer as *const () as isize) },
            previous
        );
        window.cursor.refresh();
        assert_cursor_visible(false, Some(&window));
        unsafe { SendMessageW(handle, WM_SYSCOMMAND, SC_MAXIMIZE, 0) };
        assert!(!unsafe { GetPropW(handle, MAXIMIZE_REQUESTED.as_ptr()) }.is_null());
        window.cursor.release();
        assert_eq!(
            unsafe { GetWindowLongPtrW(handle, GWLP_WNDPROC) },
            outer as *const () as isize
        );
        assert!(!unsafe { GetPropW(handle, PREVIOUS_PROC.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, MAXIMIZE_ENABLED.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, MAXIMIZE_REQUESTED.as_ptr()) }.is_null());
        assert_eq!(
            unsafe { GetWindowLongPtrW(handle, GWL_STYLE) } & WS_MAXIMIZEBOX,
            0
        );
        refresh(handle);
        assert_cursor_visible(true, Some(&window));
        // The retained transparent callback forwards destruction and cleans
        // its properties even though its Rust controller has been released.
        drop(window);
        assert!(unsafe { GetPropW(handle, PREVIOUS_PROC.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, OWNER.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, HIDDEN.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, MAXIMIZE_ENABLED.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, MAXIMIZE_REQUESTED.as_ptr()) }.is_null());
        assert!(unsafe { GetPropW(handle, OUTER_PREVIOUS.as_ptr()) }.is_null());
        assert_cursor_visible(true, None);
    }

    #[test]
    #[ignore = "opens native windows under the stationary pointer; run serially"]
    fn native_class_cursor_keeps_visibility_local_across_controller_lifetimes() {
        let graphics = Graphics::default();
        let mut first = GraphicsWindow::new(&graphics).unwrap();
        put_window_under_stationary_pointer(&mut first);
        first.set_mouse_cursor_visible(false).unwrap();
        assert_cursor_visible(false, Some(&first));

        let mut second = GraphicsWindow::new(&graphics).unwrap();
        put_window_under_stationary_pointer(&mut second);
        second.set_mouse_cursor_visible(false).unwrap();
        assert_cursor_visible(false, Some(&second));
        let handle = second.window.get_window_handle();
        assert_eq!(unsafe { GetClassLongPtrW(handle, GCLP_HCURSOR) }, 0);

        // A controller that goes away must not reinstate the class arrow for
        // a second window that is still hiding its own cursor.
        drop(first);
        assert_eq!(unsafe { GetClassLongPtrW(handle, GCLP_HCURSOR) }, 0);
        second.cursor.refresh();
        assert_cursor_visible(false, Some(&second));

        // minifb without our subclass also supplies its visible cursor in
        // WM_SETCURSOR. A NULL class cursor must not leave that window hidden.
        let mut point = Point::default();
        assert_ne!(unsafe { GetCursorPos(&mut point) }, 0);
        let mut plain = Window::new(
            "AVL plain minifb cursor verification",
            320,
            240,
            Default::default(),
        )
        .unwrap();
        plain.set_position(point.x as isize - 50, point.y as isize - 50);
        plain.update();
        let plain_handle = plain.get_window_handle();
        assert!(pointer_in_client(plain_handle));
        unsafe {
            SendMessageW(
                plain_handle,
                WM_SETCURSOR,
                plain_handle as usize,
                HTCLIENT | (WM_MOUSEMOVE << 16),
            )
        };
        assert_cursor_visible(true, None);
        drop(plain);

        put_window_under_stationary_pointer(&mut second);
        assert_cursor_visible(false, Some(&second));
        second.set_mouse_cursor_visible(true).unwrap();
        assert_cursor_visible(true, Some(&second));
        drop(second);
        assert_cursor_visible(true, None);
    }
}
