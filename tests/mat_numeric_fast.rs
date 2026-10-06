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
fn scalar_operations_keep_operands_dimensions_and_all_stored_elements() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(&mut interpreter, &format!("MAT BASE {base}:DIM A(2,2)"));
        command(&mut interpreter, "MAT A=4");
        for (expression, expected) in [
            ("A+3", 7.0),
            ("3+A", 7.0),
            ("A-3", 1.0),
            ("3-A", -1.0),
            ("A*3", 12.0),
            ("3*A", 12.0),
            ("A/2", 2.0),
            ("2/A", 0.5),
            ("A^3", 64.0),
            ("3^A", 81.0),
            ("-A", -4.0),
        ] {
            command(&mut interpreter, &format!("MAT C={expression}"));
            for row in 0..=2 {
                for col in 0..=2 {
                    let actual = number(&mut interpreter, &format!("C({row},{col})"));
                    assert_eq!(actual, expected, "{expression}");
                    assert_eq!(number(&mut interpreter, &format!("A({row},{col})")), 4.0);
                }
            }
        }
    }
}

#[test]
fn elementwise_column_results_keep_rank_and_addition_accepts_equivalent_shapes() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM A(3,{base}),B(3)"),
        );
        command(&mut interpreter, "MAT A=5:MAT B=2");
        for (expression, expected, vector) in [
            ("A+B", 7.0, false),
            ("B-A", -3.0, true),
            ("-A", -5.0, false),
        ] {
            command(&mut interpreter, &format!("MAT C={expression}"));
            assert_eq!(number(&mut interpreter, "UBOUND(C)"), 3.0);
            if vector {
                assert_eq!(
                    eval_expression(&mut interpreter, "UBOUND(C,2)")
                        .unwrap_err()
                        .code,
                    ErrorCode::IndexOutOfRange
                );
            } else {
                assert_eq!(number(&mut interpreter, "UBOUND(C,2)"), base as f64);
            }
            for index in 0..=3 {
                assert_eq!(
                    number(
                        &mut interpreter,
                        &if vector {
                            format!("C({index})")
                        } else {
                            format!("C({index},{base})")
                        }
                    ),
                    expected
                );
            }
        }
        command(&mut interpreter, "MAT A=A+B:MAT B=B+B");
        assert_eq!(number(&mut interpreter, &format!("A(2,{base})")), 7.0);
        assert_eq!(number(&mut interpreter, "B(2)"), 4.0);
    }
}

#[test]
fn elementwise_operations_include_stored_cells_when_active_region_is_empty() {
    for dimensions in ["0", "0,2", "2,0", "0,0"] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE 1:DIM A({dimensions}):MAT A=9"),
        );
        command(&mut interpreter, "MAT C=37");
        assert_eq!(
            interpreter.process_immediate("MAT C=A/0").unwrap_err().code,
            ErrorCode::DivisionByZero
        );
        assert_eq!(number(&mut interpreter, "C(0)"), 37.0);
        for (expression, expected) in [("0/A", 0.0), ("-A", -9.0), ("A+A", 18.0)] {
            command(&mut interpreter, &format!("MAT C={expression}"));
            if dimensions.contains(',') {
                assert_eq!(number(&mut interpreter, "C(0,0)"), expected);
            } else {
                assert_eq!(number(&mut interpreter, "C(0)"), expected);
            }
        }
    }
}

#[test]
fn vector_column_elementwise_mapping_matches_python_with_both_bases_and_text() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM V(2),M(2,{base}),V$(2),M$(2,{base})"),
        );
        command(
            &mut interpreter,
            "MAT V=3:MAT M=7:MAT V$=\"v\":MAT M$=\"m\"",
        );
        if base == 1 {
            command(&mut interpreter, "M(0,0)=11:M$(0,0)=\"b\"");
        }
        command(&mut interpreter, "MAT C=V+M:MAT C$=V$+M$");
        assert_eq!(number(&mut interpreter, "UBOUND(C)"), 2.0);
        assert_eq!(number(&mut interpreter, "C(0)"), 10.0);
        assert_eq!(
            eval_expression(&mut interpreter, "C$(0)")
                .unwrap()
                .into_string()
                .unwrap(),
            "vm"
        );
        command(&mut interpreter, "MAT C=M+V:MAT C$=M$+V$");
        assert_eq!(number(&mut interpreter, "UBOUND(C,2)"), base as f64);
        for row in 0..=2 {
            for col in 0..=base {
                let border = base == 1 && row == 0 && col == 0;
                assert_eq!(
                    number(&mut interpreter, &format!("C({row},{col})")),
                    if border { 14.0 } else { 10.0 }
                );
                assert_eq!(
                    eval_expression(&mut interpreter, &format!("C$({row},{col})"))
                        .unwrap()
                        .into_string()
                        .unwrap(),
                    if border { "bv" } else { "mv" }
                );
            }
        }
    }
}

