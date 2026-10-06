//! Differential checks for syntax cached once and executed against changing state.
use super::*;
use crate::expr::eval_expression;

fn setup(interpreter: &mut Interpreter, source: &str) {
    interpreter.process_immediate(source).unwrap();
}

fn assert_same_array(left: &Interpreter, right: &Interpreter, name: &str) {
    let left = left.array_ref(name).unwrap();
    let right = right.array_ref(name).unwrap();
    assert_eq!(left.dims, right.dims, "{name}");
    match (&left.data, &right.data) {
        (ArrayData::Number(left), ArrayData::Number(right)) => {
            assert_eq!(left.len(), right.len(), "{name}");
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                if left.is_nan() {
                    assert!(right.is_nan(), "{name}[{index}]");
                } else {
                    assert_eq!(left.to_bits(), right.to_bits(), "{name}[{index}]");
                }
            }
        }
        (ArrayData::Str(left), ArrayData::Str(right)) => assert_eq!(left, right, "{name}"),
        _ => panic!("different array types: {name}"),
    }
}

fn replay_pair(
    cached: &mut Interpreter,
    raw: &mut Interpreter,
    compiled: &CompiledMatAssignment,
    source: &str,
) {
    let cached_result = cached.execute_compiled_mat_assignment(compiled);
    let raw_result = raw.execute_mat_assignment(source);
    match (cached_result, raw_result) {
        (Ok(()), Ok(())) => {}
        (Err(cached), Err(raw)) => {
            assert_eq!(cached.code, raw.code, "{source}");
            assert_eq!(cached.line, raw.line, "{source}");
            assert_eq!(cached.detail, raw.detail, "{source}");
        }
        (cached, raw) => panic!("{source}: cached={cached:?}, raw={raw:?}"),
    }
}

#[test]
fn cached_mat_compiles_expression_syntax_without_running_effectful_calls() {
    let mut interpreter = Interpreter::new();
    for source in [
        "MAT C=RND",
        "MAT C=RND()",
        "MAT C=TIME",
        "MAT C=TIME()",
        "MAT C=IIF(1,2,1/0)",
        "MAT C=FNVALUE()",
        "MAT C=A+FNVALUE()",
        "MAT C=A+ABS(1)",
        "MAT C=(2*A/3)",
        "MAT C=A/2",
        "MAT C=A^2",
        "MAT C=TRN(A)",
        "MAT C=INV(A)",
        "MAT C$=A$+\"x\"",
        "MAT C=A(1)",
    ] {
        assert!(
            matches!(
                interpreter.compile_cached_command(source),
                CachedCommand::MatAssignment(_)
            ),
            "{source} should cache syntax without binding runtime operands"
        );
    }
    for source in ["MAT C=CON", "MAT C=ZER", "MAT C=IDN"] {
        assert!(matches!(
            interpreter.compile_cached_command(source),
            CachedCommand::Raw(_)
        ));
    }
    assert!(matches!(
        interpreter.compile_cached_command("MAT C=(A-B)*.5"),
        CachedCommand::MatAssignment(_)
    ));
    // Compilation must not execute or reject a statement in an untaken branch.
    interpreter
        .program
        .load_text("10 IF 0 THEN MAT C=FNNOTDEFINED()\n20 IF 0 THEN MAT C=A/0\n30 DONE=1")
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.numeric_variables.get("DONE"), Some(&1.0));
    assert!(!interpreter.array_exists("C"));
}

#[test]
fn cached_mat_replay_resolves_scalar_array_resize_base_and_clear_dynamically() {
    let source = "C=A*2";
    let compiled = CompiledMatAssignment::compile(source).unwrap();
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for initialization in [
        "A=3",
        "DIM A(2):MAT A=4",
        "MAT BASE 1:REDIM A(4):MAT A=5",
        "CLEAR:A=9",
    ] {
        setup(&mut cached, initialization);
        setup(&mut raw, initialization);
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "C");
    }
}

#[test]
fn cached_mat_self_assignment_declares_target_before_reading_it() {
    let source = "A=A+1";
    let compiled = CompiledMatAssignment::compile(source).unwrap();
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for _ in 0..3 {
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "A");
    }
    assert_eq!(cached.array_ref("A").unwrap().dims, vec![10]);
    assert_eq!(
        cached
            .array_ref("A")
            .unwrap()
            .get_number_direct_1d(0)
            .unwrap(),
        3.0
    );
}

#[test]
fn cached_mat_replay_preserves_float_order_nonfinite_values_and_failure_state() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        setup(interpreter, "DIM A(1),B(2),T(1,1,1):MAT A=4:MAT C=19");
        interpreter.current_line = Some(321);
    }
    for source in [
        "C=1E16-1E16+1",
        "C=-(1E16-1E16)+1",
        "C=-0",
        "C=1E309",
        "C=-1E309",
        "C=1E309-1E309",
        "C=A*0.5+A",
        "C=A+B",
        "C=T*2",
    ] {
        let compiled = CompiledMatAssignment::compile(source).expect(source);
        replay_pair(&mut cached, &mut raw, &compiled, source);
        for name in ["A", "B", "C", "T"] {
            assert_same_array(&cached, &raw, name);
        }
    }
}

