use avl_basic::{Debugger, Interpreter};

fn loaded_interpreter(program: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.process_immediate("ZONE 8").unwrap();
    for line in program.lines() {
        interpreter.process_immediate(line).unwrap();
    }
    interpreter
}

fn run_program(program: &str) -> String {
    let mut interpreter = loaded_interpreter(program);
    interpreter.process_immediate("RUN").unwrap();
    interpreter.take_output()
}

#[test]
fn resume_next_continues_inside_then_clause_and_skips_else_clause() {
    let output = run_program(
        r#"10 ON ERROR GOTO 100
20 X=1
30 IF X=1 THEN PRINT "THEN BEFORE":A=1/0:PRINT "THEN AFTER" ELSE PRINT "WRONG ELSE"
40 PRINT "DONE"
50 END
100 PRINT "HANDLER"
110 RESUME NEXT"#,
    );

    assert_eq!(output, "THEN BEFORE\nHANDLER\nTHEN AFTER\nDONE\n");
}

#[test]
fn resume_next_continues_inside_else_clause() {
    let output = run_program(
        r#"10 ON ERROR GOTO 100
20 X=0
30 IF X=1 THEN PRINT "WRONG THEN" ELSE PRINT "ELSE BEFORE":A=1/0:PRINT "ELSE AFTER"
40 PRINT "DONE"
50 END
100 PRINT "HANDLER"
110 RESUME NEXT"#,
    );

    assert_eq!(output, "ELSE BEFORE\nHANDLER\nELSE AFTER\nDONE\n");
}

#[test]
fn resume_next_after_error_in_if_condition_enters_then_clause() {
    let output = run_program(
        r#"10 ON ERROR GOTO 100
20 IF 1/0 THEN PRINT "THEN AFTER CONDITION" ELSE PRINT "WRONG ELSE"
30 PRINT "DONE"
40 END
100 PRINT "HANDLER"
110 RESUME NEXT"#,
    );

    assert_eq!(output, "HANDLER\nTHEN AFTER CONDITION\nDONE\n");
}

#[test]
fn on_error_resume_next_shorthand_continues_inside_then_clause() {
    let output = run_program(
        r#"10 ON ERROR RESUME NEXT
20 X=1
30 IF X=1 THEN PRINT "THEN BEFORE":A=1/0:PRINT "THEN AFTER" ELSE PRINT "WRONG ELSE"
40 PRINT "DONE"
50 END"#,
    );

    assert_eq!(output, "THEN BEFORE\nTHEN AFTER\nDONE\n");
}

#[test]
fn on_error_resume_next_shorthand_continues_inside_else_clause() {
    let output = run_program(
        r#"10 ON ERROR RESUME NEXT
20 X=0
30 IF X=1 THEN PRINT "WRONG THEN" ELSE PRINT "ELSE BEFORE":A=1/0:PRINT "ELSE AFTER"
40 PRINT "DONE"
50 END"#,
    );

    assert_eq!(output, "ELSE BEFORE\nELSE AFTER\nDONE\n");
}

#[test]
fn cont_resumes_after_stop_inside_then_clause() {
    let mut interpreter = loaded_interpreter(
        r#"10 X=1
20 IF X=1 THEN PRINT "THEN BEFORE":STOP:PRINT "THEN AFTER" ELSE PRINT "WRONG ELSE"
30 PRINT "DONE"
40 END"#,
    );

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(
        interpreter.take_output(),
        "THEN BEFORE\nLine 20. Program stopped.\n"
    );

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "THEN AFTER\nDONE\n");
}

#[test]
fn cont_resumes_after_stop_inside_else_clause() {
    let mut interpreter = loaded_interpreter(
        r#"10 X=0
20 IF X=1 THEN PRINT "WRONG THEN" ELSE PRINT "ELSE BEFORE":STOP:PRINT "ELSE AFTER"
30 PRINT "DONE"
40 END"#,
    );

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(
        interpreter.take_output(),
        "ELSE BEFORE\nLine 20. Program stopped.\n"
    );

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "ELSE AFTER\nDONE\n");
}

fn gosub_program(condition: i32) -> String {
    format!(
        r#"10 X={condition}
20 IF X=1 THEN GOSUB 100:GOSUB 200:PRINT "THEN AFTER" ELSE GOSUB 300:GOSUB 400:PRINT "ELSE AFTER"
30 PRINT "DONE"
40 END
100 PRINT "THEN SUB 1":RETURN
200 PRINT "THEN SUB 2":RETURN
300 PRINT "ELSE SUB 1":RETURN
400 PRINT "ELSE SUB 2":RETURN"#
    )
}

