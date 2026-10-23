//! Public-API contracts for labels in existing line-argument positions.

use avl_basic::{console, ErrorCode, Interpreter, RunOutcome};

fn interpreter(source: &str) -> Interpreter {
    let mut basic = Interpreter::new();
    basic.program.load_text(source).unwrap();
    basic
}

fn run(source: &str) -> String {
    let mut basic = interpreter(source);
    assert_eq!(basic.run_loaded().unwrap(), RunOutcome::End);
    basic.take_output()
}

#[test]
fn restore_uses_numeric_variables_even_when_a_label_has_the_same_name() {
    assert_eq!(
        run("10 Records=310\n20 RESTORE Records:READ A\n30 RESTORE (Records):READ B\n40 RESTORE Records+0:READ C\n50 PRINT A;B;C:END\n100 Records:\n110 DATA 11\n310 DATA 22"),
        " 22  22  22\n"
    );
}

#[test]
fn restore_numeric_variables_use_their_ordinary_sub_and_fn_scopes() {
    assert_eq!(
        run("10 Records=110:CALL Work:PRINT FNFetch(230)\n20 RESTORE Records:READ A:PRINT A:END\n100 Records:\n110 DATA 7\n200 DEF SUB Work\n210 LOCAL Records:Records=230\n220 RESTORE Records:READ A:PRINT A\n225 Records:\n230 DATA 9\n240 SUBEND\n300 DEF FNFetch(Records)\n305 Records:\n310 RESTORE Records:READ A\n320 FNFetch=A\n330 FNEND"),
        " 9\n 9\n 7\n"
    );
}

#[test]
fn restore_requires_data_on_the_exact_numeric_line_without_a_later_search() {
    for operand in ["100", "Records", "(Records)", "Records+0", "Missing"] {
        let mut basic = interpreter(&format!(
            "10 Records=100:RESTORE {operand}\n20 END\n100 Records:\n110 DATA 7"
        ));
        assert_eq!(
            basic.run_loaded().unwrap_err().code,
            ErrorCode::NoData,
            "{operand}"
        );
    }
}

#[test]
fn restore_keeps_numeric_expression_grammar_and_the_data_catalog() {
    for data in [
        "DATA\"first\"",
        "data\"first\"",
        "   DATA\"first\"",
        "\tdata\"first\"",
    ] {
        for operand in ["110", "Records", "(Records)", "Records+0", "+110", "&H6E"] {
            assert_eq!(
                run(&format!("10 Records=110:RESTORE {operand}:READ A$\n20 PRINT A$:END\n100 Records:\n110 {data}\n120 DATA \"later\"")),
                "first\n",
                "{data:?}, {operand}"
            );
        }
    }
}

#[test]
fn bare_restore_resets_to_the_first_value() {
    assert_eq!(
        run("10 READ A:READ B\n20 RESTORE:READ C\n30 PRINT A;B;C:END\n100 DATA 7,9"),
        " 7  9  7\n"
    );
}

#[test]
fn restore_in_an_untaken_branch_does_not_report_a_data_error() {
    assert_eq!(
        run("10 IF 0 THEN RESTORE Missing\n20 PRINT \"OK\":END\n100 DATA 7\n200 Missing:"),
        "OK\n"
    );
}

#[test]
fn restore_operands_follow_numeric_variable_casing_while_labels_keep_their_spelling() {
    let mut basic = Interpreter::new();
    for source in ["10 Records=110", "20 rEcOrDs:", "30 RESTORE records"] {
        basic.process_immediate(source).unwrap();
    }
    assert_eq!(basic.program.get(20), Some(" rEcOrDs:"));
    assert_eq!(basic.program.get(30), Some(" RESTORE Records"));
}

#[test]
fn numeric_mouse_registration_keeps_its_existing_owner_checks_and_error_timing() {
    for source in [
        "10 CALL Work:PRINT \"OK\":END\n100 DEF SUB Work\n110 ON MOUSE LEFTDOWN GOSUB 130\n120 EXIT SUB\n130 RETURN\n140 SUBEND",
        "10 ON MOUSE LEFTDOWN GOSUB 130:PRINT \"OK\":END\n100 DEF SUB Work\n110 EXIT SUB\n130 RETURN\n140 SUBEND",
    ] {
        // No mouse event is dispatched. Historically these numeric targets
        // are accepted at registration; any dispatch checks remain separate.
        assert_eq!(run(source), "OK\n");
    }
}