#[test]
fn structural_column_results_keep_two_dimensions_and_transpose_inverse_require_matrices() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!(
                "MAT BASE {base}:DIM A({},{}),B({},{base}),V(2)",
                base + 1,
                base + 1,
                base + 1
            ),
        );
        command(&mut interpreter, "MAT A=2:MAT B=3:MAT C=A*B");
        assert_eq!(number(&mut interpreter, "UBOUND(C,2)"), base as f64);
        assert_eq!(number(&mut interpreter, &format!("C({base},{base})")), 12.0);
        command(&mut interpreter, "MAT D=TRN(C)");
        assert_eq!(number(&mut interpreter, "UBOUND(D,2)"), (base + 1) as f64);
        for expression in ["TRN(V)", "INV(V)"] {
            assert_eq!(
                interpreter
                    .process_immediate(&format!("MAT D={expression}"))
                    .unwrap_err()
                    .code,
                ErrorCode::InvalidDimensions
            );
            assert_eq!(number(&mut interpreter, &format!("D({base},{base})")), 12.0);
        }
    }
}

#[test]
fn inverse_preserves_inactive_input_borders_but_transpose_clears_them() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "MAT BASE 1:DIM A(2,2):MAT A=999:A(1,1)=2:A(1,2)=0:A(2,1)=0:A(2,2)=4",
    );
    command(&mut interpreter, "MAT C=INV(A):MAT D=TRN(A)");
    for row in 0..=2 {
        for col in 0..=2 {
            let inactive = row == 0 || col == 0;
            let inverse = if inactive {
                999.0
            } else if row == col {
                if row == 1 {
                    0.5
                } else {
                    0.25
                }
            } else {
                0.0
            };
            let transpose = if inactive {
                0.0
            } else if row == col {
                if row == 1 {
                    2.0
                } else {
                    4.0
                }
            } else {
                0.0
            };
            assert_eq!(
                number(&mut interpreter, &format!("C({row},{col})")),
                inverse
            );
            assert_eq!(
                number(&mut interpreter, &format!("D({row},{col})")),
                transpose
            );
        }
    }
}

#[test]
fn arithmetic_failures_leave_existing_destination_and_operands_intact() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(2),B(3),T(1,1,1),C(1),S$(1)");
    command(&mut interpreter, "MAT A=4:A(2)=0:MAT C=37");
    for (expression, code) in [
        ("A/0", ErrorCode::DivisionByZero),
        ("2/A", ErrorCode::DivisionByZero),
        ("A+B", ErrorCode::InvalidDimensions),
        ("T*2", ErrorCode::InvalidDimensions),
        ("S$*A", ErrorCode::ForbiddenExpression),
    ] {
        let error = interpreter
            .process_immediate(&format!("MAT C={expression}"))
            .unwrap_err();
        assert_eq!(error.code, code, "{expression}");
        assert_eq!(number(&mut interpreter, "UBOUND(C)"), 1.0);
        assert_eq!(number(&mut interpreter, "C(0)"), 37.0);
        assert_eq!(number(&mut interpreter, "C(1)"), 37.0);
        assert_eq!(number(&mut interpreter, "A(0)"), 4.0);
        assert_eq!(number(&mut interpreter, "A(2)"), 0.0);
    }
}

#[test]
fn matrix_product_keeps_accumulation_order_and_self_assignment() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "DIM A(0,2),B(2,0)");
    command(
        &mut interpreter,
        "A(0,0)=1E16:A(0,1)=-1E16:A(0,2)=1:MAT B=1",
    );
    command(&mut interpreter, "MAT A=A*B");
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 0.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A,2)"), 0.0);
    assert_eq!(number(&mut interpreter, "A(0,0)"), 1.0);
}

