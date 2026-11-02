//! X11 synchronized presentation with XRender nearest scaling.
//!
//! minifb retains ownership of the native window and its input/event connection.
//! This connection owns only pixmaps, pictures, a GC and event subscription. The
//! server releases all of them when the connection closes, including after a
//! timeout; a pixmap awaiting Idle is never overwritten or reused.

use super::PresentationViewport;
use minifb::Window;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::present::{self, CompleteKind, CompleteNotifyEvent};
use x11rb::protocol::render;
use x11rb::protocol::xproto::{
    ConnectionExt, CreateGCAux, ImageFormat, ImageOrder, MapState, Rectangle, VisualClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

const EVENT_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_PIXELS: usize = 64 * 1024 * 1024;
// A bounded upload chunk also works without BIG-REQUESTS. Four extra bytes
// reserve the extended request length if a server allows a larger request.
const PUT_IMAGE_HEADER_BYTES: usize = 28;
const MAX_UPLOAD_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
struct RenderState {
    format: render::Pictformat,
    source_pixmap: u32,
    source_picture: u32,
    destination_picture: u32,
    logical: (usize, usize),
    viewport: Option<PresentationViewport>,
}

impl RenderState {
    fn new(conn: &RustConnection, window: u32) -> Result<Self, String> {
        let version = render::query_version(conn, 0, 10)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        if version.major_version == 0 && version.minor_version < 10 {
            return Err("XRender scaler requires Render 0.10 (nearest + edge padding)".into());
        }
        let formats = render::query_pict_formats(conn)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        let format = formats
            .formats
            .iter()
            .find(|format| {
                format.type_ == render::PictType::DIRECT
                    && format.depth == 24
                    && format.direct.red_shift == 16
                    && format.direct.red_mask == 255
                    && format.direct.green_shift == 8
                    && format.direct.green_mask == 255
                    && format.direct.blue_shift == 0
                    && format.direct.blue_mask == 255
                    && format.direct.alpha_mask == 0
            })
            .ok_or_else(|| {
                "XRender scaler requires an opaque 24-bit RGB picture format".to_string()
            })?
            .id;
        let filters = render::query_filters(conn, window)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        if !filters
            .filters
            .iter()
            .any(|filter| filter.name == b"nearest")
        {
            return Err("XRender nearest filter is unavailable".into());
        }
        Ok(Self {
            format,
            source_pixmap: 0,
            source_picture: 0,
            destination_picture: 0,
            logical: (0, 0),
            viewport: None,
        })
    }
}

pub(super) struct LinuxPresenter {
    conn: RustConnection,
    window: u32,
    event_id: u32,
    gc: u32,
    pixmap: u32,
    size: (usize, usize),
    serial: u32,
    pending: bool,
    little_endian: bool,
    max_request_bytes: usize,
    pixels: Vec<u8>,
    render: Option<RenderState>,
}

impl std::fmt::Debug for LinuxPresenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxPresenter")
            .field("window", &self.window)
            .field("size", &self.size)
            .field("pending", &self.pending)
            .field(
                "scaler",
                &if self.render.is_some() {
                    "xrender"
                } else {
                    "cpu"
                },
            )
            .finish()
    }
}

fn describe(error: impl std::fmt::Display) -> String {
    format!("X11 Present: {error}")
}

/// Order earlier minifb XPutImage requests before a submission on our separate
/// connection. Call only after a minifb blit, not for every synchronized frame.
/// XSync with discard=false retains all pending minifb input events.
pub(super) fn sync_minifb(window: &Window) -> Result<(), String> {
    let display = match window.display_handle().map_err(describe)?.as_raw() {
        RawDisplayHandle::Xlib(handle) => handle
            .display
            .ok_or_else(|| "X11 Present cannot access minifb's Xlib display".to_string())?
            .as_ptr()
            .cast(),
        _ => return Err("X11 Present ordering requires minifb's Xlib display".into()),
    };
    let xlib = x11_dl::xlib::Xlib::open().map_err(describe)?;
    // SAFETY: minifb owns this live display for the lifetime of `window`.
    // Discard is false: do not consume or erase its queued input events.
    unsafe { (xlib.XSync)(display, 0) };
    Ok(())
}

