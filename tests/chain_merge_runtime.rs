use avl_basic::{DebugAction, Debugger, ErrorCode, Interpreter, RunOutcome};
use std::fs;

fn setup(source: &str, child: &str) -> (tempfile::TempDir, Interpreter) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("parent.bas"), source).unwrap();
    fs::write(directory.path().join("child.bas"), child).unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    interpreter
        .load_file(&directory.path().join("parent.bas"))
        .unwrap();
    (directory, interpreter)
}

#[test]
fn chain_from_a_stopped_prompt_starts_the_child_and_discards_the_old_if() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        let (_directory, mut interpreter) = setup(
            "10 A=7\n20 IF 1 THEN\n30 STOP\n40 ENDIF\n50 END",
            "200 PRINT \"FRESH\";A",
        );
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
        assert_eq!(interpreter.take_output(), "Line 30. Program stopped.\n");
        interpreter
            .process_immediate(&format!("{command} \"child.bas\",200"))
            .unwrap();
        assert_eq!(interpreter.take_output(), "FRESH 7\n");
        assert_eq!(
            interpreter.process_immediate("CONT").unwrap_err().code,
            ErrorCode::NoStoppedProgram
        );
    }
}

#[test]
fn immediate_chain_preserves_variables_arrays_and_open_channel_position() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        let (directory, mut interpreter) = setup(
            "10 END",
            "200 LINE INPUT #1,T$\n210 PRINT A;S$;B(1);T$\n220 STOP\n230 PRINT \"DONE\"\n240 END",
        );
        fs::write(directory.path().join("data.txt"), "first\nsecond\n").unwrap();
        interpreter
            .process_immediate("A=7:S$=\"kept\":DIM B(1):B(1)=9")
            .unwrap();
        interpreter
            .process_immediate("OPEN \"data.txt\" FOR INPUT AS #1")
            .unwrap();
        interpreter.process_immediate("LINE INPUT #1,T$").unwrap();
        interpreter
            .process_immediate(&format!("{command} \"child.bas\",200:PRINT \"STALE\""))
            .unwrap();
        assert_eq!(
            interpreter.take_output(),
            " 7 kept 9 second\nLine 220. Program stopped.\n"
        );
        interpreter.process_immediate("CONT").unwrap();
        assert_eq!(interpreter.take_output(), "DONE\n");
    }
}

#[test]
fn immediate_chain_inside_inline_if_transfers_even_when_the_cursor_is_unchanged() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        let (_directory, mut interpreter) = setup("", "10 PRINT \"CHILD\"");
        interpreter
            .process_immediate(&format!(
                "IF 1 THEN {command} \"child.bas\":PRINT \"STALE\""
            ))
            .unwrap();
        assert_eq!(interpreter.take_output(), "CHILD\n");
    }
}

#[test]
fn program_chain_discards_old_for_while_gosub_and_if_frames() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        for (child_command, expected) in [
            ("NEXT I", ErrorCode::NextWithoutFor),
            ("WEND", ErrorCode::WendWithoutWhile),
            ("RETURN", ErrorCode::ReturnWithoutGosub),
            ("ENDIF", ErrorCode::EndIfWithoutIf),
        ] {
            let parent = format!(
                "10 FOR I=1 TO 1\n20 WHILE 1\n30 IF 1 THEN\n40 GOSUB 100\n50 ENDIF\n60 WEND\n70 NEXT I\n80 END\n100 {command} \"child.bas\",200\n110 RETURN"
            );
            let (_directory, mut interpreter) =
                setup(&parent, &format!("200 {child_command}\n210 END"));
            let error = interpreter.run_loaded().unwrap_err();
            assert_eq!(
                (error.code, error.line),
                (expected, Some(200)),
                "{command}: {child_command}"
            );
        }
    }
}

#[test]
fn chain_clears_handlers_error_state_and_scheduled_interrupts() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        let (_directory, mut interpreter) = setup(
            "10 AFTER 10000,1 GOSUB 300\n20 ON ERROR GOTO 100\n30 A=1/0\n40 END\n100 STOP\n110 END\n300 RETURN",
            "200 PRINT ERR;ERL\n210 PRINT 1/0\n220 END",
        );
        interpreter.run_loaded().unwrap();
        interpreter.take_output();
        let mut debugger = Debugger::scripted([DebugAction::Continue]);
        debugger.replace_breakpoints([200]);
        interpreter.set_debugger(debugger);
        let error = interpreter
            .process_immediate(&format!("{command} \"child.bas\",200"))
            .unwrap_err();
        assert_eq!(
            (error.code, error.line),
            (ErrorCode::DivisionByZero, Some(210))
        );
        assert_eq!(interpreter.take_output(), " 0  0\n");
        let snapshot = &interpreter.debugger().unwrap().snapshots()[0];
        assert!(snapshot.timers.is_empty());
        assert_eq!(snapshot.stack.len(), 1);
        assert_eq!((snapshot.err, snapshot.erl), (0, 0));
    }
}

