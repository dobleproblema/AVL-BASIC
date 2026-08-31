use avl_basic::{
    DebugAction, DebugDataSnapshot, DebugFrameKind, DebugPauseReason, DebugSnapshot, DebugValue,
    Debugger, ErrorCode, Interpreter, RunOutcome,
};
use std::collections::BTreeSet;

const TIMER_PAUSE_SOURCE: &str =
    "10 AFTER 5,1 GOSUB 100:PAUSE 200\n20 DONE=1:PRINT DONE\n30 END\n100 FIRED=1\n110 RETURN";

fn debugger_with_breakpoints(
    breakpoints: impl IntoIterator<Item = i32>,
    actions: impl IntoIterator<Item = DebugAction>,
) -> Debugger {
    let mut debugger = Debugger::scripted(actions);
    debugger.replace_breakpoints(breakpoints);
    debugger
}

fn run_scripted(source: &str, debugger: Debugger) -> (Interpreter, RunOutcome) {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.set_debugger(debugger);
    let outcome = interpreter.run_loaded().unwrap();
    (interpreter, outcome)
}

fn snapshots(interpreter: &Interpreter) -> Vec<DebugSnapshot> {
    interpreter.debugger().unwrap().snapshots().to_vec()
}

fn locations(snapshots: &[DebugSnapshot]) -> Vec<(i32, usize)> {
    snapshots
        .iter()
        .map(|snapshot| (snapshot.location.line, snapshot.location.statement))
        .collect()
}

fn command_trace(snapshots: &[DebugSnapshot]) -> Vec<(i32, &str)> {
    snapshots
        .iter()
        .map(|snapshot| (snapshot.location.line, snapshot.location.command.as_str()))
        .collect()
}

fn source_fragment(snapshot: &DebugSnapshot) -> &str {
    let span = snapshot
        .location
        .source_span
        .clone()
        .expect("visible debugger locations must have a source span");
    snapshot
        .location
        .source
        .get(span)
        .expect("debugger source span must be a valid UTF-8 byte range")
}

fn step_into_then_continue(count: usize) -> Vec<DebugAction> {
    let mut actions = vec![DebugAction::StepInto; count];
    actions.push(DebugAction::Continue);
    actions
}

fn numeric_value(snapshot: &DebugSnapshot, name: &str) -> Option<f64> {
    snapshot
        .variables
        .iter()
        .find(|variable| variable.name.eq_ignore_ascii_case(name))
        .and_then(|variable| match variable.value {
            DebugValue::Number(value) => Some(value),
            DebugValue::String(_) => None,
        })
}

fn array_element<'a>(snapshot: &'a DebugSnapshot, name: &str) -> Option<&'a DebugValue> {
    snapshot
        .array_elements
        .iter()
        .find(|variable| variable.name.eq_ignore_ascii_case(name))
        .map(|variable| &variable.value)
}

#[test]
fn data_snapshot_tracks_line_item_value_restore_and_exhaustion() {
    let source = r#"10 DATA 10,20:DATA "A"
20 DATA 30
30 READ A
40 READ B:READ C$
50 RESTORE 10
60 READ D,E,F$,G
70 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [30],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        locations(&snapshots),
        [(30, 0), (40, 0), (40, 1), (50, 0), (60, 0), (70, 0)]
    );
    assert_eq!(
        snapshots[0].data,
        DebugDataSnapshot::Next {
            position: 0,
            line: 10,
            line_item: 1,
            value: DebugValue::Number(10.0),
        }
    );
    assert_eq!(
        snapshots[1].data,
        DebugDataSnapshot::Next {
            position: 1,
            line: 10,
            line_item: 2,
            value: DebugValue::Number(20.0),
        }
    );
    assert_eq!(
        snapshots[2].data,
        DebugDataSnapshot::Next {
            position: 2,
            line: 10,
            line_item: 3,
            value: DebugValue::String(String::from("A")),
        }
    );
    assert_eq!(
        snapshots[3].data,
        DebugDataSnapshot::Next {
            position: 3,
            line: 20,
            line_item: 1,
            value: DebugValue::Number(30.0),
        }
    );
    assert_eq!(
        snapshots[4].data,
        DebugDataSnapshot::Next {
            position: 0,
            line: 10,
            line_item: 1,
            value: DebugValue::Number(10.0),
        }
    );
    assert_eq!(
        snapshots[5].data,
        DebugDataSnapshot::Exhausted {
            position: 4,
            total: 4,
        }
    );

    let (empty, outcome) = run_scripted(
        "10 END",
        debugger_with_breakpoints([10], [DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(
        empty.debugger().unwrap().snapshots()[0].data,
        DebugDataSnapshot::Empty
    );
}

#[test]
fn array_element_snapshot_keeps_one_last_scalar_write_per_array() {
    let source = r#"10 DIM A(20),B$(2)
20 A(7)=9
30 A(12)=8
40 B$(1)="X"
50 REDIM A(5)
60 MAT B$=B$
70 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        locations(&snapshots),
        [(20, 0), (30, 0), (40, 0), (50, 0), (60, 0), (70, 0)]
    );
    assert!(snapshots[0].array_elements.is_empty());
    assert_eq!(
        array_element(&snapshots[1], "A(7)"),
        Some(&DebugValue::Number(9.0))
    );
    assert_eq!(snapshots[1].array_elements.len(), 1);
    assert_eq!(
        array_element(&snapshots[2], "A(12)"),
        Some(&DebugValue::Number(8.0))
    );
    assert_eq!(array_element(&snapshots[2], "A(7)"), None);
    assert_eq!(
        array_element(&snapshots[3], "A(12)"),
        Some(&DebugValue::Number(8.0))
    );
    assert_eq!(
        array_element(&snapshots[3], "B$(1)"),
        Some(&DebugValue::String(String::from("X")))
    );
    assert_eq!(snapshots[3].array_elements.len(), 2);
    assert_eq!(array_element(&snapshots[4], "A(12)"), None);
    assert_eq!(
        array_element(&snapshots[4], "B$(1)"),
        Some(&DebugValue::String(String::from("X")))
    );
    assert!(snapshots[5].array_elements.is_empty());
}

#[test]
fn copied_function_array_starts_without_inherited_write_history() {
    let source = r#"10 DEF FNCOPY(X)
20 MARK=1
30 X(1)=7
40 MAT FNCOPY=X
50 FNEND
60 DIM A(2)
70 A(1)=5
80 MAT R=FNCOPY(A)
90 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(20, 0), (30, 0), (40, 0)]);
    assert_eq!(
        array_element(&snapshots[0], "A(1)"),
        Some(&DebugValue::Number(5.0))
    );
    assert_eq!(array_element(&snapshots[0], "X(1)"), None);
    assert_eq!(array_element(&snapshots[1], "X(1)"), None);
    assert_eq!(
        array_element(&snapshots[2], "X(1)"),
        Some(&DebugValue::Number(7.0))
    );
}