impl LinuxPresenter {
    pub(super) fn new(window: &Window) -> Result<Self, String> {
        let native_window = match window.window_handle().map_err(describe)?.as_raw() {
            RawWindowHandle::Xlib(handle) => u32::try_from(handle.window).map_err(describe)?,
            RawWindowHandle::Xcb(handle) => handle.window.get(),
            _ => return Err("X11 Present requires an X11 graphics window".into()),
        };
        let (conn, _) = RustConnection::connect(None).map_err(describe)?;
        let version = present::query_version(&conn, 1, 0)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        if version.major_version < 1 {
            return Err("X11 Present version 1.0 is unavailable".into());
        }
        let attributes = conn
            .get_window_attributes(native_window)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        let geometry = conn
            .get_geometry(native_window)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        let visual = conn
            .setup()
            .roots
            .iter()
            .flat_map(|screen| &screen.allowed_depths)
            .flat_map(|depth| &depth.visuals)
            .find(|visual| visual.visual_id == attributes.visual)
            .ok_or_else(|| "X11 Present cannot identify the window visual".to_string())?;
        let format = conn
            .setup()
            .pixmap_formats
            .iter()
            .find(|format| format.depth == geometry.depth)
            .ok_or_else(|| "X11 Present cannot identify the pixmap format".to_string())?;
        if geometry.depth != 24
            || format.bits_per_pixel != 32
            || format.scanline_pad != 32
            || visual.class != VisualClass::TRUE_COLOR
            || visual.red_mask != 0x00ff0000
            || visual.green_mask != 0x0000ff00
            || visual.blue_mask != 0x000000ff
        {
            return Err("X11 Present requires 24-bit TrueColor RGB with 32-bit pixels".into());
        }
        let little_endian = match conn.setup().image_byte_order {
            ImageOrder::LSB_FIRST => true,
            ImageOrder::MSB_FIRST => false,
            _ => return Err("X11 Present has an unsupported image byte order".into()),
        };
        // Learn the real request limit once, outside the frame loop.
        let max_request_bytes = conn.maximum_request_bytes().min(MAX_UPLOAD_REQUEST_BYTES);
        let render = Some(RenderState::new(&conn, native_window)?);
        let event_id = conn.generate_id().map_err(describe)?;
        present::select_input(
            &conn,
            event_id,
            native_window,
            present::EventMask::COMPLETE_NOTIFY | present::EventMask::IDLE_NOTIFY,
        )
        .map_err(describe)?
        .check()
        .map_err(describe)?;
        let gc = conn.generate_id().map_err(describe)?;
        conn.create_gc(gc, native_window, &CreateGCAux::new().graphics_exposures(0))
            .map_err(describe)?
            .check()
            .map_err(describe)?;
        Ok(Self {
            conn,
            window: native_window,
            event_id,
            gc,
            pixmap: 0,
            size: (0, 0),
            serial: 0,
            pending: false,
            little_endian,
            max_request_bytes,
            pixels: Vec::new(),
            render,
        })
    }

    pub(super) fn present(
        &mut self,
        buffer: &[u32],
        logical: (usize, usize),
        physical: (usize, usize),
    ) -> Result<(), String> {
        if self.pending {
            return Err("X11 Present cannot reuse a pixmap whose completion is unknown".into());
        }
        if physical.0 == 0 || physical.1 == 0 {
            return Ok(());
        }
        // Query map state instead of interpreting keyboard focus as visibility:
        // a full-screen window on another monitor may legitimately be unfocused.
        let attributes = self
            .conn
            .get_window_attributes(self.window)
            .map_err(describe)?
            .reply()
            .map_err(describe)?;
        if attributes.map_state != MapState::VIEWABLE {
            return Ok(());
        }
        self.upload_frame(buffer, logical, physical)?;
        self.serial = self.serial.wrapping_add(1);
        if self.serial == 0 {
            self.serial = 1;
        }
        // COPY guarantees that an idle buffer is not retained as a scanout
        // buffer. No ASYNC flag and no synthetic target-MSC offsets are used.
        self.pending = true;
        present::pixmap(
            &self.conn,
            self.window,
            self.pixmap,
            self.serial,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            present::Option::COPY.into(),
            0,
            0,
            0,
            &[],
        )
        .map_err(describe)?;
        self.conn.flush().map_err(describe)?;
        self.wait_complete_and_idle(Instant::now() + EVENT_TIMEOUT)?;
        self.pending = false;
        Ok(())
    }