#[test]
fn rectangular_products_match_ordered_dot_products_with_either_operand_as_target() {
    let left: [[f64; 5]; 3] = [
        [1e16, -1e16, 1.0, 0.25, -0.25],
        [0.1, -3.0, 1e-16, 7.5, -2.0],
        [-0.0, 2.0, -5.5, 1e16, -1e16],
    ];
    let right: [[f64; 7]; 5] = [
        [1.0, 0.1, -1.0, 2.0, 0.0, -0.5, 1e-16],
        [1.0, 0.1, -1.0, 2.0, 0.0, 0.5, 1e-16],
        [1.0, 3.0, 2.0, 0.25, 7.0, 1.0, -2.0],
        [1.0, 0.5, 4.0, -1.0, 0.0, 3.0, 1.0],
        [1.0, 0.5, 4.0, -1.0, 0.0, -3.0, 1.0],
    ];
    for columns in [2, 3, 4, 5, 7] {
        for base in [0, 1] {
            for target in ["C", "A", "B"] {
                let mut interpreter = Interpreter::new();
                command(
                    &mut interpreter,
                    &format!(
                        "MAT BASE {base}:DIM A({},{}),B({},{})",
                        left.len() + base - 1,
                        left[0].len() + base - 1,
                        right.len() + base - 1,
                        columns + base - 1
                    ),
                );
                command(&mut interpreter, "MAT A=99:MAT B=99");
                for (name, rows) in [
                    (
                        "A",
                        left.iter().map(|row| row.as_slice()).collect::<Vec<_>>(),
                    ),
                    (
                        "B",
                        right.iter().map(|row| &row[..columns]).collect::<Vec<_>>(),
                    ),
                ] {
                    for (row, values) in rows.into_iter().enumerate() {
                        for (col, value) in values.iter().enumerate() {
                            command(
                                &mut interpreter,
                                &format!("{name}({},{})={value:.17e}", row + base, col + base),
                            );
                        }
                    }
                }
                command(&mut interpreter, &format!("MAT {target}=A*B"));
                assert_eq!(
                    number(&mut interpreter, &format!("UBOUND({target},1)")),
                    (left.len() + base - 1) as f64
                );
                assert_eq!(
                    number(&mut interpreter, &format!("UBOUND({target},2)")),
                    (columns + base - 1) as f64
                );
                for (row, values) in left.iter().enumerate() {
                    for col in 0..columns {
                        let mut expected: f64 = 0.0;
                        for k in 0..values.len() {
                            expected += values[k] * right[k][col];
                        }
                        let actual = number(
                            &mut interpreter,
                            &format!("{target}({},{})", row + base, col + base),
                        );
                        assert_eq!(
                            actual.to_bits(),
                            expected.to_bits(),
                            "width {columns}, BASE {base}, {target}, ({row},{col})"
                        );
                    }
                }
                if base == 1 {
                    for row in 0..=left.len() {
                        assert_eq!(number(&mut interpreter, &format!("{target}({row},0)")), 0.0);
                    }
                    for col in 0..=columns {
                        assert_eq!(number(&mut interpreter, &format!("{target}(0,{col})")), 0.0);
                    }
                }
                for (name, rows) in [
                    (
                        "A",
                        left.iter().map(|row| row.as_slice()).collect::<Vec<_>>(),
                    ),
                    (
                        "B",
                        right.iter().map(|row| &row[..columns]).collect::<Vec<_>>(),
                    ),
                ] {
                    if name == target {
                        continue;
                    }
                    for (row, values) in rows.into_iter().enumerate() {
                        for (col, expected) in values.iter().enumerate() {
                            let actual = number(
                                &mut interpreter,
                                &format!("{name}({},{})", row + base, col + base),
                            );
                            assert_eq!(actual.to_bits(), expected.to_bits());
                        }
                    }
                    if base == 1 {
                        assert_eq!(number(&mut interpreter, &format!("{name}(0,0)")), 99.0);
                    }
                }
            }
        }
    }
}

