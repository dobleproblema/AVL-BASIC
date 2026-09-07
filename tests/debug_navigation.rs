use avl_basic::{
    DebugAction, DebugDataSnapshot, DebugFrameKind, DebugPauseAccess, DebugPauseReason,
    DebugSnapshot, DebugStatementTarget, DebugValue, Debugger, Interpreter, RunOutcome,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::io;
use std::rc::Rc;

fn run(
    source: &str,
    breakpoints: &[i32],
    pause_first: bool,
    mut handler: impl FnMut(
            &DebugSnapshot,
            &mut HashSet<i32>,
            &mut dyn DebugPauseAccess,
        ) -> io::Result<DebugAction>
        + 'static,
) -> (Interpreter, RunOutcome, usize) {
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let mut debugger = Debugger::interactive_editable(move |snapshot, breakpoints, access| {
        let count = observed.get() + 1;
        observed.set(count);
        assert!(
            count <= 20,
            "navigation repeated indefinitely at {:?}",
            snapshot.location
        );
        handler(snapshot, breakpoints, access)
    });
    debugger.replace_breakpoints(breakpoints.iter().copied());
    if pause_first {
        debugger.request_pause();
    }
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.set_debugger(debugger);
    let outcome = interpreter.run_loaded().unwrap();
    (interpreter, outcome, calls.get())
}

fn number(snapshot: &DebugSnapshot, name: &str) -> Option<f64> {
    snapshot
        .variables
        .iter()
        .find(|variable| variable.name.eq_ignore_ascii_case(name))
        .and_then(|variable| match variable.value {
            DebugValue::Number(value) => Some(value),
            _ => None,
        })
}

fn source_line(snapshot: &DebugSnapshot, line: i32) -> &str {
    snapshot
        .source_lines
        .iter()
        .find(|source| source.starts_with(&format!("{line} ")))
        .map(String::as_str)
        .unwrap()
}

fn target(snapshot: &DebugSnapshot, line: i32, text: &str) -> DebugStatementTarget {
    let source = source_line(snapshot, line);
    let start = source.find(text).unwrap();
    DebugStatementTarget {
        line,
        source_span: start..start + text.len(),
    }
}

fn unchanged_runtime(before: &DebugSnapshot, after: &DebugSnapshot) {
    assert_eq!(before.variables, after.variables);
    assert_eq!(before.array_elements, after.array_elements);
    assert_eq!(before.arrays, after.arrays);
    assert_eq!(before.stack.len(), after.stack.len());
    for (index, (old, new)) in before.stack.iter().zip(&after.stack).enumerate() {
        assert_eq!(old.kind, new.kind);
        assert_eq!(old.name, new.name);
        if index + 1 < before.stack.len() {
            assert_eq!(old.line, new.line);
        } else {
            assert_eq!(new.line, Some(after.location.line));
        }
    }
    assert_eq!(before.data, after.data);
    assert_eq!(before.timers, after.timers);
    assert_eq!((before.err, before.erl), (after.err, after.erl));
    assert_eq!(before.source_lines, after.source_lines);
}

#[test]
fn edited_values_can_rewind_to_an_earlier_statement_without_executing_it() {
    let source = "10 A=1\n20 A=A+1:B=A+10\n30 C=A\n40 PRINT A;B;C\n50 END";
    for pause_line in [20, 30] {
        let mut phase = 0;
        let (mut interpreter, outcome, calls) = run(
            source,
            &[pause_line],
            false,
            move |snapshot, breakpoints, access| {
                if pause_line == 20 && phase == 0 {
                    phase += 1;
                    assert_eq!(snapshot.location.command, "A=A+1");
                    return Ok(DebugAction::StepInto);
                }
                if phase == usize::from(pause_line == 20) {
                    phase += 1;
                    assert_eq!(
                        snapshot.location.command,
                        if pause_line == 20 { "B=A+10" } else { "C=A" }
                    );
                    let edited = access.set_scalar("A", DebugValue::Number(40.0)).unwrap();
                    let destination = target(snapshot, 20, "A=A+1");
                    assert!(access.statement_targets().contains(&destination));
                    let fresh = access.set_next(&destination).unwrap();
                    assert_eq!(fresh.location.line, 20);
                    assert_eq!(fresh.location.command, "A=A+1");
                    assert_eq!(fresh.location.source_span, Some(destination.source_span));
                    assert_eq!(fresh.reason, snapshot.reason);
                    unchanged_runtime(&edited, &fresh);
                    assert_eq!(number(&fresh, "A"), Some(40.0));
                    access.idle().unwrap();
                    return Ok(DebugAction::StepInto);
                }
                assert_eq!(snapshot.location.command, "B=A+10");
                assert_eq!(number(snapshot, "A"), Some(41.0));
                assert_eq!(
                    number(snapshot, "B"),
                    if pause_line == 20 { None } else { Some(12.0) }
                );
                breakpoints.clear();
                Ok(DebugAction::Continue)
            },
        );
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, if pause_line == 20 { 3 } else { 2 });
        assert_eq!(interpreter.take_output(), " 41  51  41\n");
    }
}

