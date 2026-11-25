use super::{Graphics, Texture};
use std::cell::RefCell;
use std::ops::Range;

// A one-byte per channel-pair table pays for itself on large blended surfaces.
// Keep the last opacity for subsequent draws; ordinary drawing allocates nothing.
thread_local! {
    static OPACITY: RefCell<OpacityTable> = RefCell::default();
}

#[derive(Default)]
struct OpacityTable {
    alpha: Option<f64>,
    channels: Vec<u8>,
}

impl OpacityTable {
    fn for_alpha(&mut self, alpha: f64) -> &[u8] {
        if self.alpha != Some(alpha) {
            self.channels = alpha_channel_table(alpha);
            self.alpha = Some(alpha);
        }
        &self.channels
    }
}

pub(crate) fn alpha_channel_table(alpha: f64) -> Vec<u8> {
    let mut channels = vec![0; 256 * 256];
    for (destination, row) in channels.chunks_exact_mut(256).enumerate() {
        for (source, entry) in row.iter_mut().enumerate() {
            *entry = super::alpha::blend_rgb(source as u32, destination as u32, alpha) as u8;
        }
    }
    channels
}

struct MappedRect {
    left: usize,
    top: usize,
    columns: Vec<u32>,
    rows: Vec<u32>,
}

/// Axis-aligned rectangles sample at pixel centers. The triangle raster sums
/// three weighted terms; the axis map interpolates two endpoints. For bounded
/// canvas coordinates all edge weights and the area are exactly representable.
/// Stay away from integer texel boundaries so rounding in the divisions,
/// products and sums cannot change the selected texel.
fn axis_map(
    pixels: Range<i32>,
    start: i32,
    end: i32,
    a: f64,
    b: f64,
    size: usize,
) -> Option<Vec<u32>> {
    if size == 1 {
        return Some(vec![0; pixels.len()]);
    }
    let tolerance = 32.0 * f64::EPSILON * a.abs().max(b.abs()).max(1.0);
    let mut indices = Vec::with_capacity(pixels.len());
    for pixel in pixels {
        let position = (f64::from(pixel - start) + 0.5) / f64::from(end - start);
        let coordinate = a + position * (b - a);
        if (coordinate - coordinate.round()).abs() <= tolerance {
            return None;
        }
        indices.push(Texture::wrapped_coordinate(coordinate, size) as u32);
    }
    Some(indices)
}

impl Graphics {
    fn map_texture_rect(
        &self,
        texture: &Texture,
        corners: [i32; 4],
        uv: [f64; 4],
    ) -> Option<MappedRect> {
        let [ax, ay, bx, by] = corners;
        let (cl, cr, ct, cb) = self.drawable_bounds()?;
        let left = ax.min(bx).max(cl);
        let right = ax.max(bx).min(cr.saturating_add(1));
        let top = ay.min(by).max(ct);
        let bottom = ay.max(by).min(cb.saturating_add(1));
        if right <= left
            || bottom <= top
            || i64::from(right - left) * i64::from(bottom - top) < 65_536
            || texture.width.max(texture.height) > u32::MAX as usize
            || !corners.iter().all(|v| v.unsigned_abs() <= 1 << 20)
            || !uv.iter().all(|v| v.abs() <= 1_048_576.0)
        {
            return None;
        }
        let [u0, v0, u1, v1] = uv;
        let columns = axis_map(left..right, ax, bx, u0, u1, texture.width)?;
        let mut rows = axis_map(top..bottom, ay, by, v0, v1, texture.height)?;
        for row in &mut rows {
            *row = texture.height as u32 - 1 - *row;
        }
        Some(MappedRect {
            left: left as usize,
            top: top as usize,
            columns,
            rows,
        })
    }

