//! Best-effort X11 terminal identification. Unknown ownership or topology
//! suspends fullscreen so the console remains accessible. The active window
//! is never guessed to be the terminal.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::ffi::{c_int, c_long, c_ulong, CStr};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use x11_dl::{xlib, xrandr};

// Opt-in diagnostics for desktop/terminal combinations we cannot reproduce.
// Log only window metadata and decisions, never terminal text or environment
// contents. A closed diagnostic pipe must not interrupt the BASIC program.
fn trace(message: std::fmt::Arguments<'_>) {
    if std::env::var_os("AVL_BASIC_CONSOLE_TRACE").is_some_and(|value| value == "1") {
        use std::io::Write;
        let _ = writeln!(std::io::stderr().lock(), "[console-x11] {message}");
    }
}

type ErrorHandler = unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XErrorEvent) -> c_int;
static PROBE_LOCK: Mutex<()> = Mutex::new(());
static PROBE_DISPLAY: AtomicPtr<xlib::Display> = AtomicPtr::new(ptr::null_mut());
static PROBE_FAILED: AtomicBool = AtomicBool::new(false);
static PREVIOUS_HANDLER: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn probe_error(
    display: *mut xlib::Display,
    event: *mut xlib::XErrorEvent,
) -> c_int {
    if display == PROBE_DISPLAY.load(Ordering::SeqCst) {
        PROBE_FAILED.store(true, Ordering::SeqCst);
        return 0;
    }
    let previous = PREVIOUS_HANDLER.load(Ordering::SeqCst);
    if previous != 0 {
        let handler: ErrorHandler = std::mem::transmute(previous);
        return handler(display, event);
    }
    0
}

struct Probe<'a> {
    x: &'a xlib::Xlib,
    display: *mut xlib::Display,
    previous: Option<ErrorHandler>,
}

impl Drop for Probe<'_> {
    fn drop(&mut self) {
        unsafe {
            // Drain errors before restoring the process-wide Xlib handler. All
            // our requests use this private connection, never minifb's queue.
            (self.x.XSync)(self.display, xlib::False);
            (self.x.XCloseDisplay)(self.display);
            (self.x.XSetErrorHandler)(self.previous);
        }
        PROBE_DISPLAY.store(ptr::null_mut(), Ordering::SeqCst);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

impl Rect {
    fn contains(self, other: Self) -> bool {
        self.width > 0
            && self.height > 0
            && other.width > 0
            && other.height > 0
            && other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }

    fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
}

fn graphics_monitor(graphics: Rect, monitors: &[Rect], fullscreen: bool) -> Option<Rect> {
    if graphics.width <= 0 || graphics.height <= 0 {
        return None;
    }
    let index = if fullscreen {
        let mut matches = monitors
            .iter()
            .enumerate()
            .filter(|(_, monitor)| monitor.contains(graphics));
        let (index, _) = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        index
    } else {
        // Normal window decorations may extend beyond a monitor, especially
        // when the logical canvas already matches its resolution. Use a unique
        // greatest intersection as the likely destination, then inspect the
        // actual fullscreen geometry after the WM has applied the request.
        let mut greatest = 0i128;
        let mut index = None;
        for (candidate, monitor) in monitors.iter().enumerate() {
            if monitor.width <= 0 || monitor.height <= 0 {
                continue;
            }
            let width = ((graphics.x + graphics.width).min(monitor.x + monitor.width)
                - graphics.x.max(monitor.x))
            .max(0) as i128;
            let height = ((graphics.y + graphics.height).min(monitor.y + monitor.height)
                - graphics.y.max(monitor.y))
            .max(0) as i128;
            let area = width * height;
            if area > greatest {
                greatest = area;
                index = Some(candidate);
            } else if area == greatest && area != 0 {
                index = None;
            }
        }
        index?
    };
    let selected = monitors[index];
    if monitors.iter().enumerate().any(|(other, monitor)| {
        other != index && monitor.width > 0 && monitor.height > 0 && selected.overlaps(*monitor)
    }) {
        return None;
    }
    Some(selected)
}

fn separate_monitors(
    graphics: Rect,
    terminal: Rect,
    monitors: &[Rect],
    fullscreen: bool,
) -> Option<bool> {
    let graphics_monitor = graphics_monitor(graphics, monitors, fullscreen)?;
    if terminal.width <= 0 || terminal.height <= 0 {
        return None;
    }
    if graphics_monitor.overlaps(terminal) {
        return Some(false);
    }
    // The terminal may extend beyond the desktop, or span two non-graphics
    // monitors. What matters is that some of it is visible and none is covered
    // by the graphics monitor, rather than full containment in a single one.
    monitors
        .iter()
        .any(|monitor| {
            monitor.width > 0
                && monitor.height > 0
                && !graphics_monitor.overlaps(*monitor)
                && monitor.overlaps(terminal)
        })
        .then_some(true)
}

fn parent_pid(stat: &str) -> Option<u32> {
    // /proc's comm field may itself contain spaces and parentheses.
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn ancestors() -> Vec<u32> {
    let mut result = Vec::new();
    let mut pid = std::process::id();
    for _ in 0..128 {
        let Some(parent) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| parent_pid(&stat))
        else {
            break;
        };
        if parent <= 1 || result.contains(&parent) {
            break;
        }
        result.push(parent);
        pid = parent;
    }
    result
}

fn parse_window_id(value: &str) -> Option<xlib::Window> {
    let value = value.trim();
    let parsed = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        c_ulong::from_str_radix(hex, 16).ok()?
    } else {
        value.parse().ok()?
    };
    (parsed != 0).then_some(parsed)
}

