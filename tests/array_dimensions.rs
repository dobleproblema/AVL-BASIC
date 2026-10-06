use avl_basic::expr::eval_expression;
use avl_basic::{ErrorCode, Interpreter};

fn number(interpreter: &mut Interpreter, expression: &str) -> f64 {
    eval_expression(interpreter, expression)
        .unwrap()
        .as_number()
        .unwrap()
}

fn text(interpreter: &mut Interpreter, expression: &str) -> String {
    eval_expression(interpreter, expression)
        .unwrap()
        .into_string()
        .unwrap()
}

#[test]
fn invalid_bounds_report_basic_errors_without_replacing_existing_arrays() {
    for statement in ["DIM", "REDIM"] {
        for (bound, expected) in [
            ("INF", ErrorCode::Overflow),
            ("1E309", ErrorCode::Overflow),
            ("2147483648", ErrorCode::Overflow),
            ("1E100", ErrorCode::Overflow),
            ("-1", ErrorCode::InvalidValue),
            ("-INF", ErrorCode::InvalidValue),
            ("SOURCE(0)", ErrorCode::InvalidValue),
        ] {
            let mut interpreter = Interpreter::new();
            interpreter
                .process_immediate(
                    "DIM A(1),A$(1),SOURCE(0):A(0)=11:A(1)=22:\
                     A$(0)=\"first\":A$(1)=\"last\":\
                     MAT SOURCE=INF:MAT SOURCE=SOURCE-SOURCE",
                )
                .unwrap();
            for array in ["A", "A$"] {
                let source = format!("{statement} {array}({bound})");
                let error = interpreter.process_immediate(&source).unwrap_err();
                assert_eq!(error.code, expected, "{source}");
                assert_eq!(
                    number(&mut interpreter, &format!("UBOUND({array})")),
                    1.0,
                    "{source}"
                );
            }
            assert_eq!(number(&mut interpreter, "A(0)"), 11.0);
            assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
            assert_eq!(text(&mut interpreter, "A$(0)"), "first");
            assert_eq!(text(&mut interpreter, "A$(1)"), "last");
        }
    }
}

#[test]
fn invalid_new_declarations_do_not_create_an_array() {
    for array in ["A", "A$"] {
        let mut interpreter = Interpreter::new();
        let error = interpreter
            .process_immediate(&format!("DIM {array}(INF)"))
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Overflow);
        let error = eval_expression(&mut interpreter, &format!("UBOUND({array})")).unwrap_err();
        assert_eq!(error.code, ErrorCode::Undefined);
    }
}

#[test]
fn checked_total_sizes_reject_product_and_byte_capacity_overflow_before_allocation() {
    // Each bound fits i32. The combined cell count or allocation byte size
    // cannot fit the platform, so these cases never request a huge allocation.
    for (small, impossible, first_cell, last_cell) in [
        ("1,1", "2147483647,2147483647", "0,0", "1,1"),
        ("1,1", "1073741823,1073741823", "0,0", "1,1"),
        (
            "1,1,1",
            "2147483647,2147483647,2147483647",
            "0,0,0",
            "1,1,1",
        ),
    ] {
        for statement in ["DIM", "REDIM"] {
            let mut interpreter = Interpreter::new();
            interpreter
                .process_immediate(&format!(
                    "DIM A({small}),A$({small}):\
                     A({first_cell})=11:A({last_cell})=22:\
                     A$({first_cell})=\"first\":A$({last_cell})=\"last\""
                ))
                .unwrap();
            for array in ["A", "A$"] {
                let source = format!("{statement} {array}({impossible})");
                let error = interpreter.process_immediate(&source).unwrap_err();
                assert_eq!(error.code, ErrorCode::Overflow, "{source}");
                for dimension in 1..=small.split(',').count() {
                    assert_eq!(
                        number(&mut interpreter, &format!("UBOUND({array},{dimension})")),
                        1.0,
                        "{source}"
                    );
                }
            }
            assert_eq!(number(&mut interpreter, &format!("A({first_cell})")), 11.0);
            assert_eq!(number(&mut interpreter, &format!("A({last_cell})")), 22.0);
            assert_eq!(
                text(&mut interpreter, &format!("A$({first_cell})")),
                "first"
            );
            assert_eq!(text(&mut interpreter, &format!("A$({last_cell})")), "last");
        }
    }
}

#[test]
fn valid_bounds_keep_truncation_zero_based_storage_and_redim_intersection() {
    let mut interpreter = Interpreter::new();
    interpreter
        .process_immediate("DIM A(1.9),A$(1.9),Z(-0):A(0)=11:A(1)=22:A$(1)=\"last\"")
        .unwrap();
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 1.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A$)"), 1.0);
    assert_eq!(number(&mut interpreter, "UBOUND(Z)"), 0.0);
    interpreter
        .process_immediate("REDIM A(3.9),A$(3.9)")
        .unwrap();
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 3.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 11.0);
    assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
    assert_eq!(number(&mut interpreter, "A(3)"), 0.0);
    assert_eq!(text(&mut interpreter, "A$(1)"), "last");
    assert_eq!(text(&mut interpreter, "A$(3)"), "");
    interpreter.process_immediate("REDIM A(0.9)").unwrap();
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 0.0);
    assert_eq!(number(&mut interpreter, "A(0)"), 11.0);
}

