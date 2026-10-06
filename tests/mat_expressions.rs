use avl_basic::expr::eval_expression;
use avl_basic::{ErrorCode, Interpreter};

fn command(interpreter: &mut Interpreter, source: &str) {
    interpreter.process_immediate(source).unwrap();
}

fn number(interpreter: &mut Interpreter, source: &str) -> f64 {
    eval_expression(interpreter, source)
        .unwrap()
        .as_number()
        .unwrap()
}

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.run_loaded().unwrap();
    interpreter
}

#[test]
fn matrix_times_vector_preserves_vector_rank_and_index_aliases() {
    for base in [0, 1] {
        let bound = base + 1;
        let mut interpreter = run(&format!(
            "10 MAT BASE {base}\n\
20 DIM A({bound},{bound}),V({bound})\n\
30 DATA 2,3,5,7,11,13\n\
40 MAT READ A,V\n\
50 MAT C=A*V\n\
60 END"
        ));
        assert_eq!(number(&mut interpreter, "UBOUND(C)"), bound as f64);
        assert!(eval_expression(&mut interpreter, "UBOUND(C,2)").is_err());
        for (index, expected) in [(base, 61.0), (bound, 146.0)] {
            for expression in [
                format!("C({index})"),
                format!("C({index},{base})"),
                format!("C({base},{index})"),
            ] {
                assert_eq!(number(&mut interpreter, &expression), expected);
            }
        }
    }
}

#[test]
fn compound_mat_uses_basic_precedence_and_resolves_scalar_functions_symmetrically() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(1),B(1):MAT A=4:MAT B=2:DYE=5");
    for (expression, expected) in [
        ("A*2+3", 11.0),
        ("2+3*A", 14.0),
        ("A/2+1", 3.0),
        ("(A+B)*.5", 3.0),
        ("A+ABS(-1)", 5.0),
        ("ABS(-1)+A", 5.0),
        ("(2*A/4)", 2.0),
        ("A*(DYE-2)", 12.0),
        ("A*1E+2", 400.0),
        ("A*1E-2", 0.04),
        ("-A^2", -16.0),
        ("(-A)^2", 16.0),
        ("A^-2", 0.0625),
        ("A^2^3", 4096.0),
        ("A*(DYE MOD 3+1)", 12.0),
        ("A*(DYE>2)", -4.0),
        ("A+SUM(A)", 12.0),
        ("(A+ABSUM(A))/ABS(-2)", 6.0),
        ("A+IIF(1,2,1/0)", 6.0),
        ("A+A(0)", 8.0),
    ] {
        command(&mut interpreter, &format!("MAT C={expression}"));
        for index in 0..=1 {
            assert_eq!(
                number(&mut interpreter, &format!("C({index})")),
                expected,
                "{expression}"
            );
        }
    }
}

#[test]
fn a_bare_mat_name_prefers_array_but_scalar_function_arguments_keep_scalar_context() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "A=100:DIM A(1):MAT A=4");
    command(&mut interpreter, "MAT C=A+ABS(A)");
    assert_eq!(number(&mut interpreter, "C(0)"), 104.0);
    assert_eq!(number(&mut interpreter, "A"), 100.0);
    command(&mut interpreter, "MAT C=A");
    assert_eq!(number(&mut interpreter, "C(0)"), 4.0);
}

#[test]
fn functions_run_once_per_occurrence_in_left_to_right_order() {
    let mut interpreter = run("10 DIM A(100):MAT A=4\n20 ORDER$=\"\":CALLS=0\n\
         30 MAT C=A*FNTAG(\"L\")+FNTAG(\"R\")*A\n40 END\n\
         100 DEF FNTAG(T$)\n110 ORDER$=ORDER$+T$:CALLS=CALLS+1\n\
         120 FNTAG=2\n130 FNEND");
    assert_eq!(
        eval_expression(&mut interpreter, "ORDER$")
            .unwrap()
            .into_string()
            .unwrap(),
        "LR"
    );
    assert_eq!(number(&mut interpreter, "CALLS"), 2.0);
    for index in 0..=100 {
        assert_eq!(number(&mut interpreter, &format!("C({index})")), 16.0);
    }
}

#[test]
fn array_operands_are_snapshotted_when_reached_and_function_effects_are_preserved() {
    let mut interpreter = run("10 DIM A(1),B(1):MAT A=4:MAT B=2\n\
         20 MAT C=A+FNCHANGE()\n30 FIRST=C(0):CHANGED=A(0)\n\
         40 MAT A=4\n50 MAT C=FNCHANGE()+A\n60 SECOND=C(0)\n70 END\n\
         100 DEF FNCHANGE()\n110 MAT A=9\n120 MAT FNCHANGE=B\n130 FNEND");
    assert_eq!(number(&mut interpreter, "FIRST"), 6.0);
    assert_eq!(number(&mut interpreter, "CHANGED"), 9.0);
    assert_eq!(number(&mut interpreter, "SECOND"), 11.0);
}

