use avl_basic::{ErrorCode, Interpreter};

fn loaded(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    for line in source.lines() {
        interpreter.process_immediate(line).unwrap();
    }
    interpreter
}

fn output(source: &str) -> String {
    let mut interpreter = loaded(source);
    interpreter.run_loaded().unwrap();
    interpreter.take_output()
}

#[test]
fn calls_before_definitions_and_end_resolve_all_routine_kinds() {
    assert_eq!(
        output(
            "10 A=2:PRINT FNOUTER(3)\n20 CALL OUTER(4)\n30 A=5:PRINT FNINNER(3)\n40 END\n100 DEF FNOUTER(X)\n110 LOCAL T\n120 T=FNINNER(X)\n130 FNOUTER=T+1\n140 FNEND\n200 DEF SUB OUTER(N)\n210 CALL INNER(N)\n220 SUBEND\n300 DEF SUB INNER(N)\n310 PRINT FNINNER(N)\n320 SUBEND\n400 DEF FNINNER(X)=A*X"
        ),
        " 7\n 8\n 15\n"
    );
}

#[test]
fn zero_argument_string_and_array_returns_are_available_before_definitions() {
    assert_eq!(
        output(
            "10 PRINT FNWORD$;FNZERO\n20 DIM A(1),B(1):A(0)=4:A(1)=6\n30 MAT B=FNCOPY(A)\n40 PRINT B(0);B(1)\n50 END\n100 DEF FNWORD$=\"ready\"\n110 DEF FNZERO=7\n200 DEF FNCOPY(X)\n210 MAT FNCOPY=X\n220 FNEND"
        ),
        "ready 7\n 4  6\n"
    );
}

#[test]
fn local_dimensions_and_function_bodies_are_evaluated_only_when_called() {
    assert_eq!(
        output(
            "10 CALL BUFFER(3)\n20 END\n100 DEF SUB BUFFER(N)\n110 LOCAL A(N)\n120 A(N)=N:PRINT A(N)\n130 SUBEND\n200 DEF FNBAD(X)=X+\n300 DEF SUB UNUSED\n310 LOCAL A(1/0)\n320 SUBEND"
        ),
        " 3\n"
    );
    let mut interpreter = loaded("10 PRINT 1\n20 PRINT FNBAD(3)\n30 END\n100 DEF FNBAD(X)=X+");
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!((error.code, error.line), (ErrorCode::Syntax, Some(20)));
    assert_eq!(interpreter.take_output(), " 1\n");
}

#[test]
fn duplicate_declarations_fail_before_any_program_statement() {
    for source in [
        "10 PRINT 99\n20 END\n100 DEF FNF(X)=X\n200 DEF fnf(X)=X+1",
        "10 PRINT 99\n20 DEF FNF(X)=X\n30 DEF FNF(X)\n40 FNF=X\n50 FNEND",
        "10 PRINT 99\n20 DEF SUB WORK\n30 SUBEND\n40 DEF SUB work\n50 SUBEND",
        "10 PRINT 99\n20 DEF FNF=1:DEF FNF=2",
    ] {
        let mut interpreter = loaded(source);
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::DuplicateRoutineDefinition
        );
        assert_eq!(interpreter.take_output(), "", "{source}");
    }
}

#[test]
fn function_and_subroutine_names_keep_their_separate_namespaces() {
    assert_eq!(
        output("10 PRINT FNF(2):CALL FNF(3)\n20 END\n100 DEF FNF(X)=X+1\n200 DEF SUB FNF(N)\n210 PRINT N+10\n220 SUBEND"),
        " 3\n 13\n"
    );
}