#[test]
fn failed_array_write_does_not_replace_the_last_successful_element() {
    let source = r#"10 DIM A(2)
20 A(1)=5
30 ON ERROR RESUME NEXT
40 A(5)=9
50 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(20, 0), (30, 0), (40, 0), (50, 0)]);
    assert_eq!(
        array_element(&snapshots[1], "A(1)"),
        Some(&DebugValue::Number(5.0))
    );
    assert_eq!(
        array_element(&snapshots[3], "A(1)"),
        Some(&DebugValue::Number(5.0))
    );
    assert_eq!(snapshots[3].array_elements.len(), 1);
}

#[test]
fn array_element_snapshot_uses_an_active_alias_then_returns_to_the_outer_name() {
    let source = r#"10 DEF SUB WORK(P)
20 P(1)=7
30 SUBEND
40 DIM A(2):A(0)=1
50 CALL WORK(A):DONE=1
60 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(20, 0), (30, 0), (50, 1)]);
    assert_eq!(
        array_element(&snapshots[0], "A(0)"),
        Some(&DebugValue::Number(1.0))
    );
    assert_eq!(
        array_element(&snapshots[1], "P(1)"),
        Some(&DebugValue::Number(7.0))
    );
    assert_eq!(snapshots[1].array_elements.len(), 1);
    assert_eq!(
        array_element(&snapshots[2], "A(1)"),
        Some(&DebugValue::Number(7.0))
    );
    assert_eq!(array_element(&snapshots[2], "P(1)"), None);
    assert_eq!(snapshots[2].array_elements.len(), 1);
}

#[test]
fn read_swap_and_mat_read_update_the_last_array_elements() {
    let source = r#"10 DATA 5,6,7
20 DIM A(2),M(1)
30 READ A(1)
40 SWAP A(1),A(2)
50 MAT READ M
60 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [30],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(30, 0), (40, 0), (50, 0), (60, 0)]);
    assert_eq!(
        array_element(&snapshots[1], "A(1)"),
        Some(&DebugValue::Number(5.0))
    );
    assert_eq!(
        array_element(&snapshots[2], "A(2)"),
        Some(&DebugValue::Number(5.0))
    );
    assert_eq!(
        array_element(&snapshots[3], "A(2)"),
        Some(&DebugValue::Number(5.0))
    );
    assert_eq!(
        array_element(&snapshots[3], "M(1)"),
        Some(&DebugValue::Number(7.0))
    );
}