#[test]
fn composed_matrix_products_transposes_and_inverses_use_real_matrix_arithmetic() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM A({},{})", base + 1, base + 1),
        );
        command(
            &mut interpreter,
            &format!(
                "A({base},{base})=1:A({base},{})=2:A({},{base})=3:A({},{})=4",
                base + 1,
                base + 1,
                base + 1,
                base + 1
            ),
        );
        command(&mut interpreter, "MAT C=A+TRN(A)*2");
        for (row, col, expected) in [(0, 0, 3.0), (0, 1, 8.0), (1, 0, 7.0), (1, 1, 12.0)] {
            assert_eq!(
                number(
                    &mut interpreter,
                    &format!("C({},{})", base + row, base + col)
                ),
                expected
            );
        }
        command(&mut interpreter, "MAT A=A+A*A");
        for (row, col, expected) in [(0, 0, 8.0), (0, 1, 12.0), (1, 0, 18.0), (1, 1, 26.0)] {
            assert_eq!(
                number(
                    &mut interpreter,
                    &format!("A({},{})", base + row, base + col)
                ),
                expected
            );
        }
        command(
            &mut interpreter,
            &format!(
                "DIM D({},{}):MAT D=IDN:MAT C=INV(D*2)*2",
                base + 1,
                base + 1
            ),
        );
        for row in 0..=1 {
            for col in 0..=1 {
                assert_eq!(
                    number(
                        &mut interpreter,
                        &format!("C({},{})", base + row, base + col)
                    ),
                    if row == col { 1.0 } else { 0.0 }
                );
            }
        }
    }
}

#[test]
fn late_failures_preserve_destination_and_source_without_partial_assignment() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(1),B(2),C(1):MAT A=4:MAT B=3:MAT C=77",
    );
    for (expression, error) in [
        ("A*2+B", ErrorCode::InvalidDimensions),
        ("A+A/0", ErrorCode::DivisionByZero),
        ("A+A(0:1)", ErrorCode::ForbiddenExpression),
    ] {
        assert_eq!(
            interpreter
                .process_immediate(&format!("MAT C={expression}"))
                .unwrap_err()
                .code,
            error,
            "{expression}"
        );
        assert_eq!(number(&mut interpreter, "C(0)"), 77.0);
        assert_eq!(number(&mut interpreter, "C(1)"), 77.0);
        assert_eq!(number(&mut interpreter, "A(0)"), 4.0);
    }
}

#[test]
fn compound_string_addition_preserves_operand_order_and_rejects_other_operators() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A$(1),B$(1),C$(1):MAT A$=\"a\":MAT B$=\"b\":MAT C$=\"old\"",
    );
    command(&mut interpreter, "MAT C$=\"<\"+A$+B$+\">\"");
    assert_eq!(
        eval_expression(&mut interpreter, "C$(0)")
            .unwrap()
            .into_string()
            .unwrap(),
        "<ab>"
    );
    let error = interpreter.process_immediate("MAT C$=A$+B$*2").unwrap_err();
    assert_eq!(error.code, ErrorCode::ForbiddenExpression);
    assert_eq!(
        eval_expression(&mut interpreter, "C$(0)")
            .unwrap()
            .into_string()
            .unwrap(),
        "<ab>"
    );
}

#[test]
fn scalar_unary_plus_preserves_text_in_mat_and_matrix_function_returns() {
    let mut interpreter = run(
        "10 T$=+\"x\"\n20 MAT A$=+\"x\"\n30 MAT B$=IIF(1,+\"x\",\"unused\")\n\
         40 MAT C$=FNPLUS$(\"x\")\n50 END\n\
         100 DEF FNPLUS$(T$)\n110 MAT FNPLUS$=+T$\n120 FNEND",
    );
    assert_eq!(
        eval_expression(&mut interpreter, "T$").unwrap(),
        avl_basic::Value::string("x")
    );
    for name in ["A$", "B$", "C$"] {
        for index in 0..=10 {
            assert_eq!(
                eval_expression(&mut interpreter, &format!("{name}({index})")).unwrap(),
                avl_basic::Value::string("x")
            );
        }
    }
    let error = interpreter.process_immediate("MAT A$=+B$").unwrap_err();
    assert_eq!(error.code, ErrorCode::ForbiddenExpression);
    assert_eq!(
        eval_expression(&mut interpreter, "A$(0)").unwrap(),
        avl_basic::Value::string("x")
    );
}

#[test]
fn invalid_left_scalar_arithmetic_stops_before_rhs_effects_and_errors() {
    for operator in ["-", "*", "/", "^"] {
        for right in ["FNTOUCH()", "(1/0)"] {
            let expression = format!("\"x\"{operator}{right}");
            let program = format!(
                "10 DIM C(1):MAT C=77:CALLS=0\n20 ON ERROR GOTO 90\n\
                 30 MAT C={expression}\n40 END\n\
                 90 CAUGHT=ERR:FAILEDLINE=ERL:END\n\
                 100 DEF FNTOUCH()\n110 CALLS=CALLS+1\n120 FNTOUCH=2\n130 FNEND"
            );
            let mut interpreter = run(&program);
            assert_eq!(number(&mut interpreter, "CAUGHT"), 5.0, "{expression}");
            assert_eq!(number(&mut interpreter, "FAILEDLINE"), 30.0, "{expression}");
            assert_eq!(number(&mut interpreter, "CALLS"), 0.0, "{expression}");
            assert_eq!(number(&mut interpreter, "C(0)"), 77.0, "{expression}");
            assert_eq!(
                eval_expression(&mut interpreter, &expression)
                    .unwrap_err()
                    .code,
                ErrorCode::TypeMismatch,
                "{expression}"
            );
            assert_eq!(number(&mut interpreter, "CALLS"), 0.0, "{expression}");
        }
    }
}