#[test]
fn conditional_and_loop_declarations_fail_even_when_unreachable() {
    for source in [
        "10 END\n20 IF 0 THEN DEF FNF(X)=X",
        "10 END\n20 IF 1 THEN PRINT 0 ELSE DEF FNF(X)=X",
        "10 END\n20 IF 0 THEN IF 1 THEN DEF FNF(X)=X ELSE PRINT 0",
        "10 END\n20 IF 0 THEN PRINT 0:DEF FNF(X)=X",
        "10 END\n20 IF 0 THEN\n30 DEF FNF(X)=X\n40 END IF",
        "10 END\n20 FOR I=1 TO 0\n30 DEF FNF(X)=X\n40 NEXT I",
        "10 END\n20 WHILE 0\n30 DEF SUB WORK\n40 SUBEND\n50 WEND",
    ] {
        let mut interpreter = loaded(source);
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::RoutineDefinitionNotAtTopLevel,
            "{source}"
        );
        assert_eq!(interpreter.take_output(), "");
    }
}

#[test]
fn static_scope_accounts_for_every_name_in_a_next_list() {
    let mut interpreter =
        loaded("10 FOR I=1 TO 1\n20 FOR J=1 TO 1\n30 NEXT J,I\n40 DEF FNF(X)=X\n50 PRINT FNF(2)");
    interpreter.process_immediate("PRINT FNF(2)").unwrap();
    assert_eq!(interpreter.take_output(), " 2\n");
    // The existing runtime does not implement NEXT lists. Its own diagnostic
    // must not be replaced by a spurious routine-placement error at line 40.
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(
        (error.code, error.line),
        (ErrorCode::NextWithoutFor, Some(30))
    );
}

#[test]
fn next_naming_an_outer_loop_closes_its_inner_declaration_scopes() {
    for next in ["NEXT I", "NEXT j:NEXT i", "NEXT:NEXT"] {
        assert_eq!(
            output(&format!(
                "10 FOR I=1 TO 1\n20 FOR J=1 TO 1\n30 {next}\n40 DEF FNF(X)=X\n50 PRINT FNF(2)"
            )),
            " 2\n"
        );
    }
}

#[test]
fn nested_declarations_keep_the_containing_routine_error() {
    for (source, code) in [
        (
            "10 END\n20 DEF FNF(X)\n30 DEF FNG(X)=X\n40 FNEND",
            ErrorCode::FunctionForbidden,
        ),
        (
            "10 END\n20 DEF SUB WORK\n30 IF 0 THEN DEF FNG(X)=X\n40 SUBEND",
            ErrorCode::SubroutineForbidden,
        ),
    ] {
        let mut interpreter = loaded(source);
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!((error.code, error.line), (code, Some(30)));
    }
}

#[test]
fn declarations_do_not_follow_goto_flow_or_repeat_when_revisited() {
    assert_eq!(
        output("10 GOTO 30\n20 DEF FNF(X)=X+1\n30 PRINT FNF(3)\n40 END"),
        " 4\n"
    );
    assert_eq!(
        output("10 I=I+1\n20 DEF FNF(X)=X+1\n30 PRINT FNF(I)\n40 IF I<2 THEN GOTO 10"),
        " 2\n 3\n"
    );
    assert_eq!(
        output("10 PRINT 1\n20 DEF SUB WORK\n30 PRINT 99\n40 SUBEND\n50 PRINT 2"),
        " 1\n 2\n"
    );
}

#[test]
fn run_from_line_and_cont_use_the_complete_catalog() {
    let mut interpreter = loaded(
        "10 PRINT 99\n20 PRINT FNF(3)\n30 STOP\n40 PRINT FNF(4)\n50 END\n100 DEF FNF(X)=X+1",
    );
    interpreter.run_loaded_from(Some(20)).unwrap();
    assert_eq!(interpreter.take_output(), " 4\nLine 30. Program stopped.\n");
    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), " 5\n");
    interpreter.run_loaded_from(Some(40)).unwrap();
    assert_eq!(interpreter.take_output(), " 5\n");
}

