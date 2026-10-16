//! Explicit opacity for immediate 2D drawing. The perspective/depth rasterizer
//! deliberately has no alpha path.
use super::*;

pub(super) fn validate_alpha(alpha: f64) -> BasicResult<()> {
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    Ok(())
}

pub(super) fn blend_rgb(source: u32, destination: u32, alpha: f64) -> u32 {
    let channel = |shift: u32| {
        let source = ((source >> shift) & 255u32) as f64;
        let destination = ((destination >> shift) & 255u32) as f64;
        (destination + (source - destination) * alpha + 0.5).floor() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn validate_values(values: &[f64]) -> BasicResult<()> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(BasicError::new(ErrorCode::InvalidArgument));
    }
    Ok(())
}

impl Graphics {
    #[allow(clippy::too_many_arguments)]
    pub fn textured_rect_with_alpha(
        &mut self,
        texture: &Texture,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        uv: Option<(f64, f64, f64, f64)>,
        transparent: Option<i32>,
        alpha: f64,
    ) -> BasicResult<()> {
        validate_alpha(alpha)?;
        if alpha == 1.0 {
            return self.textured_rect_with_transparency(texture, x0, y0, x1, y1, uv, transparent);
        }
        let (u0, v0, u1, v1) =
            uv.unwrap_or((0.0, 0.0, texture.width as f64, texture.height as f64));
        self.textured_quad_with_alpha(
            texture,
            x0,
            y0,
            u0,
            v0,
            x1,
            y0,
            u1,
            v0,
            x1,
            y1,
            u1,
            v1,
            x0,
            y1,
            u0,
            v1,
            transparent,
            alpha,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn textured_quad_with_alpha(
        &mut self,
        texture: &Texture,
        x0: f64,
        y0: f64,
        u0: f64,
        v0: f64,
        x1: f64,
        y1: f64,
        u1: f64,
        v1: f64,
        x2: f64,
        y2: f64,
        u2: f64,
        v2: f64,
        x3: f64,
        y3: f64,
        u3: f64,
        v3: f64,
        transparent: Option<i32>,
        alpha: f64,
    ) -> BasicResult<()> {
        validate_alpha(alpha)?;
        validate_values(&[
            x0, y0, u0, v0, x1, y1, u1, v1, x2, y2, u2, v2, x3, y3, u3, v3,
        ])?;
        if alpha == 1.0 {
            return self.textured_quad_with_transparency(
                texture,
                x0,
                y0,
                u0,
                v0,
                x1,
                y1,
                u1,
                v1,
                x2,
                y2,
                u2,
                v2,
                x3,
                y3,
                u3,
                v3,
                transparent,
            );
        }
        self.textured_triangle_with_alpha(
            texture,
            x0,
            y0,
            u0,
            v0,
            x1,
            y1,
            u1,
            v1,
            x2,
            y2,
            u2,
            v2,
            transparent,
            alpha,
        )?;
        self.textured_triangle_with_alpha(
            texture,
            x0,
            y0,
            u0,
            v0,
            x2,
            y2,
            u2,
            v2,
            x3,
            y3,
            u3,
            v3,
            transparent,
            alpha,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn textured_triangle_with_alpha(
        &mut self,
        texture: &Texture,
        x0: f64,
        y0: f64,
        u0: f64,
        v0: f64,
        x1: f64,
        y1: f64,
        u1: f64,
        v1: f64,
        x2: f64,
        y2: f64,
        u2: f64,
        v2: f64,
        transparent: Option<i32>,
        alpha: f64,
    ) -> BasicResult<()> {
        validate_alpha(alpha)?;
        validate_values(&[x0, y0, u0, v0, x1, y1, u1, v1, x2, y2, u2, v2])?;
        if alpha == 1.0 {
            return self.textured_triangle_with_transparency(
                texture,
                x0,
                y0,
                u0,
                v0,
                x1,
                y1,
                u1,
                v1,
                x2,
                y2,
                u2,
                v2,
                transparent,
            );
        }
        let points = [
            self.user_to_canvas(x0, y0),
            self.user_to_canvas(x1, y1),
            self.user_to_canvas(x2, y2),
        ];
        if alpha > 0.0 {
            let vertices = std::array::from_fn(|i| TexturedVertex {
                x: points[i].0,
                y: points[i].1,
                u: [u0, u1, u2][i],
                v: [v0, v1, v2][i],
            });
            self.textured_triangle_alpha_canvas(
                texture,
                vertices,
                transparent.map(resolve_color_number),
                alpha,
            );
        }
        self.cursor_x = points[2].0;
        self.cursor_y = self.canvas_y_to_logical(points[2].1);
        self.cursor_user_x = x2;
        self.cursor_user_y = y2;
        Ok(())
    }

    /// Each edge belongs to one of its two neighboring triangles. This
    /// prevents double blending along a quad diagonal, including when the
    /// caller draws the two triangles separately or reverses their winding.
    fn textured_triangle_alpha_canvas(
        &mut self,
        texture: &Texture,
        mut vertices: [TexturedVertex; 3],
        transparent: Option<u32>,
        alpha: f64,
    ) {
        let edge = |a: TexturedVertex, b: TexturedVertex, x: f64, y: f64| {
            (x - a.x as f64) * (b.y as f64 - a.y as f64)
                - (y - a.y as f64) * (b.x as f64 - a.x as f64)
        };
        let mut area = edge(
            vertices[0],
            vertices[1],
            vertices[2].x as f64,
            vertices[2].y as f64,
        );
        if area == 0.0 {
            return;
        }
        if area < 0.0 {
            vertices.swap(1, 2);
            area = -area;
        }
        let Some((clip_left, clip_right, clip_top, clip_bottom)) = self.drawable_bounds() else {
            return;
        };
        let left = vertices.iter().map(|v| v.x).min().unwrap().max(clip_left);
        let right = vertices.iter().map(|v| v.x).max().unwrap().min(clip_right);
        let top = vertices.iter().map(|v| v.y).min().unwrap().max(clip_top);
        let bottom = vertices.iter().map(|v| v.y).max().unwrap().min(clip_bottom);
        let owns_edge =
            |a: TexturedVertex, b: TexturedVertex| b.y > a.y || (b.y == a.y && b.x < a.x);
        let inclusive = [
            owns_edge(vertices[1], vertices[2]),
            owns_edge(vertices[2], vertices[0]),
            owns_edge(vertices[0], vertices[1]),
        ];
        let mut wrote_pixel = false;
        for y in top..=bottom {
            for x in left..=right {
                let px = x as f64 + 0.5;
                let py = y as f64 + 0.5;
                let weights = [
                    edge(vertices[1], vertices[2], px, py),
                    edge(vertices[2], vertices[0], px, py),
                    edge(vertices[0], vertices[1], px, py),
                ];
                if !(0..3).all(|i| weights[i] > 0.0 || (weights[i] == 0.0 && inclusive[i])) {
                    continue;
                }
                let u = (0..3).map(|i| vertices[i].u * (weights[i] / area)).sum();
                let v = (0..3).map(|i| vertices[i].v * (weights[i] / area)).sum();
                let texel = texture.sample_wrapped(u, v);
                if Some(texel) != transparent {
                    let index = y as usize * self.width + x as usize;
                    self.buffer[index] = blend_rgb(texel, self.buffer[index], alpha);
                    wrote_pixel = true;
                }
            }
        }
        self.buffer_dirty |= wrote_pixel;
    }

    pub fn filled_rectangle_with_alpha(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        color: Option<i32>,
        alpha: f64,
    ) -> BasicResult<()> {
        validate_alpha(alpha)?;
        validate_values(&[x0, y0, x1, y1])?;
        if alpha == 1.0 {
            self.rectangle(x0, y0, x1, y1, color, true);
            return Ok(());
        }
        let (ax, ay) = self.user_to_canvas(x0, y0);
        let (bx, by) = self.user_to_canvas(x1, y1);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        let Some((left, right, top, bottom)) = self.drawable_bounds() else {
            return Ok(());
        };
        for y in ay.min(by).max(top)..=ay.max(by).min(bottom) {
            self.blend_scanline(y, ax.min(bx).max(left), ax.max(bx).min(right), color, alpha);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn filled_triangle_with_alpha(
        &mut self,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: Option<i32>,
        alpha: f64,
    ) -> BasicResult<()> {
        validate_alpha(alpha)?;
        validate_values(&[x0, y0, x1, y1, x2, y2])?;
        if alpha == 1.0 {
            self.triangle(x0, y0, x1, y1, x2, y2, color, true);
            return Ok(());
        }
        let p0 = self.user_to_canvas(x0, y0);
        let p1 = self.user_to_canvas(x1, y1);
        let p2 = self.user_to_canvas(x2, y2);
        let color = color
            .map(resolve_color_number)
            .unwrap_or(self.current_color);
        // Preserve the inclusive filled-triangle silhouette, but visit the
        // shared middle scanline once rather than filling both halves over it.
        let mut points = [p0, p1, p2];
        points.sort_by_key(|p| p.1);
        let [(ax, ay), (bx, by), (cx, cy)] = points;
        if alpha > 0.0 && ay != cy {
            if let Some((clip_left, clip_right, top, bottom)) = self.drawable_bounds() {
                let split_x = ax as i128
                    + ((cx as i128 - ax as i128) * (by as i128 - ay as i128))
                        .div_euclid(cy as i128 - ay as i128);
                let half_span = |start_a: i128,
                                 start_b: i128,
                                 end_a: i128,
                                 end_b: i128,
                                 from: i32,
                                 to: i32,
                                 y: i32| {
                    let step = |start: i128, end: i128| {
                        let slope = ((end - start) << 16).div_euclid(to as i128 - from as i128);
                        ((start << 16) + slope * (y as i128 - from as i128)) >> 16
                    };
                    let a = step(start_a, end_a);
                    let b = step(start_b, end_b);
                    (a.min(b), a.max(b))
                };
                for y in ay.max(top)..=cy.min(bottom) {
                    let (mut left, mut right) = if by == cy {
                        half_span(ax as i128, ax as i128, bx as i128, cx as i128, ay, by, y)
                    } else if ay == by {
                        half_span(ax as i128, bx as i128, cx as i128, cx as i128, by, cy, y)
                    } else if y <= by {
                        half_span(ax as i128, ax as i128, bx as i128, split_x, ay, by, y)
                    } else {
                        half_span(bx as i128, split_x, cx as i128, cx as i128, by, cy, y)
                    };
                    if y == by && ay != by && by != cy {
                        // Both inclusive legacy halves cover this row. Their
                        // fixed-point rounding can differ by one pixel.
                        let lower =
                            half_span(bx as i128, split_x, cx as i128, cx as i128, by, cy, y);
                        left = left.min(lower.0);
                        right = right.max(lower.1);
                    }
                    left = left.max(clip_left as i128);
                    right = right.min(clip_right as i128);
                    if left <= right {
                        self.blend_scanline(y, left as i32, right as i32, color, alpha);
                    }
                }
            }
        }
        self.cursor_x = p2.0;
        self.cursor_y = self.canvas_y_to_logical(p2.1);
        self.cursor_user_x = x2;
        self.cursor_user_y = y2;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn filled_circle_with_alpha(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        color: Option<i32>,
        start_angle: Option<f64>,
        end_angle: Option<f64>,
        aspect: f64,
        alpha: f64,
    ) -> BasicResult<()> {
        validate_alpha(alpha)?;
        validate_values(&[x, y, radius, aspect])?;
        validate_values(&start_angle.into_iter().chain(end_angle).collect::<Vec<_>>())?;
        if radius <= 0.0 || aspect <= 0.0 {
            return Err(BasicError::new(ErrorCode::InvalidArgument));
        }
        if alpha == 1.0 {
            return self.circle_arc(x, y, radius, color, true, start_angle, end_angle, aspect);
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
        let (start, end) = if full_circle {
            (0.0, std::f64::consts::TAU)
        } else {
            let start = start_angle.unwrap().rem_euclid(std::f64::consts::TAU);
            let mut end = end_angle.unwrap().rem_euclid(std::f64::consts::TAU);
            if end < start {
                end += std::f64::consts::TAU;
            }
            (start, end)
        };
        let Some((left, right, top, bottom)) = self.drawable_bounds() else {
            return Ok(());
        };
        if alpha == 0.0 {
            return Ok(());
        }
        let draw_top = ((cy as f64 - ry).trunc() as i32).max(top);
        let draw_bottom = ((cy as f64 + ry).trunc() as i32).min(bottom);
        for y in draw_top..=draw_bottom {
            let dy = y as f64 - cy as f64;
            let span = (1.0 - (dy / ry).powi(2)).max(0.0).sqrt() * rx;
            let x_left = ((cx as f64 - span).trunc() as i32).max(left);
            let x_right = ((cx as f64 + span).trunc() as i32).min(right);
            if full_circle {
                self.blend_scanline(y, x_left, x_right, color, alpha);
            } else {
                for x in x_left..=x_right {
                    let mut angle = (dy / ry).atan2((x as f64 - cx as f64) / rx);
                    if angle < start {
                        angle += std::f64::consts::TAU;
                    }
                    if angle >= start && angle <= end {
                        self.blend_scanline(y, x, x, color, alpha);
                    }
                }
            }
        }
        Ok(())
    }

    fn blend_scanline(&mut self, y: i32, left: i32, right: i32, color: u32, alpha: f64) {
        if alpha == 0.0 {
            return;
        }
        let left = left.max(0).max(self.w_left);
        let right = right.min(self.width as i32 - 1).min(self.w_right);
        if left > right {
            return;
        }
        let row = y as usize * self.width;
        for pixel in &mut self.buffer[row + left as usize..=row + right as usize] {
            *pixel = blend_rgb(color, *pixel, alpha);
        }
        self.buffer_dirty = true;
    }
}