#[test]
fn chain_from_inline_gosub_does_not_return_to_the_old_clause_at_child_eof() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        for debug in [false, true] {
            let source = format!(
                "10 IF 1 THEN GOSUB 100:PRINT \"STALE\"\n20 END\n100 {command} \"child.bas\",500"
            );
            let (_directory, mut interpreter) = setup(&source, "500 PRINT \"CHILD\"");
            if debug {
                interpreter.set_debugger(Debugger::scripted([]));
            }
            assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
            assert_eq!(
                interpreter.take_output(),
                "CHILD\n",
                "{command}, debugger={debug}"
            );
        }
    }
}

#[test]
fn child_stop_keeps_its_own_gosub_return_after_chaining_from_inline_gosub() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        let source = format!(
            "10 IF 1 THEN GOSUB 100:PRINT \"STALE\"\n20 END\n100 {command} \"child.bas\",500"
        );
        let (_directory, mut interpreter) = setup(
            &source,
            "500 GOSUB 700\n510 PRINT \"DONE\"\n520 END\n700 PRINT \"CHILD\"\n710 STOP\n720 RETURN",
        );
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
        assert_eq!(
            interpreter.take_output(),
            "CHILD\nLine 710. Program stopped.\n"
        );
        interpreter.process_immediate("CONT").unwrap();
        assert_eq!(interpreter.take_output(), "DONE\n");
    }
}

#[test]
fn failed_file_reads_preserve_the_program_catalog_channel_and_continuation() {
    for operation in ["CHAIN", "MERGE", "CHAIN MERGE"] {
        for failure in ["missing", "malformed", "encoding"] {
            let (directory, mut interpreter) = setup(
                "10 A=7\n20 IF 1 THEN\n30 STOP\n40 PRINT FNF(A)\n50 ENDIF\n60 END\n100 DEF FNF(X)=X+1",
                "200 PRINT 99",
            );
            match failure {
                "malformed" => fs::write(
                    directory.path().join("broken.bas"),
                    "200 PRINT 99\nnot a numbered line\n",
                )
                .unwrap(),
                "encoding" => {
                    fs::write(directory.path().join("broken.bas"), b"200 PRINT 99\n\xff\n").unwrap()
                }
                _ => {}
            }
            fs::write(directory.path().join("data.txt"), "kept\n").unwrap();
            interpreter.run_loaded().unwrap();
            interpreter.take_output();
            interpreter
                .process_immediate("OPEN \"data.txt\" FOR INPUT AS #1")
                .unwrap();
            let old_program = interpreter.program.list();
            let suffix = if operation == "CHAIN MERGE" {
                ",200,DELETE 20-60"
            } else {
                ""
            };
            let error = interpreter
                .process_immediate(&format!("{operation} \"broken.bas\"{suffix}"))
                .unwrap_err();
            let expected = if failure == "missing" {
                ErrorCode::FileNotFound
            } else {
                ErrorCode::InvalidLineFormat
            };
            assert_eq!(error.code, expected, "{operation}, {failure}");
            assert_eq!(interpreter.program.list(), old_program);
            interpreter
                .process_immediate("LINE INPUT #1,T$:PRINT T$;FNF(2)")
                .unwrap();
            assert_eq!(interpreter.take_output(), "kept 3\n");
            interpreter.process_immediate("CONT").unwrap();
            assert_eq!(interpreter.take_output(), " 8\n");
        }
    }
}

#[test]
fn merge_inside_a_program_preserves_its_active_control_and_error_handler() {
    let (directory, mut interpreter) = setup(
        "10 ON ERROR GOTO 100\n20 FOR I=1 TO 2\n30 WHILE I<3\n40 IF 1 THEN\n50 MERGE \"extra.bas\"\n60 T=T+I:A=1/0\n70 ENDIF\n80 EXIT WHILE\n90 WEND:NEXT I:PRINT T:END\n100 RESUME NEXT",
        "",
    );
    fs::write(directory.path().join("extra.bas"), "200 DEF FNF(X)=X+1").unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 3\n");
}