    pub(super) fn blend_mapped_rect(
        &mut self,
        texture: &Texture,
        corners: [i32; 4],
        uv: [f64; 4],
        alpha: f64,
    ) -> bool {
        let Some(rect) = self.map_texture_rect(texture, corners, uv) else {
            return false;
        };
        OPACITY.with(|state| {
            let mut cached = state.borrow_mut();
            let channels = cached.for_alpha(alpha);
            for (dy, &texture_y) in rect.rows.iter().enumerate() {
                let source = texture_y as usize * texture.width;
                let source_row = &texture.pixels[source..source + texture.width];
                let start = (rect.top + dy) * self.width + rect.left;
                let dest_row = &mut self.buffer[start..start + rect.columns.len()];
                for (pixel, &texture_x) in dest_row.iter_mut().zip(&rect.columns) {
                    let src = source_row[texture_x as usize];
                    let dst = *pixel;
                    // Each index is the pair of 8-bit destination/source channels.
                    let r = channels[(((dst >> 8) & 0xff00) | ((src >> 16) & 255)) as usize];
                    let g = channels[((dst & 0xff00) | ((src >> 8) & 255)) as usize];
                    let b = channels[(((dst & 255) << 8) | (src & 255)) as usize];
                    *pixel = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                }
            }
        });
        self.buffer_dirty = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_matches_the_quad_on_both_paths() {
        let texture = Texture {
            width: 64,
            height: 48,
            pixels: (0u32..64 * 48)
                .map(|i| i.wrapping_mul(0x61bd1) & 0xff_ffff)
                .collect(),
        };
        for (xy, uv, fast) in [
            ([0.0, 0.0, 640.0, 480.0], [0.0, 0.0, 64.0, 48.0], true),
            ([640.0, 480.0, 0.0, 0.0], [0.0, 0.0, 64.0, 48.0], true),
            (
                [-20.0, 25.0, 600.0, 451.0],
                [-7.1234, 55.314159, 140.8234, -35.314159],
                true,
            ),
            ([0.0, 0.0, 640.0, 480.0], [0.5, 0.0, 640.5, 48.0], false),
            ([0.0, 0.0, 10.0, 12.0], [0.0, 0.0, 64.0, 48.0], false),
            ([0.0, 0.0, 0.0, 480.0], [0.0, 0.0, 64.0, 48.0], false),
        ] {
            let [x0, y0, x1, y1] = xy;
            let [u0, v0, u1, v1] = uv;
            let mut actual = Graphics::new(640);
            for (i, pixel) in actual.buffer.iter_mut().enumerate() {
                *pixel = (i as u32).wrapping_mul(0x9e3779b1) & 0xff_ffff;
            }
            actual.set_origin(0, 0, Some((3, 628, 473, 6))).unwrap();
            actual.clear_buffer_dirty();
            let mut oracle = actual.clone();
            let (ax, ay) = actual.user_to_canvas(x0, y0);
            let (bx, by) = actual.user_to_canvas(x1, y1);
            assert_eq!(
                actual
                    .map_texture_rect(&texture, [ax, ay, bx, by], uv)
                    .is_some(),
                fast
            );
            actual
                .textured_rect_with_alpha(
                    &texture,
                    x0,
                    y0,
                    x1,
                    y1,
                    Some((u0, v0, u1, v1)),
                    None,
                    0.55,
                )
                .unwrap();
            oracle
                .textured_quad_with_alpha(
                    &texture, x0, y0, u0, v0, x1, y0, u1, v0, x1, y1, u1, v1, x0, y1, u0, v1, None,
                    0.55,
                )
                .unwrap();
            assert!(actual.buffer == oracle.buffer);
            assert_eq!(actual.buffer_dirty, oracle.buffer_dirty);
            assert_eq!(
                (actual.cursor_x, actual.cursor_y),
                (oracle.cursor_x, oracle.cursor_y)
            );
            assert_eq!(
                (actual.cursor_user_x, actual.cursor_user_y),
                (oracle.cursor_user_x, oracle.cursor_user_y)
            );
        }
    }

    #[test]
    fn opacity_table_matches_channel_blending() {
        let mut state = OpacityTable::default();
        for alpha in [0.01, 0.25, 0.5, 0.501, 0.7, 0.99] {
            let t = state.for_alpha(alpha);
            for dst in 0..256u32 {
                for src in 0..256u32 {
                    assert_eq!(
                        t[(dst * 256 + src) as usize],
                        super::super::alpha::blend_rgb(src, dst, alpha) as u8
                    );
                }
            }
        }
    }
}
