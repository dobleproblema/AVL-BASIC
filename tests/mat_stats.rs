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

#[test]
fn rectangular_statistics_skip_base_borders_and_retain_first_ties() {
    let values = [[-3.0, 2.0, 5.0], [5.0, -4.0, 1.0]];
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM A({},{})", base + 1, base + 2),
        );
        command(&mut interpreter, "MAT A=999");
        for (row, values) in values.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                command(
                    &mut interpreter,
                    &format!("A({},{})={value}", row + base, column + base),
                );
            }
        }
        for (function, expected) in [
            ("SUM", 6.0),
            ("ABSUM", 20.0),
            ("FNORM", 80.0_f64.sqrt()),
            ("AMAX", 5.0),
            ("AMIN", -4.0),
            ("MAXAB", 5.0),
            ("RNORM", 10.0),
            ("CNORM", 8.0),
        ] {
            assert_eq!(
                number(&mut interpreter, &format!("{function}(A)")),
                expected,
                "{function}"
            );
        }
        for (variable, position) in [
            ("AMAXROW", base),
            ("AMAXCOL", base + 2),
            ("AMINROW", base + 1),
            ("AMINCOL", base + 1),
            ("MAXABROW", base),
            ("MAXABCOL", base + 2),
            ("RNORMROW", base),
            ("CNORMCOL", base),
        ] {
            assert_eq!(
                number(&mut interpreter, variable),
                position as f64,
                "{variable}"
            );
        }
        if base == 1 {
            assert_eq!(number(&mut interpreter, "A(0,0)"), 999.0);
        }
    }
}

#[test]
fn vector_and_column_shapes_share_statistics_and_dot_products() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!(
                "MAT BASE {base}:DIM V({}),C({},{base}),W({})",
                base + 2,
                base + 2,
                base + 2
            ),
        );
        command(&mut interpreter, "MAT V=999:MAT C=999:MAT W=999");
        for (index, value) in [-2, 3, -3].iter().enumerate() {
            command(
                &mut interpreter,
                &format!(
                    "V({})={value}:C({},{base})={value}:W({})={}",
                    index + base,
                    index + base,
                    index + base,
                    index + 1
                ),
            );
        }
        for array in ["V", "C"] {
            for (function, expected) in [
                ("SUM", -2.0),
                ("ABSUM", 8.0),
                ("FNORM", 22.0_f64.sqrt()),
                ("AMAX", 3.0),
                ("AMIN", -3.0),
                ("MAXAB", 3.0),
                ("RNORM", 3.0),
                ("CNORM", 8.0),
            ] {
                assert_eq!(
                    number(&mut interpreter, &format!("{function}({array})")),
                    expected
                );
            }
            assert_eq!(number(&mut interpreter, "RNORMROW"), (base + 1) as f64);
            assert_eq!(number(&mut interpreter, "CNORMCOL"), base as f64);
            assert_eq!(number(&mut interpreter, "MAXABROW"), (base + 1) as f64);
            assert_eq!(number(&mut interpreter, "MAXABCOL"), base as f64);
            assert_eq!(number(&mut interpreter, &format!("DOT({array},W)")), -5.0);
            assert_eq!(number(&mut interpreter, &format!("DOT(W,{array})")), -5.0);
        }
    }
}

#[test]
fn only_successful_requested_statistics_update_their_context() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(2),S$(2),T(1,1,1),B(2,2):A(0)=-5:A(1)=7:A(2)=3:B(2,2)=1",
    );
    let contexts = [
        "AMAXROW", "AMAXCOL", "AMINROW", "AMINCOL", "MAXABROW", "MAXABCOL", "CNORMCOL", "RNORMROW",
    ];
    for expression in ["AMAX(B)", "MAXAB(B)", "CNORM(B)", "RNORM(B)"] {
        number(&mut interpreter, expression);
    }
    command(&mut interpreter, "B(2,2)=-1");
    number(&mut interpreter, "AMIN(B)");
    for expression in ["SUM(A)", "ABSUM(A)", "FNORM(A)", "DOT(A,A)"] {
        number(&mut interpreter, expression);
    }
    for context in contexts {
        assert_eq!(number(&mut interpreter, context), 2.0);
    }
    assert_eq!(number(&mut interpreter, "AMAX(A)"), 7.0);
    for context in &contexts[2..] {
        assert_eq!(number(&mut interpreter, context), 2.0);
    }
    assert_eq!(number(&mut interpreter, "AMAXROW"), 1.0);
    assert_eq!(number(&mut interpreter, "AMAXCOL"), 0.0);
    for (expression, error) in [
        ("AMAX(S$)", ErrorCode::TypeMismatch),
        ("AMAX(T)", ErrorCode::InvalidDimensions),
        ("AMAX(MISSING)", ErrorCode::Undefined),
        ("AMAX(A,A)", ErrorCode::ArgumentMismatch),
    ] {
        assert_eq!(
            eval_expression(&mut interpreter, expression)
                .unwrap_err()
                .code,
            error
        );
        assert_eq!(number(&mut interpreter, "AMAXROW"), 1.0);
    }
}

