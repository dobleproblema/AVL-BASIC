use avl_basic::{ErrorCode, Interpreter};

const PROGRAM: &str = "10 PRINT 1\n30 PRINT 2\n55 END";

#[test]
fn delete_open_range_after_last_line_preserves_the_loaded_program() {
    let mut interpreter = Interpreter::new();
    interpreter
        .load_file(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/matrix.bas"
        )))
        .unwrap();
    let original = interpreter.program.list();

    for command in ["DELETE 1-5", "DELETE 60-70", "DELETE 60-"] {
        interpreter.process_immediate(command).unwrap();
        assert_eq!(interpreter.program.list(), original, "{command}");
    }

    interpreter.process_immediate("LIST").unwrap();
    assert_eq!(interpreter.take_output(), original);
}

#[test]
fn delete_ranges_outside_the_program_preserve_its_lines() {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(PROGRAM).unwrap();
    let original = interpreter.program.list();

    for command in [
        "DELETE -5",
        "DELETE 1-5",
        "DELETE 60-70",
        "DELETE 60-",
        "DELETE 2147483647-",
    ] {
        interpreter.process_immediate(command).unwrap();
        assert_eq!(interpreter.program.list(), original, "{command}");
    }
}

#[test]
fn delete_ranges_are_valid_on_an_empty_program() {
    let mut interpreter = Interpreter::new();

    for command in ["DELETE 60-", "DELETE -5", "DELETE 60-70", "DELETE -"] {
        interpreter.process_immediate(command).unwrap();
        assert!(interpreter.program.is_empty(), "{command}");
    }
}

#[test]
fn delete_open_ranges_include_the_boundary_and_can_clear_the_program() {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(PROGRAM).unwrap();

    interpreter.process_immediate("DELETE 55-").unwrap();
    assert_eq!(interpreter.program.line_numbers(), vec![10, 30]);
    interpreter.process_immediate("DELETE -10").unwrap();
    assert_eq!(interpreter.program.line_numbers(), vec![30]);
    interpreter.process_immediate("DELETE 30-").unwrap();
    assert!(interpreter.program.is_empty());
    interpreter.process_immediate("DELETE 60-").unwrap();
}

#[test]
fn delete_explicitly_reversed_ranges_and_missing_single_lines_still_report_errors() {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(PROGRAM).unwrap();
    let original = interpreter.program.list();

    for (command, expected) in [
        ("DELETE 70-60", ErrorCode::InvalidArgument),
        ("DELETE 60", ErrorCode::TargetLineNotFound),
    ] {
        assert_eq!(
            interpreter.process_immediate(command).unwrap_err().code,
            expected,
            "{command}"
        );
        assert_eq!(interpreter.program.list(), original, "{command}");
    }
}
