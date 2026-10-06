use avl_basic::expr::eval_expression;
use avl_basic::{ErrorCode, Interpreter};

fn number(interpreter: &mut Interpreter, expression: &str) -> f64 {
    eval_expression(interpreter, expression)
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn inverse_of_an_uninitialized_matrix_reports_invalid_value_at_its_source_line() {
    let mut interpreter = Interpreter::new();
    for line in ["15 DIM a(3,3)", "20 MAT BASE 1", "25 MAT b=INV(a)"] {
        interpreter.process_immediate(line).unwrap();
    }

    let error = interpreter.process_immediate("RUN").unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidValue);
    assert_eq!(error.line, Some(25));
    assert_eq!(error.display_for_basic(), "Line 25. Invalid value.");
}

#[test]
fn inverse_of_a_nonzero_singular_matrix_reports_invalid_value_for_both_bases() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        interpreter
            .process_immediate(&format!("MAT BASE {base}:DIM A({},{})", base + 1, base + 1))
            .unwrap();
        for (row, col, value) in [(0, 0, 1), (0, 1, 2), (1, 0, 2), (1, 1, 4)] {
            interpreter
                .process_immediate(&format!("A({},{})={value}", base + row, base + col))
                .unwrap();
        }

        let error = interpreter.process_immediate("MAT B=INV(A)").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidValue, "MAT BASE {base}");
    }
}

#[test]
fn singular_inverse_error_handlers_receive_invalid_value_code_and_source_line() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 ON ERROR GOTO 100\n15 DIM A(3,3)\n20 MAT BASE 1\n25 MAT B=INV(A)\n\
             30 END\n100 E=ERR:L=ERL\n110 RESUME NEXT",
        )
        .unwrap();

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(number(&mut interpreter, "E"), 4.0);
    assert_eq!(number(&mut interpreter, "L"), 25.0);
}

#[test]
fn inverse_can_swap_a_zero_diagonal_pivot_to_invert_a_nonsingular_matrix() {
    let mut interpreter = Interpreter::new();
    interpreter
        .process_immediate("MAT BASE 1:DIM A(2,2):A(1,2)=1:A(2,1)=1:MAT B=INV(A)")
        .unwrap();

    for (row, col, expected) in [(1, 1, 0.0), (1, 2, 1.0), (2, 1, 1.0), (2, 2, 0.0)] {
        assert_eq!(
            number(&mut interpreter, &format!("B({row},{col})")),
            expected
        );
    }
}

#[test]
fn matrix_division_and_invalid_inverse_dimensions_keep_their_errors() {
    let mut interpreter = Interpreter::new();
    interpreter
        .process_immediate("MAT BASE 1:DIM A(2,2),C(2,3)")
        .unwrap();

    for (expression, expected) in [
        ("A/0", ErrorCode::DivisionByZero),
        ("2/A", ErrorCode::DivisionByZero),
        ("INV(C)", ErrorCode::InvalidDimensions),
    ] {
        let error = interpreter
            .process_immediate(&format!("MAT B={expression}"))
            .unwrap_err();
        assert_eq!(error.code, expected, "{expression}");
    }
}

#[test]
fn inverse_normalizes_extreme_finite_magnitudes_without_a_false_inverse() {
    for base in [0, 1] {
        for scale in [1e308_f64, 1e-308] {
            let mut interpreter = Interpreter::new();
            interpreter
                .process_immediate(&format!(
                    "MAT BASE {base}:DIM A({},{}):MAT A=999",
                    base + 1,
                    base + 1
                ))
                .unwrap();
            for (row, col, sign) in [(0, 0, 1.0), (0, 1, 1.0), (1, 0, 1.0), (1, 1, -1.0)] {
                let value = sign * scale;
                interpreter
                    .process_immediate(&format!("A({},{})={value:.17e}", base + row, base + col))
                    .unwrap();
            }
            interpreter
                .process_immediate("MAT B=INV(A):MAT C=A*B")
                .unwrap();
            for (row, col, sign) in [(0, 0, 1.0), (0, 1, 1.0), (1, 0, 1.0), (1, 1, -1.0)] {
                let actual = number(
                    &mut interpreter,
                    &format!("B({},{})", base + row, base + col),
                );
                let expected = sign * (0.5 / scale);
                assert!(actual.is_finite());
                assert!((actual / expected - 1.0).abs() < 1e-14);
                let product = number(
                    &mut interpreter,
                    &format!("C({},{})", base + row, base + col),
                );
                assert!((product - if row == col { 1.0 } else { 0.0 }).abs() < 1e-14);
            }
            if base == 1 {
                assert_eq!(number(&mut interpreter, "B(0,0)"), 999.0);
                assert_eq!(number(&mut interpreter, "A(0,0)"), 999.0);
            }
        }
    }
}

#[test]
fn inverse_keeps_widely_separated_representable_scales() {
    let mut interpreter = Interpreter::new();
    interpreter
        .process_immediate("DIM A(1,1):A(0,0)=1E308:A(1,1)=1E-308:MAT B=INV(A)")
        .unwrap();
    assert_eq!(number(&mut interpreter, "B(0,0)"), 1.0 / 1e308);
    assert_eq!(number(&mut interpreter, "B(1,1)"), 1.0 / 1e-308);
    assert_eq!(number(&mut interpreter, "B(0,1)"), 0.0);
    assert_eq!(number(&mut interpreter, "B(1,0)"), 0.0);
}

#[test]
fn inverse_rejects_nonfinite_inputs_and_unrepresentable_results_atomically() {
    for base in [0, 1] {
        for setup in [
            "MAT A=1E309",
            "MAT A=-1E309",
            "MAT A=1E309:MAT A=A-A",
            "MAT A=1E-309",
        ] {
            let mut interpreter = Interpreter::new();
            interpreter
                .process_immediate(&format!(
                    "MAT BASE {base}:DIM A({base},{base}),B(1):MAT B=37"
                ))
                .unwrap();
            interpreter.process_immediate(setup).unwrap();
            let error = interpreter.process_immediate("MAT B=INV(A)").unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidValue, "BASE {base}, {setup}");
            assert_eq!(number(&mut interpreter, "UBOUND(B)"), 1.0);
            assert_eq!(number(&mut interpreter, "B(0)"), 37.0);
            assert_eq!(number(&mut interpreter, "B(1)"), 37.0);
        }
    }
}
