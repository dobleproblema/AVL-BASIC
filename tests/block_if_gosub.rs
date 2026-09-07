use avl_basic::{DebugAction, Debugger, Interpreter, RunOutcome};

// Expected results describe BASIC continuation semantics, independently of
// either implementation. Run both normally and with instruction stepping.
fn loaded(source: &str, debug: bool) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    if debug {
        let mut pauses = 0;
        let mut debugger = Debugger::interactive_editable(move |snapshot, _, _| {
            pauses += 1;
            assert!(
                pauses <= 200,
                "unexpected repeated execution at {:?}",
                snapshot.location
            );
            Ok(DebugAction::StepInto)
        });
        debugger.request_pause();
        interpreter.set_debugger(debugger);
    }
    interpreter
}

fn assert_program(source: &str, expected: &str) {
    for debug in [false, true] {
        let mut interpreter = loaded(source, debug);
        let outcome = interpreter
            .run_loaded()
            .unwrap_or_else(|error| panic!("debug={debug}, {error:?}\n{source}"));
        assert_eq!(outcome, RunOutcome::End, "debug={debug}\n{source}");
        assert_eq!(
            interpreter.take_output(),
            expected,
            "debug={debug}\n{source}"
        );
    }
}

#[test]
fn gosub_preserves_the_enclosing_block_if() {
    assert_program(
        r#"10 IF 1 THEN
20 GOSUB 100
30 PRINT "CALLER"
40 ENDIF
50 END
100 PRINT "CALLEE"
110 RETURN"#,
        "CALLEE\nCALLER\n",
    );
}

#[test]
fn gosub_returns_to_then_elseif_and_else_branches() {
    assert_program(
        r#"10 A=0
20 FOR K=1 TO 3
30 IF K=1 THEN
40 GOSUB 200
50 ELSEIF K=2 THEN
60 GOSUB 300
70 ELSE
80 GOSUB 400
90 ENDIF
100 NEXT K
110 PRINT A;K
120 END
200 A=A+1
210 RETURN
300 A=A+10
310 RETURN
400 A=A+100
410 RETURN"#,
        " 111  4\n",
    );
}

#[test]
fn nested_gosubs_keep_each_callers_own_block_if() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+1000
50 ENDIF
60 PRINT A
70 END
100 IF 1 THEN
110 A=A+1
120 GOSUB 200
130 A=A+10
140 ENDIF
150 RETURN
200 IF 1 THEN
210 A=A+100
220 ENDIF
230 RETURN"#,
        " 1111\n",
    );
}

#[test]
fn early_return_discards_callee_if_without_discarding_caller_if() {
    assert_program(
        r#"10 IF 1 THEN
20 GOSUB 100
30 PRINT "CALLER"
40 ENDIF
50 END
100 IF 1 THEN
110 PRINT "CALLEE"
120 RETURN
130 ENDIF
140 PRINT "UNREACHABLE"
150 RETURN"#,
        "CALLEE\nCALLER\n",
    );
}

#[test]
fn on_gosub_and_inline_if_resume_colon_continuations_inside_a_block() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 ON 2 GOSUB 100,200:A=A+1
40 IF 1 THEN GOSUB 100:A=A+10 ELSE A=9
50 ENDIF
60 PRINT A
70 END
100 A=A+2
110 RETURN
200 A=A+3
210 RETURN"#,
        " 16\n",
    );
}

#[test]
fn real_goto_still_discards_the_inner_if_it_leaves() {
    assert_program(
        r#"10 IF 1 THEN
20 IF 1 THEN
30 GOTO 60
40 ENDIF
50 PRINT "UNREACHABLE"
60 ELSE
70 PRINT "WRONG BRANCH"
80 ENDIF
90 PRINT "DONE"
100 END"#,
        "DONE\n",
    );
}

#[test]
fn gosub_keeps_the_block_if_around_caller_and_callee_for_loops() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 FOR I=1 TO 2
40 GOSUB 100
50 NEXT I
60 ENDIF
70 PRINT A;I
80 END
100 FOR J=1 TO 1
110 A=A+1
120 A=A+2
130 NEXT J
140 RETURN"#,
        " 6  3\n",
    );
}

#[test]
fn recursive_early_returns_distinguish_identical_source_if_frames() {
    assert_program(
        r#"10 N=3:A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A;N
70 END
100 IF N>0 THEN
110 N=N-1
120 GOSUB 100
130 A=A+1
140 RETURN
150 ENDIF
160 RETURN"#,
        " 103  0\n",
    );
}

#[test]
fn goto_inside_callee_keeps_the_callers_selected_then_branch() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ELSE
60 A=999
70 ENDIF
80 PRINT A
90 END
100 GOTO 120
110 A=9999
120 A=A+1
130 RETURN"#,
        " 101\n",
    );
}

