use avl_basic::{ErrorCode, Interpreter};

fn prepare_call(
    setup: &str,
    call: &str,
    definitions: &str,
    immediate: bool,
) -> (Interpreter, String) {
    let mut interpreter = Interpreter::new();
    interpreter.process_immediate("ZONE 8").unwrap();
    let source = format!("10 {setup}\n20 {call}\n30 END\n{definitions}");
    for line in source.lines() {
        interpreter.process_immediate(line).unwrap();
    }
    let command = if immediate {
        interpreter.process_immediate(setup).unwrap();
        call.to_string()
    } else {
        "RUN".to_string()
    };
    (interpreter, command)
}

fn run_call(setup: &str, call: &str, definitions: &str, immediate: bool) -> Interpreter {
    let (mut interpreter, command) = prepare_call(setup, call, definitions, immediate);
    interpreter.process_immediate(&command).unwrap();
    interpreter
}

#[test]
fn numeric_whole_arrays_keep_reference_through_redundant_parentheses() {
    for immediate in [false, true] {
        for argument in ["source", "(source)", "((source))", "(((source)))"] {
            let mut interpreter = run_call(
                "DIM source(1),values(1):source(0)=7:source(1)=9:values(0)=40:values(1)=41",
                &format!("CALL Touch({argument})"),
                "100 DEF SUB Touch(values)\n110 values(0)=values(0)+1:PRINT values(0)\n120 SUBEND",
                immediate,
            );
            assert_eq!(interpreter.take_output(), " 8\n", "{argument}");
            interpreter
                .process_immediate("PRINT source(0);source(1);values(0);values(1)")
                .unwrap();
            assert_eq!(interpreter.take_output(), " 8  9  40  41\n", "{argument}");
        }
    }
}

#[test]
fn string_whole_arrays_keep_reference_through_redundant_parentheses() {
    for immediate in [false, true] {
        for argument in ["source$", "(source$)", "((source$))", "(((source$)))"] {
            let mut interpreter = run_call(
                "DIM source$(1),values$(0):source$(0)=\"array\":source$(1)=\"stable\":values$(0)=\"outside\"",
                &format!("CALL Touch({argument})"),
                "100 DEF SUB Touch(values$)\n110 values$(0)=values$(0)+\"!\":PRINT values$(0)\n120 SUBEND",
                immediate,
            );
            assert_eq!(interpreter.take_output(), "array!\n", "{argument}");
            interpreter
                .process_immediate("PRINT source$(0);\"|\";source$(1);\"|\";values$(0)")
                .unwrap();
            assert_eq!(
                interpreter.take_output(),
                "array!|stable|outside\n",
                "{argument}"
            );
        }
    }
}

#[test]
fn homonymous_scalars_still_take_priority_and_pass_by_value() {
    for immediate in [false, true] {
        for string_value in [false, true] {
            let (setup, name, definitions, caller_check, expected, restored) = if string_value {
                (
                    "DIM source$(0):source$(0)=\"array\":source$=\"scalar\"",
                    "source$",
                    "100 DEF SUB Touch(value$)\n110 value$=value$+\"!\":PRINT value$\n120 SUBEND",
                    "PRINT source$;\"|\";source$(0)",
                    "scalar!\n",
                    "scalar|array\n",
                )
            } else {
                (
                    "DIM source(0):source(0)=100:source=7",
                    "source",
                    "100 DEF SUB Touch(value)\n110 value=value+1:PRINT value\n120 SUBEND",
                    "PRINT source;source(0)",
                    " 8\n",
                    " 7  100\n",
                )
            };
            for argument in [name.to_string(), format!("({name})"), format!("(({name}))")] {
                let mut interpreter = run_call(
                    setup,
                    &format!("CALL Touch({argument})"),
                    definitions,
                    immediate,
                );
                assert_eq!(interpreter.take_output(), expected, "{argument}");
                interpreter.process_immediate(caller_check).unwrap();
                assert_eq!(interpreter.take_output(), restored, "{argument}");
            }
        }
    }
}

