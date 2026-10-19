use avl_basic::{
    DebugAction, DebugFrameKind, DebugValue, Debugger, ErrorCode, Interpreter, RunOutcome,
};
use std::cell::Cell;
use std::rc::Rc;

/// Breakpoints also provide a finite statement budget for broken RESUME paths.
/// A regression must fail the test instead of leaving Cargo in an error loop.
fn loaded(source: &str) -> Interpreter {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    let mut statements = 0;
    let mut debugger = Debugger::interactive_editable(move |_, _, _| {
        statements += 1;
        assert!(statements <= 4096, "error recovery did not terminate");
        Ok(DebugAction::Continue)
    });
    debugger.replace_breakpoints(
        source
            .lines()
            .filter_map(|line| line.split_whitespace().next()?.parse::<i32>().ok()),
    );
    interpreter.set_debugger(debugger);
    interpreter
}

fn output(source: &str) -> String {
    let mut interpreter = loaded(source);
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    interpreter.take_output()
}

#[test]
fn local_sub_handler_retries_with_its_live_parameters_and_locals() {
    assert_eq!(
        output(
            r#"10 D=99:L=88:CALL Divide(10)
20 PRINT Result;D;L;Caught;FaultLine;ERR;ERL
30 END
100 DEF SUB Divide(Value)
110 LOCAL D,L
120 ON ERROR GOTO 180
130 D=0:L=3
140 Result=Value/D+L
150 EXIT SUB
180 Caught=ERR:FaultLine=ERL:D=2:L=L+1
190 RESUME
200 SUBEND"#,
        ),
        " 9  99  88  6  140  0  0\n"
    );
}

#[test]
fn local_function_handler_retries_and_restores_the_callers_bindings() {
    assert_eq!(
        output(
            r#"10 X=99:D=88:PRINT FNDivide(10)
20 PRINT X;D;Caught;FaultLine;ERR;ERL
30 END
100 DEF FNDivide(X)
110 LOCAL D
120 ON ERROR GOTO 180
130 D=0
140 FNDivide=X/D
150 EXIT FN
180 Caught=ERR:FaultLine=ERL:D=2
190 RESUME 0
200 FNEND"#,
        ),
        " 5\n 99  88  6  140  0  0\n"
    );
}

#[test]
fn local_resume_next_keeps_the_tail_of_then_and_else_branches() {
    for condition in [0, 1] {
        let source = format!(
            "10 CALL Work({condition})\n20 PRINT A;B;Hits;ERR;ERL\n30 END\n\
             100 DEF SUB Work(Condition)\n110 ON ERROR GOTO 180\n\
             120 IF Condition THEN A=1:ERROR 5:B=2 ELSE A=3:ERROR 5:B=4\n\
             130 EXIT SUB\n180 Hits=Hits+1\n190 RESUME NEXT\n200 SUBEND"
        );
        assert_eq!(
            output(&source),
            if condition == 1 {
                " 1  2  1  0  0\n"
            } else {
                " 3  4  1  0  0\n"
            }
        );
    }
}

#[test]
fn local_resume_line_skips_faulting_work_and_preserves_the_return_value() {
    for (kind, call, result, exit, end) in [
        ("SUB Work", "CALL Work", "Result", "EXIT SUB", "SUBEND"),
        ("FNWork", "Result=FNWork", "FNWork", "EXIT FN", "FNEND"),
    ] {
        let source = format!(
            "10 {call}\n20 PRINT Result;Skipped;Caught;FaultLine;ERR;ERL\n30 END\n\
             100 DEF {kind}\n110 ON ERROR GOTO 180\n120 ERROR 5\n130 Skipped=1\n\
             140 {result}=7\n150 {exit}\n180 Caught=ERR:FaultLine=ERL\n\
             190 RESUME 140\n200 {end}"
        );
        assert_eq!(output(&source), " 7  0  5  120  0  0\n");
    }
}

#[test]
fn on_error_resume_next_is_local_and_exposes_each_errors_original_line() {
    assert_eq!(
        output(
            r#"10 ON ERROR GOTO 80
20 CALL Work
30 ERROR 5
40 PRINT FirstError;FirstLine;SecondError;SecondLine;LocalTail;GlobalHits
50 END
80 GlobalHits=GlobalHits+1:RESUME NEXT
100 DEF SUB Work
110 ON ERROR RESUME NEXT
120 ERROR 5:FirstError=ERR:FirstLine=ERL
130 A=1/0:SecondError=ERR:SecondLine=ERL
140 LocalTail=1
150 SUBEND"#,
        ),
        " 5  120  6  130  1  1\n"
    );
}

#[test]
fn local_handler_catches_a_callee_and_resume_next_skips_the_call_after_unwind() {
    assert_eq!(
        output(
            r#"10 L=99:CALL Parent
20 PRINT Result;L;Caught;FaultLine;ChildTail;ERR;ERL
30 END
100 DEF SUB Parent
110 LOCAL L
120 L=10:ON ERROR GOTO 180
130 CALL Child:Result=L
140 EXIT SUB
180 Caught=ERR:FaultLine=ERL:L=L+1
190 RESUME NEXT
200 SUBEND
300 DEF SUB Child
310 LOCAL L
320 L=20
330 ERROR 5
340 ChildTail=1
350 SUBEND"#,
        ),
        " 11  99  5  330  0  0  0\n"
    );
}

#[test]
fn ancestor_resume_repeats_the_call_instead_of_the_callees_failed_statement() {
    assert_eq!(
        output(
            r#"10 CALL Parent
20 PRINT Calls;Result;ChildTail;Caught;FaultLine
30 END
100 DEF SUB Parent
110 ON ERROR GOTO 180
120 CALL Child:Result=7
130 EXIT SUB
180 Caught=ERR:FaultLine=ERL:Ready=1
190 RESUME
200 SUBEND
300 DEF SUB Child
310 Calls=Calls+1
320 IF Ready=0 THEN ERROR 5
330 ChildTail=ChildTail+1
340 SUBEND"#,
        ),
        " 2  7  1  5  320\n"
    );
}