    fn upload_frame(
        &mut self,
        buffer: &[u32],
        logical: (usize, usize),
        physical: (usize, usize),
    ) -> Result<&'static str, String> {
        validate_dimensions(buffer.len(), logical, physical)?;
        if self.pending {
            return Err("X11 Present cannot overwrite a pending frame".into());
        }
        self.ensure_pixmap(physical)?;
        if logical.0 > i16::MAX as usize || logical.1 > i16::MAX as usize {
            return Err("XRender logical dimensions exceed the supported X11 image range".into());
        }
        self.ensure_render_source(logical)?;
        // Only logical pixels cross the client/server connection. The
        // opaque RGB picture format ignores the unused high byte.
        let source = &buffer[..logical.0 * logical.1];
        let pixmap = self.render.as_ref().unwrap().source_pixmap;
        let strategy = if let Some(bytes) = native_image_bytes(source, self.little_endian) {
            // PutImage writes these bytes or copies them into its own send
            // buffer before returning; its cookie retains no framebuffer borrow.
            self.upload_image(pixmap, logical, bytes)?;
            "borrowed"
        } else {
            encode_rgb(source, self.little_endian, &mut self.pixels);
            self.upload_image(pixmap, logical, &self.pixels)?;
            "converted"
        };
        self.composite_frame(logical, physical)?;
        Ok(strategy)
    }

    fn upload_image(&self, pixmap: u32, size: (usize, usize), pixels: &[u8]) -> Result<(), String> {
        let rows_per_request = upload_rows(size.0, self.max_request_bytes)?;
        let row_bytes = size.0 * 4;
        for y in (0..size.1).step_by(rows_per_request) {
            let rows = rows_per_request.min(size.1 - y);
            self.conn
                .put_image(
                    ImageFormat::Z_PIXMAP,
                    pixmap,
                    self.gc,
                    size.0 as u16,
                    rows as u16,
                    0,
                    y as i16,
                    0,
                    24,
                    &pixels[y * row_bytes..(y + rows) * row_bytes],
                )
                .map_err(describe)?;
        }
        Ok(())
    }

    fn ensure_render_source(&mut self, logical: (usize, usize)) -> Result<(), String> {
        let render = self.render.as_mut().unwrap();
        if render.logical == logical {
            return Ok(());
        }
        let pixmap = self.conn.generate_id().map_err(describe)?;
        let picture = self.conn.generate_id().map_err(describe)?;
        self.conn
            .create_pixmap(24, pixmap, self.window, logical.0 as u16, logical.1 as u16)
            .map_err(describe)?
            .check()
            .map_err(describe)?;
        render::create_picture(
            &self.conn,
            picture,
            pixmap,
            render.format,
            &render::CreatePictureAux::new().repeat(render::Repeat::PAD),
        )
        .map_err(describe)?
        .check()
        .map_err(describe)?;
        render::set_picture_filter(&self.conn, picture, b"nearest", &[])
            .map_err(describe)?
            .check()
            .map_err(describe)?;
        if render.source_picture != 0 {
            render::free_picture(&self.conn, render.source_picture).map_err(describe)?;
            self.conn
                .free_pixmap(render.source_pixmap)
                .map_err(describe)?;
        }
        render.source_pixmap = pixmap;
        render.source_picture = picture;
        render.logical = logical;
        render.viewport = None;
        Ok(())
    }

    fn composite_frame(
        &mut self,
        logical: (usize, usize),
        physical: (usize, usize),
    ) -> Result<(), String> {
        let viewport = PresentationViewport::fit(logical, physical)
            .ok_or_else(|| "XRender has no visible viewport".to_string())?;
        let render = self.render.as_mut().unwrap();
        if render.viewport != Some(viewport) {
            // Margins are deterministic black. Reclear on viewport changes so
            // a previous wider image cannot survive in a newly exposed margin.
            self.conn
                .poly_fill_rectangle(
                    self.pixmap,
                    self.gc,
                    &[Rectangle {
                        x: 0,
                        y: 0,
                        width: physical.0 as u16,
                        height: physical.1 as u16,
                    }],
                )
                .map_err(describe)?;
            render::set_picture_transform(
                &self.conn,
                render.source_picture,
                nearest_transform(logical, (viewport.width, viewport.height))?,
            )
            .map_err(describe)?;
            render.viewport = Some(viewport);
        }
        // XRender's conventional pixel-centre nearest sampling is intentional.
        // It is sharp, but at fractional scales its source-pixel boundaries can
        // differ from minifb's floor-at-pixel-edge convention. No per-resolution
        // compensation, projective matrix or interpolation is introduced.
        render::composite(
            &self.conn,
            render::PictOp::SRC,
            render.source_picture,
            0u32,
            render.destination_picture,
            0,
            0,
            0,
            0,
            viewport.x as i16,
            viewport.y as i16,
            viewport.width as u16,
            viewport.height as u16,
        )
        .map_err(describe)?;
        Ok(())
    }

    fn ensure_pixmap(&mut self, physical: (usize, usize)) -> Result<(), String> {
        if self.size == physical {
            return Ok(());
        }
        let next = self.conn.generate_id().map_err(describe)?;
        self.conn
            .create_pixmap(24, next, self.window, physical.0 as u16, physical.1 as u16)
            .map_err(describe)?
            .check()
            .map_err(describe)?;
        if let Some(render) = &mut self.render {
            let picture = self.conn.generate_id().map_err(describe)?;
            render::create_picture(
                &self.conn,
                picture,
                next,
                render.format,
                &render::CreatePictureAux::new(),
            )
            .map_err(describe)?
            .check()
            .map_err(describe)?;
            if render.destination_picture != 0 {
                render::free_picture(&self.conn, render.destination_picture).map_err(describe)?;
            }
            render.destination_picture = picture;
            render.viewport = None;
        }
        if self.pixmap != 0 {
            self.conn
                .free_pixmap(self.pixmap)
                .map_err(describe)?
                .check()
                .map_err(describe)?;
        }
        self.pixmap = next;
        self.size = physical;
        Ok(())
    }

    fn wait_complete_and_idle(&self, deadline: Instant) -> Result<CompleteNotifyEvent, String> {
        let mut complete = None;
        let mut idle = false;
        loop {
            if Instant::now() >= deadline {
                return Err(
                    "X11 Present timed out after 250 ms waiting for Complete and Idle".into(),
                );
            }
            while let Some(event) = self.conn.poll_for_event().map_err(describe)? {
                match event {
                    Event::PresentCompleteNotify(event)
                        if event.event == self.event_id
                            && event.window == self.window
                            && event.serial == self.serial
                            && event.kind == CompleteKind::PIXMAP =>
                    {
                        complete = Some(event)
                    }
                    Event::PresentIdleNotify(event)
                        if event.event == self.event_id
                            && event.window == self.window
                            && event.serial == self.serial
                            && event.pixmap == self.pixmap =>
                    {
                        idle = true
                    }
                    Event::Error(error) => {
                        return Err(format!("X11 Present protocol error: {error:?}"))
                    }
                    _ => {}
                }
                if idle {
                    if let Some(event) = complete {
                        return Ok(event);
                    }
                }
                if Instant::now() >= deadline {
                    return Err("X11 Present timed out while draining its event queue".into());
                }
            }
            wait_readable(self.conn.stream().as_raw_fd(), deadline)?;
        }
    }
}