#[test]
fn cached_mat_body_rebinds_sub_aliases_locals_and_function_return_targets() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 DIM A(1),B(2),P(1)\n\
         20 MAT A=2:MAT B=3:MAT P=99\n\
         30 DEF SUB RESCALE(P)\n\
         40 LOCAL L(1)\n\
         50 MAT L=P+1\n\
         60 MAT P=L*2\n\
         70 SUBEND\n\
         80 CALL RESCALE(A):CALL RESCALE(B)\n\
         90 MAT D=FNCOPY(A):MAT E=FNCOPY(B)\n\
         100 END\n\
         200 DEF FNCOPY(X)\n\
         210 MAT FNCOPY=X+1\n\
         220 FNEND",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    for (name, bound, expected) in [
        ("A", 1, 6.0),
        ("B", 2, 8.0),
        ("P", 1, 99.0),
        ("D", 1, 7.0),
        ("E", 2, 9.0),
    ] {
        let array = interpreter.array_ref(name).unwrap();
        assert_eq!(array.dims, vec![bound], "{name}");
        for index in 0..=bound {
            assert_eq!(
                array.get_number_direct_1d(index as i32).unwrap(),
                expected,
                "{name}"
            );
        }
    }
    assert!(!interpreter.array_exists("L"));
    assert!(interpreter.array_aliases.is_empty());
    assert!(interpreter.active_functions.is_empty());
    assert!(interpreter.active_subs.is_empty());
}

#[test]
fn cached_mat_errors_keep_trace_line_and_resume_next_destination() {
    let mut interpreter = Interpreter::new();
    interpreter.ansi_output = false;
    assert!(matches!(
        interpreter.compile_cached_command("MAT C=A+B"),
        CachedCommand::MatAssignment(_)
    ));
    interpreter
        .program
        .load_text(
            "10 DIM A(1),B(2),C(1):MAT A=2:MAT B=3:MAT C=17\n\
         20 ON ERROR GOTO 100\n\
         30 TRON\n\
         40 MAT C=A+B\n\
         50 TROFF\n\
         60 AFTERERROR=C(0):DONE=1:END\n\
         100 LASTERR=ERR:LASTLINE=ERL\n\
         110 RESUME NEXT",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.numeric_variables.get("LASTLINE"), Some(&40.0));
    assert_eq!(
        interpreter.numeric_variables.get("LASTERR"),
        Some(&(basic_error_number(&BasicError::new(ErrorCode::InvalidDimensions)) as f64))
    );
    assert_eq!(interpreter.numeric_variables.get("AFTERERROR"), Some(&17.0));
    assert_eq!(interpreter.numeric_variables.get("DONE"), Some(&1.0));
    assert_eq!(interpreter.take_output(), "[40][100][110][50]\n");
    assert!(interpreter.last_error.is_none());
}

#[test]
fn cached_mat_copy_replays_dynamic_selectors_shapes_base_and_source_existence() {
    let source = "B=A(L:H)";
    let compiled = CompiledMatAssignment::compile(source).unwrap();
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    // Before DIM, the original path must still determine the error and whether
    // a target gets declared; the cache cannot bind A as an existing array.
    cached.current_line = Some(77);
    raw.current_line = Some(77);
    replay_pair(&mut cached, &mut raw, &compiled, source);
    assert_eq!(cached.array_exists("B"), raw.array_exists("B"));
    for interpreter in [&mut cached, &mut raw] {
        setup(interpreter, "DIM A(4):FOR I=0 TO 4:A(I)=10+I:NEXT I");
    }
    for initialization in [
        "L=0:H=2",
        "L=4:H=0",
        "MAT BASE 1:L=1:H=0",
        "MAT BASE 0:L=0:H=4",
    ] {
        setup(&mut cached, initialization);
        setup(&mut raw, initialization);
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "B");
    }
}

#[test]
fn cached_mat_copy_preserves_selector_string_case_and_rhs_before_lhs() {
    let mut interpreter = Interpreter::new();
    let source = r#"MAT B(FNI("lEfT"):3)=A(FNI("rIgHt"):3)"#;
    assert!(matches!(
        interpreter.compile_cached_command(source),
        CachedCommand::MatAssignment(_)
    ));
    interpreter
        .program
        .load_text(&format!(
            "10 DIM A(3),B(3):FOR I=0 TO 3:A(I)=10+I:NEXT I\n\
         20 ORDER$=\"\"\n\
         30 GOSUB 200:GOSUB 200:END\n\
         100 DEF FNI(T$)\n\
         110 ORDER$=ORDER$+T$\n\
         120 FNI=1\n\
         130 FNEND\n\
         200 {source}\n\
         210 RETURN"
        ))
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(
        interpreter
            .string_variables
            .get("ORDER$")
            .map(String::as_str),
        Some("rIgHtlEfTrIgHtlEfT")
    );
    let result = interpreter.array_ref("B").unwrap();
    assert_eq!(result.get_number_direct_1d(0).unwrap(), 0.0);
    for index in 1..=3 {
        assert_eq!(
            result.get_number_direct_1d(index).unwrap(),
            (10 + index) as f64
        );
    }
}

