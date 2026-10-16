//! Local opacity clauses for 2D drawing, introduced by the reserved ALPHA keyword.
use super::*;

pub(super) fn compile_alpha_clause(
    source: &str,
) -> BasicResult<(&str, Option<CompiledNumberExpr>)> {
    let bytes = source.as_bytes();
    let mut quoted = false;
    let mut depth = 0i32;
    for (index, ch) in source.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            'a' | 'A' if !quoted && depth == 0 => {
                if index == 0
                    || !bytes[index - 1].is_ascii_whitespace()
                    || !bytes
                        .get(index..index + 5)
                        .is_some_and(|token| token.eq_ignore_ascii_case(b"ALPHA"))
                    || bytes
                        .get(index + 5)
                        .is_some_and(|ch| !ch.is_ascii_whitespace())
                {
                    continue;
                }
                let prefix = source[..index].trim_end();
                // The suffix follows complete drawing arguments, not a comma or operator.
                let Some(last) = prefix.chars().last() else {
                    continue;
                };
                if ",+-*/\\^=<>(&|".contains(last) {
                    continue;
                }
                let word = prefix
                    .rsplit(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
                    .next()
                    .unwrap_or("");
                if ["MOD", "AND", "OR", "XOR", "NOT", "EQV", "IMP"]
                    .iter()
                    .any(|operator| word.eq_ignore_ascii_case(operator))
                {
                    continue;
                }
                let expression = source[index + 5..].trim();
                if expression.is_empty() {
                    return Err(BasicError::new(ErrorCode::ArgumentMismatch));
                }
                return Ok((prefix, Some(compile_number_expression(expression)?)));
            }
            _ => {}
        }
    }
    Ok((source, None))
}

impl Interpreter {
    pub(super) fn eval_alpha(&mut self, expression: &CompiledNumberExpr) -> BasicResult<f64> {
        let alpha = expression
            .eval(self)
            .map_err(|error| self.with_current_line(error))?;
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(self.err(ErrorCode::InvalidArgument));
        }
        Ok(alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_does_not_capture_quoted_text_or_other_identifiers() {
        for source in [
            "T$,0,0,OPACITY,20",
            "T$,0,0,20, OPACITY",
            "T$,0,0,20,2 + OPACITY",
            "T$,0,0,20,2 MOD OPACITY",
            "T$,0,0,20,(OPACITY)",
            "T$,0,0,20,OPACITY(1)",
            "\"ALPHA .5\",0,0,20,20",
            "T$,0,0,20,ALPHABET",
            "T$,0,0,20,1 AND OPACITY",
        ] {
            let (body, alpha) = compile_alpha_clause(source).unwrap();
            assert_eq!(body, source);
            assert!(alpha.is_none(), "{source}");
        }
        for source in [
            "T$,0,0,20, OPACITY ALPHA .5",
            "T$,0,0,20,20 alpha (OPACITY/2)",
            "T$,0,0,20,2 + OPACITY ALPHA .5",
        ] {
            assert!(
                compile_alpha_clause(source).unwrap().1.is_some(),
                "{source}"
            );
        }
    }

    #[test]
    fn cached_and_immediate_2d_alpha_commands_match() {
        let commands = [
            "FRECTANGLE 10,10,90,90,RGB(200,100,50) ALPHA .4",
            "FTRIANGLE 10,10,90,10,40,90,RGB(200,100,50) ALPHA .4",
            "FCIRCLE 50,50,30,RGB(200,100,50) ALPHA .4",
            "FCIRCLER 30,30,20,RGB(200,100,50),0,180,.7 ALPHA .4",
            "TRECTANGLE T$,10,10,90,90 ALPHA .4",
            "TTRIANGLE T$,10,10,0,0,90,10,1,0,40,90,0,1 ALPHA .4",
            "TQUAD T$,10,10,0,0,90,10,1,0,90,90,1,1,10,90,0,1 ALPHA .4",
        ];
        let setup = || {
            let mut interpreter = Interpreter::new();
            interpreter.graphics_window_enabled = false;
            interpreter
                .process_immediate("PAPER RGB(20,40,80):CLG:T$=\"1x1:c86432\":MOVE 10,20")
                .unwrap();
            interpreter
        };
        for source in commands {
            let mut immediate = setup();
            immediate.process_immediate(source).unwrap();
            let mut cached = setup();
            let command = cached.compile_cached_command(source);
            match command {
                CachedCommand::Rect(compiled) => cached.execute_compiled_rect(&compiled),
                CachedCommand::Triangle(compiled) => cached.execute_compiled_triangle(&compiled),
                CachedCommand::Circle(compiled) => cached.execute_compiled_circle(&compiled),
                CachedCommand::TexturedRect(compiled) => {
                    cached.execute_compiled_textured_rect(&compiled)
                }
                CachedCommand::TexturedTriangle(compiled) => {
                    cached.execute_compiled_textured_triangle(&compiled)
                }
                CachedCommand::TexturedQuad(compiled) => {
                    cached.execute_compiled_textured_quad(&compiled)
                }
                other => panic!("alpha command fell back to raw parsing: {other:?}"),
            }
            .unwrap();
            assert_eq!(
                immediate.graphics.buffer(),
                cached.graphics.buffer(),
                "{source}"
            );
            assert_eq!(
                (immediate.graphics.xpos(), immediate.graphics.ypos()),
                (cached.graphics.xpos(), cached.graphics.ypos()),
                "{source}"
            );
        }
    }
}
