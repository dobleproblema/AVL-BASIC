use avl_basic::{console, Debugger, ErrorCode, Interpreter, RunOutcome};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn assert_underlines(diagnostic: &str, source: &str, failed: &str) {
    let mut lines = diagnostic.lines();
    let header = lines.next().unwrap();
    assert!(header.contains("Division by zero."), "{diagnostic}");
    let shown = lines.next().unwrap();
    assert_eq!(shown.trim_start(), source, "{diagnostic}");
    let marker = lines.next().unwrap();
    let failed_at = source.find(failed).unwrap();
    let column = shown.len() - shown.trim_start().len() + source[..failed_at].chars().count();
    assert_eq!(marker.find('^'), Some(column), "{diagnostic}");
    let underline = &marker[column..];
    assert_eq!(underline.len(), failed.chars().count(), "{diagnostic}");
    assert!(underline.bytes().all(|ch| matches!(ch, b'^' | b'~')));
    assert!(lines.next().is_none(), "{diagnostic}");
}

fn interpreter_at(root: &Path, source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = root.to_path_buf();
    interpreter.current_dir = root.to_path_buf();
    interpreter.program.load_text(source).unwrap();
    interpreter
}

#[test]
fn known_builtins_report_arity_and_unknown_user_functions_report_undefined() {
    let mut interpreter = Interpreter::new();
    for source in ["PRINT RND(1)", "PRINT KEYDOWN()", "PRINT VERSION$(1)"] {
        let error = interpreter.process_immediate(source).unwrap_err();
        assert_eq!(error.code, ErrorCode::ArgumentMismatch, "{source}");
    }
    let error = interpreter
        .process_immediate("PRINT FNMissing(1)")
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Undefined);
}

#[test]
fn malformed_assignment_targets_report_a_syntax_error() {
    let mut interpreter = Interpreter::new();
    for source in ["X+=2", "LET X+=2", "A+(1)=2"] {
        let error = interpreter.process_immediate(source).unwrap_err();
        assert_eq!(error.code, ErrorCode::Syntax, "{source}");
        assert_eq!(error.display_for_basic(), "Syntax error.");
    }

    let source = "20 X=1 : X+=2 : X=3";
    interpreter.program.load_text(source).unwrap();
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.code, ErrorCode::Syntax);
    assert_eq!(error.line, Some(20));
    let statement = error.statement.unwrap();
    assert_eq!(statement.source, source);
    assert_eq!(&statement.source[statement.span], "X+=2");
}

#[test]
fn read_validates_all_destinations_before_using_data() {
    let directory = tempfile::tempdir().unwrap();
    let mut immediate = Interpreter::new();
    for statement in ["READ", "READ 3", "READ A(1+)"] {
        let error = immediate.process_immediate(statement).unwrap_err();
        assert_eq!(error.code, ErrorCode::Syntax, "{statement}");
    }
    assert_eq!(
        immediate.process_immediate("READ A").unwrap_err().code,
        ErrorCode::DataExhausted
    );

    let mut immediate = interpreter_at(directory.path(), "10 DATA 7\n20 END");
    assert_eq!(immediate.run_loaded().unwrap(), RunOutcome::End);
    immediate.process_immediate("READ A : PRINT A").unwrap();
    assert_eq!(immediate.take_output().trim(), "7");

    let mut interpreter = interpreter_at(
        directory.path(),
        concat!(
            "10 DATA 9,8,\"Z\"\n",
            "20 ON ERROR GOTO 80\n",
            "30 A=42 : S$=\"ABC\"\n",
            "40 READ A,3\n",
            "50 READ B(1),C,MID$(S$,2,1)\n",
            "60 PRINT A : PRINT B(1) : PRINT C : PRINT S$ : END\n",
            "80 PRINT ERR : PRINT ERL : RESUME NEXT\n",
        ),
    );
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(
        interpreter
            .take_output()
            .split_whitespace()
            .collect::<Vec<_>>(),
        ["15", "40", "42", "9", "8", "AZC"]
    );
}

