//! Optional XWayland fullscreen surface emulation on minifb's own X11 connection.
//! Never changes a physical output: both the XWAYLAND extension and the selected
//! output's RANDR Emulation property are mandatory before any RandR mutation.

use std::ffi::{c_int, c_long};
use std::ptr;
use x11_dl::{xlib, xrandr};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Request {
    pub width: u32,
    pub height: u32,
}

impl Request {
    pub(super) const HD: Self = Self {
        width: 1280,
        height: 720,
    };
}

#[derive(Clone, Debug)]
struct Crtc {
    id: xrandr::RRCrtc,
    mode: xrandr::RRMode,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    rotation: xrandr::Rotation,
    outputs: Vec<xrandr::RROutput>,
}

#[derive(Clone, Debug)]
struct Saved {
    output: xrandr::RROutput,
    previous: Crtc,
    requested_mode: xrandr::RRMode,
    request: Request,
}

#[derive(Default)]
pub(super) struct State {
    saved: Option<Saved>,
}

struct Resources<'a> {
    api: &'a xrandr::Xrandr,
    raw: *mut xrandr::XRRScreenResources,
}

impl Drop for Resources<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.XRRFreeScreenResources)(self.raw) };
    }
}

impl<'a> Resources<'a> {
    fn get(
        api: &'a xrandr::Xrandr,
        display: *mut xlib::Display,
        root: xlib::Window,
    ) -> Result<Self, String> {
        let raw = unsafe { (api.XRRGetScreenResourcesCurrent)(display, root) };
        if raw.is_null() {
            return Err("Cannot query XWayland RandR resources".into());
        }
        Ok(Self { api, raw })
    }

    fn outputs(&self) -> Result<&[xrandr::RROutput], String> {
        unsafe { checked_slice((*self.raw).outputs, (*self.raw).noutput, 128) }
    }

    fn modes(&self) -> Result<&[xrandr::XRRModeInfo], String> {
        unsafe { checked_slice((*self.raw).modes, (*self.raw).nmode, 16384) }
    }
}

// All callers borrow arrays owned by a live Xlib allocation, before its matching
// free. Reject malformed counts and null nonempty arrays rather than fabricating
// a slice; Xlib's documented structs supply the array bounds.
unsafe fn checked_slice<'a, T>(
    data: *const T,
    count: c_int,
    limit: c_int,
) -> Result<&'a [T], String> {
    if count < 0 || count > limit || (count > 0 && data.is_null()) {
        return Err("Invalid RandR array returned by XWayland".into());
    }
    if count == 0 {
        return Ok(&[]);
    }
    Ok(unsafe { std::slice::from_raw_parts(data, count as usize) })
}

fn supports_xwayland(
    xlib: &xlib::Xlib,
    randr: &xrandr::Xrandr,
    display: *mut xlib::Display,
) -> bool {
    let (mut opcode, mut event, mut error) = (0, 0, 0);
    if unsafe {
        (xlib.XQueryExtension)(
            display,
            c"XWAYLAND".as_ptr(),
            &mut opcode,
            &mut event,
            &mut error,
        )
    } == 0
    {
        return false;
    }
    let (mut major, mut minor) = (0, 0);
    unsafe {
        (randr.XRRQueryVersion)(display, &mut major, &mut minor) != 0 && (major, minor) >= (1, 3)
    }
}

fn is_emulated_output(
    xlib: &xlib::Xlib,
    randr: &xrandr::Xrandr,
    display: *mut xlib::Display,
    output: xrandr::RROutput,
) -> Result<bool, String> {
    let atom = unsafe { (xlib.XInternAtom)(display, c"RANDR Emulation".as_ptr(), xlib::True) };
    if atom == 0 {
        return Ok(false);
    }
    let (mut kind, mut format, mut items, mut after) = (0, 0, 0, 0);
    let mut data = ptr::null_mut();
    let status = unsafe {
        (randr.XRRGetOutputProperty)(
            display,
            output,
            atom,
            0,
            1,
            xlib::False,
            xlib::False,
            xlib::XA_INTEGER,
            &mut kind,
            &mut format,
            &mut items,
            &mut after,
            &mut data,
        )
    };
    // Xlib expands format32 values to native longs even on LP64.
    let valid = status == xlib::Success as i32
        && kind == xlib::XA_INTEGER
        && format == 32
        && items == 1
        && after == 0
        && !data.is_null()
        && unsafe { *data.cast::<c_long>() } == 1;
    if !data.is_null() {
        unsafe { (xlib.XFree)(data.cast()) };
    }
    if status != xlib::Success as i32 {
        return Err(format!(
            "Cannot query RANDR Emulation property (status {status})"
        ));
    }
    Ok(valid)
}