#[test]
fn gosub_return_resumes_inside_then_clause() {
    assert_eq!(
        run_program(&gosub_program(1)),
        "THEN SUB 1\nTHEN SUB 2\nTHEN AFTER\nDONE\n"
    );
}

#[test]
fn gosub_return_resumes_inside_else_clause() {
    assert_eq!(
        run_program(&gosub_program(0)),
        "ELSE SUB 1\nELSE SUB 2\nELSE AFTER\nDONE\n"
    );
}

fn on_gosub_program(condition: i32) -> String {
    format!(
        r#"10 X={condition}:N=2
20 IF X=1 THEN ON N GOSUB 100,200:PRINT "THEN AFTER" ELSE ON N GOSUB 300,400:PRINT "ELSE AFTER"
30 PRINT "DONE"
40 END
100 PRINT "THEN TARGET 1":RETURN
200 PRINT "THEN TARGET 2":RETURN
300 PRINT "ELSE TARGET 1":RETURN
400 PRINT "ELSE TARGET 2":RETURN"#
    )
}

#[test]
fn on_gosub_return_resumes_inside_then_clause() {
    assert_eq!(
        run_program(&on_gosub_program(1)),
        "THEN TARGET 2\nTHEN AFTER\nDONE\n"
    );
}

#[test]
fn on_gosub_return_resumes_inside_else_clause() {
    assert_eq!(
        run_program(&on_gosub_program(0)),
        "ELSE TARGET 2\nELSE AFTER\nDONE\n"
    );
}

#[test]
fn nested_if_gosub_return_preserves_nearest_else_association() {
    let output = run_program(
        r#"10 X=1:Y=0
20 IF X=1 THEN IF Y=1 THEN GOSUB 100:PRINT "INNER THEN" ELSE GOSUB 200:PRINT "INNER ELSE" ELSE GOSUB 300:PRINT "OUTER ELSE"
30 PRINT "DONE"
40 END
100 PRINT "INNER THEN SUB":RETURN
200 PRINT "INNER ELSE SUB":RETURN
300 PRINT "OUTER ELSE SUB":RETURN"#,
    );

    assert_eq!(output, "INNER ELSE SUB\nINNER ELSE\nDONE\n");
}

#[test]
fn resume_retries_the_exact_statement_inside_then_and_else_clauses() {
    let output = run_program(
        r#"10 ON ERROR GOTO 100
20 X=1:D=0
30 IF X=1 THEN A=10/D:PRINT "THEN RETRIED" ELSE PRINT "WRONG THEN"
40 X=0:D=0
50 IF X=1 THEN PRINT "WRONG ELSE" ELSE B=12/D:PRINT "ELSE RETRIED"
60 PRINT "DONE"
70 END
100 D=2
110 RESUME"#,
    );

    assert_eq!(output, "THEN RETRIED\nELSE RETRIED\nDONE\n");
}

#[test]
fn resume_next_after_last_then_statement_skips_else_clause() {
    let output = run_program(
        r#"10 ON ERROR GOTO 100
20 IF 1 THEN PRINT "THEN":A=1/0 ELSE PRINT "WRONG ELSE"
30 PRINT "DONE"
40 END
100 PRINT "HANDLER"
110 RESUME NEXT"#,
    );

    assert_eq!(output, "THEN\nHANDLER\nDONE\n");
}

#[test]
fn cont_after_last_then_statement_skips_else_clause() {
    let mut interpreter = loaded_interpreter(
        r#"10 IF 1 THEN PRINT "THEN":STOP ELSE PRINT "WRONG ELSE"
20 PRINT "DONE"
30 END"#,
    );

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(
        interpreter.take_output(),
        "THEN\nLine 10. Program stopped.\n"
    );

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "DONE\n");
}

#[test]
fn return_after_last_then_gosub_skips_else_clause() {
    let output = run_program(
        r#"10 IF 1 THEN PRINT "THEN":GOSUB 100 ELSE PRINT "WRONG ELSE"
20 PRINT "DONE"
30 END
100 PRINT "SUB"
110 RETURN"#,
    );

    assert_eq!(output, "THEN\nSUB\nDONE\n");
}

