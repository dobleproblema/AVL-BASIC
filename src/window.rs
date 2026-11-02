use crate::console;
use crate::error::{BasicError, BasicResult, ErrorCode};
use crate::graphics::Graphics;
use crate::keyboard::{
    function_key_code, BACKSPACE, DELETE, DOWN, END, ENTER, ESCAPE, F1, F12, HOME, INSERT, LEFT,
    PAGE_DOWN, PAGE_UP, RIGHT, TAB, UP,
};
#[cfg(target_os = "linux")]
use minifb::Icon;
use minifb::{
    InputCallback, Key, KeyRepeat, MouseButton, MouseMode, ScaleMode, Window, WindowOptions,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
#[cfg(any(windows, test))]
use std::ffi::c_void;
use std::fmt;
#[cfg(target_os = "linux")]
use std::io::Cursor;
use std::rc::Rc;

mod fullscreen;
use fullscreen::FullscreenController;
#[cfg(target_os = "linux")]
mod confinement_linux;
#[cfg(all(windows, test))]
mod confinement_tests;
#[cfg(target_os = "linux")]
mod console_monitor_linux;
#[cfg(windows)]
mod console_monitor_windows;
#[cfg(windows)]
mod cursor_windows;
#[cfg(target_os = "linux")]
mod linux_present;
#[cfg(windows)]
mod sync_windows;

const INPUT_EVENT_QUEUE_LIMIT: usize = 4096;

/// Physical destination used by minifb's proportional presentation. This is
/// deliberately independent of Graphics::SCALE, which maps BASIC coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationViewport {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PresentationViewport {
    pub fn fit(logical: (usize, usize), physical: (usize, usize)) -> Option<Self> {
        let (lw, lh) = logical;
        let (pw, ph) = physical;
        if lw == 0 || lh == 0 || pw == 0 || ph == 0 {
            return None;
        }
        // Match minifb's float arithmetic and integer truncation on both
        // Win32 and X11; a different rounding rule can displace the mouse.
        let aspect = lw as f32 / lh as f32;
        let (width, height) = if aspect > pw as f32 / ph as f32 {
            (pw, ((pw as f32 / aspect) as usize).max(1).min(ph))
        } else {
            (((ph as f32 * aspect) as usize).max(1).min(pw), ph)
        };
        Some(Self {
            x: (pw - width) / 2,
            y: (ph - height) / 2,
            width,
            height,
        })
    }

    /// Returns a physical graphics-buffer pixel, with a top-left origin.
    /// Margins and points outside the window are not canvas pixels.
    pub fn canvas_pixel(self, point: (f32, f32), logical: (usize, usize)) -> Option<(i32, i32)> {
        let (x, y) = (
            point.0 as f64 - self.x as f64,
            point.1 as f64 - self.y as f64,
        );
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= self.width as f64
            || y >= self.height as f64
        {
            return None;
        }
        Some((
            (x * logical.0 as f64 / self.width as f64).floor() as i32,
            (y * logical.1 as f64 / self.height as f64).floor() as i32,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsInputEvent {
    Character(char),
    Enter,
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

type InputEventQueue = Rc<RefCell<VecDeque<GraphicsInputEvent>>>;

fn has_pending_pause_key(
    key_queue: &VecDeque<u8>,
    input_events: &VecDeque<GraphicsInputEvent>,
) -> bool {
    !key_queue.is_empty() || !input_events.is_empty()
}

#[derive(Default)]
struct FullscreenShortcut {
    alt_keys: Cell<u8>,
    enter_down: Cell<bool>,
    requested: Cell<bool>,
}

struct GraphicsInputCallback {
    events: InputEventQueue,
    shortcut: Rc<FullscreenShortcut>,
}

// minifb's Win32 scancode table does not report LeftAlt/RightAlt.
// GetKeyState, unlike GetAsyncKeyState, observes the keyboard state at the
// message currently being dispatched, including a complete queued chord.
#[cfg(all(windows, not(test)))]
fn callback_native_alt_down() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetKeyState(key: i32) -> i16;
    }
    unsafe { (GetKeyState(0x12) as u16 & 0x8000) != 0 }
}

#[cfg(any(not(windows), test))]
fn callback_native_alt_down() -> bool {
    false
}

impl GraphicsInputCallback {
    fn push(&self, event: GraphicsInputEvent) {
        let mut events = self.events.borrow_mut();
        if events.len() >= INPUT_EVENT_QUEUE_LIMIT {
            events.pop_front();
        }
        events.push_back(event);
    }
}

impl InputCallback for GraphicsInputCallback {
    fn add_char(&mut self, codepoint: u32) {
        if let Some(ch) = char::from_u32(codepoint) {
            if matches!(ch, '\r' | '\n') {
                if self.shortcut.alt_keys.get() != 0 || callback_native_alt_down() {
                    return;
                }
                self.push(GraphicsInputEvent::Enter);
                return;
            }
            if !ch.is_control() {
                self.push(GraphicsInputEvent::Character(ch));
            }
        }
    }

    fn set_key_state(&mut self, key: Key, pressed: bool) {
        let alt_bit = match key {
            Key::LeftAlt => 1,
            Key::RightAlt => 2,
            _ => 0,
        };
        if alt_bit != 0 {
            if pressed {
                self.shortcut
                    .alt_keys
                    .set(self.shortcut.alt_keys.get() | alt_bit);
            } else {
                self.shortcut
                    .alt_keys
                    .set(self.shortcut.alt_keys.get() & !alt_bit);
            }
            return;
        }
        if matches!(key, Key::Enter | Key::NumPadEnter) {
            if pressed
                && !self.shortcut.enter_down.get()
                && (self.shortcut.alt_keys.get() != 0 || callback_native_alt_down())
            {
                self.shortcut.requested.set(true);
            }
            self.shortcut.enter_down.set(pressed);
        }
        if !pressed {
            return;
        }
        let event = match key {
            Key::Enter | Key::NumPadEnter
                if self.shortcut.alt_keys.get() == 0 && !callback_native_alt_down() =>
            {
                GraphicsInputEvent::Enter
            }
            Key::Escape => GraphicsInputEvent::Escape,
            Key::Backspace => GraphicsInputEvent::Backspace,
            Key::Delete => GraphicsInputEvent::Delete,
            Key::Left => GraphicsInputEvent::Left,
            Key::Right => GraphicsInputEvent::Right,
            Key::Home => GraphicsInputEvent::Home,
            Key::End => GraphicsInputEvent::End,
            _ => return,
        };
        self.push(event);
    }
}

pub struct GraphicsWindow {
    // Cursor callback removal/restoration must happen while the HWND is alive.
    #[cfg(windows)]
    cursor: cursor_windows::CursorController,
    // The presenter's separate connection must be dropped before the window.
    #[cfg(target_os = "linux")]
    linux_presenter: Option<linux_present::LinuxPresenter>,
    #[cfg(target_os = "linux")]
    linux_blit_pending: bool,
    #[cfg(target_os = "linux")]
    pointer_confinement: confinement_linux::State,
    window: Window,
    width: usize,
    height: usize,
    mouse: MouseSnapshot,
    mouse_cursor_visible: bool,
    pointer_confinement_suspended: bool,
    // Whether the optional native request is armed, including a refused grab.
    pointer_confinement_requested: bool,
    pointer_confinement_failed: bool,
    key_queue: VecDeque<u8>,
    input_events: InputEventQueue,
    ctrl_c_down: bool,
    fullscreen: FullscreenController,
    fullscreen_requested: bool,
    presentation_dirty: bool,
    last_physical_size: (usize, usize),
    fullscreen_shortcut: Rc<FullscreenShortcut>,
}

#[derive(Debug, Clone, Default)]
pub struct MouseSnapshot {
    pub x: i32,
    pub y: i32,
    pub left: bool,
    pub right: bool,
    pub event: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct CtrlCState {
    foreground: bool,
    down: bool,
}

impl fmt::Debug for GraphicsWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphicsWindow")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("open", &self.window.is_open())
            .finish()
    }
}

impl GraphicsWindow {
    pub fn new(graphics: &Graphics) -> BasicResult<Self> {
        let mut window = Window::new(
            "AVL BASIC Graphics",
            graphics.width,
            graphics.height,
            WindowOptions {
                // Windowed presentation stays at its MODE size. Native
                // fullscreen transitions may still resize it programmatically.
                resize: false,
                scale_mode: ScaleMode::AspectRatioStretch,
                ..WindowOptions::default()
            },
        )
        .map_err(|err| BasicError::new(ErrorCode::Unsupported).with_detail(err.to_string()))?;
        set_embedded_window_icon(&mut window);
        // BASIC drawing commands already decide when to present (`FRAME` or
        // immediate mode). A minifb-side FPS cap here would sleep on every
        // event/buffer update and make PLOT-heavy programs unusably slow.
        window.set_target_fps(0);
        let input_events = InputEventQueue::default();
        let fullscreen_shortcut = Rc::new(FullscreenShortcut::default());
        window.set_input_callback(Box::new(GraphicsInputCallback {
            events: input_events.clone(),
            shortcut: fullscreen_shortcut.clone(),
        }));
        let mut graphics_window = Self {
            #[cfg(windows)]
            cursor: cursor_windows::CursorController::new(&window)
                .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))?,
            #[cfg(target_os = "linux")]
            linux_presenter: None,
            #[cfg(target_os = "linux")]
            linux_blit_pending: true,
            #[cfg(target_os = "linux")]
            pointer_confinement: confinement_linux::State::default(),
            window,
            width: graphics.width,
            height: graphics.height,
            mouse: MouseSnapshot::default(),
            mouse_cursor_visible: true,
            pointer_confinement_suspended: false,
            pointer_confinement_requested: false,
            pointer_confinement_failed: false,
            key_queue: VecDeque::new(),
            input_events,
            ctrl_c_down: false,
            fullscreen: FullscreenController::default(),
            fullscreen_requested: false,
            presentation_dirty: true,
            last_physical_size: (graphics.width, graphics.height),
            fullscreen_shortcut,
        };
        graphics_window
            .window
            .update_with_buffer(graphics.buffer(), graphics.width, graphics.height)
            .map_err(|err| BasicError::new(ErrorCode::Unsupported).with_detail(err.to_string()))?;
        graphics_window.clear_transient_input();
        Ok(graphics_window)
    }

    pub fn matches_size(&self, graphics: &Graphics) -> bool {
        self.width == graphics.width && self.height == graphics.height
    }

    /// Preserve the normal placement when MODE replaces the native window.
    pub(crate) fn position_for_recreation(&self) -> Option<(isize, isize)> {
        if !self.window.is_open() {
            return None;
        }
        self.fullscreen.windowed_position(&self.window)
    }

    pub(crate) fn restore_position(&mut self, position: (isize, isize)) -> BasicResult<()> {
        FullscreenController::set_position(&self.window, position)
            .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))?;
        self.window.update();
        Ok(())
    }

    /// Select presentation without changing the logical drawing surface.
    pub fn set_fullscreen(&mut self, enabled: bool) -> BasicResult<()> {
        self.apply_fullscreen_transition(enabled)?;
        self.fullscreen_requested = enabled;
        Ok(())
    }

    pub fn fullscreen_requested(&self) -> bool {
        self.fullscreen_requested
    }

    /// Inherit a presentation preference without covering a debugger or console.
    pub(crate) fn set_fullscreen_request(&mut self, enabled: bool) {
        self.fullscreen_requested = enabled;
    }

    /// Resume the selected presentation after temporarily revealing the console.
    pub fn resume_fullscreen(&mut self) -> BasicResult<()> {
        if self.is_fullscreen() != self.fullscreen_requested {
            self.apply_fullscreen_transition(self.fullscreen_requested)?;
        }
        Ok(())
    }

    fn apply_fullscreen_transition(&mut self, enabled: bool) -> BasicResult<()> {
        self.release_pointer_confinement();
        // Process pending native events before changing window styles or size.
        self.window.update();
        self.fullscreen
            .set(&self.window, enabled)
            .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))?;
        self.presentation_dirty = true;
        self.window.update();
        self.last_physical_size = self.window.get_size();
        #[cfg(windows)]
        self.cursor.refresh();
        self.update_pointer_confinement();
        Ok(())
    }

    pub fn is_fullscreen(&self) -> bool {
        self.fullscreen.is_fullscreen()
    }

    pub fn physical_size(&self) -> (usize, usize) {
        self.window.get_size()
    }

    pub(crate) fn presentation_needs_refresh(&self) -> bool {
        self.presentation_dirty || self.physical_size() != self.last_physical_size
    }

    pub fn is_active(&mut self) -> bool {
        self.window.is_active()
    }

    pub fn set_title(&mut self, title: &str) {
        self.window.update();
        self.window.set_title(title);
    }

    pub fn presentation_viewport(&self) -> Option<PresentationViewport> {
        PresentationViewport::fit((self.width, self.height), self.physical_size())
    }

    pub fn suspend_fullscreen_for_console(&mut self) {
        self.suspend_pointer_confinement_for_console();
        if self.is_fullscreen() {
            // Keep the requested presentation so the next explicit graphics
            // operation can restore it; passive event pumping must not do so.
            if let Err(error) = self.apply_fullscreen_transition(false) {
                eprintln!("Fullscreen: {error}");
            }
        }
    }

    /// Apply a debugger's pending selection without requesting keyboard focus.
    fn resume_fullscreen_for_console(&mut self) {
        if let Err(error) = self.resume_fullscreen() {
            eprintln!("Fullscreen: {error}");
        }
    }

    pub fn present(&mut self, graphics: &Graphics) -> BasicResult<MouseSnapshot> {
        if !self.window.is_open() {
            return Ok(self.mouse.clone());
        }
        let physical_size = self.window.get_size();
        self.presentation_dirty |= physical_size != self.last_physical_size;
        if graphics.buffer_dirty() || self.presentation_dirty {
            self.window
                .update_with_buffer(graphics.buffer(), graphics.width, graphics.height)
                .map_err(|err| {
                    BasicError::new(ErrorCode::Unsupported).with_detail(err.to_string())
                })?;
            #[cfg(target_os = "linux")]
            {
                self.linux_blit_pending = true;
            }
            // X11 processes ConfigureNotify after blitting. A resize during
            // that update replaces its image, so the next pump must repaint.
            self.presentation_dirty = self.window.get_size() != physical_size;
            self.last_physical_size = physical_size;
        } else {
            self.window.update();
        }
        self.update_keyboard();
        Ok(self.update_mouse())
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn prepare_sync(&mut self) -> BasicResult<()> {
        if self.linux_blit_pending {
            // The old XPutImage and new Present use different connections.
            // Drain the former only after a real blit, never on steady frames.
            linux_present::sync_minifb(&self.window)
                .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))?;
            self.linux_blit_pending = false;
        }
        if self.linux_presenter.is_none() {
            self.linux_presenter = Some(
                linux_present::LinuxPresenter::new(&self.window).map_err(|message| {
                    BasicError::new(ErrorCode::Unsupported).with_detail(message)
                })?,
            );
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn disable_sync(&mut self) {
        self.linux_presenter = None;
        self.presentation_dirty = true;
    }

    /// Deliver the framebuffer through synchronized X11 Present.
    /// Only an explicit FRAME uses this path. Polling input never waits here.
    #[cfg(target_os = "linux")]
    pub(crate) fn present_synchronized(
        &mut self,
        graphics: &Graphics,
    ) -> BasicResult<MouseSnapshot> {
        self.window.update();
        if !self.window.is_open() {
            return Ok(self.mouse.clone());
        }
        self.prepare_sync()?;
        let physical_size = self.window.get_size();
        let result = self.linux_presenter.as_mut().unwrap().present(
            graphics.buffer(),
            (graphics.width, graphics.height),
            physical_size,
        );
        match result {
            Ok(()) => (),
            Err(message) => {
                // An outstanding pixmap may still belong to the server. Close
                // our connection rather than ever reusing it after a timeout.
                self.disable_sync();
                return Err(BasicError::new(ErrorCode::Unsupported).with_detail(message));
            }
        }
        self.window.update();
        self.presentation_dirty = self.window.get_size() != physical_size;
        self.last_physical_size = physical_size;
        self.update_keyboard();
        Ok(self.update_mouse())
    }

    #[cfg(windows)]
    pub(crate) fn prepare_sync(&mut self) -> BasicResult<()> {
        sync_windows::prepare()
            .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn disable_sync(&mut self) {}

    #[cfg(windows)]
    pub(crate) fn present_synchronized(
        &mut self,
        graphics: &Graphics,
    ) -> BasicResult<MouseSnapshot> {
        let mouse = self.present(graphics)?;
        sync_windows::wait(&self.window)
            .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))?;
        Ok(mouse)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub(crate) fn prepare_sync(&mut self) -> BasicResult<()> {
        Err(BasicError::new(ErrorCode::Unsupported)
            .with_detail("SCREEN SYNC is unavailable on this platform"))
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub(crate) fn present_synchronized(
        &mut self,
        _graphics: &Graphics,
    ) -> BasicResult<MouseSnapshot> {
        self.prepare_sync()?;
        unreachable!()
    }

    pub fn pump_events(&mut self) -> MouseSnapshot {
        if self.window.is_open() {
            self.window.update();
            self.update_keyboard();
        }
        self.update_mouse()
    }

    pub fn pump_lifecycle(&mut self, graphics: &Graphics) -> BasicResult<bool> {
        if self.window.is_open() {
            self.window.update();
            self.update_fullscreen_shortcut();
            self.update_pointer_confinement();
            let physical_size = self.window.get_size();
            if self.presentation_dirty || physical_size != self.last_physical_size {
                self.window
                    .update_with_buffer(graphics.buffer(), self.width, self.height)
                    .map_err(|err| {
                        BasicError::new(ErrorCode::Unsupported).with_detail(err.to_string())
                    })?;
                #[cfg(target_os = "linux")]
                {
                    self.linux_blit_pending = true;
                }
                self.presentation_dirty = self.window.get_size() != physical_size;
                self.last_physical_size = physical_size;
            }
            self.clear_transient_input();
            self.mouse.event.clear();
        }
        Ok(self.window.is_open())
    }

    pub fn set_mouse_cursor_visible(&mut self, visible: bool) -> BasicResult<()> {
        #[cfg(windows)]
        self.cursor
            .set_visible(visible)
            .map_err(|message| BasicError::new(ErrorCode::Unsupported).with_detail(message))?;
        #[cfg(not(windows))]
        if self.mouse_cursor_visible != visible {
            self.window.set_cursor_visibility(visible);
        }
        self.mouse_cursor_visible = visible;
        self.update_pointer_confinement();
        Ok(())
    }

    pub fn mouse_cursor_visible(&self) -> bool {
        self.mouse_cursor_visible
    }

    /// A console/debugger can own interaction even if the graphics window
    /// remains fullscreen on another monitor. Passive event pumps must not
    /// reacquire the pointer until an explicit graphics operation resumes it.
    pub(crate) fn suspend_pointer_confinement_for_console(&mut self) {
        self.pointer_confinement_suspended = true;
        self.release_pointer_confinement();
    }

    pub(crate) fn resume_pointer_confinement_for_graphics(&mut self) {
        if !self.pointer_confinement_suspended {
            return;
        }
        self.pointer_confinement_suspended = false;
        self.update_pointer_confinement();
    }

    fn release_pointer_confinement(&mut self) {
        if !self.pointer_confinement_requested && !self.pointer_confinement_failed {
            return;
        }
        #[cfg(windows)]
        let result = self.cursor.release_confinement();
        #[cfg(target_os = "linux")]
        let result = self.pointer_confinement.release(&self.window);
        #[cfg(not(any(windows, target_os = "linux")))]
        let result: Result<(), String> = Ok(());
        if let Err(message) = result {
            self.report_pointer_confinement_failure(message);
        }
        self.pointer_confinement_requested = false;
    }

    fn report_pointer_confinement_failure(&mut self, message: String) {
        if !self.pointer_confinement_failed {
            eprintln!("Pointer confinement: {message}");
        }
        self.pointer_confinement_failed = true;
    }

    fn update_pointer_confinement(&mut self) {
        let enabled = self.window.is_open()
            && self.is_fullscreen()
            && !self.mouse_cursor_visible
            && !self.pointer_confinement_suspended
            && self.window.is_active();
        if !enabled {
            self.release_pointer_confinement();
            self.pointer_confinement_failed = false;
            return;
        }
        if self.pointer_confinement_failed {
            return;
        }
        // Win32 messages maintain geometry and release/reacquire on focus
        // changes. Do not reinstall that request for every drawing command.
        #[cfg(windows)]
        if self.pointer_confinement_requested {
            return;
        }
        #[cfg(windows)]
        let result = self.cursor.set_confinement((self.width, self.height), true);
        #[cfg(target_os = "linux")]
        let result = {
            let viewport = self.presentation_viewport();
            self.pointer_confinement
                .update(&self.window, viewport, true)
        };
        #[cfg(not(any(windows, target_os = "linux")))]
        let result: Result<(), String> = Ok(());
        if let Err(message) = result {
            self.report_pointer_confinement_failure(message);
            self.release_pointer_confinement();
        } else {
            self.pointer_confinement_requested = true;
        }
    }

    pub fn focus(&mut self) {
        // Process pending native events before activation and paint messages.
        self.window.update();
        focus_window_handle(self.window.get_window_handle());
        self.update_pointer_confinement();
    }

    pub fn clear_key_queue(&mut self) {
        self.key_queue.clear();
        self.input_events.borrow_mut().clear();
    }

    pub fn consume_pause_key(&mut self) -> bool {
        let pending = {
            let input_events = self.input_events.borrow();
            has_pending_pause_key(&self.key_queue, &input_events)
        };
        if !pending {
            return false;
        }
        self.clear_key_queue();
        true
    }

    pub fn clear_transient_input(&mut self) {
        self.key_queue.clear();
        self.input_events.borrow_mut().clear();
        self.fullscreen_shortcut.alt_keys.set(0);
        self.fullscreen_shortcut.enter_down.set(false);
        self.fullscreen_shortcut.requested.set(false);
        if self.window.is_open() {
            self.window.update();
            let _ = self.window.get_keys_pressed(KeyRepeat::No);
            self.ctrl_c_down = self.ctrl_c_state().down;
        }
        self.input_events.borrow_mut().clear();
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn settle_keyboard_before_console_focus(&mut self) {
        while self.window.is_open() {
            // Keep dispatching messages while the graphics window still owns
            // the focus, so every key-down seen by minifb also gets its key-up.
            self.window.update();
            self.clear_key_queue();

            #[cfg(windows)]
            let foreground = self.window.is_open() && self.ctrl_c_state().foreground;
            #[cfg(target_os = "linux")]
            let foreground = self.window.is_open() && self.window.is_active();
            if !foreground || self.window.get_keys().is_empty() {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        self.clear_key_queue();
        self.ctrl_c_down = self.ctrl_c_state().down;
    }

    pub fn take_key_code(&mut self) -> Option<u8> {
        self.key_queue.pop_front()
    }

    pub fn take_input_event(&mut self) -> Option<GraphicsInputEvent> {
        self.input_events.borrow_mut().pop_front()
    }

    pub fn key_down_code(&self, code: u8) -> bool {
        let Some(key) = code_to_key(code) else {
            return false;
        };
        self.window.is_key_down(key)
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    fn update_mouse(&mut self) -> MouseSnapshot {
        let previous = self.mouse.clone();
        let point =
            self.window
                .get_unscaled_mouse_pos(MouseMode::Pass)
                .and_then(|point| {
                    let viewport = self.presentation_viewport()?;
                    let pixel = viewport.canvas_pixel(point, (self.width, self.height));
                    #[cfg(windows)]
                    if pixel.is_none() {
                        // A relative packet at a DPI-scaled clip boundary can
                        // virtualize to -1/width. Retry from the physical pointer;
                        // keep the ordinary mapping and real margins unchanged.
                        return self.cursor.canonical_client_pointer().and_then(|point| {
                            viewport.canvas_pixel(point, (self.width, self.height))
                        });
                    }
                    pixel
                });
        if let Some((x, y)) = point {
            self.mouse.x = x;
            self.mouse.y = self.height as i32 - 1 - y;
        } else {
            self.mouse.x = -1;
            self.mouse.y = -1;
        }
        self.mouse.left = point.is_some() && self.window.get_mouse_down(MouseButton::Left);
        self.mouse.right = point.is_some() && self.window.get_mouse_down(MouseButton::Right);
        let moved = self.mouse.x != previous.x || self.mouse.y != previous.y;
        self.mouse.event = if self.mouse.left && !previous.left {
            "LEFTDOWN".to_string()
        } else if !self.mouse.left && previous.left {
            "LEFTUP".to_string()
        } else if self.mouse.right && !previous.right {
            "RIGHTDOWN".to_string()
        } else if !self.mouse.right && previous.right {
            "RIGHTUP".to_string()
        } else if moved && self.mouse.left {
            "LEFTDRAG".to_string()
        } else if moved && self.mouse.right {
            "RIGHTDRAG".to_string()
        } else if moved {
            "MOVE".to_string()
        } else {
            String::new()
        };
        self.mouse.clone()
    }

    fn update_fullscreen_shortcut(&mut self) -> bool {
        // Capture the edge in InputCallback, rather than polling the final key
        // state: a complete press/release can arrive in one minifb update.
        let requested = self.fullscreen_shortcut.requested.replace(false);
        #[cfg(windows)]
        let maximize_requested = self.cursor.take_maximize_request();
        #[cfg(not(windows))]
        let maximize_requested = false;
        let active = self.window.is_active();
        if !active {
            // Alt-up may have gone to another app after Alt+Tab. Do not keep
            // suppressing ordinary Enter when this window becomes active again.
            self.fullscreen_shortcut.alt_keys.set(0);
            self.fullscreen_shortcut.enter_down.set(false);
        }
        if maximize_requested || (requested && active) {
            // A native maximize command explicitly requests ON, including when
            // activation is still queued. It wins over a coincident toggle.
            let enabled = maximize_requested || !self.is_fullscreen();
            if let Err(error) = self.set_fullscreen(enabled) {
                eprintln!("Fullscreen: {error}");
            }
        }
        // Only Alt+Enter consumes Enter. A title-bar action must not swallow a
        // normal Enter key that happens to arrive during the same update.
        requested && active
    }

    fn update_keyboard(&mut self) {
        let fullscreen_key = self.update_fullscreen_shortcut();
        self.update_pointer_confinement();
        if self.update_ctrl_c_state() {
            self.key_queue.clear();
            self.input_events.borrow_mut().clear();
            return;
        }
        let ctrl_down = self.ctrl_down();
        for key in self.window.get_keys_pressed(KeyRepeat::No) {
            if fullscreen_key && matches!(key, Key::Enter | Key::NumPadEnter) {
                continue;
            }
            if key == Key::C && ctrl_down {
                console::request_interrupt();
                continue;
            }
            if let Some(code) = key_to_code(key, self.shift_down()) {
                self.key_queue.push_back(code);
            }
        }
    }

    fn update_ctrl_c_state(&mut self) -> bool {
        let state = self.ctrl_c_state();
        if !state.foreground {
            self.ctrl_c_down = state.down;
            return false;
        }
        let pressed = state.down && !self.ctrl_c_down;
        self.ctrl_c_down = state.down;
        if pressed {
            console::request_interrupt();
        }
        pressed
    }

    #[cfg(windows)]
    fn ctrl_c_state(&self) -> CtrlCState {
        #[link(name = "user32")]
        extern "system" {
            fn GetAsyncKeyState(v_key: i32) -> i16;
            fn GetForegroundWindow() -> *mut c_void;
        }

        const VK_CONTROL: i32 = 0x11;
        const VK_LCONTROL: i32 = 0xA2;
        const VK_RCONTROL: i32 = 0xA3;
        const VK_C: i32 = 0x43;

        unsafe {
            let c = GetAsyncKeyState(VK_C) as u16;
            let ctrl = GetAsyncKeyState(VK_CONTROL) as u16;
            let left_ctrl = GetAsyncKeyState(VK_LCONTROL) as u16;
            let right_ctrl = GetAsyncKeyState(VK_RCONTROL) as u16;

            CtrlCState {
                foreground: GetForegroundWindow() == self.window.get_window_handle(),
                down: c & 0x8000 != 0 && (ctrl | left_ctrl | right_ctrl) & 0x8000 != 0,
            }
        }
    }

    #[cfg(not(windows))]
    fn ctrl_c_state(&self) -> CtrlCState {
        CtrlCState {
            foreground: true,
            down: self.ctrl_down() && self.window.is_key_down(Key::C),
        }
    }

    fn shift_down(&self) -> bool {
        self.window.is_key_down(Key::LeftShift) || self.window.is_key_down(Key::RightShift)
    }

    fn ctrl_down(&self) -> bool {
        self.window.is_key_down(Key::LeftCtrl) || self.window.is_key_down(Key::RightCtrl)
    }
}

impl Drop for GraphicsWindow {
    fn drop(&mut self) {
        self.release_pointer_confinement();
        #[cfg(windows)]
        self.cursor.release();
        // Restore native decorations/placement while the HWND/XID is alive.
        if self.window.is_open() && self.fullscreen.is_fullscreen() {
            self.window.update();
            let _ = self.fullscreen.set(&self.window, false);
        }
        #[cfg(target_os = "linux")]
        if let Err(error) = self.fullscreen.release_emulation(&self.window) {
            eprintln!("Fullscreen emulation cleanup: {error}");
        }
    }
}

#[cfg(windows)]
fn set_embedded_window_icon(window: &mut Window) {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
        fn LoadImageW(
            instance: *mut c_void,
            name: *const u16,
            image_type: u32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> *mut c_void;
        fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    const ICON_RESOURCE_ID: usize = 1;
    const IMAGE_ICON: u32 = 1;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const SM_CXICON: i32 = 11;
    const SM_CYICON: i32 = 12;
    const SM_CXSMICON: i32 = 49;
    const SM_CYSMICON: i32 = 50;

    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let hwnd = window.get_window_handle();
        if instance.is_null() || hwnd.is_null() {
            return;
        }
        let resource = ICON_RESOURCE_ID as *const u16;
        let small = LoadImageW(
            instance,
            resource,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            0,
        );
        if !small.is_null() {
            let _ = SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small as isize);
        }
        let big = LoadImageW(
            instance,
            resource,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXICON),
            GetSystemMetrics(SM_CYICON),
            0,
        );
        if !big.is_null() {
            let _ = SendMessageW(hwnd, WM_SETICON, ICON_BIG, big as isize);
        }
    }
}

#[cfg(target_os = "linux")]
fn set_embedded_window_icon(window: &mut Window) {
    let Some(argb) = embedded_linux_window_icon_argb() else {
        return;
    };
    if let Ok(icon) = Icon::try_from(argb.as_slice()) {
        window.set_icon(icon);
    }
}

#[cfg(target_os = "linux")]
fn embedded_linux_window_icon_argb() -> Option<Vec<u64>> {
    const ICONS: [&[u8]; 3] = [
        include_bytes!("../assets/linux/hicolor/32x32/apps/avl-basic.png"),
        include_bytes!("../assets/linux/hicolor/64x64/apps/avl-basic.png"),
        include_bytes!("../assets/linux/hicolor/128x128/apps/avl-basic.png"),
    ];
    let mut argb = Vec::new();
    for icon in ICONS {
        argb.extend(png_window_icon_argb(icon)?);
    }
    Some(argb)
}

#[cfg(target_os = "linux")]
fn png_window_icon_argb(png_bytes: &[u8]) -> Option<Vec<u64>> {
    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut decoded = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut decoded).ok()?;
    let pixels = &decoded[..info.buffer_size()];
    let width = usize::try_from(info.width).ok()?;
    let height = usize::try_from(info.height).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    let pixel_count = width.checked_mul(height)?;
    let mut argb = Vec::with_capacity(pixel_count.checked_add(2)?);
    argb.push(width as u64);
    argb.push(height as u64);
    for index in 0..pixel_count {
        argb.push(png_pixel_to_argb(pixels, info.color_type, index)?);
    }
    Some(argb)
}

#[cfg(target_os = "linux")]
fn png_pixel_to_argb(pixels: &[u8], color_type: png::ColorType, index: usize) -> Option<u64> {
    let (red, green, blue, alpha) = match color_type {
        png::ColorType::Rgba => {
            let start = index.checked_mul(4)?;
            (
                *pixels.get(start)?,
                *pixels.get(start + 1)?,
                *pixels.get(start + 2)?,
                *pixels.get(start + 3)?,
            )
        }
        png::ColorType::Rgb => {
            let start = index.checked_mul(3)?;
            (
                *pixels.get(start)?,
                *pixels.get(start + 1)?,
                *pixels.get(start + 2)?,
                255,
            )
        }
        png::ColorType::GrayscaleAlpha => {
            let start = index.checked_mul(2)?;
            let gray = *pixels.get(start)?;
            (gray, gray, gray, *pixels.get(start + 1)?)
        }
        png::ColorType::Grayscale => {
            let gray = *pixels.get(index)?;
            (gray, gray, gray, 255)
        }
        png::ColorType::Indexed => return None,
    };
    Some(((alpha as u64) << 24) | ((red as u64) << 16) | ((green as u64) << 8) | blue as u64)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn set_embedded_window_icon(_window: &mut Window) {}

fn key_to_code(key: Key, shifted: bool) -> Option<u8> {
    match key {
        Key::A => Some(if shifted { b'A' } else { b'a' }),
        Key::B => Some(if shifted { b'B' } else { b'b' }),
        Key::C => Some(if shifted { b'C' } else { b'c' }),
        Key::D => Some(if shifted { b'D' } else { b'd' }),
        Key::E => Some(if shifted { b'E' } else { b'e' }),
        Key::F => Some(if shifted { b'F' } else { b'f' }),
        Key::G => Some(if shifted { b'G' } else { b'g' }),
        Key::H => Some(if shifted { b'H' } else { b'h' }),
        Key::I => Some(if shifted { b'I' } else { b'i' }),
        Key::J => Some(if shifted { b'J' } else { b'j' }),
        Key::K => Some(if shifted { b'K' } else { b'k' }),
        Key::L => Some(if shifted { b'L' } else { b'l' }),
        Key::M => Some(if shifted { b'M' } else { b'm' }),
        Key::N => Some(if shifted { b'N' } else { b'n' }),
        Key::O => Some(if shifted { b'O' } else { b'o' }),
        Key::P => Some(if shifted { b'P' } else { b'p' }),
        Key::Q => Some(if shifted { b'Q' } else { b'q' }),
        Key::R => Some(if shifted { b'R' } else { b'r' }),
        Key::S => Some(if shifted { b'S' } else { b's' }),
        Key::T => Some(if shifted { b'T' } else { b't' }),
        Key::U => Some(if shifted { b'U' } else { b'u' }),
        Key::V => Some(if shifted { b'V' } else { b'v' }),
        Key::W => Some(if shifted { b'W' } else { b'w' }),
        Key::X => Some(if shifted { b'X' } else { b'x' }),
        Key::Y => Some(if shifted { b'Y' } else { b'y' }),
        Key::Z => Some(if shifted { b'Z' } else { b'z' }),
        Key::Key0 | Key::NumPad0 => Some(b'0'),
        Key::Key1 | Key::NumPad1 => Some(b'1'),
        Key::Key2 | Key::NumPad2 => Some(b'2'),
        Key::Key3 | Key::NumPad3 => Some(b'3'),
        Key::Key4 | Key::NumPad4 => Some(b'4'),
        Key::Key5 | Key::NumPad5 => Some(b'5'),
        Key::Key6 | Key::NumPad6 => Some(b'6'),
        Key::Key7 | Key::NumPad7 => Some(b'7'),
        Key::Key8 | Key::NumPad8 => Some(b'8'),
        Key::Key9 | Key::NumPad9 => Some(b'9'),
        Key::Space => Some(b' '),
        Key::Enter | Key::NumPadEnter => Some(ENTER),
        Key::Left => Some(LEFT),
        Key::Right => Some(RIGHT),
        Key::Up => Some(UP),
        Key::Down => Some(DOWN),
        Key::Home => Some(HOME),
        Key::End => Some(END),
        Key::PageUp => Some(PAGE_UP),
        Key::PageDown => Some(PAGE_DOWN),
        Key::Insert => Some(INSERT),
        Key::Delete => Some(DELETE),
        Key::Tab => Some(TAB),
        Key::Backspace => Some(BACKSPACE),
        Key::Escape => Some(ESCAPE),
        Key::F1 => function_key_code(1),
        Key::F2 => function_key_code(2),
        Key::F3 => function_key_code(3),
        Key::F4 => function_key_code(4),
        Key::F5 => function_key_code(5),
        Key::F6 => function_key_code(6),
        Key::F7 => function_key_code(7),
        Key::F8 => function_key_code(8),
        Key::F9 => function_key_code(9),
        Key::F10 => function_key_code(10),
        Key::F11 => function_key_code(11),
        Key::F12 => function_key_code(12),
        Key::Comma => Some(b','),
        Key::Period | Key::NumPadDot => Some(b'.'),
        Key::Minus | Key::NumPadMinus => Some(b'-'),
        Key::Equal => Some(b'='),
        Key::Slash | Key::NumPadSlash => Some(b'/'),
        Key::Semicolon => Some(b';'),
        Key::Apostrophe => Some(b'\''),
        Key::LeftBracket => Some(b'['),
        Key::RightBracket => Some(b']'),
        Key::Backslash => Some(b'\\'),
        _ => None,
    }
}

fn code_to_key(code: u8) -> Option<Key> {
    match code {
        b'a' | b'A' => Some(Key::A),
        b'b' | b'B' => Some(Key::B),
        b'c' | b'C' => Some(Key::C),
        b'd' | b'D' => Some(Key::D),
        b'e' | b'E' => Some(Key::E),
        b'f' | b'F' => Some(Key::F),
        b'g' | b'G' => Some(Key::G),
        b'h' | b'H' => Some(Key::H),
        b'i' | b'I' => Some(Key::I),
        b'j' | b'J' => Some(Key::J),
        b'k' | b'K' => Some(Key::K),
        b'l' | b'L' => Some(Key::L),
        b'm' | b'M' => Some(Key::M),
        b'n' | b'N' => Some(Key::N),
        b'o' | b'O' => Some(Key::O),
        b'p' | b'P' => Some(Key::P),
        b'q' | b'Q' => Some(Key::Q),
        b'r' | b'R' => Some(Key::R),
        b's' | b'S' => Some(Key::S),
        b't' | b'T' => Some(Key::T),
        b'u' | b'U' => Some(Key::U),
        b'v' | b'V' => Some(Key::V),
        b'w' | b'W' => Some(Key::W),
        b'x' | b'X' => Some(Key::X),
        b'y' | b'Y' => Some(Key::Y),
        b'z' | b'Z' => Some(Key::Z),
        b'0' => Some(Key::Key0),
        b'1' => Some(Key::Key1),
        b'2' => Some(Key::Key2),
        b'3' => Some(Key::Key3),
        b'4' => Some(Key::Key4),
        b'5' => Some(Key::Key5),
        b'6' => Some(Key::Key6),
        b'7' => Some(Key::Key7),
        b'8' => Some(Key::Key8),
        b'9' => Some(Key::Key9),
        b' ' => Some(Key::Space),
        ENTER => Some(Key::Enter),
        LEFT => Some(Key::Left),
        RIGHT => Some(Key::Right),
        UP => Some(Key::Up),
        DOWN => Some(Key::Down),
        HOME => Some(Key::Home),
        END => Some(Key::End),
        PAGE_UP => Some(Key::PageUp),
        PAGE_DOWN => Some(Key::PageDown),
        INSERT => Some(Key::Insert),
        DELETE => Some(Key::Delete),
        TAB => Some(Key::Tab),
        BACKSPACE => Some(Key::Backspace),
        ESCAPE => Some(Key::Escape),
        F1 => Some(Key::F1),
        code @ F1..=F12 => match code - F1 + 1 {
            2 => Some(Key::F2),
            3 => Some(Key::F3),
            4 => Some(Key::F4),
            5 => Some(Key::F5),
            6 => Some(Key::F6),
            7 => Some(Key::F7),
            8 => Some(Key::F8),
            9 => Some(Key::F9),
            10 => Some(Key::F10),
            11 => Some(Key::F11),
            12 => Some(Key::F12),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(windows)]
pub fn focus_console_window(graphics_window: Option<&mut GraphicsWindow>) {
    focus_console_window_impl(graphics_window, false);
}

#[cfg(windows)]
fn focus_console_window_impl(graphics_window: Option<&mut GraphicsWindow>, for_debugger: bool) {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetAncestor(hwnd: *mut c_void, flags: u32) -> *mut c_void;
        fn GetClassNameW(hwnd: *mut c_void, class_name: *mut u16, max_count: i32) -> i32;
        fn GetForegroundWindow() -> *mut c_void;
        fn IsWindow(hwnd: *mut c_void) -> i32;
        fn IsWindowVisible(hwnd: *mut c_void) -> i32;
    }

    let console_window = unsafe { GetConsoleWindow() };
    let mut class_name = [0u16; 64];
    let class_len = unsafe {
        GetClassNameW(
            console_window,
            class_name.as_mut_ptr(),
            class_name.len() as i32,
        )
    };
    let class_name = &class_name[..class_len.max(0) as usize];
    const GA_ROOTOWNER: u32 = 3;
    let owner = unsafe { GetAncestor(console_window, GA_ROOTOWNER) };
    let visible_owner = (!owner.is_null()
        && unsafe { IsWindow(owner) != 0 && IsWindowVisible(owner) != 0 })
    .then_some(owner);
    // Resolve the visible host even if it already owns focus: skipping
    // activation must not make us lose its monitor identity.
    let target = console_activation_target(
        console_window,
        class_name,
        visible_owner,
        std::ptr::null_mut(),
    );
    if let Some(window) = graphics_window {
        window.suspend_pointer_confinement_for_console();
        // Settle keys before changing presentation or focus. Then read the
        // current monitor positions, since waiting for key-up can take time.
        window.settle_keyboard_before_console_focus();
        let keep_fullscreen = target.is_some_and(|target| {
            console_monitor_windows::console_is_separate(window.window.get_window_handle(), target)
        });
        if !keep_fullscreen {
            window.suspend_fullscreen_for_console();
        } else if for_debugger {
            window.resume_fullscreen_for_console();
        }
    }
    if let Some(target) = target {
        if target != unsafe { GetForegroundWindow() } {
            focus_window_handle(target);
        }
    }
}

#[cfg(windows)]
pub fn focus_console_window_for_debugger(graphics_window: Option<&mut GraphicsWindow>) {
    focus_console_window_impl(graphics_window, true);
}

#[cfg(any(windows, test))]
fn console_activation_target(
    console_window: *mut c_void,
    class_name: &[u16],
    visible_owner: Option<*mut c_void>,
    foreground: *mut c_void,
) -> Option<*mut c_void> {
    if console_window.is_null() {
        return None;
    }
    // A pseudoconsole's HWND is an internal helper, not the visible terminal.
    // Keep classic consoles and unresolved hosts on their existing path.
    let target = if class_name
        .iter()
        .copied()
        .eq("PseudoConsoleWindow".encode_utf16())
    {
        visible_owner
            .filter(|owner| !owner.is_null())
            .unwrap_or(console_window)
    } else {
        console_window
    };
    (target != foreground).then_some(target)
}

#[cfg(windows)]
fn focus_window_handle(hwnd: *mut c_void) {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    #[link(name = "user32")]
    extern "system" {
        fn AttachThreadInput(id_attach: u32, id_attach_to: u32, attach: i32) -> i32;
        fn BringWindowToTop(hwnd: *mut c_void) -> i32;
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, process_id: *mut u32) -> u32;
        fn SetActiveWindow(hwnd: *mut c_void) -> *mut c_void;
        fn SetFocus(hwnd: *mut c_void) -> *mut c_void;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
        fn ShowWindow(hwnd: *mut c_void, cmd_show: i32) -> i32;
    }

    unsafe {
        if hwnd.is_null() {
            return;
        }

        let current_thread = GetCurrentThreadId();
        let foreground = GetForegroundWindow();
        let foreground_thread = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, std::ptr::null_mut())
        };
        let attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(current_thread, foreground_thread, 1) != 0;

        const SW_SHOW: i32 = 5;
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetActiveWindow(hwnd);
        let _ = SetFocus(hwnd);

        if attached {
            let _ = AttachThreadInput(current_thread, foreground_thread, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn focus_console_window(graphics_window: Option<&mut GraphicsWindow>) {
    if let Some(window) = graphics_window {
        window.suspend_pointer_confinement_for_console();
        #[cfg(target_os = "linux")]
        console_monitor_linux::focus_console(window, false);
        #[cfg(not(target_os = "linux"))]
        window.suspend_fullscreen_for_console();
    }
}

#[cfg(not(windows))]
pub fn focus_console_window_for_debugger(graphics_window: Option<&mut GraphicsWindow>) {
    #[cfg(target_os = "linux")]
    if let Some(window) = graphics_window {
        window.suspend_pointer_confinement_for_console();
        console_monitor_linux::focus_console(window, true);
    }
    #[cfg(not(target_os = "linux"))]
    focus_console_window(graphics_window);
}

#[cfg(not(windows))]
fn focus_window_handle(_hwnd: *mut std::ffi::c_void) {}

#[cfg(test)]
mod tests {
    use super::{
        code_to_key, console_activation_target, has_pending_pause_key, key_to_code,
        GraphicsInputCallback, GraphicsInputEvent, InputEventQueue, PresentationViewport,
    };
    use minifb::{InputCallback, Key};
    use std::collections::VecDeque;
    use std::ffi::c_void;

    #[test]
    fn fullscreen_viewport_preserves_aspect_and_maps_only_the_canvas() {
        let old = PresentationViewport::fit((640, 480), (1920, 1080)).unwrap();
        assert_eq!(
            old,
            PresentationViewport {
                x: 240,
                y: 0,
                width: 1440,
                height: 1080
            }
        );
        assert_eq!(old.canvas_pixel((239.0, 540.0), (640, 480)), None);
        assert_eq!(old.canvas_pixel((1680.0, 540.0), (640, 480)), None);
        assert_eq!(old.canvas_pixel((240.0, 0.0), (640, 480)), Some((0, 0)));
        assert_eq!(
            old.canvas_pixel((960.0, 540.0), (640, 480)),
            Some((320, 240))
        );
        assert_eq!(
            old.canvas_pixel((1679.0, 1079.0), (640, 480)),
            Some((639, 479))
        );
        let wide = PresentationViewport::fit((960, 540), (1920, 1080)).unwrap();
        assert_eq!(
            wide,
            PresentationViewport {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(wide.canvas_pixel((1.0, 1.0), (960, 540)), Some((0, 0)));
        assert_eq!(wide.canvas_pixel((2.0, 2.0), (960, 540)), Some((1, 1)));
        assert_eq!(
            wide.canvas_pixel((1919.0, 1079.0), (960, 540)),
            Some((959, 539))
        );
    }

    #[test]
    fn presentation_viewport_handles_minimization_odd_sizes_and_letterboxing() {
        assert_eq!(PresentationViewport::fit((960, 540), (0, 0)), None);
        let wide = PresentationViewport::fit((960, 540), (800, 600)).unwrap();
        assert_eq!(
            wide,
            PresentationViewport {
                x: 0,
                y: 75,
                width: 800,
                height: 450
            }
        );
        assert_eq!(wide.canvas_pixel((400.0, 74.0), (960, 540)), None);
        assert_eq!(wide.canvas_pixel((400.0, 525.0), (960, 540)), None);
        assert_eq!(
            wide.canvas_pixel((400.0, 300.0), (960, 540)),
            Some((480, 270))
        );
        let odd = PresentationViewport::fit((640, 480), (1001, 701)).unwrap();
        assert!(odd.x * 2 + odd.width <= 1001);
        assert!(odd.y * 2 + odd.height <= 701);
        assert_eq!(odd.canvas_pixel((f32::NAN, 1.0), (640, 480)), None);
        assert_eq!(odd.canvas_pixel((-1.0, 1.0), (640, 480)), None);
    }

    /// Exercises the real Win32 window contract. Run explicitly on an unlocked
    /// desktop; ordinary unit-test runs must not open or minimize windows.
    #[cfg(windows)]
    #[test]
    #[ignore = "opens a Windows graphics window and changes its presentation"]
    fn fixed_window_policy_survives_fullscreen_and_minimize() {
        use super::GraphicsWindow;
        use crate::graphics::Graphics;
        use std::mem::size_of;
        use std::time::{Duration, Instant};

        #[repr(C)]
        #[derive(Default)]
        struct Rect {
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
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
            fn GetWindowLongPtrW(window: *mut c_void, index: i32) -> isize;
            fn ShowWindow(window: *mut c_void, command: i32) -> i32;
            fn IsIconic(window: *mut c_void) -> i32;
            fn IsZoomed(window: *mut c_void) -> i32;
            fn SendMessageW(
                window: *mut c_void,
                message: u32,
                wparam: usize,
                lparam: isize,
            ) -> isize;
            fn MonitorFromWindow(window: *mut c_void, flags: u32) -> *mut c_void;
            fn GetMonitorInfoW(monitor: *mut c_void, info: *mut MonitorInfo) -> i32;
        }

        const GWL_STYLE: i32 = -16;
        const WS_THICKFRAME: isize = 0x0004_0000;
        const WS_MINIMIZEBOX: isize = 0x0002_0000;
        const WS_MAXIMIZEBOX: isize = 0x0001_0000;
        const SW_MINIMIZE: i32 = 6;
        const SW_RESTORE: i32 = 9;
        const MONITOR_DEFAULTTONEAREST: u32 = 2;

        fn wait_until(window: &mut GraphicsWindow, condition: impl Fn(&GraphicsWindow) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                window.window.update();
                if condition(window) {
                    return;
                }
                assert!(window.window.is_open(), "the test window was closed");
                assert!(Instant::now() < deadline, "window transition timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        let mut graphics = Graphics::default();
        graphics.set_mode(960).unwrap();
        graphics.set_paper_rgb(11, 23, 47).unwrap();
        graphics.clg();
        graphics
            .circle(480.0, 270.0, 100.0, Some(0xff8040), false)
            .unwrap();
        let original_pixels = graphics.buffer().to_vec();
        let original_capture = graphics.capture_screen();
        let mut window = GraphicsWindow::new(&graphics).unwrap();
        assert!(!window.fullscreen_requested());
        window.set_fullscreen_request(true);
        assert!(window.fullscreen_requested());
        assert!(
            !window.is_fullscreen(),
            "inheriting a preference changed presentation"
        );
        window.set_fullscreen_request(false);
        window.set_title("AVL fixed-window policy verification");
        window.present(&graphics).unwrap();
        let handle = window.window.get_window_handle();
        let original_size = window.physical_size();
        let original_style = unsafe { GetWindowLongPtrW(handle, GWL_STYLE) };
        assert_eq!(original_style & WS_THICKFRAME, 0, "manual resize enabled");
        assert_ne!(
            original_style & WS_MAXIMIZEBOX,
            0,
            "fullscreen button unavailable"
        );
        assert_ne!(original_style & WS_MINIMIZEBOX, 0, "minimize unavailable");

        let mut monitor_info = MonitorInfo {
            size: size_of::<MonitorInfo>() as u32,
            ..MonitorInfo::default()
        };
        let monitor = unsafe { MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST) };
        assert!(!monitor.is_null());
        assert_ne!(unsafe { GetMonitorInfoW(monitor, &mut monitor_info) }, 0);
        let fullscreen_size = (
            (monitor_info.monitor.right - monitor_info.monitor.left) as usize,
            (monitor_info.monitor.bottom - monitor_info.monitor.top) as usize,
        );

        let original_position = window.position_for_recreation();
        for _ in 0..2 {
            // Exercise the native title-bar command, including its reserved
            // low bits and duplicate delivery. It must queue ON without letting
            // Windows maximize first or re-entering fullscreen from WNDPROC.
            unsafe {
                SendMessageW(handle, 0x0112, 0xf033, 0);
                SendMessageW(handle, 0x0112, 0xf030, 0);
            }
            assert!(
                !window.is_fullscreen(),
                "native callback changed presentation"
            );
            assert_eq!(unsafe { IsZoomed(handle) }, 0);
            assert_eq!(window.physical_size(), original_size);
            assert_eq!(window.position_for_recreation(), original_position);
            assert!(window.matches_size(&graphics));
            assert!(window
                .input_events
                .borrow()
                .iter()
                .all(|event| !matches!(event, GraphicsInputEvent::Enter)));
            assert!(
                !window.update_fullscreen_shortcut(),
                "a maximize command consumed an ordinary Enter key"
            );
            wait_until(&mut window, |w| w.physical_size() == fullscreen_size);
            assert!(window.is_fullscreen());
            assert!(window.fullscreen_requested());
            window.present(&graphics).unwrap();
            assert_eq!(graphics.buffer(), original_pixels.as_slice());
            assert_eq!(graphics.capture_screen(), original_capture);

            // Maximize is an explicit ON request, not a toggle. A duplicate
            // arriving while fullscreen wins over a simultaneous Alt+Enter.
            window.fullscreen_shortcut.requested.set(true);
            unsafe { SendMessageW(handle, 0x0112, 0xf030, 0) };
            let active = window.window.is_active();
            assert_eq!(window.update_fullscreen_shortcut(), active);
            assert!(window.is_fullscreen());
            assert!(window.fullscreen_requested());

            window.suspend_fullscreen_for_console();
            wait_until(&mut window, |w| w.physical_size() == original_size);
            assert!(!window.is_fullscreen());
            assert!(window.fullscreen_requested());
            window.pump_events();
            assert!(
                !window.is_fullscreen(),
                "passive pumping resumed fullscreen"
            );
            window.resume_fullscreen().unwrap();
            wait_until(&mut window, |w| w.physical_size() == fullscreen_size);
            assert!(window.is_fullscreen());
            assert!(window.fullscreen_requested());

            window.set_fullscreen(false).unwrap();
            wait_until(&mut window, |w| w.physical_size() == original_size);
            assert!(!window.is_fullscreen());
            assert!(!window.fullscreen_requested());
            assert_eq!(window.position_for_recreation(), original_position);
            window.resume_fullscreen().unwrap();
            assert!(!window.is_fullscreen(), "explicit OFF was not retained");
            assert_eq!(
                unsafe { GetWindowLongPtrW(handle, GWL_STYLE) },
                original_style,
                "fullscreen did not restore the fixed-size window style"
            );
            assert_eq!(graphics.buffer(), original_pixels.as_slice());
            assert_eq!(graphics.capture_screen(), original_capture);
        }

        // Read actual native state, rather than assuming the minimize style bit
        // alone guarantees that minimizing and restoring still work.
        window.window.update();
        unsafe { ShowWindow(handle, SW_MINIMIZE) };
        wait_until(&mut window, |_| unsafe { IsIconic(handle) != 0 });
        unsafe { ShowWindow(handle, SW_RESTORE) };
        wait_until(&mut window, |w| unsafe {
            IsIconic(handle) == 0 && w.physical_size() == original_size
        });
        window.present(&graphics).unwrap();
        assert_eq!(
            unsafe { GetWindowLongPtrW(handle, GWL_STYLE) },
            original_style
        );
        assert_eq!(graphics.buffer(), original_pixels.as_slice());
        assert_eq!(graphics.capture_screen(), original_capture);
    }

    #[test]
    fn fullscreen_shortcut_does_not_leak_enter_into_graphical_input() {
        let events = InputEventQueue::default();
        let mut callback = GraphicsInputCallback {
            events: events.clone(),
            shortcut: Default::default(),
        };
        callback.set_key_state(Key::LeftAlt, true);
        callback.set_key_state(Key::Enter, true);
        callback.add_char('\r' as u32);
        callback.set_key_state(Key::Enter, false);
        callback.set_key_state(Key::LeftAlt, false);
        assert!(events.borrow().is_empty());
        assert!(callback.shortcut.requested.get());
        assert!(!callback.shortcut.enter_down.get());
        callback.set_key_state(Key::Enter, true);
        assert_eq!(
            events.borrow().iter().copied().collect::<Vec<_>>(),
            vec![GraphicsInputEvent::Enter]
        );
        assert_eq!(key_to_code(Key::Escape, false), Some(27));
        assert_eq!(key_to_code(Key::F11, false), Some(138));
    }

    #[cfg(target_os = "linux")]
    use super::embedded_linux_window_icon_argb;

    #[test]
    fn console_focus_resolves_only_a_pseudoconsole_owner() {
        let console = 1usize as *mut c_void;
        let host = 2usize as *mut c_void;
        let other = 3usize as *mut c_void;
        for (class, owner, expected) in [
            ("ConsoleWindowClass", Some(host), console),
            ("PseudoConsoleWindow", Some(host), host),
            ("PseudoConsoleWindow", None, console),
            ("PseudoConsoleWindow", Some(std::ptr::null_mut()), console),
            ("PseudoConsoleWindow", Some(console), console),
            ("", Some(host), console),
        ] {
            let class = class.encode_utf16().collect::<Vec<_>>();
            assert_eq!(
                console_activation_target(console, &class, owner, other),
                Some(expected)
            );
        }
    }

    #[test]
    fn console_focus_skips_only_the_exact_active_target() {
        let console = 1usize as *mut c_void;
        let host = 2usize as *mut c_void;
        let other = 3usize as *mut c_void;
        let pseudo = "PseudoConsoleWindow".encode_utf16().collect::<Vec<_>>();
        let classic = "ConsoleWindowClass".encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            console_activation_target(console, &pseudo, Some(host), host),
            None
        );
        assert_eq!(
            console_activation_target(console, &classic, None, console),
            None
        );
        assert_eq!(
            console_activation_target(console, &pseudo, Some(host), other),
            Some(host)
        );
        assert_eq!(
            console_activation_target(console, &pseudo, Some(host), console),
            Some(host)
        );
        assert_eq!(
            console_activation_target(std::ptr::null_mut(), &pseudo, Some(host), other),
            None
        );
    }

    #[test]
    fn gui_arrow_codes_match_python_keydown_values() {
        assert_eq!(key_to_code(Key::Left, false), Some(28));
        assert_eq!(key_to_code(Key::Right, false), Some(29));
        assert_eq!(key_to_code(Key::Up, false), Some(30));
        assert_eq!(key_to_code(Key::Down, false), Some(31));
        assert_eq!(key_to_code(Key::F1, false), Some(128));
        assert_eq!(key_to_code(Key::F12, false), Some(139));
        assert_eq!(code_to_key(28), Some(Key::Left));
        assert_eq!(code_to_key(29), Some(Key::Right));
        assert_eq!(code_to_key(30), Some(Key::Up));
        assert_eq!(code_to_key(31), Some(Key::Down));
        assert_eq!(code_to_key(128), Some(Key::F1));
        assert_eq!(code_to_key(139), Some(Key::F12));
    }

    #[test]
    fn graphical_input_callback_preserves_unicode_and_edit_event_order() {
        let events = InputEventQueue::default();
        let mut callback = GraphicsInputCallback {
            events: events.clone(),
            shortcut: Default::default(),
        };

        callback.add_char('ñ' as u32);
        callback.set_key_state(Key::Left, true);
        callback.add_char('á' as u32);
        callback.set_key_state(Key::Enter, true);

        assert_eq!(
            events.borrow().iter().copied().collect::<Vec<_>>(),
            vec![
                GraphicsInputEvent::Character('ñ'),
                GraphicsInputEvent::Left,
                GraphicsInputEvent::Character('á'),
                GraphicsInputEvent::Enter,
            ]
        );
    }

    #[test]
    fn pause_detects_canonical_and_unicode_graphics_keys() {
        let mut keys = VecDeque::new();
        let mut events = VecDeque::new();

        assert!(!has_pending_pause_key(&keys, &events));
        keys.push_back(b'A');
        assert!(has_pending_pause_key(&keys, &events));
        keys.clear();
        events.push_back(GraphicsInputEvent::Character('ñ'));
        assert!(has_pending_pause_key(&keys, &events));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn embedded_linux_icon_contains_all_x11_sizes() {
        let argb = embedded_linux_window_icon_argb()
            .expect("the embedded Linux icons must be valid PNG files");
        let mut offset = 0;
        for size in [32usize, 64, 128] {
            assert_eq!(&argb[offset..offset + 2], &[size as u64, size as u64]);
            let pixels = &argb[offset + 2..offset + 2 + size * size];
            assert!(pixels.iter().any(|pixel| pixel >> 24 != 0));
            offset += 2 + size * size;
        }
        assert_eq!(offset, argb.len());
    }
}
