use avl_basic::{
    DebugAction, DebugFrameKind, DebugPauseAccess, DebugSnapshot, DebugStatementTarget, DebugValue,
    Debugger, Interpreter, RunOutcome,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::io;
use std::rc::Rc;

fn run(
    source: &str,
    breakpoints: &[i32],
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
            "handler navigation repeated at {:?}",
            snapshot.location
        );
        handler(snapshot, breakpoints, access)
    });
    debugger.replace_breakpoints(breakpoints.iter().copied());
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.set_debugger(debugger);
    let outcome = interpreter.run_loaded().unwrap();
    (interpreter, outcome, calls.get())
}

fn value<'a>(snapshot: &'a DebugSnapshot, name: &str) -> Option<&'a DebugValue> {
    snapshot
        .variables
        .iter()
        .find(|variable| variable.name.eq_ignore_ascii_case(name))
        .map(|variable| &variable.value)
}

fn number(snapshot: &DebugSnapshot, name: &str) -> Option<f64> {
    match value(snapshot, name) {
        Some(DebugValue::Number(value)) => Some(*value),
        _ => None,
    }
}

fn target(snapshot: &DebugSnapshot, line: i32, command: &str) -> DebugStatementTarget {
    let source = snapshot
        .source_lines
        .iter()
        .find(|source| source.starts_with(&format!("{line} ")))
        .unwrap();
    let start = source.find(command).unwrap();
    DebugStatementTarget {
        line,
        source_span: start..start + command.len(),
    }
}

/// Moving the program counter must preserve values and every live continuation.
/// Only the current frame's displayed source line changes with the counter.
fn unchanged_runtime(before: &DebugSnapshot, after: &DebugSnapshot) {
    assert_eq!(before.variables, after.variables);
    assert_eq!(before.arrays, after.arrays);
    assert_eq!(before.array_elements, after.array_elements);
    assert_eq!(before.data, after.data);
    assert_eq!(before.timers, after.timers);
    assert_eq!((before.err, before.erl), (after.err, after.erl));
    assert_eq!(before.source_lines, after.source_lines);
    assert_eq!(before.reason, after.reason);
    assert_eq!(before.stack.len(), after.stack.len());
    for (index, (old, new)) in before.stack.iter().zip(&after.stack).enumerate() {
        assert_eq!((old.kind, &old.name), (new.kind, &new.name));
        if index + 1 < before.stack.len() {
            assert_eq!(old.line, new.line);
        } else {
            assert_eq!(new.line, Some(after.location.line));
        }
    }
}

fn move_to(
    snapshot: &DebugSnapshot,
    access: &mut dyn DebugPauseAccess,
    line: i32,
    command: &str,
) -> DebugSnapshot {
    let destination = target(snapshot, line, command);
    assert!(access.statement_targets().contains(&destination));
    let fresh = access.set_next(&destination).unwrap();
    assert_eq!(fresh.location.line, line);
    assert_eq!(fresh.location.command, command);
    assert_eq!(fresh.location.source_span, Some(destination.source_span));
    unchanged_runtime(snapshot, &fresh);
    fresh
}