#[test]
fn return_after_last_then_on_gosub_skips_else_clause() {
    let output = run_program(
        r#"10 N=2:IF 1 THEN PRINT "THEN":ON N GOSUB 100,200 ELSE PRINT "WRONG ELSE"
20 PRINT "DONE"
30 END
100 PRINT "SUB 1":RETURN
200 PRINT "SUB 2":RETURN"#,
    );

    assert_eq!(output, "THEN\nSUB 2\nDONE\n");
}

fn numeric_branch_program(condition: i32) -> String {
    format!(
        r#"10 X={condition}
20 IF X=1 THEN 100 ELSE 200
30 PRINT "WRONG FALLTHROUGH":END
100 PRINT "NUMERIC THEN":GOTO 300
200 PRINT "NUMERIC ELSE"
300 END"#
    )
}

#[test]
fn numeric_then_branch_jumps_to_its_line() {
    assert_eq!(run_program(&numeric_branch_program(1)), "NUMERIC THEN\n");
}

#[test]
fn numeric_else_branch_jumps_to_its_line() {
    assert_eq!(run_program(&numeric_branch_program(0)), "NUMERIC ELSE\n");
}

#[test]
fn resume_next_after_missing_numeric_targets_continues_after_if() {
    let output = run_program(
        r#"10 ON ERROR GOTO 1000
20 X=1
30 IF X=1 THEN 999 ELSE 900
40 PRINT "AFTER THEN ERROR"
50 X=0
60 IF X=1 THEN 900 ELSE 998
70 PRINT "AFTER ELSE ERROR"
80 END
900 PRINT "WRONG TARGET":END
1000 PRINT "HANDLER"
1010 RESUME NEXT"#,
    );

    assert_eq!(
        output,
        "HANDLER\nAFTER THEN ERROR\nHANDLER\nAFTER ELSE ERROR\n"
    );
}

#[test]
fn merge_inside_then_clause_preserves_the_python_continuation_when_lines_shift() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("insert.bas"), "5 REM INSERTED\n").unwrap();
    let mut interpreter = loaded_interpreter(
        r#"10 A=A+1:IF A=1 THEN MERGE "insert.bas":PRINT "AFTER"
20 PRINT A
30 END"#,
    );
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();

    interpreter.process_immediate("RUN").unwrap();

    assert_eq!(interpreter.take_output(), "AFTER\n 2\n");
}

#[test]
fn inactive_debugger_preserves_merge_continuation_inside_then_clause() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("insert.bas"), "5 REM INSERTED\n").unwrap();
    let mut interpreter = loaded_interpreter(
        r#"10 A=A+1:IF A=1 THEN MERGE "insert.bas":PRINT "AFTER"
20 PRINT A
30 END"#,
    );
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    interpreter.set_debugger(Debugger::scripted([]));

    interpreter.process_immediate("RUN").unwrap();

    assert_eq!(interpreter.take_output(), "AFTER\n 2\n");
    assert!(interpreter.debugger().unwrap().snapshots().is_empty());
}

#[test]
fn gosub_after_merge_inside_then_returns_to_the_original_clause() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("insert.bas"), "5 REM INSERTED\n").unwrap();
    let mut interpreter = loaded_interpreter(
        r#"10 A=A+1:IF A=1 THEN MERGE "insert.bas":GOSUB 100:PRINT "AFTER"
20 PRINT A
30 END
100 PRINT "SUB":RETURN"#,
    );
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();

    interpreter.process_immediate("RUN").unwrap();

    assert_eq!(interpreter.take_output(), "SUB\nAFTER\n 1\n");
}

#[test]
fn on_gosub_after_merge_inside_then_returns_to_the_original_clause() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("insert.bas"), "5 REM INSERTED\n").unwrap();
    let mut interpreter = loaded_interpreter(
        r#"10 A=A+1:IF A=1 THEN MERGE "insert.bas":ON 1 GOSUB 100:PRINT "AFTER"
20 PRINT A
30 END
100 PRINT "SUB":RETURN"#,
    );
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();

    interpreter.process_immediate("RUN").unwrap();

    assert_eq!(interpreter.take_output(), "SUB\nAFTER\n 1\n");
}

#[test]
fn for_next_runs_inside_then_and_else_clauses() {
    let output = run_program(
        r#"10 IF 1 THEN FOR I=1 TO 3:PRINT "T";:NEXT I:PRINT "!" ELSE PRINT "WRONG THEN"
20 IF 0 THEN PRINT "WRONG ELSE" ELSE FOR J=1 TO 2:PRINT "E";:NEXT J:PRINT "!"
30 END"#,
    );

    assert_eq!(output, "TTT!\nEE!\n");
}

