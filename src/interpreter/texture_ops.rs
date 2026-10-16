//! Affine texture commands and perspective variants with a caller-owned depth array.
use super::*;

#[derive(Debug, Clone)]
enum TextureMapping {
    Affine(Box<[CompiledNumberExpr]>),
    Perspective {
        depth: String,
        values: Box<[CompiledNumberExpr]>,
        stride: usize,
    },
}

#[derive(Debug, Clone)]
pub(super) struct CompiledTexturedTriangle {
    texture: CompiledTextureSource,
    mapping: TextureMapping,
    transparent: Option<CompiledColorExpr>,
    alpha: Option<CompiledNumberExpr>,
}

#[derive(Debug, Clone)]
pub(super) struct CompiledTexturedQuad {
    texture: CompiledTextureSource,
    mapping: TextureMapping,
    transparent: Option<CompiledColorExpr>,
    alpha: Option<CompiledNumberExpr>,
}

fn compile_mapping(
    source: &str,
    vertices: usize,
) -> BasicResult<(
    CompiledTextureSource,
    TextureMapping,
    Option<CompiledColorExpr>,
    Option<CompiledNumberExpr>,
)> {
    let (source, alpha) = compile_alpha_clause(source)?;
    let args = split_arguments(source);
    let lengths = [1 + vertices * 4, 2 + vertices * 5, 2 + vertices * 8];
    let count = if lengths.contains(&args.len()) {
        args.len()
    } else if !args.is_empty() && lengths.contains(&(args.len() - 1)) {
        args.len() - 1
    } else {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    };
    if args.iter().any(|arg| arg.trim().is_empty()) {
        return Err(BasicError::new(ErrorCode::ArgumentMismatch));
    }
    let texture = compile_texture_source(&args[0])?;
    let mapping = if count == 1 + vertices * 4 {
        TextureMapping::Affine(
            args[1..count]
                .iter()
                .map(|arg| compile_number_expression(arg.trim()))
                .collect::<BasicResult<Vec<_>>>()?
                .into_boxed_slice(),
        )
    } else {
        if alpha.is_some() {
            return Err(BasicError::new(ErrorCode::ArgumentMismatch));
        }
        let name = args[1].trim();
        if !is_basic_identifier(name) {
            return Err(BasicError::new(ErrorCode::Syntax));
        }
        if name.ends_with('$') {
            return Err(BasicError::new(ErrorCode::TypeMismatch));
        }
        TextureMapping::Perspective {
            depth: name.to_ascii_uppercase(),
            values: args[2..count]
                .iter()
                .map(|arg| compile_number_expression(arg.trim()))
                .collect::<BasicResult<Vec<_>>>()?
                .into_boxed_slice(),
            stride: (count - 2) / vertices,
        }
    };
    let transparent = if count < args.len() {
        Some(compile_color_expression(args[count].trim())?)
    } else {
        None
    };
    Ok((texture, mapping, transparent, alpha))
}

pub(super) fn compile_textured_triangle_statement(
    source: &str,
) -> BasicResult<CompiledTexturedTriangle> {
    let (texture, mapping, transparent, alpha) = compile_mapping(source, 3)?;
    Ok(CompiledTexturedTriangle {
        texture,
        mapping,
        transparent,
        alpha,
    })
}

pub(super) fn compile_textured_quad_statement(source: &str) -> BasicResult<CompiledTexturedQuad> {
    let (texture, mapping, transparent, alpha) = compile_mapping(source, 4)?;
    Ok(CompiledTexturedQuad {
        texture,
        mapping,
        transparent,
        alpha,
    })
}

impl Interpreter {
    pub(super) fn execute_compiled_textured_triangle(
        &mut self,
        compiled: &CompiledTexturedTriangle,
    ) -> BasicResult<()> {
        self.execute_compiled_textured_polygon::<3>(
            &compiled.texture,
            &compiled.mapping,
            compiled.transparent.as_ref(),
            compiled.alpha.as_ref(),
        )
    }

