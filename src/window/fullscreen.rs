//! Desktop fullscreen for the graphics window.
//!
//! This changes window presentation only: it never changes the physical display mode or
//! the logical graphics buffer. The owner calls `set(window, false)` before
//! returning to a console that may be covered, or dropping an open window.
//! A console identified on a separate monitor can leave fullscreen active.

#[cfg(target_os = "linux")]
#[path = "fullscreen_emulation.rs"]
mod emulation;

#[derive(Default)]
pub(super) struct FullscreenController {
    native: platform::Controller,
}

impl FullscreenController {
    pub(super) fn set(&mut self, window: &minifb::Window, enabled: bool) -> Result<(), String> {
        self.native.set(window, enabled)
    }

    pub(super) fn is_fullscreen(&self) -> bool {
        self.native.is_fullscreen()
    }

    pub(super) fn windowed_position(&self, window: &minifb::Window) -> Option<(isize, isize)> {
        self.native.windowed_position(window)
    }

    pub(super) fn set_position(
        window: &minifb::Window,
        position: (isize, isize),
    ) -> Result<(), String> {
        platform::Controller::set_position(window, position)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn release_emulation(&mut self, window: &minifb::Window) -> Result<(), String> {
        self.native.release_emulation(window)
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::mem::size_of;

    type Hwnd = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct WindowPlacement {
        length: u32,
        flags: u32,
        show_cmd: u32,
        min_position: Point,
        max_position: Point,
        normal_position: Rect,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct MonitorInfo {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
        fn SetWindowLongPtrW(window: Hwnd, index: i32, value: isize) -> isize;
        fn GetWindowPlacement(window: Hwnd, placement: *mut WindowPlacement) -> i32;
        fn SetWindowPlacement(window: Hwnd, placement: *const WindowPlacement) -> i32;
        fn GetWindowRect(window: Hwnd, rect: *mut Rect) -> i32;
        fn IsIconic(window: Hwnd) -> i32;
        fn MonitorFromWindow(window: Hwnd, flags: u32) -> *mut c_void;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
        fn SetWindowPos(
            window: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn SetLastError(code: u32);
        fn GetLastError() -> u32;
    }

    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const WS_OVERLAPPEDWINDOW: isize = 0x00cf_0000;
    const EXTENDED_BORDERS: isize = 0x0000_0001 | 0x0000_0100 | 0x0000_0200 | 0x0002_0000;
    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOOWNERZORDER: u32 = 0x0200;

    #[derive(Clone, Copy)]
    struct WindowedState {
        style: isize,
        ex_style: isize,
        placement: WindowPlacement,
        rect: Rect,
        monitor_origin: Point,
    }

    #[derive(Default)]
    pub(super) struct Controller {
        saved: Option<WindowedState>,
    }

    fn last_error(operation: &str) -> String {
        format!("{operation}: {}", std::io::Error::last_os_error())
    }

    fn get_style(window: Hwnd, index: i32) -> Result<isize, String> {
        unsafe {
            SetLastError(0);
            let value = GetWindowLongPtrW(window, index);
            if value == 0 && GetLastError() != 0 {
                Err(last_error("GetWindowLongPtrW"))
            } else {
                Ok(value)
            }
        }
    }

    fn set_style(window: Hwnd, index: i32, value: isize) -> Result<(), String> {
        unsafe {
            SetLastError(0);
            if SetWindowLongPtrW(window, index, value) == 0 && GetLastError() != 0 {
                Err(last_error("SetWindowLongPtrW"))
            } else {
                Ok(())
            }
        }
    }

    fn restore(window: Hwnd, state: WindowedState) -> Result<(), String> {
        set_style(window, GWL_STYLE, state.style)?;
        set_style(window, GWL_EXSTYLE, state.ex_style)?;
        unsafe {
            if SetWindowPlacement(window, &state.placement) == 0 {
                return Err(last_error("SetWindowPlacement"));
            }
            // SetWindowPlacement also restores a maximized/minimized state.
            // A frame refresh makes the restored decorations take effect.
            if SetWindowPos(
                window,
                std::ptr::null_mut(),
                state.rect.left,
                state.rect.top,
                state.rect.right - state.rect.left,
                state.rect.bottom - state.rect.top,
                SWP_FRAMECHANGED
                    | SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOZORDER
                    | SWP_NOACTIVATE
                    | SWP_NOOWNERZORDER,
            ) == 0
            {
                return Err(last_error("SetWindowPos (restore)"));
            }
        }
        Ok(())
    }

    impl Controller {
        pub(super) fn is_fullscreen(&self) -> bool {
            self.saved.is_some()
        }

        pub(super) fn windowed_position(&self, window: &minifb::Window) -> Option<(isize, isize)> {
            let mut rect = Rect::default();
            if !window.is_open()
                || unsafe { GetWindowRect(window.get_window_handle(), &mut rect) } == 0
            {
                return None;
            }
            Some(match self.saved {
                Some(saved) => {
                    let mut info = MonitorInfo {
                        size: size_of::<MonitorInfo>() as u32,
                        ..Default::default()
                    };
                    let monitor = unsafe {
                        MonitorFromWindow(window.get_window_handle(), MONITOR_DEFAULTTONEAREST)
                    };
                    if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
                        return None;
                    }
                    (
                        saved.rect.left as isize + info.monitor.left as isize
                            - saved.monitor_origin.x as isize,
                        saved.rect.top as isize + info.monitor.top as isize
                            - saved.monitor_origin.y as isize,
                    )
                }
                None if unsafe { IsIconic(window.get_window_handle()) } != 0 => {
                    let mut placement = WindowPlacement {
                        length: size_of::<WindowPlacement>() as u32,
                        ..Default::default()
                    };
                    let mut info = MonitorInfo {
                        size: size_of::<MonitorInfo>() as u32,
                        ..Default::default()
                    };
                    let monitor = unsafe {
                        MonitorFromWindow(window.get_window_handle(), MONITOR_DEFAULTTONEAREST)
                    };
                    if monitor.is_null()
                        || unsafe { GetWindowPlacement(window.get_window_handle(), &mut placement) }
                            == 0
                        || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0
                    {
                        return None;
                    }
                    // WINDOWPLACEMENT uses workspace coordinates. Recover the
                    // normal desktop position instead of the minimized -32000.
                    (
                        placement.normal_position.left as isize + info.work.left as isize
                            - info.monitor.left as isize,
                        placement.normal_position.top as isize + info.work.top as isize
                            - info.monitor.top as isize,
                    )
                }
                None => (rect.left as isize, rect.top as isize),
            })
        }

        pub(super) fn set_position(
            window: &minifb::Window,
            position: (isize, isize),
        ) -> Result<(), String> {
            let x = i32::try_from(position.0).map_err(|_| "Window position is out of range")?;
            let y = i32::try_from(position.1).map_err(|_| "Window position is out of range")?;
            if unsafe {
                SetWindowPos(
                    window.get_window_handle(),
                    std::ptr::null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                )
            } == 0
            {
                return Err(last_error("SetWindowPos (position)"));
            }
            Ok(())
        }

        pub(super) fn set(&mut self, window: &minifb::Window, enabled: bool) -> Result<(), String> {
            if !window.is_open() {
                self.saved = None;
                return if enabled {
                    Err("Cannot enter fullscreen after the graphics window was closed".into())
                } else {
                    Ok(())
                };
            }
            if enabled == self.is_fullscreen() {
                return Ok(());
            }
            let handle = window.get_window_handle();
            if !enabled {
                if let Some(saved) = self.saved {
                    let position = self.windowed_position(window);
                    restore(handle, saved)?;
                    if let Some(position) = position {
                        Self::set_position(window, position)?;
                    }
                    self.saved = None;
                }
                return Ok(());
            }

            let mut placement = WindowPlacement {
                length: size_of::<WindowPlacement>() as u32,
                ..WindowPlacement::default()
            };
            let mut rect = Rect::default();
            let mut info = MonitorInfo {
                size: size_of::<MonitorInfo>() as u32,
                ..MonitorInfo::default()
            };
            unsafe {
                if GetWindowPlacement(handle, &mut placement) == 0 {
                    return Err(last_error("GetWindowPlacement"));
                }
                if GetWindowRect(handle, &mut rect) == 0 {
                    return Err(last_error("GetWindowRect"));
                }
                let monitor = MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST);
                if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 {
                    return Err(last_error("GetMonitorInfoW"));
                }
            }
            let saved = WindowedState {
                style: get_style(handle, GWL_STYLE)?,
                ex_style: get_style(handle, GWL_EXSTYLE)?,
                placement,
                rect,
                monitor_origin: Point {
                    x: info.monitor.left,
                    y: info.monitor.top,
                },
            };
            let enter_result = (|| {
                set_style(handle, GWL_STYLE, saved.style & !WS_OVERLAPPEDWINDOW)?;
                set_style(handle, GWL_EXSTYLE, saved.ex_style & !EXTENDED_BORDERS)?;
                unsafe {
                    if SetWindowPos(
                        handle,
                        std::ptr::null_mut(),
                        info.monitor.left,
                        info.monitor.top,
                        info.monitor.right - info.monitor.left,
                        info.monitor.bottom - info.monitor.top,
                        SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                    ) == 0
                    {
                        return Err(last_error("SetWindowPos (fullscreen)"));
                    }
                }
                Ok(())
            })();
            if let Err(error) = enter_result {
                return match restore(handle, saved) {
                    Ok(()) => Err(error),
                    Err(restore_error) => Err(format!("{error}; rollback failed: {restore_error}")),
                };
            }
            self.saved = Some(saved);
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn win32_struct_layouts_match_the_native_abi() {
            assert_eq!(size_of::<Point>(), 8);
            assert_eq!(size_of::<Rect>(), 16);
            assert_eq!(size_of::<WindowPlacement>(), 44);
            assert_eq!(size_of::<MonitorInfo>(), 40);
        }

        #[test]
        #[ignore = "opens and minimizes its own native graphics window"]
        fn minimized_window_retains_its_normal_desktop_position() {
            #[link(name = "user32")]
            extern "system" {
                fn ShowWindow(window: Hwnd, command: i32) -> i32;
            }
            for fullscreen in [false, true] {
                let mut window = minifb::Window::new(
                    "AVL minimized placement verification",
                    640,
                    480,
                    minifb::WindowOptions::default(),
                )
                .unwrap();
                window.set_target_fps(0);
                let mut controller = Controller::default();
                Controller::set_position(&window, (200, 160)).unwrap();
                window.update();
                let normal = controller.windowed_position(&window).unwrap();
                if fullscreen {
                    controller.set(&window, true).unwrap();
                }
                unsafe { ShowWindow(window.get_window_handle(), 6) }; // SW_MINIMIZE
                window.update();
                assert_ne!(unsafe { IsIconic(window.get_window_handle()) }, 0);
                assert_eq!(controller.windowed_position(&window), Some(normal));
                if fullscreen {
                    controller.set(&window, false).unwrap();
                    window.update();
                    assert_eq!(controller.windowed_position(&window), Some(normal));
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::emulation;
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use std::ffi::{c_long, c_uchar, c_ulong};
    use std::ptr;
    use std::time::{Duration, Instant};
    use x11_dl::{xlib, xrandr};

    const TRANSITION_TIMEOUT: Duration = Duration::from_millis(500);
    // ICCCM WM_STATE values (not provided by x11-dl's Xlib bindings).
    const NORMAL_STATE: c_ulong = 1;
    const ICONIC_STATE: c_ulong = 3;

    #[derive(Clone, Copy)]
    struct WindowedState {
        hints: xlib::XSizeHints,
        width: u32,
        height: u32,
        position: (isize, isize),
        monitor_origin: Option<(isize, isize)>,
    }

    #[derive(Default)]
    pub(super) struct Controller {
        active: bool,
        saved: Option<WindowedState>,
        emulation: emulation::State,
    }

    fn native_window(window: &minifb::Window) -> Option<(*mut xlib::Display, i32, xlib::Window)> {
        if !window.is_open() {
            return None;
        }
        let display = window.display_handle().ok()?;
        let handle = window.window_handle().ok()?;
        let (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(handle)) =
            (display.as_raw(), handle.as_raw())
        else {
            return None;
        };
        Some((
            display.display?.as_ptr().cast(),
            display.screen,
            handle.window,
        ))
    }

    // Walk to the WM's outer frame. XGetWindowAttributes on the client alone
    // reports coordinates relative to its reparenting frame, not the desktop.
    fn outer_geometry(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
    ) -> Option<(isize, isize, isize, isize)> {
        let mut current = id;
        for _ in 0..32 {
            let (mut root, mut parent, mut children, mut count) = (0, 0, ptr::null_mut(), 0);
            let ok = unsafe {
                (xlib.XQueryTree)(
                    display,
                    current,
                    &mut root,
                    &mut parent,
                    &mut children,
                    &mut count,
                )
            };
            if !children.is_null() {
                unsafe { (xlib.XFree)(children.cast()) };
            }
            if ok == 0 || parent == 0 {
                return None;
            }
            if parent == root {
                let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
                if unsafe { (xlib.XGetWindowAttributes)(display, current, &mut attrs) } == 0 {
                    return None;
                }
                return Some((
                    attrs.x as isize,
                    attrs.y as isize,
                    attrs.width as isize + 2 * attrs.border_width as isize,
                    attrs.height as isize + 2 * attrs.border_width as isize,
                ));
            }
            current = parent;
        }
        None
    }

    fn monitor_origin(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        screen: i32,
        geometry: (isize, isize, isize, isize),
    ) -> Option<(isize, isize)> {
        let randr = xrandr::Xrandr::open().ok()?;
        let (mut major, mut minor) = (0, 0);
        if unsafe { (randr.XRRQueryVersion)(display, &mut major, &mut minor) } == 0
            || (major, minor) < (1, 5)
        {
            return None;
        }
        let mut count = 0;
        let monitors = unsafe {
            (randr.XRRGetMonitors)(
                display,
                (xlib.XRootWindow)(display, screen),
                xlib::True,
                &mut count,
            )
        };
        if monitors.is_null() {
            return None;
        }
        let mut best = (0isize, None);
        if (1..=128).contains(&count) {
            for monitor in unsafe { std::slice::from_raw_parts(monitors, count as usize) } {
                let width = ((geometry.0 + geometry.2)
                    .min(monitor.x as isize + monitor.width as isize)
                    - geometry.0.max(monitor.x as isize))
                .max(0);
                let height = ((geometry.1 + geometry.3)
                    .min(monitor.y as isize + monitor.height as isize)
                    - geometry.1.max(monitor.y as isize))
                .max(0);
                let area = width.saturating_mul(height);
                if area > best.0 {
                    best = (area, Some((monitor.x as isize, monitor.y as isize)));
                } else if area == best.0 && area != 0 {
                    best.1 = None;
                }
            }
        }
        unsafe { (randr.XRRFreeMonitors)(monitors) };
        best.1
    }

    fn move_window(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
        position: (isize, isize),
    ) -> Result<(), String> {
        // Wait for the WM to finish mapping a replacement window. A Configure
        // request sent before mapping can lose to the WM's initial placement.
        let deadline = Instant::now() + Duration::from_millis(200);
        loop {
            let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
            if unsafe { (xlib.XGetWindowAttributes)(display, id, &mut attrs) } == 0
                || attrs.map_state == xlib::IsViewable
                || Instant::now() >= deadline
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut requested = position;
        let mut before = outer_geometry(xlib, display, id).map(|(x, y, _, _)| (x, y));
        if before == Some(position) {
            return Ok(());
        }
        for _ in 0..4 {
            let x = i32::try_from(requested.0).map_err(|_| "Window position is out of range")?;
            let y = i32::try_from(requested.1).map_err(|_| "Window position is out of range")?;
            // Neither request raises the window or asks for keyboard focus.
            unsafe {
                (xlib.XMoveWindow)(display, id, x, y);
                (xlib.XFlush)(display);
            }
            std::thread::sleep(Duration::from_millis(25));
            let after = outer_geometry(xlib, display, id).map(|(x, y, _, _)| (x, y));
            if after == Some(position) {
                return Ok(());
            }
            if let Some(after) = after.filter(|after| Some(*after) != before) {
                // WMs may count compositor shadows differently from the X11
                // outer frame. Correct the observed offset, not a hard-coded
                // decoration size. An ignored request is retried unchanged.
                requested.0 += position.0 - after.0;
                requested.1 += position.1 - after.1;
            }
            before = after;
        }
        if std::env::var_os("AVL_BASIC_CONSOLE_TRACE").is_some_and(|value| value == "1") {
            use std::io::Write;
            let _ =
                writeln!(std::io::stderr().lock(),
                "[window-x11] position not confirmed: requested={position:?}, actual={before:?}"
            );
        }
        // A compositor can legitimately clamp a window to its work area. The
        // measured result remains authoritative; placement is best effort.
        Ok(())
    }

    fn fixed_size_hints(size: (usize, usize)) -> Result<xlib::XSizeHints, String> {
        let width = i32::try_from(size.0).map_err(|_| "The X11 window width is too large")?;
        let height = i32::try_from(size.1).map_err(|_| "The X11 window height is too large")?;
        if width <= 0 || height <= 0 {
            return Err("Cannot save an empty X11 window size".into());
        }
        let mut hints: xlib::XSizeHints = unsafe { std::mem::zeroed() };
        hints.flags = xlib::PMinSize | xlib::PMaxSize;
        hints.min_width = width;
        hints.max_width = width;
        hints.min_height = height;
        hints.max_height = height;
        Ok(hints)
    }

    fn fullscreen_hints(mut hints: xlib::XSizeHints) -> xlib::XSizeHints {
        // A fixed normal window must still be allowed to cover the monitor.
        // Preserve every unrelated WM hint for restoration afterwards.
        hints.flags &= !(xlib::PMinSize | xlib::PMaxSize);
        hints
    }

    fn save_windowed_state(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        screen: i32,
        id: xlib::Window,
        size: (usize, usize),
    ) -> Result<WindowedState, String> {
        let fallback = fixed_size_hints(size)?;
        let mut hints: xlib::XSizeHints = unsafe { std::mem::zeroed() };
        let mut supplied = 0;
        let found =
            unsafe { (xlib.XGetWMNormalHints)(display, id, &mut hints, &mut supplied) != 0 };
        let geometry =
            outer_geometry(xlib, display, id).ok_or("Cannot save the X11 window position")?;
        Ok(WindowedState {
            hints: if found { hints } else { fallback },
            width: size.0 as u32,
            height: size.1 as u32,
            position: (geometry.0, geometry.1),
            monitor_origin: monitor_origin(xlib, display, screen, geometry),
        })
    }

    fn write_hints(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
        mut hints: xlib::XSizeHints,
    ) {
        unsafe {
            (xlib.XSetWMNormalHints)(display, id, &mut hints);
            (xlib.XFlush)(display);
        }
    }

    fn restore_windowed_state(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
        saved: WindowedState,
        position: Option<(isize, isize)>,
    ) {
        // Reinstate the normal constraints before requesting the old client
        // size. Some WMs confirm EWMH before their ConfigureNotify arrives.
        unsafe {
            let mut hints = saved.hints;
            (xlib.XSetWMNormalHints)(display, id, &mut hints);
            (xlib.XResizeWindow)(display, id, saved.width, saved.height);
            (xlib.XFlush)(display);
        }
        let _ = move_window(xlib, display, id, position.unwrap_or(saved.position));
    }

    fn atoms_property(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        window: xlib::Window,
        property: xlib::Atom,
    ) -> Result<Vec<xlib::Atom>, String> {
        let mut actual_type: xlib::Atom = 0;
        let mut format = 0;
        let mut count: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut data: *mut c_uchar = ptr::null_mut();
        let status = unsafe {
            (xlib.XGetWindowProperty)(
                display,
                window,
                property,
                0,
                4096,
                xlib::False,
                xlib::XA_ATOM,
                &mut actual_type,
                &mut format,
                &mut count,
                &mut bytes_after,
                &mut data,
            )
        };
        let result = if status != xlib::Success as i32 {
            Err(format!("XGetWindowProperty failed with status {status}"))
        } else if actual_type == 0 {
            Ok(Vec::new())
        } else if actual_type != xlib::XA_ATOM || format != 32 || bytes_after != 0 {
            Err("Unexpected or oversized EWMH atom property".to_owned())
        } else if data.is_null() || count == 0 {
            Ok(Vec::new())
        } else {
            // Xlib represents format-32 items as native unsigned longs, also
            // on 64-bit hosts. The property storage must be released by XFree.
            Ok(unsafe {
                std::slice::from_raw_parts(data.cast::<c_ulong>(), count as usize).to_vec()
            })
        };
        if !data.is_null() {
            unsafe { (xlib.XFree)(data.cast()) };
        }
        result
    }

    fn managed_window_is_ready(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
        wm_state: xlib::Atom,
    ) -> Result<bool, String> {
        let mut actual_type = 0;
        let mut format = 0;
        let mut count = 0;
        let mut bytes_after = 0;
        let mut data: *mut c_uchar = ptr::null_mut();
        let status = unsafe {
            (xlib.XGetWindowProperty)(
                display,
                id,
                wm_state,
                0,
                1,
                xlib::False,
                wm_state,
                &mut actual_type,
                &mut format,
                &mut count,
                &mut bytes_after,
                &mut data,
            )
        };
        // ICCCM WM_STATE has a state and an icon window. Only the first
        // format-32 item is needed; Xlib exposes it as a native unsigned long.
        let state = if status != xlib::Success as i32 {
            Err(format!("Cannot read the X11 window's WM_STATE: {status}"))
        } else if actual_type == 0 {
            Ok(None)
        } else if actual_type != wm_state || format != 32 || count != 1 || data.is_null() {
            Err("Unexpected X11 WM_STATE property".into())
        } else {
            Ok(Some(unsafe { *data.cast::<c_ulong>() }))
        };
        if !data.is_null() {
            unsafe { (xlib.XFree)(data.cast()) };
        }
        match state? {
            // An existing minimized window is already managed. Do not wait
            // for it to become visible or change the user's minimized state.
            Some(ICONIC_STATE) => Ok(true),
            Some(NORMAL_STATE) => {
                let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
                if unsafe { (xlib.XGetWindowAttributes)(display, id, &mut attrs) } == 0 {
                    return Err("Cannot verify the X11 window's mapping".into());
                }
                Ok(attrs.map_state == xlib::IsViewable)
            }
            _ => Ok(false),
        }
    }

    fn wait_until_managed(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
    ) -> Result<(), String> {
        let wm_state = unsafe { (xlib.XInternAtom)(display, c"WM_STATE".as_ptr(), xlib::False) };
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        loop {
            if managed_window_is_ready(xlib, display, id, wm_state)? {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(
                    "The X11 window manager did not manage the window within 500 ms".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn request(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        root: xlib::Window,
        window: xlib::Window,
        state: xlib::Atom,
        fullscreen: xlib::Atom,
        enabled: bool,
    ) -> Result<(), String> {
        let mut message: xlib::XClientMessageEvent = unsafe { std::mem::zeroed() };
        message.type_ = xlib::ClientMessage;
        message.display = display;
        message.window = window;
        message.message_type = state;
        message.format = 32;
        message.data.set_long(0, if enabled { 1 } else { 0 });
        message.data.set_long(1, fullscreen as c_long);
        message.data.set_long(2, 0);
        message.data.set_long(3, 1); // EWMH source: ordinary application.
        let mut event = xlib::XEvent::from(message);
        let sent = unsafe {
            (xlib.XSendEvent)(
                display,
                root,
                xlib::False,
                xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask,
                &mut event,
            )
        };
        if sent == 0 {
            return Err("The X11 fullscreen request could not be sent".to_owned());
        }
        unsafe { (xlib.XFlush)(display) };
        Ok(())
    }

    fn wait_for_state(
        xlib: &xlib::Xlib,
        display: *mut xlib::Display,
        id: xlib::Window,
        state: xlib::Atom,
        fullscreen: xlib::Atom,
        enabled: bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + TRANSITION_TIMEOUT;
        loop {
            if atoms_property(xlib, display, id, state)?.contains(&fullscreen) == enabled {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(
                    "The X11 window manager did not confirm fullscreen within 500 ms".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    impl Controller {
        pub(super) fn release_emulation(&mut self, window: &minifb::Window) -> Result<(), String> {
            if self.emulation.expected().is_none() {
                return Ok(());
            }
            // minifb keeps its Display alive until Window::drop even after a
            // WM_DELETE_WINDOW message has made is_open() false. Borrow it here;
            // never retain a raw Display pointer in our controller.
            let display = window.display_handle().map_err(|error| error.to_string())?;
            let RawDisplayHandle::Xlib(handle) = display.as_raw() else {
                return Err("Cannot restore emulation without minifb's X11 display".into());
            };
            let display = handle
                .display
                .ok_or("The owning X11 display is unavailable")?
                .as_ptr()
                .cast::<xlib::Display>();
            let xlib = xlib::Xlib::open().map_err(|error| error.to_string())?;
            let root = unsafe { (xlib.XRootWindow)(display, handle.screen) };
            self.emulation.restore(display, root)
        }

        fn verify_emulated_size(
            &self,
            xlib: &xlib::Xlib,
            display: *mut xlib::Display,
            id: xlib::Window,
        ) -> Result<bool, String> {
            let Some(expected) = self.emulation.expected() else {
                return Ok(true);
            };
            let deadline = Instant::now() + TRANSITION_TIMEOUT;
            loop {
                let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
                if unsafe { (xlib.XGetWindowAttributes)(display, id, &mut attrs) } == 0 {
                    return Err("Cannot verify the emulated fullscreen drawable".into());
                }
                let observed = (attrs.width.max(0) as u32, attrs.height.max(0) as u32);
                if observed == (expected.width, expected.height) {
                    return Ok(true);
                }
                if Instant::now() >= deadline {
                    return Ok(false);
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        pub(super) fn is_fullscreen(&self) -> bool {
            self.active
        }

        pub(super) fn windowed_position(&self, window: &minifb::Window) -> Option<(isize, isize)> {
            let (display, screen, id) = native_window(window)?;
            let xlib = xlib::Xlib::open().ok()?;
            let geometry = outer_geometry(&xlib, display, id)?;
            let current = (geometry.0, geometry.1);
            let Some(saved) = self.saved else {
                return Some(current);
            };
            let state =
                unsafe { (xlib.XInternAtom)(display, c"_NET_WM_STATE".as_ptr(), xlib::True) };
            let fullscreen = unsafe {
                (xlib.XInternAtom)(display, c"_NET_WM_STATE_FULLSCREEN".as_ptr(), xlib::True)
            };
            if state == 0
                || fullscreen == 0
                || !atoms_property(&xlib, display, id, state)
                    .ok()?
                    .contains(&fullscreen)
            {
                // The WM may have left fullscreen independently; its normal
                // position is authoritative in that case, not a monitor delta.
                return Some(current);
            }
            Some(
                match (
                    saved.monitor_origin,
                    monitor_origin(&xlib, display, screen, geometry),
                ) {
                    (Some(old), Some(new)) => (
                        saved.position.0 + new.0 - old.0,
                        saved.position.1 + new.1 - old.1,
                    ),
                    _ => saved.position,
                },
            )
        }

        pub(super) fn set_position(
            window: &minifb::Window,
            position: (isize, isize),
        ) -> Result<(), String> {
            let (display, _, id) =
                native_window(window).ok_or("Cannot position a closed or non-X11 window")?;
            let xlib = xlib::Xlib::open().map_err(|error| error.to_string())?;
            move_window(&xlib, display, id, position)
        }

        pub(super) fn set(&mut self, window: &minifb::Window, enabled: bool) -> Result<(), String> {
            if !window.is_open() {
                self.release_emulation(window)?;
                self.active = false;
                self.saved = None;
                return if enabled {
                    Err("Cannot enter fullscreen after the graphics window was closed".into())
                } else {
                    Ok(())
                };
            }
            let display_handle = window.display_handle().map_err(|error| error.to_string())?;
            let window_handle = window.window_handle().map_err(|error| error.to_string())?;
            let (display, screen) = match display_handle.as_raw() {
                RawDisplayHandle::Xlib(handle) => (
                    handle
                        .display
                        .ok_or("minifb did not provide an X11 display")?
                        .as_ptr()
                        .cast::<xlib::Display>(),
                    handle.screen,
                ),
                _ => return Err("Fullscreen requires minifb's X11 backend".into()),
            };
            let id = match window_handle.as_raw() {
                RawWindowHandle::Xlib(handle) => handle.window,
                _ => return Err("minifb did not provide an X11 window".into()),
            };
            let xlib = xlib::Xlib::open().map_err(|error| error.to_string())?;
            let root = unsafe { (xlib.XRootWindow)(display, screen) };
            let supported =
                unsafe { (xlib.XInternAtom)(display, c"_NET_SUPPORTED".as_ptr(), xlib::True) };
            let state =
                unsafe { (xlib.XInternAtom)(display, c"_NET_WM_STATE".as_ptr(), xlib::True) };
            let fullscreen = unsafe {
                (xlib.XInternAtom)(display, c"_NET_WM_STATE_FULLSCREEN".as_ptr(), xlib::True)
            };
            if supported == 0 || state == 0 || fullscreen == 0 {
                return Err("The X11 window manager does not advertise EWMH fullscreen".into());
            }
            let supported_atoms = atoms_property(&xlib, display, root, supported)?;
            if !supported_atoms.contains(&state) || !supported_atoms.contains(&fullscreen) {
                return Err("The X11 window manager does not support EWMH fullscreen".into());
            }
            if enabled {
                // minifb maps asynchronously. A new MODE window may exist
                // before the WM manages it; its placement and fullscreen
                // requests can otherwise race the WM's initial setup. Root
                // EWMH support alone does not acknowledge this client window.
                wait_until_managed(&xlib, display, id)?;
            }
            let previous = atoms_property(&xlib, display, id, state)?.contains(&fullscreen);
            // Keep the snapshot unchanged until OFF succeeds: after a failed
            // transition and rollback, retries must not add the monitor delta
            // twice. Only the eventual restore uses this translated position.
            let restore_position = (!enabled).then(|| self.windowed_position(window)).flatten();
            self.active = previous;
            // All fullscreen entry points, including Alt+Enter, use this same
            // automatic surface preference. Unsupported servers stay at desktop size.
            let emulation_request = enabled.then_some(emulation::Request::HD);
            let confirm_surface = |controller: &mut Self| -> Result<(), String> {
                if !controller.verify_emulated_size(&xlib, display, id)? {
                    // Some XWayland WMs advertise and accept RandR modes but
                    // keep fullscreen at desktop size. Revoke the optional
                    // mode first, then confirm ordinary fullscreen. Protocol
                    // and restoration failures still use the normal error path.
                    controller.emulation.restore(display, root)?;
                    request(&xlib, display, root, id, state, fullscreen, true)?;
                    wait_for_state(&xlib, display, id, state, fullscreen, true)?;
                }
                Ok(())
            };
            if previous == enabled {
                // The WM may already have left fullscreen independently. Do
                // not leave the normal window with its size limits removed.
                if !enabled {
                    self.emulation.restore(display, root)?;
                    if let Some(saved) = self.saved.take() {
                        restore_windowed_state(&xlib, display, id, saved, restore_position);
                    }
                } else {
                    confirm_surface(self)?;
                }
                return Ok(());
            }

            if enabled {
                let saved = match self.saved {
                    Some(saved) => saved,
                    None => save_windowed_state(&xlib, display, screen, id, window.get_size())?,
                };
                self.saved = Some(saved);
                write_hints(&xlib, display, id, fullscreen_hints(saved.hints));
            }

            let transition = (|| {
                if let Some(mode) = emulation_request {
                    let geometry = outer_geometry(&xlib, display, id)
                        .ok_or("Cannot locate the window for fullscreen emulation")?;
                    self.emulation.enable(display, root, geometry, mode)?;
                }
                request(&xlib, display, root, id, state, fullscreen, enabled)?;
                wait_for_state(&xlib, display, id, state, fullscreen, enabled)?;
                if enabled {
                    confirm_surface(self)?;
                }
                Ok::<(), String>(())
            })();
            match transition {
                Ok(()) => {
                    self.active = enabled;
                    if !enabled {
                        let restored = self.emulation.restore(display, root);
                        if let Some(saved) = self.saved.take() {
                            restore_windowed_state(&xlib, display, id, saved, restore_position);
                        }
                        restored?;
                    }
                    Ok(())
                }
                Err(error) => {
                    // Revoke a refused/delayed request before restoring normal
                    // limits. If rollback cannot be confirmed, keep the saved
                    // state while fullscreen so a later exit/Drop can retry.
                    let rollback = request(&xlib, display, root, id, state, fullscreen, previous)
                        .and_then(|_| {
                            wait_for_state(&xlib, display, id, state, fullscreen, previous)
                        });
                    self.active = atoms_property(&xlib, display, id, state)
                        .map(|atoms| atoms.contains(&fullscreen))
                        .unwrap_or(previous);
                    // Entry failures must revoke a mode request even when the
                    // WM refuses its fullscreen rollback. The physical desktop
                    // was never changed; any failed restoration remains saved
                    // for OFF/close/Drop to retry on this same live connection.
                    let emulation_restore = if enabled || !self.active {
                        self.emulation.restore(display, root)
                    } else {
                        Ok(())
                    };
                    if !self.active {
                        if let Some(saved) = self.saved.take() {
                            restore_windowed_state(&xlib, display, id, saved, restore_position);
                        }
                    }
                    let error = match emulation_restore {
                        Ok(()) => error,
                        Err(restore_error) => {
                            format!("{error}; emulation restoration: {restore_error}")
                        }
                    };
                    match rollback {
                        Ok(()) => Err(error),
                        Err(rollback_error) => Err(format!("{error}; rollback: {rollback_error}")),
                    }
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fullscreen_relaxes_only_the_normal_size_limits() {
            let mut original = fixed_size_hints((960, 540)).unwrap();
            original.flags |= xlib::PPosition | xlib::PAspect | xlib::PResizeInc;
            original.x = 17;
            original.y = 31;
            original.width_inc = 8;
            original.height_inc = 16;
            original.min_aspect.x = 4;
            original.min_aspect.y = 3;
            let relaxed = fullscreen_hints(original);
            assert_eq!(
                relaxed.flags,
                xlib::PPosition | xlib::PAspect | xlib::PResizeInc
            );
            let mut restored = relaxed;
            restored.flags = original.flags;
            assert_eq!(restored, original);
        }

        #[test]
        fn absent_normal_hints_fall_back_to_a_nonempty_fixed_client_size() {
            let hints = fixed_size_hints((800, 600)).unwrap();
            assert_eq!(hints.flags, xlib::PMinSize | xlib::PMaxSize);
            assert_eq!((hints.min_width, hints.max_width), (800, 800));
            assert_eq!((hints.min_height, hints.max_height), (600, 600));
            assert!(fixed_size_hints((0, 600)).is_err());
            assert!(fixed_size_hints((800, 0)).is_err());
            assert!(fixed_size_hints((i32::MAX as usize + 1, 600)).is_err());
        }

        #[test]
        #[ignore = "opens native X11 windows; run explicitly with an EWMH window manager"]
        fn native_immediate_fullscreen_waits_for_each_fresh_window_to_be_managed() {
            struct RestoringWindow {
                window: minifb::Window,
                controller: Controller,
            }

            impl Drop for RestoringWindow {
                fn drop(&mut self) {
                    let _ = self.controller.set(&self.window, false);
                }
            }

            let xlib = xlib::Xlib::open().unwrap();
            // Recreate promptly at each MODE size, without a preliminary
            // update/present or delay that could hide asynchronous WM setup.
            for cycle in 0..4 {
                for logical in [(640, 360), (960, 540), (1280, 720)] {
                    let window = minifb::Window::new(
                        "AVL immediate fullscreen management verification",
                        logical.0,
                        logical.1,
                        minifb::WindowOptions {
                            resize: false,
                            scale_mode: minifb::ScaleMode::AspectRatioStretch,
                            ..minifb::WindowOptions::default()
                        },
                    )
                    .expect("X11 test window must open");
                    let mut owned = RestoringWindow {
                        window,
                        controller: Controller::default(),
                    };
                    owned.window.set_target_fps(0);
                    owned
                        .controller
                        .set(&owned.window, true)
                        .unwrap_or_else(|error| panic!("cycle {cycle}, MODE {logical:?}: {error}"));
                    assert!(owned.controller.is_fullscreen());
                    let saved = owned.controller.saved.unwrap();
                    assert_eq!((saved.width as usize, saved.height as usize), logical);
                    let (display, _, id) = native_window(&owned.window).unwrap();
                    let state = unsafe {
                        (xlib.XInternAtom)(display, c"_NET_WM_STATE".as_ptr(), xlib::True)
                    };
                    let fullscreen = unsafe {
                        (xlib.XInternAtom)(
                            display,
                            c"_NET_WM_STATE_FULLSCREEN".as_ptr(),
                            xlib::True,
                        )
                    };
                    assert!(atoms_property(&xlib, display, id, state)
                        .unwrap()
                        .contains(&fullscreen));
                    owned.controller.set(&owned.window, false).unwrap();
                    assert!(!owned.controller.is_fullscreen());
                    assert!(owned.controller.saved.is_none());
                    let mut restored: xlib::XSizeHints = unsafe { std::mem::zeroed() };
                    let mut supplied = 0;
                    assert_ne!(
                        unsafe {
                            (xlib.XGetWMNormalHints)(display, id, &mut restored, &mut supplied)
                        },
                        0
                    );
                    assert_eq!(restored, saved.hints);
                }
            }
        }

        #[test]
        #[ignore = "opens a native X11 window; run explicitly on the single-monitor WSLg test display"]
        fn native_fixed_window_releases_and_restores_size_hints_for_fullscreen() {
            struct RestoringWindow {
                window: minifb::Window,
                controller: Controller,
            }

            impl Drop for RestoringWindow {
                fn drop(&mut self) {
                    let _ = self.controller.set(&self.window, false);
                }
            }

            fn read_hints(
                xlib: &xlib::Xlib,
                display: *mut xlib::Display,
                id: xlib::Window,
            ) -> xlib::XSizeHints {
                let mut hints: xlib::XSizeHints = unsafe { std::mem::zeroed() };
                let mut supplied = 0;
                assert_ne!(
                    unsafe { (xlib.XGetWMNormalHints)(display, id, &mut hints, &mut supplied) },
                    0,
                    "the native window must expose WM_NORMAL_HINTS"
                );
                hints
            }

            fn native_size(
                xlib: &xlib::Xlib,
                display: *mut xlib::Display,
                id: xlib::Window,
            ) -> (usize, usize) {
                let (mut root, mut x, mut y) = (0, 0, 0);
                let (mut width, mut height, mut border, mut depth) = (0, 0, 0, 0);
                assert_ne!(
                    unsafe {
                        (xlib.XGetGeometry)(
                            display,
                            id,
                            &mut root,
                            &mut x,
                            &mut y,
                            &mut width,
                            &mut height,
                            &mut border,
                            &mut depth,
                        )
                    },
                    0
                );
                (width as usize, height as usize)
            }

            fn present_until_size(
                window: &mut minifb::Window,
                pixels: &[u32],
                logical: (usize, usize),
                expected: (usize, usize),
            ) {
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    assert!(window.is_open(), "native test window was closed");
                    window
                        .update_with_buffer(pixels, logical.0, logical.1)
                        .expect("presenting the fixed logical framebuffer must succeed");
                    if window.get_size() == expected {
                        return;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "window size {:?} did not reach {expected:?}",
                        window.get_size()
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
            }

            let logical = (640, 480);
            let pixels = vec![0x2060a0; logical.0 * logical.1];
            let window = minifb::Window::new(
                "AVL fixed-window fullscreen hints verification",
                logical.0,
                logical.1,
                minifb::WindowOptions {
                    resize: false,
                    scale_mode: minifb::ScaleMode::AspectRatioStretch,
                    ..minifb::WindowOptions::default()
                },
            )
            .expect("X11 test window must open");
            let mut owned = RestoringWindow {
                window,
                controller: Controller::default(),
            };
            owned.window.set_target_fps(0);
            present_until_size(&mut owned.window, &pixels, logical, logical);

            let (display, screen, id) = {
                let display = owned.window.display_handle().unwrap();
                let handle = owned.window.window_handle().unwrap();
                let RawDisplayHandle::Xlib(display) = display.as_raw() else {
                    panic!("the native test requires X11");
                };
                let RawWindowHandle::Xlib(handle) = handle.as_raw() else {
                    panic!("the native test requires an X11 window");
                };
                (
                    display.display.unwrap().as_ptr().cast::<xlib::Display>(),
                    display.screen,
                    handle.window,
                )
            };
            let xlib = xlib::Xlib::open().unwrap();
            let root = unsafe { (xlib.XRootWindow)(display, screen) };
            let desktop_size = native_size(&xlib, display, root);
            assert!(desktop_size.0 > logical.0 && desktop_size.1 > logical.1);
            let original = read_hints(&xlib, display, id);
            let size_limits = xlib::PMinSize | xlib::PMaxSize;
            assert_eq!(original.flags & size_limits, size_limits);
            assert_eq!((original.min_width, original.max_width), (640, 640));
            assert_eq!((original.min_height, original.max_height), (480, 480));

            for cycle in 1..=2 {
                assert_eq!(read_hints(&xlib, display, id), original);
                owned.controller.set(&owned.window, true).unwrap();
                let fullscreen_size = owned
                    .controller
                    .emulation
                    .expected()
                    .map_or(desktop_size, |mode| {
                        (mode.width as usize, mode.height as usize)
                    });
                // Repeating ON must preserve the first normal-window snapshot,
                // including after an advertised mode fell back to desktop size.
                owned.controller.set(&owned.window, true).unwrap();
                present_until_size(&mut owned.window, &pixels, logical, fullscreen_size);
                assert!(owned.controller.is_fullscreen());
                let expanded = read_hints(&xlib, display, id);
                assert_eq!(expanded.flags & size_limits, 0);
                // XGetWMNormalHints may zero fields whose flags are absent.
                // Their values impose no constraints while fullscreen; the
                // saved, active min/max values are checked intact on exit.
                assert_eq!(expanded.flags, fullscreen_hints(original).flags);
                assert_eq!(native_size(&xlib, display, id), fullscreen_size);

                owned.controller.set(&owned.window, false).unwrap();
                present_until_size(&mut owned.window, &pixels, logical, logical);
                assert!(!owned.controller.is_fullscreen());
                assert_eq!(read_hints(&xlib, display, id), original);
                assert_eq!(native_size(&xlib, display, id), logical);
                assert!(owned.controller.saved.is_none());
                println!(
                    "X11 fixed-window cycle {cycle}: {logical:?} -> {fullscreen_size:?} -> {logical:?}; normal hints restored"
                );
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    #[derive(Default)]
    pub(super) struct Controller;

    impl Controller {
        pub(super) fn windowed_position(&self, window: &minifb::Window) -> Option<(isize, isize)> {
            window.is_open().then(|| window.get_position())
        }

        pub(super) fn set_position(
            _window: &minifb::Window,
            _position: (isize, isize),
        ) -> Result<(), String> {
            Ok(())
        }

        pub(super) fn is_fullscreen(&self) -> bool {
            false
        }

        pub(super) fn set(
            &mut self,
            _window: &minifb::Window,
            enabled: bool,
        ) -> Result<(), String> {
            if enabled {
                Err("Fullscreen is supported on Windows and Linux/X11".into())
            } else {
                Ok(())
            }
        }
    }
}