#[test]
fn cached_mat_expression_tree_matches_raw_new_precedence_functions_and_string_composition() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        setup(
            interpreter,
            "A=100:DIM A(1),B(1),C(1),A$(1),B$(1):MAT A=4:MAT B=2:MAT A$=\"a\":MAT B$=\"b\":DYE=5",
        );
    }
    for (source, expected) in [
        ("C=A+ABS(1)", 5.0),
        ("C=ABS(1)+A", 5.0),
        ("C=A+ABS(A)", 104.0),
        ("C=(2*A/4)", 2.0),
        ("C=A*2+3", 11.0),
        ("C=-A^2", -16.0),
        ("C=A^2^3", 4096.0),
        ("C=A*(DYE-2)", 12.0),
        ("C=A*1E+3", 4000.0),
        ("C=A+ABS(SUM(A))", 12.0),
        ("C=A+IIF(1,2,1/0)", 6.0),
        ("C=A*(DYE MOD 3+1)", 12.0),
        ("C=A*(DYE>2)", -4.0),
    ] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "C");
        assert_eq!(
            cached
                .array_ref("C")
                .unwrap()
                .get_number_direct_1d(0)
                .unwrap(),
            expected,
            "{source}"
        );
    }
    let source = "C$=\"<\"+A$+B$+\">\"";
    let compiled = CompiledMatAssignment::compile(source).unwrap();
    replay_pair(&mut cached, &mut raw, &compiled, source);
    assert_same_array(&cached, &raw, "C$");
    assert_eq!(
        cached
            .array_ref("C$")
            .unwrap()
            .get(&[0])
            .unwrap()
            .into_string()
            .unwrap(),
        "<ab>"
    );
    for source in ["C=A MOD 3", "C=NOT A"] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        let error = cached
            .execute_compiled_mat_assignment(&compiled)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ForbiddenExpression, "{source}");
        let raw_error = raw.execute_mat_assignment(source).unwrap_err();
        assert_eq!(error.code, raw_error.code);
        assert_same_array(&cached, &raw, "C");
    }
}

#[test]
fn cached_mat_indexed_elements_use_normal_scalar_evaluation_and_fill_existing_target() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        setup(interpreter, "DIM A(1,1),V(1),A$(1,1),V$(1):MAT A=4:MAT V=2:MAT A$=\"MiXeD\":MAT V$=\"old\":A(0,0)=7");
    }
    for (source, expected) in [
        ("V=A(0,0)", 7.0),
        ("V=A(0,0)+V", 14.0),
        ("V=ABS(A(0,0))*V", 98.0),
        ("V=V(0)+V(1)", 196.0),
    ] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "V");
        assert_eq!(cached.array_ref("V").unwrap().dims, vec![1]);
        for index in 0..=1 {
            assert_eq!(
                cached
                    .array_ref("V")
                    .unwrap()
                    .get_number_direct_1d(index)
                    .unwrap(),
                expected,
                "{source}"
            );
        }
    }
    for source in ["V$=A$(1,0)", "V$=\"<\"+A$(1,0)+\">\""] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "V$");
    }
    assert_eq!(
        cached
            .array_ref("V$")
            .unwrap()
            .get(&[0])
            .unwrap()
            .into_string()
            .unwrap(),
        "<MiXeD>"
    );
    for source in ["V=A+A(0:1,0)", "V=A+A(,0)", "V=A+A(0,)"] {
        assert_eq!(
            cached.execute_mat_assignment(source).unwrap_err().code,
            ErrorCode::ForbiddenExpression,
            "{source}"
        );
        assert_eq!(
            raw.execute_mat_assignment(source).unwrap_err().code,
            ErrorCode::ForbiddenExpression,
            "{source}"
        );
        assert_same_array(&cached, &raw, "V");
    }
}