#[test]
fn diagnostic_messages_and_on_error_codes_match_the_cause() {
    let directory = tempfile::tempdir().unwrap();
    for (setup, statement, code, message) in [
        ("REM", "X+=2", 15, "Syntax error."),
        ("REM", "PRINT ABS((-1)^0.5)", 4, "Invalid value."),
        ("REM", "A=ABS((-1)^0.5)", 4, "Invalid value."),
        ("REM", "PRINT ABS(\"A\")", 5, "Invalid value type."),
        ("REM", "PRINT RND(1)", 31, "Incorrect number of arguments."),
        (
            "REM",
            "PRINT KEYDOWN()",
            31,
            "Incorrect number of arguments.",
        ),
        (
            "REM",
            "PRINT FNMissing(1)",
            8,
            "Undefined variable or function.",
        ),
        (
            "REM",
            "EXIT FOR",
            17,
            "EXIT FOR without an active FOR loop.",
        ),
        (
            "REM",
            "EXIT WHILE",
            19,
            "EXIT WHILE without an active WHILE loop.",
        ),
        (
            "REM",
            "SCREEN FULL MAYBE",
            31,
            "SCREEN FULL expects ON or OFF.",
        ),
        (
            "REM",
            "SCREEN SYNC MAYBE",
            31,
            "SCREEN SYNC expects ON or OFF.",
        ),
        (
            "DIM A(1,1),B(2,2)",
            "MAT C=A+B",
            52,
            "Invalid array dimensions.",
        ),
        ("REM", "PRINT &H", 15, "Syntax error."),
        ("REM", "PRINT &X2", 15, "Syntax error."),
        ("REM", "PRINT &HFFFFFFFFFFFFFFFFF", 7, "Numeric overflow."),
        ("REM", "DIM A()", 27, "Empty dimension expression."),
        ("DIM A(2)", "REDIM A()", 27, "Empty dimension expression."),
        (
            "REM",
            "OPEN \"absent/data.txt\" FOR INPUT AS #1",
            40,
            "File not found.",
        ),
        (
            "REM",
            "OPEN \"absent/data.txt\" FOR OUTPUT AS #1",
            61,
            "File I/O error.",
        ),
    ] {
        let mut interpreter =
            interpreter_at(directory.path(), &format!("10 {setup}\n30 {statement}"));
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(error.code.number(), code, "{statement}: {error}");
        assert_eq!(error.display_for_basic(), format!("Line 30. {message}"));

        let mut interpreter = interpreter_at(
            directory.path(),
            &format!(
                "10 {setup}\n20 ON ERROR GOTO 100\n30 {statement}\n40 END\n100 PRINT ERR : PRINT ERL : END"
            ),
        );
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
        assert_eq!(
            interpreter
                .take_output()
                .split_whitespace()
                .collect::<Vec<_>>(),
            [code.to_string(), "30".to_string()],
            "{statement}"
        );
    }
}

#[test]
fn recursion_diagnostics_preserve_err_in_the_root_handler() {
    let directory = tempfile::tempdir().unwrap();
    for (call, definition, code, line, message) in [
        (
            "CALL Again",
            "50 DEF SUB Again\n60 CALL Again\n70 SUBEND",
            55,
            60,
            "Recursive CALL to subroutine 'AGAIN' is not allowed.",
        ),
        (
            "PRINT FNR(1)",
            "50 DEF FNR(X)=FNR(X)",
            47,
            20,
            "Recursive call to function 'FNR' is not allowed.",
        ),
    ] {
        let source = format!("20 {call}\n30 END\n{definition}\n");
        let mut interpreter = interpreter_at(directory.path(), &source);
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(error.code.number(), code);
        assert_eq!(error.display_for_basic(), format!("Line {line}. {message}"));

        let source = format!("10 ON ERROR GOTO 100\n{source}100 PRINT ERR : PRINT ERL : END");
        let mut interpreter = interpreter_at(directory.path(), &source);
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
        assert_eq!(
            interpreter
                .take_output()
                .split_whitespace()
                .collect::<Vec<_>>(),
            [code.to_string(), line.to_string()],
        );
    }
}

#[test]
fn static_diagnostic_detail_is_preserved_when_a_handler_fails() {
    let directory = tempfile::tempdir().unwrap();
    let mut interpreter = interpreter_at(
        directory.path(),
        "10 ON ERROR GOTO 100\n20 ERROR 5\n30 END\n100 EXIT FOR",
    );
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.code, ErrorCode::NextWithoutFor);
    assert_eq!(
        error.display_for_basic(),
        "Error in error handler: EXIT FOR without an active FOR loop."
    );
}

#[test]
fn command_line_underlines_the_failed_statement_after_a_colon_inside_a_string() {
    let directory = tempfile::tempdir().unwrap();
    let source = r#"10 PRINT "é:ok" : PRINT 1/0 : PRINT "unreachable""#;
    fs::write(directory.path().join("failure.bas"), source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .current_dir(directory.path())
        .arg("failure.bas")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "é:ok");
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert_underlines(&diagnostic, source, "PRINT 1/0");
}

