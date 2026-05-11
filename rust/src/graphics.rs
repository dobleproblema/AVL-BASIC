use crate::error::{BasicError, BasicResult, ErrorCode};
use crate::fonts::{font_dimensions, glyph_rows, FontKind};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Graphics {
    pub width: usize,
    pub height: usize,
    buffer: Vec<u32>,
    owner: Vec<i32>,
    current_color: u32,
    background_color: u32,
    origin_x: i32,
    origin_y: i32,
    w_left: i32,
    w_top: i32,
    w_right: i32,
    w_bottom: i32,
    cursor_x: i32,
    cursor_y: i32,
    cursor_user_x: f64,
    cursor_user_y: f64,
    text_col: i32,
    text_row: i32,
    font: FontKind,
    ldir: i32,
    pen_width: i32,
    mask: u8,
    scale: Option<Scale>,
    cross_at_x: Option<f64>,
    cross_at_y: Option<f64>,
    graph_range: Option<(f64, f64, f64, f64)>,
    graph_x_axis_range: Option<(f64, f64)>,
    graph_y_axis_range: Option<(f64, f64)>,
    x_axis_label_bboxes: Vec<(i32, i32, i32, i32)>,
    collision_mode: i32,
    collision_color: Option<u32>,
    hit: bool,
    hit_color: bool,
    hit_sprite: bool,
    hit_color_rgb: u32,
    hit_id: i32,
    sprites: HashMap<i32, SpriteMemory>,
}

#[derive(Debug, Clone, Copy)]
struct Scale {
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
    border: i32,
}

#[derive(Debug, Clone)]
struct SpriteMemory {
    data: String,
    transparent: Option<u32>,
    pixels: Vec<usize>,
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new(640)
    }
}

impl Graphics {
    pub fn new(mode_width: usize) -> Self {
        let (width, height) = match mode_width {
            800 => (800, 600),
            1024 => (1024, 768),
            _ => (640, 480),
        };
        let background_color = 0x000000;
        Self {
            width,
            height,
            buffer: vec![background_color; width * height],
            owner: vec![0; width * height],
            current_color: 0xffffff,
            background_color,
            origin_x: 0,
            origin_y: 0,
            w_left: 0,
            w_top: 0,
            w_right: width as i32 - 1,
            w_bottom: height as i32 - 1,
            cursor_x: 0,
            cursor_y: 0,
            cursor_user_x: 0.0,
            cursor_user_y: 0.0,
            text_col: 0,
            text_row: 0,
            font: FontKind::Small,
            ldir: 0,
            pen_width: 1,
            mask: 255,
            scale: None,
            cross_at_x: None,
            cross_at_y: None,
            graph_range: None,
            graph_x_axis_range: None,
            graph_y_axis_range: None,
            x_axis_label_bboxes: Vec::new(),
            collision_mode: 0,
            collision_color: None,
            hit: false,
            hit_color: false,
            hit_sprite: false,
            hit_color_rgb: 0,
            hit_id: 0,
            sprites: HashMap::new(),
        }
    }

    pub fn reset_state(&mut self) {
        let mode = self.width;
        *self = Graphics::new(mode);
    }

    pub fn reset_window_state_preserving_buffer(&mut self) {
        self.current_color = 0xffffff;
        self.background_color = 0x000000;
        self.origin_x = 0;
        self.origin_y = 0;
        self.reset_viewport();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.cursor_user_x = 0.0;
        self.cursor_user_y = 0.0;
        self.text_col = 0;
        self.text_row = 0;
        self.font = FontKind::Small;
        self.ldir = 0;
        self.pen_width = 1;
        self.mask = 255;
        self.scale = None;
        self.cross_at_x = None;
        self.cross_at_y = None;
        self.reset_graph_ranges();
        self.x_axis_label_bboxes.clear();
        self.collision_mode = 0;
        self.collision_color = None;
        self.hit = false;
        self.hit_color = false;
        self.hit_sprite = false;
        self.hit_color_rgb = 0;
        self.hit_id = 0;
        self.owner.fill(0);
        self.sprites.clear();
    }