fn nearest_transform(
    logical: (usize, usize),
    viewport: (usize, usize),
) -> Result<render::Transform, String> {
    fn ratio(source: usize, destination: usize) -> Result<i32, String> {
        if source == 0 || destination == 0 {
            return Err("XRender cannot scale an empty image".into());
        }
        let value = ((source as u64) * 65536 + destination as u64 / 2) / destination as u64;
        i32::try_from(value)
            .ok()
            .filter(|&value| value > 0)
            .ok_or_else(|| "XRender scaling ratio exceeds 16.16 coordinates".to_string())
    }
    Ok(render::Transform {
        matrix11: ratio(logical.0, viewport.0)?,
        matrix12: 0,
        matrix13: 0,
        matrix21: 0,
        matrix22: ratio(logical.1, viewport.1)?,
        matrix23: 0,
        matrix31: 0,
        matrix32: 0,
        matrix33: 65536,
    })
}

fn native_image_bytes(buffer: &[u32], little_endian: bool) -> Option<&[u8]> {
    if little_endian != cfg!(target_endian = "little") {
        return None;
    }
    // SAFETY: u32 has no padding or uninitialized bytes; u8 accepts every bit
    // pattern and has alignment 1. The returned immutable slice borrows the
    // original storage and lifetime, and size_of_val covers exactly that slice.
    // The validated X11 depth24 RGB visual and opaque Render picture ignore
    // the high byte; no alpha channel is exposed by this borrowed representation.
    Some(unsafe {
        std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), std::mem::size_of_val(buffer))
    })
}

fn encode_rgb(buffer: &[u32], little_endian: bool, pixels: &mut Vec<u8>) {
    pixels.resize(buffer.len() * 4, 0);
    for (value, destination) in buffer.iter().zip(pixels.chunks_exact_mut(4)) {
        let value = value & 0x00ff_ffff;
        destination.copy_from_slice(&if little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        });
    }
}

// Dropping the private connection uses X11's default DestroyAll close-down
// mode to release our pixmap, GC and event subscription. It never destroys the
// minifb window. Avoid synchronous cleanup requests after a failed server wait.