#[test]
fn breakpoints_continue_across_colons_and_rearm_after_self_goto() {
    let (mut colon, outcome) = run_scripted(
        "10 A=1:B=2:C=3\n20 PRINT A;B;C\n30 END",
        debugger_with_breakpoints([10], [DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);
    let colon_snapshots = snapshots(&colon);
    assert_eq!(locations(&colon_snapshots), [(10, 0)]);
    assert_eq!(colon.take_output(), " 1  2  3\n");

    let (self_goto, outcome) = run_scripted(
        "10 A=A+1:IF A<2 THEN GOTO 10\n20 END",
        debugger_with_breakpoints([10], [DebugAction::Continue, DebugAction::Abort]),
    );
    assert_eq!(outcome, RunOutcome::End);
    let self_goto_snapshots = snapshots(&self_goto);
    assert_eq!(locations(&self_goto_snapshots), [(10, 0), (10, 0)]);
    assert!(self_goto_snapshots
        .iter()
        .all(|snapshot| snapshot.reason == DebugPauseReason::Breakpoint));
}

#[test]
fn pause_request_stops_at_the_first_statement_and_breakpoint_takes_precedence() {
    let mut requested = Debugger::scripted([DebugAction::Continue]);
    requested.request_pause();
    let (requested, outcome) = run_scripted("10 A=1\n20 END", requested);
    assert_eq!(outcome, RunOutcome::End);
    let requested_snapshots = snapshots(&requested);
    assert_eq!(locations(&requested_snapshots), [(10, 0)]);
    assert_eq!(
        requested_snapshots[0].reason,
        DebugPauseReason::PauseRequested
    );

    let mut coincident = debugger_with_breakpoints([10], [DebugAction::Continue]);
    coincident.request_pause();
    let (coincident, outcome) = run_scripted("10 A=1\n20 END", coincident);
    assert_eq!(outcome, RunOutcome::End);
    let coincident_snapshots = snapshots(&coincident);
    assert_eq!(locations(&coincident_snapshots), [(10, 0)]);
    assert_eq!(coincident_snapshots[0].reason, DebugPauseReason::Breakpoint);
}

#[test]
fn gosub_step_into_over_and_out_stop_at_the_expected_statements() {
    let source = "10 A=1:GOSUB 100:A=A+10\n20 PRINT A\n30 END\n100 A=A+1:RETURN";

    let (mut into_and_out, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let into_snapshots = snapshots(&into_and_out);
    assert_eq!(
        locations(&into_snapshots),
        [(10, 0), (10, 1), (100, 0), (10, 2)]
    );
    assert_eq!(
        into_snapshots[2]
            .stack
            .iter()
            .map(|frame| frame.kind)
            .collect::<Vec<_>>(),
        [DebugFrameKind::Program, DebugFrameKind::Gosub]
    );
    assert_eq!(into_and_out.take_output(), " 12\n");

    let (mut over, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(locations(&snapshots(&over)), [(10, 0), (10, 1), (10, 2)]);
    assert_eq!(over.take_output(), " 12\n");
}

#[test]
fn step_into_returns_to_a_pause_suspended_by_a_timer_interrupt() {
    let (interpreter, outcome) = run_scripted(
        TIMER_PAUSE_SOURCE,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Abort,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        locations(&snapshots),
        [(10, 0), (10, 1), (100, 0), (110, 0), (10, 1)]
    );
    assert_eq!(snapshots[4].reason, DebugPauseReason::Step);
}

#[test]
fn step_over_returns_to_a_pause_after_running_one_timer_interrupt() {
    let (mut interpreter, outcome) = run_scripted(
        TIMER_PAUSE_SOURCE,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::Continue,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(10, 0), (10, 1), (10, 1)]);
    assert_eq!(snapshots[2].reason, DebugPauseReason::Step);
    assert_eq!(numeric_value(&snapshots[2], "FIRED"), Some(1.0));
    assert_eq!(numeric_value(&snapshots[2], "DONE"), None);
    assert_eq!(interpreter.take_output(), " 1\n");
}

#[test]
fn step_out_of_a_timer_interrupt_returns_to_the_suspended_pause() {
    let (interpreter, outcome) = run_scripted(
        TIMER_PAUSE_SOURCE,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::Abort,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(10, 0), (10, 1), (100, 0), (10, 1)]);
    assert_eq!(snapshots[3].reason, DebugPauseReason::Step);
    assert_eq!(numeric_value(&snapshots[3], "FIRED"), Some(1.0));
}

#[test]
fn explicit_breakpoint_inside_callee_interrupts_step_over() {
    let source = "10 A=1:GOSUB 100:A=A+10\n20 PRINT A\n30 END\n100 A=A+1:RETURN";
    let (mut interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10, 100],
            [
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(locations(&snapshots), [(10, 0), (10, 1), (100, 0)]);
    assert_eq!(snapshots[2].reason, DebugPauseReason::Breakpoint);
    assert_eq!(interpreter.take_output(), " 12\n");
}

#[test]
fn sub_steps_expose_locals_arrays_and_parameter_aliases() {
    let source = r#"10 DEF SUB WORK(P)
20 LOCAL L
30 LOCAL C
40 LOCAL C(1)
50 L=P(0)+1:C=7:C(0)=8:P(0)=L
60 SUBEND
70 DIM A(1):A(0)=4
80 CALL WORK(A):PRINT A(0)
90 END"#;

    let (mut into_and_out, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [80],
            [
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let into_snapshots = snapshots(&into_and_out);
    assert_eq!(locations(&into_snapshots), [(80, 0), (20, 0), (80, 1)]);

    let inside = &into_snapshots[1];
    assert_eq!(numeric_value(inside, "L"), Some(0.0));
    assert_eq!(numeric_value(inside, "C"), Some(0.0));
    assert!(inside
        .arrays
        .iter()
        .any(|array| { array.name == "C" && array.alias_of.is_none() && array.dimensions == [1] }));
    assert!(inside
        .arrays
        .iter()
        .any(|array| array.name == "P" && array.alias_of.as_deref() == Some("A")));
    assert!(inside.stack.iter().any(|frame| {
        frame.kind == DebugFrameKind::Sub && frame.name.as_deref() == Some("WORK")
    }));

    let caller = &into_snapshots[2];
    assert_eq!(numeric_value(caller, "L"), None);
    assert_eq!(numeric_value(caller, "C"), None);
    assert!(!caller.arrays.iter().any(|array| array.name == "C"));
    assert!(!caller.arrays.iter().any(|array| array.name == "P"));
    assert_eq!(into_and_out.take_output(), " 5\n");

    let (over, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([80], [DebugAction::StepOver, DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(locations(&snapshots(&over)), [(80, 0), (80, 1)]);
}

#[test]
fn array_parameters_hide_same_named_outer_arrays_in_snapshots() {
    let source = r#"10 DEF SUB WORK(A)
20 A(0)=A(0)+1
30 SUBEND
40 DIM A(1),B(2)
50 CALL WORK(B)
60 END"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([20], [DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);

    let snapshots = snapshots(&interpreter);
    let visible_a = snapshots[0]
        .arrays
        .iter()
        .filter(|array| array.name == "A")
        .collect::<Vec<_>>();
    assert_eq!(visible_a.len(), 1);
    assert_eq!(visible_a[0].alias_of.as_deref(), Some("B"));
    assert_eq!(visible_a[0].dimensions, [2]);
    assert!(snapshots[0]
        .arrays
        .iter()
        .any(|array| array.name == "B" && array.alias_of.is_none()));
}

#[test]
fn multiline_function_is_steppable_and_single_line_function_is_atomic() {
    let multiline = r#"10 DEF FNF(X)
20 LOCAL L
30 L=X+1
40 FNF=L*2
50 FNEND
60 A=FNF(3):PRINT A
70 END"#;
    let (mut stepped, outcome) = run_scripted(
        multiline,
        debugger_with_breakpoints(
            [60],
            [
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let stepped_snapshots = snapshots(&stepped);
    assert_eq!(locations(&stepped_snapshots), [(60, 0), (20, 0), (60, 1)]);
    assert!(stepped_snapshots[1].stack.iter().any(|frame| {
        frame.kind == DebugFrameKind::Function && frame.name.as_deref() == Some("FNF")
    }));
    assert_eq!(numeric_value(&stepped_snapshots[1], "L"), Some(0.0));
    assert_eq!(stepped.take_output(), " 8\n");

    let (mut over, outcome) = run_scripted(
        multiline,
        debugger_with_breakpoints([60], [DebugAction::StepOver, DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(locations(&snapshots(&over)), [(60, 0), (60, 1)]);
    assert_eq!(over.take_output(), " 8\n");

    let single_line = "10 DEF FNG(X)=X+1\n20 A=FNG(2):PRINT A\n30 END";
    let (mut atomic, outcome) = run_scripted(
        single_line,
        debugger_with_breakpoints([20], [DebugAction::StepInto, DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);
    let atomic_snapshots = snapshots(&atomic);
    assert_eq!(locations(&atomic_snapshots), [(20, 0), (20, 1)]);
    assert!(atomic_snapshots[1]
        .stack
        .iter()
        .all(|frame| frame.kind != DebugFrameKind::Function));
    assert_eq!(atomic.take_output(), " 3\n");
}

#[test]
fn on_error_and_resume_next_preserve_then_clear_err_and_erl() {
    let source = r#"10 ON ERROR GOTO 100
20 A=1:ERROR 5:B=2
30 PRINT B
40 END
100 H=ERR:E=ERL
110 RESUME NEXT"#;
    let (mut interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        locations(&snapshots),
        [(20, 0), (20, 1), (100, 0), (100, 1), (110, 0), (20, 2),]
    );
    assert_eq!((snapshots[2].err, snapshots[2].erl), (5, 20));
    assert_eq!((snapshots[4].err, snapshots[4].erl), (5, 20));
    assert_eq!((snapshots[5].err, snapshots[5].erl), (0, 0));
    assert_eq!(interpreter.take_output(), " 2\n");
}

#[test]
fn resume_retry_and_resume_line_restore_the_expected_cursor() {
    let retry_source = r#"10 ON ERROR GOTO 100
20 A=10/Z:PRINT A
30 END
100 Z=2
110 RESUME"#;
    let (mut retry, outcome) = run_scripted(
        retry_source,
        debugger_with_breakpoints(
            [100],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let retry_snapshots = snapshots(&retry);
    assert_eq!(locations(&retry_snapshots), [(100, 0), (110, 0), (20, 0)]);
    assert_eq!((retry_snapshots[0].err, retry_snapshots[0].erl), (6, 20));
    assert_eq!((retry_snapshots[2].err, retry_snapshots[2].erl), (0, 0));
    assert_eq!(retry.take_output(), " 5\n");

    let line_source = r#"10 ON ERROR GOTO 100
20 ERROR 5
30 PRINT "TARGET"
40 END
100 RESUME 30"#;
    let (mut line, outcome) = run_scripted(
        line_source,
        debugger_with_breakpoints([100], [DebugAction::StepInto, DebugAction::Continue]),
    );
    assert_eq!(outcome, RunOutcome::End);
    let line_snapshots = snapshots(&line);
    assert_eq!(locations(&line_snapshots), [(100, 0), (30, 0)]);
    assert_eq!((line_snapshots[0].err, line_snapshots[0].erl), (5, 20));
    assert_eq!((line_snapshots[1].err, line_snapshots[1].erl), (0, 0));
    assert_eq!(line.take_output(), "TARGET\n");
}

#[test]
fn abort_unwinds_top_level_sub_and_function_without_creating_continuation() {
    let (mut top, outcome) = run_scripted(
        "10 PRINT \"BEFORE\"\n20 PRINT \"NO\"\n30 END",
        debugger_with_breakpoints([20], [DebugAction::Abort]),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(top.take_output(), "BEFORE\n");
    assert_eq!(
        top.process_immediate("CONT").unwrap_err().code,
        ErrorCode::NoStoppedProgram
    );
    top.clear_debugger();
    assert_eq!(top.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(top.take_output(), "BEFORE\nNO\n");

    let sub_source = r#"10 DEF SUB WORK
20 PRINT "INSIDE"
30 SUBEND
40 PRINT "BEFORE"
50 CALL WORK
60 PRINT "AFTER"
70 END"#;
    let (mut sub, outcome) = run_scripted(
        sub_source,
        debugger_with_breakpoints([20], [DebugAction::Abort]),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(sub.take_output(), "BEFORE\n");
    assert_eq!(
        sub.process_immediate("CONT").unwrap_err().code,
        ErrorCode::NoStoppedProgram
    );

    let function_source = r#"10 DEF FNF(X)
20 FNF=X+1
30 FNEND
40 PRINT "BEFORE"
50 A=FNF(2)
60 PRINT "AFTER"
70 END"#;
    let (mut function, outcome) = run_scripted(
        function_source,
        debugger_with_breakpoints([20], [DebugAction::Abort]),
    );
    assert_eq!(outcome, RunOutcome::End);
    assert_eq!(function.take_output(), "BEFORE\n");
    assert_eq!(
        function.process_immediate("CONT").unwrap_err().code,
        ErrorCode::NoStoppedProgram
    );
}

#[test]
fn debugger_remains_active_across_top_level_stop_and_cont() {
    let source = "10 A=1\n20 STOP\n30 PRINT \"CONTINUED\"\n40 END";
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.set_debugger(debugger_with_breakpoints(
        [10, 30],
        [DebugAction::Continue, DebugAction::Continue],
    ));

    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
    assert_eq!(locations(&snapshots(&interpreter)), [(10, 0)]);

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(locations(&snapshots(&interpreter)), [(10, 0), (30, 0)]);
    let output = interpreter.take_output();
    assert!(output.contains("Line 20. Program stopped."), "{output:?}");
    assert!(output.ends_with("CONTINUED\n"), "{output:?}");
}

#[test]
fn ctrl_c_with_an_inactive_debugger_preserves_top_level_cont() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text("10 PRINT 1\n20 PRINT 2\n30 END")
        .unwrap();
    interpreter.set_debugger(Debugger::scripted([]));
    interpreter.request_interrupt_for_test();

    let error = interpreter.run_loaded().unwrap_err();
    assert_eq!(error.code, ErrorCode::KeyboardInterrupt);
    assert_eq!(error.line, Some(10));
    assert_eq!(interpreter.take_output(), "");

    interpreter.process_immediate("CONT").unwrap();
    assert_eq!(interpreter.take_output(), " 1\n 2\n");
    assert!(interpreter.debugger().unwrap().snapshots().is_empty());
}

#[test]
fn chain_and_merge_snapshots_use_the_current_program_source() {
    let dir = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();

    let chain_first = dir.path().join("chain-first.bas");
    let chain_second = dir.path().join("chain-second.bas");
    std::fs::write(&chain_first, "10 CHAIN \"chain-second.bas\"\n").unwrap();
    std::fs::write(&chain_second, "900 PRINT \"CHAINED\"\n910 END\n").unwrap();

    let mut chained = Interpreter::new();
    chained.load_file(&chain_first).unwrap();
    chained.set_debugger(debugger_with_breakpoints([900], [DebugAction::Continue]));
    assert_eq!(chained.run_loaded().unwrap(), RunOutcome::End);
    let chain_snapshots = snapshots(&chained);
    assert_eq!(locations(&chain_snapshots), [(900, 0)]);
    assert!(chain_snapshots[0]
        .source_lines
        .iter()
        .any(|line| line.contains("CHAINED")));
    assert!(!chain_snapshots[0]
        .source_lines
        .iter()
        .any(|line| line.contains("CHAIN \"chain-second.bas\"")));
    assert_eq!(chained.take_output(), "CHAINED\n");

    let merge_first = dir.path().join("merge-first.bas");
    let merge_extra = dir.path().join("merge-extra.bas");
    std::fs::write(
        &merge_first,
        "10 MERGE \"merge-extra.bas\"\n20 GOTO 900\n30 END\n",
    )
    .unwrap();
    std::fs::write(&merge_extra, "900 PRINT \"MERGED\"\n910 END\n").unwrap();

    let mut merged = Interpreter::new();
    merged.load_file(&merge_first).unwrap();
    merged.set_debugger(debugger_with_breakpoints([900], [DebugAction::Continue]));
    assert_eq!(merged.run_loaded().unwrap(), RunOutcome::End);
    let merge_snapshots = snapshots(&merged);
    assert_eq!(locations(&merge_snapshots), [(900, 0)]);
    assert!(merge_snapshots[0]
        .source_lines
        .iter()
        .any(|line| line.contains("MERGE \"merge-extra.bas\"")));
    assert!(merge_snapshots[0]
        .source_lines
        .iter()
        .any(|line| line.contains("MERGED")));
    assert_eq!(merged.take_output(), "MERGED\n");
}

#[derive(Debug, PartialEq, Eq)]
enum RunSignature {
    Ok(RunOutcome),
    Err(ErrorCode, Option<i32>, String),
}

fn semantic_run(source: &str, debugger_attached: bool) -> (RunSignature, String, usize) {
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    if debugger_attached {
        interpreter.set_debugger(Debugger::scripted([]));
    }
    let signature = match interpreter.run_loaded() {
        Ok(outcome) => RunSignature::Ok(outcome),
        Err(error) => RunSignature::Err(error.code, error.line, error.display_for_basic()),
    };
    let snapshot_count = interpreter
        .debugger()
        .map_or(0, |debugger| debugger.snapshots().len());
    (signature, interpreter.take_output(), snapshot_count)
}

#[test]
fn attached_but_inactive_debugger_is_semantically_transparent() {
    let programs = [
        "10 A=1:B=2\n20 GOSUB 100\n30 PRINT A;B\n40 END\n100 A=A+1:RETURN",
        r#"10 DEF FNF(X)=X*2
20 DEF SUB WORK(P)
30 LOCAL L
40 L=FNF(P(0))
50 P(0)=L
60 SUBEND
70 DIM A(1):A(0)=3
80 CALL WORK(A)
90 PRINT A(0)
100 END"#,
        "10 ON ERROR RESUME NEXT\n20 A=1/0:B=7\n30 PRINT B\n40 END",
        "10 IF 1 THEN A=1:A=2 ELSE A=9\n20 PRINT A\n30 END",
        "10 IF 0 THEN A=1 ELSE A=2:A=3\n20 PRINT A\n30 END",
        "10 X=1:Y=0\n20 IF X THEN IF Y THEN A=1 ELSE A=2 ELSE A=3\n30 PRINT A\n40 END",
        "10 IF 1 THEN GOSUB 100:A=3 ELSE A=9\n20 PRINT A\n30 END\n100 A=2:RETURN",
        "10 ON ERROR GOTO 100\n20 IF 1 THEN A=1/0:A=2 ELSE A=9\n30 PRINT A\n40 END\n100 RESUME NEXT",
        "10 A=1/0",
    ];

    for source in programs {
        let plain = semantic_run(source, false);
        let debugged = semantic_run(source, true);
        assert_eq!(debugged.0, plain.0, "outcome differs for {source:?}");
        assert_eq!(debugged.1, plain.1, "output differs for {source:?}");
        assert_eq!(debugged.2, 0, "inactive debugger paused for {source:?}");
    }
}

#[test]
fn immediate_renum_and_deletion_keep_breakpoints_attached_to_program_lines() {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text("10 A=1\n20 A=2\n30 A=3\n40 END")
        .unwrap();
    interpreter.set_debugger(debugger_with_breakpoints(
        [20, 40],
        [DebugAction::Continue, DebugAction::Continue],
    ));

    interpreter.process_immediate("RENUM 100,10").unwrap();
    let breakpoints = interpreter
        .debugger()
        .unwrap()
        .breakpoints()
        .collect::<BTreeSet<_>>();
    assert_eq!(breakpoints, BTreeSet::from([110, 130]));

    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    assert_eq!(locations(&snapshots(&interpreter)), [(110, 0), (130, 0)]);

    interpreter.process_immediate("DELETE 110").unwrap();
    let breakpoints = interpreter
        .debugger()
        .unwrap()
        .breakpoints()
        .collect::<BTreeSet<_>>();
    assert_eq!(breakpoints, BTreeSet::from([130]));

    interpreter.process_immediate("130").unwrap();
    assert_eq!(interpreter.debugger().unwrap().breakpoints().count(), 0);
    interpreter.process_immediate("NEW").unwrap();
    assert_eq!(interpreter.debugger().unwrap().breakpoints().count(), 0);
}

#[test]
fn inline_if_steps_each_selected_then_statement_and_skips_else() {
    let source = "10 A=0\n20 IF 1 THEN A=1:A=2 ELSE A=9:A=10\n30 END";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([20], step_into_then_continue(3)),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (20, "IF 1 THEN A=1:A=2 ELSE A=9:A=10"),
            (20, "A=1"),
            (20, "A=2"),
            (30, "END"),
        ]
    );
    assert_eq!(snapshots[0].reason, DebugPauseReason::Breakpoint);
    assert!(snapshots[1..]
        .iter()
        .all(|snapshot| snapshot.reason == DebugPauseReason::Step));
    assert_eq!(numeric_value(&snapshots[1], "A"), Some(0.0));
    assert_eq!(numeric_value(&snapshots[2], "A"), Some(1.0));
    assert_eq!(numeric_value(&snapshots[3], "A"), Some(2.0));
    assert_eq!(source_fragment(&snapshots[1]), "A=1");
    assert_eq!(source_fragment(&snapshots[2]), "A=2");
}

#[test]
fn inline_if_steps_each_selected_else_statement_and_skips_then() {
    let source = "10 A=0\n20 IF 0 THEN A=1:A=2 ELSE A=9:A=10\n30 END";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([20], step_into_then_continue(3)),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (20, "IF 0 THEN A=1:A=2 ELSE A=9:A=10"),
            (20, "A=9"),
            (20, "A=10"),
            (30, "END"),
        ]
    );
    assert_eq!(numeric_value(&snapshots[1], "A"), Some(0.0));
    assert_eq!(numeric_value(&snapshots[2], "A"), Some(9.0));
    assert_eq!(numeric_value(&snapshots[3], "A"), Some(10.0));
    assert_eq!(source_fragment(&snapshots[1]), "A=9");
    assert_eq!(source_fragment(&snapshots[2]), "A=10");
}

#[test]
fn nested_inline_if_steps_only_the_selected_path_and_preserves_dangling_else() {
    let outer = "IF X THEN IF Y THEN A=1 ELSE B=2 ELSE C=3";
    let inner = "IF Y THEN A=1 ELSE B=2";
    let cases = [
        (
            1,
            1,
            vec![(20, outer), (20, inner), (20, "A=1"), (30, "END")],
        ),
        (
            1,
            0,
            vec![(20, outer), (20, inner), (20, "B=2"), (30, "END")],
        ),
        (0, 0, vec![(20, outer), (20, "C=3"), (30, "END")]),
    ];

    for (x, y, expected) in cases {
        let source =
            format!("10 X={x}:Y={y}\n20 IF X THEN IF Y THEN A=1 ELSE B=2 ELSE C=3\n30 END");
        let (interpreter, outcome) = run_scripted(
            &source,
            debugger_with_breakpoints([20], step_into_then_continue(expected.len() - 1)),
        );

        assert_eq!(outcome, RunOutcome::End, "X={x}, Y={y}");
        let snapshots = snapshots(&interpreter);
        assert_eq!(command_trace(&snapshots), expected, "X={x}, Y={y}");
        assert_eq!(snapshots[0].reason, DebugPauseReason::Breakpoint);
        assert!(snapshots[1..]
            .iter()
            .all(|snapshot| snapshot.reason == DebugPauseReason::Step));
    }
}

#[test]
fn inline_gosub_step_into_and_out_returns_to_the_next_branch_statement() {
    let source = "10 IF 1 THEN A=1:GOSUB 100:A=3 ELSE A=9\n20 END\n100 A=2:RETURN";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (10, "IF 1 THEN A=1:GOSUB 100:A=3 ELSE A=9"),
            (10, "A=1"),
            (10, "GOSUB 100"),
            (100, "A=2"),
            (10, "A=3"),
            (20, "END"),
        ]
    );
    assert_eq!(
        snapshots[3]
            .stack
            .iter()
            .map(|frame| frame.kind)
            .collect::<Vec<_>>(),
        [DebugFrameKind::Program, DebugFrameKind::Gosub]
    );
    assert_eq!(numeric_value(&snapshots[4], "A"), Some(2.0));
    assert_eq!(numeric_value(&snapshots[5], "A"), Some(3.0));
}

#[test]
fn inline_gosub_step_over_skips_the_callee() {
    let source = "10 IF 1 THEN A=1:GOSUB 100:A=3 ELSE A=9\n20 END\n100 A=2:RETURN";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (10, "IF 1 THEN A=1:GOSUB 100:A=3 ELSE A=9"),
            (10, "A=1"),
            (10, "GOSUB 100"),
            (10, "A=3"),
            (20, "END"),
        ]
    );
    assert_eq!(numeric_value(&snapshots[3], "A"), Some(2.0));
}

#[test]
fn callee_breakpoint_interrupts_step_over_from_an_inline_gosub() {
    let source = "10 IF 1 THEN A=1:GOSUB 100:A=3 ELSE A=9\n20 END\n100 A=2:RETURN";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10, 100],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::StepOut,
                DebugAction::Continue,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (10, "IF 1 THEN A=1:GOSUB 100:A=3 ELSE A=9"),
            (10, "A=1"),
            (10, "GOSUB 100"),
            (100, "A=2"),
            (10, "A=3"),
        ]
    );
    assert_eq!(snapshots[3].reason, DebugPauseReason::Breakpoint);
    assert_eq!(snapshots[4].reason, DebugPauseReason::Step);
    assert_eq!(numeric_value(&snapshots[4], "A"), Some(2.0));
}

#[test]
fn inline_on_gosub_obeys_step_into_out_and_over() {
    let source = "10 N=2\n20 IF 1 THEN ON N GOSUB 100,200:A=3 ELSE A=9\n30 END\n100 A=1:RETURN\n200 A=2:RETURN";

    let (into, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepOut,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let into_snapshots = snapshots(&into);
    assert_eq!(
        command_trace(&into_snapshots),
        [
            (20, "IF 1 THEN ON N GOSUB 100,200:A=3 ELSE A=9"),
            (20, "ON N GOSUB 100,200"),
            (200, "A=2"),
            (20, "A=3"),
            (30, "END"),
        ]
    );

    let (over, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );
    assert_eq!(outcome, RunOutcome::End);
    let over_snapshots = snapshots(&over);
    assert_eq!(
        command_trace(&over_snapshots),
        [
            (20, "IF 1 THEN ON N GOSUB 100,200:A=3 ELSE A=9"),
            (20, "ON N GOSUB 100,200"),
            (20, "A=3"),
            (30, "END"),
        ]
    );
    assert_eq!(numeric_value(&over_snapshots[2], "A"), Some(2.0));
}

#[test]
fn continue_from_an_inline_if_line_breakpoint_does_not_pause_at_its_leaves() {
    let source = "10 IF 1 THEN A=1:A=2 ELSE A=9\n20 PRINT A\n30 END";
    let (mut interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([10], [DebugAction::Continue]),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [(10, "IF 1 THEN A=1:A=2 ELSE A=9")]
    );
    assert_eq!(snapshots[0].reason, DebugPauseReason::Breakpoint);
    assert_eq!(interpreter.take_output(), " 2\n");
}

#[test]
fn abort_from_an_inline_if_leaf_stops_before_executing_that_leaf() {
    let source = "10 IF 1 THEN A=1:B=2 ELSE A=9\n20 END";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([10], [DebugAction::StepInto, DebugAction::Abort]),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [(10, "IF 1 THEN A=1:B=2 ELSE A=9"), (10, "A=1"),]
    );
    assert_eq!(numeric_value(&snapshots[1], "A"), None);
    assert_eq!(numeric_value(&snapshots[1], "B"), None);
}

#[test]
fn inline_goto_restarts_at_the_guard_and_rearms_its_line_breakpoint() {
    let source = "10 IF A=0 THEN A=1:GOTO 10 ELSE A=2\n20 END";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [10],
            [
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (10, "IF A=0 THEN A=1:GOTO 10 ELSE A=2"),
            (10, "A=1"),
            (10, "GOTO 10"),
            (10, "IF A=0 THEN A=1:GOTO 10 ELSE A=2"),
            (10, "A=2"),
            (20, "END"),
        ]
    );
    assert_eq!(snapshots[0].reason, DebugPauseReason::Breakpoint);
    assert_eq!(snapshots[3].reason, DebugPauseReason::Breakpoint);
    assert_eq!(numeric_value(&snapshots[3], "A"), Some(1.0));
    assert_eq!(numeric_value(&snapshots[5], "A"), Some(2.0));
}

#[test]
fn resume_next_returns_to_the_exact_next_statement_inside_then() {
    let source = r#"10 ON ERROR GOTO 100
20 IF 1 THEN A=1:ERROR 5:B=2 ELSE B=9
30 END
100 H=ERR
110 RESUME NEXT"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([20], step_into_then_continue(6)),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (20, "IF 1 THEN A=1:ERROR 5:B=2 ELSE B=9"),
            (20, "A=1"),
            (20, "ERROR 5"),
            (100, "H=ERR"),
            (110, "RESUME NEXT"),
            (20, "B=2"),
            (30, "END"),
        ]
    );
    assert_eq!((snapshots[3].err, snapshots[3].erl), (5, 20));
    assert_eq!((snapshots[4].err, snapshots[4].erl), (5, 20));
    assert_eq!((snapshots[5].err, snapshots[5].erl), (0, 0));
    assert_eq!(numeric_value(&snapshots[5], "A"), Some(1.0));
    assert_eq!(numeric_value(&snapshots[5], "B"), None);
    assert_eq!(numeric_value(&snapshots[6], "B"), Some(2.0));
}

#[test]
fn resume_retries_the_exact_failing_statement_inside_else() {
    let source = r#"10 ON ERROR GOTO 100
20 D=0
30 IF 0 THEN A=9 ELSE A=10/D:B=2
40 END
100 D=2
110 RESUME"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([30], step_into_then_continue(6)),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (30, "IF 0 THEN A=9 ELSE A=10/D:B=2"),
            (30, "A=10/D"),
            (100, "D=2"),
            (110, "RESUME"),
            (30, "A=10/D"),
            (30, "B=2"),
            (40, "END"),
        ]
    );
    assert_eq!((snapshots[2].err, snapshots[2].erl), (6, 30));
    assert_eq!((snapshots[3].err, snapshots[3].erl), (6, 30));
    assert_eq!((snapshots[4].err, snapshots[4].erl), (0, 0));
    assert_eq!(numeric_value(&snapshots[5], "A"), Some(5.0));
    assert_eq!(numeric_value(&snapshots[6], "B"), Some(2.0));
}

#[test]
fn resume_next_after_an_inline_if_condition_error_enters_then() {
    let source = r#"10 ON ERROR GOTO 100
20 IF 1/0 THEN A=1:A=2 ELSE A=9
30 END
100 RESUME NEXT"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([20], step_into_then_continue(4)),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (20, "IF 1/0 THEN A=1:A=2 ELSE A=9"),
            (100, "RESUME NEXT"),
            (20, "A=1"),
            (20, "A=2"),
            (30, "END"),
        ]
    );
    assert_eq!((snapshots[1].err, snapshots[1].erl), (6, 20));
    assert_eq!((snapshots[2].err, snapshots[2].erl), (0, 0));
    assert_eq!(numeric_value(&snapshots[4], "A"), Some(2.0));
}

#[test]
fn resume_next_after_the_last_then_statement_skips_else_and_hidden_jumps() {
    let source = r#"10 ON ERROR GOTO 100
20 IF 1 THEN ERROR 5 ELSE A=9
30 A=3
40 END
100 RESUME NEXT"#;
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints([20], step_into_then_continue(3)),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (20, "IF 1 THEN ERROR 5 ELSE A=9"),
            (20, "ERROR 5"),
            (100, "RESUME NEXT"),
            (30, "A=3"),
        ]
    );
    assert!(command_trace(&snapshots)
        .iter()
        .all(|(_, command)| *command != "A=9"));
}