#[test]
fn bounds_evaluate_once_in_order_and_stop_before_later_effects_on_error() {
    for statement in ["DIM", "REDIM"] {
        for (bounds, expected_calls) in [("INF,FNTICK()", 0.0), ("FNTICK(),INF,FNTICK()", 1.0)] {
            let mut interpreter = Interpreter::new();
            interpreter
                .program
                .load_text(&format!(
                    "10 DIM A(1,1):A(0,0)=11:CALLS=0\n\
                     20 {statement} A({bounds})\n30 END\n\
                     100 DEF FNTICK()\n110 CALLS=CALLS+1\n120 FNTICK=2\n130 FNEND"
                ))
                .unwrap();
            let error = interpreter.run_loaded().unwrap_err();
            assert_eq!(error.code, ErrorCode::Overflow, "{statement} A({bounds})");
            assert_eq!(error.line, Some(20));
            assert_eq!(number(&mut interpreter, "CALLS"), expected_calls);
            assert_eq!(number(&mut interpreter, "A(0,0)"), 11.0);
            assert_eq!(number(&mut interpreter, "UBOUND(A,1)"), 1.0);
            assert_eq!(number(&mut interpreter, "UBOUND(A,2)"), 1.0);
        }
    }
}

#[test]
fn local_dimension_errors_restore_global_arrays_and_parameter_aliases() {
    for (bound, expected) in [
        ("INF", ErrorCode::Overflow),
        ("2147483648", ErrorCode::Overflow),
        ("1E100", ErrorCode::Overflow),
        ("SOURCE(0)", ErrorCode::InvalidValue),
        ("-1", ErrorCode::InvalidValue),
    ] {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(&format!(
                "10 DIM A(1),B(1),SOURCE(0):A(0)=11:A(1)=22:B(0)=33:\
                 MAT SOURCE=INF:MAT SOURCE=SOURCE-SOURCE\n\
                 20 CALL BAD(B)\n30 END\n\
                 100 DEF SUB BAD(P)\n110 LOCAL A({bound})\n120 SUBEND\n\
                 200 DEF SUB GOOD(P)\n210 P(0)=44\n220 SUBEND"
            ))
            .unwrap();
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(error.code, expected, "LOCAL A({bound})");
        assert_eq!(number(&mut interpreter, "A(0)"), 11.0);
        assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
        assert_eq!(number(&mut interpreter, "B(0)"), 33.0);
        interpreter.process_immediate("CALL GOOD(A)").unwrap();
        assert_eq!(number(&mut interpreter, "A(0)"), 44.0);
        assert_eq!(number(&mut interpreter, "B(0)"), 33.0);
    }
}

#[test]
fn valid_local_dimensions_preserve_fractional_truncation_and_initial_values() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 DIM A(3),A$(3),Z(2):A(1)=11:A$(1)=\"global\":Z(0)=33\n\
             20 CALL WORK\n30 END\n\
             100 DEF SUB WORK\n110 LOCAL A(1.9),A$(1.9),Z(-0)\n\
             120 SEEN=UBOUND(A):ZBOUND=UBOUND(Z):INIT=A(0)+A(1):INIT$=A$(0)\n\
             130 A(1)=99:A$(1)=\"local\":Z(0)=44\n140 SUBEND",
        )
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(number(&mut interpreter, "SEEN"), 1.0);
    assert_eq!(number(&mut interpreter, "ZBOUND"), 0.0);
    assert_eq!(number(&mut interpreter, "INIT"), 0.0);
    assert_eq!(text(&mut interpreter, "INIT$"), "");
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 3.0);
    assert_eq!(number(&mut interpreter, "UBOUND(A$)"), 3.0);
    assert_eq!(number(&mut interpreter, "UBOUND(Z)"), 2.0);
    assert_eq!(number(&mut interpreter, "A(1)"), 11.0);
    assert_eq!(text(&mut interpreter, "A$(1)"), "global");
    assert_eq!(number(&mut interpreter, "Z(0)"), 33.0);
}

#[test]
fn local_total_size_overflow_fails_before_replacing_a_global_array() {
    for array in ["A", "A$"] {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(&format!(
                "10 DIM A(1),A$(1):A(1)=22:A$(1)=\"last\"\n20 CALL BAD\n30 END\n\
                 100 DEF SUB BAD\n110 LOCAL {array}(2147483647,2147483647,2147483647)\n\
                 120 SUBEND"
            ))
            .unwrap();
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(error.code, ErrorCode::Overflow, "LOCAL {array}");
        assert_eq!(number(&mut interpreter, "UBOUND(A)"), 1.0);
        assert_eq!(number(&mut interpreter, "UBOUND(A$)"), 1.0);
        assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
        assert_eq!(text(&mut interpreter, "A$(1)"), "last");
    }
}