#[test]
fn function_ancestor_resume_repeats_the_calling_expression_after_child_unwind() {
    assert_eq!(
        output(
            r#"10 X=99:PRINT FNParent(4)
20 PRINT X;Calls;Caught;FaultLine;ERR;ERL
30 END
100 DEF FNParent(X)
110 ON ERROR GOTO 180
120 FNParent=FNChild(X)+3
130 EXIT FN
180 Caught=ERR:FaultLine=ERL:Ready=1
190 RESUME
200 FNEND
300 DEF FNChild(X)
310 Calls=Calls+1
320 IF Ready=0 THEN ERROR 5
330 FNChild=X*2
340 FNEND"#,
        ),
        " 11\n 99  2  5  320  0  0\n"
    );
}

#[test]
fn function_ancestor_resume_next_skips_the_entire_failed_assignment() {
    assert_eq!(
        output(
            r#"10 PRINT FNParent
20 PRINT Calls;Caught;FaultLine
30 END
100 DEF FNParent
110 LOCAL Answer
120 Answer=7:ON ERROR GOTO 180
130 Answer=FNChild+20:Answer=Answer+1
140 FNParent=Answer:EXIT FN
180 Caught=ERR:FaultLine=ERL
190 RESUME NEXT
200 FNEND
300 DEF FNChild
310 Calls=Calls+1:ERROR 5
320 FNChild=99
330 FNEND"#,
        ),
        " 8\n 1  5  310\n"
    );
}

#[test]
fn disabling_the_child_handler_does_not_disable_the_parents_handler() {
    assert_eq!(
        output(
            r#"10 CALL Parent
20 PRINT ParentHits;ChildHits;AfterChild;ChildTail
30 END
100 DEF SUB Parent
110 ON ERROR GOTO 180
120 CALL Child:AfterChild=1
130 EXIT SUB
180 ParentHits=ParentHits+1:RESUME NEXT
200 SUBEND
300 DEF SUB Child
310 ON ERROR GOTO 380
320 ON ERROR GOTO 0
330 ERROR 5
340 ChildTail=1:EXIT SUB
380 ChildHits=ChildHits+1:RESUME NEXT
400 SUBEND"#,
        ),
        " 1  0  1  0\n"
    );
}

#[test]
fn an_error_in_an_active_handler_reaches_the_nearest_inactive_ancestor() {
    assert_eq!(
        output(
            r#"10 CALL Grandparent
20 PRINT ParentHits;ChildHits;Caught;FaultLine;AfterParent;ChildTail;ParentTail
30 END
100 DEF SUB Grandparent
110 ON ERROR GOTO 180
120 CALL Parent:AfterParent=1
130 EXIT SUB
180 Caught=ERR:FaultLine=ERL:RESUME NEXT
200 SUBEND
300 DEF SUB Parent
310 ON ERROR GOTO 380
320 CALL Child
330 ParentTail=1:EXIT SUB
380 ParentHits=ParentHits+1:ERROR 6
390 RESUME NEXT
400 SUBEND
500 DEF SUB Child
510 ON ERROR GOTO 580
520 ERROR 5
530 ChildTail=1:EXIT SUB
580 ChildHits=ChildHits+1:ERROR 5
590 RESUME NEXT
600 SUBEND"#,
        ),
        " 1  1  6  380  1  0  0\n"
    );
}

#[test]
fn a_handler_can_call_a_helper_without_losing_its_pending_error() {
    assert_eq!(
        output(
            r#"10 CALL Work
20 PRINT OuterError;OuterLine;RestoredError;RestoredLine;HelperError;HelperLine;Tail;ERR;ERL
30 END
100 DEF SUB Work
110 ON ERROR GOTO 180
120 ERROR 5
130 Tail=1:EXIT SUB
180 OuterError=ERR:OuterLine=ERL:CALL Helper
190 RestoredError=ERR:RestoredLine=ERL:RESUME NEXT
200 SUBEND
300 DEF SUB Helper
310 ON ERROR GOTO 380
320 ERROR 6
330 EXIT SUB
380 HelperError=ERR:HelperLine=ERL:RESUME NEXT
400 SUBEND"#,
        ),
        " 5  120  5  120  6  320  1  0  0\n"
    );
}

#[test]
fn unwind_restores_array_aliases_and_local_arrays_before_the_parent_handler() {
    assert_eq!(
        output(
            r#"10 DIM Source(1),Values(0),Temp(0)
20 Source(0)=3:Values(0)=99:Temp(0)=88
30 CALL Parent(Source)
40 PRINT Source(0);Values(0);Temp(0);Observed;ParentBound;UBOUND(Values);UBOUND(Temp)
50 END
100 DEF SUB Parent(Values)
110 LOCAL Temp(2)
120 Temp(0)=7:ON ERROR GOTO 180
130 CALL Child(Values)
140 EXIT SUB
180 Observed=Values(0)+Temp(0):ParentBound=UBOUND(Temp):RESUME NEXT
200 SUBEND
300 DEF SUB Child(Values)
310 LOCAL Temp(4)
320 Values(0)=Values(0)+10:Temp(0)=55
330 ERROR 5
340 Values(0)=999
350 SUBEND"#,
        ),
        " 13  99  88  20  2  0  0\n"
    );
}

#[test]
fn numeric_string_and_array_function_results_survive_local_recovery() {
    assert_eq!(
        output(
            r#"10 DIM Source(1),Answer(1):Source(0)=3:Source(1)=7
20 PRINT FNNumber;FNWord$
30 MAT Answer=FNCopy(Source)
40 PRINT Answer(0);Answer(1);Source(0);Source(1);Hits
50 END
100 DEF FNNumber
110 ON ERROR GOTO 180
120 ERROR 5
130 FNNumber=12:EXIT FN
180 Hits=Hits+1:RESUME NEXT
200 FNEND
300 DEF FNWord$
310 ON ERROR GOTO 380
320 ERROR 5
330 FNWord$="ready":EXIT FN
380 Hits=Hits+1:RESUME NEXT
400 FNEND
500 DEF FNCopy(Values)
510 ON ERROR GOTO 580
520 ERROR 5
530 MAT FNCopy=Values:EXIT FN
580 Hits=Hits+1:RESUME NEXT
600 FNEND"#,
        ),
        " 12 ready\n 3  7  3  7  3\n"
    );
}