#[test]
fn forward_destinations_skip_statements_without_fabricating_their_effects() {
    for (line, command, output) in [(20, "B=7", " 1  7  3\n"), (30, "C=3", " 1  0  3\n")] {
        let (mut interpreter, outcome, calls) = run(
            "10 A=1\n20 A=99:B=7\n30 C=3\n40 PRINT A;B;C\n50 END",
            &[20],
            false,
            move |snapshot, _, access| {
                let fresh = access.set_next(&target(snapshot, line, command)).unwrap();
                assert_eq!(fresh.location.command, command);
                unchanged_runtime(snapshot, &fresh);
                assert_eq!(number(&fresh, "A"), Some(1.0));
                assert_eq!(number(&fresh, "B"), None);
                Ok(DebugAction::Continue)
            },
        );
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
        assert_eq!(interpreter.take_output(), output);
    }
}

#[test]
fn destinations_are_complete_top_level_statements_with_numbered_utf8_spans() {
    let source = "10 S$=\"á:中🙂\":A=1\n20 IF A THEN GOSUB 100:PRINT \"then:x\" ELSE PRINT \"else:y\"\n30 END\n100 RETURN";
    let (_, outcome, calls) = run(source, &[], true, |snapshot, _, access| {
        let destinations = access.statement_targets();
        let fragments = destinations
            .iter()
            .map(|destination| {
                (
                    destination.line,
                    source_line(snapshot, destination.line)
                        .get(destination.source_span.clone())
                        .unwrap()
                        .to_string(),
                )
            })
            .collect::<Vec<_>>();
        assert!(fragments.contains(&(10, "S$=\"á:中🙂\"".into())));
        assert!(fragments.contains(&(10, "A=1".into())));
        assert!(fragments.contains(&(
            20,
            "IF A THEN GOSUB 100:PRINT \"then:x\" ELSE PRINT \"else:y\"".into()
        )));
        assert_eq!(fragments.iter().filter(|(line, _)| *line == 20).count(), 1);
        assert!(!fragments
            .iter()
            .any(|(_, fragment)| fragment == "GOSUB 100" || fragment.starts_with("PRINT")));
        assert!(access.set_next(&target(snapshot, 20, "GOSUB 100")).is_err());
        let invalid_utf8 = source_line(snapshot, 10).find('中').unwrap() + 1;
        assert!(access
            .set_next(&DebugStatementTarget {
                line: 10,
                source_span: invalid_utf8..invalid_utf8 + 1
            })
            .is_err());
        assert!(access
            .set_next(&DebugStatementTarget {
                line: 999,
                source_span: 3..6
            })
            .is_err());
        let fresh = access
            .set_next(&target(snapshot, 10, "S$=\"á:中🙂\""))
            .unwrap();
        unchanged_runtime(snapshot, &fresh);
        Ok(DebugAction::Abort)
    });
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(calls, 1);
}

