use avl_basic::Interpreter;

fn command(interpreter: &mut Interpreter, source: &str) {
    interpreter.process_immediate(source).unwrap();
}

#[test]
fn accepted_lines_store_the_canonical_identifier_spelling() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 PRINT a");
    assert_eq!(interpreter.program.get(20), Some(" PRINT A"));

    command(&mut interpreter, "30 MiVar=1");
    command(&mut interpreter, "5   PRINT mivar");
    assert_eq!(interpreter.program.get(5), Some("   PRINT MiVar"));
    command(&mut interpreter, "10 a=9");
    assert_eq!(interpreter.program.get(10), Some(" A=9"));
}

#[test]
fn deleting_the_first_occurrence_does_not_restore_old_identifier_spelling() {
    for deletion in ["DELETE 10", "DELETE 1-15", "10"] {
        let mut interpreter = Interpreter::new();
        command(&mut interpreter, "10 A=5");
        command(&mut interpreter, "20 PRINT a");
        command(&mut interpreter, deletion);
        assert_eq!(interpreter.program.get(20), Some(" PRINT A"), "{deletion}");
        command(&mut interpreter, "LIST");
        assert_eq!(interpreter.take_output(), "20 PRINT A\n", "{deletion}");
    }
}

#[test]
fn stored_listing_and_saved_source_match_through_a_load_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 PRINT a");
    command(&mut interpreter, "SAVE \"saved.bas\"");
    let saved = std::fs::read_to_string(temp.path().join("saved.bas")).unwrap();
    assert_eq!(saved, interpreter.program.list());
    assert_eq!(saved, "10 A=5\n20 PRINT A\n");
    command(&mut interpreter, "LOAD \"saved.bas\"");
    command(&mut interpreter, "DELETE 10");
    assert_eq!(interpreter.program.list(), "20 PRINT A\n");
}

#[test]
fn loaded_source_stores_consistent_identifier_spelling() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("mixed.bas");
    std::fs::write(&path, "10 MiVar=5\n20 PRINT mivar\n30 PRINT MIVAR\n").unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.load_file(&path).unwrap();
    assert_eq!(
        interpreter.program.list(),
        "10 MiVar=5\n20 PRINT MiVar\n30 PRINT MiVar\n"
    );
    command(&mut interpreter, "DELETE 10");
    assert_eq!(interpreter.program.get(20), Some(" PRINT MiVar"));
}

#[test]
fn merge_keeps_existing_identifier_style_even_when_inserting_an_earlier_line() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("child.bas"), "5 PRINT a\n30 PRINT a\n").unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 PRINT a");
    command(&mut interpreter, "MERGE \"child.bas\"");
    assert_eq!(
        interpreter.program.list(),
        "5 PRINT A\n10 A=5\n20 PRINT A\n30 PRINT A\n"
    );
}

#[test]
fn merge_still_applies_bare_line_number_deletions() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("child.bas"), "10\n30 PRINT a\n").unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 PRINT a");
    command(&mut interpreter, "MERGE \"child.bas\"");
    assert_eq!(interpreter.program.list(), "20 PRINT A\n30 PRINT A\n");
}

#[test]
fn renumbering_does_not_change_the_stored_identifier_style() {
    let mut interpreter = Interpreter::new();
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 PRINT a");
    command(&mut interpreter, "RENUM 100,10");
    command(&mut interpreter, "DELETE 100");
    assert_eq!(interpreter.program.list(), "110 PRINT A\n");
}

#[test]
fn chain_normalizes_replaced_source_and_chain_merge_keeps_existing_style() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("replace.bas"),
        "10 MiVar=7\n20 PRINT mivar\n30 END\n",
    )
    .unwrap();
    std::fs::write(temp.path().join("merge.bas"), "5 PRINT a\n30 END\n").unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    command(&mut interpreter, "CHAIN \"replace.bas\"");
    assert_eq!(
        interpreter.program.list(),
        "10 MiVar=7\n20 PRINT MiVar\n30 END\n"
    );
    assert_eq!(interpreter.take_output(), " 7\n");

    command(&mut interpreter, "NEW");
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 PRINT a");
    command(&mut interpreter, "CHAIN MERGE \"merge.bas\",10");
    assert_eq!(
        interpreter.program.list(),
        "5 PRINT A\n10 A=5\n20 PRINT A\n30 END\n"
    );
    assert_eq!(interpreter.take_output(), " 5\n");
}

#[test]
fn normalization_keeps_strings_comments_and_unquoted_data_literal() {
    let temp = tempfile::tempdir().unwrap();
    let mut interpreter = Interpreter::new();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    command(&mut interpreter, "10 A=5");
    command(&mut interpreter, "20 DATA a,\"a:A\":PRINT a ' a");
    command(&mut interpreter, "30 READ Text$,Other$");
    command(&mut interpreter, "40 PRINT Text$;\"|\";Other$");
    command(&mut interpreter, "50 REM a A");
    let source = interpreter.program.list();
    assert!(source.contains("DATA a,\"a:A\""), "{source}");
    assert!(source.contains("PRINT A ' a"), "{source}");
    assert!(source.contains("REM a A"), "{source}");
    command(&mut interpreter, "RUN");
    assert!(interpreter.take_output().ends_with("a|a:A\n"));
    command(&mut interpreter, "SAVE \"literals.bas\"");
    assert_eq!(
        std::fs::read_to_string(temp.path().join("literals.bas")).unwrap(),
        source
    );
}