#[test]
fn each_call_starts_with_a_fresh_disabled_handler() {
    assert_eq!(
        output(
            r#"10 CALL Parent
20 PRINT ChildHits;ParentHits;Calls;Tail
30 END
100 DEF SUB Parent
110 ON ERROR GOTO 180
120 CALL Child(1):CALL Child(0):Tail=1
130 EXIT SUB
180 ParentHits=ParentHits+1:RESUME NEXT
200 SUBEND
300 DEF SUB Child(Install)
310 Calls=Calls+1
320 IF Install THEN ON ERROR GOTO 380
330 ERROR 5
340 EXIT SUB
380 ChildHits=ChildHits+1:RESUME NEXT
400 SUBEND"#,
        ),
        " 1  1  2  1\n"
    );
}

#[test]
fn normal_and_early_exits_remove_the_local_handler() {
    for exit in ["EXIT SUB", "REM fall through", "ON ERROR GOTO 0:EXIT SUB"] {
        let source = format!(
            "10 ON ERROR GOTO 80\n20 CALL Work\n30 ERROR 5\n\
             40 PRINT GlobalHits;LocalHits;ERR;ERL\n50 END\n\
             80 GlobalHits=GlobalHits+1:RESUME NEXT\n\
             100 DEF SUB Work\n110 ON ERROR GOTO 180\n120 {exit}\n\
             130 GOTO 200\n180 LocalHits=LocalHits+1:RESUME NEXT\n200 SUBEND"
        );
        assert_eq!(output(&source), " 1  0  0  0\n");
    }
}

#[test]
fn resume_next_preserves_block_if_and_loop_continuations() {
    assert_eq!(
        output(
            r#"10 CALL Work
20 PRINT Total;Hits;ERR;ERL
30 END
100 DEF SUB Work
110 LOCAL I
120 ON ERROR GOTO 280
130 FOR I=1 TO 3
140 IF I=2 THEN
150 ERROR 5:Total=Total+10
160 ELSE
170 Total=Total+I
180 ENDIF
190 NEXT I
200 WHILE Total<15
210 ERROR 5:Total=Total+1
220 WEND
230 EXIT SUB
280 Hits=Hits+1:RESUME NEXT
300 SUBEND"#,
        ),
        " 15  2  0  0\n"
    );
}

#[test]
fn global_gosub_helpers_install_a_root_handler_that_unwinds_the_call() {
    assert_eq!(
        output(
            r#"10 CALL Work
20 PRINT Tail;InnerTail;GlobalHits;Caught;FaultLine;ERR;ERL
30 END
80 ON ERROR GOTO 90:RETURN
90 GlobalHits=GlobalHits+1:Caught=ERR:FaultLine=ERL:RESUME NEXT
100 DEF SUB Work
110 GOSUB 80
120 ERROR 5:InnerTail=1
130 Tail=1
140 SUBEND"#,
        ),
        " 0  0  1  5  120  0  0\n"
    );
}

#[test]
fn root_handler_restores_bindings_and_skips_the_failed_sub_or_function() {
    for (kind, invoke, result, exit, end) in [
        ("SUB Work", "CALL Work", "Result", "EXIT SUB", "SUBEND"),
        ("FNWork", "Result=FNWork", "FNWork", "EXIT FN", "FNEND"),
    ] {
        let source = format!(
            "10 L=99:ON ERROR GOTO 80\n20 {invoke}\n\
             30 PRINT Result;L;Caught;FaultLine;ERR;ERL\n40 END\n\
             80 Caught=ERR:FaultLine=ERL:L=L+1:RESUME NEXT\n\
             100 DEF {kind}\n110 LOCAL L\n120 L=3\n130 ERROR 5\n\
             140 {result}=L\n150 {exit}\n200 {end}"
        );
        assert_eq!(output(&source), " 0  100  5  130  0  0\n");
    }
}

#[test]
fn root_on_error_resume_next_unwinds_and_preserves_the_original_error() {
    assert_eq!(
        output(
            r#"10 ON ERROR RESUME NEXT
20 CALL Work
30 PRINT Result;AfterError;Caught;FaultLine;ERR;ERL
40 END
100 DEF SUB Work
110 LOCAL L
120 L=7:ERROR 5:AfterError=1:Caught=ERR:FaultLine=ERL
130 Result=L
140 SUBEND"#,
        ),
        " 0  0  0  0  5  120\n"
    );
}

#[test]
fn root_only_pera_handler_continues_after_the_outermost_call() {
    let source = r#"100 DEF SUB PERA1(A)
110 PRINT "pera1"
115 PRINT A/0
120 PRINT "pera1 final"
125 EXIT SUB
140 SUBEND
145 DEF SUB PERA2(A)
155 PRINT "pera2"
160 CALL PERA1(1)
165 PRINT "pera2 final"
170 EXIT SUB
185 SUBEND
190 ON ERROR GOTO 215
195 PRINT "root"
200 CALL PERA2(2)
205 PRINT "root final"
210 END
215 PRINT ERL;" - root error"
220 RESUME NEXT"#;
    for run in [output, fast_output] {
        assert_eq!(
            run(source),
            "root\npera2\npera1\n 115  - root error\nroot final\n"
        );
    }
}