#[test]
fn setting_the_next_statement_rejects_unbalanced_blocks_and_routine_boundaries() {
    let cases = [
        (
            "10 MARK=7\n20 FOR I=1 TO 2\n30 MARK=MARK+1\n40 NEXT I\n50 END",
            20,
            30,
            "MARK=MARK+1",
        ),
        (
            "10 MARK=7\n20 FOR I=1 TO 2\n30 MARK=MARK+1\n40 NEXT I\n50 END",
            30,
            50,
            "END",
        ),
        (
            "10 MARK=7\n20 FOR I=1 TO 2:MARK=MARK+1:NEXT I\n30 END",
            20,
            20,
            "MARK=MARK+1",
        ),
        (
            "10 MARK=7\n20 WHILE MARK<9\n30 MARK=MARK+1\n40 WEND\n50 END",
            20,
            30,
            "MARK=MARK+1",
        ),
        (
            "10 MARK=7\n20 WHILE MARK<9\n30 MARK=MARK+1\n40 WEND\n50 END",
            30,
            50,
            "END",
        ),
        (
            "10 MARK=7\n20 IF MARK=7 THEN\n30 MARK=8\n40 ELSE\n50 MARK=9\n60 ENDIF\n70 END",
            20,
            30,
            "MARK=8",
        ),
        (
            "10 MARK=7\n20 IF MARK=7 THEN\n30 MARK=8\n40 ELSE\n50 MARK=9\n60 ENDIF\n70 END",
            30,
            50,
            "MARK=9",
        ),
        (
            "10 MARK=7\n20 IF MARK=7 THEN\n30 MARK=8\n40 ELSE\n50 MARK=9\n60 ENDIF\n70 END",
            30,
            40,
            "ELSE",
        ),
        (
            "10 DEF SUB WORK\n20 MARK=MARK+1\n30 SUBEND\n40 MARK=7\n50 CALL WORK\n60 END",
            50,
            20,
            "MARK=MARK+1",
        ),
        (
            "10 DEF SUB WORK\n20 MARK=MARK+1\n30 SUBEND\n40 MARK=7\n50 CALL WORK\n60 END",
            20,
            60,
            "END",
        ),
    ];
    for (source, pause, destination, command) in cases {
        let (_, outcome, calls) = run(source, &[pause], false, move |snapshot, _, access| {
            let error = access
                .set_next(&target(snapshot, destination, command))
                .expect_err("unsafe destination accepted");
            assert!(!error.is_empty());
            let fresh = access
                .set_scalar(
                    "MARK",
                    DebugValue::Number(number(snapshot, "MARK").unwrap()),
                )
                .unwrap();
            assert_eq!(fresh.location, snapshot.location);
            unchanged_runtime(snapshot, &fresh);
            Ok(DebugAction::Abort)
        });
        assert_eq!(outcome, RunOutcome::End, "{source}");
        assert_eq!(calls, 1, "{source}");
    }
}

#[test]
fn destinations_inside_the_same_active_block_are_allowed() {
    for (source, output) in [
        ("10 MARK=0\n20 FOR I=1 TO 1\n30 MARK=1\n40 MARK=2\n50 NEXT I\n60 PRINT MARK\n70 END", " 2\n"),
        ("10 MARK=0\n20 WHILE MARK<1\n30 UNUSED=99\n40 MARK=MARK+1\n50 WEND\n60 PRINT MARK\n70 END", " 1\n"),
        ("10 MARK=0\n20 IF 1 THEN\n30 MARK=1\n40 MARK=2\n50 ELSE\n60 MARK=3\n70 ENDIF\n80 PRINT MARK\n90 END", " 2\n"),
    ] {
        let (mut interpreter, outcome, calls) = run(source, &[30], false, |snapshot, breakpoints, access| {
            let command = source_line(snapshot, 40).split_once(' ').unwrap().1;
            let fresh = access.set_next(&target(snapshot, 40, command)).unwrap();
            unchanged_runtime(snapshot, &fresh);
            breakpoints.clear();
            Ok(DebugAction::Continue)
        });
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
        assert_eq!(interpreter.take_output(), output);
    }
}