#[test]
fn run_uses_root_labels_and_requires_quoted_names_to_select_files() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("Entry.bas"), "10 PRINT \"FILE\":END").unwrap();
    let mut basic = interpreter("10 PRINT \"WRONG\":END\n100 Entry:\n110 PRINT \"LABEL\":END");
    basic.root_dir = directory.path().to_path_buf();
    basic.current_dir = directory.path().to_path_buf();
    basic.process_immediate("RUN Entry").unwrap();
    assert_eq!(basic.take_output(), "LABEL\n");
    basic.process_immediate("RUN \"Entry\"").unwrap();
    assert_eq!(basic.take_output(), "FILE\n");
    assert_eq!(
        basic.process_immediate("RUN Entry").unwrap_err().code,
        ErrorCode::TargetLineNotFound
    );
    assert_eq!(basic.take_output(), "");
}

#[test]
fn run_never_opens_an_implicit_file_and_quoted_files_can_replace_invalid_source() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("Entry.bas"), "10 PRINT \"FILE\":END").unwrap();
    let original = "10 GOTO Missing\n20 END";
    let mut basic = interpreter(original);
    basic.root_dir = directory.path().to_path_buf();
    basic.current_dir = directory.path().to_path_buf();
    assert_eq!(
        basic.process_immediate("RUN Entry").unwrap_err().code,
        ErrorCode::TargetLineNotFound
    );
    assert_eq!(basic.program.list().trim_end(), original);
    basic.process_immediate("RUN \"Entry\"").unwrap();
    assert_eq!(basic.take_output(), "FILE\n");

    let invalid_source = "10 GOTO Missing\n100 Entry:\n110 END";
    let mut basic = interpreter(invalid_source);
    basic.root_dir = directory.path().to_path_buf();
    basic.current_dir = directory.path().to_path_buf();
    assert!(basic.process_immediate("RUN Entry").is_err());
    assert_eq!(basic.program.list().trim_end(), invalid_source);
    basic.process_immediate("RUN \"Entry\"").unwrap();
    assert_eq!(basic.take_output(), "FILE\n");
}

#[test]
fn chain_resolves_entry_in_the_loaded_program_and_preserves_numeric_expressions() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("child.bas"),
        "10 PRINT \"WRONG START\":END\n100 Entry:\n110 PRINT \"CHILD\":END\n300 PRINT \"WRONG OLD LABEL\":END",
    )
    .unwrap();
    for target in ["Entry", "Entry+0", "(Entry)", "Fallback"] {
        let mut basic = interpreter(&format!(
            "10 Entry=110:Fallback=110:CHAIN \"child\",{target}\n20 END\n300 Entry:\n310 PRINT \"WRONG CALLER\":END"
        ));
        basic.root_dir = directory.path().to_path_buf();
        basic.current_dir = directory.path().to_path_buf();
        assert_eq!(basic.run_loaded().unwrap(), RunOutcome::End);
        assert_eq!(basic.take_output(), "CHILD\n", "{target}");
    }
}

#[test]
fn chain_merge_resolves_delete_bounds_before_merging_and_entry_after_merging() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("child.bas"),
        "100 Entry:\n110 PRINT \"MERGED\":END",
    )
    .unwrap();
    let mut basic = interpreter(
        "10 CHAIN MERGE \"child\",Entry,DELETE OldStart-OldEnd\n20 END\n100 OldStart:\n110 PRINT \"OLD\":END\n200 OldEnd:\n210 END",
    );
    basic.root_dir = directory.path().to_path_buf();
    basic.current_dir = directory.path().to_path_buf();
    assert_eq!(basic.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(basic.take_output(), "MERGED\n");
    assert!(!basic.program.list().contains("OldStart:"));
    assert!(!basic.program.list().contains("OldEnd:"));
}