fn wait_readable(fd: std::os::fd::RawFd, deadline: Instant) -> Result<(), String> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("X11 Present event wait timed out".into());
        }
        let timeout_ms = remaining
            .as_millis()
            .saturating_add(1)
            .min(i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one initialized pollfd; the connection owns a live descriptor.
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(describe(error));
        }
        if result == 0 {
            return Err("X11 Present event wait timed out after 250 ms".into());
        }
        if descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err("X11 Present connection closed while waiting for a frame".into());
        }
        if descriptor.revents & libc::POLLIN != 0 {
            return Ok(());
        }
    }
}

fn validate_dimensions(
    buffer_len: usize,
    logical: (usize, usize),
    physical: (usize, usize),
) -> Result<(), String> {
    let logical_pixels = logical
        .0
        .checked_mul(logical.1)
        .filter(|&n| n > 0 && n <= MAX_PIXELS);
    let physical_pixels = physical
        .0
        .checked_mul(physical.1)
        .filter(|&n| n > 0 && n <= MAX_PIXELS);
    if logical_pixels.is_none()
        || physical_pixels.is_none()
        || logical_pixels.unwrap_or(usize::MAX) > buffer_len
        || physical.0 > i16::MAX as usize
        || physical.1 > i16::MAX as usize
    {
        return Err("X11 Present buffer dimensions are unsupported or inconsistent".into());
    }
    Ok(())
}

fn upload_rows(width: usize, max_request_bytes: usize) -> Result<usize, String> {
    let row_bytes = width
        .checked_mul(4)
        .filter(|&bytes| bytes > 0)
        .ok_or_else(|| "X11 Present image row is invalid".to_string())?;
    let payload = max_request_bytes
        .checked_sub(PUT_IMAGE_HEADER_BYTES)
        .ok_or_else(|| "X11 Present request limit is too small".to_string())?;
    let rows = payload / row_bytes;
    if rows == 0 {
        return Err("X11 Present image row exceeds the server request limit".into());
    }
    Ok(rows.min(u16::MAX as usize))
}