#[test]
fn local_dimensions_stop_evaluation_after_an_invalid_bound() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(
            "10 CALLS=0:CALL BAD\n20 END\n\
             100 DEF SUB BAD\n110 LOCAL A(FNTICK(),INF,FNTICK())\n120 SUBEND\n\
             200 DEF FNTICK()\n210 CALLS=CALLS+1\n220 FNTICK=2\n230 FNEND",
        )
        .unwrap();
    assert_eq!(
        interpreter.run_loaded().unwrap_err().code,
        ErrorCode::Overflow
    );
    assert_eq!(number(&mut interpreter, "CALLS"), 1.0);
}

fn nineteen_zero_indexes() -> String {
    vec!["0"; 19].join(",")
}

fn assert_array_is_undefined(interpreter: &mut Interpreter, name: &str) {
    assert_eq!(
        eval_expression(interpreter, &format!("UBOUND({name})"))
            .unwrap_err()
            .code,
        ErrorCode::Undefined,
        "{name} must remain undefined after a failed implicit allocation"
    );
}

#[test]
fn implicit_array_creation_rejects_capacity_overflow_in_raw_and_cached_paths() {
    let indexes = nineteen_zero_indexes();
    for (source, array) in [
        (format!("X=B({indexes})"), "B"),
        (format!("X$=B$({indexes})"), "B$"),
        (format!("B({indexes})=1"), "B"),
        (format!("B$({indexes})=\"text\""), "B$"),
    ] {
        for cached in [false, true] {
            let mut interpreter = Interpreter::new();
            let error = if cached {
                interpreter
                    .program
                    .load_text(&format!("10 {source}\n20 END"))
                    .unwrap();
                interpreter.run_loaded().unwrap_err()
            } else {
                interpreter.process_immediate(&source).unwrap_err()
            };
            assert_eq!(error.code, ErrorCode::Overflow, "{source}; cached={cached}");
            assert_array_is_undefined(&mut interpreter, array);
        }
    }
}

#[test]
fn implicit_array_creation_in_dimension_expressions_preserves_previous_arrays() {
    let indexes = nineteen_zero_indexes();
    for statement in ["DIM", "REDIM"] {
        for cached in [false, true] {
            let mut interpreter = Interpreter::new();
            let source = format!("{statement} A(B({indexes}))");
            let error = if cached {
                interpreter
                    .program
                    .load_text(&format!("10 DIM A(1):A(1)=22\n20 {source}\n30 END"))
                    .unwrap();
                interpreter.run_loaded().unwrap_err()
            } else {
                interpreter.process_immediate("DIM A(1):A(1)=22").unwrap();
                interpreter.process_immediate(&source).unwrap_err()
            };
            assert_eq!(error.code, ErrorCode::Overflow, "{source}; cached={cached}");
            assert_eq!(number(&mut interpreter, "UBOUND(A)"), 1.0);
            assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
            assert_array_is_undefined(&mut interpreter, "B");
        }
    }
}

#[test]
fn implicit_array_creation_in_local_bounds_preserves_globals_and_aliases() {
    let indexes = nineteen_zero_indexes();
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text(&format!(
            "10 DIM A(1):A(1)=22\n20 CALL BAD(A)\n30 END\n\
             100 DEF SUB BAD(P)\n110 LOCAL A(B({indexes}))\n120 SUBEND\n\
             200 DEF SUB GOOD(P)\n210 P(1)=44\n220 SUBEND"
        ))
        .unwrap();
    assert_eq!(
        interpreter.run_loaded().unwrap_err().code,
        ErrorCode::Overflow
    );
    assert_array_is_undefined(&mut interpreter, "B");
    assert_eq!(number(&mut interpreter, "UBOUND(A)"), 1.0);
    assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
    interpreter.process_immediate("CALL GOOD(A)").unwrap();
    assert_eq!(number(&mut interpreter, "A(1)"), 44.0);
}

#[test]
fn implicit_array_creation_with_an_active_array_alias_rejects_capacity_overflow() {
    let indexes = nineteen_zero_indexes();
    for (source, array) in [
        (format!("X=B({indexes})"), "B"),
        (format!("X$=B$({indexes})"), "B$"),
        (format!("B({indexes})=1"), "B"),
        (format!("B$({indexes})=\"text\""), "B$"),
    ] {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(&format!(
                "10 DIM A(1):A(1)=22\n20 CALL BAD(A)\n30 END\n\
                 100 DEF SUB BAD(P)\n110 {source}\n120 SUBEND"
            ))
            .unwrap();
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::Overflow,
            "{source} with an active alias"
        );
        assert_array_is_undefined(&mut interpreter, array);
        assert_eq!(number(&mut interpreter, "A(1)"), 22.0);
    }
}
