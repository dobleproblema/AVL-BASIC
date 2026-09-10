use avl_basic::{ErrorCode, Interpreter};

fn command(interpreter: &mut Interpreter, source: &str) {
    interpreter
        .process_immediate(source)
        .unwrap_or_else(|error| panic!("{source}: {error}"));
}

fn loaded(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    for line in source.lines() {
        command(&mut interpreter, line);
    }
    interpreter
}

#[test]
fn immediate_loop_structure_is_checked_before_any_side_effect() {
    let mut interpreter = Interpreter::new();
    for source in [
        "WHILE -1 : A=1/0 : NEXT",
        "WHILE 1 : A=1/0 : NEXT",
        "WHILE NOT 1 : A=1/0 : NEXT",
        "WHILE NOT -1 : A=1/0 : NEXT",
    ] {
        assert_eq!(
            interpreter.process_immediate(source).unwrap_err().code,
            ErrorCode::NextWithoutFor,
            "{source}"
        );
    }
    for (source, expected) in [
        ("PRINT \"BAD\":WHILE 1", ErrorCode::WhileWithoutWend),
        ("PRINT \"BAD\":FOR I=1 TO 2", ErrorCode::ForWithoutNext),
        ("PRINT \"BAD\":WEND", ErrorCode::WendWithoutWhile),
        ("PRINT \"BAD\":NEXT", ErrorCode::NextWithoutFor),
        ("FOR I=1 TO 2:WHILE 0:NEXT:WEND", ErrorCode::NextWithoutFor),
        (
            "WHILE 0:FOR I=1 TO 2:WEND:NEXT",
            ErrorCode::WendWithoutWhile,
        ),
        (
            "IF 0 THEN WHILE 1:PRINT \"BAD\"",
            ErrorCode::WhileWithoutWend,
        ),
    ] {
        assert_eq!(
            interpreter.process_immediate(source).unwrap_err().code,
            expected,
            "{source}"
        );
        assert_eq!(interpreter.take_output(), "", "{source}");
    }
}

#[test]
fn immediate_loops_skip_false_bodies_and_repeat_finite_bodies() {
    let mut interpreter = Interpreter::new();
    for source in [
        "WHILE 0:PRINT \"BAD\":WEND:PRINT \"OK\"",
        "WHILE NOT -1:A=1/0:WEND:PRINT \"OK\"",
        "FOR I=2 TO 1:PRINT \"BAD\":NEXT:PRINT \"OK\"",
        "FOR I=1 TO 2 STEP -1:PRINT \"BAD\":NEXT:PRINT \"OK\"",
        "WHILE 0:FOR I=1 TO 2:WHILE 1:PRINT \"BAD\":WEND:NEXT:WEND:PRINT \"OK\"",
        "FOR I=2 TO 1:FOR J=1 TO 2:PRINT \"BAD\":NEXT:NEXT:PRINT \"OK\"",
        "IF 1 THEN WHILE 0:PRINT \"BAD\":WEND:PRINT \"OK\" ELSE PRINT \"BAD\"",
        "IF 0 THEN PRINT \"BAD\" ELSE FOR I=2 TO 1:PRINT \"BAD\":NEXT:PRINT \"OK\"",
    ] {
        command(&mut interpreter, source);
        assert_eq!(interpreter.take_output(), "OK\n", "{source}");
    }
    for (source, expected) in [
        ("A=0:WHILE A<3:A=A+1:WEND:PRINT A", " 3\n"),
        (
            "S=0:FOR I=1 TO 2:FOR J=1 TO 3:S=S+I*J:NEXT J:NEXT I:PRINT S",
            " 18\n",
        ),
        (
            "S=0:FOR I=1 TO 2:WHILE -1:S=S+I:EXIT WHILE:WEND:NEXT:PRINT S",
            " 3\n",
        ),
        (
            "S=0:WHILE S<2:FOR I=1 TO 5:S=S+1:EXIT FOR:NEXT:WEND:PRINT S",
            " 2\n",
        ),
        ("IF 1 THEN A=0:WHILE A<2:A=A+1:WEND:PRINT A", " 2\n"),
        ("FOR I=1 TO 3:NEXT:PRINT I", " 4\n"),
    ] {
        command(&mut interpreter, source);
        assert_eq!(interpreter.take_output(), expected, "{source}");
    }
    for condition in ["1", "-1", "NOT 1"] {
        assert_eq!(
            interpreter
                .process_immediate(&format!("WHILE {condition}:A=1/0:WEND"))
                .unwrap_err()
                .code,
            ErrorCode::DivisionByZero
        );
        command(
            &mut interpreter,
            "WHILE 0:PRINT \"BAD\":WEND:PRINT \"CLEAN\"",
        );
        assert_eq!(interpreter.take_output(), "CLEAN\n");
    }
}

#[test]
fn immediate_loop_targets_do_not_come_from_loaded_program_caches() {
    for program in [
        "10 WHILE -1\n20 PRINT \"PROGRAM\"\n30 WEND\n40 END",
        "10 FOR P=1 TO 5\n20 PRINT \"PROGRAM\"\n30 NEXT\n40 END",
    ] {
        let mut interpreter = loaded(program);
        command(
            &mut interpreter,
            "WHILE 0:PRINT \"BAD\":WEND:PRINT \"WHILE\"",
        );
        command(
            &mut interpreter,
            "FOR I=2 TO 1:PRINT \"BAD\":NEXT:PRINT \"FOR\"",
        );
        assert_eq!(interpreter.take_output(), "WHILE\nFOR\n");
    }
}