#[test]
fn editing_ranges_accept_root_labels_open_bounds_and_mixed_numeric_endpoints() {
    let source = "10 First:\n20 PRINT \"A\"\n30 Last:\n40 END";
    for (argument, expected) in [
        ("First-30", "10 First:\n20 PRINT \"A\"\n30 Last:\n"),
        ("20-Last", "20 PRINT \"A\"\n30 Last:\n"),
        ("-Last", "10 First:\n20 PRINT \"A\"\n30 Last:\n"),
        ("First-", source),
    ] {
        let mut basic = interpreter(source);
        basic
            .process_immediate(&format!("LIST {argument}"))
            .unwrap();
        assert_eq!(
            basic.take_output().trim_end(),
            expected.trim_end(),
            "{argument}"
        );
    }
    for argument in ["First-30", "10-Last", "-Last"] {
        let mut basic = interpreter(source);
        basic
            .process_immediate(&format!("DELETE {argument}"))
            .unwrap();
        assert_eq!(basic.program.list(), "40 END\n", "{argument}");
    }
    let mut basic = interpreter(source);
    basic.process_immediate("DELETE Last-").unwrap();
    assert_eq!(basic.program.list(), "10 First:\n20 PRINT \"A\"\n");
}

#[test]
fn failed_range_resolution_preserves_source_even_for_an_empty_program() {
    let source = "10 First:\n20 PRINT \"A\"\n30 Last:\n40 END";
    for argument in ["Last-First", "Missing-Last", "First-Missing"] {
        let mut basic = interpreter(source);
        assert!(basic
            .process_immediate(&format!("DELETE {argument}"))
            .is_err());
        assert_eq!(basic.program.list().trim_end(), source, "{argument}");
    }
    for command in ["LIST Missing", "DELETE Missing", "RENUM 100,10,Missing"] {
        let mut basic = Interpreter::new();
        assert!(basic.process_immediate(command).is_err(), "{command}");
        assert!(basic.program.is_empty());
    }
    let mut basic = Interpreter::new();
    basic.process_immediate("LIST").unwrap();
    basic.process_immediate("DELETE").unwrap();
    assert_eq!(basic.take_output(), "");
}

#[test]
fn edit_resolves_a_label_before_applying_the_terminal_requirement() {
    if console::interactive_terminal() {
        return;
    }
    let mut basic = interpreter("10 Entry:\n20 END");
    assert_eq!(
        basic.process_immediate("EDIT Entry").unwrap_err().code,
        ErrorCode::Unsupported
    );
    assert!(basic.process_immediate("EDIT Missing").is_err());
    assert_eq!(basic.program.list(), "10 Entry:\n20 END\n");
}

#[test]
fn label_based_inspection_and_editing_allow_repairing_unrelated_bad_targets() {
    let source = "10 Entry:\n20 GOTO Missing\n30 END";
    let mut basic = interpreter(source);
    basic.process_immediate("LIST Entry").unwrap();
    assert_eq!(basic.take_output(), "10 Entry:\n");
    if !console::interactive_terminal() {
        assert_eq!(
            basic.process_immediate("EDIT Entry").unwrap_err().code,
            ErrorCode::Unsupported
        );
    }
    assert_eq!(basic.program.list().trim_end(), source);
    basic.process_immediate("DELETE Entry").unwrap();
    assert_eq!(basic.program.list(), "20 GOTO Missing\n30 END\n");
}

#[test]
fn renum_resolves_old_label_bounds_but_new_numbers_and_increment_stay_numeric() {
    let source = "10 PRINT \"A\"\n20 First:\n30 Last:\n200 END";
    let mut basic = interpreter(source);
    basic.process_immediate("RENUM 100,10,First,Last").unwrap();
    assert_eq!(
        basic.program.list(),
        "10 PRINT \"A\"\n100 First:\n110 Last:\n200 END\n"
    );
    for command in ["RENUM First,10", "RENUM 100,First"] {
        let mut basic = interpreter(source);
        assert!(basic.process_immediate(command).is_err(), "{command}");
        assert_eq!(basic.program.list().trim_end(), source);
    }
}

#[test]
fn renum_preserves_numeric_inline_if_shorthand_and_keeps_symbolic_names() {
    let mut basic = interpreter(
        "10 IF 0 THEN 100 ELSE 200\n20 END\n100 First:\n110 PRINT \"WRONG\":END\n200 Last:\n210 PRINT \"OK\":END",
    );
    basic.process_immediate("RENUM 1000,10").unwrap();
    assert!(basic
        .program
        .list()
        .contains("1000 IF 0 THEN 1020 ELSE 1040"));
    assert!(basic.program.list().contains("1020 First:"));
    assert!(basic.program.list().contains("1040 Last:"));
    assert_eq!(basic.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(basic.take_output(), "OK\n");
}