#[test]
fn numeric_inline_targets_are_visible_at_their_source_literals() {
    let cases = [
        (1, "100", 100, "A=1", " 1\n"),
        (0, "200", 200, "A=2", " 2\n"),
    ];

    for (condition, literal, target_line, target_command, output) in cases {
        let source = format!(
            "10 X={condition}\n20 IF X THEN 100 ELSE 200\n30 END\n100 A=1:GOTO 300\n200 A=2\n300 PRINT A\n310 END"
        );
        let (mut interpreter, outcome) = run_scripted(
            &source,
            debugger_with_breakpoints(
                [20],
                [
                    DebugAction::StepInto,
                    DebugAction::StepInto,
                    DebugAction::Continue,
                ],
            ),
        );

        assert_eq!(outcome, RunOutcome::End, "condition {condition}");
        let snapshots = snapshots(&interpreter);
        assert_eq!(
            command_trace(&snapshots),
            [
                (20, "IF X THEN 100 ELSE 200"),
                (20, literal),
                (target_line, target_command),
            ],
            "condition {condition}"
        );
        assert_eq!(source_fragment(&snapshots[1]), literal);
        assert_eq!(interpreter.take_output(), output);
    }
}

#[test]
fn repeated_utf8_inline_commands_have_distinct_exact_source_spans() {
    const LEAF: &str = "PRINT \"é\"";
    for (condition, selected) in [(1, [0usize, 1usize]), (0, [2usize, 3usize])] {
        let listing =
            format!("20 IF {condition} THEN PRINT \"é\":PRINT \"é\" ELSE PRINT \"é\":PRINT \"é\"");
        let source = format!("10 X=0\n{listing}\n30 END");
        let (interpreter, outcome) = run_scripted(
            &source,
            debugger_with_breakpoints([20], step_into_then_continue(3)),
        );

        assert_eq!(outcome, RunOutcome::End, "condition {condition}");
        let snapshots = snapshots(&interpreter);
        assert_eq!(snapshots.len(), 4);
        let guard_start = listing.find("IF ").unwrap();
        assert_eq!(
            snapshots[0].location.source_span,
            Some(guard_start..listing.len())
        );
        assert_eq!(source_fragment(&snapshots[0]), &listing[guard_start..]);

        let starts = listing
            .match_indices(LEAF)
            .map(|(start, _)| start)
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 4);
        for (snapshot, occurrence) in snapshots[1..3].iter().zip(selected) {
            let start = starts[occurrence];
            assert_eq!(snapshot.location.source, listing);
            assert_eq!(
                snapshot.location.source_span,
                Some(start..start + LEAF.len())
            );
            assert_eq!(snapshot.location.command, LEAF);
            assert_eq!(source_fragment(snapshot), LEAF);
        }
        assert_ne!(
            snapshots[1].location.source_span,
            snapshots[2].location.source_span
        );
    }
}