fn crtc_info(
    randr: &xrandr::Xrandr,
    display: *mut xlib::Display,
    resources: &Resources<'_>,
    id: xrandr::RRCrtc,
) -> Result<Crtc, String> {
    let raw = unsafe { (randr.XRRGetCrtcInfo)(display, resources.raw, id) };
    if raw.is_null() {
        return Err("Cannot query the selected XWayland CRTC".into());
    }
    let result = (|| {
        let info = unsafe { &*raw };
        let outputs = unsafe { checked_slice(info.outputs, info.noutput, 128)? }.to_vec();
        Ok(Crtc {
            id,
            mode: info.mode,
            x: info.x,
            y: info.y,
            width: info.width,
            height: info.height,
            rotation: info.rotation,
            outputs,
        })
    })();
    unsafe { (randr.XRRFreeCrtcInfo)(raw) };
    result
}

fn output_info(
    randr: &xrandr::Xrandr,
    display: *mut xlib::Display,
    resources: &Resources<'_>,
    id: xrandr::RROutput,
) -> Result<Option<(xrandr::RRCrtc, Vec<xrandr::RRMode>)>, String> {
    let raw = unsafe { (randr.XRRGetOutputInfo)(display, resources.raw, id) };
    if raw.is_null() {
        return Err("Cannot query XWayland output information".into());
    }
    let result = (|| {
        let info = unsafe { &*raw };
        if info.connection != xrandr::RR_Connected as u16 || info.crtc == 0 {
            return Ok(None);
        }
        Ok(Some((
            info.crtc,
            unsafe { checked_slice(info.modes, info.nmode, 16384)? }.to_vec(),
        )))
    })();
    unsafe { (randr.XRRFreeOutputInfo)(raw) };
    result
}

fn overlap(a: (i64, i64, i64, i64), b: (i64, i64, i64, i64)) -> i64 {
    let w = (a.0.saturating_add(a.2).min(b.0.saturating_add(b.2)) - a.0.max(b.0)).max(0);
    let h = (a.1.saturating_add(a.3).min(b.1.saturating_add(b.3)) - a.1.max(b.1)).max(0);
    w.saturating_mul(h)
}

fn select_unique(areas: &[i64]) -> Result<usize, String> {
    let max = areas.iter().copied().max().unwrap_or(0);
    if max <= 0 {
        return Err("Cannot identify the window's XWayland monitor".into());
    }
    let mut indices = areas.iter().enumerate().filter(|(_, area)| **area == max);
    let selected = indices.next().unwrap().0;
    if indices.next().is_some() {
        return Err("Window overlaps multiple monitors equally; move it onto one monitor before fullscreen emulation".into());
    }
    Ok(selected)
}

// Unsupported capability is the only path that falls back to desktop fullscreen.
// Transport/protocol errors remain errors, and every mutation is outside preflight.
enum Preflight {
    Unsupported,
    Ready(Saved),
}

fn preflight(
    xlib: &xlib::Xlib,
    randr: &xrandr::Xrandr,
    display: *mut xlib::Display,
    resources: &Resources<'_>,
    geometry: (isize, isize, isize, isize),
    request: Request,
) -> Result<Preflight, String> {
    let mut candidates = Vec::new();
    let mut areas = Vec::new();
    for &output in resources.outputs()? {
        let Some((crtc_id, modes)) = output_info(randr, display, resources, output)? else {
            continue;
        };
        let crtc = crtc_info(randr, display, resources, crtc_id)?;
        let area = overlap(
            (
                geometry.0 as i64,
                geometry.1 as i64,
                geometry.2 as i64,
                geometry.3 as i64,
            ),
            (
                crtc.x as i64,
                crtc.y as i64,
                crtc.width as i64,
                crtc.height as i64,
            ),
        );
        areas.push(area);
        candidates.push((output, crtc, modes));
    }
    let selected = select_unique(&areas)?;
    let (output, previous, modes) = &candidates[selected];
    if !is_emulated_output(xlib, randr, display, *output)? {
        return Ok(Preflight::Unsupported);
    }
    let Some(mode) = supported_mode(previous, *output, modes, resources.modes()?, request) else {
        return Ok(Preflight::Unsupported);
    };
    Ok(Preflight::Ready(Saved {
        output: *output,
        previous: previous.clone(),
        requested_mode: mode,
        request,
    }))
}