#[test]
fn matrix_square_snapshots_both_aliased_operands() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM A({},{})", base + 1, base + 1),
        );
        for row in 0..2 {
            for col in 0..2 {
                command(
                    &mut interpreter,
                    &format!("A({},{})={}", row + base, col + base, row * 2 + col + 1),
                );
            }
        }
        command(&mut interpreter, "MAT A=A*A");
        for (row, expected) in [[7.0, 10.0], [15.0, 22.0]].iter().enumerate() {
            for (col, value) in expected.iter().enumerate() {
                assert_eq!(
                    number(
                        &mut interpreter,
                        &format!("A({},{})", row + base, col + base)
                    ),
                    *value
                );
            }
        }
    }
}

#[test]
fn empty_matrix_products_do_not_evaluate_nonexistent_terms() {
    for (rows, inner, cols) in [
        (0, 3, 2),
        (2, 0, 3),
        (2, 3, 0),
        (0, 0, 0),
        (2, 0, 1),
        (0, 3, 1),
    ] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE 1:DIM A({rows},{inner}),B({inner},{cols})"),
        );
        command(&mut interpreter, "MAT A=1E309:MAT B=0:MAT C=A*B");
        assert_eq!(number(&mut interpreter, "UBOUND(C,1)"), rows as f64);
        assert_eq!(number(&mut interpreter, "UBOUND(C,2)"), cols as f64);
        for row in 0..=rows {
            for col in 0..=cols {
                assert_eq!(number(&mut interpreter, &format!("C({row},{col})")), 0.0);
            }
        }
    }
}

#[test]
fn matrix_products_keep_zero_times_infinity_and_nan() {
    let values = [
        ("0", 0.0),
        ("-0", -0.0),
        ("1E309", f64::INFINITY),
        ("-1E309", f64::NEG_INFINITY),
        ("1E309:MAT T=T-T", f64::NAN),
        ("1", 1.0),
    ];
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!(
                "MAT BASE {base}:DIM A({},{base}),B({base},{}),T(0)",
                values.len() + base - 1,
                values.len() + base - 1
            ),
        );
        // Scalar arithmetic rejects NaN, whereas operations on MAT arrays
        // propagate it. Build the fixture through T-T, including index 0.
        for (index, (initializer, _)) in values.iter().enumerate() {
            command(
                &mut interpreter,
                &format!(
                    "MAT T={initializer}:MAT A({},{base})=T(0:0):MAT B({base},{})=T(0:0)",
                    index + base,
                    index + base
                ),
            );
        }
        command(&mut interpreter, "MAT C=A*B");
        for (row, (_, left)) in values.iter().enumerate() {
            for (col, (_, right)) in values.iter().enumerate() {
                let expected = 0.0 + left * right;
                let actual = number(
                    &mut interpreter,
                    &format!("C({},{})", row + base, col + base),
                );
                if expected.is_nan() {
                    assert!(actual.is_nan(), "BASE {base}, ({row},{col})");
                } else {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "BASE {base}, ({row},{col})"
                    );
                }
            }
        }
        // Exercise the specialized single-column product with the same
        // non-finite terms, including a zero coefficient times infinity.
        command(&mut interpreter, &format!("DIM V({base})"));
        for (col, (_, right)) in values.iter().enumerate() {
            command(
                &mut interpreter,
                &format!("MAT V=B(,{}):MAT C=A*V", col + base),
            );
            for (row, (_, left)) in values.iter().enumerate() {
                let expected = 0.0 + left * right;
                let actual = number(&mut interpreter, &format!("C({},{base})", row + base));
                if expected.is_nan() {
                    assert!(actual.is_nan(), "column, BASE {base}, ({row},{col})");
                } else {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "column, BASE {base}, ({row},{col})"
                    );
                }
            }
        }
    }
}

#[test]
fn function_operands_are_evaluated_once_left_to_right_and_snapshotted() {
    let mut interpreter = Interpreter::new();
    for line in r#"10 DIM A(1):N=0
20 MAT C=FNLEFT(0)+FNRIGHT(0)
30 END
100 DEF FNLEFT(X)
110 N=N+1:A(0)=3
120 MAT FNLEFT=A
130 FNEND
200 DEF FNRIGHT(X)
210 N=N+1:A(0)=7
220 MAT FNRIGHT=A
230 FNEND"#
        .lines()
    {
        command(&mut interpreter, line);
    }
    interpreter.run_loaded().unwrap();
    assert_eq!(number(&mut interpreter, "N"), 2.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 7.0);
    assert_eq!(number(&mut interpreter, "C(0)"), 10.0);
}
