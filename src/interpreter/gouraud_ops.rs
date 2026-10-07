//! Generic perspective Gouraud triangle command with a caller-owned depth array.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct CompiledGouraud {
    depth: String,
    values: Box<[CompiledNumberExpr]>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headless() -> Interpreter {
        let mut interpreter = Interpreter::new();
        interpreter.graphics_window_enabled = false;
        interpreter
            .process_immediate("MODE 640:DIM D(639,479)")
            .unwrap();
        interpreter
    }

    const ARGS: &str = "D,10,10,.5,255,0,0,110,10,1,0,255,0,10,110,.25,0,0,255";

    #[test]
    fn raw_and_cached_command_match_rgb_depth_and_cursor() {
        let mut raw = headless();
        raw.process_immediate(&format!("GTRIANGLE {ARGS}")).unwrap();
        let mut cached = headless();
        let command = cached.compile_cached_command(&format!("GTRIANGLE {ARGS}"));
        assert!(matches!(command, CachedCommand::Gouraud(_)));
        let CachedCommand::Gouraud(compiled) = command else {
            unreachable!()
        };
        cached.execute_compiled_gouraud(&compiled).unwrap();
        assert_eq!(raw.graphics.buffer(), cached.graphics.buffer());
        assert_eq!(
            raw.array_ref("D").unwrap().get_number(&[30, 30]).unwrap(),
            cached
                .array_ref("D")
                .unwrap()
                .get_number(&[30, 30])
                .unwrap()
        );
        assert_eq!(raw.graphics.buffer()[(479 - 30) * 640 + 30], 0x885f18);
        assert_eq!(raw.graphics.xpos(), cached.graphics.xpos());
        assert_eq!(raw.graphics.ypos(), cached.graphics.ypos());
    }

    #[test]
    fn cached_depth_alias_follows_rebinding_and_redimension() {
        let mut interpreter = headless();
        interpreter.process_immediate("DIM E(639,479)").unwrap();
        interpreter.set_array_alias_binding("P", "D".to_owned());
        let compiled = CompiledGouraud::compile(&ARGS.replacen('D', "P", 1)).unwrap();
        interpreter.execute_compiled_gouraud(&compiled).unwrap();
        assert!(
            interpreter
                .array_ref("D")
                .unwrap()
                .get_number(&[30, 30])
                .unwrap()
                > 0.0
        );
        interpreter.set_array_alias_binding("P", "E".to_owned());
        interpreter.execute_compiled_gouraud(&compiled).unwrap();
        assert!(
            interpreter
                .array_ref("E")
                .unwrap()
                .get_number(&[30, 30])
                .unwrap()
                > 0.0
        );
        interpreter.remove_array_alias_binding("P");
        interpreter.process_immediate("DIM P(2,2)").unwrap();
        let before = interpreter.graphics.buffer().to_vec();
        let error = interpreter.execute_compiled_gouraud(&compiled).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidValue);
        assert_eq!(before, interpreter.graphics.buffer());
    }

    #[test]
    fn argument_failures_keep_depth_and_first_error_line() {
        let mut interpreter = headless();
        interpreter.current_line = Some(240);
        let compiled =
            CompiledGouraud::compile("D,1/0,10,1,255,0,0,20,10,1,255,0,0,10,20,1,255,0,0").unwrap();
        let before = interpreter.graphics.buffer().to_vec();
        let error = interpreter.execute_compiled_gouraud(&compiled).unwrap_err();
        assert_eq!(error.code, ErrorCode::DivisionByZero);
        assert_eq!(error.line, Some(240));
        assert_eq!(before, interpreter.graphics.buffer());
        assert_eq!(
            interpreter
                .array_ref("D")
                .unwrap()
                .get_number(&[10, 10])
                .unwrap(),
            0.0
        );
        assert_eq!(
            CompiledGouraud::compile("D,1,2").unwrap_err().code,
            ErrorCode::ArgumentMismatch
        );
        assert_eq!(
            CompiledGouraud::compile(&ARGS.replacen('D', "D$", 1))
                .unwrap_err()
                .code,
            ErrorCode::TypeMismatch
        );
    }
}

impl CompiledGouraud {
    pub(super) fn compile(source: &str) -> BasicResult<Self> {
        let args = split_arguments(source);
        if args.len() != 19 || args.iter().any(|a| a.trim().is_empty()) {
            return Err(BasicError::new(ErrorCode::ArgumentMismatch));
        }
        let name = args[0].trim();
        if !is_basic_identifier(name) {
            return Err(BasicError::new(ErrorCode::Syntax));
        }
        if name.ends_with('$') {
            return Err(BasicError::new(ErrorCode::TypeMismatch));
        }
        let values = args[1..]
            .iter()
            .map(|s| compile_number_expression(s.trim()))
            .collect::<BasicResult<Vec<_>>>()?;
        Ok(Self {
            depth: name.to_ascii_uppercase(),
            values: values.into_boxed_slice(),
        })
    }
}

impl Interpreter {
    pub(super) fn execute_compiled_gouraud(
        &mut self,
        compiled: &CompiledGouraud,
    ) -> BasicResult<()> {
        // An expression can call a function and rebind/REDIM the depth array.
        // Resolve the array only after all expressions have been evaluated.
        let mut vertices = [[0.0; 6]; 3];
        for (i, expr) in compiled.values.iter().enumerate() {
            vertices[i / 6][i % 6] = expr.eval(self).map_err(|e| self.with_current_line(e))?;
        }
        let blocked = self.graphics_window_blocked();
        if !blocked && !self.graphics_window_ready_for_program_drawing() {
            self.ensure_graphics_window()?;
        }
        let key = self.array_lookup_key(&compiled.depth).into_owned();
        let error_line = self.current_line;
        let error = |code| {
            let mut e = BasicError::new(code);
            e.line = error_line;
            e
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
        self.graphics
            .gouraud_triangle(vertices, depth, array.dims[0] + 1, array.dims[1] + 1)
            .map_err(|mut e| {
                if e.line.is_none() {
                    e.line = error_line;
                }
                e
            })?;
        array.clear_debug_write();
        if blocked {
            Ok(())
        } else {
            self.refresh_graphics_window_after_ensure()
        }
    }
}
