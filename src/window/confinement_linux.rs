//! Best-effort pointer confinement on minifb's own X11 connection.
//! The caller releases this state before dropping its Window. No Display
//! pointer is retained or dereferenced from a destructor.
use super::PresentationViewport;
use minifb::Window;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::ptr;
use x11_dl::xlib;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rectangle {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl Rectangle {
    fn new(viewport: PresentationViewport, physical: (usize, usize)) -> Result<Self, String> {
        if viewport.width == 0
            || viewport.height == 0
            || viewport.x > i16::MAX as usize
            || viewport.y > i16::MAX as usize
            || viewport.width > u16::MAX as usize
            || viewport.height > u16::MAX as usize
            || viewport
                .x
                .checked_add(viewport.width)
                .is_none_or(|right| right > physical.0)
            || viewport
                .y
                .checked_add(viewport.height)
                .is_none_or(|bottom| bottom > physical.1)
        {
            return Err("Invalid X11 pointer-confinement rectangle".into());
        }
        Ok(Self {
            x: viewport.x as i32,
            y: viewport.y as i32,
            width: viewport.width as u32,
            height: viewport.height as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Request {
    // Identity only. The actual Display is borrowed from Window on every call.
    display_id: usize,
    parent: xlib::Window,
    rectangle: Rectangle,
}

struct Child {
    owner: Request,
    window: xlib::Window,
    grabbed: bool,
}

#[derive(Default)]
pub(super) struct State {
    api: Option<xlib::Xlib>,
    unavailable: bool,
    child: Option<Child>,
    last_attempt: Option<Request>,
}

fn native_window(window: &Window) -> Option<(*mut xlib::Display, xlib::Window)> {
    let display = window.display_handle().ok()?;
    let handle = window.window_handle().ok()?;
    let (RawDisplayHandle::Xlib(display), RawWindowHandle::Xlib(handle)) =
        (display.as_raw(), handle.as_raw())
    else {
        return None;
    };
    Some((display.display?.as_ptr().cast(), handle.window))
}

fn owns_focus(focus: xlib::Window, parent: xlib::Window, child: Option<xlib::Window>) -> bool {
    // minifb has no native child controls. The only descendant we add is the
    // InputOnly confinement window. Never traverse another client's windows.
    focus != 0
        && focus != xlib::PointerRoot as xlib::Window
        && (focus == parent || child == Some(focus))
}

impl State {
    pub(super) fn update(
        &mut self,
        window: &Window,
        viewport: Option<PresentationViewport>,
        enabled: bool,
    ) -> Result<(), String> {
        let Some(viewport) = viewport.filter(|_| enabled && window.is_open()) else {
            return self.release(window);
        };
        let Some((display, parent)) = native_window(window) else {
            return self.release(window);
        };
        let rectangle = match Rectangle::new(viewport, window.get_size()) {
            Ok(rectangle) => rectangle,
            Err(error) => {
                self.release(window)?;
                return Err(error);
            }
        };
        let request = Request {
            display_id: display as usize,
            parent,
            rectangle,
        };
        if self.last_attempt == Some(request) {
            // Also remember a refused grab. Retrying every FRAME would add
            // round-trips and could unexpectedly take over a later gesture.
            return Ok(());
        }
        if let Some(child) = &self.child {
            if child.owner.display_id != request.display_id || child.owner.parent != parent {
                return Err(
                    "X11 pointer confinement must be released before replacing its window".into(),
                );
            }
        }
        self.last_attempt = Some(request);
        if self.unavailable {
            return Ok(());
        }
        if self.api.is_none() {
            match xlib::Xlib::open() {
                Ok(api) => self.api = Some(api),
                Err(_) => {
                    self.unavailable = true;
                    return Ok(());
                }
            }
        }
        let api = self.api.as_ref().unwrap();
        if let Some(child) = self.child.as_mut() {
            if child.grabbed {
                unsafe { (api.XUngrabPointer)(display, xlib::CurrentTime) };
                child.grabbed = false;
            }
        }
        let mut attributes: xlib::XWindowAttributes = unsafe { std::mem::zeroed() };
        if unsafe { (api.XGetWindowAttributes)(display, parent, &mut attributes) } == 0 {
            self.release(window)?;
            return Err("Cannot query the X11 pointer-confinement window".into());
        }
        let (mut focused, mut revert) = (0, 0);
        unsafe { (api.XGetInputFocus)(display, &mut focused, &mut revert) };
        let focus_is_ours = owns_focus(
            focused,
            parent,
            self.child.as_ref().map(|child| child.window),
        );
        if attributes.map_state != xlib::IsViewable || !focus_is_ours {
            self.release(window)?;
            self.last_attempt = Some(request);
            return Ok(());
        }
        let rectangle = request.rectangle;
        let child_window = if let Some(child) = self.child.as_mut() {
            unsafe {
                (api.XMoveResizeWindow)(
                    display,
                    child.window,
                    rectangle.x,
                    rectangle.y,
                    rectangle.width,
                    rectangle.height,
                );
            }
            child.owner = request;
            child.window
        } else {
            // InputOnly, border/depth zero, inherited visual, and no selected
            // events: pointer events continue to the original minifb parent.
            let child_window = unsafe {
                (api.XCreateWindow)(
                    display,
                    parent,
                    rectangle.x,
                    rectangle.y,
                    rectangle.width,
                    rectangle.height,
                    0,
                    0,
                    xlib::InputOnly as u32,
                    ptr::null_mut(),
                    0,
                    ptr::null_mut(),
                )
            };
            if child_window == 0 {
                return Err("Cannot create the X11 pointer-confinement region".into());
            }
            self.child = Some(Child {
                owner: request,
                window: child_window,
                grabbed: false,
            });
            child_window
        };
        unsafe { (api.XMapWindow)(display, child_window) };
        let status = unsafe {
            (api.XGrabPointer)(
                display,
                parent,
                xlib::True,
                (xlib::PointerMotionMask | xlib::ButtonPressMask | xlib::ButtonReleaseMask) as u32,
                xlib::GrabModeAsync,
                xlib::GrabModeAsync,
                child_window,
                0,
                xlib::CurrentTime,
            )
        };
        if status == xlib::GrabSuccess {
            self.child.as_mut().unwrap().grabbed = true;
            return Ok(());
        }
        self.release(window)?;
        self.last_attempt = Some(request);
        match status {
            // This feature is implicit. A busy/unviewable desktop must not
            // abort a BASIC program or steal another client's active grab.
            xlib::AlreadyGrabbed
            | xlib::GrabNotViewable
            | xlib::GrabFrozen
            | xlib::GrabInvalidTime => Ok(()),
            _ => Err(format!(
                "Unexpected X11 pointer-confinement status {status}"
            )),
        }
    }

    pub(super) fn release(&mut self, window: &Window) -> Result<(), String> {
        self.last_attempt = None;
        let Some(child) = self.child.as_ref() else {
            return Ok(());
        };
        let Some((display, parent)) = native_window(window) else {
            return Err("Cannot release X11 pointer confinement without its owning window".into());
        };
        if child.owner.display_id != display as usize || child.owner.parent != parent {
            return Err("Cannot release X11 pointer confinement through a different window".into());
        }
        let api = self.api.as_ref().unwrap();
        let child = self.child.take().unwrap();
        unsafe {
            if child.grabbed {
                (api.XUngrabPointer)(display, xlib::CurrentTime);
            }
            (api.XDestroyWindow)(display, child.window);
            (api.XFlush)(display);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minifb::{MouseMode, WindowOptions};
    use std::time::{Duration, Instant};

    #[test]
    fn confinement_rectangle_obeys_canvas_and_x11_protocol_bounds() {
        let viewport = PresentationViewport::fit((640, 480), (320, 180)).unwrap();
        assert_eq!(
            Rectangle::new(viewport, (320, 180)).unwrap(),
            Rectangle {
                x: 40,
                y: 0,
                width: 240,
                height: 180
            }
        );
        for invalid in [
            PresentationViewport {
                width: 0,
                ..viewport
            },
            PresentationViewport {
                height: 0,
                ..viewport
            },
            PresentationViewport { x: 81, ..viewport },
            PresentationViewport { y: 1, ..viewport },
            PresentationViewport {
                x: usize::MAX,
                ..viewport
            },
            PresentationViewport {
                width: usize::MAX,
                ..viewport
            },
        ] {
            assert!(Rectangle::new(invalid, (320, 180)).is_err());
        }
        let largest = PresentationViewport {
            x: 0,
            y: 0,
            width: 65535,
            height: 65535,
        };
        assert!(Rectangle::new(largest, (65535, 65535)).is_ok());
        assert!(Rectangle::new(
            PresentationViewport {
                width: 65536,
                ..largest
            },
            (65536, 65535)
        )
        .is_err());
        assert!(Rectangle::new(
            PresentationViewport {
                x: 32768,
                width: 1,
                height: 1,
                y: 0
            },
            (65535, 65535)
        )
        .is_err());
    }

    #[test]
    fn confinement_focus_accepts_only_our_client_and_our_known_child() {
        assert!(owns_focus(100, 100, None));
        assert!(owns_focus(101, 100, Some(101)));
        assert!(!owns_focus(101, 100, None));
        assert!(!owns_focus(200, 100, Some(101)));
        assert!(!owns_focus(0, 100, None));
        assert!(!owns_focus(xlib::PointerRoot as xlib::Window, 100, None));
    }

    struct Fixture {
        state: State,
        window: Window,
        api: xlib::Xlib,
        original: (i32, i32),
    }

    impl Fixture {
        fn new() -> Self {
            let api = xlib::Xlib::open().unwrap();
            let mut window = Window::new(
                "AVL Linux confinement adapter test",
                320,
                180,
                WindowOptions::default(),
            )
            .unwrap();
            window.set_target_fps(0);
            let (display, parent) = native_window(&window).unwrap();
            let (mut root, mut child, mut root_x, mut root_y, mut x, mut y, mut mask) =
                (0, 0, 0, 0, 0, 0, 0);
            unsafe {
                (api.XQueryPointer)(
                    display,
                    parent,
                    &mut root,
                    &mut child,
                    &mut root_x,
                    &mut root_y,
                    &mut x,
                    &mut y,
                    &mut mask,
                )
            };
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                window.update();
                let (mut focus, mut revert) = (0, 0);
                unsafe { (api.XGetInputFocus)(display, &mut focus, &mut revert) };
                if focus == parent {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "native test window must gain real desktop focus"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Self {
                state: State::default(),
                window,
                api,
                original: (root_x, root_y),
            }
        }

        fn warp_and_check(&mut self, target: (i32, i32), expected: (i32, i32)) {
            let (display, parent) = native_window(&self.window).unwrap();
            unsafe { (self.api.XWarpPointer)(display, 0, parent, 0, 0, 0, 0, target.0, target.1) };
            let deadline = Instant::now() + Duration::from_millis(500);
            loop {
                self.window.update();
                let point = self.window.get_unscaled_mouse_pos(MouseMode::Pass);
                if point == Some((expected.0 as f32, expected.1 as f32)) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "mouse position {point:?}, expected {expected:?}"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.state.release(&self.window);
            if let Some((display, _)) = native_window(&self.window) {
                unsafe {
                    let root = (self.api.XDefaultRootWindow)(display);
                    (self.api.XWarpPointer)(
                        display,
                        0,
                        root,
                        0,
                        0,
                        0,
                        0,
                        self.original.0,
                        self.original.1,
                    );
                    (self.api.XSync)(display, xlib::False);
                }
            }
        }
    }

    // This extra connection exists only in the opt-in test, to verify that
    // AlreadyGrabbed never steals a different client's pointer ownership.
    struct CompetingClient {
        api: xlib::Xlib,
        display: *mut xlib::Display,
    }

    impl CompetingClient {
        fn new(parent: xlib::Window) -> Self {
            let api = xlib::Xlib::open().unwrap();
            let display = unsafe { (api.XOpenDisplay)(ptr::null()) };
            assert!(!display.is_null());
            let client = Self { api, display };
            assert_eq!(
                unsafe {
                    (client.api.XGrabPointer)(
                        display,
                        parent,
                        xlib::True,
                        xlib::PointerMotionMask as u32,
                        xlib::GrabModeAsync,
                        xlib::GrabModeAsync,
                        0,
                        0,
                        xlib::CurrentTime,
                    )
                },
                xlib::GrabSuccess
            );
            client
        }
    }

    impl Drop for CompetingClient {
        fn drop(&mut self) {
            unsafe {
                (self.api.XUngrabPointer)(self.display, xlib::CurrentTime);
                (self.api.XCloseDisplay)(self.display);
            }
        }
    }

    #[test]
    #[ignore = "opens an X11 window and briefly confines/restores the pointer; no fullscreen"]
    fn native_confinement_tracks_viewport_and_releases_without_fullscreen() {
        let mut fixture = Fixture::new();
        let viewport = PresentationViewport::fit((640, 480), (320, 180)).unwrap();
        fixture.warp_and_check((10, 90), (10, 90));
        fixture
            .state
            .update(&fixture.window, Some(viewport), true)
            .unwrap();
        assert!(fixture
            .state
            .child
            .as_ref()
            .is_some_and(|child| child.grabbed));
        for (target, expected) in [
            ((10, 90), (40, 90)),
            ((310, 90), (279, 90)),
            ((160, -20), (160, 0)),
            ((160, 200), (160, 179)),
        ] {
            fixture.warp_and_check(target, expected);
        }
        // Reconfiguration follows the new canvas instead of leaving stale bounds.
        let smaller = PresentationViewport {
            x: 80,
            y: 20,
            width: 160,
            height: 120,
        };
        fixture
            .state
            .update(&fixture.window, Some(smaller), true)
            .unwrap();
        fixture.warp_and_check((10, 90), (80, 90));
        fixture.warp_and_check((310, 170), (239, 139));
        fixture
            .state
            .update(&fixture.window, Some(smaller), false)
            .unwrap();
        assert!(fixture.state.child.is_none());
        fixture.warp_and_check((10, 90), (10, 90));
        let (_, parent) = native_window(&fixture.window).unwrap();
        let competitor = CompetingClient::new(parent);
        fixture
            .state
            .update(&fixture.window, Some(viewport), true)
            .unwrap();
        assert!(fixture.state.child.is_none(), "stole another client's grab");
        assert!(fixture.state.last_attempt.is_some());
        drop(competitor);
        fixture
            .state
            .update(&fixture.window, Some(viewport), true)
            .unwrap();
        assert!(
            fixture.state.child.is_none(),
            "retried without a state edge"
        );
        fixture
            .state
            .update(&fixture.window, Some(viewport), false)
            .unwrap();
        fixture
            .state
            .update(&fixture.window, Some(viewport), true)
            .unwrap();
        assert!(fixture
            .state
            .child
            .as_ref()
            .is_some_and(|child| child.grabbed));
        fixture.state.release(&fixture.window).unwrap();
        fixture.warp_and_check((10, 90), (10, 90));
    }
}