#[test]
fn merge_restarts_data_after_overwriting_a_data_line() {
    let (_directory, mut interpreter) = setup("10 DATA 1,2\n20 END", "10 DATA 7,8");
    interpreter.run_loaded().unwrap();
    interpreter.process_immediate("READ A:PRINT A").unwrap();
    assert_eq!(interpreter.take_output(), " 1\n");
    interpreter
        .process_immediate("MERGE \"child.bas\":READ A:PRINT A")
        .unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
    interpreter
        .process_immediate("RESTORE 10:READ A:PRINT A")
        .unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
}

#[test]
fn merge_retains_changed_source_across_calls_errors_and_stop() {
    let cases = [
        (
            "10 MERGE \"child\":CALL WORK:GOSUB 100:PRINT \"TAIL\"\n20 PRINT \"DONE\":END\n100 PRINT \"SUB\":RETURN\n200 DEF SUB WORK\n210 PRINT \"WORK\"\n220 SUBEND",
            "WORK\nSUB\nTAIL\nDONE\n",
        ),
        (
            "5 ON ERROR GOTO 100\n10 ERROR 15:PRINT \"TAIL\"\n20 PRINT \"DONE\":END\n100 MERGE \"child\":PRINT ERL:RESUME NEXT",
            " 10\nTAIL\nDONE\n",
        ),
    ];
    for debugger in [false, true] {
        for child in ["10\n", "10 PRINT \"REPLACEMENT\"\n"] {
            for (source, expected) in cases {
                let (_directory, mut interpreter) = setup(source, child);
                if debugger {
                    interpreter.set_debugger(Debugger::scripted([]));
                }
                interpreter.run_loaded().unwrap();
                assert_eq!(interpreter.take_output(), expected);
            }
            let (_directory, mut interpreter) = setup(
                "10 MERGE \"child\":STOP:PRINT \"TAIL\"\n20 PRINT \"DONE\":END",
                child,
            );
            if debugger {
                interpreter.set_debugger(Debugger::scripted([]));
            }
            assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
            interpreter.take_output();
            interpreter.process_immediate("CONT").unwrap();
            assert_eq!(interpreter.take_output(), "TAIL\nDONE\n");
        }
    }
}

#[test]
fn goto_after_merge_uses_the_new_source_at_the_same_line_number() {
    for debugger in [false, true] {
        let (_directory, mut interpreter) =
            setup("10 MERGE \"child\":GOTO 10\n20 END", "10 PRINT \"NEW\":END");
        if debugger {
            interpreter.set_debugger(Debugger::scripted([]));
        }
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.take_output(), "NEW\n");
    }
}

#[test]
fn restore_a_deleted_data_line_reports_no_data() {
    let (_directory, mut interpreter) = setup("10 DATA 1\n20 END", "200 END\n300 DATA 9");
    interpreter
        .process_immediate("CHAIN MERGE \"child.bas\",200,DELETE 10-10")
        .unwrap();
    assert_eq!(
        interpreter
            .process_immediate("RESTORE 10")
            .unwrap_err()
            .code,
        ErrorCode::NoData
    );
    interpreter
        .process_immediate("RESTORE 300:READ A:PRINT A")
        .unwrap();
    assert_eq!(interpreter.take_output(), " 9\n");
}

#[test]
fn immediate_chain_rejects_routine_body_entry_at_the_requested_line() {
    for command in ["CHAIN", "CHAIN MERGE"] {
        let (_directory, mut interpreter) =
            setup("10 END", "200 END\n300 DEF FNF(X)\n310 FNF=X+1\n320 FNEND");
        let error = interpreter
            .process_immediate(&format!("{command} \"child.bas\",310"))
            .unwrap_err();
        assert_eq!(
            (error.code, error.line),
            (ErrorCode::InvalidTargetLine, Some(310))
        );
    }
}

#[test]
fn cont_after_stop_at_end_of_deleted_merged_line_runs_its_inserted_successor() {
    for debug in [false, true] {
        let (_directory, mut interpreter) = setup(
            "10 MERGE \"child\":STOP\n20 PRINT \"DONE\":END",
            "10\n15 PRINT \"MID\"\n",
        );
        if debug {
            interpreter.set_debugger(Debugger::scripted([]));
        }

        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
        assert_eq!(
            interpreter.take_output(),
            "Line 10. Program stopped.\n",
            "debugger={debug}"
        );
        interpreter.process_immediate("CONT").unwrap();
        assert_eq!(interpreter.take_output(), "MID\nDONE\n", "debugger={debug}");
    }
}