#[test]
fn cached_mat_calls_keep_effects_once_and_operand_snapshots_before_late_failure() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        interpreter
            .program
            .load_text(
                "10 DIM A(1),B(2),C(1):MAT A=4:MAT B=2:MAT C=77\n20 CALLS=0:END\n\
             100 DEF FNEFFECT()\n110 CALLS=CALLS+1:MAT A=9\n120 FNEFFECT=2\n130 FNEND",
            )
            .unwrap();
        interpreter.run_loaded().unwrap();
        interpreter.end_requested = false;
    }
    for (source, expected) in [("C=A+FNEFFECT()", 6.0), ("C=FNEFFECT()+A", 11.0)] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "C");
        assert_eq!(
            cached
                .array_ref("C")
                .unwrap()
                .get_number_direct_1d(0)
                .unwrap(),
            expected
        );
    }
    let compiled = CompiledMatAssignment::compile("C=FNEFFECT()+B").unwrap();
    // This is valid and returns B's shape; a following mismatch must preserve it.
    replay_pair(&mut cached, &mut raw, &compiled, "C=FNEFFECT()+B");
    let compiled = CompiledMatAssignment::compile("C=FNEFFECT()*A+B").unwrap();
    replay_pair(&mut cached, &mut raw, &compiled, "C=FNEFFECT()*A+B");
    assert_eq!(cached.numeric_variables.get("CALLS"), Some(&4.0));
    assert_eq!(raw.numeric_variables.get("CALLS"), Some(&4.0));
    assert_same_array(&cached, &raw, "C");
    assert_eq!(cached.array_ref("C").unwrap().dims, vec![2]);
}

#[test]
fn cached_mat_empty_active_blocks_still_evaluate_scalar_calls_and_keep_zero_division_contract() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        interpreter
            .program
            .load_text(
                "10 MAT BASE 1:DIM A(0):CALLS=0:END\n\
             100 DEF FNCOUNT()\n110 CALLS=CALLS+1\n120 FNCOUNT=2\n130 FNEND",
            )
            .unwrap();
        interpreter.run_loaded().unwrap();
        interpreter.end_requested = false;
    }
    for source in ["C=A/0", "C=A*FNCOUNT()", "C=A/(1/0)"] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "C");
    }
    assert_eq!(cached.numeric_variables.get("CALLS"), Some(&1.0));
    assert_eq!(raw.numeric_variables.get("CALLS"), Some(&1.0));
}

#[test]
fn cached_and_raw_mat_scalar_operators_preserve_first_error_and_function_effects() {
    let make_interpreter = || {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(
                "10 DIM A(1),C(1):MAT A=4:MAT C=77:CALLS=0:END\n\
             100 DEF FNTOUCH()\n110 CALLS=CALLS+1\n120 FNTOUCH=2\n130 FNEND",
            )
            .unwrap();
        interpreter.run_loaded().unwrap();
        interpreter.end_requested = false;
        interpreter.current_line = Some(321);
        interpreter
    };
    for operator in ["\\", "MOD", "AND", "OR", "XOR"] {
        for (left, right, expected, calls, compare_scalar) in [
            ("\"x\"", "FNTOUCH()", ErrorCode::TypeMismatch, 0.0, true),
            ("\"x\"", "(1/0)", ErrorCode::TypeMismatch, 0.0, true),
            ("FNTOUCH()", "\"x\"", ErrorCode::TypeMismatch, 1.0, true),
            ("A", "FNTOUCH()", ErrorCode::ForbiddenExpression, 0.0, false),
            ("FNTOUCH()", "A", ErrorCode::ForbiddenExpression, 1.0, false),
        ] {
            let expression = format!("{left} {operator} {right}");
            let source = format!("C={expression}");
            let compiled = CompiledMatAssignment::compile(&source).unwrap();
            for cached in [false, true] {
                let mut interpreter = make_interpreter();
                let result = if cached {
                    interpreter.execute_compiled_mat_assignment(&compiled)
                } else {
                    interpreter.execute_mat_assignment(&source)
                };
                let error = result.unwrap_err();
                assert_eq!(error.code, expected, "{source}, cached={cached}");
                assert_eq!(error.line, Some(321), "{source}, cached={cached}");
                assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&calls));
                assert_eq!(
                    interpreter
                        .array_ref("C")
                        .unwrap()
                        .get_number_direct_1d(0)
                        .unwrap(),
                    77.0,
                    "{source}, cached={cached}"
                );
                if compare_scalar {
                    let mut scalar = make_interpreter();
                    let ordinary_error = eval_expression(&mut scalar, &expression).unwrap_err();
                    assert_eq!(error.code, ordinary_error.code, "{source}");
                    assert_eq!(
                        interpreter.numeric_variables.get("CALLS"),
                        scalar.numeric_variables.get("CALLS"),
                        "{source}"
                    );
                }
            }
        }
    }
    // Comparisons evaluate both scalar values, and bitwise operators still
    // evaluate their RHS even when the left value determines the result.
    for expression in ["\"x\"=FNTOUCH()", "0 AND FNTOUCH()", "-1 OR FNTOUCH()"] {
        let source = format!("C={expression}");
        let compiled = CompiledMatAssignment::compile(&source).unwrap();
        let mut scalar = make_interpreter();
        let ordinary = eval_expression(&mut scalar, expression);
        for cached in [false, true] {
            let mut interpreter = make_interpreter();
            let result = if cached {
                interpreter.execute_compiled_mat_assignment(&compiled)
            } else {
                interpreter.execute_mat_assignment(&source)
            };
            match (&ordinary, result) {
                (Ok(value), Ok(())) => assert_eq!(
                    interpreter.array_ref("C").unwrap().get(&[0]).unwrap(),
                    *value,
                    "{source}, cached={cached}"
                ),
                (Err(ordinary), Err(error)) => {
                    assert_eq!(error.code, ordinary.code, "{source}, cached={cached}");
                    assert_eq!(error.line, Some(321));
                    assert_eq!(
                        interpreter
                            .array_ref("C")
                            .unwrap()
                            .get_number_direct_1d(0)
                            .unwrap(),
                        77.0
                    );
                }
                (ordinary, result) => panic!("{source}: ordinary={ordinary:?}, MAT={result:?}"),
            }
            assert_eq!(
                interpreter.numeric_variables.get("CALLS"),
                scalar.numeric_variables.get("CALLS"),
                "{source}, cached={cached}"
            );
        }
    }
}

