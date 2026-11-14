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
fn hadamard_keeps_left_rank_and_matches_mat_column_indexing() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM A(2,{base}),B(2,{base}),V(2)"),
        );
        command(
            &mut interpreter,
            "MAT A=3:MAT B=7:MAT V=2:A(0,0)=5:B(0,0)=11:V(0)=-2",
        );
        command(&mut interpreter, "MAT C=A@B");
        assert_eq!(number(&mut interpreter, "UBOUND(C,2)"), base as f64);
        assert_eq!(number(&mut interpreter, "C(0,0)"), 55.0);
        assert_eq!(number(&mut interpreter, &format!("C(1,{base})")), 21.0);

        command(&mut interpreter, "MAT C=V@A");
        assert_eq!(number(&mut interpreter, "UBOUND(C)"), 2.0);
        assert_eq!(
            eval_expression(&mut interpreter, "UBOUND(C,2)")
                .unwrap_err()
                .code,
            ErrorCode::IndexOutOfRange
        );
        assert_eq!(
            number(&mut interpreter, "C(0)"),
            if base == 0 { -10.0 } else { -6.0 }
        );
        assert_eq!(number(&mut interpreter, "C(1)"), 6.0);

        command(&mut interpreter, "MAT A=A @ V");
        assert_eq!(number(&mut interpreter, "UBOUND(A,2)"), base as f64);
        assert_eq!(number(&mut interpreter, "A(0,0)"), -10.0);
        assert_eq!(number(&mut interpreter, &format!("A(1,{base})")), 6.0);
        assert_eq!(number(&mut interpreter, "V(0)"), -2.0);
    }
}

#[test]
fn array_division_matches_active_vector_and_column_shapes_in_both_orders() {
    for base in [0, 1] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!("MAT BASE {base}:DIM V(2),M(2,{base}):MAT V=12:MAT M=3"),
        );
        if base == 1 {
            command(&mut interpreter, "V(0)=17:M(0,1)=0:M(1,0)=19");
        }
        command(&mut interpreter, "MAT C=V/M");
        assert_eq!(number(&mut interpreter, "UBOUND(C)"), 2.0);
        assert_eq!(
            eval_expression(&mut interpreter, "UBOUND(C,2)")
                .unwrap_err()
                .code,
            ErrorCode::IndexOutOfRange
        );
        assert_eq!(
            number(&mut interpreter, "C(0)"),
            if base == 0 { 4.0 } else { 17.0 }
        );
        assert_eq!(number(&mut interpreter, "C(1)"), 4.0);
        assert_eq!(number(&mut interpreter, "C(2)"), 4.0);

        if base == 1 {
            command(&mut interpreter, "V(0)=0");
        }
        command(&mut interpreter, "MAT D=M/V");
        assert_eq!(number(&mut interpreter, "UBOUND(D,2)"), base as f64);
        assert_eq!(number(&mut interpreter, &format!("D(1,{base})")), 0.25);
        assert_eq!(number(&mut interpreter, &format!("D(2,{base})")), 0.25);
        if base == 0 {
            assert_eq!(number(&mut interpreter, "D(0,0)"), 0.25);
        } else {
            assert_eq!(number(&mut interpreter, "D(0,1)"), 0.0);
            assert_eq!(number(&mut interpreter, "D(1,0)"), 19.0);
        }
    }
}

#[test]
fn array_denominators_compose_with_hadamard_and_powers_in_loaded_mat() {
    let mut interpreter = run("10 MAT BASE 1:DIM A(2),B(2):A(0)=4:B(0)=9\n\
         20 DATA 12,18,2,3\n30 MAT READ A,B\n\
         40 MAT C=8/B@A:MAT D=A/B^2\n50 END");
    for (cell, expected) in [
        ("C(0)", 36.0),
        ("C(1)", 48.0),
        ("C(2)", 48.0),
        ("D(0)", 4.0),
        ("D(1)", 3.0),
        ("D(2)", 2.0),
    ] {
        assert_eq!(number(&mut interpreter, cell), expected, "{cell}");
    }
}