#[test]
fn interrupts_sample_can_edit_and_reprint_after_its_timer_takes_the_winning_branch() {
    let phase = Rc::new(Cell::new(0));
    let observed = phase.clone();
    let mut original = String::new();
    let (mut interpreter, outcome, calls) = run(
        include_str!("../samples/interrupts.bas"),
        &[290, 200],
        move |snapshot, breakpoints, access| match observed.get() {
            0 => {
                assert_eq!(snapshot.location.line, 290);
                // The checker may fire once before the first guess is generated.
                if value(snapshot, "A$").is_none() {
                    return Ok(DebugAction::Continue);
                }
                let chosen = value(snapshot, "S$").unwrap().clone();
                let DebugValue::String(text) = &chosen else {
                    panic!("S$ must be a string");
                };
                original = text.to_uppercase();
                let fresh = access.set_scalar("A$", chosen).unwrap();
                assert_eq!(value(&fresh, "A$"), value(&fresh, "S$"));
                breakpoints.remove(&290);
                observed.set(1);
                Ok(DebugAction::Continue)
            }
            1 => {
                assert_eq!(snapshot.location.line, 200);
                assert!(snapshot
                    .stack
                    .iter()
                    .any(|frame| frame.kind == DebugFrameKind::Timer));
                // Stop at END exactly as in the reported workflow, without sounding BEEP.
                let at_end = move_to(snapshot, access, 200, "END");
                let replacement = if original == "Z" { "q" } else { "z" };
                let edited = access
                    .set_scalar("A$", DebugValue::String(replacement.to_string()))
                    .unwrap();
                assert_eq!(edited.location, at_end.location);
                move_to(&edited, access, 190, "PRINT");
                observed.set(2);
                Ok(DebugAction::Continue)
            }
            2 => {
                assert_eq!(snapshot.location.line, 200);
                let DebugValue::String(edited) = value(snapshot, "A$").unwrap() else {
                    panic!("A$ must remain a string");
                };
                assert_ne!(edited.to_uppercase(), original);
                move_to(snapshot, access, 200, "END");
                breakpoints.clear();
                observed.set(3);
                Ok(DebugAction::Continue)
            }
            _ => panic!("unexpected pause at {:?}", snapshot.location),
        },
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(phase.get(), 3);
    assert!((3..=20).contains(&calls));
    let output = interpreter.take_output();
    assert_eq!(
        output.matches("' is correct. I win!").count(),
        2,
        "{output}"
    );
    assert!(output.contains("'Z' is correct. I win!") || output.contains("'Q' is correct. I win!"));
}

#[test]
fn gosub_navigation_preserves_nested_returns_and_the_inline_caller_continuation() {
    let cases = [
        (
            "10 A=0\n20 IF 1 THEN GOSUB 100:A=A+100 ELSE A=9\n30 PRINT A\n40 END\n100 A=A+1\n110 RETURN",
            110,
            100,
            "A=A+1",
            1,
            " 102\n",
        ),
        (
            "10 A=0\n20 IF 1 THEN GOSUB 100:A=A+100 ELSE A=9\n30 PRINT A\n40 END\n100 A=A+1\n110 GOSUB 200\n120 A=A+10\n130 RETURN\n200 A=A+2\n210 RETURN",
            210,
            200,
            "A=A+2",
            2,
            " 115\n",
        ),
    ];
    for (source, pause_line, destination, command, depth, expected) in cases {
        let (mut interpreter, outcome, calls) = run(
            source,
            &[pause_line],
            move |snapshot, breakpoints, access| {
                assert_eq!(
                    snapshot
                        .stack
                        .iter()
                        .filter(|frame| frame.kind == DebugFrameKind::Gosub)
                        .count(),
                    depth
                );
                move_to(snapshot, access, destination, command);
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
fn handler_navigation_preserves_inherited_and_handler_local_loops() {
    let cases = [
        (
            "10 A=0\n20 IF 1 THEN\n30 FOR I=1 TO 2\n40 GOSUB 100\n50 NEXT I\n60 ENDIF\n70 PRINT A;I\n80 END\n100 FOR J=1 TO 1\n110 A=A+1\n120 A=A+2\n130 NEXT J\n140 RETURN",
            120,
            110,
            " 7  3\n",
        ),
        (
            "10 A=0:I=0\n20 WHILE I<2\n30 GOSUB 100\n40 I=I+1\n50 WEND\n60 PRINT A;I\n70 END\n100 J=0\n110 WHILE J<1\n120 A=A+1\n130 A=A+2\n140 J=J+1\n150 WEND\n160 RETURN",
            130,
            120,
            " 7  2\n",
        ),
    ];
    for (source, pause_line, destination, expected) in cases {
        let (mut interpreter, outcome, calls) = run(
            source,
            &[pause_line],
            move |snapshot, breakpoints, access| {
                assert_eq!(number(snapshot, "A"), Some(1.0));
                move_to(snapshot, access, destination, "A=A+1");
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
fn stepping_over_and_out_of_gosub_preserves_the_callers_block_if() {
    for callee in [
        "100 IF 1 THEN\n110 A=A+1\n120 ENDIF\n130 RETURN",
        "100 IF 1 THEN\n110 A=A+1\n120 RETURN\n130 ENDIF\n140 RETURN",
    ] {
        let source = format!(
            "10 A=0\n20 IF 1 THEN\n30 GOSUB 100\n40 A=A+10\n50 ENDIF\n60 PRINT A\n70 END\n{callee}"
        );
        for (pause_line, action) in [(30, DebugAction::StepOver), (110, DebugAction::StepOut)] {
            let mut phase = 0;
            let (mut interpreter, outcome, calls) =
                run(&source, &[pause_line], move |snapshot, breakpoints, _| {
                    assert_eq!((snapshot.err, snapshot.erl), (0, 0));
                    match phase {
                        0 => {
                            assert_eq!(snapshot.location.line, pause_line);
                            assert_eq!(number(snapshot, "A"), Some(0.0));
                            assert_eq!(
                                snapshot
                                    .stack
                                    .iter()
                                    .any(|frame| frame.kind == DebugFrameKind::Gosub),
                                action == DebugAction::StepOut
                            );
                            breakpoints.clear();
                            phase = 1;
                            Ok(action)
                        }
                        1 => {
                            assert_eq!(snapshot.location.line, 40);
                            assert_eq!(number(snapshot, "A"), Some(1.0));
                            assert_eq!(snapshot.stack.len(), 1);
                            phase = 2;
                            Ok(DebugAction::StepInto)
                        }
                        2 => {
                            assert_eq!(snapshot.location.line, 50);
                            assert_eq!(snapshot.location.command, "ENDIF");
                            assert_eq!(number(snapshot, "A"), Some(11.0));
                            phase = 3;
                            Ok(DebugAction::StepInto)
                        }
                        3 => {
                            assert_eq!(snapshot.location.line, 60);
                            assert_eq!(number(snapshot, "A"), Some(11.0));
                            assert_eq!(snapshot.stack.len(), 1);
                            phase = 4;
                            Ok(DebugAction::Continue)
                        }
                        _ => panic!("unexpected pause at {:?}", snapshot.location),
                    }
                });
            assert_eq!(outcome, RunOutcome::End);
            assert_eq!(calls, 4, "{action:?}: {source}");
            assert_eq!(interpreter.take_output(), " 11\n", "{action:?}: {source}");
        }
    }
}

#[test]
fn timer_navigation_preserves_pause_deadline_and_inline_or_gosub_return() {
    for (source, has_gosub) in [
        (
            "10 A=0\n20 AFTER 1,1 GOSUB 100\n30 IF 1 THEN PAUSE 25:DONE=1 ELSE DONE=9\n40 PRINT A;DONE\n50 END\n100 A=A+1\n110 A=A+2\n120 RETURN",
            false,
        ),
        (
            "10 A=0:GOSUB 200\n20 PRINT A;DONE\n30 END\n100 A=A+1\n110 A=A+2\n120 RETURN\n200 AFTER 1,1 GOSUB 100\n210 PAUSE 25:DONE=1\n220 RETURN",
            true,
        ),
    ] {
        let mut phase = 0;
        let (mut interpreter, outcome, calls) = run(source, &[110], move |snapshot, breakpoints, access| {
            assert!(snapshot.stack.iter().any(|frame| frame.kind == DebugFrameKind::Timer));
            assert_eq!(snapshot.stack.iter().any(|frame| frame.kind == DebugFrameKind::Gosub), has_gosub);
            if phase == 0 {
                phase = 1;
                assert_eq!(number(snapshot, "A"), Some(1.0));
                move_to(snapshot, access, 100, "A=A+1");
                return Ok(DebugAction::StepInto);
            }
            assert_eq!(snapshot.location.line, 110);
            assert_eq!(number(snapshot, "A"), Some(2.0));
            assert_eq!(number(snapshot, "DONE"), None);
            breakpoints.clear();
            Ok(DebugAction::Continue)
        });
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 2);
        assert_eq!(interpreter.take_output(), " 4  1\n");
    }
}

#[test]
fn error_handler_navigation_preserves_err_erl_and_each_resume_mode() {
    let cases = [
        (
            "10 ON ERROR GOTO 100\n20 D=0\n30 IF 0 THEN A=9 ELSE A=10/D:B=2\n40 PRINT A;B;H;ERR;ERL\n50 END\n100 D=2\n110 H=H+1\n120 RESUME",
            110,
            6,
            30,
            " 5  2  2  0  0\n",
        ),
        (
            "10 ON ERROR GOTO 100\n20 IF 1 THEN A=1:ERROR 5:B=2 ELSE B=9\n30 PRINT A;B;H;ERR;ERL\n40 END\n100 H=H+1\n110 X=ERR:Y=ERL\n120 RESUME NEXT",
            100,
            5,
            20,
            " 1  2  2  0  0\n",
        ),
        (
            "10 ON ERROR GOTO 100\n20 ERROR 5\n30 SKIPPED=1\n40 PRINT H;ERR;ERL\n50 END\n100 H=H+1\n110 X=ERR\n120 RESUME 40",
            100,
            5,
            20,
            " 2  0  0\n",
        ),
    ];
    for (source, destination, err, erl, expected) in cases {
        let (mut interpreter, outcome, calls) =
            run(source, &[120], move |snapshot, breakpoints, access| {
                assert_eq!((snapshot.err, snapshot.erl), (err, erl));
                assert_eq!(number(snapshot, "H"), Some(1.0));
                move_to(snapshot, access, destination, "H=H+1");
                breakpoints.clear();
                Ok(DebugAction::Continue)
            });
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
        assert_eq!(interpreter.take_output(), expected);
    }
}

#[test]
fn external_error_handler_keeps_suspended_sub_and_function_local_bindings() {
    for (source, kind, expected) in [
        (
            "10 DEF SUB WORK\n20 LOCAL L\n30 L=3\n40 L=10/0\n50 RESULT=L\n60 SUBEND\n70 ON ERROR GOTO 200\n80 L=99:CALL WORK\n90 PRINT RESULT;L;H;ERR;ERL\n100 END\n200 H=H+1\n210 L=L+1\n220 RESUME NEXT",
            DebugFrameKind::Sub,
            " 11  99  1  0  0\n",
        ),
        (
            "10 DEF FNF(X)\n20 LOCAL L\n30 L=X+1\n40 L=10/0\n50 FNF=L*2\n60 FNEND\n70 ON ERROR GOTO 200\n80 L=99:RESULT=FNF(2)\n90 PRINT RESULT;L;H;ERR;ERL\n100 END\n200 H=H+1\n210 L=L+1\n220 RESUME NEXT",
            DebugFrameKind::Function,
            " 22  99  1  0  0\n",
        ),
    ] {
        let (mut interpreter, outcome, calls) = run(source, &[220], move |snapshot, breakpoints, access| {
            assert_eq!((snapshot.err, snapshot.erl), (6, 40));
            assert!(snapshot.stack.iter().any(|frame| frame.kind == kind));
            assert_eq!(number(snapshot, "L"), Some(4.0));
            let edited = access.set_scalar("L", DebugValue::Number(10.0)).unwrap();
            move_to(&edited, access, 210, "L=L+1");
            breakpoints.clear();
            Ok(DebugAction::Continue)
        });
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
        assert_eq!(interpreter.take_output(), expected);
    }
}

#[test]
fn sibling_functions_in_one_gosub_expression_use_the_actual_parent_context() {
    let source = "10 DEF FNA(X)\n20 LOCAL L\n30 L=X+1\n40 FNA=L\n50 FNEND\n60 DEF FNB(X)\n70 LOCAL L\n80 L=X+1\n90 L=L+2\n100 FNB=L\n110 FNEND\n120 GOSUB 200\n130 PRINT RESULT\n140 END\n200 RESULT=FNA(1)+FNB(2)\n210 RETURN";
    let (mut interpreter, outcome, calls) = run(source, &[100], |snapshot, breakpoints, access| {
        assert!(snapshot
            .stack
            .iter()
            .any(|frame| frame.kind == DebugFrameKind::Gosub));
        assert!(snapshot
            .stack
            .iter()
            .any(|frame| frame.kind == DebugFrameKind::Function
                && frame.name.as_deref() == Some("FNB")));
        assert_eq!(number(snapshot, "L"), Some(5.0));
        move_to(snapshot, access, 90, "L=L+2");
        breakpoints.clear();
        Ok(DebugAction::Continue)
    });
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(calls, 1);
    assert_eq!(interpreter.take_output(), " 9\n");
}

#[test]
fn navigation_cannot_enter_leave_or_change_handlers_or_their_local_loop_depth() {
    let handlers = "10 A=7\n20 GOSUB 100:GOSUB 200\n30 END\n100 A=A+1\n110 A=A+2\n120 RETURN\n200 A=A+9\n210 RETURN";
    let loops = "10 A=7\n20 GOSUB 100\n30 END\n100 FOR J=1 TO 2\n110 A=A+1\n120 NEXT J\n130 RETURN";
    let errors = "10 A=7:ON ERROR GOTO 100\n20 ERROR 5\n30 END\n100 A=A+1\n110 A=A+2\n120 RESUME NEXT\n130 A=99\n140 END";
    for (source, pause_line, destination, command) in [
        (handlers, 20, 100, "A=A+1"),
        (handlers, 110, 30, "END"),
        (handlers, 110, 200, "A=A+9"),
        (loops, 100, 110, "A=A+1"),
        (loops, 110, 130, "RETURN"),
        (errors, 110, 30, "END"),
        (errors, 110, 130, "A=99"),
    ] {
        let (_, outcome, calls) = run(source, &[pause_line], move |snapshot, _, access| {
            let error = access
                .set_next(&target(snapshot, destination, command))
                .unwrap_err();
            assert!(!error.is_empty());
            let fresh = access
                .set_scalar("A", DebugValue::Number(number(snapshot, "A").unwrap()))
                .unwrap();
            assert_eq!(fresh.location, snapshot.location);
            unchanged_runtime(snapshot, &fresh);
            Ok(DebugAction::Abort)
        });
        assert_eq!(outcome, RunOutcome::End);
        assert_eq!(calls, 1);
    }
}