#[test]
fn empty_while_rechecks_its_condition_until_false_at_prompt_and_in_programs() {
    let function = "100 DEF FNAGAIN\n110 C=C+1\n120 FNAGAIN=C<4\n130 FNEND";
    let mut interpreter = loaded(function);
    for source in [
        "C=0:WHILE FNAGAIN:WEND:PRINT C",
        "C=0:IF 1 THEN WHILE FNAGAIN:WEND:PRINT C",
    ] {
        command(&mut interpreter, source);
        assert_eq!(interpreter.take_output(), " 4\n");
    }
    let mut interpreter = loaded(&format!(
        "10 C=0:WHILE FNAGAIN:WEND:PRINT C\n20 END\n{function}"
    ));
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 4\n");
}

#[test]
fn immediate_loops_and_errors_preserve_stopped_program_scopes() {
    let mut interpreter = loaded(
        "10 S=0\n20 FOR I=1 TO 2\n30 WHILE S<3\n40 S=S+1\n50 STOP\n60 WEND\n70 NEXT\n80 PRINT S;I\n90 END",
    );
    interpreter.run_loaded().unwrap();
    interpreter.take_output();
    for resume in 0..3 {
        command(
            &mut interpreter,
            "FOR J=1 TO 2:WHILE 0:PRINT \"BAD\":WEND:NEXT:IF 1 THEN PRINT \"PROMPT\"",
        );
        assert_eq!(interpreter.take_output(), "PROMPT\n");
        assert_eq!(
            interpreter
                .process_immediate("WHILE 1:A=1/0:WEND")
                .unwrap_err()
                .code,
            ErrorCode::DivisionByZero
        );
        command(&mut interpreter, "CONT");
        let output = interpreter.take_output();
        if resume == 2 {
            assert_eq!(output, " 3  3\n");
        } else {
            assert_eq!(output, "Line 50. Program stopped.\n");
        }
    }
    assert_eq!(
        interpreter.process_immediate("CONT").unwrap_err().code,
        ErrorCode::NoStoppedProgram
    );
}

#[test]
fn run_from_an_immediate_if_keeps_new_scopes_at_the_same_stop_location() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("fresh.bas"),
        "10 X=99:N=3\n20 FOR I=1 TO N\n30 STOP\n40 NEXT\n50 PRINT I\n60 END",
    )
    .unwrap();
    let mut interpreter = loaded("10 N=1\n20 FOR I=1 TO N\n30 STOP\n40 NEXT\n50 PRINT I\n60 END");
    interpreter.root_dir = dir.path().to_path_buf();
    interpreter.current_dir = dir.path().to_path_buf();
    interpreter.run_loaded().unwrap();
    interpreter.take_output();
    command(&mut interpreter, "IF 1 THEN RUN \"fresh.bas\"");
    interpreter.take_output();
    for resume in 0..3 {
        command(&mut interpreter, "CONT");
        assert_eq!(
            interpreter.take_output(),
            if resume == 2 {
                " 4\n"
            } else {
                "Line 30. Program stopped.\n"
            }
        );
    }
}

#[test]
fn pending_interrupt_cancels_immediate_statements_without_affecting_the_next_run() {
    let mut interpreter = loaded("10 PRINT \"RUN OK\"\n20 END");
    interpreter.request_interrupt_for_test();
    let error = interpreter
        .process_immediate("WHILE -1:PRINT \"BAD\":WEND:PRINT \"UNREACHABLE\"")
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::KeyboardInterrupt);
    assert_eq!(error.line, None);
    assert_eq!(interpreter.take_output(), "");
    assert_eq!(
        interpreter.process_immediate("CONT").unwrap_err().code,
        ErrorCode::NoStoppedProgram
    );
    command(&mut interpreter, "PRINT \"PROMPT OK\"");
    command(&mut interpreter, "RUN");
    assert_eq!(interpreter.take_output(), "PROMPT OK\nRUN OK\n");
}

#[test]
fn immediate_interrupt_preserves_stopped_loops_and_open_data_channels() {
    let dir = tempfile::tempdir().unwrap();
    let mut interpreter = loaded(
        "10 OPEN \"kept.txt\" FOR OUTPUT AS #1\n20 FOR I=1 TO 2\n30 STOP\n40 WRITE #1,I\n50 NEXT\n60 END",
    );
    interpreter.root_dir = dir.path().to_path_buf();
    interpreter.current_dir = dir.path().to_path_buf();
    interpreter.run_loaded().unwrap();
    interpreter.take_output();
    interpreter.request_interrupt_for_test();
    let error = interpreter
        .process_immediate("FOR J=1 TO 10:WRITE #1,\"BAD\":NEXT:WRITE #1,\"UNREACHABLE\"")
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::KeyboardInterrupt);
    assert_eq!(error.line, None);
    command(&mut interpreter, "WRITE #1,\"PROMPT OK\"");
    command(&mut interpreter, "CONT");
    assert_eq!(interpreter.take_output(), "Line 30. Program stopped.\n");
    command(&mut interpreter, "CONT");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("kept.txt")).unwrap(),
        "\"PROMPT OK\"\n1\n2\n"
    );
}