#[test]
fn inline_if_source_spans_survive_merge_inserting_earlier_lines() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("insert.bas"), "5 REM INSERTED\n").unwrap();
    let source = r#"10 IF DONE=0 THEN DONE=1:MERGE "insert.bas":PRINT "AFTER"
20 END"#;
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(source).unwrap();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    interpreter.set_debugger(debugger_with_breakpoints(
        [10],
        [
            DebugAction::StepInto,
            DebugAction::StepInto,
            DebugAction::StepInto,
            DebugAction::StepInto,
            DebugAction::StepInto,
            DebugAction::Continue,
        ],
    ));

    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (
                10,
                "IF DONE=0 THEN DONE=1:MERGE \"insert.bas\":PRINT \"AFTER\""
            ),
            (10, "DONE=1"),
            (10, "MERGE \"insert.bas\""),
            (10, "PRINT \"AFTER\""),
            (
                10,
                "IF DONE=0 THEN DONE=1:MERGE \"insert.bas\":PRINT \"AFTER\"",
            ),
            (20, "END"),
        ]
    );
    assert_eq!(snapshots[4].reason, DebugPauseReason::Breakpoint);
    assert_eq!(source_fragment(&snapshots[3]), "PRINT \"AFTER\"");
    assert!(snapshots[3]
        .source_lines
        .iter()
        .any(|line| line == "5 REM INSERTED"));
    assert_eq!(interpreter.take_output(), "AFTER\n");
}