    pub fn set_mode(&mut self, mode_width: usize) -> BasicResult<()> {
        if !matches!(mode_width, 640 | 800 | 1024) {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        let current_color = self.current_color;
        let background_color = self.background_color;
        let font = self.font;
        let ldir = self.ldir;
        *self = Graphics::new(mode_width);
        self.current_color = current_color;
        self.background_color = background_color;
        self.font = font;
        self.ldir = ldir;
        self.clg();
        Ok(())
    }

    pub fn clg(&mut self) {
        let left = self.w_left.max(0) as usize;
        let right = self.w_right.min(self.width as i32 - 1) as usize;
        let top = self.w_top.max(0) as usize;
        let bottom = self.w_bottom.min(self.height as i32 - 1) as usize;
        if left <= right && top <= bottom {
            for y in top..=bottom {
                let start = y * self.width + left;
                let end = y * self.width + right + 1;
                self.buffer[start..end].fill(self.background_color);
                self.owner[start..end].fill(0);
            }
        }
        self.hit = false;
        self.hit_color = false;
        self.hit_sprite = false;
        self.hit_color_rgb = 0;
        self.hit_id = 0;
        self.move_to(0.0, 0.0);
        self.locate(0, 0);
    }

    pub fn set_ink(&mut self, color: i32) {
        self.current_color = resolve_color_number(color);
    }

    pub fn set_paper(&mut self, color: i32) {
        self.background_color = resolve_color_number(color);
    }

    pub fn set_pen_width(&mut self, width: i32) -> BasicResult<()> {
        if !matches!(width, 1 | 2 | 4) {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.pen_width = width;
        Ok(())
    }

    pub fn set_mask(&mut self, mask: Option<i32>) -> BasicResult<()> {
        let mask = mask.unwrap_or(255);
        if !(0..=255).contains(&mask) {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.mask = mask as u8;
        Ok(())
    }

    pub fn set_origin(
        &mut self,
        x: i32,
        y: i32,
        viewport: Option<(i32, i32, i32, i32)>,
    ) -> BasicResult<()> {
        if self.scale.is_some() {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.origin_x = x;
        self.origin_y = y;
        let (left, right, top, bottom) =
            viewport.unwrap_or((0, self.width as i32 - 1, self.height as i32 - 1, 0));
        let left = left.clamp(0, self.width as i32 - 1);
        let right = right.clamp(0, self.width as i32 - 1);
        let top = top.clamp(0, self.height as i32 - 1);
        let bottom = bottom.clamp(0, self.height as i32 - 1);
        if left >= right || bottom >= top {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.w_left = left;
        self.w_right = right;
        self.w_top = self.height as i32 - 1 - top;
        self.w_bottom = self.height as i32 - 1 - bottom;
        self.move_to(0.0, 0.0);
        Ok(())
    }

    pub fn set_scale(&mut self, args: Option<(f64, f64, f64, f64, i32)>) -> BasicResult<()> {
        if let Some((xmin, xmax, ymin, ymax, border)) = args {
            if xmin == xmax || ymin == ymax || border < 0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            self.origin_x = 0;
            self.origin_y = 0;
            self.reset_viewport();
            self.scale = Some(Scale {
                xmin,
                xmax,
                ymin,
                ymax,
                border,
            });
        } else {
            self.scale = None;
            self.reset_viewport();
        }
        self.reset_graph_ranges();
        Ok(())
    }

    pub fn has_explicit_scale(&self) -> bool {
        self.scale.is_some()
    }

    pub fn scale_bounds(&self) -> (f64, f64, f64, f64) {
        if let Some(scale) = self.scale {
            (scale.xmin, scale.xmax, scale.ymin, scale.ymax)
        } else {
            (
                0.0,
                self.width.saturating_sub(1) as f64,
                0.0,
                self.height.saturating_sub(1) as f64,
            )
        }
    }

    pub fn set_cross_at(&mut self, cross: Option<(f64, f64)>) -> BasicResult<()> {
        if let Some((x, y)) = cross {
            if !x.is_finite() || !y.is_finite() {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            self.cross_at_x = Some(x);
            self.cross_at_y = Some(y);
        } else {
            self.cross_at_x = None;
            self.cross_at_y = None;
        }
        Ok(())
    }

    pub fn set_graph_range(&mut self, range: Option<(f64, f64, f64, f64)>) -> BasicResult<()> {
        let Some((xmin, xmax, ymin, ymax)) = range else {
            self.graph_range = None;
            return Ok(());
        };
        if !self.has_explicit_scale()
            || !xmin.is_finite()
            || !xmax.is_finite()
            || !ymin.is_finite()
            || !ymax.is_finite()
            || xmax <= xmin
            || ymax <= ymin
        {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        let (sxmin, sxmax, symin, symax) = self.scale_bounds();
        let eps = 1e-12;
        if xmin < sxmin - eps || xmax > sxmax + eps || ymin < symin - eps || ymax > symax + eps {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.graph_range = Some((xmin, xmax, ymin, ymax));
        Ok(())
    }

    pub fn graph_plot_bounds(&self) -> BasicResult<(f64, f64, f64, f64)> {
        if !self.has_explicit_scale() {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        if let Some(range) = self.graph_range {
            return Ok(range);
        }
        let (sxmin, sxmax, symin, symax) = self.scale_bounds();
        let (xmin, xmax) = self.graph_x_axis_range.unwrap_or((sxmin, sxmax));
        let (ymin, ymax) = self.graph_y_axis_range.unwrap_or((symin, symax));
        Ok((
            xmin.min(xmax),
            xmin.max(xmax),
            ymin.min(ymax),
            ymin.max(ymax),
        ))
    }

    pub fn graph_effective_pixel_width(&self, xmin: f64, xmax: f64, y: f64) -> usize {
        let (x0, _) = self.user_to_canvas(xmin, y);
        let (x1, _) = self.user_to_canvas(xmax, y);
        (x1 - x0).unsigned_abs().max(1) as usize
    }

    pub fn draw_x_axis(
        &mut self,
        tic: f64,
        xmin: f64,
        xmax: f64,
        explicit_range: bool,
        label_side: i32,
        orientation: i32,
        force_scientific_labels: bool,
        subdivisions: i32,
    ) -> BasicResult<()> {
        let y = self.cross_at_y.unwrap_or(0.0);
        let border = self.scale_border();
        if !self.has_explicit_scale()
            || !tic.is_finite()
            || !xmin.is_finite()
            || !xmax.is_finite()
            || xmin == xmax
            || border < 0
            || border * 2 >= self.width as i32
            || !matches!(orientation, 0 | 1)
            || subdivisions < 1
        {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        let left = xmin.min(xmax);
        let right = xmin.max(xmax);
        let (axis_start, axis_end) = self.x_axis_canvas_span(y, left, right, border);
        let (_, y_pixel) = self.user_to_canvas(0.0, y);
        let mut mask_phase = 0;
        if axis_start <= axis_end {
            for x in axis_start..=axis_end {
                self.draw_masked_axis_pixel(x, y_pixel, &mut mask_phase);
            }
        }
        if tic != 0.0 {
            let show_labels = label_side >= 0;
            let labels_below = label_side == 0;
            self.draw_x_axis_ticks(
                y,
                tic.abs(),
                left,
                right,
                axis_start,
                axis_end,
                labels_below,
                show_labels,
                orientation == 1,
                force_scientific_labels,
                subdivisions,
                &mut mask_phase,
            );
        }
        if explicit_range {
            self.graph_x_axis_range = Some((left, right));
        }
        Ok(())
    }

    pub fn draw_y_axis(
        &mut self,
        tic: f64,
        ymin: f64,
        ymax: f64,
        explicit_range: bool,
        label_side: i32,
        force_scientific_labels: bool,
        subdivisions: i32,
    ) -> BasicResult<()> {
        let x = self.cross_at_x.unwrap_or(0.0);
        let border = self.scale_border();
        if !self.has_explicit_scale()
            || !tic.is_finite()
            || !ymin.is_finite()
            || !ymax.is_finite()
            || ymin == ymax
            || border < 0
            || border * 2 >= self.height as i32
            || subdivisions < 1
        {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        let bottom = ymin.min(ymax);
        let top = ymin.max(ymax);
        let (axis_start, axis_end) = self.y_axis_canvas_span(x, bottom, top, border);
        let (x_pixel, _) = self.user_to_canvas(x, 0.0);
        let mut mask_phase = 0;
        if axis_start <= axis_end {
            for y in axis_start..=axis_end {
                if self.over_x_axis_label(x_pixel, y) {
                    continue;
                }
                self.draw_masked_axis_pixel(x_pixel, y, &mut mask_phase);
            }
        }
        if tic != 0.0 {
            let show_labels = label_side >= 0;
            let labels_left = label_side == 0;
            self.draw_y_axis_ticks(
                x,
                tic.abs(),
                bottom,
                top,
                axis_start,
                axis_end,
                labels_left,
                show_labels,
                force_scientific_labels,
                subdivisions,
                &mut mask_phase,
            );
        }
        if explicit_range {
            self.graph_y_axis_range = Some((bottom, top));
        }
        Ok(())
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        let (cx, cy) = self.user_to_canvas(x, y);
        self.cursor_x = cx;
        self.cursor_y = self.canvas_y_to_logical(cy);
        self.cursor_user_x = x;
        self.cursor_user_y = y;
    }

    pub fn set_cursor_from_logical_screen(&mut self, x: i32, y: i32) {
        let canvas_y = self.height as i32 - 1 - y;
        self.set_cursor_from_canvas(x, canvas_y);
    }

    pub fn plot(&mut self, x: f64, y: f64, color: Option<i32>) {
        let (cx, cy) = self.user_to_canvas(x, y);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        self.draw_brush(cx, cy, color);
        self.cursor_x = cx;
        self.cursor_y = self.canvas_y_to_logical(cy);
        self.cursor_user_x = x;
        self.cursor_user_y = y;
    }

    pub fn draw_to(&mut self, x: f64, y: f64, color: Option<i32>) {
        let start = (self.cursor_x, self.logical_y_to_canvas(self.cursor_y));
        let end = self.user_to_canvas(x, y);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        self.line_canvas(start.0, start.1, end.0, end.1, color);
        self.cursor_x = end.0;
        self.cursor_y = self.canvas_y_to_logical(end.1);
        self.cursor_user_x = x;
        self.cursor_user_y = y;
    }

    pub fn line_between(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, color: Option<i32>) {
        let p1 = self.user_to_canvas(x1, y1);
        let p2 = self.user_to_canvas(x2, y2);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        self.line_canvas(p1.0, p1.1, p2.0, p2.1, color);
        self.cursor_x = p2.0;
        self.cursor_y = self.canvas_y_to_logical(p2.1);
        self.cursor_user_x = x2;
        self.cursor_user_y = y2;
    }

    pub fn line_between_with_mask_phase(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: Option<i32>,
        mask_phase: u8,
        skip_first_pixel: bool,
    ) -> (u8, bool) {
        let p1 = self.user_to_canvas(x1, y1);
        let p2 = self.user_to_canvas(x2, y2);
        if skip_first_pixel && p1 == p2 {
            return (mask_phase, false);
        }
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        let next_phase =
            self.line_canvas_phase(p1.0, p1.1, p2.0, p2.1, color, mask_phase, skip_first_pixel);
        self.cursor_x = p2.0;
        self.cursor_y = self.canvas_y_to_logical(p2.1);
        self.cursor_user_x = x2;
        self.cursor_user_y = y2;
        (next_phase, p1 != p2 || !skip_first_pixel)
    }

    pub fn rectangle(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: Option<i32>,
        filled: bool,
    ) {
        let (ax, ay) = self.user_to_canvas(x1, y1);
        let (bx, by) = self.user_to_canvas(x2, y2);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        let min_x = ax.min(bx);
        let max_x = ax.max(bx);
        let min_y = ay.min(by);
        let max_y = ay.max(by);
        if filled {
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    self.set_canvas_pixel(x, y, color);
                }
            }
        } else {
            self.line_canvas(min_x, min_y, max_x, min_y, color);
            self.line_canvas(max_x, min_y, max_x, max_y, color);
            self.line_canvas(max_x, max_y, min_x, max_y, color);
            self.line_canvas(min_x, max_y, min_x, min_y, color);
        }
    }

    pub fn triangle(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: Option<i32>,
        filled: bool,
    ) {
        let p0 = self.user_to_canvas(x0, y0);
        let p1 = self.user_to_canvas(x1, y1);
        let p2 = self.user_to_canvas(x2, y2);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        if filled {
            self.filled_triangle_canvas(p0, p1, p2, color);
        } else {
            let phase = self.line_canvas_phase(p0.0, p0.1, p1.0, p1.1, color, 0, false);
            let phase = self.line_canvas_phase(p1.0, p1.1, p2.0, p2.1, color, phase, true);
            self.line_canvas_phase(p2.0, p2.1, p0.0, p0.1, color, phase, true);
        }
        self.cursor_x = p2.0;
        self.cursor_y = self.canvas_y_to_logical(p2.1);
        self.cursor_user_x = x2;
        self.cursor_user_y = y2;
    }

    pub fn circle(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        color: Option<i32>,
        filled: bool,
    ) -> BasicResult<()> {
        self.circle_arc(x, y, radius, color, filled, None, None, 1.0)
    }

    pub fn circle_arc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        color: Option<i32>,
        filled: bool,
        start_angle: Option<f64>,
        end_angle: Option<f64>,
        aspect: f64,
    ) -> BasicResult<()> {
        if radius <= 0.0 || aspect <= 0.0 || !radius.is_finite() || !aspect.is_finite() {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        let (cx, cy) = self.user_to_canvas(x, y);
        let (rx, ry) = self.scaled_radii(radius, aspect);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        let full_circle = match (start_angle, end_angle) {
            (Some(start), Some(end)) => (end - start).abs() >= std::f64::consts::TAU,
            _ => true,
        };
        let (start_angle, end_angle) = if full_circle {
            (0.0, std::f64::consts::TAU)
        } else {
            let start = start_angle.unwrap().rem_euclid(std::f64::consts::TAU);
            let mut end = end_angle.unwrap().rem_euclid(std::f64::consts::TAU);
            if end < start {
                end += std::f64::consts::TAU;
            }
            (start, end)
        };
        if filled {
            let y_start = (cy as f64 - ry).trunc() as i32;
            let y_end = (cy as f64 + ry).trunc() as i32;
            let inv_rx = 1.0 / rx;
            let inv_ry = 1.0 / ry;
            for y in y_start..=y_end {
                let dy = y as f64 - cy as f64;
                let span = (1.0 - (dy * inv_ry).powi(2)).max(0.0).sqrt() * rx;
                let x_left = (cx as f64 - span).trunc() as i32;
                let x_right = (cx as f64 + span).trunc() as i32;
                if full_circle {
                    for x in x_left..=x_right {
                        self.set_canvas_pixel(x, y, color);
                    }
                    continue;
                }
                let ndy = dy * inv_ry;
                for x in x_left..=x_right {
                    let ndx = (x as f64 - cx as f64) * inv_rx;
                    let mut angle = ndy.atan2(ndx);
                    if angle < start_angle {
                        angle += std::f64::consts::TAU;
                    }
                    if angle >= start_angle && angle <= end_angle {
                        self.set_canvas_pixel(x, y, color);
                    }
                }
            }
        } else {
            let steps = ((rx.max(ry) * std::f64::consts::TAU).ceil() as usize).clamp(16, 20_000);
            let total = end_angle - start_angle;
            let mut previous: Option<(i32, i32)> = None;
            for i in 0..=steps {
                let angle = start_angle + total * (i as f64 / steps as f64);
                let px = (cx as f64 + rx * angle.cos()).round() as i32;
                let py = (cy as f64 + ry * angle.sin()).round() as i32;
                if let Some((lx, ly)) = previous {
                    if lx != px || ly != py {
                        self.line_canvas(lx, ly, px, py, color);
                    }
                } else {
                    self.draw_brush(px, py, color);
                }
                previous = Some((px, py));
            }
        }
        Ok(())
    }

    pub fn fill(&mut self, x: f64, y: f64, color: Option<i32>) {
        let (sx, sy) = self.user_to_canvas(x, y);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        if !self.in_drawable_bounds(sx, sy) {
            return;
        }
        let target = self
            .get_canvas_pixel(sx, sy)
            .unwrap_or(self.background_color);
        if target == color {
            return;
        }
        let mut queue = VecDeque::new();
        queue.push_back((sx, sy));
        while let Some((x, y)) = queue.pop_front() {
            if !self.in_drawable_bounds(x, y) || self.get_canvas_pixel(x, y) != Some(target) {
                continue;
            }
            self.set_canvas_pixel(x, y, color);
            queue.push_back((x + 1, y));
            queue.push_back((x - 1, y));
            queue.push_back((x, y + 1));
            queue.push_back((x, y - 1));
        }
    }

    pub fn locate(&mut self, col: i32, row: i32) {
        self.text_col = col;
        self.text_row = row;
    }

    pub fn set_font(&mut self, font: FontKind) {
        let (current_width, _) = font_dimensions(self.font);
        let (new_width, _) = font_dimensions(font);
        if current_width > new_width {
            self.text_col = ((self.text_col as f64) * 2.0).ceil() as i32;
        } else if current_width < new_width {
            self.text_col = ((self.text_col as f64) / 2.0).ceil() as i32;
        }
        self.font = font;
    }

    pub fn set_ldir(&mut self, angle: i32) {
        self.ldir = angle;
    }

    pub fn disp(&mut self, text: &str, ink: Option<i32>, paper: Option<i32>) {
        let color = ink.map(resolve_color_number).unwrap_or(self.current_color);
        let paper = self.text_background_color(paper);
        let (cell_w, cell_h) = font_dimensions(self.font);
        let mut x = self.text_col * cell_w;
        let y = self.text_row * cell_h;
        for ch in text.chars() {
            self.draw_glyph(x as f64, y as f64, 1.0, 0.0, ch, color, paper, true);
            x += cell_w;
            self.text_col += 1;
        }
    }

    pub fn gdisp(&mut self, text: &str, ink: Option<i32>, paper: Option<i32>) {
        let color = ink.map(resolve_color_number).unwrap_or(self.current_color);
        let paper = self.text_background_color(paper);
        let (cell_w, _) = font_dimensions(self.font);
        let radians = -(self.ldir as f64).to_radians();
        let cos_a = radians.cos();
        let sin_a = radians.sin();
        let dx = cell_w as f64 * cos_a;
        let dy = cell_w as f64 * sin_a;
        let mut x = self.cursor_x as f64;
        let mut y = self.logical_y_to_canvas(self.cursor_y) as f64;
        for ch in text.chars() {
            self.draw_glyph(x, y, cos_a, sin_a, ch, color, paper, false);
            x += dx;
            y += dy;
        }
        self.cursor_x = x.round() as i32;
        self.cursor_y = self.canvas_y_to_logical(y.round() as i32);
        self.cursor_user_x += text.chars().count() as f64 * dx;
        self.cursor_user_y -= text.chars().count() as f64 * dy;
    }

    pub fn capture_screen(&self) -> String {
        let mut out = format!("{}x{}:", self.width, self.height);
        for rgb in &self.buffer {
            out.push_str(&format!("{rgb:06x}"));
        }
        out
    }

    pub fn restore_screen(&mut self, screen: &str) -> BasicResult<()> {
        let (w, h, pixels) = parse_gscr(screen)?;
        if w != self.width || h != self.height {
            return Err(BasicError::new(ErrorCode::InvalidValue));
        }
        self.buffer = pixels;
        self.owner.fill(0);
        Ok(())
    }

    pub fn capture_sprite(&self, x1: f64, y1: f64, x2: f64, y2: f64) -> String {
        let (ax, ay) = self.user_to_canvas(x1, y1);
        let (bx, by) = self.user_to_canvas(x2, y2);
        let min_x = ax.min(bx);
        let max_x = ax.max(bx);
        let min_y = ay.min(by);
        let max_y = ay.max(by);
        let width = (max_x - min_x + 1).max(0) as usize;
        let height = (max_y - min_y + 1).max(0) as usize;
        let mut out = format!("{width}x{height}:");
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                out.push_str(&format!("{:06x}", self.get_canvas_pixel(x, y).unwrap_or(0)));
            }
        }
        out
    }

    pub fn draw_sprite(
        &mut self,
        sprite: &str,
        x: f64,
        y: f64,
        transparent: Option<i32>,
        sprite_id: Option<i32>,
        hittest_only: bool,
    ) -> BasicResult<()> {
        let (w, h, pixels) = parse_gscr(sprite)?;
        let transparent_rgb = transparent.map(resolve_color_number);
        let id = sprite_id.unwrap_or(0);
        if id < 0 {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.hit = false;
        self.hit_color = false;
        self.hit_sprite = false;
        self.hit_color_rgb = 0;
        self.hit_id = 0;
        if let Some(id) = sprite_id {
            if id <= 0 {
                return Err(BasicError::new(ErrorCode::InvalidArgument));
            }
            if !hittest_only {
                self.clear_owner_for_id(id);
                self.sprites.insert(
                    id,
                    SpriteMemory {
                        data: sprite.to_string(),
                        transparent: transparent_rgb,
                        pixels: Vec::new(),
                    },
                );
            }
        }
        let (left, bottom) = self.user_to_canvas(x, y);
        let top = bottom - h as i32 + 1;
        let detect_color = matches!(self.collision_mode, 1 | 3);
        let detect_sprite = matches!(self.collision_mode, 2 | 3);
        let mut touched = Vec::new();
        for row in 0..h {
            for col in 0..w {
                let rgb = pixels[row * w + col];
                if Some(rgb) == transparent_rgb {
                    continue;
                }
                let cx = left + col as i32;
                let cy = top + row as i32;
                if !self.in_drawable_bounds(cx, cy) {
                    continue;
                }
                let idx = self.index(cx, cy).unwrap();
                if detect_color {
                    let current = self.buffer[idx];
                    let color_hit = match self.collision_color {
                        Some(filter) => current == filter,
                        None => current != self.background_color,
                    };
                    if color_hit {
                        self.hit = true;
                        self.hit_color = true;
                        if self.hit_color_rgb == 0 {
                            self.hit_color_rgb = current;
                        }
                    }
                }
                if detect_sprite && self.owner[idx] != 0 && self.owner[idx] != id {
                    self.hit = true;
                    self.hit_sprite = true;
                    if self.hit_id == 0 {
                        self.hit_id = self.owner[idx];
                    }
                    continue;
                }
                if !hittest_only {
                    self.buffer[idx] = rgb;
                    if id > 0 {
                        self.owner[idx] = id;
                        touched.push(idx);
                    }
                }
            }
        }
        if let Some(id) = sprite_id {
            if let Some(mem) = self.sprites.get_mut(&id) {
                mem.pixels = touched;
            }
        }
        Ok(())
    }

    pub fn sprite_move(
        &mut self,
        id: i32,
        x: f64,
        y: f64,
        transparent: Option<i32>,
    ) -> BasicResult<()> {
        let Some(mem) = self.sprites.get(&id).cloned() else {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        };
        let transparent = transparent.or_else(|| mem.transparent.map(rgb_to_index_like));
        self.draw_sprite(&mem.data, x, y, transparent, Some(id), false)
    }

    pub fn sprite_delete(&mut self, id: i32) {
        self.clear_owner_for_id(id);
        self.sprites.remove(&id);
    }

    pub fn colmode(&mut self, mode: i32) -> BasicResult<()> {
        if !(0..=3).contains(&mode) {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        self.collision_mode = mode;
        Ok(())
    }

    pub fn colcolor(&mut self, color: Option<i32>) {
        self.collision_color = color.map(resolve_color_number);
    }

    pub fn colreset(&mut self) {
        self.owner.fill(0);
        self.hit = false;
        self.hit_color = false;
        self.hit_sprite = false;
        self.hit_color_rgb = 0;
        self.hit_id = 0;
    }

    pub fn test(&self, x: f64, y: f64) -> i32 {
        let (cx, cy) = self.user_to_canvas(x, y);
        self.get_canvas_pixel(cx, cy).unwrap_or(0) as i32
    }

    pub fn hit(&self) -> i32 {
        if self.hit {
            -1
        } else {
            0
        }
    }

    pub fn hitcolor(&self) -> i32 {
        self.hit_color_rgb as i32
    }

    pub fn hitsprite(&self) -> i32 {
        if self.hit_sprite {
            -1
        } else {
            0
        }
    }

    pub fn hitid(&self) -> i32 {
        self.hit_id
    }

    pub fn buffer(&self) -> &[u32] {
        &self.buffer
    }

    pub fn xpos(&self) -> f64 {
        self.cursor_user_x
    }

    pub fn ypos(&self) -> f64 {
        self.cursor_user_y
    }

    pub fn hpos(&self) -> i32 {
        self.text_col
    }

    pub fn vpos(&self) -> i32 {
        self.text_row
    }

    pub fn save_png(&self, path: &Path) -> BasicResult<()> {
        let file = File::create(path)
            .map_err(|e| BasicError::new(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
        let writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, self.width as u32, self.height as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut png_writer = encoder
            .write_header()
            .map_err(|e| BasicError::new(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
        let mut bytes = Vec::with_capacity(self.buffer.len() * 3);
        for rgb in &self.buffer {
            bytes.push((rgb >> 16) as u8);
            bytes.push((rgb >> 8) as u8);
            bytes.push(*rgb as u8);
        }
        png_writer
            .write_image_data(&bytes)
            .map_err(|e| BasicError::new(ErrorCode::InvalidValue).with_detail(e.to_string()))
    }

    pub fn load_png_to_gscr(path: &Path) -> BasicResult<String> {
        let file = File::open(path)
            .map_err(|e| BasicError::new(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
        let decoder = png::Decoder::new(BufReader::new(file));
        let mut reader = decoder
            .read_info()
            .map_err(|e| BasicError::new(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| BasicError::new(ErrorCode::InvalidValue).with_detail(e.to_string()))?;
        let bytes = &buf[..info.buffer_size()];
        let mut out = format!("{}x{}:", info.width, info.height);
        match info.color_type {
            png::ColorType::Rgb => {
                for chunk in bytes.chunks_exact(3) {
                    out.push_str(&format!("{:02x}{:02x}{:02x}", chunk[0], chunk[1], chunk[2]));
                }
            }
            png::ColorType::Rgba => {
                for chunk in bytes.chunks_exact(4) {
                    out.push_str(&format!("{:02x}{:02x}{:02x}", chunk[0], chunk[1], chunk[2]));
                }
            }
            _ => return Err(BasicError::new(ErrorCode::InvalidValue)),
        }
        Ok(out)
    }

    fn user_to_canvas(&self, x: f64, y: f64) -> (i32, i32) {
        if let Some(scale) = self.scale {
            let sx = (self.width as i32 - 1 - 2 * scale.border) as f64 / (scale.xmax - scale.xmin);
            let sy = (self.height as i32 - 1 - 2 * scale.border) as f64 / (scale.ymax - scale.ymin);
            let cx = scale.border as f64 + (x - scale.xmin) * sx;
            let cy = self.height as f64 - 1.0 - scale.border as f64 - (y - scale.ymin) * sy;
            (cx.round() as i32, cy.round() as i32)
        } else {
            let cx = x.round() as i32 + self.origin_x;
            let cy = self.height as i32 - 1 - (y.round() as i32 + self.origin_y);
            (cx, cy)
        }
    }

    fn set_cursor_from_canvas(&mut self, canvas_x: i32, canvas_y: i32) {
        self.cursor_x = canvas_x;
        self.cursor_y = self.canvas_y_to_logical(canvas_y);
        let logical_x = canvas_x - self.origin_x;
        let logical_y = self.height as i32 - 1 - canvas_y - self.origin_y;
        let (user_x, user_y) = if let Some(scale) = self.scale {
            let sx = (self.width as i32 - 1 - 2 * scale.border) as f64 / (scale.xmax - scale.xmin);
            let sy = (self.height as i32 - 1 - 2 * scale.border) as f64 / (scale.ymax - scale.ymin);
            (
                normalize_user_coord(scale.xmin + (logical_x as f64 - scale.border as f64) / sx),
                normalize_user_coord(scale.ymin + (logical_y as f64 - scale.border as f64) / sy),
            )
        } else {
            (
                normalize_user_coord(logical_x as f64),
                normalize_user_coord(logical_y as f64),
            )
        };
        self.cursor_user_x = user_x;
        self.cursor_user_y = user_y;
    }

    fn scaled_radii(&self, radius: f64, aspect: f64) -> (f64, f64) {
        if let Some(scale) = self.scale {
            let sx =
                (self.width as i32 - 1 - 2 * scale.border) as f64 / (scale.xmax - scale.xmin).abs();
            let sy = (self.height as i32 - 1 - 2 * scale.border) as f64
                / (scale.ymax - scale.ymin).abs();
            (radius * aspect * sx, radius * sy)
        } else {
            (radius * aspect, radius)
        }
    }

    fn reset_graph_ranges(&mut self) {
        self.graph_range = None;
        self.graph_x_axis_range = None;
        self.graph_y_axis_range = None;
    }

    fn reset_viewport(&mut self) {
        self.w_left = 0;
        self.w_top = 0;
        self.w_right = self.width as i32 - 1;
        self.w_bottom = self.height as i32 - 1;
    }

    fn draw_x_axis_ticks(
        &mut self,
        y: f64,
        tic: f64,
        xmin: f64,
        xmax: f64,
        axis_start: i32,
        axis_end: i32,
        labels_below: bool,
        show_labels: bool,
        labels_vertical: bool,
        force_scientific_labels: bool,
        subdivisions: i32,
        mask_phase: &mut u8,
    ) {
        const TIC_LENGTH: i32 = 8;
        const SUB_TIC_LENGTH: i32 = 3;
        let draw_tics_upward = if show_labels { labels_below } else { true };
        let direction = if draw_tics_upward { -1 } else { 1 };
        let axis_span = (axis_end - axis_start + 1).max(1) as f64;
        if axis_ticks_too_dense(
            xmin,
            xmax,
            tic,
            axis_span,
            if show_labels { 18.0 } else { 9.0 },
        ) {
            return;
        }
        let axis_intersection_x = self.cross_at_x.unwrap_or(0.0);
        let (center_tick, x_intersection_pixel) =
            if axis_intersection_x >= xmin && axis_intersection_x <= xmax {
                (
                    axis_intersection_x,
                    Some(self.axis_x_tick_pixel(axis_intersection_x, y, xmin, xmax)),
                )
            } else {
                (xmin, None)
            };
        if subdivisions > 1 {
            let sub_tic = tic / subdivisions as f64;
            if !axis_ticks_too_dense(xmin, xmax, sub_tic, axis_span, 6.0) {
                let sub_ticks = build_axis_ticks(xmin, xmax, center_tick, sub_tic, axis_span);
                let main_eps = tic.abs().mul_add(1e-9, 0.0).max(1e-12);
                let mut seen_sub_tick_pixels = HashSet::new();
                for x in sub_ticks {
                    let main_index = ((x - center_tick) / tic).round();
                    if ((x - center_tick) - main_index * tic).abs() > main_eps {
                        let cx = self.axis_x_tick_pixel(x, y, xmin, xmax);
                        if !seen_sub_tick_pixels.insert(cx) {
                            continue;
                        }
                        let (_, cy) = self.user_to_canvas(x, y);
                        for step in 1..=SUB_TIC_LENGTH {
                            self.draw_masked_axis_pixel(cx, cy + direction * step, mask_phase);
                        }
                    }
                }
            }
        }
        self.x_axis_label_bboxes.clear();
        let ticks = build_axis_ticks(xmin, xmax, center_tick, tic, axis_span);
        let mut seen_tick_pixels = HashSet::new();
        for x in ticks {
            let cx = self.axis_x_tick_pixel(x, y, xmin, xmax);
            let is_intersection = x_intersection_pixel.is_some_and(|pixel| (cx - pixel).abs() < 2);
            if !seen_tick_pixels.insert(cx) && !is_intersection {
                continue;
            }
            let (_, cy) = self.user_to_canvas(x, y);
            for step in 1..=TIC_LENGTH {
                self.draw_masked_axis_pixel(cx, cy + direction * step, mask_phase);
            }
            if show_labels {
                let label = format_axis_tick_label(x, force_scientific_labels);
                if labels_vertical {
                    let x_center = if is_intersection {
                        x_intersection_pixel.unwrap_or(cx) + 10
                    } else {
                        cx
                    };
                    let (pivot_x, pivot_y, ldir) = if labels_below {
                        (x_center + 6, cy + 5, -90)
                    } else {
                        (x_center - 6, cy - 5, 90)
                    };
                    self.draw_axis_label_at_angle(pivot_x, pivot_y, &label, ldir);
                    let rot_w = 16;
                    let rot_h = label.chars().count() as i32 * 8;
                    let bbox_x = x_center - rot_w / 2;
                    let bbox_y = if labels_below { cy + 5 } else { cy - 5 - rot_h };
                    self.remember_x_axis_label_bbox(bbox_x, bbox_y, rot_w, rot_h);
                } else {
                    let label_width = label.chars().count() as i32 * 8;
                    let label_x = if is_intersection {
                        x_intersection_pixel.unwrap_or(cx) + 4
                    } else {
                        cx - label_width / 2
                    };
                    let label_y = if labels_below { cy + 5 } else { cy - 18 };
                    self.draw_axis_label(label_x, label_y, &label);
                    self.remember_x_axis_label_bbox(label_x, label_y, label_width, 16);
                }
            }
        }
    }

    fn draw_y_axis_ticks(
        &mut self,
        x: f64,
        tic: f64,
        ymin: f64,
        ymax: f64,
        axis_start: i32,
        axis_end: i32,
        labels_left: bool,
        show_labels: bool,
        force_scientific_labels: bool,
        subdivisions: i32,
        mask_phase: &mut u8,
    ) {
        const TIC_LENGTH: i32 = 8;
        const SUB_TIC_LENGTH: i32 = 3;
        let draw_tics_left = if show_labels { !labels_left } else { false };
        let direction = if draw_tics_left { -1 } else { 1 };
        let axis_span = (axis_end - axis_start + 1).max(1) as f64;
        if axis_ticks_too_dense(
            ymin,
            ymax,
            tic,
            axis_span,
            if show_labels { 18.0 } else { 9.0 },
        ) {
            return;
        }
        let axis_intersection_y = self.cross_at_y.unwrap_or(0.0);
        let (center_tick, y_intersection_pixel) =
            if axis_intersection_y >= ymin && axis_intersection_y <= ymax {
                (
                    axis_intersection_y,
                    Some(self.axis_y_tick_pixel(axis_intersection_y, x, ymin, ymax)),
                )
            } else {
                (ymin, None)
            };
        if subdivisions > 1 {
            let sub_tic = tic / subdivisions as f64;
            if !axis_ticks_too_dense(ymin, ymax, sub_tic, axis_span, 6.0) {
                let sub_ticks = build_axis_ticks(ymin, ymax, center_tick, sub_tic, axis_span);
                let main_eps = tic.abs().mul_add(1e-9, 0.0).max(1e-12);
                let mut seen_sub_tick_pixels = HashSet::new();
                for y in sub_ticks {
                    let main_index = ((y - center_tick) / tic).round();
                    if ((y - center_tick) - main_index * tic).abs() > main_eps {
                        let cy = self.axis_y_tick_pixel(y, x, ymin, ymax);
                        if !seen_sub_tick_pixels.insert(cy) {
                            continue;
                        }
                        let (cx, _) = self.user_to_canvas(x, y);
                        if self.over_x_axis_label(cx, cy) {
                            continue;
                        }
                        for step in 0..SUB_TIC_LENGTH {
                            self.draw_masked_axis_pixel(cx + direction * step, cy, mask_phase);
                        }
                    }
                }
            }
        }
        let ticks = build_axis_ticks(ymin, ymax, center_tick, tic, axis_span);
        let mut seen_tick_pixels = HashSet::new();
        for y in ticks {
            let cy = self.axis_y_tick_pixel(y, x, ymin, ymax);
            let is_intersection = y_intersection_pixel.is_some_and(|pixel| (cy - pixel).abs() < 2);
            if !seen_tick_pixels.insert(cy) && !is_intersection {
                continue;
            }
            let (cx, _) = self.user_to_canvas(x, y);
            if self.over_x_axis_label(cx, cy) {
                continue;
            }
            for step in 0..TIC_LENGTH {
                self.draw_masked_axis_pixel(cx + direction * step, cy, mask_phase);
            }
            if is_intersection {
                continue;
            }
            if show_labels {
                let label = format_axis_tick_label(y, force_scientific_labels);
                let label_width = label.chars().count() as i32 * 8;
                let label_x = if labels_left {
                    cx - TIC_LENGTH - 4 - label_width + 5
                } else {
                    cx + TIC_LENGTH
                };
                self.draw_axis_label(label_x, cy - 6, &label);
            }
        }
    }

    fn scale_border(&self) -> i32 {
        self.scale.map(|scale| scale.border).unwrap_or(0)
    }

    fn is_physical_scale(&self) -> bool {
        self.scale.is_some_and(|scale| {
            scale.border == 0
                && scale.xmin.abs() < 1e-9
                && scale.ymin.abs() < 1e-9
                && (scale.xmax - (self.width.saturating_sub(1) as f64)).abs() < 1e-9
                && (scale.ymax - (self.height.saturating_sub(1) as f64)).abs() < 1e-9
        })
    }

    fn x_axis_canvas_span(&self, y: f64, xmin: f64, xmax: f64, border: i32) -> (i32, i32) {
        if self.is_physical_scale() {
            return (border, self.width as i32 - border - 1);
        }
        let (x0, _) = self.user_to_canvas(xmin, y);
        let (x1, _) = self.user_to_canvas(xmax, y);
        (
            x0.min(x1).max(border),
            x0.max(x1).min(self.width as i32 - border - 1),
        )
    }

    fn y_axis_canvas_span(&self, x: f64, ymin: f64, ymax: f64, border: i32) -> (i32, i32) {
        if self.is_physical_scale() {
            return (border, self.height as i32 - border - 1);
        }
        let (_, y0) = self.user_to_canvas(x, ymin);
        let (_, y1) = self.user_to_canvas(x, ymax);
        (
            y0.min(y1).max(border),
            y0.max(y1).min(self.height as i32 - border - 1),
        )
    }

    fn axis_x_tick_pixel(&self, x: f64, y: f64, xmin: f64, xmax: f64) -> i32 {
        if self.is_physical_scale() {
            let border = self.scale_border();
            let effective_width = self.width as i32 - 2 * border;
            return border + ((x - xmin) * effective_width as f64 / (xmax - xmin)).round() as i32;
        }
        self.user_to_canvas(x, y).0
    }

    fn axis_y_tick_pixel(&self, y: f64, x: f64, ymin: f64, ymax: f64) -> i32 {
        if self.is_physical_scale() {
            let border = self.scale_border();
            let effective_height = self.height as i32 - 2 * border;
            return self.height as i32
                - border
                - ((y - ymin) * effective_height as f64 / (ymax - ymin)).round() as i32;
        }
        self.user_to_canvas(x, y).1
    }

    fn draw_masked_axis_pixel(&mut self, x: i32, y: i32, mask_phase: &mut u8) {
        if ((self.mask >> (*mask_phase & 7)) & 1) != 0 {
            self.set_canvas_pixel(x, y, self.current_color);
        }
        *mask_phase = mask_phase.wrapping_add(1);
    }

    fn remember_x_axis_label_bbox(&mut self, x: i32, y: i32, width: i32, height: i32) {
        let mut x0 = x;
        let mut y0 = y;
        let mut x1 = x + width - 1;
        let mut y1 = y + height - 1;
        if x1 < 0 || y1 < 0 || x0 >= self.width as i32 || y0 >= self.height as i32 {
            return;
        }
        x0 = x0.max(0);
        y0 = y0.max(0);
        x1 = x1.min(self.width as i32 - 1);
        y1 = y1.min(self.height as i32 - 1);
        if x0 <= x1 && y0 <= y1 {
            self.x_axis_label_bboxes.push((x0, y0, x1, y1));
        }
    }

    fn over_x_axis_label(&self, x: i32, y: i32) -> bool {
        self.x_axis_label_bboxes
            .iter()
            .any(|&(x0, y0, x1, y1)| x0 <= x && x <= x1 && y0 <= y && y <= y1)
    }

    fn draw_axis_label(&mut self, x: i32, y: i32, label: &str) {
        let saved_font = self.font;
        self.font = FontKind::Small;
        let mut px = x;
        for ch in label.chars() {
            self.draw_glyph(
                px as f64,
                y as f64,
                1.0,
                0.0,
                ch,
                self.current_color,
                None,
                false,
            );
            px += 8;
        }
        self.font = saved_font;
    }

    fn draw_axis_label_at_angle(&mut self, x: i32, y: i32, label: &str, ldir: i32) {
        let saved_font = self.font;
        self.font = FontKind::Small;
        let (cell_w, _) = font_dimensions(self.font);
        let radians = -(ldir as f64).to_radians();
        let cos_a = radians.cos();
        let sin_a = radians.sin();
        let dx = cell_w as f64 * cos_a;
        let dy = cell_w as f64 * sin_a;
        let mut px = x as f64;
        let mut py = y as f64;
        for ch in label.chars() {
            self.draw_glyph(px, py, cos_a, sin_a, ch, self.current_color, None, false);
            px += dx;
            py += dy;
        }
        self.font = saved_font;
    }

    fn logical_y_to_canvas(&self, y: i32) -> i32 {
        self.height as i32 - 1 - y
    }

    fn canvas_y_to_logical(&self, y: i32) -> i32 {
        self.height as i32 - 1 - y
    }

    fn filled_triangle_canvas(
        &mut self,
        p0: (i32, i32),
        p1: (i32, i32),
        p2: (i32, i32),
        color: u32,
    ) {
        let mut pts = [p0, p1, p2];
        pts.sort_by_key(|point| point.1);
        let [(x0, y0), (x1, y1), (x2, y2)] = pts;
        if y0 == y2 {
            return;
        }

        if y1 == y2 {
            self.fill_flat_bottom_triangle((x0, y0), (x1, y1), (x2, y2), color);
        } else if y0 == y1 {
            self.fill_flat_top_triangle((x0, y0), (x1, y1), (x2, y2), color);
        } else {
            let split_x =
                x0 + floor_div_i64((x2 - x0) as i64 * (y1 - y0) as i64, (y2 - y0) as i64) as i32;
            self.fill_flat_bottom_triangle((x0, y0), (x1, y1), (split_x, y1), color);
            self.fill_flat_top_triangle((x1, y1), (split_x, y1), (x2, y2), color);
        }
    }

    fn fill_flat_bottom_triangle(
        &mut self,
        a: (i32, i32),
        b: (i32, i32),
        c: (i32, i32),
        color: u32,
    ) {
        let (ax, ay) = a;
        let (bx, by) = b;
        let (cx, _) = c;
        let dy = by - ay;
        if dy == 0 {
            return;
        }
        const SHIFT: i64 = 16;
        let dx1 = floor_div_i64(((bx - ax) as i64) << SHIFT, dy as i64);
        let dx2 = floor_div_i64(((cx - ax) as i64) << SHIFT, dy as i64);
        let mut x1_fp = (ax as i64) << SHIFT;
        let mut x2_fp = (ax as i64) << SHIFT;
        for y in ay..=by {
            self.fill_scanline(y, x1_fp >> SHIFT, x2_fp >> SHIFT, color);
            x1_fp += dx1;
            x2_fp += dx2;
        }
    }

    fn fill_flat_top_triangle(&mut self, a: (i32, i32), b: (i32, i32), c: (i32, i32), color: u32) {
        let (ax, ay) = a;
        let (bx, _) = b;
        let (cx, cy) = c;
        let dy = cy - ay;
        if dy == 0 {
            return;
        }
        const SHIFT: i64 = 16;
        let dx1 = floor_div_i64(((cx - ax) as i64) << SHIFT, dy as i64);
        let dx2 = floor_div_i64(((cx - bx) as i64) << SHIFT, dy as i64);
        let mut x1_fp = (ax as i64) << SHIFT;
        let mut x2_fp = (bx as i64) << SHIFT;
        for y in ay..=cy {
            self.fill_scanline(y, x1_fp >> SHIFT, x2_fp >> SHIFT, color);
            x1_fp += dx1;
            x2_fp += dx2;
        }
    }

    fn fill_scanline(&mut self, y: i32, x1: i64, x2: i64, color: u32) {
        if y < self.w_top || y > self.w_bottom || y < 0 || y >= self.height as i32 {
            return;
        }
        let mut left = x1.min(x2);
        let mut right = x1.max(x2);
        left = left.max(0).max(self.w_left as i64);
        right = right.min(self.width as i64 - 1).min(self.w_right as i64);
        if left > right {
            return;
        }
        let row = y as usize * self.width;
        let start = row + left as usize;
        let end = row + right as usize + 1;
        self.buffer[start..end].fill(color);
    }

    fn line_canvas(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        self.line_canvas_phase(x0, y0, x1, y1, color, 0, false);
    }

    fn line_canvas_phase(
        &mut self,
        mut x0: i32,
        mut y0: i32,
        x1: i32,
        y1: i32,
        color: u32,
        mut pixel_index: u8,
        skip_first_pixel: bool,
    ) -> u8 {
        let dx = (x1 as i64 - x0 as i64).abs();
        let dy = (y1 as i64 - y0 as i64).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut first_pixel = true;
        loop {
            let consume_pixel = !(skip_first_pixel && first_pixel);
            if consume_pixel {
                if ((self.mask >> (pixel_index & 7)) & 1) != 0 {
                    self.draw_brush(x0, y0, color);
                }
                pixel_index = pixel_index.wrapping_add(1);
            }
            first_pixel = false;
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = err * 2;
            if e2 > -dy {
                err -= dy;
                x0 += sx;
            }
            if e2 < dx {
                err += dx;
                y0 += sy;
            }
        }
        pixel_index & 7
    }

    fn draw_brush(&mut self, x: i32, y: i32, color: u32) {
        if self.pen_width == 1 {
            self.set_canvas_pixel(x, y, color);
            return;
        }
        let padding = self.pen_width / 2;
        for dy in -padding..padding {
            for dx in -padding..padding {
                self.set_canvas_pixel(x + dx, y + dy, color);
            }
        }
    }

    fn draw_glyph(
        &mut self,
        x: f64,
        y: f64,
        cos_a: f64,
        sin_a: f64,
        ch: char,
        ink: u32,
        paper: Option<u32>,
        text_mode: bool,
    ) {
        let (w, _) = font_dimensions(self.font);
        let Some(rows) = glyph_rows(self.font, ch).or_else(|| glyph_rows(self.font, '□')) else {
            return;
        };
        for (row_idx, bits) in rows.iter().enumerate() {
            let row = row_idx as f64;
            let row_base_x = x - row * sin_a;
            let row_base_y = y + row * cos_a;
            for col_idx in 0..w {
                let on = (bits >> (w - 1 - col_idx)) & 1 != 0;
                let color = if on {
                    ink
                } else if let Some(bg) = paper {
                    bg
                } else {
                    continue;
                };
                let col = col_idx as f64;
                let px = (row_base_x + col * cos_a).round() as i32;
                let py = (row_base_y + col * sin_a).round() as i32;
                if text_mode {
                    self.set_canvas_pixel_no_viewport(px, py, color);
                } else {
                    self.set_canvas_pixel(px, py, color);
                }
            }
        }
    }

    fn text_background_color(&self, paper: Option<i32>) -> Option<u32> {
        match paper {
            Some(color) if color < 0 => None,
            Some(color) => Some(resolve_color_number(color)),
            None => Some(self.background_color),
        }
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    fn in_drawable_bounds(&self, x: i32, y: i32) -> bool {
        self.in_bounds(x, y)
            && x >= self.w_left
            && x <= self.w_right
            && y >= self.w_top
            && y <= self.w_bottom
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if self.in_bounds(x, y) {
            Some(y as usize * self.width + x as usize)
        } else {
            None
        }
    }

    fn set_canvas_pixel(&mut self, x: i32, y: i32, color: u32) {
        if self.in_drawable_bounds(x, y) {
            let idx = y as usize * self.width + x as usize;
            self.buffer[idx] = color;
        }
    }

    fn set_canvas_pixel_no_viewport(&mut self, x: i32, y: i32, color: u32) {
        if let Some(idx) = self.index(x, y) {
            self.buffer[idx] = color;
        }
    }

    fn get_canvas_pixel(&self, x: i32, y: i32) -> Option<u32> {
        self.index(x, y).map(|idx| self.buffer[idx])
    }

    fn clear_owner_for_id(&mut self, id: i32) {
        if id == 0 {
            return;
        }
        for owner in &mut self.owner {
            if *owner == id {
                *owner = 0;
            }
        }
    }
}

pub fn rgb_number(r: i32, g: i32, b: i32) -> BasicResult<i32> {
    if !(0..=255).contains(&r) || !(0..=255).contains(&g) || !(0..=255).contains(&b) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    Ok((r << 16) | (g << 8) | b)
}

fn floor_div_i64(numerator: i64, denominator: i64) -> i64 {
    numerator.div_euclid(denominator)
}

pub fn resolve_color_number(color: i32) -> u32 {
    const COLORS: [u32; 32] = [
        0x000000, 0xffffff, 0xff0000, 0x008000, 0x0000ff, 0xffff00, 0xff00ff, 0x00ffff, 0xff8c00,
        0x800080, 0xa52a2a, 0x808080, 0x90ee90, 0xadd8e6, 0xd3d3d3, 0x9370db, 0xe0ffff, 0xff69b4,
        0xffd700, 0x4b0082, 0xee82ee, 0x4682b4, 0xfa8072, 0xf0e68c, 0xffc0cb, 0x808000, 0x00ff00,
        0x000080, 0x008080, 0xd2b48c, 0x800000, 0xfffff0,
    ];
    if color >= 0 && (color as usize) < COLORS.len() {
        COLORS[color as usize]
    } else {
        color as u32 & 0x00ff_ffff
    }
}

fn axis_ticks_too_dense(
    min_value: f64,
    max_value: f64,
    tick_spacing: f64,
    pixel_span: f64,
    min_pixels_between_ticks: f64,
) -> bool {
    if tick_spacing <= 0.0 {
        return false;
    }
    let logical_span = max_value - min_value;
    if logical_span <= 0.0 {
        return false;
    }
    let pixels_per_tick = pixel_span.max(1.0) * (tick_spacing / logical_span);
    pixels_per_tick < min_pixels_between_ticks
}

fn build_axis_ticks(
    min_value: f64,
    max_value: f64,
    center_tick: f64,
    tick_spacing: f64,
    pixel_span: f64,
) -> Vec<f64> {
    if tick_spacing <= 0.0 || max_value < min_value {
        return Vec::new();
    }

    let n_start = ((min_value - center_tick) / tick_spacing - 1e-12).ceil();
    let n_end = ((max_value - center_tick) / tick_spacing + 1e-12).floor();
    if !n_start.is_finite() || !n_end.is_finite() || n_start > n_end {
        return Vec::new();
    }

    let pixel_span = pixel_span.max(1.0).round();
    let logical_span = max_value - min_value;
    let units_per_pixel = logical_span / pixel_span;
    let mut stride = 1.0;
    if units_per_pixel > tick_spacing {
        stride = (units_per_pixel / tick_spacing).ceil();
    }

    let max_ticks = 32.0_f64.max(pixel_span * 4.0);
    let est_count = ((n_end - n_start) / stride).floor() + 1.0;
    if est_count > max_ticks {
        stride *= (est_count / max_ticks).ceil();
    }

    let mut ticks = Vec::new();
    let mut has_center_tick = false;
    let mut n = n_start;
    let guard_limit = max_ticks as usize + 4;
    while n <= n_end + 0.5 && ticks.len() < guard_limit {
        if n.abs() < 0.5 {
            has_center_tick = true;
        }
        ticks.push(center_tick + n * tick_spacing);
        n += stride;
    }

    if n_start <= 0.0 && 0.0 <= n_end && !has_center_tick {
        ticks.push(center_tick);
        ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    }

    ticks
}

fn normalize_user_coord(value: f64) -> f64 {
    let rounded = value.round();
    if (value - rounded).abs() < 1e-9 {
        rounded
    } else {
        value
    }
}

fn format_axis_tick_label(value: f64, force_scientific: bool) -> String {
    if force_scientific {
        if !value.is_finite() || value.abs() < 5e-15 {
            return "0".to_string();
        }
        let text = format!("{value:.14E}");
        let Some((mantissa, exponent)) = text.split_once('E') else {
            return text;
        };
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        let exponent = exponent.parse::<i32>().unwrap_or(0);
        return format!("{mantissa}E{exponent:+}");
    }

    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

fn rgb_to_index_like(rgb: u32) -> i32 {
    rgb as i32
}

fn parse_gscr(screen: &str) -> BasicResult<(usize, usize, Vec<u32>)> {
    let Some((res, hex)) = screen.split_once(':') else {
        return Err(BasicError::new(ErrorCode::InvalidValue));
    };
    let Some((w, h)) = res.split_once('x') else {
        return Err(BasicError::new(ErrorCode::InvalidValue));
    };
    let w = w
        .parse::<usize>()
        .map_err(|_| BasicError::new(ErrorCode::InvalidValue))?;
    let h = h
        .parse::<usize>()
        .map_err(|_| BasicError::new(ErrorCode::InvalidValue))?;
    if hex.len() != w * h * 6 {
        return Err(BasicError::new(ErrorCode::InvalidValue));
    }
    let mut pixels = Vec::with_capacity(w * h);
    for i in (0..hex.len()).step_by(6) {
        let rgb = u32::from_str_radix(&hex[i..i + 6], 16)
            .map_err(|_| BasicError::new(ErrorCode::InvalidValue))?;
        pixels.push(rgb);
    }
    Ok((w, h, pixels))
}
