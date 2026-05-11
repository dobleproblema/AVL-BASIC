use avl_basic::{console, ErrorCode, Interpreter};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

fn run_rust(program: &str) -> String {
    let mut interp = Interpreter::new();
    for line in program.lines() {
        interp.process_immediate(line).unwrap();
    }
    interp.process_immediate("RUN").unwrap();
    interp.take_output()
}

fn run_rust_cli(program: &str, input: &str) -> String {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(program.as_bytes()).unwrap();
    file.flush().unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .arg(file.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "interpreter failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ctrl_c_during_run_interrupts_program_without_losing_cont() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 1").unwrap();
    interp.process_immediate("20 PRINT 2").unwrap();

    interp.request_interrupt_for_test();
    let err = interp.process_immediate("RUN").unwrap_err();

    assert_eq!(err.code, ErrorCode::KeyboardInterrupt);
    assert_eq!(err.display_for_basic(), "Execution interrupted by user.");
    assert_eq!(interp.take_output(), "");

    interp.process_immediate("CONT").unwrap();
    assert_eq!(interp.take_output(), " 1\n 2\n");
}

#[test]
fn immediate_goto_after_stop_resumes_at_target_preserving_runtime() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 A=1").unwrap();
    interp.process_immediate("20 STOP").unwrap();
    interp.process_immediate("30 A=A+1").unwrap();
    interp.process_immediate("40 PRINT A").unwrap();

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "");

    interp.process_immediate("GOTO 30").unwrap();
    assert_eq!(interp.take_output(), " 2\n");
}

#[test]
fn immediate_end_cancels_stopped_program_and_does_not_poison_next_immediate_line() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 STOP").unwrap();
    interp.process_immediate("RUN").unwrap();

    interp.process_immediate("END").unwrap();
    let err = interp.process_immediate("CONT").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "There is no stopped program to continue."
    );

    interp.process_immediate("PRINT 1:PRINT 2").unwrap();
    assert_eq!(interp.take_output(), " 1\n 2\n");
}

#[test]
fn immediate_colon_commands_run_after_stopped_program_without_losing_cont() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 STOP").unwrap();
    interp.process_immediate("20 PRINT \"CONT\"").unwrap();

    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), "");

    interp
        .process_immediate("MAT BASE 1:DIM tester(3,3)")
        .unwrap();
    interp
        .process_immediate("PRINT LBOUND(tester):PRINT UBOUND(tester,2)")
        .unwrap();
    assert_eq!(interp.take_output(), " 1\n 3\n");

    interp.process_immediate("CONT").unwrap();
    assert_eq!(interp.take_output(), "CONT\n");
}

#[test]
fn run_file_clears_previous_routine_line_owners() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("first.bas"),
        "100 DEF SUB OLD\n200 PRINT 9\n300 SUBEND\n400 CALL OLD\n500 END\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("second.bas"),
        "100 GOSUB 200\n110 END\n200 PRINT 2\n210 RETURN\n",
    )
    .unwrap();

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    interp.process_immediate("RUN \"first.bas\"").unwrap();
    assert_eq!(interp.take_output(), " 9\n");

    interp.process_immediate("RUN \"second.bas\"").unwrap();
    assert_eq!(interp.take_output(), " 2\n");
}

#[test]
fn print_and_arithmetic_baseline() {
    let output = run_rust(
        r#"10 PRINT 2+3*4
20 PRINT "HOLA"+" "+"BASIC"
30 PRINT 7=7
40 END"#,
    );
    assert_eq!(output, " 14\nHOLA BASIC\n-1\n");
}