#[test]
fn early_jumps_and_run_entry_cannot_enter_routine_bodies() {
    for transfer in ["GOTO 110", "GOSUB 110"] {
        let mut interpreter = loaded(&format!(
            "10 {transfer}\n20 END\n100 DEF SUB WORK\n110 PRINT 99\n120 SUBEND"
        ));
        let error = interpreter.run_loaded().unwrap_err();
        assert_eq!(
            (error.code, error.line),
            (ErrorCode::InvalidTargetLine, Some(10))
        );
        assert_eq!(interpreter.take_output(), "");
    }
    let mut interpreter = loaded("10 END\n100 DEF FNF(X)\n110 FNF=X\n120 FNEND");
    assert_eq!(
        interpreter.run_loaded_from(Some(110)).unwrap_err().code,
        ErrorCode::InvalidTargetLine
    );
    let mut interpreter = loaded("10 STOP\n20 END\n100 DEF SUB WORK\n110 PRINT 99\n120 SUBEND");
    interpreter.run_loaded().unwrap();
    assert_eq!(
        interpreter.process_immediate("GOTO 110").unwrap_err().code,
        ErrorCode::InvalidTargetLine
    );
    interpreter.process_immediate("CONT").unwrap();
}

#[test]
fn immediate_calls_rebuild_after_edits_deletion_and_renumbering() {
    let mut interpreter =
        loaded("100 DEF SUB WORK\n110 PRINT FNF(2)\n120 SUBEND\n200 DEF FNF(X)=X+1");
    interpreter.process_immediate("CALL WORK").unwrap();
    assert_eq!(interpreter.take_output(), " 3\n");
    interpreter.process_immediate("200 DEF FNF(X)=X+5").unwrap();
    interpreter.process_immediate("RENUM 10,10").unwrap();
    interpreter.process_immediate("CALL WORK").unwrap();
    assert_eq!(interpreter.take_output(), " 7\n");
    interpreter.process_immediate("DELETE 40").unwrap();
    assert_eq!(
        interpreter.process_immediate("CALL WORK").unwrap_err().code,
        ErrorCode::Undefined
    );
}

#[test]
fn clear_and_run_reset_direct_definitions_but_keep_program_declarations() {
    let mut interpreter = loaded("10 PRINT FNF(2)\n20 END\n100 DEF FNF(X)=X+1");
    interpreter
        .process_immediate("DEF FNDIRECT(X)=X+10")
        .unwrap();
    assert_eq!(
        interpreter
            .process_immediate("DEF FNDIRECT(X)=X+20")
            .unwrap_err()
            .code,
        ErrorCode::DuplicateRoutineDefinition
    );
    assert_eq!(
        interpreter
            .process_immediate("DEF FNF(X)=X+20")
            .unwrap_err()
            .code,
        ErrorCode::DuplicateRoutineDefinition
    );
    interpreter.process_immediate("CLEAR").unwrap();
    interpreter.process_immediate("PRINT FNF(2)").unwrap();
    assert_eq!(interpreter.take_output(), " 3\n");
    assert_eq!(
        interpreter
            .process_immediate("PRINT FNDIRECT(2)")
            .unwrap_err()
            .code,
        ErrorCode::Undefined
    );
    interpreter
        .process_immediate("DEF FNDIRECT(X)=X+10")
        .unwrap();
    interpreter.run_loaded().unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 3\n 3\n");
    assert_eq!(
        interpreter
            .process_immediate("PRINT FNDIRECT(2)")
            .unwrap_err()
            .code,
        ErrorCode::Undefined
    );
}

#[test]
fn incomplete_program_edits_are_accepted_and_failed_catalogs_do_not_leak() {
    let mut interpreter = loaded("10 DEF FNG(X)=X+1\n20 DEF SUB WORK");
    assert_eq!(
        interpreter
            .process_immediate("PRINT FNG(1)")
            .unwrap_err()
            .code,
        ErrorCode::SubEndWithoutDef
    );
    interpreter.process_immediate("30 SUBEND").unwrap();
    interpreter.process_immediate("PRINT FNG(1)").unwrap();
    assert_eq!(interpreter.take_output(), " 2\n");
}