    pub(super) fn execute_compiled_textured_quad(
        &mut self,
        compiled: &CompiledTexturedQuad,
    ) -> BasicResult<()> {
        self.execute_compiled_textured_polygon::<4>(
            &compiled.texture,
            &compiled.mapping,
            compiled.transparent.as_ref(),
            compiled.alpha.as_ref(),
        )
    }

    fn execute_compiled_textured_polygon<const N: usize>(
        &mut self,
        source: &CompiledTextureSource,
        mapping: &TextureMapping,
        transparent: Option<&CompiledColorExpr>,
        alpha: Option<&CompiledNumberExpr>,
    ) -> BasicResult<()> {
        let blocked = self.graphics_window_blocked();
        if matches!(mapping, TextureMapping::Affine(_)) && !blocked {
            self.ensure_graphics_window()?;
        }
        let texture = self.eval_compiled_texture(source)?;
        match mapping {
            TextureMapping::Affine(expressions) => {
                let mut vertices = [[0.0; 4]; N];
                for (i, expr) in expressions.iter().enumerate() {
                    vertices[i / 4][i % 4] = self.eval_texture_number(expr)?;
                }
                let transparent = transparent.map(|expr| expr.eval(self)).transpose()?;
                let alpha = alpha.map(|expr| self.eval_alpha(expr)).transpose()?;
                let [x0, y0, u0, v0] = vertices[0];
                let [x1, y1, u1, v1] = vertices[1];
                let [x2, y2, u2, v2] = vertices[2];
                if let Some(alpha) = alpha {
                    if N == 3 {
                        self.graphics.textured_triangle_with_alpha(
                            &texture,
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
                    } else {
                        let [x3, y3, u3, v3] = vertices[3];
                        self.graphics.textured_quad_with_alpha(
                            &texture,
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
                            alpha,
                        )?;
                    }
                } else if N == 3 {
                    self.graphics.textured_triangle_with_transparency(
                        &texture,
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
                    )?;
                } else {
                    let [x3, y3, u3, v3] = vertices[3];
                    self.graphics.textured_quad_with_transparency(
                        &texture,
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
                    )?;
                }
            }
            TextureMapping::Perspective {
                depth,
                values,
                stride,
            } => {
                let mut vertices = [[0.0, 0.0, 1.0, 0.0, 0.0, 255.0, 255.0, 255.0]; N];
                // Functions in these expressions may REDIM or rebind the array.
                // Evaluate once, left to right, and resolve the depth alias last.
                for (i, expr) in values.iter().enumerate() {
                    vertices[i / stride][i % stride] =
                        expr.eval(self).map_err(|e| self.with_current_line(e))?;
                }
                let transparent = transparent.map(|expr| expr.eval(self)).transpose()?;
                if !blocked && !self.graphics_window_ready_for_program_drawing() {
                    self.ensure_graphics_window()?;
                }
                let key = self.array_lookup_key(depth).into_owned();
                let error_line = self.current_line;
                let error = |code| {
                    let mut error = BasicError::new(code);
                    error.line = error_line;
                    error
                };
                let array = self
                    .arrays
                    .get_mut(&key)
                    .ok_or_else(|| error(ErrorCode::InvalidArgument))?;
                if array.dims.len() != 2 {
                    return Err(error(ErrorCode::InvalidIndex));
                }
                let ArrayData::Number(ref mut depth) = array.data else {
                    return Err(error(ErrorCode::TypeMismatch));
                };
                let width = array.dims[0] + 1;
                let height = array.dims[1] + 1;
                let result = if N == 3 {
                    self.graphics
                        .perspective_textured_triangle_with_transparency(
                            &texture,
                            [vertices[0], vertices[1], vertices[2]],
                            depth,
                            width,
                            height,
                            transparent,
                        )
                } else {
                    self.graphics.perspective_textured_quad_with_transparency(
                        &texture,
                        [vertices[0], vertices[1], vertices[2], vertices[3]],
                        depth,
                        width,
                        height,
                        transparent,
                    )
                };
                result.map_err(|mut error| {
                    if error.line.is_none() {
                        error.line = error_line;
                    }
                    error
                })?;
                array.clear_debug_write();
            }
        }
        if blocked {
            Ok(())
        } else {
            self.refresh_graphics_window_after_ensure()
        }
    }
}
