//! Conservative native monitor check for a resolved console host.

use std::ffi::c_void;
use std::mem::size_of;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    fn valid(self) -> bool {
        self.left < self.right && self.top < self.bottom
    }

    fn overlaps(self, other: Self) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }
}

#[repr(C)]
#[derive(Default)]
struct MonitorInfo {
    size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
}

#[link(name = "user32")]
extern "system" {
    fn IsWindowVisible(window: *mut c_void) -> i32;
    fn IsIconic(window: *mut c_void) -> i32;
    fn GetWindowRect(window: *mut c_void, rect: *mut Rect) -> i32;
    fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
    fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmGetWindowAttribute(
        window: *mut c_void,
        attribute: u32,
        value: *mut u32,
        size: u32,
    ) -> i32;
}

/// Unknown, hidden, minimized, overlapping and offscreen hosts all fall back
/// to windowed graphics. Never infer a monitor from a pseudoconsole helper.
pub(super) fn console_is_separate(graphics: *mut c_void, terminal: *mut c_void) -> bool {
    if graphics.is_null() || terminal.is_null() || graphics == terminal {
        return false;
    }
    unsafe {
        if IsWindowVisible(terminal) == 0 || IsIconic(terminal) != 0 {
            return false;
        }
        let mut cloaked = 0;
        // DWMWA_CLOAKED: e.g. a terminal on another virtual desktop is not
        // evidence of an accessible terminal on a second physical monitor.
        if DwmGetWindowAttribute(terminal, 14, &mut cloaked, size_of::<u32>() as u32) < 0
            || cloaked != 0
        {
            return false;
        }
        // DEFAULTTONULL avoids treating an entirely offscreen window as safe.
        let graphics_monitor = MonitorFromWindow(graphics, 0);
        let terminal_monitor = MonitorFromWindow(terminal, 0);
        if graphics_monitor.is_null()
            || terminal_monitor.is_null()
            || graphics_monitor == terminal_monitor
        {
            return false;
        }
        let mut info = MonitorInfo {
            size: size_of::<MonitorInfo>() as u32,
            ..Default::default()
        };
        let mut rect = Rect::default();
        if GetMonitorInfoW(graphics_monitor, &mut info) == 0
            || GetWindowRect(terminal, &mut rect) == 0
        {
            return false;
        }
        // MonitorFromWindow reports the largest intersection. A terminal
        // straddling both monitors may still be covered by fullscreen graphics.
        rect.valid() && info.monitor.valid() && !rect.overlaps(info.monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_includes_straddling_but_not_touching_edges() {
        let main = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(main.overlaps(Rect {
            left: 1800,
            top: 50,
            right: 2600,
            bottom: 800
        }));
        assert!(!main.overlaps(Rect {
            left: 1920,
            top: 50,
            right: 2600,
            bottom: 800
        }));
        assert!(!main.overlaps(Rect {
            left: -1280,
            top: 0,
            right: 0,
            bottom: 720
        }));
        assert!(!main.overlaps(Rect {
            left: 0,
            top: -1080,
            right: 1920,
            bottom: 0
        }));
        assert!(main.overlaps(main));
        assert!(!Rect::default().valid());
    }

    #[test]
    fn unresolved_handles_never_keep_fullscreen() {
        assert!(!console_is_separate(
            std::ptr::null_mut(),
            std::ptr::null_mut()
        ));
    }
}
