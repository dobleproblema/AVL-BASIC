use avl_basic::expr::{compile_expression, eval_compiled_number, eval_expression};
use avl_basic::{ErrorCode, Interpreter, Value};

fn run_program(source: &str) -> String {
    let mut interpreter = Interpreter::new();
    for line in source.lines() {
        interpreter.process_immediate(line).unwrap();
    }
    interpreter.run_loaded().unwrap();
    interpreter.take_output()
}

#[test]
fn skips_runtime_errors_in_both_branches_and_both_evaluators() {
    for source in [
        "IIF(1, 42, 1/0)",
        "IIF(0, 1/0, 42)",
        "IIF(-.25, 42, SQR(-1))",
        "IIF(.25, 42, FNUNDEFINED())",
        "IIF(0, UNDEFINED(999), 42)",
    ] {
        let mut interpreter = Interpreter::new();
        assert_eq!(
            eval_expression(&mut interpreter, source).unwrap(),
            Value::number(42.0),
            "{source}"
        );
        let expression = compile_expression(source).unwrap();
        assert_eq!(
            eval_compiled_number(&mut interpreter, &expression).unwrap(),
            42.0,
            "{source}"
        );
    }
}

#[test]
fn selected_branch_and_condition_still_report_their_errors() {
    for source in ["IIF(1, 1/0, 42)", "IIF(0, 42, 1/0)", "IIF(1/0, 42, 43)"] {
        let mut interpreter = Interpreter::new();
        assert_eq!(
            eval_expression(&mut interpreter, source).unwrap_err().code,
            ErrorCode::DivisionByZero,
            "{source}"
        );
        assert_eq!(
            eval_compiled_number(&mut interpreter, &compile_expression(source).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::DivisionByZero,
            "{source}"
        );
    }
    let mut interpreter = Interpreter::new();
    assert_eq!(
        eval_expression(&mut interpreter, "IIF(\"yes\", 1, 2)")
            .unwrap_err()
            .code,
        ErrorCode::TypeMismatch
    );
}

#[test]
fn returns_selected_type_with_mixed_branches_and_nested_calls() {
    let cases = [
        ("IIF(1, \"yes\", 1/0)", Value::string("yes")),
        ("IIF(0, 1/0, \"no\")", Value::string("no")),
        ("IIF(1, 42, \"no\")", Value::number(42.0)),
        ("IIF(0, \"yes\", 42)", Value::number(42.0)),
        (
            "\"[\" + IIF(1, \"yes\", 42) + \"]\"",
            Value::string("[yes]"),
        ),
        (
            "IIF(0, 1/0, IIF(1, \"nested\", 1/0))",
            Value::string("nested"),
        ),
        (
            "IIF(IIF(0, 1/0, -.25), IIF(0, 1/0, 42), 1/0)",
            Value::number(42.0),
        ),
        ("iif(1, 42, 1/0)", Value::number(42.0)),
    ];
    let mut interpreter = Interpreter::new();
    for (source, expected) in cases {
        assert_eq!(
            eval_expression(&mut interpreter, source).unwrap(),
            expected,
            "{source}"
        );
    }
    assert_eq!(
        eval_compiled_number(
            &mut interpreter,
            &compile_expression("IIF(0, \"unused\", 42)").unwrap()
        )
        .unwrap(),
        42.0
    );
    assert_eq!(
        eval_compiled_number(
            &mut interpreter,
            &compile_expression("IIF(1, \"selected\", 42)").unwrap()
        )
        .unwrap_err()
        .code,
        ErrorCode::TypeMismatch
    );
}

#[test]
fn requires_exactly_three_arguments_before_evaluating_any() {
    assert_eq!(
        compile_expression("IIF").unwrap_err().code,
        ErrorCode::ArgumentMismatch
    );
    for source in ["IIF()", "IIF(1)", "IIF(1/0, 42)", "IIF(1/0, 42, 43, 44)"] {
        let mut interpreter = Interpreter::new();
        assert_eq!(
            eval_expression(&mut interpreter, source).unwrap_err().code,
            ErrorCode::ArgumentMismatch,
            "{source}"
        );
        assert_eq!(
            eval_compiled_number(&mut interpreter, &compile_expression(source).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::ArgumentMismatch,
            "{source}"
        );
    }
}

#[test]
fn requires_nonempty_arguments_and_valid_syntax_even_in_skipped_branches() {
    for source in [
        "IIF(, 1, 2)",
        "IIF(1, , 2)",
        "IIF(1, 2, )",
        "IIF(1, 2, 3,)",
        "IIF(1, 42, 1+)",
        "IIF(0, 1+, 42)",
        "IIF(1, 42, IIF(0, 1, 2+))",
    ] {
        let mut interpreter = Interpreter::new();
        assert_eq!(
            eval_expression(&mut interpreter, source).unwrap_err().code,
            ErrorCode::Syntax,
            "{source}"
        );
    }
}

#[test]
fn evaluates_condition_once_and_calls_only_selected_user_functions() {
    let source = r#"10 C=0:T=0:F=0
20 A=IIF(FNCOND, FNTRUE, FNFALSE)
30 PRINT IIF(FNCOND, FNFALSE, FNTRUE)
40 PRINT C;T;F;A
50 END
100 DEF FNCOND
110 C=C+1
120 FNCOND=C
130 FNEND
200 DEF FNTRUE
210 T=T+1
220 FNTRUE=7
230 FNEND
300 DEF FNFALSE
310 F=F+1
320 FNFALSE=9
330 FNEND"#;
    assert_eq!(run_program(source), " 9\n 2  1  1  7\n");
}

#[test]
fn skipped_random_calls_do_not_consume_random_values() {
    let mut interpreter = Interpreter::new();
    let mut reference = Interpreter::new();
    interpreter.process_immediate("RANDOMIZE 42").unwrap();
    reference.process_immediate("RANDOMIZE 42").unwrap();
    eval_expression(&mut interpreter, "IIF(1, 7, RND)").unwrap();
    eval_compiled_number(
        &mut interpreter,
        &compile_expression("IIF(0, RND, 7)").unwrap(),
    )
    .unwrap();
    assert_eq!(
        eval_expression(&mut interpreter, "RND").unwrap(),
        eval_expression(&mut reference, "RND").unwrap()
    );
}

#[test]
fn works_in_numeric_loop_bounds_arrays_and_string_assignments() {
    let source = r#"10 DIM A(2)
20 FOR I=0 TO IIF(1, 2, 1/0)
30 A(I)=IIF(I=0, 10, 20/I)
40 NEXT I
50 N=0
60 WHILE IIF(N<2, 1, 0)
70 N=N+1
80 WEND
90 S$=IIF(0, 1/0, "done")
100 PRINT A(0);A(1);A(2);N;S$
110 END"#;
    assert_eq!(run_program(source), " 10  20  10  2 done\n");
}

#[test]
fn rejects_selected_arrays_and_skips_unselected_array_functions() {
    let routines = r#"100 DEF FNCOPY(X)
110 C=C+1
120 MAT FNCOPY=X
130 FNEND
200 DEF SUB TAKE(X)
210 C=C+100
220 SUBEND"#;
    let skipped = format!(
        "10 DIM A(1):A(0)=4:C=0\n20 PRINT IIF(1, \"safe\", FNCOPY(A))\n30 N=IIF(0, FNCOPY(A), 42)\n40 PRINT N;C\n50 END\n{routines}"
    );
    assert_eq!(run_program(&skipped), "safe\n 42  0\n");
    for command in [
        "CALL TAKE(IIF(1, FNCOPY(A), 42))",
        "N=IIF(0, 42, FNCOPY(A))",
    ] {
        let mut interpreter = Interpreter::new();
        let source = format!("10 DIM A(1):A(0)=4:C=0\n20 {command}\n30 END\n{routines}");
        for line in source.lines() {
            interpreter.process_immediate(line).unwrap();
        }
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::TypeMismatch,
            "{command}"
        );
        assert_eq!(
            eval_expression(&mut interpreter, "C").unwrap(),
            Value::number(1.0)
        );
    }
}

#[test]
fn help_and_identifier_rules_recognize_iif() {
    let mut interpreter = Interpreter::new();
    interpreter.process_immediate("HELP IIF").unwrap();
    let help = interpreter.take_output();
    assert!(help.contains("IIF(condition, when_true, when_false)"));
    assert!(help.contains("not evaluated"));
    assert!(interpreter.process_immediate("IIF=1").is_err());
    assert!(interpreter.process_immediate("DIM IIF(3)").is_err());
}