#[test]
fn inline_if_snapshot_keeps_the_executed_source_when_merge_replaces_its_line() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("replace.bas"),
        "10 REM REPLACED LINE THAT IS DELIBERATELY LONGER THAN THE ORIGINAL EXECUTION PLAN\n",
    )
    .unwrap();
    let old_line = r#"10 IF 1 THEN MERGE "replace.bas":AFTER=1 ELSE AFTER=9"#;
    let source = format!("{old_line}\n20 END");
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(&source).unwrap();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    interpreter.set_debugger(debugger_with_breakpoints(
        [10],
        [
            DebugAction::StepInto,
            DebugAction::StepInto,
            DebugAction::Abort,
        ],
    ));

    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (10, "IF 1 THEN MERGE \"replace.bas\":AFTER=1 ELSE AFTER=9"),
            (10, "MERGE \"replace.bas\""),
            (10, "AFTER=1"),
        ]
    );

    let after = &snapshots[2];
    let after_start = old_line.find("AFTER=1").unwrap();
    assert_eq!(after.location.source, old_line);
    assert_eq!(
        after.location.source_span,
        Some(after_start..after_start + "AFTER=1".len())
    );
    assert_eq!(source_fragment(after), "AFTER=1");
    assert!(after.source_lines.iter().any(|line| line == old_line));
}