fn choose_terminal(
    explicit: Option<xlib::Window>,
    candidates: &[xlib::Window],
) -> Option<xlib::Window> {
    match explicit {
        Some(id) => candidates.contains(&id).then_some(id),
        None if candidates.len() == 1 => candidates.first().copied(),
        _ => None,
    }
}

fn desktop_is_current(desktop: Option<c_ulong>, current: Option<c_ulong>) -> bool {
    match (desktop, current) {
        // A sticky window is on the current desktop even when XWayland does
        // not publish the compositor's current workspace on its root window.
        (Some(desktop), _) if desktop == u32::MAX as c_ulong => true,
        (Some(desktop), Some(current)) => desktop == current,
        _ => false,
    }
}

impl Probe<'_> {
    fn atom(&self, name: &CStr) -> Option<xlib::Atom> {
        let atom = unsafe { (self.x.XInternAtom)(self.display, name.as_ptr(), xlib::True) };
        (atom != 0).then_some(atom)
    }

    fn property(
        &self,
        id: xlib::Window,
        name: &CStr,
        kind: xlib::Atom,
        format: i32,
    ) -> Option<Vec<u8>> {
        let atom = self.atom(name)?;
        let (mut actual_type, mut actual_format, mut count, mut remaining) = (0, 0, 0, 0);
        let mut data = ptr::null_mut();
        let status = unsafe {
            (self.x.XGetWindowProperty)(
                self.display,
                id,
                atom,
                0,
                4096,
                xlib::False,
                kind,
                &mut actual_type,
                &mut actual_format,
                &mut count,
                &mut remaining,
                &mut data,
            )
        };
        let valid = status == xlib::Success as i32
            && actual_type == kind
            && actual_format == format
            && remaining == 0
            && !data.is_null();
        let length = if format == 32 {
            count as usize * std::mem::size_of::<c_ulong>()
        } else {
            count as usize
        };
        let result = valid.then(|| unsafe { std::slice::from_raw_parts(data, length).to_vec() });
        if !data.is_null() {
            unsafe { (self.x.XFree)(data.cast()) };
        }
        result
    }

    fn numbers(&self, id: xlib::Window, name: &CStr, kind: xlib::Atom) -> Option<Vec<c_ulong>> {
        let bytes = self.property(id, name, kind, 32)?;
        Some(
            bytes
                .chunks_exact(std::mem::size_of::<c_ulong>())
                // Xlib stores protocol CARDINAL/32 values in native longs.
                // On LP64 it can sign-extend 0xffffffff to ULONG_MAX; only
                // the low 32 bits belong to the property on the wire.
                .map(|chunk| {
                    c_ulong::from_ne_bytes(chunk.try_into().unwrap()) & u32::MAX as c_ulong
                })
                .collect(),
        )
    }

    fn number(&self, id: xlib::Window, name: &CStr) -> Option<c_ulong> {
        let values = self.numbers(id, name, xlib::XA_CARDINAL)?;
        (values.len() == 1).then(|| values[0])
    }

    fn local_ancestor(&self, id: xlib::Window, ancestors: &[u32], hostname: &[u8]) -> bool {
        let Some(pid) = self
            .number(id, c"_NET_WM_PID")
            .and_then(|pid| u32::try_from(pid).ok())
        else {
            trace(format_args!(
                "client={id:#x}: missing or invalid _NET_WM_PID"
            ));
            return false;
        };
        if !ancestors.contains(&pid) {
            trace(format_args!("client={id:#x}: pid={pid} is not an ancestor"));
            return false;
        }
        let Some(mut machine) = self.property(id, c"WM_CLIENT_MACHINE", xlib::XA_STRING, 8) else {
            trace(format_args!(
                "client={id:#x}: ancestor pid={pid}, missing WM_CLIENT_MACHINE"
            ));
            return false;
        };
        while machine.last() == Some(&0) {
            machine.pop();
        }
        // Matching local PID numbers alone is unsafe with forwarded X11.
        let local = machine == hostname;
        trace(format_args!(
            "client={id:#x}: ancestor pid={pid}, local_machine={local}"
        ));
        local
    }

    fn visible_rect(&self, id: xlib::Window, root: xlib::Window) -> Option<Rect> {
        let mut attrs: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
        if unsafe { (self.x.XGetWindowAttributes)(self.display, id, &mut attrs) } == 0
            || attrs.map_state != xlib::IsViewable
            || attrs.root != root
        {
            trace(format_args!(
                "window={id:#x}: attributes unavailable, unmapped or wrong root"
            ));
            return None;
        }
        if let (Some(hidden), Some(states)) = (
            self.atom(c"_NET_WM_STATE_HIDDEN"),
            self.numbers(id, c"_NET_WM_STATE", xlib::XA_ATOM),
        ) {
            if states.contains(&hidden) {
                trace(format_args!("window={id:#x}: hidden"));
                return None;
            }
        }
        let (mut x, mut y, mut child) = (0, 0, 0);
        if unsafe {
            (self.x.XTranslateCoordinates)(self.display, id, root, 0, 0, &mut x, &mut y, &mut child)
        } == 0
        {
            return None;
        }
        let mut rect = Rect {
            x: x as i64,
            y: y as i64,
            width: attrs.width as i64,
            height: attrs.height as i64,
        };
        if let Some(extents) = self.numbers(id, c"_NET_FRAME_EXTENTS", xlib::XA_CARDINAL) {
            trace(format_args!(
                "window={id:#x}: client_rect={rect:?}, frame_extents={extents:?}"
            ));
            if extents.len() != 4 || extents.iter().any(|value| *value > 100_000) {
                return None;
            }
            rect.x -= extents[0] as i64;
            rect.y -= extents[2] as i64;
            rect.width += (extents[0] + extents[1]) as i64;
            rect.height += (extents[2] + extents[3]) as i64;
        } else {
            // If the WM cannot describe decorations, inspect its outer frame.
            let mut current = id;
            let mut found_root = false;
            for _ in 0..32 {
                let (mut tree_root, mut parent, mut children, mut count) =
                    (0, 0, ptr::null_mut(), 0);
                let ok = unsafe {
                    (self.x.XQueryTree)(
                        self.display,
                        current,
                        &mut tree_root,
                        &mut parent,
                        &mut children,
                        &mut count,
                    )
                };
                if !children.is_null() {
                    unsafe { (self.x.XFree)(children.cast()) };
                }
                if ok == 0 || tree_root != root || parent == 0 {
                    return None;
                }
                if parent == root {
                    found_root = true;
                    break;
                }
                current = parent;
            }
            if !found_root {
                return None;
            }
            if current != id {
                let mut frame: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
                if unsafe { (self.x.XGetWindowAttributes)(self.display, current, &mut frame) } == 0
                {
                    return None;
                }
                rect = Rect {
                    x: frame.x as i64,
                    y: frame.y as i64,
                    width: frame.width as i64 + 2 * frame.border_width as i64,
                    height: frame.height as i64 + 2 * frame.border_width as i64,
                };
            } else {
                rect.x -= attrs.border_width as i64;
                rect.y -= attrs.border_width as i64;
                rect.width += 2 * attrs.border_width as i64;
                rect.height += 2 * attrs.border_width as i64;
            }
        }
        trace(format_args!("window={id:#x}: outer_rect={rect:?}"));
        (rect.width > 0 && rect.height > 0).then_some(rect)
    }

    fn same_desktop(&self, terminal: xlib::Window, root: xlib::Window) -> bool {
        let desktops = (
            self.number(terminal, c"_NET_WM_DESKTOP"),
            self.number(root, c"_NET_CURRENT_DESKTOP"),
        );
        trace(format_args!(
            "terminal={terminal:#x}: (window_desktop, current_desktop)={desktops:?}"
        ));
        desktop_is_current(desktops.0, desktops.1)
    }

    fn monitors(&self, root: xlib::Window) -> Option<Vec<Rect>> {
        let randr = xrandr::Xrandr::open().ok()?;
        let (mut major, mut minor) = (0, 0);
        if unsafe { (randr.XRRQueryVersion)(self.display, &mut major, &mut minor) } == 0
            || (major, minor) < (1, 5)
        {
            trace(format_args!(
                "RandR unavailable or older than 1.5: {major}.{minor}"
            ));
            return None;
        }
        let mut count = 0;
        let native = unsafe { (randr.XRRGetMonitors)(self.display, root, xlib::True, &mut count) };
        if native.is_null() {
            return None;
        }
        let result = if (1..=128).contains(&count) {
            Some(
                unsafe { std::slice::from_raw_parts(native, count as usize) }
                    .iter()
                    .map(|monitor| Rect {
                        x: monitor.x as i64,
                        y: monitor.y as i64,
                        width: monitor.width as i64,
                        height: monitor.height as i64,
                    })
                    .collect(),
            )
        } else {
            None
        };
        unsafe { (randr.XRRFreeMonitors)(native) };
        trace(format_args!("RandR {major}.{minor}: monitors={result:?}"));
        result
    }

    fn terminal(
        &self,
        graphics: xlib::Window,
        root: xlib::Window,
        ancestors: &[u32],
        hostname: &[u8],
        explicit: Option<xlib::Window>,
    ) -> Option<xlib::Window> {
        let windows = self
            .numbers(root, c"_NET_CLIENT_LIST", xlib::XA_WINDOW)
            .or_else(|| {
                trace(format_args!(
                    "missing _NET_CLIENT_LIST; terminal cannot be identified via X11"
                ));
                None
            })?;
        trace(format_args!("graphics={graphics:#x}, WINDOWID={explicit:?}, ancestors={ancestors:?}, clients={windows:?}"));
        let candidates: Vec<_> = windows
            .into_iter()
            .filter(|id| *id != graphics && self.local_ancestor(*id, ancestors, hostname))
            .collect();
        trace(format_args!("validated_terminal_candidates={candidates:?}"));
        let terminal = choose_terminal(explicit, &candidates).or_else(|| {
            trace(format_args!(
                "no unique validated terminal (or WINDOWID does not match)"
            ));
            None
        })?;
        if !self.same_desktop(terminal, root) {
            return None;
        }
        self.visible_rect(terminal, root)?;
        Some(terminal)
    }

    fn separate(
        &self,
        graphics: xlib::Window,
        terminal: xlib::Window,
        root: xlib::Window,
        fullscreen: bool,
    ) -> Option<bool> {
        let separate = separate_monitors(
            self.visible_rect(graphics, root)?,
            self.visible_rect(terminal, root)?,
            &self.monitors(root)?,
            fullscreen,
        )?;
        trace(format_args!("terminal_separate={separate}"));
        Some(separate)
    }

    fn activate(&self, terminal: xlib::Window, graphics: xlib::Window, root: xlib::Window) -> bool {
        let active = || {
            self.numbers(root, c"_NET_ACTIVE_WINDOW", xlib::XA_WINDOW)
                .is_some_and(|values| values.as_slice() == [terminal])
        };
        if active() {
            trace(format_args!("terminal already active"));
            return true;
        }
        let Some(atom) = self.atom(c"_NET_ACTIVE_WINDOW") else {
            return false;
        };
        if !self
            .numbers(root, c"_NET_SUPPORTED", xlib::XA_ATOM)
            .is_some_and(|supported| supported.contains(&atom))
        {
            return false;
        }
        let mut message: xlib::XClientMessageEvent = unsafe { std::mem::zeroed() };
        message.type_ = xlib::ClientMessage;
        message.display = self.display;
        message.window = terminal;
        message.message_type = atom;
        message.format = 32;
        message.data.set_long(0, 1); // Ordinary application, not a pager.
        message.data.set_long(1, xlib::CurrentTime as c_long);
        // Help the WM recognize a transfer from our own active window, without
        // claiming an unrelated foreground window as the requester's.
        if self
            .numbers(root, c"_NET_ACTIVE_WINDOW", xlib::XA_WINDOW)
            .is_some_and(|values| values.as_slice() == [graphics])
        {
            message.data.set_long(2, graphics as c_long);
        }
        let mut event = xlib::XEvent::from(message);
        if unsafe {
            (self.x.XSendEvent)(
                self.display,
                root,
                xlib::False,
                xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask,
                &mut event,
            )
        } == 0
        {
            return false;
        }
        unsafe { (self.x.XFlush)(self.display) };
        let deadline = Instant::now() + Duration::from_millis(200);
        loop {
            if PROBE_FAILED.load(Ordering::SeqCst) {
                return false;
            }
            if active() {
                trace(format_args!("terminal activation confirmed"));
                return true;
            }
            if Instant::now() >= deadline {
                trace(format_args!(
                    "terminal activation not confirmed within 200 ms"
                ));
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

pub(super) fn focus_console(window: &mut super::GraphicsWindow, for_debugger: bool) {
    trace(format_args!(
        "console return: fullscreen={}",
        window.is_fullscreen()
    ));
    if focus_known_console(window, for_debugger).is_none() {
        trace(format_args!(
            "terminal/location unavailable: suspending fullscreen to keep the console accessible"
        ));
        window.settle_keyboard_before_console_focus();
        window.suspend_fullscreen_for_console();
    }
    trace(format_args!(
        "console return complete: fullscreen={}",
        window.is_fullscreen()
    ));
}

fn focus_known_console(window: &mut super::GraphicsWindow, for_debugger: bool) -> Option<()> {
    if unsafe { libc::isatty(libc::STDIN_FILENO) } == 0 {
        trace(format_args!("stdin is not a terminal"));
        return None;
    }
    let ancestors = ancestors();
    if ancestors.is_empty() {
        trace(format_args!("process ancestry unavailable"));
        return None;
    }
    let mut hostname = [0u8; 256];
    if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } != 0 {
        return None;
    }
    let Some(length) = hostname.iter().position(|byte| *byte == 0) else {
        return None;
    };
    if length == 0 {
        return None;
    }
    let explicit = match std::env::var("WINDOWID") {
        Ok(value) => match parse_window_id(&value) {
            Some(id) => Some(id),
            None => return None,
        },
        Err(std::env::VarError::NotPresent) => None,
        Err(_) => return None,
    };
    let (Ok(display_handle), Ok(window_handle)) = (
        window.window.display_handle(),
        window.window.window_handle(),
    ) else {
        return None;
    };
    let (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(handle)) =
        (display_handle.as_raw(), window_handle.as_raw())
    else {
        return None;
    };
    let Some(native_display) = display.display else {
        return None;
    };
    let Ok(x) = xlib::Xlib::open() else {
        return None;
    };
    let Ok(_lock) = PROBE_LOCK.lock() else {
        return None;
    };
    let connection =
        unsafe { (x.XOpenDisplay)((x.XDisplayString)(native_display.as_ptr().cast())) };
    if connection.is_null() {
        trace(format_args!("cannot open private X11 connection"));
        return None;
    }
    PROBE_FAILED.store(false, Ordering::SeqCst);
    PROBE_DISPLAY.store(connection, Ordering::SeqCst);
    let previous = unsafe { (x.XSetErrorHandler)(Some(probe_error)) };
    PREVIOUS_HANDLER.store(
        previous.map_or(0, |handler| handler as usize),
        Ordering::SeqCst,
    );
    let probe = Probe {
        x: &x,
        display: connection,
        previous,
    };
    let root = unsafe { (x.XRootWindow)(connection, display.screen) };
    let terminal = probe.terminal(
        handle.window,
        root,
        &ancestors,
        &hostname[..length],
        explicit,
    )?;
    window.settle_keyboard_before_console_focus();
    if !window.is_open() || !probe.same_desktop(terminal, root) {
        return None;
    }
    let separate = probe.separate(handle.window, terminal, root, window.is_fullscreen())?;
    unsafe { (x.XSync)(connection, xlib::False) };
    if PROBE_FAILED.load(Ordering::SeqCst) {
        return None;
    }
    if !separate {
        trace(format_args!(
            "suspending fullscreen: separate monitor not established"
        ));
        window.suspend_fullscreen_for_console();
    } else if for_debugger {
        window.resume_fullscreen_for_console();
        if window.is_fullscreen()
            && probe.separate(handle.window, terminal, root, true) != Some(true)
        {
            trace(format_args!(
                "suspending fullscreen: separation not confirmed after the WM transition"
            ));
            window.suspend_fullscreen_for_console();
        }
    }
    // Activation is best effort. A focus-stealing policy or slow WM reply does
    // not change proven monitor geometry: a visible terminal on another monitor
    // remains accessible without shrinking graphics. After a valid geometry
    // decision, activation errors must not turn known overlap into an unknown
    // location and make the debugger re-enable fullscreen over the terminal.
    if !probe.activate(terminal, handle.window, root) {
        trace(format_args!(
            "activation not confirmed; retaining monitor-based presentation"
        ));
    }
    unsafe { (x.XSync)(connection, xlib::False) };
    if PROBE_FAILED.load(Ordering::SeqCst) {
        trace(format_args!(
            "X11 activation error; preserving the established monitor decision"
        ));
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn separate_fullscreen_monitors(
        graphics: Rect,
        terminal: Rect,
        monitors: &[Rect],
    ) -> Option<bool> {
        separate_monitors(graphics, terminal, monitors, true)
    }

    fn rect(x: i64, y: i64, width: i64, height: i64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn sticky_terminal_does_not_require_a_current_desktop_property() {
        assert!(desktop_is_current(Some(u32::MAX as c_ulong), None));
        assert!(desktop_is_current(Some(u32::MAX as c_ulong), Some(3)));
        assert!(desktop_is_current(Some(3), Some(3)));
        assert!(!desktop_is_current(Some(2), Some(3)));
        assert!(!desktop_is_current(Some(0), None));
        assert!(!desktop_is_current(None, Some(0)));
        assert!(!desktop_is_current(None, None));
    }

    #[test]
    fn normal_window_uses_unique_largest_intersection_and_rechecks_fullscreen() {
        let left = rect(0, 0, 1280, 720);
        let right = rect(1280, 0, 1280, 720);
        let monitors = [left, right];
        let terminal_left = rect(100, 50, 700, 400);
        let terminal_right = rect(1340, 50, 700, 400);
        let normal = rect(100, 80, 962, 559);
        let oversized = rect(0, 0, 1282, 739);
        assert_eq!(
            separate_monitors(normal, terminal_right, &monitors, false),
            Some(true)
        );
        assert_eq!(
            separate_monitors(oversized, terminal_right, &monitors, false),
            Some(true)
        );
        assert_eq!(
            separate_monitors(oversized, terminal_left, &monitors, false),
            Some(false)
        );
        // Already-fullscreen geometry must actually identify the monitor;
        // do not apply the pre-transition estimate to the resulting surface.
        assert_eq!(
            separate_monitors(oversized, terminal_right, &monitors, true),
            None
        );
        assert_eq!(
            separate_monitors(right, terminal_right, &monitors, true),
            Some(false)
        );
        assert_eq!(
            graphics_monitor(rect(960, 0, 640, 480), &monitors, false),
            None
        );
        assert_eq!(
            graphics_monitor(rect(3000, 0, 640, 480), &monitors, false),
            None
        );
        assert_eq!(graphics_monitor(normal, &[left, left, right], false), None);
    }

    #[test]
    fn monitor_relation_distinguishes_overlap_from_unknown_topology() {
        let left = rect(-1920, 0, 1920, 1080);
        let right = rect(0, 0, 2560, 1440);
        let terminal = rect(50, 50, 800, 600);
        assert_eq!(
            separate_fullscreen_monitors(left, terminal, &[left, right]),
            Some(true)
        );
        assert_eq!(
            separate_fullscreen_monitors(right, terminal, &[left, right]),
            Some(false)
        );
        assert_eq!(
            separate_fullscreen_monitors(left, rect(-10, 50, 800, 600), &[left, right]),
            Some(false)
        );
        // Missing monitors, cloned/overlapping topology, an off-desktop
        // terminal or a graphics window spanning monitors are unknown, rather
        // than evidence that fullscreen would cover the terminal.
        assert_eq!(separate_fullscreen_monitors(left, terminal, &[]), None);
        assert_eq!(separate_fullscreen_monitors(left, terminal, &[left]), None);
        assert_eq!(
            separate_fullscreen_monitors(left, terminal, &[left, left, right]),
            None
        );
        assert_eq!(
            separate_fullscreen_monitors(left, terminal, &[left, rect(-100, 0, 2660, 1440)]),
            None
        );
        assert_eq!(
            separate_fullscreen_monitors(left, rect(50, 50, 0, 600), &[left, right]),
            None
        );
        assert_eq!(
            separate_fullscreen_monitors(rect(-10, 0, 640, 480), terminal, &[left, right]),
            None
        );
    }

    #[test]
    fn a_terminal_need_not_fit_wholly_on_one_other_monitor() {
        let graphics = rect(0, 0, 1920, 1080);
        let right = rect(1920, 0, 1920, 1080);
        let far_right = rect(3840, 0, 1920, 1080);
        // Partly beyond the desktop edge or across two other monitors.
        assert_eq!(
            separate_fullscreen_monitors(graphics, rect(3300, 50, 800, 600), &[graphics, right]),
            Some(true)
        );
        assert_eq!(
            separate_fullscreen_monitors(
                graphics,
                rect(3700, 50, 800, 600),
                &[graphics, right, far_right]
            ),
            Some(true)
        );
        // A maximized terminal's decorations may extend above the display.
        assert_eq!(
            separate_fullscreen_monitors(graphics, rect(1920, -5, 1920, 1090), &[graphics, right]),
            Some(true)
        );
        // A one-pixel overlap with the graphics monitor still fails safe.
        assert_eq!(
            separate_fullscreen_monitors(graphics, rect(1919, 50, 800, 600), &[graphics, right]),
            Some(false)
        );
        assert_eq!(
            separate_fullscreen_monitors(graphics, rect(4000, 50, 800, 600), &[graphics, right]),
            None
        );
    }

    #[test]
    fn ownership_is_unique_or_explicit_and_validated() {
        assert_eq!(choose_terminal(None, &[7]), Some(7));
        assert_eq!(choose_terminal(None, &[7, 8]), None);
        assert_eq!(choose_terminal(Some(8), &[7, 8]), Some(8));
        assert_eq!(choose_terminal(Some(9), &[7, 8]), None);
        assert_eq!(choose_terminal(None, &[]), None);
        assert_eq!(parse_window_id("0xABC"), Some(0xabc));
        assert_eq!(parse_window_id("123"), Some(123));
        assert_eq!(parse_window_id("0"), None);
        assert_eq!(parse_window_id("nonsense"), None);
    }

    #[test]
    fn process_parent_parsing_handles_unusual_command_names() {
        assert_eq!(parent_pid("321 (my terminal (tab)) S 123 99 88"), Some(123));
        assert_eq!(parent_pid("invalid"), None);
        assert_eq!(parent_pid("321 (name) S nope"), None);
    }

    #[test]
    #[ignore = "requires an X11 display; checks that a disappearing terminal cannot terminate BASIC"]
    fn vanished_x11_window_is_an_unknown_location() {
        let x = xlib::Xlib::open().unwrap();
        let _lock = PROBE_LOCK.lock().unwrap();
        let display = unsafe { (x.XOpenDisplay)(ptr::null()) };
        assert!(!display.is_null(), "an X11 display is required");
        PROBE_FAILED.store(false, Ordering::SeqCst);
        PROBE_DISPLAY.store(display, Ordering::SeqCst);
        let previous = unsafe { (x.XSetErrorHandler)(Some(probe_error)) };
        PREVIOUS_HANDLER.store(
            previous.map_or(0, |handler| handler as usize),
            Ordering::SeqCst,
        );
        let probe = Probe {
            x: &x,
            display,
            previous,
        };
        let root = unsafe { (x.XDefaultRootWindow)(display) };
        let stale = unsafe { (x.XCreateSimpleWindow)(display, root, 0, 0, 10, 10, 0, 0, 0) };
        assert_ne!(stale, 0);
        unsafe {
            (x.XDestroyWindow)(display, stale);
            (x.XSync)(display, xlib::False);
        }
        assert!(!PROBE_FAILED.load(Ordering::SeqCst));
        assert_eq!(probe.visible_rect(stale, root), None);
        unsafe {
            (x.XSync)(display, xlib::False);
        }
        assert!(PROBE_FAILED.load(Ordering::SeqCst));
        drop(probe);
        assert!(PROBE_DISPLAY.load(Ordering::SeqCst).is_null());
    }

    #[test]
    #[ignore = "opens an X11 graphics window and injects a delayed key release"]
    fn x11_console_handoff_settles_the_held_key() {
        use super::super::GraphicsWindow;
        use crate::graphics::Graphics;
        let graphics = Graphics::default();
        let mut window = GraphicsWindow::new(&graphics).unwrap();
        window.set_title("AVL X11 console key-release verification");
        let deadline = Instant::now() + Duration::from_secs(2);
        while !window.window.is_active() {
            window.present(&graphics).unwrap();
            assert!(
                Instant::now() < deadline,
                "test graphics window must become active"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let RawWindowHandle::Xlib(handle) = window.window.window_handle().unwrap().as_raw() else {
            panic!("X11 required")
        };
        let id = handle.window;
        let x = xlib::Xlib::open().unwrap();
        let display = unsafe { (x.XOpenDisplay)(ptr::null()) };
        assert!(!display.is_null());
        let root = unsafe { (x.XDefaultRootWindow)(display) };
        let keycode = unsafe { (x.XKeysymToKeycode)(display, b'n' as c_ulong) } as u32;
        unsafe fn send(
            x: &xlib::Xlib,
            display: *mut xlib::Display,
            id: xlib::Window,
            root: xlib::Window,
            keycode: u32,
            kind: c_int,
        ) {
            let mut key: xlib::XKeyEvent = std::mem::zeroed();
            key.type_ = kind;
            key.display = display;
            key.window = id;
            key.root = root;
            key.keycode = keycode;
            key.same_screen = xlib::True;
            let mut event = xlib::XEvent::from(key);
            let mask = if kind == xlib::KeyPress {
                xlib::KeyPressMask
            } else {
                xlib::KeyReleaseMask
            };
            assert_ne!(
                (x.XSendEvent)(display, id, xlib::False, mask, &mut event),
                0
            );
            (x.XFlush)(display);
        }
        unsafe {
            send(&x, display, id, root, keycode, xlib::KeyPress);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !window.key_down_code(b'N') {
            window.window.update();
            assert!(Instant::now() < deadline, "X11 key press must reach minifb");
            std::thread::sleep(Duration::from_millis(5));
        }
        let release = std::thread::spawn(move || {
            let x = xlib::Xlib::open().unwrap();
            let display = unsafe { (x.XOpenDisplay)(ptr::null()) };
            assert!(!display.is_null());
            std::thread::sleep(Duration::from_millis(80));
            unsafe {
                send(&x, display, id, root, keycode, xlib::KeyRelease);
            }
            // Bound a failing test too: a missed release must not hang the suite.
            std::thread::sleep(Duration::from_millis(300));
            unsafe {
                let mut focus: xlib::XFocusChangeEvent = std::mem::zeroed();
                focus.type_ = xlib::FocusOut;
                focus.display = display;
                focus.window = id;
                let mut event = xlib::XEvent::from(focus);
                (x.XSendEvent)(display, id, xlib::False, xlib::FocusChangeMask, &mut event);
                (x.XFlush)(display);
                (x.XCloseDisplay)(display);
            }
        });
        let started = Instant::now();
        window.settle_keyboard_before_console_focus();
        assert!(started.elapsed() >= Duration::from_millis(40));
        assert!(!window.key_down_code(b'N'), "handoff retained the held key");
        assert!(window.window.get_keys().is_empty());
        release.join().unwrap();
        unsafe {
            (x.XCloseDisplay)(display);
        }
    }
}