#[test]
fn root_resume_repeats_the_entire_nested_call_or_function_expression() {
    for resume in ["RESUME", "RESUME 0"] {
        for (invoke, parent, parent_body, parent_end, leaf, leaf_body, leaf_end) in [
            (
                "CALL Parent(3)",
                "SUB Parent(X)",
                "Result=FNChild(X)+10",
                "SUBEND",
                "FNChild(X)",
                "FNChild=X*2",
                "FNEND",
            ),
            (
                "Result=FNParent(3)+10",
                "FNParent(X)",
                "FNParent=FNChild(X)",
                "FNEND",
                "FNChild(X)",
                "FNChild=X*2",
                "FNEND",
            ),
        ] {
            let source = format!(
                "10 X=99:ON ERROR GOTO 80\n20 {invoke}:RootTail=1\n\
                 30 PRINT Result;X;ParentCalls;ChildCalls;Caught;FaultLine;Observed;RootTail;ERR;ERL\n40 END\n\
                 80 Caught=ERR:FaultLine=ERL:Observed=X:Ready=1:{resume}\n\
                 100 DEF {parent}\n110 ParentCalls=ParentCalls+1\n120 {parent_body}\n130 {parent_end}\n\
                 300 DEF {leaf}\n310 ChildCalls=ChildCalls+1\n320 IF Ready=0 THEN ERROR 5\n330 {leaf_body}\n340 {leaf_end}"
            );
            for run in [output, fast_output] {
                assert_eq!(
                    run(&source),
                    " 16  99  2  2  5  320  99  1  0  0\n",
                    "{source}"
                );
            }
        }
    }
}

#[test]
fn root_recovery_restores_aliases_local_arrays_and_root_control_flow() {
    for policy in ["ON ERROR GOTO 80", "ON ERROR RESUME NEXT"] {
        let source = format!(
            "10 DIM Source(0),Values(0),Temp(0):Values(0)=99:Temp(0)=88:I=77\n\
             20 {policy}\n30 FOR N=1 TO 2\n40 IF N>0 THEN\n\
             50 CALL Parent(Source):Total=Total+N:Caught=ERR:FaultLine=ERL\n\
             60 ENDIF\n65 NEXT N\n70 PRINT Source(0);Values(0);Temp(0);UBOUND(Temp);I;Total;ChildTail;ParentTail;Observed;Hits;Caught;FaultLine;ERR;ERL:END\n\
             80 Observed=Values(0)+Temp(0)+I:Hits=Hits+1:RESUME NEXT\n\
             100 DEF SUB Parent(Values)\n110 LOCAL Temp(2),I\n120 Temp(0)=7\n\
             130 FOR I=1 TO 2\n140 IF I>0 THEN\n150 Answer=FNChild(Values)\n\
             160 ENDIF\n170 NEXT I\n180 ParentTail=1\n190 SUBEND\n\
             300 DEF FNChild(Values)\n310 LOCAL Temp(4),I\n320 Temp(0)=55:Values(0)=Values(0)+1\n\
             330 FOR I=1 TO 2\n340 IF I>0 THEN\n350 ERROR 5:ChildTail=1\n\
             360 ENDIF\n370 NEXT I\n380 FNChild=7\n390 FNEND"
        );
        for run in [output, fast_output] {
            assert_eq!(
                run(&source),
                if policy.ends_with("80") {
                    " 2  99  88  0  77  3  0  0  264  2  0  0  0  0\n"
                } else {
                    " 2  99  88  0  77  3  0  0  0  0  5  350  5  350\n"
                },
                "{source}"
            );
        }
    }
}

#[test]
fn root_implicit_recovery_does_not_make_an_explicit_resume_valid() {
    let source = "10 ON ERROR RESUME NEXT\n20 ERROR 5:Captured=ERR:CapturedLine=ERL\n30 RESUME NEXT\n40 PRINT Captured;CapturedLine;ERR;ERL\n50 END";
    for run in [output, fast_output] {
        assert_eq!(
            run(source),
            format!(" 5  20  {}  30\n", ErrorCode::HandlerError.number())
        );
    }
}

#[test]
fn a_root_gosub_helper_can_resume_its_callers_active_local_handler() {
    let source = r#"10 CALL Work
20 PRINT Tail;Hits;ERR;ERL:END
80 RESUME NEXT
100 DEF SUB Work
110 ON ERROR GOTO 180
120 ERROR 5:Tail=1
130 EXIT SUB
180 Hits=Hits+1:GOSUB 80
190 SUBEND"#;
    for run in [output, fast_output] {
        assert_eq!(run(source), " 1  1  0  0\n");
    }
}

#[test]
fn root_recovery_unwinds_a_timer_interruption_created_inside_the_failed_call() {
    for (invocation, declaration, terminator) in [
        ("CALL Work", "SUB Work", "SUBEND"),
        ("Result=FNWork", "FNWork", "FNEND"),
    ] {
        let source = format!(
            "10 ON ERROR GOTO 150\n20 AFTER 2 GOSUB 80\n30 {invocation}\n\
             40 AFTER 1 GOSUB 90\n50 PAUSE 30\n60 PRINT Hits;Caught;WorkTail:END\n\
             80 ERROR 5:RETURN\n90 Hits=Hits+1:RETURN\n150 Caught=Caught+1:RESUME NEXT\n\
             200 DEF {declaration}\n210 PAUSE 100\n220 WorkTail=1\n230 {terminator}"
        );
        for run in [output, fast_output] {
            assert_eq!(run(&source), " 1  1  0\n", "{source}");
        }
    }
}

#[test]
fn root_recovery_keeps_a_timer_interruption_that_was_running_before_the_call() {
    let source = r#"10 ON ERROR GOTO 150
20 AFTER 1 GOSUB 80
30 PAUSE 30
40 AFTER 1 GOSUB 90
50 PAUSE 30
60 PRINT Hits;Caught;WorkTail;IsrTail:END
80 CALL Work:IsrTail=1:RETURN
90 Hits=Hits+1:RETURN
150 Caught=Caught+1:RESUME NEXT
200 DEF SUB Work
210 ERROR 5:WorkTail=1
220 SUBEND"#;
    for run in [output, fast_output] {
        assert_eq!(run(source), " 1  1  0  1\n");
    }
}

#[test]
fn root_resume_discards_unfinished_handler_loops_before_restoring_the_caller() {
    let source = r#"10 ON ERROR GOTO 100
20 FOR I=1 TO 2
30 ERROR 5:Hits=Hits+1
40 NEXT
50 PRINT Hits;I;J:END
100 FOR J=1 TO 1
110 RESUME NEXT
120 NEXT J"#;
    for run in [output, fast_output] {
        assert_eq!(run(source), " 2  3  1\n");
    }
}

