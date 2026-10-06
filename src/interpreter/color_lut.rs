use super::{
    compile_fast_number_expr, ArrayOrCallKind, BasicError, BasicResult, BinaryOp, CachedArraySlot,
    ErrorCode, Expr, FastNumberExpr, Interpreter,
};
use crate::expr::checked_number;

/// A color expression composed exclusively of integer-indexed table reads and
/// additions. Keep this separate from the general numeric evaluator: each read
/// fuses its array/INT/index dispatch, without changing the expression tree.
#[derive(Debug, Clone)]
pub(super) enum CompiledColorLutExpr {
    Lookup {
        name: String,
        slot: CachedArraySlot,
        index: FastNumberExpr,
    },
    Add {
        left: Box<Self>,
        right: Box<Self>,
    },
}

impl CompiledColorLutExpr {
    pub(super) fn compile(expr: &Expr) -> Option<Box<Self>> {
        Self::compile_node(expr).map(Box::new)
    }

    fn compile_node(expr: &Expr) -> Option<Self> {
        match expr {
            Expr::Binary {
                op: BinaryOp::Add,
                left,
                right,
            } => Some(Self::Add {
                left: Box::new(Self::compile_node(left)?),
                right: Box::new(Self::compile_node(right)?),
            }),
            Expr::ArrayOrCall { name, args, kind }
                if *kind == ArrayOrCallKind::Array && !name.ends_with('$') =>
            {
                let [Expr::ArrayOrCall {
                    name: function,
                    args: index_args,
                    kind: ArrayOrCallKind::BuiltinFunction,
                }] = args.as_slice()
                else {
                    return None;
                };
                if function != "INT" {
                    return None;
                }
                let [index] = index_args.as_slice() else {
                    return None;
                };
                Some(Self::Lookup {
                    name: name.clone(),
                    slot: CachedArraySlot::default(),
                    index: compile_fast_number_expr(index, true)?,
                })
            }
            _ => None,
        }
    }

    #[inline]
    pub(super) fn eval(&self, interpreter: &mut Interpreter) -> BasicResult<f64> {
        match self {
            Self::Lookup { name, slot, index } => {
                // INT returns an integral finite value or the original NaN/Inf.
                // Thus the general index round-trip/fract checks reduce exactly
                // to this finite check. The i32 cast retains saturation for
                // large finite indices, including the subsequent bounds error.
                let value = index.eval(interpreter)?.floor();
                if !value.is_finite() {
                    return Err(BasicError::new(ErrorCode::InvalidIndex));
                }
                // Resolve the table only after evaluating the index, matching
                // normal array reads and preserving effects/error order.
                interpreter.get_array_number_cached_1d(name, slot, value as i32)
            }
            Self::Add { left, right } => {
                let left = left.eval(interpreter)?;
                let right = right.eval(interpreter)?;
                checked_number(left + right)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{
        color_number_from_number, compile_color_expression, compile_expression,
        eval_compiled_number, ArrayValue, SimpleRng,
    };

    fn interpreter_with_tables() -> Interpreter {
        let mut interpreter = Interpreter::new();
        interpreter.graphics_window_enabled = false;
        interpreter.rng = SimpleRng::new(1234);
        for (name, multiplier) in [("R", 65536.0), ("G", 256.0), ("B", 1.0)] {
            let mut array = ArrayValue::new(name, vec![8]);
            for index in 0..=8 {
                array
                    .set_number_direct_1d(index, index as f64 * multiplier)
                    .unwrap();
            }
            interpreter.arrays.insert(name.to_string(), array);
        }
        interpreter
    }

    fn assert_same_number(actual: BasicResult<f64>, expected: BasicResult<f64>, source: &str) {
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual.to_bits(), expected.to_bits(), "{source}")
            }
            (Err(actual), Err(expected)) => {
                assert_eq!(actual.code, expected.code, "{source}");
                assert_eq!(actual.line, expected.line, "{source}");
                assert_eq!(actual.detail, expected.detail, "{source}");
            }
            (actual, expected) => panic!("{source}: {actual:?} != {expected:?}"),
        }
    }

    #[test]
    fn color_lut_recognition_is_exact_and_unsupported_indices_fall_back() {
        for source in [
            "R(INT(X))",
            "R(INT(X-1))+G(INT(Y+1))+B(INT(Z))",
            "R(INT(RND*4))+G(INT(TIME MOD 4))",
            "R(INT(X))+(G(INT(Y))+B(INT(Z)))",
        ] {
            let expr = compile_expression(source).unwrap();
            assert!(CompiledColorLutExpr::compile(&expr).is_some(), "{source}");
        }
        for source in [
            "R(X)",
            "R(INT(X),1)",
            "R(FIX(X))",
            "R(INT(X))+1",
            "R(INT(X))-G(INT(Y))",
            "R(INT(FNSTEP()))",
            "R(INT(IIF(X,1/0,1)))",
            "R$(INT(X))",
            "RGB(1,2,3)",
        ] {
            let expr = compile_expression(source).unwrap();
            assert!(CompiledColorLutExpr::compile(&expr).is_none(), "{source}");
        }
    }