#[test]
fn hadamard_composes_at_multiplicative_precedence_in_loaded_mat() {
    let mut interpreter = run("10 DIM A(1),B(1),T(1):MAT A=2:MAT B=3:MAT T=7\n\
         20 MAT C=T+A @ B^2\n\
         30 MAT D=A*2 @ B\n40 END");
    assert_eq!(number(&mut interpreter, "C(0)"), 25.0);
    assert_eq!(number(&mut interpreter, "D(0)"), 12.0);
    assert_eq!(
        eval_expression(&mut interpreter, "A(0) @ B(0)")
            .unwrap_err()
            .code,
        ErrorCode::Syntax
    );
    for alias in ["HAD", "HADAMARD"] {
        assert_eq!(
            interpreter
                .process_immediate(&format!("MAT C=A {alias} B"))
                .unwrap_err()
                .code,
            ErrorCode::Syntax,
            "{alias}"
        );
    }
    assert_eq!(number(&mut interpreter, "C(0)"), 25.0);
}

#[test]
fn former_operator_spellings_are_ordinary_identifiers() {
    for name in ["HAD", "HADAMARD"] {
        let mut interpreter = Interpreter::new();
        command(
            &mut interpreter,
            &format!(
                "{name}=7:{name}$=\"ok\":DIM {name}(1),B(1):MAT {name}=3:MAT B=2:MAT C={name}@B"
            ),
        );
        assert_eq!(number(&mut interpreter, name), 7.0);
        assert_eq!(number(&mut interpreter, &format!("{name}(0)")), 3.0);
        assert_eq!(number(&mut interpreter, "C(0)"), 6.0);
        assert_eq!(
            eval_expression(&mut interpreter, &format!("{name}$")).unwrap(),
            avl_basic::Value::string("ok")
        );
    }
}

#[test]
fn hadamard_symbol_uses_multiplicative_precedence_and_left_associativity() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(1),B(1),C(1):MAT A=2:MAT B=3:MAT C=5",
    );
    for (expression, expected) in [
        ("A@B", 6.0),
        ("A*2@B", 12.0),
        ("A@B*2", 12.0),
        ("A/2@B", 3.0),
        ("A@B/2", 3.0),
        ("5 MOD 3*A@B", 12.0),
        ("A@B^2", 18.0),
        ("(A@B)^2", 36.0),
        ("-A^2@B", -12.0),
        ("A@-B^2", -18.0),
        ("A@B@C", 30.0),
    ] {
        command(&mut interpreter, &format!("MAT D={expression}"));
        assert_eq!(number(&mut interpreter, "D(0)"), expected, "{expression}");
        assert_eq!(number(&mut interpreter, "D(1)"), expected, "{expression}");
    }
    command(&mut interpreter, "MAT A=1E308:MAT B=1E-308:MAT C=1E-308");
    command(&mut interpreter, "MAT D=A@B@C:MAT E=A@(B@C)");
    assert!(number(&mut interpreter, "D(0)") > 0.0);
    assert_eq!(number(&mut interpreter, "E(0)"), 0.0);

    for expression in ["2@3", "A(0)@B(0)"] {
        assert_eq!(
            eval_expression(&mut interpreter, expression)
                .unwrap_err()
                .code,
            ErrorCode::Syntax,
            "{expression}"
        );
    }
    assert_eq!(
        interpreter.process_immediate("MAT D=2@3").unwrap_err().code,
        ErrorCode::ForbiddenExpression
    );
}

#[test]
fn hadamard_symbol_preserves_at_signs_in_strings_data_and_comments() {
    let mut interpreter = run(
        r#"10 PRINT "user@example.org";" C:\tmp\a@b.bas" ' @ remains a comment
20 REM MAT X=A@B@
30 DATA "x@y",@
40 READ S$,T$
50 DIM A(0),B(0):MAT A=2:MAT B=3
60 MAT C=A@B ' @ remains a comment
70 PRINT S$;" ";T$
80 END"#,
    );
    assert_eq!(
        interpreter.take_output(),
        "user@example.org C:\\tmp\\a@b.bas\nx@y @\n"
    );
    assert_eq!(number(&mut interpreter, "C(0)"), 6.0);
}