#[test]
fn root_resume_line_cannot_reenter_an_abandoned_routine() {
    let source = "10 ON ERROR GOTO 80\n20 CALL Work\n30 END\n80 RESUME 120\n100 DEF SUB Work\n110 ERROR 5\n120 EXIT SUB\n130 SUBEND";
    let mut interpreter = loaded(source);
    assert_eq!(
        interpreter.run_loaded().unwrap_err().code,
        ErrorCode::InvalidTargetLine
    );
}

#[test]
fn root_resume_unwinds_a_timer_interruption_created_inside_the_handler() {
    let source = r#"10 ON ERROR GOTO 100
20 ERROR 5:Tail=1
30 AFTER 1 GOSUB 90
40 PAUSE 30
50 PRINT Hits;Caught;Tail;ERR;ERL:END
80 RESUME NEXT
90 Hits=Hits+1:RETURN
100 Caught=Caught+1:AFTER 1 GOSUB 80:PAUSE 30
110 RESUME NEXT"#;
    for run in [output, fast_output] {
        assert_eq!(run(source), " 1  1  1  0  0\n");
    }
}

#[test]
fn on_error_targets_cannot_cross_routine_boundaries() {
    for source in [
        "10 CALL Work\n20 END\n80 END\n100 DEF SUB Work\n110 ON ERROR GOTO 80\n120 SUBEND",
        "10 CALL Work\n20 END\n100 DEF SUB Work\n110 ON ERROR GOTO 310\n120 SUBEND\n300 DEF SUB Other\n310 EXIT SUB\n320 SUBEND",
        "10 PRINT FNWork\n20 END\n80 END\n100 DEF FNWork\n110 ON ERROR GOTO 80\n120 FNWork=1\n130 FNEND",
        "10 ON ERROR GOTO 110\n20 END\n100 DEF SUB Work\n110 EXIT SUB\n120 SUBEND",
    ] {
        let mut interpreter = loaded(source);
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::InvalidTargetLine,
            "{source}"
        );
    }
}

#[test]
fn local_resume_target_cannot_enter_another_routine_or_top_level() {
    for destination in [80, 310] {
        let source = format!(
            "10 CALL Work\n20 END\n80 END\n\
             100 DEF SUB Work\n110 ON ERROR GOTO 180\n120 ERROR 5\n130 EXIT SUB\n\
             180 RESUME {destination}\n200 SUBEND\n\
             300 DEF SUB Other\n310 EXIT SUB\n320 SUBEND"
        );
        let mut interpreter = loaded(&source);
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::InvalidTargetLine,
            "{source}"
        );
    }
}

#[test]
fn disabled_local_handlers_leave_unhandled_errors_visible() {
    let mut interpreter = loaded(
        "10 CALL Work\n20 END\n100 DEF SUB Work\n110 ON ERROR GOTO 180\n120 ON ERROR GOTO 0\n130 ERROR 5\n140 EXIT SUB\n180 RESUME NEXT\n200 SUBEND",
    );
    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(
        (error.code, error.line),
        (ErrorCode::TypeMismatch, Some(130))
    );
}

#[test]
fn missing_local_handler_targets_report_the_normal_missing_line_error() {
    let mut interpreter =
        loaded("10 CALL Work\n20 END\n100 DEF SUB Work\n110 ON ERROR GOTO 999\n120 SUBEND");
    assert_eq!(
        interpreter.run_loaded().unwrap_err().code,
        ErrorCode::TargetLineNotFound
    );
}

#[test]
fn handlers_may_exit_the_routine_without_resume() {
    for (kind, invoke, assignment, exit, end) in [
        ("SUB Work", "CALL Work", "Result=7", "EXIT SUB", "SUBEND"),
        ("FNWork", "Result=FNWork", "FNWork=7", "EXIT FN", "FNEND"),
    ] {
        for terminate in [exit, "REM fall through to terminator"] {
            let source = format!(
                "10 {invoke}\n20 {invoke}\n30 PRINT Result;Hits;ERR;ERL\n40 END\n\
                 100 DEF {kind}\n110 ON ERROR GOTO 180\n120 ERROR 5\n130 {exit}\n\
                 180 Hits=Hits+1:{assignment}\n190 {terminate}\n200 {end}"
            );
            assert_eq!(output(&source), " 7  2  0  0\n", "{source}");
        }
    }
}

#[test]
fn resume_without_a_pending_local_error_remains_an_error() {
    for (kind, invoke, end) in [
        ("SUB Work", "CALL Work", "SUBEND"),
        ("FNWork", "Result=FNWork", "FNEND"),
    ] {
        let source = format!("10 {invoke}\n20 END\n100 DEF {kind}\n110 RESUME NEXT\n120 {end}");
        let mut interpreter = loaded(&source);
        assert_eq!(
            interpreter.run_loaded().unwrap_err().code,
            ErrorCode::HandlerError,
            "{source}"
        );
    }
}

#[test]
fn errors_in_a_gosub_keep_the_procedures_handler_and_return_continuation() {
    assert_eq!(
        output(
            r#"10 CALL Work
20 PRINT Hits;InnerTail;OuterTail;Caught;FaultLine;ERR;ERL
30 END
80 ERROR 5:InnerTail=1
90 RETURN
100 DEF SUB Work
110 ON ERROR GOTO 180
120 GOSUB 80:OuterTail=1
130 EXIT SUB
180 Hits=Hits+1:Caught=ERR:FaultLine=ERL:RESUME NEXT
200 SUBEND"#,
        ),
        " 1  1  1  5  80  0  0\n"
    );
}

#[test]
fn local_on_error_resume_next_skips_an_unhandled_callee_after_cleanup() {
    assert_eq!(
        output(
            r#"10 L=99:CALL Parent
20 PRINT ParentResult;L;Caught;FaultLine;ChildTail;ParentTail
30 END
100 DEF SUB Parent
110 LOCAL L
120 L=10:ON ERROR RESUME NEXT
130 CALL Child:Caught=ERR:FaultLine=ERL:ParentTail=1
140 ParentResult=L
150 SUBEND
300 DEF SUB Child
310 LOCAL L
320 L=20:ERROR 5
330 ChildTail=1
340 SUBEND"#,
        ),
        " 10  99  5  320  0  1\n"
    );
}

