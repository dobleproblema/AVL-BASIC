//! Synchronize an explicit FRAME with Windows desktop composition.
//! DwmFlush does not prove that a particular GDI frame reached physical scanout.
use minifb::Window;
use std::ffi::c_void;

#[link(name = "dwmapi")]
extern "system" {
    fn DwmIsCompositionEnabled(enabled: *mut i32) -> i32;
    fn DwmFlush() -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    fn GdiFlush() -> i32;
}
#[link(name = "user32")]
extern "system" {
    fn IsWindowVisible(window: *mut c_void) -> i32;
    fn IsIconic(window: *mut c_void) -> i32;
}

pub(super) fn prepare() -> Result<(), String> {
    let mut composed = 0;
    let result = unsafe { DwmIsCompositionEnabled(&mut composed) };
    if result != 0 || composed == 0 {
        return Err(format!(
            "SCREEN SYNC ON requires active DWM composition (HRESULT 0x{:08X})",
            result as u32
        ));
    }
    Ok(())
}

pub(super) fn wait(window: &Window) -> Result<(), String> {
    if !window.is_open() {
        return Ok(());
    }
    let handle = window.get_window_handle();
    let size = window.get_size();
    if handle.is_null()
        || size.0 == 0
        || size.1 == 0
        || unsafe { IsWindowVisible(handle) == 0 || IsIconic(handle) != 0 }
    {
        return Ok(());
    }
    if unsafe { GdiFlush() } == 0 {
        return Err("GdiFlush failed during SCREEN SYNC".into());
    }
    let result = unsafe { DwmFlush() };
    if result != 0 {
        return Err(format!(
            "DwmFlush failed during SCREEN SYNC (HRESULT 0x{:08X})",
            result as u32
        ));
    }
    Ok(())
}