#[test]
fn cached_and_raw_mat_unary_plus_keep_scalar_text_but_reject_text_arrays() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        setup(
            interpreter,
            "T$=\"MiXeD\":DIM A$(1),C$(1):MAT A$=\"array\":MAT C$=\"old\"",
        );
        interpreter.current_line = Some(345);
    }
    for (source, expected) in [
        ("C$=+\"x\"", "x"),
        ("C$=IIF(1,+\"x\",\"unused\")", "x"),
        ("C$=+T$", "MiXeD"),
    ] {
        let compiled = CompiledMatAssignment::compile(source).unwrap();
        replay_pair(&mut cached, &mut raw, &compiled, source);
        assert_same_array(&cached, &raw, "C$");
        for index in 0..=1 {
            assert_eq!(
                cached.array_ref("C$").unwrap().get(&[index]).unwrap(),
                Value::string(expected)
            );
        }
    }
    let source = "C$=+A$";
    let compiled = CompiledMatAssignment::compile(source).unwrap();
    let error = cached
        .execute_compiled_mat_assignment(&compiled)
        .unwrap_err();
    let raw_error = raw.execute_mat_assignment(source).unwrap_err();
    assert_eq!(error.code, ErrorCode::ForbiddenExpression);
    assert_eq!(error.line, Some(345));
    assert_eq!(error.code, raw_error.code);
    assert_eq!(error.line, raw_error.line);
    assert_same_array(&cached, &raw, "C$");
    assert_eq!(
        cached.array_ref("C$").unwrap().get(&[0]).unwrap(),
        Value::string("MiXeD")
    );
}

#[test]
fn cached_and_raw_mat_scalar_arithmetic_validate_left_before_rhs_failure() {
    for operator in ["-", "*", "/", "^"] {
        let source = format!("C=\"x\"{operator}(1/0)");
        let compiled = CompiledMatAssignment::compile(&source).unwrap();
        for cached in [false, true] {
            let mut interpreter = Interpreter::new();
            setup(&mut interpreter, "DIM C(1):MAT C=77");
            interpreter.current_line = Some(367);
            let error = if cached {
                interpreter.execute_compiled_mat_assignment(&compiled)
            } else {
                interpreter.execute_mat_assignment(&source)
            }
            .unwrap_err();
            assert_eq!(
                error.code,
                ErrorCode::TypeMismatch,
                "{source}, cached={cached}"
            );
            assert_eq!(error.line, Some(367));
            assert_eq!(
                interpreter
                    .array_ref("C")
                    .unwrap()
                    .get_number_direct_1d(0)
                    .unwrap(),
                77.0
            );
        }
    }
}

