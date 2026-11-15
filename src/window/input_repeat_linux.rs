use minifb::Window;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_int;
use std::ptr::NonNull;
use std::rc::Rc;
use x11_dl::xlib;

type WireConverter =
    Option<unsafe extern "C" fn(*mut xlib::Display, *mut xlib::XEvent, *mut xlib::xEvent) -> c_int>;

const EVENT_TYPES: [c_int; 3] = [xlib::KeyPress, xlib::KeyRelease, xlib::FocusOut];
const CONVERTERS: [WireConverter; EVENT_TYPES.len()] = [
    Some(convert_event::<0>),
    Some(convert_event::<1>),
    Some(convert_event::<2>),
];

thread_local! {
    static FILTERS: RefCell<HashMap<NonNull<xlib::Display>, RepeatState>> =
        RefCell::new(HashMap::new());
}

struct RepeatState {
    previous: [WireConverter; EVENT_TYPES.len()],
    window: xlib::Window,
    held: [bool; 256],
    allow_repeat: Rc<Cell<bool>>,
}

pub(super) struct InputRepeatFilter {
    display: NonNull<xlib::Display>,
    // Keep the Xlib symbols available until after restoring the converters.
    xlib: xlib::Xlib,
    allow_repeat: Rc<Cell<bool>>,
}

impl InputRepeatFilter {
    /// The owner must drop this filter before the minifb window.
    pub(super) fn new(window: &Window) -> Option<Self> {
        let RawDisplayHandle::Xlib(display_handle) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Xlib(window_handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let display = display_handle.display?.cast::<xlib::Display>();
        let xlib = xlib::Xlib::open().ok()?;
        let mut supported = 0;
        // SAFETY: minifb owns this display and XKB is queried on the owning
        // thread. With detectable autorepeat, a held key only generates
        // additional KeyPress events. Without it, the server also sends
        // fake KeyRelease events that cannot be filtered independently.
        let detectable =
            unsafe { (xlib.XkbGetDetectableAutoRepeat)(display.as_ptr(), &mut supported) };
        if detectable == 0 || supported == 0 {
            return None;
        }

        let allow_repeat = Rc::new(Cell::new(false));
        FILTERS.with(|filters| {
            let mut filters = filters.borrow_mut();
            if filters.contains_key(&display) {
                return None;
            }
            // SAFETY: these are Xlib's core wire converters for the live
            // display owned by this thread. They are restored on drop.
            let previous = std::array::from_fn(|index| unsafe {
                (xlib.XESetWireToEvent)(display.as_ptr(), EVENT_TYPES[index], CONVERTERS[index])
            });
            if previous.iter().any(Option::is_none) {
                // The core converter must be able to decode each event
                // before we can examine its physical keycode.
                restore(&xlib, display, previous);
                return None;
            }
            filters.insert(
                display,
                RepeatState {
                    previous,
                    window: window_handle.window,
                    held: [false; 256],
                    allow_repeat: allow_repeat.clone(),
                },
            );
            Some(Self {
                display,
                xlib,
                allow_repeat,
            })
        })
    }

    pub(super) fn allow_repeat(&self) -> Rc<Cell<bool>> {
        self.allow_repeat.clone()
    }
}

impl Drop for InputRepeatFilter {
    fn drop(&mut self) {
        FILTERS.with(|filters| {
            if let Some(state) = filters.borrow_mut().remove(&self.display) {
                restore(&self.xlib, self.display, state.previous);
            }
        });
    }
}

fn restore(
    xlib: &xlib::Xlib,
    display: NonNull<xlib::Display>,
    previous: [WireConverter; EVENT_TYPES.len()],
) {
    for (event_type, converter) in EVENT_TYPES.into_iter().zip(previous) {
        // SAFETY: the original converter belongs to this live display and
        // this event type, and the owner is still on its creation thread.
        unsafe { (xlib.XESetWireToEvent)(display.as_ptr(), event_type, converter) };
    }
}

unsafe extern "C" fn convert_event<const INDEX: usize>(
    display: *mut xlib::Display,
    event: *mut xlib::XEvent,
    wire: *mut xlib::xEvent,
) -> c_int {
    let Some(display_key) = NonNull::new(display) else {
        return 0;
    };
    FILTERS.with(|filters| {
        let Ok(mut filters) = filters.try_borrow_mut() else {
            return 0;
        };
        let Some(state) = filters.get_mut(&display_key) else {
            return 0;
        };
        let Some(previous) = state.previous[INDEX] else {
            return 0;
        };
        // SAFETY: Xlib supplies these pointers and we forward them unchanged
        // to the original converter. It initializes the XEvent on success.
        let accepted = unsafe { previous(display, event, wire) };
        if accepted == 0 || unsafe { (*event).any.window } != state.window {
            return accepted;
        }

        match EVENT_TYPES[INDEX] {
            xlib::FocusOut => state.held.fill(false),
            xlib::KeyPress | xlib::KeyRelease => {
                // SAFETY: the wire converter was installed for KeyPress or
                // KeyRelease, which use the XEvent key member.
                let keycode = unsafe { (*event).key.keycode };
                if let Some(held) = state.held.get_mut(keycode as usize) {
                    let pressed = EVENT_TYPES[INDEX] == xlib::KeyPress;
                    let was_held = std::mem::replace(held, pressed);
                    if pressed && was_held && !state.allow_repeat.get() {
                        // A false result tells Xlib not to queue this event.
                        return 0;
                    }
                }
            }
            _ => {}
        }
        accepted
    })
}