#[test]
fn the_same_active_sub_and_function_can_change_the_next_arithmetic_statement() {
    let cases = [
        ("10 DEF SUB WORK\n20 LOCAL L\n30 L=2\n40 L=L+1\n50 RESULT=L\n60 SUBEND\n70 L=99:CALL WORK\n80 PRINT RESULT;L\n90 END", DebugFrameKind::Sub, " 11  99\n"),
        ("10 DEF FNF(X)\n20 LOCAL L\n30 L=X+1\n40 L=L+1\n50 FNF=L*2\n60 FNEND\n70 L=99:RESULT=FNF(2)\n80 PRINT RESULT;L\n90 END", DebugFrameKind::Function, " 22  99\n"),
    ];
    for (source, kind, output) in cases {
        let mut phase = 0;
        let (mut interpreter, outcome, calls) = run(
            source,
            &[50],
            false,
            move |snapshot, breakpoints, access| {
                assert!(snapshot.stack.iter().any(|frame| frame.kind == kind));
                if phase == 0 {
                    phase += 1;
                    let edited = access.set_scalar("L", DebugValue::Number(10.0)).unwrap();
                    let fresh = access.set_next(&target(snapshot, 40, "L=L+1")).unwrap();
                    unchanged_runtime(&edited, &fresh);
                    assert_eq!(fresh.location.command, "L=L+1");
                    return Ok(DebugAction::StepInto);
                }
                assert_eq!(snapshot.location.line, 50);
                assert_eq!(number(snapshot, "L"), Some(11.0));
                breakpoints.clear();
                Ok(DebugAction::Continue)
            },
        );
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 2);
        assert_eq!(interpreter.take_output(), output);
    }
}

#[test]
fn elseif_and_else_bodies_allow_local_moves_but_reject_branch_changes() {
    for (elseif_condition, pause, destination, rejected, expected) in
        [(1, 50, 60, 80, " 3\n"), (0, 80, 90, 50, " 5\n")]
    {
        let source = format!("10 MARK=0\n20 IF 0 THEN\n30 MARK=1\n40 ELSEIF {elseif_condition} THEN\n50 MARK=2\n60 MARK=3\n70 ELSE\n80 MARK=4\n90 MARK=5\n100 ENDIF\n110 PRINT MARK\n120 END");
        let (mut interpreter, outcome, calls) = run(
            &source,
            &[pause],
            false,
            move |snapshot, breakpoints, access| {
                let wrong_command = source_line(snapshot, rejected).split_once(' ').unwrap().1;
                assert!(access
                    .set_next(&target(snapshot, rejected, wrong_command))
                    .is_err());
                let command = source_line(snapshot, destination)
                    .split_once(' ')
                    .unwrap()
                    .1;
                let fresh = access
                    .set_next(&target(snapshot, destination, command))
                    .unwrap();
                unchanged_runtime(snapshot, &fresh);
                breakpoints.clear();
                Ok(DebugAction::Continue)
            },
        );
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
        assert_eq!(interpreter.take_output(), expected);
    }
}