fn supported_mode(
    previous: &Crtc,
    output: xrandr::RROutput,
    output_modes: &[xrandr::RRMode],
    modes: &[xrandr::XRRModeInfo],
    request: Request,
) -> Option<xrandr::RRMode> {
    if previous.rotation != xrandr::RR_Rotate_0 as u16
        || previous.outputs != [output]
        || request.width > previous.width
        || request.height > previous.height
    {
        return None;
    }
    modes
        .iter()
        .find(|mode| {
            mode.width == request.width
                && mode.height == request.height
                && output_modes.contains(&mode.id)
        })
        .map(|mode| mode.id)
}

impl State {
    pub(super) fn enable(
        &mut self,
        display: *mut xlib::Display,
        root: xlib::Window,
        geometry: (isize, isize, isize, isize),
        request: Request,
    ) -> Result<(), String> {
        if self.saved.is_some() {
            return Err("An earlier fullscreen emulation still needs restoration".into());
        }
        let xlib = xlib::Xlib::open().map_err(|e| e.to_string())?;
        let Ok(randr) = xrandr::Xrandr::open() else {
            return Ok(());
        };
        if !supports_xwayland(&xlib, &randr, display) {
            return Ok(());
        }
        let resources = Resources::get(&randr, display, root)?;
        let saved = match preflight(&xlib, &randr, display, &resources, geometry, request)? {
            Preflight::Unsupported => return Ok(()),
            Preflight::Ready(saved) => saved,
        };
        let previous = saved.previous.clone();
        let requested_mode = saved.requested_mode;
        let mut outputs = previous.outputs.clone();
        let status = unsafe {
            (randr.XRRSetCrtcConfig)(
                display,
                resources.raw,
                previous.id,
                xlib::CurrentTime,
                previous.x,
                previous.y,
                requested_mode,
                previous.rotation,
                outputs.as_mut_ptr(),
                outputs.len() as i32,
            )
        };
        if status != xrandr::RRSetConfigSuccess {
            return Err(format!(
                "XWayland rejected mode emulation (RandR status {status})"
            ));
        }
        // Persist rollback before any fallible confirmation after the mutation.
        self.saved = Some(saved);
        let resources = Resources::get(&randr, display, root)?;
        let observed = crtc_info(&randr, display, &resources, previous.id)?;
        if observed.mode != requested_mode {
            return Err("XWayland did not confirm the requested emulated mode".into());
        }
        Ok(())
    }

    pub(super) fn expected(&self) -> Option<Request> {
        self.saved.as_ref().map(|saved| saved.request)
    }