    #[test]
    fn color_lut_keeps_floating_point_association_and_checked_addition() {
        for source in [
            "R(INT(0))+G(INT(0))+B(INT(0))",
            "R(INT(0))+(G(INT(0))+B(INT(0)))",
        ] {
            let expr = compile_expression(source).unwrap();
            let plan = CompiledColorLutExpr::compile(&expr).unwrap();
            for values in [
                [1.0e16, -1.0e16, 1.0],
                [-0.0, -0.0, -0.0],
                [f64::MAX, f64::MAX, 0.0],
                [f64::INFINITY, f64::NEG_INFINITY, 1.0],
                [f64::NAN, 1.0, 2.0],
            ] {
                let mut candidate = interpreter_with_tables();
                let mut reference = interpreter_with_tables();
                for interpreter in [&mut candidate, &mut reference] {
                    for (name, value) in ["R", "G", "B"].into_iter().zip(values) {
                        interpreter
                            .array_mut(name)
                            .unwrap()
                            .set_number_direct_1d(0, value)
                            .unwrap();
                    }
                }
                assert_same_number(
                    plan.eval(&mut candidate),
                    eval_compiled_number(&mut reference, &expr),
                    source,
                );
            }
        }
    }

    #[test]
    fn color_lut_int_indices_keep_ieee_conversion_and_bounds_errors() {
        let expr = compile_expression("R(INT(I))").unwrap();
        let plan = CompiledColorLutExpr::compile(&expr).unwrap();
        for index in [
            -0.0,
            0.0,
            0.9,
            1.1,
            8.9,
            -0.1,
            9.0,
            i32::MAX as f64 + 1.0,
            i32::MIN as f64 - 1.0,
            1.0e100,
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            let mut candidate = interpreter_with_tables();
            let mut reference = interpreter_with_tables();
            for interpreter in [&mut candidate, &mut reference] {
                interpreter.numeric_variables.insert("I".to_string(), index);
            }
            assert_same_number(
                plan.eval(&mut candidate),
                eval_compiled_number(&mut reference, &expr),
                "R(INT(I))",
            );
        }
    }

    #[test]
    fn color_lut_evaluates_effects_once_and_stops_at_the_first_error() {
        for (source, draws) in [
            ("R(INT(RND*4))+G(INT(RND*4))+B(INT(RND*4))", 3),
            ("R(INT(-1))+G(INT(RND*4))", 0),
            ("R(INT(RND+1/0))+G(INT(RND*4))", 1),
            ("R(INT(INF))+G(INT(RND*4))", 0),
            ("R(INT(RND*0-1))+G(INT(RND*4))", 1),
        ] {
            let expr = compile_expression(source).unwrap();
            let plan = CompiledColorLutExpr::compile(&expr).unwrap();
            let mut candidate = interpreter_with_tables();
            let mut reference = interpreter_with_tables();
            assert_same_number(
                plan.eval(&mut candidate),
                eval_compiled_number(&mut reference, &expr),
                source,
            );
            let mut expected_rng = SimpleRng::new(1234);
            for _ in 0..draws {
                expected_rng.next_f64();
            }
            let next = expected_rng.next_f64();
            assert_eq!(candidate.rng.next_f64(), next, "{source}");
            assert_eq!(reference.rng.next_f64(), next, "{source}");
        }

        let source = "R(INT(0))+G(INT(0))+B(INT(RND*0))";
        let expr = compile_expression(source).unwrap();
        let plan = CompiledColorLutExpr::compile(&expr).unwrap();
        let mut candidate = interpreter_with_tables();
        for name in ["R", "G"] {
            candidate
                .array_mut(name)
                .unwrap()
                .set_number_direct_1d(0, f64::MAX)
                .unwrap();
        }
        assert_eq!(
            plan.eval(&mut candidate).unwrap_err().code,
            ErrorCode::Overflow
        );
        assert_eq!(candidate.rng.next_f64(), SimpleRng::new(1234).next_f64());
    }