#[test]
fn active_sub_and_function_moves_preserve_loops_in_their_caller() {
    let cases = [
        ("10 DEF SUB WORK\n20 LOCAL L\n30 L=2\n40 L=L+1\n50 RESULT=L\n60 SUBEND\n70 L=99:FOR I=1 TO 2\n80 CALL WORK\n90 TOTAL=TOTAL+RESULT:NEXT I\n100 PRINT TOTAL;I;L\n110 END", " 14  3  99\n"),
        ("10 DEF FNF(X)\n20 LOCAL L\n30 L=X+1\n40 L=L+1\n50 FNF=L*2\n60 FNEND\n70 L=99:FOR I=1 TO 2\n80 RESULT=FNF(I)\n90 TOTAL=TOTAL+RESULT:NEXT I\n100 PRINT TOTAL;I;L\n110 END", " 30  3  99\n"),
        ("10 DEF SUB WORK\n20 LOCAL L\n25 LOCAL J\n30 L=2:FOR J=1 TO 1\n40 L=L+1\n50 RESULT=L\n60 NEXT J\n65 SUBEND\n70 L=99:FOR I=1 TO 2\n80 CALL WORK\n90 TOTAL=TOTAL+RESULT:NEXT I\n100 PRINT TOTAL;I;L\n110 END", " 14  3  99\n"),
    ];
    for (source, expected) in cases {
        let mut phase = 0;
        let (mut interpreter, outcome, calls) = run(
            source,
            &[50],
            false,
            move |snapshot, breakpoints, access| {
                assert_eq!(number(snapshot, "I"), Some(1.0));
                if phase == 0 {
                    phase += 1;
                    let edited = access.set_scalar("L", DebugValue::Number(10.0)).unwrap();
                    let fresh = access.set_next(&target(snapshot, 40, "L=L+1")).unwrap();
                    unchanged_runtime(&edited, &fresh);
                    return Ok(DebugAction::StepInto);
                }
                assert_eq!(snapshot.location.line, 50);
                assert_eq!(number(snapshot, "L"), Some(11.0));
                breakpoints.clear();
                Ok(DebugAction::Continue)
            },
        );
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 2);
        assert_eq!(interpreter.take_output(), expected);
    }
}