#[test]
fn hadamard_rejects_invalid_operands_without_replacing_destination() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "DIM A(1),B(2),M(0,1),T(1,1,1),S$(1),C(1):MAT A=4:MAT C=77",
    );
    for (expression, code) in [
        ("A @ B", ErrorCode::InvalidDimensions),
        ("A @ M", ErrorCode::InvalidDimensions),
        ("T @ T", ErrorCode::InvalidDimensions),
        ("A @ S$", ErrorCode::ForbiddenExpression),
        ("S$ @ A", ErrorCode::ForbiddenExpression),
        ("A @ A(0)", ErrorCode::ForbiddenExpression),
        ("2 @ A", ErrorCode::ForbiddenExpression),
    ] {
        let error = interpreter
            .process_immediate(&format!("MAT C={expression}"))
            .unwrap_err();
        assert_eq!(error.code, code, "{expression}");
        if code == ErrorCode::InvalidDimensions {
            assert_eq!(error.display_for_basic(), "Invalid array dimensions.");
        }
        assert_eq!(number(&mut interpreter, "C(0)"), 77.0);
        assert_eq!(number(&mut interpreter, "A(0)"), 4.0);
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

#[test]
fn mat_map_composes_with_mat_operators_and_can_replace_its_input() {
    let mut interpreter = run("10 MAT BASE 0:DIM A(2),B(2):A=17:MAT B=2\n\
         20 A(0)=.25:A(1)=-.25:A(2)=.375\n\
         30 MAT A=MAP(aBs,MAP(INT,A*10)) @ B\n\
         40 MAT C=INT(A*10)\n50 END");
    for (index, expected) in [4.0, 6.0, 6.0].into_iter().enumerate() {
        assert_eq!(number(&mut interpreter, &format!("A({index})")), expected);
    }
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 2.0);
    assert_eq!(
        eval_expression(&mut interpreter, "UBOUND(A,2)")
            .unwrap_err()
            .code,
        ErrorCode::IndexOutOfRange
    );
    assert_eq!(number(&mut interpreter, "A"), 17.0);
    assert_eq!(number(&mut interpreter, "C(0)"), 170.0);
}

#[test]
fn mat_map_preserves_base_one_border_of_the_evaluated_argument() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "MAT BASE 1:DIM A(2,1),V(1),E(0,1):A(0,1)=-2:A(1,0)=-3:A(2,0)=-4:\
         A(1,1)=.5:A(2,1)=1:V(0)=-2:V(1)=.5:E(0,1)=-7",
    );
    command(
        &mut interpreter,
        "MAT C=MAP(LOG,A*2):MAT W=MAP(LOG,V*2):MAT D=MAP(LOG,E)",
    );
    let log_two = number(&mut interpreter, "LOG(2)");
    for (cell, expected) in [
        ("UBOUND(C,2)", 1.0),
        ("C(0,0)", 0.0),
        ("C(0,1)", -4.0),
        ("C(1,0)", -6.0),
        ("C(2,0)", -8.0),
        ("C(1,1)", 0.0),
        ("C(2,1)", log_two),
        ("W(0)", -4.0),
        ("W(1)", 0.0),
        ("D(0,0)", 0.0),
        ("D(0,1)", -7.0),
        ("A(0,1)", -2.0),
    ] {
        assert_eq!(number(&mut interpreter, cell), expected, "{cell}");
    }
}

#[test]
fn mat_map_matches_pure_unary_scalar_functions_in_both_angle_modes() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 0:DIM A(1):A(0)=.5:A(1)=.75");
    for mode in ["DEG", "RAD"] {
        command(&mut interpreter, mode);
        for function in [
            "ABS", "INT", "SQR", "SIN", "COS", "FIX", "SGN", "FRAC", "LOG", "LOG10", "EXP", "TAN",
            "ASN", "ACS", "ATN", "COT", "RTD", "DTR", "ROUND",
        ] {
            command(&mut interpreter, &format!("MAT C=MAP({function},A)"));
            for index in 0..=1 {
                let expected = number(&mut interpreter, &format!("{function}(A({index}))"));
                assert_eq!(
                    number(&mut interpreter, &format!("C({index})")).to_bits(),
                    expected.to_bits(),
                    "{mode} {function} at index {index}"
                );
            }
        }
    }
}