#[test]
fn immediate_repl_underlines_the_failed_leaf_of_a_chained_if() {
    let source = r#"A=1 : IF A THEN PRINT "before" : PRINT 1/0 ELSE PRINT "other branch""#;
    let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{source}\nSYSTEM").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines = stdout.lines().collect::<Vec<_>>();
    let header = lines
        .iter()
        .position(|line| line.contains("Division by zero."))
        .unwrap_or_else(|| panic!("missing error in {stdout}"));
    assert_eq!(lines[header - 1], "before");
    assert_underlines(
        &lines[header..header + 3].join("\n"),
        &console::normalize_code(source),
        "PRINT 1/0",
    );
}

#[test]
fn repl_diagnostics_color_the_source_even_without_a_numbered_header() {
    for (theme, red, keyword) in [
        ("dark", "\x1b[31m", "\x1b[97mPRINT"),
        ("light", "\x1b[38;5;160m", "\x1b[30mPRINT"),
    ] {
        for (input, header) in [
            ("A=1 : PRINT 1/0\n", "Division by zero."),
            (
                "10 ON ERROR GOTO 100\n20 ERROR 5\n30 END\n100 A=1 : PRINT 1/0\nRUN\n",
                "Error in error handler: Division by zero.",
            ),
        ] {
            let mut child = Command::new(env!("CARGO_BIN_EXE_avl-basic"))
                .env_remove("NO_COLOR")
                .env("AVL_BASIC_COLOR", "always")
                .env("AVL_BASIC_THEME", theme)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            write!(child.stdin.take().unwrap(), "{input}SYSTEM\n").unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            let stdout = String::from_utf8(output.stdout).unwrap();
            let lines: Vec<_> = stdout.lines().collect();
            let message = lines.iter().position(|line| line.contains(header)).unwrap();
            assert_eq!(lines[message], format!("\x1b[3m{red}{header}\x1b[0m"));
            assert!(lines[message + 1].contains(keyword), "{theme}: {stdout:?}");
            assert!(!lines[message + 1].contains(red), "{theme}: {stdout:?}");
            assert!(lines[message + 2].starts_with(&format!("\x1b[3m{red}")));
            assert!(lines[message + 2].ends_with("^^^^^^^^^\x1b[0m"));
        }
    }
}

#[test]
fn normal_and_debug_runs_mark_inline_if_guards_and_executed_branches() {
    for (source, failed) in [
        ("120 IF 1/0 THEN A=2 ELSE A=3", "IF 1/0 THEN"),
        ("120 IF 1 THEN A=2 : B=A/0 : C=4 ELSE B=0", "B=A/0"),
        ("120 IF 0 THEN B=0 ELSE A=2 : B=A/0 : C=4", "B=A/0"),
    ] {
        for debug in [false, true] {
            let mut interpreter = Interpreter::new();
            interpreter.program.load_text(source).unwrap();
            if debug {
                interpreter.set_debugger(Debugger::scripted([]));
            }
            let error = interpreter.run_loaded().unwrap_err();
            assert_eq!(error.code, ErrorCode::DivisionByZero);
            assert_eq!(error.line, Some(120));
            assert_eq!(error.display_for_basic(), "Line 120. Division by zero.");
            assert_underlines(&error.display_diagnostic(), source, failed);
        }
    }
}

#[test]
fn inline_if_with_a_tab_after_the_keyword_marks_the_branch() {
    let mut interpreter = Interpreter::new();
    let source = "120 A=1 : IF\tA THEN PRINT 1/0 ELSE PRINT 0";
    interpreter.program.load_text(source).unwrap();
    let error = interpreter.run_loaded().unwrap_err();
    let statement = error.statement.unwrap();
    assert_eq!(statement.source, source);
    assert_eq!(&statement.source[statement.span], "PRINT 1/0");
}

#[test]
fn error_after_merge_uses_the_original_source_of_the_running_line() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = r#"10 A=1 : IF A THEN MERGE "replace.bas" : PRINT 1/0 ELSE PRINT 0"#;
    fs::write(
        root.join("replace.bas"),
        "5 REM inserted before the running line\n10 PRINT 99\n",
    )
    .unwrap();

    for debug in [false, true] {
        let mut interpreter = interpreter_at(root, source);
        if debug {
            interpreter.set_debugger(Debugger::scripted([]));
        }
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(error.line, Some(10));
        assert_underlines(&error.display_diagnostic(), source, "PRINT 1/0");
        assert_eq!(interpreter.program.get(10), Some(" PRINT 99"));
    }
}