#[test]
fn inline_if_snapshot_keeps_the_executed_source_when_merge_deletes_its_line() {
    let temp = tempfile::tempdir().unwrap();
    // A BASIC source line containing only its number deletes that line, and
    // MERGE applies the same Program::add_source_line rule as direct editing.
    std::fs::write(temp.path().join("delete.bas"), "10\n").unwrap();
    let old_line = r#"10 IF 1 THEN MERGE "delete.bas":AFTER=1 ELSE AFTER=9"#;
    let source = format!("{old_line}\n20 END");
    let mut interpreter = Interpreter::new();
    interpreter.program.load_text(&source).unwrap();
    interpreter.root_dir = temp.path().to_path_buf();
    interpreter.current_dir = temp.path().to_path_buf();
    interpreter.set_debugger(debugger_with_breakpoints(
        [10],
        [
            DebugAction::StepInto,
            DebugAction::StepInto,
            DebugAction::Abort,
        ],
    ));

    assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (10, "IF 1 THEN MERGE \"delete.bas\":AFTER=1 ELSE AFTER=9"),
            (10, "MERGE \"delete.bas\""),
            (10, "AFTER=1"),
        ]
    );

    let after = &snapshots[2];
    let after_start = old_line.find("AFTER=1").unwrap();
    assert_eq!(after.location.source, old_line);
    assert_eq!(
        after.location.source_span,
        Some(after_start..after_start + "AFTER=1".len())
    );
    assert_eq!(source_fragment(after), "AFTER=1");
    assert!(after.source_lines.iter().any(|line| line == old_line));
}

#[test]
fn inline_pause_step_over_returns_after_a_timer_interrupt() {
    let source = "10 AFTER 5,1 GOSUB 100\n20 IF 1 THEN PAUSE 200:DONE=1 ELSE DONE=9\n30 END\n100 FIRED=1\n110 RETURN";
    let (interpreter, outcome) = run_scripted(
        source,
        debugger_with_breakpoints(
            [20],
            [
                DebugAction::StepInto,
                DebugAction::StepOver,
                DebugAction::StepInto,
                DebugAction::StepInto,
                DebugAction::Continue,
            ],
        ),
    );

    assert_eq!(outcome, RunOutcome::End);
    let snapshots = snapshots(&interpreter);
    assert_eq!(
        command_trace(&snapshots),
        [
            (20, "IF 1 THEN PAUSE 200:DONE=1 ELSE DONE=9"),
            (20, "PAUSE 200"),
            (20, "PAUSE 200"),
            (20, "DONE=1"),
            (30, "END"),
        ]
    );
    assert_eq!(numeric_value(&snapshots[2], "FIRED"), Some(1.0));
    assert_eq!(numeric_value(&snapshots[2], "DONE"), None);
    assert_eq!(numeric_value(&snapshots[4], "DONE"), Some(1.0));
}