#[test]
fn cached_and_raw_mat_reject_rank_zero_even_for_function_returns_and_keep_higher_rank_return() {
    for rank in [0, 3] {
        for active_function in [false, true] {
            for cached in [false, true] {
                let mut interpreter = Interpreter::new();
                let target = if active_function { "FNRESULT" } else { "C" };
                setup(
                    &mut interpreter,
                    &format!("DIM {target}(1):MAT {target}=77"),
                );
                interpreter.current_line = Some(456);
                let mut source_array = ArrayValue::new("A", vec![1; rank]);
                source_array.fill(Value::number(4.0)).unwrap();
                interpreter.arrays.insert("A".to_string(), source_array);
                if active_function {
                    interpreter.active_functions.push(ActiveFunctionFrame {
                        name: Rc::from(target),
                        return_value: Some(Value::number(9.0)),
                    });
                }
                let source = format!("{target}=A");
                let compiled = CompiledMatAssignment::compile(&source).unwrap();
                let result = if cached {
                    interpreter.execute_compiled_mat_assignment(&compiled)
                } else {
                    interpreter.execute_mat_assignment(&source)
                };
                if rank == 3 && active_function {
                    result.unwrap();
                    assert_eq!(interpreter.array_ref(target).unwrap().dims, vec![1; 3]);
                    assert_eq!(
                        interpreter.active_functions.last().unwrap().return_value,
                        Some(Value::ArrayRef(target.to_string()))
                    );
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, ErrorCode::InvalidDimensions);
                    assert_eq!(error.line, Some(456));
                    let target_array = interpreter.array_ref(target).unwrap();
                    assert_eq!(target_array.dims, vec![1]);
                    assert_eq!(target_array.get_number_direct_1d(0).unwrap(), 77.0);
                    if active_function {
                        assert_eq!(
                            interpreter.active_functions.last().unwrap().return_value,
                            Some(Value::number(9.0))
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cached_and_raw_scalar_mat_domain_and_overflow_errors_keep_destination_intact() {
    for (expression, expected) in [
        ("(-1)^.5", ErrorCode::InvalidValue),
        ("ABS((-1)^.5)", ErrorCode::TypeMismatch),
        ("1E309-1E309", ErrorCode::InvalidValue),
        ("1E308+1E308", ErrorCode::Overflow),
        ("-1E308-1E308", ErrorCode::Overflow),
        ("1E308*2", ErrorCode::Overflow),
        ("1E308/1E-308", ErrorCode::Overflow),
        ("1E308^2", ErrorCode::Overflow),
        ("0^-1", ErrorCode::Overflow),
        ("EXP(1000)", ErrorCode::Overflow),
    ] {
        let source = format!("C={expression}");
        let compiled = CompiledMatAssignment::compile(&source).unwrap();
        for cached in [false, true] {
            let mut interpreter = Interpreter::new();
            setup(&mut interpreter, "DIM C(2):C(0)=-0:C(1)=77:C(2)=123.5");
            interpreter.current_line = Some(567);
            let ordinary_error = eval_expression(&mut interpreter, expression).unwrap_err();
            assert_eq!(ordinary_error.code, expected, "{expression}");
            let error = if cached {
                interpreter.execute_compiled_mat_assignment(&compiled)
            } else {
                interpreter.execute_mat_assignment(&source)
            }
            .unwrap_err();
            assert_eq!(error.code, ordinary_error.code, "{source}, cached={cached}");
            assert_eq!(error.line, Some(567), "{source}, cached={cached}");
            let target = interpreter.array_ref("C").unwrap();
            assert_eq!(target.dims, vec![2]);
            for (index, expected) in [(0, -0.0_f64), (1, 77.0), (2, 123.5)] {
                assert_eq!(
                    target.get_number_direct_1d(index).unwrap().to_bits(),
                    expected.to_bits(),
                    "{source}, cached={cached}, C({index})"
                );
            }
        }
    }
}

#[test]
fn cached_and_raw_mat_unary_minus_type_error_keeps_line_and_destination() {
    let source = "C=-\"x\"";
    let compiled = CompiledMatAssignment::compile(source).unwrap();
    for cached in [false, true] {
        let mut interpreter = Interpreter::new();
        setup(&mut interpreter, "DIM C(1):MAT C=77");
        interpreter.current_line = Some(567);
        let error = if cached {
            interpreter.execute_compiled_mat_assignment(&compiled)
        } else {
            interpreter.execute_mat_assignment(source)
        }
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::TypeMismatch);
        assert_eq!(error.line, Some(567));
        let target = interpreter.array_ref("C").unwrap();
        assert_eq!(target.dims, vec![1]);
        assert_eq!(target.get_number_direct_1d(0).unwrap(), 77.0);
        assert_eq!(target.get_number_direct_1d(1).unwrap(), 77.0);
    }
}

#[test]
fn scalar_mat_numeric_errors_keep_erl_and_resume_next_in_cached_and_raw_programs() {
    for (expression, expected) in [
        ("(-1)^.5", ErrorCode::InvalidValue),
        ("ABS((-1)^.5)", ErrorCode::TypeMismatch),
        ("1E308*2", ErrorCode::Overflow),
        ("EXP(1000)", ErrorCode::Overflow),
    ] {
        for cached in [false, true] {
            let mut interpreter = Interpreter::new();
            interpreter
                .program
                .load_text(&format!(
                    "10 DIM C(1):MAT C=77\n20 ON ERROR GOTO 100\n\
                     30 MAT C={expression}\n40 DONE=1:END\n\
                     100 CAUGHT=ERR:FAILEDLINE=ERL\n110 RESUME NEXT"
                ))
                .unwrap();
            interpreter.prepare_run().unwrap();
            interpreter.rebuild_data();
            interpreter.rebuild_command_cache();
            interpreter.ensure_routine_catalog().unwrap();
            if !cached {
                let raw = vec![Rc::new(CachedCommand::Raw(Rc::from(format!(
                    "MAT C={expression}"
                ))))];
                let line_index = interpreter.line_index(30).unwrap();
                let mut lines = interpreter.compiled_line_cache.as_ref().to_vec();
                lines[line_index] = Rc::from(raw.clone().into_boxed_slice());
                interpreter.compiled_line_cache = Rc::from(lines.into_boxed_slice());
                interpreter.compiled_command_cache.insert(30, raw);
            }
            interpreter
                .run_from(Cursor {
                    line_idx: 0,
                    cmd_idx: 0,
                })
                .unwrap();
            assert_eq!(
                interpreter.numeric_variables.get("CAUGHT"),
                Some(&(expected.number() as f64)),
                "{expression}, cached={cached}"
            );
            assert_eq!(interpreter.numeric_variables.get("FAILEDLINE"), Some(&30.0));
            assert_eq!(interpreter.numeric_variables.get("DONE"), Some(&1.0));
            assert_eq!(
                interpreter
                    .array_ref("C")
                    .unwrap()
                    .get_number_direct_1d(0)
                    .unwrap(),
                77.0
            );
            assert!(interpreter.last_error.is_none());
        }
    }
}

#[test]
fn cached_and_raw_text_mat_reject_invalid_operators_with_err37() {
    let mut cached = Interpreter::new();
    let mut raw = Interpreter::new();
    for interpreter in [&mut cached, &mut raw] {
        setup(
            interpreter,
            "DIM A$(1),B$(1),N(1),C$(1):MAT A$=\"a\":MAT B$=\"b\":MAT N=2:MAT C$=\"old\"",
        );
        interpreter.current_line = Some(678);
    }
    for expression in [
        "+A$",
        "-A$",
        "NOT A$",
        "A$-B$",
        "A$*B$",
        "A$/B$",
        "A$^B$",
        "A$\\B$",
        "A$ MOD B$",
        "A$ AND B$",
        "A$ OR B$",
        "A$ XOR B$",
        "A$=B$",
        "A$<>B$",
        "A$<B$",
        "A$>B$",
        "A$<=B$",
        "A$>=B$",
        "A$*N",
        "N*A$",
        "A$*2",
        "2*A$",
        "A$/2",
        "2/A$",
    ] {
        let source = format!("C$={expression}");
        let compiled = CompiledMatAssignment::compile(&source).unwrap();
        let error = cached
            .execute_compiled_mat_assignment(&compiled)
            .unwrap_err();
        let raw_error = raw.execute_mat_assignment(&source).unwrap_err();
        assert_eq!(error.code, ErrorCode::ForbiddenExpression, "{source}");
        assert_eq!(error.code.number(), 37, "{source}");
        assert_eq!(error.line, Some(678), "{source}");
        assert_eq!(error.code, raw_error.code, "{source}");
        assert_eq!(error.line, raw_error.line, "{source}");
        for name in ["A$", "B$", "N", "C$"] {
            assert_same_array(&cached, &raw, name);
        }
        assert_eq!(
            cached.array_ref("C$").unwrap().get(&[0]).unwrap(),
            Value::string("old")
        );
        assert_eq!(
            cached.array_ref("C$").unwrap().get(&[1]).unwrap(),
            Value::string("old")
        );
    }
}

#[test]
fn cached_and_raw_text_rank_three_cannot_be_added_to_scalar_even_for_fn_returns() {
    for active_function in [false, true] {
        for scalar_left in [false, true] {
            for cached in [false, true] {
                let mut interpreter = Interpreter::new();
                let target = if active_function { "FNRESULT$" } else { "C$" };
                setup(
                    &mut interpreter,
                    &format!("DIM {target}(1):MAT {target}=\"old\""),
                );
                let mut array = ArrayValue::new("T$", vec![1, 1, 1]);
                array.fill(Value::string("text")).unwrap();
                interpreter.arrays.insert("T$".to_string(), array);
                interpreter.current_line = Some(789);
                if active_function {
                    interpreter.active_functions.push(ActiveFunctionFrame {
                        name: Rc::from(target),
                        return_value: Some(Value::string("previous")),
                    });
                }
                let expression = if scalar_left {
                    "\"prefix\"+T$"
                } else {
                    "T$+\"suffix\""
                };
                let source = format!("{target}={expression}");
                let compiled = CompiledMatAssignment::compile(&source).unwrap();
                let error = if cached {
                    interpreter.execute_compiled_mat_assignment(&compiled)
                } else {
                    interpreter.execute_mat_assignment(&source)
                }
                .unwrap_err();
                assert_eq!(error.code, ErrorCode::InvalidDimensions, "{source}");
                assert_eq!(error.line, Some(789));
                assert_eq!(interpreter.array_ref(target).unwrap().dims, vec![1]);
                assert_eq!(
                    interpreter.array_ref(target).unwrap().get(&[0]).unwrap(),
                    Value::string("old")
                );
                assert_eq!(
                    interpreter
                        .array_ref("T$")
                        .unwrap()
                        .get(&[0, 0, 0])
                        .unwrap(),
                    Value::string("text")
                );
                if active_function {
                    assert_eq!(
                        interpreter.active_functions.last().unwrap().return_value,
                        Some(Value::string("previous"))
                    );
                }
            }
        }
    }
}

#[test]
fn cached_and_raw_transposes_preserve_ieee_bits_self_assignment_and_base_padding() {
    let active_bits = [
        1.25_f64.to_bits(),
        (-0.0_f64).to_bits(),
        0x7ff8_0000_0000_0042_u64,
        f64::INFINITY.to_bits(),
        1,
        0x7ff0_0000_0000_0027,
        0xfff8_0000_0000_0123,
        (-2.5_f64).to_bits(),
        f64::NEG_INFINITY.to_bits(),
    ];
    for base in [0, 1] {
        for (rows, cols) in [
            (0, 0),
            (0, 2),
            (2, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (2, 3),
            (3, 2),
        ] {
            if base == 0 && (rows == 0 || cols == 0) {
                continue;
            }
            let lower = base as usize;
            let upper = |count: usize| if base == 1 { count } else { count - 1 };
            let mut source_array = ArrayValue::new("A", vec![upper(rows), upper(cols)]);
            let stride = source_array.dims[1] + 1;
            let ArrayData::Number(source_values) = &mut source_array.data else {
                unreachable!();
            };
            // Inactive cells deliberately contain nonzero and nonfinite bits;
            // structural TRN must clear their output while leaving A intact.
            for (index, value) in source_values.iter_mut().enumerate() {
                *value = f64::from_bits(0x7ff8_0000_0000_1000 + index as u64);
            }
            for row in 0..rows {
                for col in 0..cols {
                    source_values[(lower + row) * stride + lower + col] =
                        f64::from_bits(active_bits[(row * cols + col) % active_bits.len()]);
                }
            }
            let source_bits = source_values
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>();
            for target in ["C", "A"] {
                let statement = format!("{target}=TRN(A)");
                let compiled = CompiledMatAssignment::compile(&statement).unwrap();
                for cached in [false, true] {
                    let mut interpreter = Interpreter::new();
                    interpreter.mat_base = base;
                    interpreter
                        .arrays
                        .insert("A".to_string(), source_array.clone());
                    if cached {
                        interpreter
                            .execute_compiled_mat_assignment(&compiled)
                            .unwrap();
                    } else {
                        interpreter.execute_mat_assignment(&statement).unwrap();
                    }
                    let result = interpreter.array_ref(target).unwrap();
                    assert_eq!(result.dims, vec![upper(cols), upper(rows)]);
                    for row in 0..=result.dims[0] {
                        for col in 0..=result.dims[1] {
                            let expected = if row < lower || col < lower {
                                0.0_f64.to_bits()
                            } else {
                                source_bits[col * stride + row]
                            };
                            assert_eq!(
                                result.get_number_direct_2d(row as i32, col as i32).unwrap().to_bits(),
                                expected,
                                "BASE {base}, {rows}x{cols}, {statement}, cached={cached}, ({row},{col})"
                            );
                        }
                    }
                    if target != "A" {
                        let ArrayData::Number(values) = &interpreter.array_ref("A").unwrap().data
                        else {
                            unreachable!();
                        };
                        assert_eq!(
                            values
                                .iter()
                                .map(|value| value.to_bits())
                                .collect::<Vec<_>>(),
                            source_bits,
                            "BASE {base}, {rows}x{cols}, cached={cached}: source mutated"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn square_transpose_keeps_argument_snapshot_and_following_function_effects() {
    for base in [0, 1] {
        for cached in [false, true] {
            let mut interpreter = Interpreter::new();
            let next = base + 1;
            interpreter.program.load_text(&format!(
                "10 MAT BASE {base}:DIM A({next},{next})\n\
                 20 A({base},{base})=1:A({base},{next})=2:A({next},{base})=3:A({next},{next})=4:CALLS=0:END\n\
                 100 DEF FNRESET()\n110 SEEN=A({base},{next}):CALLS=CALLS+1\n\
                 120 MAT A=9\n130 FNRESET=0\n140 FNEND"
            )).unwrap();
            interpreter.run_loaded().unwrap();
            interpreter.end_requested = false;
            let statement = "C=TRN(A)+FNRESET()+A";
            let compiled = CompiledMatAssignment::compile(statement).unwrap();
            if cached {
                interpreter
                    .execute_compiled_mat_assignment(&compiled)
                    .unwrap();
            } else {
                interpreter.execute_mat_assignment(statement).unwrap();
            }
            assert_eq!(interpreter.numeric_variables.get("CALLS"), Some(&1.0));
            assert_eq!(interpreter.numeric_variables.get("SEEN"), Some(&2.0));
            for (row, col, expected) in [(0, 0, 10.0), (0, 1, 12.0), (1, 0, 11.0), (1, 1, 13.0)] {
                assert_eq!(
                    interpreter
                        .array_ref("C")
                        .unwrap()
                        .get_number_direct_2d(base + row, base + col)
                        .unwrap(),
                    expected,
                    "BASE {base}, cached={cached}, ({row},{col})"
                );
                assert_eq!(
                    interpreter
                        .array_ref("A")
                        .unwrap()
                        .get_number_direct_2d(base + row, base + col)
                        .unwrap(),
                    9.0
                );
            }
        }
    }
}