#[test]
fn empty_active_regions_reset_only_requested_positions() {
    for dimensions in ["0", "0,2", "2,0", "0,0"] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE 1:DIM A({dimensions}):MAT A=999"),
        );
        for function in [
            "SUM", "ABSUM", "FNORM", "AMAX", "AMIN", "MAXAB", "RNORM", "CNORM",
        ] {
            assert_eq!(number(&mut interpreter, &format!("{function}(A)")), 0.0);
        }
        for context in [
            "AMAXROW", "AMAXCOL", "AMINROW", "AMINCOL", "MAXABROW", "MAXABCOL", "RNORMROW",
            "CNORMCOL",
        ] {
            assert_eq!(number(&mut interpreter, context), 0.0);
        }
    }
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 1:DIM V(0),C(0,1)");
    assert_eq!(
        number(&mut interpreter, "DOT(V,C)").to_bits(),
        std::iter::empty::<f64>().sum::<f64>().to_bits()
    );
}

#[test]
fn dot_checks_names_types_column_shapes_and_lengths_in_existing_order() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM V(2),W(3),M(1,2),T(0,0,0),S$(1)");
    for (expression, error) in [
        ("DOT(V)", ErrorCode::ArgumentMismatch),
        ("DOT(S$,MISSING)", ErrorCode::Undefined),
        ("DOT(M,S$)", ErrorCode::InvalidDimensions),
        ("DOT(S$,M)", ErrorCode::TypeMismatch),
        ("DOT(V,T)", ErrorCode::InvalidDimensions),
        ("DOT(V,S$)", ErrorCode::TypeMismatch),
        ("DOT(V,W)", ErrorCode::InvalidDimensions),
        ("DOT(M,V)", ErrorCode::InvalidDimensions),
    ] {
        assert_eq!(
            eval_expression(&mut interpreter, expression)
                .unwrap_err()
                .code,
            error,
            "{expression}"
        );
    }
}

#[test]
fn sequential_reductions_keep_rounding_order() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(3),W(3):A(0)=1E16:A(1)=1:A(2)=-1E16:A(3)=2:MAT W=1",
    );
    assert_eq!(number(&mut interpreter, "SUM(A)"), 2.0);
    assert_eq!(number(&mut interpreter, "DOT(A,W)"), 2.0);
    command(&mut interpreter, "A(0)=1E16:A(1)=1:A(2)=1:A(3)=1");
    assert_eq!(number(&mut interpreter, "ABSUM(A)"), 1E16);
    assert_eq!(number(&mut interpreter, "CNORM(A)"), 1E16);
    command(&mut interpreter, "A(0)=1E8");
    assert_eq!(number(&mut interpreter, "FNORM(A)"), 1E8);
}

#[test]
fn statistics_follow_dynamic_base_redim_and_subroutine_array_aliases() {
    let mut interpreter = Interpreter::new();
    let program = "10 DIM A(2),B(3):MAT A=2:MAT B=3\n20 DEF SUB STAT(P)\n30 TOTAL=TOTAL+SUM(P):PEAK=AMAX(P)\n40 SUBEND\n50 CALL STAT(A):CALL STAT(B)\n60 MAT BASE 1:CALL STAT(A)\n70 REDIM A(4):MAT A=5:CALL STAT(A)\n80 END";
    for line in program.lines() {
        command(&mut interpreter, line);
    }
    interpreter.run_loaded().unwrap();
    assert_eq!(number(&mut interpreter, "TOTAL"), 42.0);
    assert_eq!(number(&mut interpreter, "PEAK"), 5.0);
    assert_eq!(number(&mut interpreter, "AMAXROW"), 1.0);
    assert_eq!(number(&mut interpreter, "AMAXCOL"), 1.0);
}

#[test]
fn determinant_copies_active_region_without_changing_source_or_error_contract() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!(
                "MAT BASE {base}:DIM A({},{}),V({base}),R({},{}),S$(1),T(1,1,1)",
                base + 1,
                base + 1,
                base + 1,
                base + 2
            ),
        );
        command(&mut interpreter, "MAT A=999:MAT V=7");
        for (row, values) in [[1, 2], [3, 4]].iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                command(
                    &mut interpreter,
                    &format!("A({},{})={value}", row + base, column + base),
                );
            }
        }
        assert!((number(&mut interpreter, "DET(A)") + 2.0).abs() < 1E-12);
        assert_eq!(number(&mut interpreter, "DET(V)"), 7.0);
        for (row, values) in [[1, 2], [3, 4]].iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                assert_eq!(
                    number(
                        &mut interpreter,
                        &format!("A({},{})", row + base, column + base)
                    ),
                    *value as f64
                );
            }
        }
        assert_eq!(number(&mut interpreter, "UBOUND(A,1)"), (base + 1) as f64);
        assert_eq!(number(&mut interpreter, "UBOUND(A,2)"), (base + 1) as f64);
        if base == 1 {
            assert_eq!(number(&mut interpreter, "A(0,0)"), 999.0);
            assert_eq!(number(&mut interpreter, "A(0,1)"), 999.0);
            assert_eq!(number(&mut interpreter, "A(1,0)"), 999.0);
        }
        for (expression, error) in [
            ("DET(R)", ErrorCode::InvalidDimensions),
            ("DET(S$)", ErrorCode::TypeMismatch),
            ("DET(T)", ErrorCode::InvalidDimensions),
            ("DET(MISSING)", ErrorCode::Undefined),
            ("DET(A,A)", ErrorCode::ArgumentMismatch),
        ] {
            assert_eq!(
                eval_expression(&mut interpreter, expression)
                    .unwrap_err()
                    .code,
                error,
                "{expression}"
            );
        }
    }
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 1:DIM E(0,0),V(0):MAT E=999");
    assert_eq!(number(&mut interpreter, "DET(E)"), 1.0);
    assert_eq!(number(&mut interpreter, "E(0,0)"), 999.0);
    assert_eq!(
        eval_expression(&mut interpreter, "DET(V)")
            .unwrap_err()
            .code,
        ErrorCode::InvalidDimensions
    );
}