#[test]
fn immediate_cat_save_load_delete_and_list_ranges() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("alpha.bas"), "10 PRINT 1\n").unwrap();
    std::fs::write(temp.path().join("notes.txt"), "ignored\n").unwrap();
    std::fs::create_dir(temp.path().join("subdir")).unwrap();

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    interp.process_immediate("CAT").unwrap();
    let cat = interp.take_output();
    assert!(cat.contains("alpha.bas"));
    assert!(cat.contains("subdir/"));
    assert!(!cat.contains("notes.txt"));

    interp.process_immediate("10 print 1").unwrap();
    interp.process_immediate("20 print 2").unwrap();
    interp.process_immediate("30 print 3").unwrap();
    interp.process_immediate("LIST 20-").unwrap();
    assert_eq!(interp.take_output(), "20 PRINT 2\n30 PRINT 3\n");

    interp.process_immediate("DELETE -20").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(interp.take_output(), "30 PRINT 3\n");

    interp.process_immediate("SAVE \"saved").unwrap();
    assert!(temp.path().join("saved.bas").exists());

    interp.process_immediate("NEW").unwrap();
    interp.process_immediate("LOAD \"saved").unwrap();
    interp.process_immediate("RUN").unwrap();
    assert_eq!(interp.take_output(), " 3\n");
}

#[test]
fn missing_load_file_reports_basic_file_not_found() {
    let mut interp = Interpreter::new();
    let err = interp
        .process_immediate("LOAD \"este-programa no existel.bas\"")
        .expect_err("missing LOAD target should fail");
    assert_eq!(err.display_for_basic(), "File not found.");
}

#[test]
fn cd_reports_python_path_errors_and_keeps_virtual_root() {
    let temp = tempfile::tempdir().unwrap();
    let examples = temp.path().join("examples");
    std::fs::create_dir(&examples).unwrap();

    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();

    let err = interp.process_immediate("CD").unwrap_err();
    assert_eq!(err.display_for_basic(), "Invalid argument.");

    let err = interp.process_immediate("CD missing").unwrap_err();
    assert_eq!(err.display_for_basic(), "Missing quotes.");

    let err = interp.process_immediate("CD \"fol\"").unwrap_err();
    assert_eq!(err.display_for_basic(), "File not found.");
    assert_eq!(interp.current_dir, temp.path());

    interp.process_immediate("CD \"examples\"").unwrap();
    assert!(interp.current_dir.ends_with("examples"));

    let err = interp.process_immediate("CD \"dgo\"").unwrap_err();
    assert_eq!(err.display_for_basic(), "File not found.");
    assert!(interp.current_dir.ends_with("examples"));

    interp.process_immediate("CD \"..\"").unwrap();
    assert_eq!(
        interp.current_dir.canonicalize().unwrap(),
        temp.path().canonicalize().unwrap()
    );
}