#[cfg(test)]
fn scale_rgb(
    buffer: &[u32],
    logical: (usize, usize),
    physical: (usize, usize),
    little_endian: bool,
    pixels: &mut Vec<u8>,
) -> Result<(), String> {
    validate_dimensions(buffer.len(), logical, physical)?;
    let viewport = PresentationViewport::fit(logical, physical)
        .ok_or_else(|| "X11 Present has no visible viewport".to_string())?;
    pixels.resize(physical.0 * physical.1 * 4, 0);
    pixels.fill(0);
    let source_x: Vec<usize> = (0..viewport.width)
        .map(|x| x * logical.0 / viewport.width)
        .collect();
    let mut previous_source_y = None;
    for y in 0..viewport.height {
        let source_y = y * logical.1 / viewport.height;
        let start = ((y + viewport.y) * physical.0 + viewport.x) * 4;
        if previous_source_y == Some(source_y) {
            let previous_start = start - physical.0 * 4;
            pixels.copy_within(previous_start..previous_start + viewport.width * 4, start);
        } else {
            let source_row = &buffer[source_y * logical.0..(source_y + 1) * logical.0];
            for (x, &source_x) in source_x.iter().enumerate() {
                let value = source_row[source_x] & 0x00ff_ffff;
                let bytes = if little_endian {
                    value.to_le_bytes()
                } else {
                    value.to_be_bytes()
                };
                pixels[start + x * 4..start + x * 4 + 4].copy_from_slice(&bytes);
            }
        }
        previous_source_y = Some(source_y);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_present_native_image_bytes_borrows_exact_storage() {
        let input = [0x00112233u32, 0xff445566, 0x80778899];
        let source = &input[1..2];
        let bytes = native_image_bytes(source, cfg!(target_endian = "little")).unwrap();
        assert_eq!(bytes.as_ptr(), source.as_ptr().cast::<u8>());
        assert_eq!(bytes.len(), std::mem::size_of_val(source));
        assert_eq!(bytes, &input[1].to_ne_bytes());
        // The unused high byte is borrowed, not scanned/masked in another pass.
        assert_eq!(u32::from_ne_bytes(bytes.try_into().unwrap()), 0xff445566);
        assert!(native_image_bytes(&[], cfg!(target_endian = "little"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn linux_present_opposite_endian_keeps_explicit_conversion_fallback() {
        let input = [0x00112233u32, 0xff445566, 0x80778899];
        let server_little = !cfg!(target_endian = "little");
        assert!(native_image_bytes(&input, server_little).is_none());
        let mut converted = Vec::new();
        encode_rgb(&input, server_little, &mut converted);
        assert_ne!(converted.as_ptr(), input.as_ptr().cast::<u8>());
        let expected: Vec<u8> = input
            .iter()
            .flat_map(|pixel| {
                let rgb = pixel & 0x00ff_ffff;
                if server_little {
                    rgb.to_le_bytes()
                } else {
                    rgb.to_be_bytes()
                }
            })
            .collect();
        assert_eq!(converted, expected);
    }

    // Test-only reference to the exact scaler shipped in the pinned minifb
    // dependency. Production code relies exclusively on public APIs.
    extern "C" {
        fn image_resize_linear_aspect_fill(
            dst: *mut u32,
            dw: u32,
            dh: u32,
            src: *const u32,
            sw: u32,
            sh: u32,
            stride: u32,
            background: u32,
        );
    }

    #[test]
    fn linux_present_matches_minifb_pixels_and_odd_margins() {
        for (logical, physical) in [
            ((4, 3), (9, 9)),
            ((7, 3), (13, 11)),
            ((13, 11), (7, 3)),
            ((640, 360), (960, 540)),
            ((960, 540), (1280, 720)),
            ((1280, 720), (1920, 1200)),
            ((1024, 768), (1365, 767)),
            ((1, 1), (3, 7)),
            ((1921, 2), (3, 9)),
        ] {
            let input: Vec<u32> = (0..logical.0 * logical.1)
                .map(|n| ((n as u32).wrapping_mul(0x125e93)) & 0x00ff_ffff)
                .collect();
            let mut expected = vec![0u32; physical.0 * physical.1];
            // SAFETY: buffers match dimensions and the dependency's documented C layout.
            unsafe {
                image_resize_linear_aspect_fill(
                    expected.as_mut_ptr(),
                    physical.0 as u32,
                    physical.1 as u32,
                    input.as_ptr(),
                    logical.0 as u32,
                    logical.1 as u32,
                    logical.0 as u32,
                    0,
                );
            }
            let mut actual = Vec::new();
            scale_rgb(&input, logical, physical, true, &mut actual).unwrap();
            for (position, (bytes, expected)) in actual.chunks_exact(4).zip(expected).enumerate() {
                assert_eq!(
                    u32::from_le_bytes(bytes.try_into().unwrap()),
                    expected,
                    "logical={logical:?} physical={physical:?} pixel={position}"
                );
            }
        }
    }

    #[test]
    fn linux_present_rgb_byte_order_and_unused_alpha() {
        let source = [0xf0123456, 0xffabcdef];
        let mut bytes = Vec::new();
        scale_rgb(&source, (2, 1), (2, 1), true, &mut bytes).unwrap();
        assert_eq!(bytes, [0x56, 0x34, 0x12, 0, 0xef, 0xcd, 0xab, 0]);
        scale_rgb(&source, (2, 1), (2, 1), false, &mut bytes).unwrap();
        assert_eq!(bytes, [0, 0x12, 0x34, 0x56, 0, 0xab, 0xcd, 0xef]);
    }

    #[test]
    fn linux_present_uploads_fit_request_boundaries() {
        for width in [1, 640, 1280, 3840, 8192] {
            for limit in [65_536, 262_140, 1_048_576] {
                let rows = upload_rows(width, limit).unwrap();
                assert!(rows * width * 4 + PUT_IMAGE_HEADER_BYTES <= limit);
                assert!(
                    (rows + 1) * width * 4 + PUT_IMAGE_HEADER_BYTES > limit
                        || rows == u16::MAX as usize
                );
            }
        }
        assert!(upload_rows(100, 400).is_err());
        assert!(upload_rows(0, 65_536).is_err());
        assert!(upload_rows(usize::MAX, 65_536).is_err());
    }

    #[test]
    fn linux_present_rejects_invalid_and_excessive_buffers() {
        assert!(validate_dimensions(4, (2, 2), (10, 10)).is_ok());
        assert!(validate_dimensions(3, (2, 2), (10, 10)).is_err());
        assert!(validate_dimensions(4, (0, 2), (10, 10)).is_err());
        assert!(validate_dimensions(4, (2, 2), (0, 10)).is_err());
        assert!(validate_dimensions(4, (2, 2), (65535, 10)).is_err());
        assert!(validate_dimensions(4, (2, 2), (32767, 32767)).is_err());
        assert!(validate_dimensions(4, (usize::MAX, 2), (10, 10)).is_err());
    }

    #[test]
    fn linux_xrender_affine_contract() {
        let transform = nearest_transform((1280, 720), (6144, 3456)).unwrap();
        assert_eq!(transform.matrix11, 13653);
        assert_eq!(transform.matrix22, 13653);
        assert_eq!(transform.matrix33, 65536);
        assert_eq!(transform.matrix13, 0);
        assert_eq!(transform.matrix23, 0);
        assert!(nearest_transform((0, 1), (10, 10)).is_err());
        assert!(nearest_transform((1, 1), (0, 10)).is_err());
        for multiple in [1, 2, 3, 4] {
            let t = nearest_transform((16, 9), (16 * multiple, 9 * multiple)).unwrap();
            for x in 0..16 * multiple {
                assert_eq!(render_sample_index(x, t.matrix11, 16).0, x / multiple);
            }
        }
    }

    // Pixman's nearest convention evaluates destination pixel centres and
    // resolves an exact boundary toward the north/west pixel. Hardware paths
    // may resolve a boundary differently; only that one-pixel tie is permitted
    // by the native comparison below. This is deliberately not minifb's edge
    // sampling model.
    fn render_sample_index(pixel: usize, step: i32, source_size: usize) -> (usize, bool) {
        let value = ((2 * pixel as i64 + 1) * i64::from(step)) / 2;
        let result = ((value - 1) >> 16).clamp(0, source_size as i64 - 1) as usize;
        let fraction = value.rem_euclid(65536);
        (result, fraction <= 1 || fraction >= 65535)
    }

    #[test]
    #[ignore = "Opt-in XRender server readback; creates one small test window and private pixmaps"]
    fn linux_xrender_native_readback_geometry_and_sampling() {
        use minifb::{Scale, WindowOptions};
        let mut window = Window::new(
            "AVL XRender pixel validation",
            321,
            217,
            WindowOptions {
                scale: Scale::X1,
                resize: false,
                ..WindowOptions::default()
            },
        )
        .unwrap();
        window.set_target_fps(0);
        std::thread::sleep(Duration::from_millis(250));
        window.update();
        sync_minifb(&window).unwrap();
        let mut presenter = LinuxPresenter::new(&window).unwrap();
        let cases = [
            ((16, 9), (16, 9)),
            ((17, 7), (34, 14)),
            ((4, 3), (12, 9)),
            ((16, 9), (321, 217)),
            ((4, 3), (321, 217)),
            ((1280, 720), (6144, 3456)),
            ((640, 480), (1024, 768)),
            ((1280, 720), (641, 477)),
            ((640, 480), (321, 217)),
        ];
        for (logical, physical) in cases {
            let input: Vec<u32> = (0..logical.1)
                .flat_map(|y| {
                    (0..logical.0)
                        .map(move |x| 0xff00_0000 | ((y as u32 + 1) << 12) | (x as u32 + 1))
                })
                .collect();
            // The large result remains an offscreen pixmap: never read outside
            // the small test window or depend on desktop dimensions.
            presenter.upload_frame(&input, logical, physical).unwrap();
            let reply = presenter
                .conn
                .get_image(
                    ImageFormat::Z_PIXMAP,
                    presenter.pixmap,
                    0,
                    0,
                    physical.0 as u16,
                    physical.1 as u16,
                    0x00ff_ffff,
                )
                .unwrap()
                .reply()
                .unwrap();
            let viewport = PresentationViewport::fit(logical, physical).unwrap();
            let transform = nearest_transform(logical, (viewport.width, viewport.height)).unwrap();
            let mut cpu_difference = 0usize;
            let mut allowed_ties = 0usize;
            assert_eq!(reply.data.len(), physical.0 * physical.1 * 4);
            for y in 0..physical.1 {
                for x in 0..physical.0 {
                    let offset = (y * physical.0 + x) * 4;
                    let bytes = reply.data[offset..offset + 4].try_into().unwrap();
                    let actual = (if presenter.little_endian {
                        u32::from_le_bytes(bytes)
                    } else {
                        u32::from_be_bytes(bytes)
                    }) & 0x00ff_ffff;
                    if x < viewport.x
                        || x >= viewport.x + viewport.width
                        || y < viewport.y
                        || y >= viewport.y + viewport.height
                    {
                        assert_eq!(
                            actual, 0,
                            "nonblack margin logical={logical:?} physical={physical:?} at{x},{y}"
                        );
                        continue;
                    }
                    let vx = x - viewport.x;
                    let vy = y - viewport.y;
                    // Decode this coordinate pattern to check sampling and
                    // opaque boundaries. A separate palette test excludes
                    // blended colors, which a coordinate gradient alone cannot.
                    let sx = (actual & 4095)
                        .checked_sub(1)
                        .expect("black/blended XRender pixel")
                        as usize;
                    let sy = (actual >> 12)
                        .checked_sub(1)
                        .expect("black/blended XRender pixel")
                        as usize;
                    assert!(
                        sx < logical.0 && sy < logical.1,
                        "XRender color is not an original source pixel"
                    );
                    let (expected_x, tie_x) =
                        render_sample_index(vx, transform.matrix11, logical.0);
                    let (expected_y, tie_y) =
                        render_sample_index(vy, transform.matrix22, logical.1);
                    let valid_x = sx == expected_x || (tie_x && sx.abs_diff(expected_x) <= 1);
                    let valid_y = sy == expected_y || (tie_y && sy.abs_diff(expected_y) <= 1);
                    assert!(valid_x && valid_y,
                        "unexpected nearest sample logical={logical:?} physical={physical:?} at{x},{y}: {sx},{sy} expected{expected_x},{expected_y}");
                    allowed_ties += usize::from(sx != expected_x || sy != expected_y);
                    cpu_difference += usize::from(
                        sx != vx * logical.0 / viewport.width
                            || sy != vy * logical.1 / viewport.height,
                    );
                }
            }
            println!("XRender native RGB/margins PASS {logical:?}->{physical:?}; minifb boundary differences={cpu_difference}; boundary ties={allowed_ties}");
        }
        drop(presenter);
        drop(window);
    }

    #[test]
    #[ignore = "Opt-in XRender palette readback; owns one small window and pixmaps"]
    fn linux_xrender_native_nearest_palette_has_no_blended_colors() {
        use minifb::{Scale, WindowOptions};
        let mut window = Window::new(
            "AVL XRender nearest palette validation",
            321,
            217,
            WindowOptions {
                scale: Scale::X1,
                resize: false,
                ..WindowOptions::default()
            },
        )
        .unwrap();
        window.set_target_fps(0);
        std::thread::sleep(Duration::from_millis(250));
        window.update();
        sync_minifb(&window).unwrap();
        let mut presenter = LinuxPresenter::new(&window).unwrap();
        let palette = [0x00ff0000u32, 0x0000ff00, 0x000000ff];
        let logical = (17, 13);
        let physical = (321, 217);
        // Depth24/RGB pictures must ignore all values of the unused high byte,
        // including when PutImage receives the original words without masking.
        let input: Vec<u32> = (0..logical.1)
            .flat_map(|y| {
                (0..logical.0).map(move |x| {
                    palette[(x + y) % palette.len()] | ((((x * 31 + y * 47) as u32) & 255) << 24)
                })
            })
            .collect();
        let strategy = presenter.upload_frame(&input, logical, physical).unwrap();
        assert_eq!(
            strategy,
            if presenter.little_endian == cfg!(target_endian = "little") {
                "borrowed"
            } else {
                "converted"
            }
        );
        let reply = presenter
            .conn
            .get_image(
                ImageFormat::Z_PIXMAP,
                presenter.pixmap,
                0,
                0,
                physical.0 as u16,
                physical.1 as u16,
                0x00ff_ffff,
            )
            .unwrap()
            .reply()
            .unwrap();
        let viewport = PresentationViewport::fit(logical, physical).unwrap();
        let mut color_counts = [0usize; 3];
        for y in 0..physical.1 {
            for x in 0..physical.0 {
                let offset = (y * physical.0 + x) * 4;
                let bytes = reply.data[offset..offset + 4].try_into().unwrap();
                let value = (if presenter.little_endian {
                    u32::from_le_bytes(bytes)
                } else {
                    u32::from_be_bytes(bytes)
                }) & 0x00ff_ffff;
                if x < viewport.x
                    || x >= viewport.x + viewport.width
                    || y < viewport.y
                    || y >= viewport.y + viewport.height
                {
                    assert_eq!(value, 0, "palette test margin is not black at{x},{y}");
                } else {
                    let color = palette
                        .iter()
                        .position(|&color| color == value)
                        .unwrap_or_else(|| {
                            panic!("nearest produced a blended/outside color {value:06x} at{x},{y}")
                        });
                    color_counts[color] += 1;
                }
            }
        }
        assert!(color_counts.iter().all(|&count| count > 0));
        println!("XRender three-color nearest palette PASS upload={strategy}: counts={color_counts:?}; high bytes ignored, zero blended colors, black margins");
        drop(presenter);
        drop(window);
    }
}