#[test]
fn determinant_does_not_classify_small_nonzero_pivots_as_singular() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM A({base},{base})"),
        );
        for value in [1e-13_f64, -1e-13, 1e-200] {
            command(&mut interpreter, &format!("A({base},{base})={value:.17e}"));
            assert_eq!(number(&mut interpreter, "DET(A)"), value);
        }
        command(
            &mut interpreter,
            &format!("DIM A({},{}):MAT A=0", base + 1, base + 1),
        );
        command(
            &mut interpreter,
            &format!("A({base},{base})=1E-13:A({},{})=2E-13", base + 1, base + 1),
        );
        assert_eq!(number(&mut interpreter, "DET(A)"), 1e-13 * 2e-13);
        command(&mut interpreter, &format!("A({},{})=0", base + 1, base + 1));
        assert_eq!(number(&mut interpreter, "DET(A)"), 0.0);
    }
}

#[test]
fn determinant_recovers_products_after_extreme_pivots_in_either_order() {
    for base in [0, 1] {
        for values in [
            [1e200, 1e200, 1e-200, 1e-200],
            [1e-200, 1e-200, 1e200, 1e200],
            [1e-160, 1e-160, 1e160, 1e160],
        ] {
            let mut interpreter = Interpreter::new();
            command(
                &mut interpreter,
                &format!("MAT BASE {base}:DIM A({},{}):MAT A=0", base + 3, base + 3),
            );
            for (index, value) in values.into_iter().enumerate() {
                command(
                    &mut interpreter,
                    &format!("A({},{})={value:.17e}", base + index, base + index),
                );
            }
            let determinant = number(&mut interpreter, "DET(A)");
            assert!(
                (determinant - 1.0).abs() < 1e-14,
                "{values:?}: {determinant}"
            );
            command(&mut interpreter, "MAT B=INV(A)");
            assert_eq!(number(&mut interpreter, "DET(A)"), determinant);
            // Swapping the first two rows changes only the determinant sign.
            command(
                &mut interpreter,
                &format!(
                    "A({base},{base})=0:A({},{})=0:A({base},{})={:.17e}:A({},{base})={:.17e}",
                    base + 1,
                    base + 1,
                    base + 1,
                    values[1],
                    base + 1,
                    values[0]
                ),
            );
            assert!((number(&mut interpreter, "DET(A)") + 1.0).abs() < 1e-14);
        }
    }
}

#[test]
fn frobenius_norm_preserves_representable_extreme_results_and_active_bases() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!(
                "MAT BASE {base}:DIM V({}),M({},{}):MAT V=999:MAT M=999",
                base + 1,
                base,
                base + 1
            ),
        );
        for scale in [1e200_f64, 1e-200, 1e-160] {
            for (index, coefficient) in [3.0, -4.0].into_iter().enumerate() {
                let value = coefficient * scale;
                command(
                    &mut interpreter,
                    &format!(
                        "V({})={value:.17e}:M({base},{})={value:.17e}",
                        base + index,
                        base + index
                    ),
                );
            }
            for array in ["V", "M"] {
                let actual = number(&mut interpreter, &format!("FNORM({array})"));
                assert!(actual.is_finite());
                assert!((actual / (5.0 * scale) - 1.0).abs() < 1e-14);
            }
        }
        command(
            &mut interpreter,
            &format!("V({base})=1E200:V({})=1E-200", base + 1),
        );
        assert_eq!(number(&mut interpreter, "FNORM(V)"), 1e200);
        command(
            &mut interpreter,
            &format!("V({base})=1E308:V({})=1E308", base + 1),
        );
        let actual = number(&mut interpreter, "FNORM(V)");
        assert!(actual.is_finite());
        assert!((actual / 1e308 - 2.0_f64.sqrt()).abs() < 1e-14);
        command(&mut interpreter, "MAT V=0:MAT M=0");
        for array in ["V", "M"] {
            assert_eq!(
                number(&mut interpreter, &format!("FNORM({array})")).to_bits(),
                0.0_f64.to_bits()
            );
        }
    }
}