#[test]
fn a_nested_function_move_affects_only_the_active_call() {
    let source = "10 DEF FNG(X)\n20 LOCAL L\n30 L=X\n40 L=L+1\n50 FNG=L\n60 FNEND\n70 DEF FNF(X)\n80 LOCAL L\n90 L=FNG(X)\n100 FNF=L+1\n110 FNEND\n120 RESULT=FNF(0)\n130 PRINT RESULT\n140 END";
    let mut phase = 0;
    let (mut interpreter, outcome, calls) = run(
        source,
        &[50],
        false,
        move |snapshot, breakpoints, access| {
            assert_eq!(
                snapshot
                    .stack
                    .iter()
                    .filter(|frame| frame.kind == DebugFrameKind::Function)
                    .count(),
                2
            );
            assert_eq!(number(snapshot, "X"), Some(0.0));
            if phase == 0 {
                phase += 1;
                let edited = access.set_scalar("L", DebugValue::Number(10.0)).unwrap();
                let fresh = access.set_next(&target(snapshot, 40, "L=L+1")).unwrap();
                unchanged_runtime(&edited, &fresh);
                return Ok(DebugAction::StepInto);
            }
            assert_eq!(snapshot.location.line, 50);
            assert_eq!(number(snapshot, "L"), Some(11.0));
            breakpoints.clear();
            Ok(DebugAction::Continue)
        },
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(calls, 2);
    assert_eq!(interpreter.take_output(), " 12\n");
}

#[test]
fn active_gosub_timer_and_error_contexts_allow_local_moves_but_reject_exiting() {
    for source in [
        "10 MARK=7\n20 GOSUB 100\n30 END\n100 MARK=8\n110 MARK=9\n120 RETURN",
        "10 MARK=7\n20 AFTER 1,1 GOSUB 100\n30 PAUSE 25\n40 END\n100 MARK=8\n110 MARK=9\n120 RETURN",
        "10 MARK=7\n20 ON ERROR GOTO 100\n30 X=1/0\n40 END\n100 MARK=8\n110 MARK=9\n120 RESUME NEXT",
    ] {
        let (_, outcome, calls) = run(source, &[100], false, |snapshot, _, access| {
            assert!(access.set_next(&target(snapshot, 10, "MARK=7")).is_err());
            let fresh = access.set_scalar("MARK", DebugValue::Number(7.0)).unwrap();
            unchanged_runtime(snapshot, &fresh);
            assert_eq!(snapshot.location, fresh.location);
            let moved = access.set_next(&target(snapshot, 110, "MARK=9")).unwrap();
            unchanged_runtime(snapshot, &moved);
            assert_eq!(moved.location.line, 110);
            assert_eq!(moved.location.command, "MARK=9");
            assert!(access.set_next(&target(snapshot, 10, "MARK=7")).is_err());
            let restored = access.set_next(&target(snapshot, 100, "MARK=8")).unwrap();
            unchanged_runtime(snapshot, &restored);
            assert_eq!(snapshot.location, restored.location);
            Ok(DebugAction::Abort)
        });
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
    }
}

#[test]
fn an_inline_child_can_return_to_its_complete_if_guard() {
    let mut phase = 0;
    let (mut interpreter, outcome, calls) = run(
        "10 X=1\n20 IF X THEN A=1:B=2 ELSE A=9:B=8\n30 PRINT A;B\n40 END",
        &[20],
        false,
        move |snapshot, breakpoints, access| {
            phase += 1;
            match phase {
                1 | 2 => Ok(DebugAction::StepInto),
                3 => {
                    assert_eq!(snapshot.location.command, "B=2");
                    let edited = access.set_scalar("X", DebugValue::Number(0.0)).unwrap();
                    let whole_if = source_line(snapshot, 20).split_once(' ').unwrap().1;
                    let fresh = access.set_next(&target(snapshot, 20, whole_if)).unwrap();
                    unchanged_runtime(&edited, &fresh);
                    assert_eq!(fresh.location.command, whole_if);
                    breakpoints.clear();
                    Ok(DebugAction::Continue)
                }
                _ => panic!("unexpected re-entry"),
            }
        },
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(calls, 3);
    assert_eq!(interpreter.take_output(), " 9  8\n");
}

fn assert_restart_reset(snapshot: &DebugSnapshot, expected_reason: DebugPauseReason) {
    assert_eq!(snapshot.location.line, 10);
    assert_eq!(snapshot.location.command, "BOOT=1");
    assert_eq!(snapshot.reason, expected_reason);
    assert!(
        snapshot.variables.is_empty(),
        "stale variables: {:?}",
        snapshot.variables
    );
    assert!(snapshot.arrays.is_empty());
    assert!(snapshot.array_elements.is_empty());
    assert!(snapshot.timers.is_empty());
    assert_eq!((snapshot.err, snapshot.erl), (0, 0));
    assert_eq!(snapshot.stack.len(), 1);
    assert_eq!(snapshot.stack[0].kind, DebugFrameKind::Program);
    assert!(matches!(
        &snapshot.data,
        DebugDataSnapshot::Next {
            position: 0,
            line: 900,
            line_item: 1,
            value: DebugValue::Number(11.0)
        }
    ));
}

#[test]
fn restart_unwinds_nested_execution_and_pauses_cleanly_with_breakpoints_preserved() {
    let cases = [
        ("SUB", "10 BOOT=1\n20 DEF SUB WORK\n30 LOCAL L\n40 L=7\n50 L=L+1\n60 SUBEND\n100 DIM BUF(2):BUF(1)=9\n110 READ R\n120 AFTER 100000,1 GOSUB 800\n130 S$=\"dirty\"\n140 CALL WORK\n150 PRINT R;BUF(1):END\n800 RETURN\n900 DATA 11,22", 50, "L=L+1", Some(DebugFrameKind::Sub), " 11  9\n"),
        ("FN", "10 BOOT=1\n20 DEF FNF(X)\n30 LOCAL L\n40 L=X+1\n50 FNF=L*2\n60 FNEND\n100 DIM BUF(2):BUF(1)=9\n110 READ R\n120 AFTER 100000,1 GOSUB 800\n130 S$=\"dirty\"\n140 RESULT=FNF(3)\n150 PRINT RESULT;R;BUF(1):END\n800 RETURN\n900 DATA 11,22", 50, "FNF=L*2", Some(DebugFrameKind::Function), " 8  11  9\n"),
        ("GOSUB", "10 BOOT=1\n20 DIM BUF(2):BUF(1)=9\n30 READ R\n40 AFTER 100000,1 GOSUB 800\n50 S$=\"dirty\"\n60 GOSUB 200\n70 PRINT R;BUF(1):END\n200 GOSUB 300\n210 RETURN\n300 MARK=3\n310 RETURN\n800 RETURN\n900 DATA 11,22", 300, "MARK=3", Some(DebugFrameKind::Gosub), " 11  9\n"),
        ("TIMER", "10 BOOT=1\n20 DIM BUF(2):BUF(1)=9\n30 READ R\n40 S$=\"dirty\"\n50 AFTER 1,1 GOSUB 800\n60 PAUSE 25\n70 PRINT R;BUF(1):END\n800 FIRED=1\n810 RETURN\n900 DATA 11,22", 800, "FIRED=1", Some(DebugFrameKind::Timer), " 11  9\n"),
        ("ERROR", "10 BOOT=1\n20 DIM BUF(2):BUF(1)=9\n30 READ R\n40 AFTER 100000,1 GOSUB 800\n50 ON ERROR GOTO 600\n60 A=1/0\n70 PRINT R;BUF(1):END\n600 SEEN=ERR\n610 RESUME NEXT\n800 RETURN\n900 DATA 11,22", 600, "SEEN=ERR", None, " 11  9\n"),
        ("INLINE", "10 BOOT=1\n20 DIM BUF(2):BUF(1)=9\n30 READ R\n40 AFTER 100000,1 GOSUB 800\n50 S$=\"dirty\"\n60 FOR I=1 TO 2\n70 IF 1 THEN A=1:B=2 ELSE A=9\n80 NEXT I\n90 PRINT A;B;R;BUF(1):END\n800 RETURN\n900 DATA 11,22", 70, "B=2", None, " 1  2  11  9\n"),
    ];
    for (index, (label, source, breakpoint, trigger, frame, output)) in
        cases.into_iter().enumerate()
    {
        let mut restarted = false;
        let mut reset_seen = false;
        let add_first_breakpoint = index % 2 == 0;
        let (mut interpreter, outcome, calls) = run(
            source,
            &[breakpoint],
            false,
            move |snapshot, breakpoints, _| {
                if !restarted {
                    if snapshot.location.command != trigger {
                        return Ok(DebugAction::StepInto);
                    }
                    assert_eq!(number(snapshot, "BOOT"), Some(1.0));
                    assert!(!snapshot.arrays.is_empty());
                    assert!(!snapshot.timers.is_empty());
                    assert!(matches!(
                        snapshot.data,
                        DebugDataSnapshot::Next { position: 1, .. }
                    ));
                    if let Some(kind) = frame {
                        assert!(
                            snapshot.stack.iter().any(|frame| frame.kind == kind),
                            "{label}"
                        );
                    }
                    if label == "ERROR" {
                        assert_eq!((snapshot.err, snapshot.erl), (6, 60));
                    }
                    if label == "GOSUB" {
                        assert_eq!(
                            snapshot
                                .stack
                                .iter()
                                .filter(|frame| frame.kind == DebugFrameKind::Gosub)
                                .count(),
                            2
                        );
                    }
                    if add_first_breakpoint {
                        breakpoints.insert(10);
                    }
                    restarted = true;
                    return Ok(DebugAction::Restart);
                }
                if !reset_seen {
                    assert_restart_reset(
                        snapshot,
                        if add_first_breakpoint {
                            DebugPauseReason::Breakpoint
                        } else {
                            DebugPauseReason::PauseRequested
                        },
                    );
                    assert!(breakpoints.contains(&breakpoint));
                    assert_eq!(breakpoints.contains(&10), add_first_breakpoint);
                    breakpoints.remove(&10);
                    reset_seen = true;
                    return Ok(DebugAction::Continue);
                }
                assert_eq!(snapshot.location.line, breakpoint, "{label}");
                assert_eq!(number(snapshot, "BOOT"), Some(1.0));
                breakpoints.clear();
                Ok(DebugAction::Continue)
            },
        );
        assert_eq!(outcome, RunOutcome::End, "{label}");
        assert_eq!(calls, if label == "INLINE" { 5 } else { 3 }, "{label}");
        assert_eq!(interpreter.take_output(), output, "{label}");
    }
}
