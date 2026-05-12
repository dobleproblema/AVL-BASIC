use crate::error::{BasicError, BasicResult, ErrorCode};
use crate::graphics::Graphics;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::collections::VecDeque;
#[cfg(windows)]
use std::ffi::c_void;
use std::fmt;

pub struct GraphicsWindow {
    window: Window,
    width: usize,
    height: usize,
    mouse: MouseSnapshot,
    key_queue: VecDeque<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct MouseSnapshot {
    pub x: i32,
    pub y: i32,
    pub left: bool,
    pub right: bool,
    pub event: String,
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
            WindowOptions::default(),
        )
        .map_err(|err| BasicError::new(ErrorCode::Unsupported).with_detail(err.to_string()))?;
        set_embedded_window_icon(&mut window);
        // BASIC drawing commands already decide when to present (`FRAME` or
        // immediate mode). A minifb-side FPS cap here would sleep on every
        // event/buffer update and make PLOT-heavy programs unusably slow.
        window.set_target_fps(0);
        let mut graphics_window = Self {
            window,
            width: graphics.width,
            height: graphics.height,
            mouse: MouseSnapshot::default(),
            key_queue: VecDeque::new(),
        };
        graphics_window.present(graphics)?;
        Ok(graphics_window)
    }

    pub fn matches_size(&self, graphics: &Graphics) -> bool {
        self.width == graphics.width && self.height == graphics.height
    }

    pub fn present(&mut self, graphics: &Graphics) -> BasicResult<MouseSnapshot> {
        if !self.window.is_open() {
            return Ok(self.mouse.clone());
        }
        self.window
            .update_with_buffer(graphics.buffer(), graphics.width, graphics.height)
            .map_err(|err| BasicError::new(ErrorCode::Unsupported).with_detail(err.to_string()))?;
        self.update_keyboard();
        Ok(self.update_mouse())
    }

    pub fn pump_events(&mut self) -> MouseSnapshot {
        if self.window.is_open() {
            self.window.update();
            self.update_keyboard();
        }
        self.update_mouse()
    }

    pub fn set_mouse_cursor_visible(&mut self, visible: bool) {
        self.window.set_cursor_visibility(visible);
    }

    pub fn take_key_code(&mut self) -> Option<u8> {
        self.key_queue.pop_front()
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
        if let Some((x, y)) = self.window.get_mouse_pos(MouseMode::Pass) {
            self.mouse.x = x.round() as i32;
            self.mouse.y = self.height as i32 - 1 - y.round() as i32;
        }
        self.mouse.left = self.window.get_mouse_down(MouseButton::Left);
        self.mouse.right = self.window.get_mouse_down(MouseButton::Right);
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

    fn update_keyboard(&mut self) {
        for key in self.window.get_keys_pressed(KeyRepeat::No) {
            if let Some(code) = key_to_code(key, self.shift_down()) {
                self.key_queue.push_back(code);
            }
        }
    }

    fn shift_down(&self) -> bool {
        self.window.is_key_down(Key::LeftShift) || self.window.is_key_down(Key::RightShift)
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

#[cfg(not(windows))]
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
        Key::Enter | Key::NumPadEnter => Some(13),
        Key::Left => Some(28),
        Key::Right => Some(29),
        Key::Up => Some(30),
        Key::Down => Some(31),
        Key::Home => Some(1),
        Key::End => Some(4),
        Key::PageUp => Some(11),
        Key::PageDown => Some(12),
        Key::Insert => Some(22),
        Key::Delete => Some(127),
        Key::Tab => Some(9),
        Key::Backspace => Some(8),
        Key::Escape => Some(27),
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
        13 => Some(Key::Enter),
        28 => Some(Key::Left),
        29 => Some(Key::Right),
        30 => Some(Key::Up),
        31 => Some(Key::Down),
        1 => Some(Key::Home),
        4 => Some(Key::End),
        11 => Some(Key::PageUp),
        12 => Some(Key::PageDown),
        22 => Some(Key::Insert),
        127 => Some(Key::Delete),
        9 => Some(Key::Tab),
        8 => Some(Key::Backspace),
        27 => Some(Key::Escape),
        _ => None,
    }
}

#[cfg(windows)]
pub fn refocus_console_window() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
        fn GetConsoleWindow() -> *mut c_void;
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
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
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
}

#[cfg(not(windows))]
pub fn refocus_console_window() {}

#[cfg(test)]
mod tests {
    use super::{code_to_key, key_to_code};
    use minifb::Key;

    #[test]
    fn gui_arrow_codes_match_python_keydown_values() {
        assert_eq!(key_to_code(Key::Left, false), Some(28));
        assert_eq!(key_to_code(Key::Right, false), Some(29));
        assert_eq!(key_to_code(Key::Up, false), Some(30));
        assert_eq!(key_to_code(Key::Down, false), Some(31));
        assert_eq!(code_to_key(28), Some(Key::Left));
        assert_eq!(code_to_key(29), Some(Key::Right));
        assert_eq!(code_to_key(30), Some(Key::Up));
        assert_eq!(code_to_key(31), Some(Key::Down));
    }
}
