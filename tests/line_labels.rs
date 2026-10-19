use avl_basic::{Debugger, ErrorCode, Interpreter, RunOutcome};

fn run(source: &str, debug: bool) -> String {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    if debug {
        interpreter.set_debugger(Debugger::scripted([]));
    }
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    interpreter.take_output()
}

#[test]
fn forward_backward_and_case_insensitive_labels_preserve_numeric_lines() {
    let source = "10 GOTO Begin\n20 Again:\n30 N=N+1\n40 IF N<3 THEN GOTO aGaIn\n50 PRINT N:END\n100 Begin:\n110 GOTO Again";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 3\n");
    }
}

#[test]
fn labels_have_separate_variable_and_routine_namespaces_and_preserve_spelling() {
    let mut interpreter = Interpreter::new();
    for source in ["10 place=3:GOTO Place", "20 Place:", "30 PRINT place:END"] {
        interpreter.process_immediate(source).unwrap();
    }
    assert!(interpreter.program.list().contains("20 Place:"));
    assert!(interpreter.program.list().contains("GOTO Place"));
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 3\n");
}

#[test]
fn each_routine_and_root_can_define_the_same_label() {
    let source = "10 GOTO Done\n20 Done:\n30 CALL Work:PRINT FNValue:END\n100 DEF SUB Work\n110 GOTO Done\n120 Done:\n130 PRINT \"SUB\"\n140 SUBEND\n200 DEF FNValue\n210 GOTO Done\n220 Done:\n230 FNValue=7\n240 FNEND";
    for debug in [false, true] {
        assert_eq!(run(source, debug), "SUB\n 7\n");
    }
}

#[test]
fn symbolic_handlers_and_all_resume_modes_use_the_owning_scope() {
    for resume in ["RESUME", "RESUME 0", "RESUME NextWork", "RESUME NEXT"] {
        let source = format!("10 ON ERROR GOTO Catcher\n20 CALL Work:Tail=1\n30 NextWork:\n40 PRINT Calls;Caught;Fault;Tail:END\n80 Catcher:\n90 Caught=ERR:Fault=ERL:Ready=1:{resume}\n100 DEF SUB Work\n110 Calls=Calls+1\n120 IF Ready=0 THEN ERROR 5\n130 SUBEND");
        for debug in [false, true] {
            assert_eq!(
                run(&source, debug),
                match resume {
                    "RESUME" | "RESUME 0" => " 2  5  120  1\n",
                    "RESUME NEXT" => " 1  5  120  1\n",
                    _ => " 1  5  120  0\n",
                }
            );
        }
    }
    let source = "10 CALL Work:PRINT Result:END\n100 DEF SUB Work\n110 ON ERROR GOTO Catcher\n120 ERROR 5\n130 NextWork:\n140 Result=7:EXIT SUB\n180 Catcher:\n190 RESUME NextWork\n200 SUBEND";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 7\n");
    }
}

#[test]
fn mixed_on_lists_and_inline_gosub_use_symbolic_destinations() {
    let source = "10 ON 2 GOTO 999,Start\n20 Start:\n30 ON 1 GOSUB Helper,999\n40 IF 1 THEN GOSUB Helper:Tail=1\n50 PRINT Hits;Tail:END\n80 Helper:\n90 Hits=Hits+1:RETURN";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 2  1\n");
    }
}

#[test]
fn invalid_and_duplicate_definitions_and_undefined_references_fail_before_execution() {
    for (source, expected) in [
        ("10 PRINT \"BAD\"\n20 GOTO Missing", "Undefined label 'Missing' in scope root"),
        ("10 PRINT \"BAD\"\n20 Same:\n30 same:", "Duplicate label 'same' in scope root (first declared at line 20)"),
        ("10 PRINT \"BAD\"\n20 Name :", "Invalid label declaration 'Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 Name:PRINT 3", "Invalid label declaration 'Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 Name:REM comment", "Invalid label declaration 'Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 Name::'comment", "Invalid label declaration 'Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 Name:PRINT 3 'comment", "Invalid label declaration 'Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 Name : 'comment", "Invalid label declaration 'Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 Same:'first\n30 same: 'second", "Duplicate label 'same' in scope root (first declared at line 20)"),
        ("10 PRINT \"BAD\"\n20 Name$:", "Invalid label declaration 'Name$' in scope root"),
        ("10 PRINT \"BAD\"\n20 SIN:", "Invalid label declaration 'SIN' in scope root"),
        ("10 PRINT \"BAD\"\n20 Wrong-Name:", "Invalid label declaration 'Wrong-Name' in scope root"),
        ("10 PRINT \"BAD\"\n20 1Bad:", "Invalid label declaration '1Bad' in scope root"),
        ("10 PRINT \"BAD\"\n20 ñame:", "Invalid label declaration 'ñame' in scope root"),
        ("10 PRINT \"BAD\"\n20 GOTO RootOnly\n30 END\n100 DEF SUB Work\n110 RootOnly:\n120 SUBEND", "Undefined label 'RootOnly' in scope root"),
        ("10 RootOnly:\n20 CALL Work:END\n100 DEF SUB Work\n110 GOSUB RootOnly\n120 SUBEND", "Undefined label 'RootOnly' in scope SUB WORK"),
    ] {
        let mut interpreter = Interpreter::new();
        interpreter.program.load_text(source).unwrap();
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(error.detail.as_deref(), Some(expected), "{source}");
        assert_eq!(interpreter.take_output(), "", "{source}");
    }
}

