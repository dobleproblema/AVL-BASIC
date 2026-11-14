use avl_basic::expr::eval_expression;
use avl_basic::Interpreter;

fn run(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.run_loaded().unwrap();
    interpreter
}

fn number(interpreter: &mut Interpreter, expression: &str) -> f64 {
    eval_expression(interpreter, expression)
        .unwrap()
        .as_number()
        .unwrap()
}

#[test]
fn cached_affine_map_preserves_base_one_border_and_both_roundings() {
    // Each row has six active columns: one vector group and two tail cells.
    let mut interpreter = run(r#"10 MAT BASE 1:DIM A(2,6),C(2,6):A=71:MAT C=99
20 FOR I=0 TO 2
30 FOR J=0 TO 6
40 A(I,J)=(I*8+J+.25)/16
50 NEXT J
60 NEXT I
70 A(0,1)=.25:A(1,0)=.375
80 A(1,1)=-.1:A(1,2)=-.10000000000000002:A(1,5)=-.1:A(1,6)=-.10000000000000002
90 A(2,1)=-.10000000000000002:A(2,5)=-.10000000000000002:A(2,6)=-.1
100 MAT D=10*A+1
110 MAT E=MAP(INT,D)
120 MAT C=MAP(INT,10*A+1)
130 END"#);
    assert_eq!(number(&mut interpreter, "UBOUND(C)"), 2.0);
    assert_eq!(number(&mut interpreter, "UBOUND(C,2)"), 6.0);
    assert_eq!(number(&mut interpreter, "A"), 71.0);
    for row in 0..=2 {
        for col in 0..=6 {
            let result = number(&mut interpreter, &format!("C({row},{col})"));
            let reference = number(&mut interpreter, &format!("E({row},{col})"));
            assert_eq!(result.to_bits(), reference.to_bits(), "cell {row},{col}");
        }
    }
    // With an FMA, the first and fifth active cells would floor to -1.
    for (cell, expected) in [
        ("C(0,1)", 3.5_f64),
        ("C(1,0)", 4.75),
        ("C(1,1)", 0.0),
        ("C(1,2)", -1.0),
        ("C(1,5)", 0.0),
        ("C(1,6)", -1.0),
        ("A(1,0)", 0.375),
    ] {
        assert_eq!(
            number(&mut interpreter, cell).to_bits(),
            expected.to_bits(),
            "{cell}"
        );
    }
}

#[test]
fn cached_affine_map_handles_array_aliases_and_nonfinite_values() {
    let mut interpreter = run(r#"10 MAT BASE 0:DIM A(8),B(8)
20 A(0)=INF:A(1)=-INF:A(2)=1E308:A(3)=-0:A(4)=-1E-310:A(5)=.5:A(6)=INF:A(7)=-INF:A(8)=-.1
30 MAT B=A
40 MAT C=0*B+0:MAT D=MAP(INT,C)
50 CALL TRANSFORM(A,A)
60 MAT B=MAP(INT,1E-310*B+0)
70 END
100 DEF SUB TRANSFORM(X,Y)
110 MAT X=MAP(INT,0*Y+0)
120 SUBEND"#);
    for i in 0..=8 {
        let result = number(&mut interpreter, &format!("A({i})"));
        let reference = number(&mut interpreter, &format!("D({i})"));
        if reference.is_nan() {
            assert!(result.is_nan(), "cell {i}");
        } else {
            assert_eq!(result.to_bits(), reference.to_bits(), "cell {i}");
        }
    }
    assert_eq!(number(&mut interpreter, "B(0)"), f64::INFINITY);
    assert_eq!(number(&mut interpreter, "B(1)"), f64::NEG_INFINITY);
    for i in [3, 4] {
        assert_eq!(
            number(&mut interpreter, &format!("B({i})")).to_bits(),
            0.0_f64.to_bits(),
            "cell {i}"
        );
    }
}

#[test]
fn cached_direct_int_keeps_the_border_and_handles_both_slot_orders_and_resize() {
    let mut interpreter = run(r#"10 MAT BASE 1:DIM C(1,5),A(1,5):MAT A=2.75
20 A(0,1)=-0:A(1,0)=-.25:A(1,5)=-1.25
30 MAT C=MAP(INT,A):MAT D=MAP(INT,A)
40 MAT A=MAP(INT,A)
50 END"#);
    assert_eq!(number(&mut interpreter, "UBOUND(D,2)"), 5.0);
    for name in ["A", "C", "D"] {
        for (row, col, expected) in [(0, 1, -0.0_f64), (1, 0, -0.25), (1, 1, 2.0), (1, 5, -2.0)] {
            assert_eq!(
                number(&mut interpreter, &format!("{name}({row},{col})")).to_bits(),
                expected.to_bits()
            );
        }
    }
}

#[test]
fn cached_scaled_int_preserves_base_one_border_and_signed_zero() {
    let mut interpreter = run(r#"10 MAT BASE 1:DIM A(2,6),B(2,6),C(2,6)
20 MAT A=.275:MAT B=99:MAT C=99
30 A(0,1)=-0:A(1,0)=-.125:A(1,1)=-0:A(1,5)=-0:A(2,6)=-.125
40 MAT D=10*A:MAT E=MAP(INT,D)
50 MAT B=MAP(INT,10*A):MAT C=MAP(INT,A*10)
60 MAT F=MAP(INT,10*A+0)
70 CALL TRANSFORM(A,A)
80 END
100 DEF SUB TRANSFORM(X,Y)
110 MAT X=MAP(INT,Y*10)
120 SUBEND"#);
    for name in ["A", "B", "C"] {
        for row in 0..=2 {
            for col in 0..=6 {
                let result = number(&mut interpreter, &format!("{name}({row},{col})"));
                let reference = number(&mut interpreter, &format!("E({row},{col})"));
                assert_eq!(result.to_bits(), reference.to_bits(), "{name}({row},{col})");
            }
        }
    }
    for (row, col, expected) in [
        (0, 1, -0.0_f64),
        (1, 0, -1.25),
        (1, 1, -0.0),
        (1, 5, -0.0),
        (2, 6, -2.0),
    ] {
        assert_eq!(
            number(&mut interpreter, &format!("B({row},{col})")).to_bits(),
            expected.to_bits(),
            "B({row},{col})"
        );
    }
    for (row, col) in [(0, 1), (1, 1), (1, 5)] {
        assert_eq!(
            number(&mut interpreter, &format!("F({row},{col})")).to_bits(),
            0.0_f64.to_bits(),
            "F({row},{col})"
        );
    }
}

#[test]
fn unary_map_matches_scalar_functions_when_reusing_a_destination() {
    let mut interpreter = run(r#"10 MAT BASE 1:DIM A(1,3),B(1,3),C(1,3),D(1,3),E(1,3),U(2)
20 MAT A=-7:MAT B=99:MAT C=99:MAT D=99:MAT E=99
30 A(1,1)=30:A(1,2)=60:A(1,3)=90
40 DEG:MAT B=MAP(SIN,A)
50 RAD:MAT C=MAP(SIN,A)
60 MAT D=MAP(LOG,2*A)
70 P=2:MAT E=MAP(LOG,P*A)
80 U(0)=-8.5:U(1)=-3.25:U(2)=-0:CALL MAKEABS(U,U)
90 END
100 DEF SUB MAKEABS(X,Y)
110 MAT X=MAP(ABS,Y)
120 SUBEND"#);
    for col in 1..=3 {
        let expected = number(&mut interpreter, &format!("SIN({})", col * 30));
        let actual = number(&mut interpreter, &format!("C(1,{col})"));
        assert_eq!(actual.to_bits(), expected.to_bits(), "C(1,{col}) in RAD");
    }
    interpreter
        .process_immediate("DEG:MAT C=MAP(SIN,A)")
        .unwrap();

    for col in 1..=3 {
        let literal = col * 30;
        for name in ["B", "C"] {
            let expected = number(&mut interpreter, &format!("SIN({literal})"));
            let actual = number(&mut interpreter, &format!("{name}(1,{col})"));
            assert_eq!(actual.to_bits(), expected.to_bits(), "{name}(1,{col})");
        }
        for name in ["D", "E"] {
            let expected = number(&mut interpreter, &format!("LOG(2*{literal})"));
            let actual = number(&mut interpreter, &format!("{name}(1,{col})"));
            assert_eq!(actual.to_bits(), expected.to_bits(), "{name}(1,{col})");
        }
    }
    for (cell, expected) in [
        ("A(1,2)", 60.0_f64),
        ("A(0,1)", -7.0),
        ("B(0,1)", -7.0),
        ("C(1,0)", -7.0),
        ("D(0,1)", -14.0),
        ("E(1,0)", -14.0),
        ("U(0)", -8.5),
        ("U(1)", 3.25),
        ("U(2)", 0.0),
    ] {
        assert_eq!(
            number(&mut interpreter, cell).to_bits(),
            expected.to_bits(),
            "{cell}"
        );
    }
}

#[test]
fn unary_map_domain_errors_leave_same_shape_destinations_unchanged() {
    let setup = "MAT BASE 1:DIM A(3),B(3):MAT A=4:MAT B=71:A(3)=0";
    let mut interpreter = Interpreter::new();
    interpreter.process_immediate(setup).unwrap();
    let expected = eval_expression(&mut interpreter, "LOG(0)")
        .unwrap_err()
        .code;
    for assignment in ["MAT A=MAP(LOG,A)", "MAT B=MAP(LOG,2*A)"] {
        for stored in [false, true] {
            let error = if stored {
                interpreter
                    .program
                    .load_text(&format!("10 {setup}\n20 {assignment}\n30 END"))
                    .unwrap();
                interpreter.run_loaded().unwrap_err()
            } else {
                interpreter.process_immediate(assignment).unwrap_err()
            };
            assert_eq!(error.code, expected, "{assignment}");
            assert_eq!(number(&mut interpreter, "A(1)"), 4.0);
            assert_eq!(number(&mut interpreter, "A(3)"), 0.0);
            assert_eq!(number(&mut interpreter, "B(1)"), 71.0);
            assert_eq!(number(&mut interpreter, "B(3)"), 71.0);
        }
    }
}

#[test]
fn cached_map_resolves_coefficients_again_when_their_values_or_types_change() {
    let mut interpreter = run(r#"10 MAT BASE 1:DIM A(2,2),B(2,2):MAT A=2
20 X=3:Y=1:GOSUB 100
30 V1=B(1,1)
40 X=5:Y=2:GOSUB 100
50 V2=B(1,1)
60 DIM X(2,2):MAT X=1:GOSUB 100
70 V3=B(1,1)
80 END
100 MAT B=MAP(INT,X*A+Y)
110 RETURN"#);
    for (value, expected) in [("V1", 7.0), ("V2", 12.0), ("V3", 6.0)] {
        assert_eq!(number(&mut interpreter, value), expected, "{value}");
    }
}

#[test]
fn map_evaluates_an_rng_coefficient_once() {
    let mut interpreter = run(r#"10 MAT BASE 1:DIM A(2,2),B(2,2):MAT A=100
20 RANDOMIZE 12:R=RND:N1=RND
30 RANDOMIZE 12
40 MAT B=MAP(INT,RND*A)
50 N2=RND
60 END"#);
    assert_eq!(
        number(&mut interpreter, "N1"),
        number(&mut interpreter, "N2")
    );
    let expected = number(&mut interpreter, "INT(R*100)");
    for cell in ["B(1,1)", "B(1,2)", "B(2,1)", "B(2,2)"] {
        assert_eq!(number(&mut interpreter, cell), expected, "{cell}");
    }
}