#[test]
fn on_goto_inside_callee_preserves_then_elseif_and_else_callers() {
    assert_program(
        r#"10 A=0
20 FOR K=1 TO 3
30 IF K=1 THEN
40 GOSUB 200
50 ELSEIF K=2 THEN
60 GOSUB 200
70 ELSE
80 GOSUB 200
90 ENDIF
100 NEXT K
110 PRINT A;K
120 END
200 ON K GOTO 300,400,500
210 A=9999:RETURN
300 A=A+1:GOTO 600
400 A=A+10:GOTO 600
500 A=A+100:GOTO 600
600 RETURN"#,
        " 111  4\n",
    );
}

#[test]
fn callee_goto_discards_only_its_own_nested_if_frames() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 IF 1 THEN
40 GOSUB 100
50 A=A+100
60 ENDIF
70 ENDIF
80 PRINT A
90 END
100 IF 1 THEN
110 IF 1 THEN
120 A=A+1
130 GOTO 180
140 ENDIF
150 ENDIF
160 A=9999
180 A=A+10
190 RETURN"#,
        " 111\n",
    );
}

#[test]
fn nested_gosub_jumps_preserve_each_suspended_if_prefix() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A
70 END
100 IF 1 THEN
110 GOSUB 200
120 A=A+10
130 GOTO 170
140 ENDIF
150 A=9999
170 RETURN
200 GOTO 220
210 A=9999
220 A=A+1
230 RETURN"#,
        " 111\n",
    );
}

#[test]
fn recursive_goto_distinguishes_identical_source_if_frames() {
    assert_program(
        r#"10 N=3:A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A;N
70 END
100 IF N>0 THEN
110 N=N-1
120 GOSUB 100
130 GOTO 170
140 ENDIF
150 RETURN
170 A=A+1
180 RETURN"#,
        " 103  0\n",
    );
}

#[test]
fn bounded_backward_goto_reenters_callee_if_without_losing_caller_if() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A;I
70 END
100 I=0
105 REM LOOP
110 IF 1 THEN
120 I=I+1
130 IF I<3 THEN GOTO 105
140 ENDIF
150 A=I
160 RETURN"#,
        " 103  3\n",
    );
}

#[test]
fn sub_goto_uses_its_own_if_scope_beneath_an_outer_gosub() {
    assert_program(
        r#"10 DEF SUB WORK
20 IF 1 THEN
30 A=A+1
40 GOTO 70
50 ENDIF
60 A=9999
70 A=A+10
80 SUBEND
90 A=0
100 IF 1 THEN
110 GOSUB 200
120 A=A+100
130 ENDIF
140 PRINT A
150 END
200 CALL WORK
210 RETURN"#,
        " 111\n",
    );
}

#[test]
fn function_goto_uses_its_own_if_scope_beneath_an_outer_gosub() {
    assert_program(
        r#"10 DEF FNF(X)
20 IF 1 THEN
30 X=X+1
40 GOTO 70
50 ENDIF
60 X=9999
70 FNF=X+10
80 FNEND
90 A=0
100 IF 1 THEN
110 GOSUB 200
120 A=A+100
130 ENDIF
140 PRINT A
150 END
200 A=FNF(1)
210 RETURN"#,
        " 112\n",
    );
}

#[test]
fn gosub_goto_preserves_its_sub_callers_local_if() {
    assert_program(
        r#"10 DEF SUB WORK
20 IF 1 THEN
30 GOSUB 100
40 A=A+10
50 ENDIF
60 SUBEXIT
100 GOTO 120
110 A=9999
120 A=A+1
130 RETURN
140 SUBEND
150 A=0
160 CALL WORK
170 PRINT A
180 END"#,
        " 11\n",
    );
}

#[test]
fn gosub_goto_preserves_its_function_callers_local_if() {
    assert_program(
        r#"10 DEF FNF(X)
20 IF 1 THEN
30 GOSUB 100
40 FNF=X+10
50 ENDIF
60 FNEXIT
100 GOTO 120
110 X=9999
120 X=X+1
130 RETURN
140 FNEND
150 PRINT FNF(1)
160 END"#,
        " 12\n",
    );
}

#[test]
fn exit_for_inside_callee_if_keeps_its_callers_if() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A;I
70 END
100 FOR I=1 TO 3
110 IF 1 THEN
120 A=A+1
130 EXIT FOR
140 ENDIF
150 NEXT I
160 RETURN"#,
        " 101  1\n",
    );
}

#[test]
fn exit_while_inside_callee_if_keeps_its_callers_if() {
    assert_program(
        r#"10 A=0
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A
70 END
100 WHILE 1
110 IF 1 THEN
120 A=A+1
130 EXIT WHILE
140 ENDIF
150 WEND
160 RETURN"#,
        " 101\n",
    );
}