#[test]
fn a_local_handler_precedes_the_root_handler() {
    assert_eq!(
        output(
            r#"10 ON ERROR GOTO 80
20 CALL Work
30 PRINT LocalHits;GlobalHits;Tail
40 END
80 GlobalHits=GlobalHits+1:RESUME NEXT
100 DEF SUB Work
110 ON ERROR GOTO 180
120 ERROR 5:Tail=1
130 EXIT SUB
180 LocalHits=LocalHits+1:RESUME NEXT
200 SUBEND"#,
        ),
        " 1  0  1\n"
    );
}

#[test]
fn a_function_handler_can_call_a_function_with_its_own_handler() {
    assert_eq!(
        output(
            r#"10 PRINT FNWork
20 PRINT OuterError;OuterLine;RestoredError;RestoredLine;HelperError;HelperLine;HelperResult;ERR;ERL
30 END
100 DEF FNWork
110 ON ERROR GOTO 180
120 ERROR 5
130 FNWork=7:EXIT FN
180 OuterError=ERR:OuterLine=ERL:HelperResult=FNHelper
190 RestoredError=ERR:RestoredLine=ERL:RESUME NEXT
200 FNEND
300 DEF FNHelper
310 ON ERROR GOTO 380
320 ERROR 6
330 FNHelper=9:EXIT FN
380 HelperError=ERR:HelperLine=ERL:RESUME NEXT
400 FNEND"#,
        ),
        " 7\n 5  120  5  120  6  320  9  0  0\n"
    );
}

#[test]
fn routines_without_error_handlers_keep_nested_binding_and_exit_behavior() {
    assert_eq!(
        output(
            r#"10 DIM Source(1),Values(0),Temp(0)
20 Source(0)=3:Source(1)=7:Values(0)=99:Temp(0)=88
30 CALL Parent(Source):PRINT FNOuter(Source)
40 PRINT Source(0);Source(1);Values(0);Temp(0);UBOUND(Temp);Total
50 END
100 DEF SUB Parent(Values)
110 LOCAL Temp(2),I
120 Values(0)=Values(0)+1
130 FOR I=1 TO 2
140 CALL Child(Values)
150 NEXT I
160 Total=SUM(Values):EXIT SUB
170 Total=999
180 SUBEND
200 DEF SUB Child(Values)
210 LOCAL Temp(4)
220 Temp(0)=77:Values(0)=Values(0)+1
230 SUBEND
300 DEF FNOuter(Values)
310 LOCAL Total
320 Total=FNInner(Values)
330 FNOuter=Total*2:EXIT FN
340 FNOuter=999
350 FNEND
400 DEF FNInner(Values)
410 LOCAL Temp(3)
420 Temp(0)=SUM(Values):FNInner=Temp(0)
430 FNEND"#,
        ),
        " 26\n 6  7  99  88  0  13\n"
    );
}

#[test]
fn a_root_handler_keeps_its_pending_error_while_a_local_helper_recovers() {
    assert_eq!(
        output(
            r#"10 ON ERROR GOTO 80
20 ERROR 5:Tail=1
30 PRINT OuterError;OuterLine;RestoredError;RestoredLine;HelperError;HelperLine;Tail;ERR;ERL
40 END
80 OuterError=ERR:OuterLine=ERL:CALL Helper
90 RestoredError=ERR:RestoredLine=ERL:RESUME NEXT
100 DEF SUB Helper
110 ON ERROR GOTO 180
120 ERROR 6
130 EXIT SUB
180 HelperError=ERR:HelperLine=ERL:RESUME NEXT
200 SUBEND"#,
        ),
        " 5  20  5  20  6  120  1  0  0\n"
    );
}

#[test]
fn array_returning_function_unwind_restores_aliases_before_ancestor_recovery() {
    assert_eq!(
        output(
            r#"10 DIM Source(1),Answer(1),Values(0),Temp(0)
20 Source(0)=3:Source(1)=7:Values(0)=99:Temp(0)=88
30 MAT Answer=FNParent(Source)
40 PRINT Answer(0);Answer(1);Source(0);Values(0);Temp(0);Observed;ParentBound;Caught;FaultLine
50 END
100 DEF FNParent(Values)
110 LOCAL Temp(2)
120 Temp(0)=7:ON ERROR GOTO 180
130 MAT FNParent=FNChild(Values)
140 MAT FNParent=Values+1:EXIT FN
180 Observed=Values(0)+Temp(0):ParentBound=UBOUND(Temp):Caught=ERR:FaultLine=ERL:RESUME NEXT
200 FNEND
300 DEF FNChild(Values)
310 LOCAL Temp(4)
320 Values(0)=Values(0)+10:Temp(0)=55
330 ERROR 5
340 MAT FNChild=Values+100
350 FNEND"#,
        ),
        " 14  8  13  99  88  20  2  5  330\n"
    );
}

#[test]
fn a_single_line_function_error_propagates_to_the_callers_local_handler() {
    assert_eq!(
        output(
            r#"10 X=99:D=0:PRINT FNParent(2)
20 PRINT X;Observed;Caught;FaultLine
30 END
100 DEF FNParent(X)
110 ON ERROR GOTO 180
120 FNParent=FNChild(X+2)+X
130 EXIT FN
180 Observed=X:Caught=ERR:FaultLine=ERL:D=2:RESUME
200 FNEND
300 DEF FNChild(X)=X/D"#,
        ),
        " 4\n 99  2  6  120\n"
    );
}

#[test]
fn manual_ratio_example_handles_both_sub_and_function_errors() {
    let manual = include_str!("../MANUAL.txt").replace("\r\n", "\n");
    let section = manual
        .split("Local handlers in a subroutine and a multiline function")
        .nth(1)
        .unwrap();
    let source = section
        .split("```basic\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    assert_eq!(
        output(source),
        "Cannot calculate ratio; error  6  at line  120\n 4\n 0\n"
    );
}