#[test]
fn load_and_chain_replace_catalogs_and_merge_rebuilds_the_resulting_program() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("next.bas"),
        "10 PRINT FNNEXT(A)\n20 END\n100 DEF FNNEXT(X)=X+10",
    )
    .unwrap();
    std::fs::write(directory.path().join("extra.bas"), "200 DEF FNF(X)=X+7").unwrap();
    let mut interpreter = loaded("10 A=3\n20 CHAIN \"next.bas\"\n100 DEF FNOLD(X)=X");
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 13\n");
    assert_eq!(
        interpreter
            .process_immediate("PRINT FNOLD(3)")
            .unwrap_err()
            .code,
        ErrorCode::Undefined
    );
    interpreter
        .process_immediate("MERGE \"extra.bas\"")
        .unwrap();
    interpreter.process_immediate("PRINT FNF(2)").unwrap();
    assert_eq!(interpreter.take_output(), " 9\n");
    interpreter.process_immediate("LOAD \"next.bas\"").unwrap();
    assert_eq!(
        interpreter
            .process_immediate("PRINT FNF(2)")
            .unwrap_err()
            .code,
        ErrorCode::Undefined
    );
    interpreter.process_immediate("PRINT FNNEXT(2)").unwrap();
    assert_eq!(interpreter.take_output(), " 12\n");
}

#[test]
fn runtime_merge_registers_new_routines_and_same_line_edits_replace_the_catalog() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("extra.bas"), "200 DEF FNF(X)=X+7").unwrap();
    let mut interpreter = loaded("10 MERGE \"extra.bas\"\n20 PRINT FNF(2)\n30 END");
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 9\n");
    std::fs::write(directory.path().join("extra.bas"), "200 DEF FNF(X)=X+10").unwrap();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 12\n");
    std::fs::write(directory.path().join("extra.bas"), "300 DEF FNF(X)=X+20").unwrap();
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(
        (error.code, error.line),
        (ErrorCode::DuplicateRoutineDefinition, Some(300))
    );
    assert_eq!(interpreter.take_output(), "");
}

#[test]
fn chain_merge_registers_forward_calls_and_protects_routine_entry_lines() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("extra.bas"),
        "200 PRINT FNF(2)\n210 END\n300 DEF FNF(X)=X+7\n400 DEF SUB WORK\n410 PRINT 99\n420 SUBEND",
    )
    .unwrap();
    let mut interpreter = loaded("10 CHAIN MERGE \"extra.bas\",200\n20 END");
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    interpreter.run_loaded().unwrap();
    assert_eq!(interpreter.take_output(), " 9\n");
    for command in ["CHAIN", "CHAIN MERGE"] {
        let mut interpreter = loaded(&format!("10 {command} \"extra.bas\",410"));
        interpreter.root_dir = directory.path().to_path_buf();
        interpreter.current_dir = directory.path().to_path_buf();
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::InvalidTargetLine
        );
        assert_eq!(interpreter.take_output(), "");
    }
}

#[test]
fn chain_arguments_use_source_functions_before_the_catalog_changes() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("next.bas"),
        "500 PRINT \"arrived\"\n510 END",
    )
    .unwrap();
    for command in ["CHAIN", "CHAIN MERGE"] {
        let mut interpreter = loaded(&format!(
            "10 {command} FNFILE$,FNENTRY\n20 END\n100 DEF FNFILE$=\"next.bas\"\n110 DEF FNENTRY=500"
        ));
        interpreter.root_dir = directory.path().to_path_buf();
        interpreter.current_dir = directory.path().to_path_buf();
        interpreter.run_loaded().unwrap();
        assert_eq!(interpreter.take_output(), "arrived\n");
    }
}

#[test]
fn declaration_words_inside_comments_and_strings_are_not_definitions() {
    assert_eq!(
        output("10 PRINT \"DEF FNF(X)=X\"\n20 REM DEF FNF(X)=X\n30 IF 0 THEN PRINT \"DEF SUB WORK\"\n35 IF 0 THEN REM DEF FNF(X)=X\n36 END IF\n40 PRINT FNF(2) ' DEF FNF(X)=X+99\n100 DEF FNF(X)=X+1"),
        "DEF FNF(X)=X\n 3\n"
    );
}