#[test]
fn propagated_error_marks_the_inner_library_statement_at_a_shared_line_number() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = "20 IF 0 THEN PRINT 5 ELSE X=3 : PRINT X/Divisor : PRINT 9";
    fs::write(
        root.join("library.bas"),
        format!(
            "10 DEF SUB Inner(Divisor)\n{source}\n30 SUBEND\n40 DEF SUB Outer(Divisor)\n50 CALL Inner(Divisor)\n60 SUBEND\n"
        ),
    )
    .unwrap();
    let mut interpreter = interpreter_at(
        root,
        "10 IMPORT \"library.bas\" AS Lib\n20 PRINT \"before\" : CALL Lib.Outer(0)\n30 END",
    );
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.line, Some(20));
    assert!(error.source_path.as_ref().unwrap().ends_with("library.bas"));
    let diagnostic = error.display_diagnostic();
    assert!(diagnostic.lines().next().unwrap().contains("library.bas"));
    assert_underlines(&diagnostic, source, "PRINT X/Divisor");
}

#[test]
fn error_in_an_imported_single_line_function_marks_its_definition() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = "10 DEF FNRatio(Divisor)=8/Divisor";
    fs::write(root.join("library.bas"), source).unwrap();
    let mut interpreter = interpreter_at(
        root,
        "10 IMPORT \"library.bas\" AS Lib\n20 PRINT \"before\" : PRINT Lib.FNRatio(0)",
    );
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.line, Some(10));
    assert!(error.source_path.as_ref().unwrap().ends_with("library.bas"));
    assert_underlines(
        &error.display_diagnostic(),
        source,
        "DEF FNRatio(Divisor)=8/Divisor",
    );
}

#[test]
fn error_in_a_main_handler_keeps_the_called_library_location() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let source = "20 A=8 : PRINT A/Divisor";
    fs::write(
        root.join("library.bas"),
        format!("10 DEF SUB Divide(Divisor)\n{source}\n30 SUBEND\n"),
    )
    .unwrap();
    let mut interpreter = interpreter_at(
        root,
        concat!(
            "10 IMPORT \"library.bas\" AS Lib\n",
            "20 ON ERROR GOTO Recover\n",
            "30 ERROR 5\n",
            "40 END\n",
            "50 Recover:\n",
            "60 CALL Lib.Divide(0)\n",
        ),
    );
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.line, Some(20));
    let diagnostic = error.display_diagnostic();
    let header = diagnostic.lines().next().unwrap();
    assert!(header.contains("library.bas"));
    assert!(header.contains("Error in error handler:"));
    assert_underlines(&diagnostic, source, "PRINT A/Divisor");
}

#[test]
fn imported_local_and_caller_handlers_resume_in_their_own_scope() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for (resume, local, expected) in [
        ("RESUME", true, " 1  4  1  6  40\n"),
        ("RESUME NEXT", true, " 1  0  1  6  40\n"),
        ("RESUME", false, " 2  4  1  6  40\n"),
        ("RESUME NEXT", false, " 1  0  0  6  40\n"),
    ] {
        let library_handler = if local {
            format!("30 ON ERROR GOTO Recover\n50 EXIT SUB\n60 Recover:\n70 A(3)=ERR:A(4)=ERL:Divisor=2\n80 {resume}\n")
        } else {
            String::new()
        };
        fs::write(
            root.join("library.bas"),
            format!("10 DEF SUB Change(A,Divisor)\n40 A(0)=A(0)+1:A(1)=8/Divisor:A(2)=A(2)+1\n{library_handler}90 SUBEND\n"),
        )
        .unwrap();
        let mut interpreter = interpreter_at(
            root,
            &format!(
                "10 IMPORT \"library.bas\" AS Lib\n20 DIM A(4)\n30 ON ERROR GOTO Recover\n40 CALL Lib.Change(A,Divisor) : PRINT A(0);A(1);A(2);A(3);A(4)\n50 END\n60 Recover:\n70 A(3)=ERR:A(4)=ERL:Divisor=2\n80 {resume}\n"
            ),
        );
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
        assert_eq!(
            interpreter.take_output(),
            expected,
            "{resume}, local={local}"
        );
    }
}