#[test]
fn inline_if_can_select_either_local_handler_mode_in_subs_and_functions() {
    for (kind, invocation, result, exit, end) in [
        (
            "SUB Work(SelectHandler)",
            "CALL Work",
            "Result",
            "EXIT SUB",
            "SUBEND",
        ),
        (
            "FNWork(SelectHandler)",
            "Result=FNWork",
            "FNWork",
            "EXIT FN",
            "FNEND",
        ),
    ] {
        for mode in [0, 1] {
            let source = format!(
                "10 {invocation}({mode})\n20 PRINT Result;Hits;Observed;FaultLine;ERR;ERL\n30 END\n\
                 100 DEF {kind}\n\
                 110 IF SelectHandler THEN ON ERROR GOTO 180 ELSE ON ERROR RESUME NEXT\n\
                 120 ERROR 5:Observed=ERR:FaultLine=ERL:{result}=7\n130 {exit}\n\
                 180 Hits=Hits+1:RESUME NEXT\n200 {end}"
            );
            assert_eq!(
                output(&source),
                if mode == 0 {
                    " 7  0  5  120  0  0\n"
                } else {
                    " 7  1  0  0  0  0\n"
                },
                "{source}"
            );
        }
    }
}

#[test]
fn handler_control_flow_does_not_replace_the_interrupted_loop_or_if_state() {
    assert_eq!(
        output(
            r#"10 I=99:CALL Work
20 PRINT Total;Hits;HelperHits;I;ERR;ERL
30 END
80 HelperHits=HelperHits+1:RETURN
100 DEF SUB Work
110 LOCAL I,J
120 ON ERROR GOTO 280
130 FOR I=1 TO 3
140 IF I>0 THEN
150 ERROR 5:Total=Total+I
160 ELSE
170 Total=999
180 ENDIF
190 NEXT I
200 EXIT SUB
280 IF ERR=5 THEN
290 FOR J=1 TO 2
300 Hits=Hits+1
310 NEXT J
320 GOSUB 80
330 ENDIF
340 RESUME NEXT
350 SUBEND"#,
        ),
        " 6  6  3  99  0  0\n"
    );
}

#[test]
fn debugger_navigation_keeps_the_local_handlers_pending_error_and_bindings() {
    let source = r#"10 L=99:CALL Work
20 PRINT Result;L;ERR;ERL
30 END
100 DEF SUB Work
110 LOCAL L
120 L=3:ON ERROR GOTO 180
130 ERROR 5
140 Result=L:EXIT SUB
180 L=L+1
190 RESUME NEXT
200 SUBEND"#;
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let mut debugger = Debugger::interactive_editable(move |snapshot, breakpoints, access| {
        observed.set(observed.get() + 1);
        assert_eq!((snapshot.err, snapshot.erl), (5, 130));
        assert!(snapshot
            .stack
            .iter()
            .any(|frame| frame.kind == DebugFrameKind::Sub));
        let edited = access.set_scalar("L", DebugValue::Number(10.0)).unwrap();
        assert_eq!((edited.err, edited.erl), (5, 130));
        let destination = access
            .statement_targets()
            .into_iter()
            .find(|target| target.line == 180)
            .unwrap();
        let moved = access.set_next(&destination).unwrap();
        assert_eq!((moved.err, moved.erl), (5, 130));
        breakpoints.clear();
        Ok(DebugAction::Continue)
    });
    debugger.replace_breakpoints([190]);
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.set_debugger(debugger);
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(calls.get(), 1);
    assert_eq!(interpreter.take_output(), " 11  99  0  0\n");
}

fn fast_output(source: &str) -> String {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    interpreter.take_output()
}

#[test]
fn fast_dispatch_installs_inline_sub_and_function_handlers() {
    for (kind, invoke, result, exit, end) in [
        ("SUB Work", "CALL Work", "Result", "EXIT SUB", "SUBEND"),
        ("FNWork", "Result=FNWork", "FNWork", "EXIT FN", "FNEND"),
    ] {
        let source = format!(
            "10 {invoke}\n20 PRINT Result;X;Hits;ERR;ERL\n30 END\n\
             100 DEF {kind}\n\
             110 IF 1 THEN ON ERROR GOTO 180 ELSE ON ERROR RESUME NEXT\n\
             120 X=1/D:{result}=7\n130 {exit}\n\
             180 Hits=Hits+1:D=1:RESUME NEXT\n200 {end}"
        );
        // Repairing D also makes an accidental retry finite (and observable).
        assert_eq!(fast_output(&source), " 7  0  1  0  0\n");
    }
}

#[test]
fn fast_dispatch_resume_next_keeps_the_selected_inline_if_branch() {
    for condition in [0, 1] {
        let source = format!(
            "10 CALL Work({condition})\n20 PRINT A;B;X;Hits\n30 END\n\
             100 DEF SUB Work(Condition)\n110 ON ERROR GOTO 180\n\
             120 IF Condition THEN A=1:X=10/D:B=2 ELSE A=3:X=10/D:B=4\n\
             130 EXIT SUB\n180 Hits=Hits+1:D=1:RESUME NEXT\n200 SUBEND"
        );
        assert_eq!(
            fast_output(&source),
            if condition == 0 {
                " 3  4  0  1\n"
            } else {
                " 1  2  0  1\n"
            }
        );
    }
}

#[test]
fn fast_dispatch_ancestor_resume_repeats_the_call_after_unwind() {
    assert_eq!(
        fast_output(
            r#"10 CALL Parent
20 PRINT Calls;Result;ChildTail;Caught;FaultLine
30 END
100 DEF SUB Parent
110 ON ERROR GOTO 180
120 CALL Child:Result=7
130 EXIT SUB
180 Caught=ERR:FaultLine=ERL:Ready=1
190 RESUME
200 SUBEND
300 DEF SUB Child
310 Calls=Calls+1
320 IF Ready=0 THEN ERROR 5
330 ChildTail=ChildTail+1
340 SUBEND"#,
        ),
        " 2  7  1  5  320\n"
    );
}