    pub(super) fn restore(
        &mut self,
        display: *mut xlib::Display,
        root: xlib::Window,
    ) -> Result<(), String> {
        let Some(saved) = self.saved.as_ref() else {
            return Ok(());
        };
        let result = (|| {
            let xlib = xlib::Xlib::open().map_err(|e| e.to_string())?;
            let randr = xrandr::Xrandr::open().map_err(|e| e.to_string())?;
            if !supports_xwayland(&xlib, &randr, display) {
                return Err("Cannot restore emulation: XWayland capability disappeared".into());
            }
            let resources = Resources::get(&randr, display, root)?;
            if !resources.outputs()?.contains(&saved.output)
                || !is_emulated_output(&xlib, &randr, display, saved.output)?
            {
                return Err(
                    "Cannot restore emulation: the original emulated output is no longer available"
                        .into(),
                );
            }
            let Some((crtc, modes)) = output_info(&randr, display, &resources, saved.output)?
            else {
                return Err("Cannot restore emulation on a disconnected output".into());
            };
            if crtc != saved.previous.id || !modes.contains(&saved.previous.mode) {
                return Err("Cannot restore emulation: the output configuration changed".into());
            }
            let current = crtc_info(&randr, display, &resources, crtc)?;
            if current.outputs != saved.previous.outputs
                || current.rotation != saved.previous.rotation
                || current.x != saved.previous.x
                || current.y != saved.previous.y
            {
                return Err("Cannot restore emulation across a changed monitor layout".into());
            }
            let mut outputs = saved.previous.outputs.clone();
            let status = unsafe {
                (randr.XRRSetCrtcConfig)(
                    display,
                    resources.raw,
                    saved.previous.id,
                    xlib::CurrentTime,
                    saved.previous.x,
                    saved.previous.y,
                    saved.previous.mode,
                    saved.previous.rotation,
                    outputs.as_mut_ptr(),
                    outputs.len() as i32,
                )
            };
            if status != xrandr::RRSetConfigSuccess {
                return Err(format!(
                    "XWayland rejected emulation restoration (RandR status {status})"
                ));
            }
            let resources = Resources::get(&randr, display, root)?;
            let observed = crtc_info(&randr, display, &resources, crtc)?;
            if observed.mode != saved.previous.mode {
                return Err("XWayland did not confirm emulation restoration".into());
            }
            Ok((observed.width, observed.height))
        })();
        if result.is_ok() {
            self.saved = None;
        }
        result.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_requires_a_unique_intersecting_monitor() {
        assert_eq!(overlap((-100, 20, 200, 80), (0, 0, 1920, 1080)), 8000);
        assert_eq!(select_unique(&[200, 900, 0]).unwrap(), 1);
        assert!(select_unique(&[900, 900]).is_err());
        assert!(select_unique(&[0, 0]).is_err());
        assert!(select_unique(&[]).is_err());
    }

    #[test]
    fn mode_preflight_requires_an_advertised_safe_configuration() {
        let mut crtc = Crtc {
            id: 5,
            mode: 6,
            x: 0,
            y: 0,
            width: 3840,
            height: 2160,
            rotation: xrandr::RR_Rotate_0 as u16,
            outputs: vec![7],
        };
        let mut hd: xrandr::XRRModeInfo = unsafe { std::mem::zeroed() };
        hd.id = 10;
        hd.width = 1280;
        hd.height = 720;
        let mut wrong: xrandr::XRRModeInfo = unsafe { std::mem::zeroed() };
        wrong.id = 11;
        wrong.width = 1280;
        wrong.height = 768;
        let modes = [hd, wrong];
        assert_eq!(
            supported_mode(&crtc, 7, &[10], &modes, Request::HD),
            Some(10)
        );
        assert_eq!(supported_mode(&crtc, 7, &[11], &modes, Request::HD), None);
        assert_eq!(supported_mode(&crtc, 7, &[], &modes, Request::HD), None);
        crtc.width = 640;
        assert_eq!(supported_mode(&crtc, 7, &[10], &modes, Request::HD), None);
        crtc.width = 3840;
        crtc.rotation = xrandr::RR_Rotate_90 as u16;
        assert_eq!(supported_mode(&crtc, 7, &[10], &modes, Request::HD), None);
        crtc.rotation = xrandr::RR_Rotate_0 as u16;
        crtc.outputs.push(8);
        assert_eq!(supported_mode(&crtc, 7, &[10], &modes, Request::HD), None);
    }

    #[test]
    fn randr_arrays_reject_invalid_bounds_and_accept_empty_null() {
        unsafe {
            assert!(checked_slice::<u32>(ptr::null(), 1, 128).is_err());
            assert!(checked_slice::<u32>(ptr::null(), -1, 128).is_err());
            assert!(checked_slice::<u32>(ptr::null(), 129, 128).is_err());
            assert!(checked_slice::<u32>(ptr::null(), 0, 128)
                .unwrap()
                .is_empty());
            let values = [1u32, 2];
            assert_eq!(checked_slice(values.as_ptr(), 2, 128).unwrap(), &values);
        }
    }
}