#[test]
fn resume_next_reenters_the_callees_if_and_preserves_its_caller() {
    assert_program(
        r#"10 A=0:ON ERROR GOTO 300
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A;H;ERR;ERL
70 END
100 IF 1 THEN
110 A=A+1
120 ERROR 5
130 A=A+10
140 ENDIF
150 RETURN
300 H=ERR
310 RESUME NEXT"#,
        " 111  5  0  0\n",
    );
}

#[test]
fn resume_line_leaves_callee_if_without_discarding_caller_if() {
    assert_program(
        r#"10 A=0:ON ERROR GOTO 300
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A;H;ERR;ERL
70 END
100 IF 1 THEN
110 A=A+1
120 ERROR 5
130 A=A+10
140 ENDIF
150 RETURN
300 H=ERR
310 RESUME 150"#,
        " 101  5  0  0\n",
    );
}

// The differential runner reads these two fixtures and four prompt sequences
// directly, so CLI and in-process checks exercise exactly the same sessions.
const STOP_IN_TOP_IF: &str = r#"10 A=1
20 IF 1 THEN
30 STOP
40 A=A+110
50 ENDIF
60 PRINT A
70 END"#;

const STOP_IN_GOSUB_IF: &str = r#"10 A=1
20 IF 1 THEN
30 GOSUB 100
40 A=A+100
50 ENDIF
60 PRINT A
70 END
100 IF 1 THEN
110 STOP
120 A=A+10
130 ENDIF
140 RETURN"#;

const STOP_PROMPT_CASES: &[(&str, &str)] = &[
    ("", " 111\n"),
    ("A=7", " 117\n"),
    ("IF 1 THEN A=7 ELSE A=999", " 117\n"),
    (
        "PRINT \"PROMPT\"\nIF 0 THEN A=999 ELSE A=7",
        "PROMPT\n 117\n",
    ),
];

#[test]
fn prompt_commands_and_inline_ifs_preserve_stopped_top_level_and_gosub_ifs() {
    for source in [STOP_IN_TOP_IF, STOP_IN_GOSUB_IF] {
        for &(immediate, expected) in STOP_PROMPT_CASES {
            for debug in [false, true] {
                let mut interpreter = loaded(source, debug);
                interpreter.process_immediate("RUN").unwrap();
                let stopped = interpreter.take_output();
                assert!(stopped.contains("Program stopped."), "{stopped:?}");
                for command in immediate.lines() {
                    interpreter
                        .process_immediate(command)
                        .unwrap_or_else(|error| {
                            panic!("debug={debug}, command={command:?}: {error:?}\n{source}")
                        });
                }
                interpreter
                    .process_immediate("CONT")
                    .unwrap_or_else(|error| {
                        panic!("debug={debug}, immediate={immediate:?}: {error:?}\n{source}")
                    });
                assert_eq!(
                    interpreter.take_output(),
                    expected,
                    "debug={debug}, immediate={immediate:?}\n{source}"
                );
            }
        }
    }
}

const IMMEDIATE_GOTO_TOP_IF: &str = r#"10 IF 1 THEN
20 STOP
30 PRINT "INSIDE"
40 ENDIF
50 PRINT "DONE""#;

const IMMEDIATE_GOTO_GOSUB_IF: &str = r#"10 IF 1 THEN
20 GOSUB 100
30 ENDIF
40 PRINT "DONE"
50 END
100 IF 1 THEN
110 STOP
120 PRINT "UNREACHABLE"
130 ENDIF
140 RETURN"#;

const IMMEDIATE_GOTO_CASES: &[(&str, &str, &str)] = &[
    (IMMEDIATE_GOTO_TOP_IF, "GOTO 30", "INSIDE\nDONE\n"),
    (IMMEDIATE_GOTO_TOP_IF, "GOTO 50", "DONE\n"),
    (IMMEDIATE_GOTO_GOSUB_IF, "GOTO 140", "DONE\n"),
];

#[test]
fn immediate_goto_preserves_entered_ifs_and_discards_only_abandoned_local_ifs() {
    for &(source, command, expected) in IMMEDIATE_GOTO_CASES {
        for debug in [false, true] {
            let mut interpreter = loaded(source, debug);
            interpreter.process_immediate("RUN").unwrap();
            let stopped = interpreter.take_output();
            assert!(stopped.contains("Program stopped."), "{stopped:?}");
            interpreter
                .process_immediate(command)
                .unwrap_or_else(|error| {
                    panic!("debug={debug}, command={command:?}: {error:?}\n{source}")
                });
            assert_eq!(
                interpreter.take_output(),
                expected,
                "debug={debug}, command={command:?}\n{source}"
            );
        }
    }
}