#[test]
fn mat_map_evaluates_its_array_expression_once() {
    let mut interpreter = run("10 MAT BASE 0:DIM A(3):MAT A=2.75:CALLS=0\n\
         20 MAT C=MAP(INT,A+FNONCE())\n30 END\n\
         100 DEF FNONCE()\n110 CALLS=CALLS+1:FNONCE=.5\n120 FNEND");
    assert_eq!(number(&mut interpreter, "CALLS"), 1.0);
    for index in 0..=3 {
        assert_eq!(number(&mut interpreter, &format!("C({index})")), 3.0);
    }
}

#[test]
fn mat_map_domain_error_leaves_the_original_array_intact() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "MAT BASE 0:DIM A(2):A(0)=2:A(1)=3:A(2)=0");
    let expected = eval_expression(&mut interpreter, "LOG(A(2))")
        .unwrap_err()
        .code;
    let error = interpreter
        .process_immediate("MAT A=MAP(LOG,A)")
        .unwrap_err();
    assert_eq!(error.code, expected);
    for (index, original) in [2.0, 3.0, 0.0].into_iter().enumerate() {
        assert_eq!(number(&mut interpreter, &format!("A({index})")), original);
    }
}

#[test]
fn mat_map_rejects_invalid_selectors_arity_and_operands() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "MAT BASE 0:DIM A(1),C(1),T(1,1,1),S$(1):MAT A=5:MAT C=77",
    );
    for (expression, code) in [
        ("MAP()", ErrorCode::ArgumentMismatch),
        ("MAP(INT)", ErrorCode::ArgumentMismatch),
        ("MAP(INT,A,1)", ErrorCode::ArgumentMismatch),
        ("MAP(\"INT\",A)", ErrorCode::InvalidArgument),
        ("MAP(INT(1),A)", ErrorCode::InvalidArgument),
        ("MAP(RND,A)", ErrorCode::InvalidArgument),
        ("MAP(SUM,A)", ErrorCode::InvalidArgument),
        ("MAP(INT,A(0))", ErrorCode::ForbiddenExpression),
        ("MAP(INT,S$)", ErrorCode::ForbiddenExpression),
        ("MAP(INT,T)", ErrorCode::InvalidDimensions),
    ] {
        let error = interpreter
            .process_immediate(&format!("MAT C={expression}"))
            .unwrap_err();
        assert_eq!(error.code, code, "{expression}");
        assert_eq!(number(&mut interpreter, "C(0)"), 77.0);
        assert_eq!(number(&mut interpreter, "C(1)"), 77.0);
    }
}

#[test]
fn mat_map_preserves_existing_map_identifiers_outside_the_new_call_form() {
    let mut interpreter = Interpreter::new();
    command(
        &mut interpreter,
        "MAT BASE 0:MAP=5:DIM MAP(1):MAP(0)=2.5:MAP(1)=3.5:MAT C=MAP:MAT D=MAP(INT,MAP)",
    );
    assert_eq!(number(&mut interpreter, "MAP"), 5.0);
    assert_eq!(number(&mut interpreter, "MAP(0)+MAP(1)"), 6.0);
    assert_eq!(number(&mut interpreter, "C(0)"), 2.5);
    assert_eq!(number(&mut interpreter, "D(0)"), 2.0);
    assert_eq!(number(&mut interpreter, "D(1)"), 3.0);
    command(&mut interpreter, "MAT C=INT(MAP(1))");
    assert_eq!(number(&mut interpreter, "C(0)"), 3.0);
    assert_eq!(
        interpreter
            .process_immediate("MAT C=MAP()")
            .unwrap_err()
            .code,
        ErrorCode::ArgumentMismatch
    );
}