#[test]
fn immediate_file_command_errors_match_python() {
    let mut interp = Interpreter::new();
    assert_eq!(
        interp
            .process_immediate("LOAD")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("LOAD per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("LOAD \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("SAVE")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("SAVE per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("SAVE \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("RUN per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("MERGE \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("CHAIN \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
    assert_eq!(
        interp
            .process_immediate("CHAIN MERGE \"per.txt\"")
            .unwrap_err()
            .display_for_basic(),
        "Only names with extension '.bas' are allowed."
    );
}

#[test]
fn immediate_files_and_range_errors_match_python() {
    let temp = tempfile::tempdir().unwrap();
    let mut interp = Interpreter::new();
    interp.root_dir = temp.path().to_path_buf();
    interp.current_dir = temp.path().to_path_buf();
    interp.process_immediate("10 PRINT 1").unwrap();
    interp.process_immediate("20 PRINT 2").unwrap();

    assert_eq!(
        interp
            .process_immediate("FILES per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("FILES \"..\"")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("CAT per")
            .unwrap_err()
            .display_for_basic(),
        "Missing quotes."
    );
    assert_eq!(
        interp
            .process_immediate("LIST per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("LIST a-b")
            .unwrap_err()
            .display_for_basic(),
        "Invalid value type."
    );
    assert_eq!(
        interp
            .process_immediate("DELETE per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("RENUM per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("RENUM 10,per")
            .unwrap_err()
            .display_for_basic(),
        "Syntax error."
    );
    assert_eq!(
        interp
            .process_immediate("EDIT per")
            .unwrap_err()
            .display_for_basic(),
        "Invalid argument."
    );
    assert_eq!(
        interp
            .process_immediate("EDIT 999")
            .unwrap_err()
            .display_for_basic(),
        "Target line does not exist."
    );
}

#[test]
fn immediate_run_start_line_matches_python() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 1").unwrap();
    interp.process_immediate("20 PRINT 2").unwrap();
    interp.process_immediate("RUN 20").unwrap();
    assert_eq!(interp.take_output(), " 2\n");

    let mut interp = Interpreter::new();
    interp.process_immediate("10 PRINT 1").unwrap();
    assert_eq!(
        interp
            .process_immediate("RUN 99")
            .unwrap_err()
            .display_for_basic(),
        "Target line does not exist."
    );
}

#[test]
fn error_codes_and_messages_match_python_oracle_table() {
    let expected = [
        (
            1,
            ErrorCode::ImmediateCommand,
            "Instruction not allowed in a program.",
        ),
        (
            2,
            ErrorCode::NonImmediateCommand,
            "Instruction not allowed in immediate mode.",
        ),
        (3, ErrorCode::InvalidLineFormat, "Invalid line format."),
        (4, ErrorCode::InvalidValue, "Invalid value."),
        (5, ErrorCode::TypeMismatch, "Invalid value type."),
        (6, ErrorCode::DivisionByZero, "Division by zero."),
        (7, ErrorCode::Overflow, "Numeric overflow."),
        (8, ErrorCode::Undefined, "Undefined variable or function."),
        (9, ErrorCode::InvalidName, "Invalid name."),
        (10, ErrorCode::InvalidLineNumber, "Invalid line number."),
        (11, ErrorCode::InvalidTargetLine, "Invalid target line."),
        (
            12,
            ErrorCode::TargetLineNotFound,
            "Target line does not exist.",
        ),
        (
            13,
            ErrorCode::ReturnWithoutGosub,
            "RETURN without matching GOSUB.",
        ),
        (14, ErrorCode::ReturnNotFound, "Invalid return line."),
        (15, ErrorCode::Syntax, "Syntax error."),
        (16, ErrorCode::ForWithoutNext, "FOR without matching NEXT."),
        (17, ErrorCode::NextWithoutFor, "NEXT without matching FOR."),
        (
            18,
            ErrorCode::WhileWithoutWend,
            "WHILE without matching WEND.",
        ),
        (
            19,
            ErrorCode::WendWithoutWhile,
            "WEND without matching WHILE.",
        ),
        (20, ErrorCode::ResumeWithoutError, "RESUME without ERROR."),
        (
            21,
            ErrorCode::NumericExpression,
            "Expression must be numeric.",
        ),
        (22, ErrorCode::EvalError, "Error evaluating expression."),
        (23, ErrorCode::UnknownType, "Unknown data type."),
        (24, ErrorCode::DataExhausted, "No more DATA to read."),
        (25, ErrorCode::NoData, "The line has no DATA for RESTORE."),
        (26, ErrorCode::InvalidArgument, "Invalid argument."),
        (27, ErrorCode::NoDimension, "Empty dimension expression."),
        (28, ErrorCode::IndexError, "Error generating array indices."),
        (29, ErrorCode::UsingFormat, "Error in PRINT USING format."),
        (
            30,
            ErrorCode::VarNumberMismatch,
            "Number of inputs does not match number of variables.",
        ),
        (
            31,
            ErrorCode::ArgumentMismatch,
            "Incorrect number of arguments.",
        ),
        (32, ErrorCode::InvalidIndex, "Invalid index."),
        (33, ErrorCode::UndefinedIndex, "Undefined index."),
        (34, ErrorCode::IndexOutOfRange, "Index out of range."),
        (35, ErrorCode::OutOfBounds, "Out of range."),
        (36, ErrorCode::FunctionError, "Error evaluating function."),
        (
            37,
            ErrorCode::ForbiddenExpression,
            "Expression not allowed.",
        ),
        (
            38,
            ErrorCode::Unsupported,
            "Feature not supported in this environment.",
        ),
        (39, ErrorCode::MissingQuotes, "Missing quotes."),
        (40, ErrorCode::FileNotFound, "File not found."),
        (
            41,
            ErrorCode::KeyboardInterrupt,
            "Execution interrupted by user.",
        ),
        (
            42,
            ErrorCode::OnlyBasFiles,
            "Only names with extension '.bas' are allowed.",
        ),
        (
            43,
            ErrorCode::OnlyPngFiles,
            "Only names with extension '.png' are allowed.",
        ),
        (44, ErrorCode::HandlerError, "Error while handling errors."),
        (45, ErrorCode::MergeError, "Error merging files."),
        (
            46,
            ErrorCode::NoStoppedProgram,
            "There is no stopped program to continue.",
        ),
        (
            47,
            ErrorCode::FunctionForbidden,
            "Instruction not allowed inside a function.",
        ),
        (48, ErrorCode::FnEndWithoutDef, "Malformed function."),
        (49, ErrorCode::IfWithoutEndIf, "IF without matching END IF."),
        (50, ErrorCode::ElseWithoutIf, "ELSE without matching IF."),
        (51, ErrorCode::EndIfWithoutIf, "END IF without matching IF."),
        (
            52,
            ErrorCode::InvalidDimensions,
            "Invalid number of dimensions.",
        ),
        (
            53,
            ErrorCode::MatIdnDimension,
            "MAT IDN requires a two-dimensional square matrix.",
        ),
        (
            54,
            ErrorCode::SubroutineForbidden,
            "Instruction not allowed inside a subroutine.",
        ),
        (55, ErrorCode::SubEndWithoutDef, "Malformed subroutine."),
        (
            56,
            ErrorCode::LocalNotAtStart,
            "LOCAL must appear in the initial block of a function or subroutine.",
        ),
    ];
    for (number, code, message) in expected {
        assert_eq!(code.number(), number);
        assert_eq!(ErrorCode::from_number(number), Some(code));
        assert_eq!(code.message(), message);
    }
    assert_eq!(ErrorCode::from_number(0), None);
    assert_eq!(ErrorCode::from_number(57), None);
}

#[test]
fn immediate_and_program_only_command_errors_match_python() {
    let mut interp = Interpreter::new();
    let err = interp.process_immediate("ERROR 3").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "Instruction not allowed in immediate mode."
    );

    let mut interp = Interpreter::new();
    interp.process_immediate("10 LOAD \"missing.bas\"").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(
        err.display_for_basic(),
        "Line 10. Instruction not allowed in a program."
    );
}

#[test]
fn error_statement_reports_python_messages() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10 ERROR 40").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 10. File not found.");

    let mut interp = Interpreter::new();
    interp.process_immediate("10 ERROR 999").unwrap();
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 10. Error 999");
}

#[test]
fn console_normalization_completes_file_quotes_and_bas_extension() {
    assert_eq!(console::normalize_code("load\"demo"), "LOAD \"demo.bas\"");
    assert_eq!(
        console::normalize_code("10 print 1e3 'comment"),
        "10 PRINT 1E+3 'comment"
    );
    assert_eq!(
        console::normalize_code("10 rem print if and mod"),
        "10 REM print if and mod"
    );
    assert_eq!(
        console::normalize_code("10 print 1:print 2"),
        "10 PRINT 1 : PRINT 2"
    );
    assert_eq!(
        console::normalize_code("10 print 0:if a then print 1:print 2 else print 3:print 4"),
        "10 PRINT 0 : IF a THEN PRINT 1:PRINT 2 ELSE PRINT 3:PRINT 4"
    );
    assert_eq!(
        console::normalize_code("10 print \"a:b\":print 2"),
        "10 PRINT \"a:b\" : PRINT 2"
    );
}

#[test]
fn console_highlight_matches_python_keyword_boundaries() {
    let highlighted = console::syntax_highlight("10 IF A AND B THEN CALL worker", true);
    assert!(highlighted.contains("\x1b[38;5;248mAND\x1b[0m"));
    assert!(!highlighted.contains("\x1b[1m\x1b[3m\x1b[97mAND\x1b[0m"));
    assert!(highlighted.contains("\x1b[1m\x1b[3m\x1b[97mWORKER\x1b[0m"));

    let highlighted = console::syntax_highlight("20 X=ABS(Y)+FNfoo(Y) MOD 2", true);
    assert!(highlighted.contains("\x1b[38;5;248mABS\x1b[0m"));
    assert!(highlighted.contains("\x1b[38;5;248mFNFOO\x1b[0m"));
    assert!(highlighted.contains("\x1b[38;5;248mMOD\x1b[0m"));

    let highlighted = console::syntax_highlight("30 PRINT -5, +.25", true);
    assert!(highlighted.contains("\x1b[38;5;214m-5\x1b[0m"));
    assert!(highlighted.contains("\x1b[38;5;214m+0.25\x1b[0m"));

    let mut cases = HashMap::new();
    cases.insert("PERITA".to_string(), "perIta".to_string());
    assert_eq!(
        console::syntax_highlight_with_cases("40 REM PERITA", false, Some(&cases)),
        "40 REM PERITA"
    );
}

#[test]
fn list_preserves_indentation_and_variable_canonical_case() {
    let mut interp = Interpreter::new();
    interp.process_immediate("10   MiVariable=1").unwrap();
    interp.process_immediate("20     print mivariable").unwrap();
    interp
        .process_immediate("30 REM mivariable print if and mod")
        .unwrap();
    interp
        .process_immediate("40 print mivariable ' mivariable")
        .unwrap();
    interp
        .process_immediate("50 print 1:if mivariable then print 2:print 3")
        .unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "10   MiVariable=1\n20     PRINT MiVariable\n30 REM mivariable print if and mod\n40 PRINT MiVariable ' mivariable\n50 PRINT 1 : IF MiVariable THEN PRINT 2:PRINT 3\n"
    );

    let mut interp = Interpreter::new();
    interp.process_immediate("20 perIta=1").unwrap();
    interp.process_immediate("30 PRINT perIta").unwrap();
    interp.process_immediate("40 REM PERITA").unwrap();
    interp.process_immediate("LIST").unwrap();
    assert_eq!(
        interp.take_output(),
        "20 perIta=1\n30 PRINT perIta\n40 REM PERITA\n"
    );
}

#[test]
fn for_next_and_arrays_baseline() {
    let output = run_rust(
        r#"10 DIM A(3)
20 FOR I=1 TO 3
30 A(I)=I*I
40 NEXT I
50 PRINT A(1);A(2);A(3)
60 END"#,
    );
    assert_eq!(output, " 1  4  9\n");
}

#[test]
fn for_parser_preserves_spaces_after_equals() {
    let output = run_rust(
        r#"10 FOR C= 0 TO 2
20 PRINT C;
30 NEXT
40 PRINT
50 END"#,
    );
    assert_eq!(output, " 0  1  2 \n");
}

#[test]
fn nested_exit_for_preserves_last_loop_values() {
    let output = run_rust(
        r#"10 FOR x=1 TO 10
11 IF x=5 THEN EXIT FOR
20 FOR y=1 TO 10
21 IF y=6 THEN EXIT FOR
30 FOR z=1 TO 10
31 IF z=7 THEN EXIT FOR
40 NEXT
50 NEXT
60 NEXT
70 PRINT x;y;z"#,
    );
    assert_eq!(output, " 5  6  7\n");
}

#[test]
fn swap_exchanges_scalars_and_array_elements() {
    let output = run_rust(
        r#"10 A=B=7
20 B=2
30 SWAP A,B
40 PRINT A,B
50 DIM A$(10)
60 A$(7)="adios":A$(8)="hola"
70 SWAP A$(7),A$(8)
80 PRINT A$(7),A$(8)
90 END"#,
    );
    assert_eq!(output, " 2\t 7\nhola\tadios\n");
}

#[test]
fn swap_rejects_non_lvalues() {
    let mut interp = Interpreter::new();
    for line in r#"10 X=7
20 Y=2
30 SWAP X,Y+1
40 PRINT X,Y"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("SWAP with an expression target must fail");
    assert_eq!(err.display_for_basic(), "Line 30. Invalid argument.");
}

#[test]
fn swap_array_index_errors_follow_python_bounds() {
    let mut interp = Interpreter::new();
    for line in r#"10 X=7
20 SWAP A(17),A(18)
30 PRINT X"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("implicit arrays should keep default bounds");
    assert_eq!(err.display_for_basic(), "Line 20. Index out of range.");
}

#[test]
fn recursive_single_line_def_fn_reports_basic_error() {
    let mut interp = Interpreter::new();
    for line in r#"10 DEF FNR(X)=FNR(X-1)+1
20 PRINT FNR(3)"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("recursive single-line DEF FN must not overflow the Rust stack");
    assert_eq!(
        err.display_for_basic(),
        "Line 20. Instruction not allowed inside a function."
    );
}

#[test]
fn single_line_def_fn_rejects_array_arguments() {
    let mut interp = Interpreter::new();
    for line in r#"10 DIM A(2),B(2)
20 MAT A=2 : MAT B=3
30 DEF FNSUM(X,Y)=X(0)+Y(1)
40 PRINT FNSUM(A,B)"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("single-line DEF FN should not accept array parameters");
    assert_eq!(err.display_for_basic(), "Line 40. Invalid value type.");
}

#[test]
fn clear_resets_runtime_variables_without_clearing_program() {
    let output = run_rust(
        r#"10 X=5
20 PRINT X
30 CLEAR
40 PRINT X
50 END"#,
    );
    assert_eq!(output, " 5\n 0\n");
}

#[test]
fn mat_base_controls_lbound_and_ubound_reports_dimensions() {
    let mut interp = Interpreter::new();
    for line in r#"10 DIM A(20)
20 PRINT LBOUND(A)
30 PRINT UBOUND(A)
40 MAT BASE 1
50 PRINT LBOUND(A)
60 PRINT UBOUND(A)
70 PRINT UBOUND(A,2)"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("invalid array dimension should fail");
    assert_eq!(interp.take_output(), " 0\n 20\n 1\n 20\n");
    assert_eq!(err.display_for_basic(), "Line 70. Index out of range.");
}

#[test]
fn multiline_fn_can_return_matrix_from_mat_assignment() {
    let output = run_rust(
        r#"10 DEF FNM(X,Y)
20 MAT FNM=X+Y
40 FNEND
50 DIM A(1,1),B(1,1),C(1,1)
60 MAT A=CON
70 MAT B=2*A
80 MAT C=FNM(A,B)
90 MAT PRINT C"#,
    );
    assert_eq!(output, " 3   3\n 3   3\n");
}

#[test]
fn multiline_subroutine_locals_and_array_references() {
    let output = run_rust(
        r#"10 A=1
20 DIM D(2),Z(2)
30 D(1)=4:Z(1)=1
40 DEF SUB WORK(T)
50 LOCAL A,D(3)
60 A=7
70 D(1)=9
80 T(1)=5
90 PRINT A
100 PRINT D(1)
110 SUBEND
120 CALL WORK(Z)
130 PRINT A
140 PRINT D(1)
150 PRINT Z(1)"#,
    );
    assert_eq!(output, " 7\n 9\n 1\n 4\n 5\n");
}

#[test]
fn mat_stat_functions_update_context_values() {
    let output = run_rust(
        r#"10 MAT BASE 1
20 DIM A(3,3),V1(3),V2(3),B(3,5)
30 DATA -3,2,3,5,-3,5,2,5,-1
40 DATA 2,1,3,1,4,2
50 MAT READ A
60 MAT READ V1,V2
70 IF ABSUM(A)<>29 THEN PRINT "E1"
80 IF AMAX(A)<>5 OR AMAXCOL<>1 OR AMAXROW<>2 THEN PRINT "E2"
90 IF AMIN(A)<>-3 OR AMINCOL<>1 OR AMINROW<>1 THEN PRINT "E3"
100 IF CNORM(A)<>10 OR CNORMCOL<>1 THEN PRINT "E4"
110 IF DOT(V1,V2)<>12 THEN PRINT "E5"
120 IF ROUND(FNORM(A),2)<>10.54 THEN PRINT "E6"
130 IF LBND(A)<>1 THEN PRINT "E7"
140 IF MAXAB(A)<>5 OR MAXABCOL<>1 OR MAXABROW<>2 THEN PRINT "E8"
150 IF RNORM(A)<>13 OR RNORMROW<>2 THEN PRINT "E9"
160 IF SUM(A)<>15 THEN PRINT "E10"
170 IF UBND(B,1.4)<>3 OR UBND(B,1.6)<>5 THEN PRINT "E11"
180 PRINT "OK""#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn mat_input_fills_matrix_from_multiple_tokens_per_line() {
    let output = run_rust_cli(
        r#"10 MAT BASE 1 : DIM aa(2,2)
20 MAT INPUT aa
30 MAT PRINT aa"#,
        "1,2\n3\n4,5,6\n",
    );
    assert_eq!(output, "aa(1,1)? aa(2,1)? aa(2,2)?  1   2\n 3   4\n");
}

#[test]
fn print_using_suppresses_leading_thousands_separators() {
    let output = run_rust(
        r#"10 PRINT USING "Monthly payment:    ###,###,###.##"; 536.82
20 PRINT USING "Total paid:         ###,###,###.##"; 193255.78
30 PRINT USING "Total interest:     ###,###,###.##"; 93255.78"#,
    );
    assert_eq!(
        output,
        "Monthly payment:            536.82\nTotal paid:             193,255.78\nTotal interest:          93,255.78\n"
    );
}

#[test]
fn print_using_leading_comma_uses_european_separators() {
    let output = run_rust(
        r#"10 X=123456.7892
20 PRINT USING ",#,###,###,###.##"; X
30 PRINT USING ",#,###,###.##"; 1234567.23
40 PRINT USING ",0.00^^^^"; 12345"#,
    );
    assert_eq!(output, "      123.456,79\n1.234.567,23\n1,23E+04\n");
}

#[test]
fn tab_positions_to_columns_across_print_statements() {
    let output = run_rust(
        r###"10 PRINT "Month"; TAB(17); "Payment"; TAB(33); "Interest"; TAB(49); "Principal"; TAB(68); "Balance"
20 PRINT USING "##"; 1;
30 PRINT TAB(8);
40 PRINT USING "#,###,###,###.##"; 536.82;
50 PRINT TAB(25);
60 PRINT USING "#,###,###,###.##"; 416.67;
70 PRINT TAB(42);
80 PRINT USING "#,###,###,###.##"; 120.15;
90 PRINT TAB(59);
100 PRINT USING "#,###,###,###.##"; 99879.85"###,
    );
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(
        lines[0],
        "Month           Payment         Interest        Principal          Balance"
    );
    assert_eq!(
        lines[1],
        " 1               536.82           416.67           120.15        99,879.85"
    );
    assert!(lines.iter().all(|line| line.len() <= 80));
}

#[test]
fn degree_mode_affects_trig_functions() {
    let output = run_rust(
        r#"10 DEG
20 PRINT SIN(30);COS(60)
30 RAD
40 PRINT ROUND(SIN(PI/6),6)
50 END"#,
    );
    assert_eq!(output, " 0.5  0.5\n 0.5\n");
}

#[test]
fn graphics_screen_string_baseline() {
    let output = run_rust(
        r##"10 SCREEN
20 MODE 640
30 PAPER 0 : CLG
40 PLOT 1,1,2
50 A$=SCREEN$
60 PRINT LEFT$(A$,6)
70 PRINT TEST(1,1)
80 END"##,
    );
    assert_eq!(output, "640x48\n 16711680\n");
}

#[test]
fn string_fast_paths_preserve_basic_results() {
    let output = run_rust(
        r#"10 A$="HOLA,MUNDO"
20 PRINT LEN(A$)
30 PRINT LEFT$(A$,4)
40 PRINT RIGHT$(A$,5)
50 PRINT MID$(A$,6,5)
60 PRINT INSTR(A$,",")
70 PRINT A$[2]
80 MID$(A$,6,5)="BASIC"
90 PRINT A$
100 END"#,
    );
    assert_eq!(output, " 10\nHOLA\nMUNDO\nMUNDO\n 5\nO\nHOLA,BASIC\n");
}

#[test]
fn version_string_matches_python_language_version() {
    let output = run_rust(
        r#"10 PRINT VERSION$
20 END"#,
    );
    assert_eq!(output, "1.5.18\n");
}

#[test]
fn graphics_mask_consumes_low_bit_first() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 MASK 170
30 MOVE 0,0 : DRAW 15,0,2
40 MASK
50 PRINT SPRITE$(0,0,15,0)
60 END"#,
    );
    assert_eq!(output, format!("16x1:{}\n", "000000ff0000".repeat(8)));
}

#[test]
fn graphics_triangle_axis_and_graph_commands_run() {
    let output = run_rust(
        r#"10 SCREEN : MODE 640 : PAPER 0 : CLG
20 SCALE -1,1,-1,1,10
30 CROSSAT 0,0
40 XAXIS 0.5
50 YAXIS 0.5
60 GRAPH X*X,0.1
70 TRIANGLE -.5,-.5,.5,-.5,0,.5,2
80 FTRIANGLE -.25,-.25,.25,-.25,0,.25,3
90 PRINT "OK"
100 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn mouse_commands_and_functions_are_deterministic_headless() {
    let output = run_rust(
        r#"10 SCREEN
20 ON MOUSE LEFTDOWN GOSUB 100
30 MOUSE 0
40 PRINT MOUSEX;MOUSEY;MOUSELEFT;MOUSERIGHT;MOUSEEVENT$
50 END
100 RETURN"#,
    );
    assert_eq!(output, " 0  0  0  0 \n");
}

#[test]
fn graphics_runtime_errors_include_program_line() {
    let mut interp = Interpreter::new();
    for line in r#"10 SCREEN : CLG
20 COLMODE 2
30 SPRITE "1x1:00ff00",10,10,0,0"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp
        .process_immediate("RUN")
        .expect_err("non-positive explicit sprite id must fail");
    assert_eq!(err.display_for_basic(), "Line 30. Invalid argument.");
}

#[test]
fn frame_fps_extension_accepts_one_positive_argument() {
    let output = run_rust(
        r#"10 SCREEN
20 FRAME
30 FRAME 60
40 PRINT "OK"
50 END"#,
    );
    assert_eq!(output, "OK\n");
}

#[test]
fn frame_fps_extension_rejects_non_positive_rate() {
    let mut interp = Interpreter::new();
    for line in r#"10 SCREEN
20 FRAME 0"#
        .lines()
    {
        interp.process_immediate(line).unwrap();
    }
    let err = interp.process_immediate("RUN").unwrap_err();
    assert_eq!(err.display_for_basic(), "Line 20. Invalid argument.");
}

#[test]
fn frame_fps_extension_rejects_extra_arguments() {
    let mut interp = Interpreter::new();
    let err = interp.process_immediate("FRAME 30,60").unwrap_err();
    assert_eq!(err.display_for_basic(), "Incorrect number of arguments.");
}

#[test]
fn on_gosub_selects_one_based_target_and_returns() {
    let output = run_rust(
        r#"10 S=1 : GOSUB 100
20 S=2 : GOSUB 100
30 PRINT A
40 END
100 ON S GOSUB 200,300
110 RETURN
200 A=A+10
210 RETURN
300 A=A+100
310 RETURN"#,
    );
    assert_eq!(output, " 110\n");
}

#[test]
fn if_then_colon_else_executes_only_selected_branch() {
    let output = run_rust(
        r#"10 D=12 : F=10
20 IF D>F THEN D=D-F:K=1 ELSE K=0
30 PRINT D;K
40 D=3
50 IF D>F THEN D=D-F:K=1 ELSE K=0
60 PRINT D;K
70 END"#,
    );
    assert_eq!(output, " 2  1\n 3  0\n");
}