#[test]
fn array_elements_and_computed_arguments_remain_scalar_values() {
    for immediate in [false, true] {
        for (argument, expected) in [
            ("source(0)", " 8\n"),
            ("(source(0))", " 8\n"),
            ("((source(0)))", " 8\n"),
            ("source(0)+1", " 9\n"),
            ("(source(0)+1)", " 9\n"),
            ("source+0", " 1\n"),
            ("(source+0)", " 1\n"),
        ] {
            let mut interpreter = run_call(
                "DIM source(0):source(0)=7",
                &format!("CALL Touch({argument})"),
                "100 DEF SUB Touch(value)\n110 value=value+1:PRINT value\n120 SUBEND",
                immediate,
            );
            assert_eq!(interpreter.take_output(), expected, "{argument}");
            interpreter.process_immediate("PRINT source(0)").unwrap();
            assert_eq!(interpreter.take_output(), " 7\n", "{argument}");
        }
        for (argument, expected) in [
            ("source$(0)", "array!\n"),
            ("(source$(0))", "array!\n"),
            ("((source$(0)))", "array!\n"),
            ("source$(0)+\"?\"", "array?!\n"),
            ("source$+\"\"", "!\n"),
            ("(source$+\"\")", "!\n"),
        ] {
            let mut interpreter = run_call(
                "DIM source$(0):source$(0)=\"array\"",
                &format!("CALL Touch({argument})"),
                "100 DEF SUB Touch(value$)\n110 value$=value$+\"!\":PRINT value$\n120 SUBEND",
                immediate,
            );
            assert_eq!(interpreter.take_output(), expected, "{argument}");
            interpreter.process_immediate("PRINT source$(0)").unwrap();
            assert_eq!(interpreter.take_output(), "array\n", "{argument}");
        }
    }
}

#[test]
fn two_parenthesized_arguments_can_alias_the_same_array() {
    for immediate in [false, true] {
        let mut interpreter = run_call(
            "DIM source(0),first(0),second(0):source(0)=5:first(0)=300:second(0)=400",
            "CALL Touch((source),((source)))",
            "100 DEF SUB Touch(first,second)\n110 first(0)=first(0)+1:PRINT second(0)\n120 second(0)=second(0)+2\n130 SUBEND",
            immediate,
        );
        assert_eq!(interpreter.take_output(), " 6\n");
        interpreter
            .process_immediate("PRINT source(0);first(0);second(0)")
            .unwrap();
        assert_eq!(interpreter.take_output(), " 8  300  400\n");
    }
}

#[test]
fn nested_parenthesized_aliases_preserve_redim_mat_and_outer_bindings() {
    for immediate in [false, true] {
        for parameter in ["items", "renamedItems"] {
            let definitions = format!(
                "100 DEF SUB Outer(items)\n110 CALL Inner(((items)))\n\
                 120 PRINT UBOUND(items);items(0);items(2)\n130 items(1)=8\n140 SUBEND\n\
                 200 DEF SUB Inner({parameter})\n210 REDIM {parameter}(2)\n\
                 220 MAT {parameter}={parameter}+10\n230 SUBEND"
            );
            let mut interpreter = run_call(
                "DIM source(1),items(0):source(0)=5:source(1)=6:items(0)=300",
                "CALL Outer(((source)))",
                &definitions,
                immediate,
            );
            assert_eq!(interpreter.take_output(), " 2  15  10\n", "{parameter}");
            interpreter
                .process_immediate("PRINT source(0);source(1);source(2);UBOUND(source);items(0)")
                .unwrap();
            assert_eq!(
                interpreter.take_output(),
                " 15  8  10  2  300\n",
                "{parameter}"
            );
        }
    }
}

#[test]
fn parenthesized_aliases_restore_bindings_after_nested_errors() {
    for immediate in [false, true] {
        for (inner_call, inner_definition, code, caller_check, expected) in [
            (
                "Inner(((items)))",
                "200 DEF SUB Inner(items)\n210 REDIM items(2)\n220 MAT items=items+10\n230 result=1/0\n240 SUBEND",
                ErrorCode::DivisionByZero,
                "PRINT source(0);source(1);source(2);UBOUND(source);items(0):PRINT savedText$",
                " 15  16  10  2  300\nouter\n",
            ),
            (
                "Inner(((items)),7)",
                "200 DEF SUB Inner(items,savedText$)\n210 items(0)=999\n220 SUBEND",
                ErrorCode::TypeMismatch,
                "PRINT source(0);source(1);UBOUND(source);items(0):PRINT savedText$",
                " 5  6  1  300\nouter\n",
            ),
            (
                "Inner(((items)))",
                "200 DEF SUB Inner(items)\n205 LOCAL scratch(-1)\n210 items(0)=999\n220 SUBEND",
                ErrorCode::InvalidValue,
                "PRINT source(0);source(1);UBOUND(source);items(0):PRINT savedText$",
                " 5  6  1  300\nouter\n",
            ),
        ] {
            let definitions = format!(
                "100 DEF SUB Outer(items)\n110 CALL {inner_call}\n120 SUBEND\n{inner_definition}"
            );
            let (mut interpreter, command) = prepare_call(
                "DIM source(1),items(0):source(0)=5:source(1)=6:items(0)=300:savedText$=\"outer\"",
                "CALL Outer((source))",
                &definitions,
                immediate,
            );
            let error = interpreter.process_immediate(&command).unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(interpreter.take_output(), "");
            interpreter.process_immediate(caller_check).unwrap();
            assert_eq!(interpreter.take_output(), expected);
        }
    }
}
