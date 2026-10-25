use super::*;

fn prepared(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.prepare_run().unwrap();
    interpreter.rebuild_command_cache();
    interpreter.ensure_routine_catalog().unwrap();
    interpreter.current_line = Some(900);
    interpreter
}

fn number(interpreter: &Interpreter, name: &str) -> f64 {
    interpreter
        .numeric_variables
        .get(name)
        .copied()
        .unwrap_or(0.0)
}

#[test]
fn one_compiled_call_rebinds_across_interpreters_edits_and_reloads() {
    let compiled = compile_call_statement("WORK(2)").unwrap();
    let mut first = prepared("10 DEF SUB WORK(X)\n20 HITS=HITS+X\n30 SUBEND");
    let mut second = prepared(
        "10 REM different cursor\n100 DEF SUB WORK(X)\n110 LOCAL T\n120 HITS=HITS+X*10\n130 SUBEND",
    );
    assert_ne!(
        first.sub_runtime.sub_catalog_id,
        second.sub_runtime.sub_catalog_id
    );

    first.execute_compiled_call(&compiled).unwrap();
    second.execute_compiled_call(&compiled).unwrap();
    first.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&first, "HITS"), 4.0);
    assert_eq!(number(&second, "HITS"), 20.0);

    let old_catalog = second.sub_runtime.sub_catalog_id;
    second.process_immediate("120 HITS=HITS+X*100").unwrap();
    second.ensure_routine_catalog().unwrap();
    assert_ne!(old_catalog, second.sub_runtime.sub_catalog_id);
    second.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&second, "HITS"), 220.0);

    let old_catalog = first.sub_runtime.sub_catalog_id;
    first
        .program
        .load_text("10 REM moved definition\n20 REM more movement\n100 DEF SUB WORK(X)\n110 LOCAL Q(1)\n120 HITS=HITS+X*1000\n130 SUBEND")
        .unwrap();
    first.run_loaded().unwrap();
    assert_ne!(old_catalog, first.sub_runtime.sub_catalog_id);
    first.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&first, "HITS"), 2000.0);
}

#[test]
fn retained_compiled_call_rebinds_after_merge_and_chain_catalog_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let mut interpreter = prepared("10 DEF SUB WORK(X)\n20 HITS=HITS+X\n30 SUBEND");
    interpreter.root_dir = directory.path().to_path_buf();
    interpreter.current_dir = directory.path().to_path_buf();
    let compiled = compile_call_statement("WORK(2)").unwrap();
    interpreter.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&interpreter, "HITS"), 2.0);

    std::fs::write(
        directory.path().join("merge.bas"),
        "5 REM shifts routine cursor\n20 HITS=HITS+X*10\n",
    )
    .unwrap();
    let old_catalog = interpreter.sub_runtime.sub_catalog_id;
    interpreter
        .process_immediate("MERGE \"merge.bas\"")
        .unwrap();
    assert_ne!(old_catalog, interpreter.sub_runtime.sub_catalog_id);
    interpreter.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&interpreter, "HITS"), 22.0);

    let replacement = directory.path().join("replacement.bas");
    std::fs::write(
        &replacement,
        "10 REM different program\n20 REM different layout\n30 DEF SUB WORK(X)\n40 LOCAL A\n50 HITS=HITS+X*100\n60 SUBEND\n",
    )
    .unwrap();
    let old_catalog = interpreter.sub_runtime.sub_catalog_id;
    interpreter
        .load_file_preserving_runtime(&replacement)
        .unwrap();
    assert_ne!(old_catalog, interpreter.sub_runtime.sub_catalog_id);
    interpreter.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&interpreter, "HITS"), 222.0);
}

#[test]
fn arguments_run_before_lookup_even_when_a_warm_target_was_deleted() {
    let mut interpreter = prepared(
        "10 DEF SUB WORK(A,B)\n20 HITS=HITS+1\n30 SUBEND\n100 DEF FNTICK()\n110 TICKS=TICKS+1:FNTICK=TICKS\n120 FNEND",
    );
    let compiled = compile_call_statement("WORK(FNTICK(),1/D)").unwrap();
    interpreter.numeric_variables.insert("D".to_string(), 1.0);
    interpreter.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&interpreter, "HITS"), 1.0);
    assert_eq!(number(&interpreter, "TICKS"), 1.0);

    let old_catalog = interpreter.sub_runtime.sub_catalog_id;
    interpreter.process_immediate("DELETE 10-30").unwrap();
    interpreter.ensure_routine_catalog().unwrap();
    assert_ne!(old_catalog, interpreter.sub_runtime.sub_catalog_id);
    interpreter.numeric_variables.insert("D".to_string(), 0.0);
    let error = interpreter.execute_compiled_call(&compiled).unwrap_err();
    assert_eq!(error.code, ErrorCode::DivisionByZero);
    assert_eq!(number(&interpreter, "TICKS"), 2.0);

    interpreter.numeric_variables.insert("D".to_string(), 1.0);
    let error = interpreter.execute_compiled_call(&compiled).unwrap_err();
    assert_eq!(error.code, ErrorCode::Undefined);
    assert_eq!(number(&interpreter, "TICKS"), 3.0);
    assert_eq!(number(&interpreter, "HITS"), 1.0);
}

#[test]
fn warm_targets_still_check_reentry_and_restore_after_repeated_failures() {
    let mut interpreter = prepared(
        "10 X=99\n100 DEF SUB AGAIN\n110 LOCAL X\n120 IF REENTER THEN CALL AGAIN\n130 HITS=HITS+1\n140 SUBEND",
    );
    interpreter.numeric_variables.insert("X".to_string(), 99.0);
    let compiled = compile_call_statement("AGAIN").unwrap();
    interpreter.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&interpreter, "HITS"), 1.0);
    interpreter
        .numeric_variables
        .insert("REENTER".to_string(), 1.0);

    for _ in 0..2 {
        let error = interpreter.execute_compiled_call(&compiled).unwrap_err();
        assert_eq!(error.code, ErrorCode::SubEndWithoutDef);
        assert_eq!(error.line, Some(120));
        assert_eq!(number(&interpreter, "X"), 99.0);
        assert_eq!(number(&interpreter, "HITS"), 1.0);
        assert!(interpreter.sub_call_stack.is_empty());
        assert!(interpreter.active_subs.is_empty());
    }
    interpreter
        .numeric_variables
        .insert("REENTER".to_string(), 0.0);
    interpreter.execute_compiled_call(&compiled).unwrap();
    assert_eq!(number(&interpreter, "HITS"), 2.0);
}