    #[test]
    fn color_lut_array_caches_rebind_and_preserve_shape_and_type_errors() {
        let expr = compile_expression("R(INT(1))").unwrap();
        let plan = CompiledColorLutExpr::compile(&expr).unwrap();
        let mut first = interpreter_with_tables();
        let mut second = interpreter_with_tables();
        second
            .array_mut("R")
            .unwrap()
            .set_number_direct_1d(1, 77.0)
            .unwrap();
        assert_eq!(plan.eval(&mut first).unwrap(), 65536.0);
        assert_eq!(plan.eval(&mut second).unwrap(), 77.0);
        first.arrays.clear();
        first
            .arrays
            .insert("R".to_string(), ArrayValue::new("R", vec![0]));
        assert_eq!(
            plan.eval(&mut first).unwrap_err().code,
            ErrorCode::IndexOutOfRange
        );

        for array in [
            ArrayValue::new("R", vec![1, 1]),
            ArrayValue::new("R$", vec![2]),
        ] {
            let mut candidate = interpreter_with_tables();
            let mut reference = interpreter_with_tables();
            candidate.arrays.insert("R".to_string(), array.clone());
            reference.arrays.insert("R".to_string(), array);
            assert_same_number(
                plan.eval(&mut candidate),
                eval_compiled_number(&mut reference, &expr),
                "R(INT(1))",
            );
        }

        let mut candidate = Interpreter::new();
        let mut reference = Interpreter::new();
        assert_same_number(
            plan.eval(&mut candidate),
            eval_compiled_number(&mut reference, &expr),
            "implicit DIM R(INT(1))",
        );
        assert_eq!(candidate.array_ref("R").unwrap().dims, vec![10]);
    }

    #[test]
    fn color_lut_compiled_color_keeps_conversion_and_error_line() {
        for source in [
            "R(INT(1.9))+G(INT(2.1))+B(INT(3))",
            "R(INT(-1))+G(INT(RND))",
        ] {
            let color = compile_color_expression(source).unwrap();
            let expr = compile_expression(source).unwrap();
            let mut candidate = interpreter_with_tables();
            let mut reference = interpreter_with_tables();
            candidate.current_line = Some(123);
            reference.current_line = Some(123);
            let actual = color.eval(&mut candidate);
            let expected = eval_compiled_number(&mut reference, &expr)
                .and_then(color_number_from_number)
                .map_err(|error| reference.with_current_line(error));
            match (actual, expected) {
                (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
                (Err(actual), Err(expected)) => {
                    assert_eq!(actual.code, expected.code);
                    assert_eq!(actual.line, Some(123));
                    assert_eq!(actual.detail, expected.detail);
                }
                (actual, expected) => panic!("{source}: {actual:?} != {expected:?}"),
            }
        }
    }

    #[test]
    fn color_lut_unsupported_effectful_index_uses_normal_fallback() {
        let source = "R(INT(FNSTEP()))";
        let color = compile_color_expression(source).unwrap();
        assert!(color.lut.is_none());
        let expr = compile_expression(source).unwrap();
        let setup = || {
            let mut interpreter = Interpreter::new();
            interpreter.graphics_window_enabled = false;
            interpreter
                .program
                .load_text(
                    "10 DIM R(1):R(0)=1\n20 END\n100 DEF FNSTEP\n\
                     110 CALLS=CALLS+1:R(0)=256\n120 FNSTEP=0\n130 FNEND",
                )
                .unwrap();
            interpreter.run_loaded().unwrap();
            // END leaves its stop request set. Start a normal prompt command
            // before evaluating the function, so its body can actually run.
            interpreter.process_immediate("CALLS=0").unwrap();
            interpreter
        };
        let mut candidate = setup();
        let mut reference = setup();
        let actual = color.eval(&mut candidate).unwrap();
        let expected = eval_compiled_number(&mut reference, &expr)
            .and_then(color_number_from_number)
            .unwrap();
        assert_eq!(actual, expected);
        for interpreter in [&candidate, &reference] {
            assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&1.0));
            assert_eq!(
                interpreter
                    .array_ref("R")
                    .unwrap()
                    .get_number(&[0])
                    .unwrap(),
                256.0
            );
        }
    }

    #[test]
    fn color_lut_cached_filled_rectangle_matches_immediate_execution() {
        let commands = [
            "SCREEN:CLG:RANDOMIZE 1234",
            "DIM R(4),G(4),B(4)",
            "FOR I=0 TO 4:R(I)=I*65536:G(I)=I*256:B(I)=I:NEXT I",
            "FRECTANGLE RND*0,RND*0,2,2,R(INT(RND*4))+G(INT(RND*4))+B(INT(RND*4))",
            "NEXTVALUE=RND",
        ];
        let mut cached = Interpreter::new();
        cached.graphics_window_enabled = false;
        let program = commands
            .iter()
            .enumerate()
            .map(|(index, command)| format!("{} {command}", (index + 1) * 10))
            .collect::<Vec<_>>()
            .join("\n");
        cached.program.load_text(&program).unwrap();
        cached.run_loaded().unwrap();

        let mut immediate = Interpreter::new();
        immediate.graphics_window_enabled = false;
        for command in commands {
            immediate.process_immediate(command).unwrap();
        }
        assert_eq!(
            cached.graphics.capture_screen(),
            immediate.graphics.capture_screen()
        );
        assert_eq!(
            cached.numeric_variables.get("NEXTVALUE"),
            immediate.numeric_variables.get("NEXTVALUE")
        );
    }
}