#[test]
fn while_wend_runs_inside_then_and_else_clauses() {
    let output = run_program(
        r#"10 I=0:IF 1 THEN WHILE I<3:PRINT "T";:I=I+1:WEND:PRINT "!" ELSE PRINT "WRONG THEN"
20 J=0:IF 0 THEN PRINT "WRONG ELSE" ELSE WHILE J<2:PRINT "E";:J=J+1:WEND:PRINT "!"
30 END"#,
    );

    assert_eq!(output, "TTT!\nEE!\n");
}

#[test]
fn then_else_and_colons_inside_strings_are_not_control_tokens() {
    let output = run_program(
        r#"10 A$="THEN"
20 IF A$="THEN" THEN PRINT "THEN:ELSE":PRINT "AFTER:PRINT" ELSE PRINT "WRONG"
30 END"#,
    );

    assert_eq!(output, "THEN:ELSE\nAFTER:PRINT\n");
}

#[test]
fn goto_back_to_if_cursor_reevaluates_its_condition() {
    let output = run_program(
        r#"10 IF A=0 THEN A=A+1:GOTO 10
20 PRINT A
30 END"#,
    );

    assert_eq!(output, " 1\n");
}

#[test]
fn cont_unwinds_a_nonfinal_inline_gosub_after_stop() {
    let mut interpreter = loaded_interpreter(
        r#"10 IF 1 THEN PRINT "CALL":GOSUB 100:PRINT "CALLER AFTER" ELSE PRINT "WRONG"
20 PRINT "DONE":END
100 PRINT "SUB BEFORE":STOP:PRINT "SUB AFTER":RETURN"#,
    );

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(
        interpreter.take_output(),
        "CALL\nSUB BEFORE\nLine 100. Program stopped.\n"
    );

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "SUB AFTER\nCALLER AFTER\nDONE\n");
}

#[test]
fn cont_unwinds_a_nonfinal_inline_on_gosub_after_stop() {
    let mut interpreter = loaded_interpreter(
        r#"10 IF 1 THEN PRINT "CALL":ON 1 GOSUB 100,200:PRINT "CALLER AFTER" ELSE PRINT "WRONG"
20 PRINT "DONE":END
100 PRINT "SUB BEFORE":STOP:PRINT "SUB AFTER":RETURN
200 PRINT "WRONG TARGET":RETURN"#,
    );

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(
        interpreter.take_output(),
        "CALL\nSUB BEFORE\nLine 100. Program stopped.\n"
    );

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "SUB AFTER\nCALLER AFTER\nDONE\n");
}

#[test]
fn nonfinal_inline_gosub_that_reaches_eof_does_not_resume_its_caller() {
    let output = run_program(
        r#"10 IF 1 THEN GOSUB 100:PRINT "WRONG AFTER" ELSE PRINT "WRONG ELSE"
20 PRINT "WRONG DONE":END
100 PRINT "SUB""#,
    );

    assert_eq!(output, "SUB\n");
}

#[test]
fn nonfinal_inline_on_gosub_that_reaches_eof_does_not_resume_its_caller() {
    let output = run_program(
        r#"10 IF 1 THEN ON 1 GOSUB 100:PRINT "WRONG AFTER" ELSE PRINT "WRONG ELSE"
20 PRINT "WRONG DONE":END
100 PRINT "SUB""#,
    );

    assert_eq!(output, "SUB\n");
}

#[test]
fn cont_rebinds_an_inline_gosub_return_after_merge_shifts_the_program() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("insert.bas"), "5 REM INSERTED\n").unwrap();
    let mut interpreter = loaded_interpreter(
        r#"10 IF 1 THEN MERGE "insert.bas":GOSUB 100:PRINT "AFTER" ELSE PRINT "WRONG"
20 PRINT "DONE":END
100 PRINT "SUB":STOP:PRINT "SUB AFTER":RETURN"#,
    );
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();

    interpreter.process_immediate("RUN").unwrap();
    assert_eq!(
        interpreter.take_output(),
        "SUB\nLine 100. Program stopped.\n"
    );

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), "SUB AFTER\nAFTER\nDONE\n");
}

#[test]
fn nonfinal_on_mouse_gosub_is_not_misclassified_as_selector_dispatch() {
    let output = run_program(
        r#"10 IF 1 THEN ON MOUSE LEFTDOWN GOSUB 100:PRINT "OK" ELSE PRINT "WRONG ELSE"
20 END
100 PRINT "WRONG HANDLER":RETURN"#,
    );

    assert_eq!(output, "OK\n");
}