#[test]
fn keyword_colons_and_existing_graphics_label_remain_commands() {
    assert_eq!(run("10 PRINT:\n20 END:", false), "\n");
    assert_eq!(
        run("10 DATA:\n20 REM:\n30 PRINT \"OK\":\n40 END:", false),
        "OK\n"
    );
    assert!(avl_basic::labels::label_definition("END:").is_none());
    assert!(avl_basic::labels::label_definition("LABEL \"Hello\"").is_none());
    assert!(avl_basic::labels::label_definition("Word :").is_none());
}

#[test]
fn renum_preserves_label_names_and_renumbers_later_numeric_list_targets() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text("10 ON 2 GOTO ExitHere,40\n20 ExitHere:\n30 END\n40 PRINT 7:END")
        .unwrap();
    interpreter.process_immediate("RENUM 100,10,10").unwrap();
    assert!(interpreter
        .program
        .list()
        .contains("ON 2 GOTO ExitHere,130"));
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
}

#[test]
fn edits_and_delete_invalidate_labels_and_immediate_goto_resolves_root_labels() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text("10 STOP\n20 Finish:\n30 PRINT 7:END")
        .unwrap();
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
    interpreter.take_output();
    interpreter.process_immediate("GOTO Finish").unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
    interpreter.process_immediate("DELETE 20").unwrap();
    interpreter.process_immediate("5 GOTO Finish").unwrap();
    assert_eq!(
        interpreter.run_loaded().unwrap_err().code,
        ErrorCode::TargetLineNotFound
    );
}

#[test]
fn merge_retained_tails_rebind_labels_when_the_declaration_moves() {
    for debug in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("child.bas"),
            "100 PRINT \"WRONG\":END\n200 Destination:\n210 PRINT \"NEW\":END",
        )
        .unwrap();
        let mut interpreter = Interpreter::new();
        interpreter.root_dir = directory.path().to_path_buf();
        interpreter.current_dir = directory.path().to_path_buf();
        interpreter.program.load_text("10 MERGE \"child\":GOTO Destination\n20 END\n100 Destination:\n110 PRINT \"OLD\":END").unwrap();
        if debug {
            interpreter.set_debugger(Debugger::scripted([]));
        }
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.take_output(), "NEW\n");
    }
}

#[test]
fn load_and_chain_rebuild_catalogs_without_preserving_old_labels() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("child.bas"),
        "10 GOTO ChildEntry\n20 ChildEntry:\n30 PRINT 7:END",
    )
    .unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    interpreter
        .program
        .load_text("10 OldEntry:\n20 CHAIN \"child\"")
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
    interpreter.process_immediate("5 GOTO OldEntry").unwrap();
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(
        error.detail.as_deref(),
        Some("Undefined label 'OldEntry' in scope root")
    );
    interpreter
        .load_file(&directory.path().join("child.bas"))
        .unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
}

#[test]
fn functions_use_local_symbolic_handlers_and_keep_result_continuations() {
    let source = "10 PRINT FNWork(4):END\n100 DEF FNWork(X)\n105 Entry:\n110 LOCAL D\n120 ON ERROR GOTO Catcher\n130 D=0\n140 FNWork=X/D:EXIT FN\n180 Catcher:\n190 D=2:RESUME\n200 FNEND";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 2\n");
    }
}

#[test]
fn deleted_and_duplicated_labels_in_merge_invalidate_before_using_old_bindings() {
    for child in ["100\n", "200 Destination:\n"] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("child.bas"), child).unwrap();
        let mut interpreter = Interpreter::new();
        interpreter.root_dir = directory.path().to_path_buf();
        interpreter.current_dir = directory.path().to_path_buf();
        interpreter.program.load_text("10 MERGE \"child\":GOTO Destination\n20 END\n100 Destination:\n110 PRINT \"OLD\":END").unwrap();
        let error = interpreter.run_loaded().unwrap_err();
        assert!(error
            .detail
            .as_deref()
            .unwrap()
            .contains(if child.starts_with("100") {
                "Undefined label"
            } else {
                "Duplicate label"
            }));
        assert_eq!(interpreter.take_output(), "");
    }
}

#[test]
fn numeric_gosub_root_helpers_stay_available_without_symbolic_scope_fallback() {
    let source = "10 CALL Work:PRINT Hits:END\n80 Hits=Hits+1:RETURN\n100 DEF SUB Work\n110 GOSUB 80\n120 SUBEND";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 1\n");
    }
}