#[test]
fn root_recovery_discards_an_abandoned_local_handlers_pending_error() {
    assert_eq!(
        output(
            r#"10 ON ERROR GOTO 80
20 CALL Work
30 PRINT LocalError;LocalLine;GlobalError;GlobalLine;RestoredError;RestoredLine;Tail;ERR;ERL
40 END
80 GlobalError=ERR:GlobalLine=ERL:RESUME NEXT
100 DEF SUB Work
110 ON ERROR GOTO 180
120 ERROR 5
130 Tail=1:EXIT SUB
180 LocalError=ERR:LocalLine=ERL:ERROR 6:RestoredError=ERR:RestoredLine=ERL
190 RESUME NEXT
200 SUBEND"#,
        ),
        " 5  120  6  180  0  0  0  0  0\n"
    );
}

#[test]
fn root_implicit_resume_discards_an_abandoned_local_handler() {
    assert_eq!(
        output(
            r#"10 ON ERROR RESUME NEXT
20 CALL Work
30 PRINT LocalError;LocalLine;RestoredError;RestoredLine;Tail;ERR;ERL
40 END
100 DEF SUB Work
110 ON ERROR GOTO 180
120 ERROR 5
130 Tail=1:EXIT SUB
180 LocalError=ERR:LocalLine=ERL:ERROR 6:RestoredError=ERR:RestoredLine=ERL
190 RESUME NEXT
200 SUBEND"#,
        ),
        " 5  120  0  0  0  6  180\n"
    );
}

#[test]
fn implicit_local_recovery_does_not_make_an_explicit_resume_valid() {
    assert_eq!(
        output(
            r#"10 CALL Work
20 PRINT Captured;CapturedLine;Observed;ObservedLine;ERR;ERL
30 END
100 DEF SUB Work
110 ON ERROR RESUME NEXT
120 ERROR 5:Captured=ERR:CapturedLine=ERL
130 RESUME NEXT
140 Observed=ERR:ObservedLine=ERL
150 SUBEND"#,
        ),
        format!(" 5  120  {}  130  0  0\n", ErrorCode::HandlerError.number())
    );
}

#[test]
fn every_local_on_error_policy_clears_visible_err_and_erl() {
    for (kind, invoke, result, exit, end) in [
        ("SUB Work", "CALL Work", "Result", "EXIT SUB", "SUBEND"),
        ("FNWork", "Result=FNWork", "FNWork", "EXIT FN", "FNEND"),
    ] {
        for policy in [
            "ON ERROR GOTO 180",
            "ON ERROR RESUME NEXT",
            "ON ERROR GOTO 0",
        ] {
            let source = format!(
                "10 {invoke}\n20 PRINT Captured;CapturedLine;Observed;ObservedLine;Result;Hits;ERR;ERL\n30 END\n\
                 100 DEF {kind}\n110 ON ERROR RESUME NEXT\n\
                 120 ERROR 5:Captured=ERR:CapturedLine=ERL\n130 {policy}\n\
                 140 Observed=ERR:ObservedLine=ERL:{result}=7\n150 {exit}\n\
                 180 Hits=Hits+1:RESUME NEXT\n200 {end}"
            );
            assert_eq!(output(&source), " 5  120  0  0  7  0  0  0\n", "{source}");
        }
    }
}

#[test]
fn replacing_the_policy_inside_a_handler_clears_err_but_keeps_its_pending_resume() {
    for policy in [
        "ON ERROR GOTO 220",
        "ON ERROR RESUME NEXT",
        "ON ERROR GOTO 0",
    ] {
        let source = format!(
            "10 CALL Work\n20 PRINT Captured;CapturedLine;Observed;ObservedLine;Tail;Hits;ERR;ERL\n30 END\n\
             100 DEF SUB Work\n110 ON ERROR GOTO 180\n120 ERROR 5\n130 Tail=1:EXIT SUB\n\
             180 Captured=ERR:CapturedLine=ERL:{policy}\n\
             190 Observed=ERR:ObservedLine=ERL:RESUME NEXT\n\
             220 Hits=Hits+1:RESUME NEXT\n240 SUBEND"
        );
        assert_eq!(output(&source), " 5  120  0  0  1  0  0  0\n", "{source}");
    }
}

#[test]
fn disabling_an_active_handler_preserves_resume_but_does_not_catch_later_errors() {
    assert_eq!(
        output(
            r#"10 CALL Parent
20 PRINT Captured;CapturedLine;Observed;ObservedLine;Tail;ChildHits;ParentError;ParentLine;AfterChild
30 END
100 DEF SUB Parent
110 ON ERROR GOTO 180
120 CALL Child:AfterChild=1
130 EXIT SUB
180 ParentError=ERR:ParentLine=ERL:RESUME NEXT
200 SUBEND
300 DEF SUB Child
310 ON ERROR GOTO 380
320 ERROR 5
330 Tail=1
340 ERROR 6
350 EXIT SUB
380 ChildHits=ChildHits+1:Captured=ERR:CapturedLine=ERL:ON ERROR GOTO 0
390 Observed=ERR:ObservedLine=ERL:RESUME NEXT
400 SUBEND"#,
        ),
        " 5  320  0  0  1  1  6  340  1\n"
    );
}

#[test]
fn root_fallback_abandons_a_disabled_local_handlers_pending_resume() {
    for (policy, hits) in [("ON ERROR GOTO 80", 1), ("ON ERROR RESUME NEXT", 0)] {
        let source = format!(
            "10 {policy}\n20 CALL Work\n\
             30 PRINT Captured;CapturedLine;AfterReset;ResetLine;Restored;RestoredLine;Tail;GlobalHits;ERR;ERL\n40 END\n\
             80 GlobalHits=GlobalHits+1:RESUME NEXT\n\
             100 DEF SUB Work\n110 ON ERROR GOTO 180\n120 ERROR 5\n130 Tail=1:EXIT SUB\n\
             180 Captured=ERR:CapturedLine=ERL:ON ERROR GOTO 0\n\
             190 AfterReset=ERR:ResetLine=ERL\n\
             200 ERROR 6:Restored=ERR:RestoredLine=ERL\n210 RESUME NEXT\n220 SUBEND"
        );
        assert_eq!(
            output(&source),
            format!(
                " 5  120  0  0  0  0  0  {hits}  {}  {}\n",
                if hits == 0 { 6 } else { 0 },
                if hits == 0 { 200 } else { 0 },
            ),
            "{source}"
        );
    }
}