#[test]
fn symbolic_goto_uses_normal_block_if_jump_reconciliation() {
    let source = "10 FOR N=1 TO 2\n20 IF N=1 THEN\n30 GOTO Outside\n40 ENDIF\n50 Hits=Hits+1\n60 Outside:\n70 NEXT N\n80 PRINT Hits;N:END";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 1  3\n");
    }
}

#[test]
fn active_symbolic_error_policy_follows_a_label_moved_by_merge() {
    for debug in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("child.bas"), "100 REM old declaration removed\n110 PRINT \"WRONG\":END\n200 Recover:\n210 Caught=ERR:Fault=ERL:RESUME NEXT").unwrap();
        let mut interpreter = Interpreter::new();
        interpreter.root_dir = directory.path().to_path_buf();
        interpreter.current_dir = directory.path().to_path_buf();
        interpreter.program.load_text("10 ON ERROR GOTO Recover\n20 MERGE \"child\"\n30 ERROR 5\n40 PRINT Caught;Fault:END\n100 Recover:\n110 RESUME NEXT").unwrap();
        if debug {
            interpreter.set_debugger(Debugger::scripted([]));
        }
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.take_output(), " 5  30\n");
    }
}

#[test]
fn trace_shows_label_names_without_changing_numeric_line_markers() {
    let source = "10 TRON\n20 JumpHere:\n30 PRINT 7\n40 END";
    for debug in [false, true] {
        assert_eq!(run(source, debug), "[20 JumpHere:][30] 7\n[40]\n");
    }
}

#[test]
fn symbolic_control_operands_accept_tabs_and_multiple_spaces() {
    for separator in ["\t", "  "] {
        let source = format!("10 GOTO{separator}Start\n20 Start:\n30 GOSUB{separator}Helper\n40 ON 1 GOSUB{separator}Helper,999\n50 ON 2 GOTO{separator}999,Ready\n60 Ready:\n70 ON ERROR GOTO{separator}Catcher\n80 ERROR 5\n90 PRINT Hits;Caught:END\n100 Helper:\n110 Hits=Hits+1:RETURN\n200 Catcher:\n210 Caught=ERR:RESUME{separator}AfterError\n220 AfterError:\n230 RESUME NEXT");
        // RESUME to AfterError consumes the pending error. Jump explicitly back
        // to normal work rather than issuing another RESUME there.
        let source = source.replace("230 RESUME NEXT", "230 GOTO 90");
        for debug in [false, true] {
            assert_eq!(run(&source, debug), " 2  5\n");
        }
    }
}

#[test]
fn on_selector_variable_starting_with_sq_is_not_an_audio_event() {
    let source = "10 SQValue=1:ON SQValue GOTO Finish\n20 PRINT \"BAD\":END\n30 Finish:\n40 PRINT \"OK\":END";
    for debug in [false, true] {
        assert_eq!(run(source, debug), "OK\n");
    }
}

#[test]
fn declarations_accept_apostrophe_comments_and_preserve_them_in_source() {
    for comment in [
        "'comment",
        " ' comment: PRINT 999 : REM \"quotes\"",
        "\t' español ñ 🙂",
    ] {
        let source = format!(
            "10 GOTO Finish\n20 PRINT \"WRONG\":END\n100 Finish:{comment}\n110 PRINT \"OK\":END"
        );
        assert_eq!(
            avl_basic::labels::label_definition(&format!("Finish:{comment}")),
            Some("Finish")
        );
        for debug in [false, true] {
            assert_eq!(run(&source, debug), "OK\n");
        }
        let mut interpreter = Interpreter::new();
        for line in source.lines() {
            interpreter.process_immediate(line).unwrap();
        }
        assert!(
            interpreter
                .program
                .list()
                .contains(&format!("100 Finish:{comment}")),
            "{comment:?}: {}",
            interpreter.program.list()
        );
        interpreter.process_immediate("RENUM 1000,10,10").unwrap();
        assert!(interpreter
            .program
            .list()
            .contains(&format!("1020 Finish:{comment}")));
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.take_output(), "OK\n");
    }
}

#[test]
fn commented_declarations_work_with_local_root_handlers_and_leading_locals() {
    let source = "10 ON ERROR GOTO RootHandler\n20 CALL Work\n30 PRINT Hits;Caught;Fault:END\n80 RootHandler:'root recovery\n90 Caught=ERR:Fault=ERL:RESUME NEXT\n100 DEF SUB Work\n105 Entry: 'before LOCAL\n110 LOCAL D\n120 ON ERROR GOTO LocalHandler\n130 ERROR 5\n140 ERROR 6\n150 EXIT SUB\n180 LocalHandler:'local recovery\n190 Hits=Hits+1:ON ERROR GOTO 0:RESUME NEXT\n200 SUBEND";
    for debug in [false, true] {
        assert_eq!(run(source, debug), " 1  6  140\n");
    }
}

#[test]
fn trace_uses_the_label_name_while_preserving_its_comment_only_in_source() {
    let source = "10 TRON\n20 JumpHere:'not part of the trace\n30 PRINT 7\n40 END";
    for debug in [false, true] {
        assert_eq!(run(source, debug), "[20 JumpHere:][30] 7\n[40]\n");
    }
}
