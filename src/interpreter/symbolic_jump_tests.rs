use super::*;

#[derive(Debug, Clone, Copy)]
enum BindingState {
    Cold,
    Current,
    Stale,
}

const BINDINGS: [BindingState; 3] = [
    BindingState::Cold,
    BindingState::Current,
    BindingState::Stale,
];

fn symbolic(source: &str, line: i32) -> Rc<SymbolicCommand> {
    Rc::new(SymbolicCommand {
        source: Rc::from(source),
        line,
        inline_sub_call: false,
        bound: RefCell::new(None),
    })
}

fn compare_result(actual: BasicResult<()>, expected: BasicResult<()>) {
    match (actual, expected) {
        (Ok(()), Ok(())) => {}
        (Err(actual), Err(expected)) => {
            assert_eq!(actual.code, expected.code);
            assert_eq!(actual.line, expected.line);
            assert_eq!(actual.detail, expected.detail);
        }
        (actual, expected) => panic!("symbolic result {actual:?}, numeric result {expected:?}"),
    }
}

fn compare_flow(actual: &Interpreter, expected: &Interpreter) {
    assert_eq!(actual.gosub_stack, expected.gosub_stack);
    assert_eq!(actual.if_stack, expected.if_stack);
    assert_eq!(
        actual.repeat_current_command,
        expected.repeat_current_command
    );
    assert_eq!(actual.root_label_handler, expected.root_label_handler);
}

fn root_fixture(state: BindingState, gosub: bool) -> (Interpreter, Rc<SymbolicCommand>, i32) {
    let mut interpreter = Interpreter::new();
    interpreter
        .program
        .load_text("10 REM caller\n20 END\n100 Destination:\n110 RETURN")
        .unwrap();
    interpreter.rebuild_command_cache();
    interpreter.ensure_routine_catalog().unwrap();
    let command = symbolic(
        if gosub {
            "GOSUB Destination"
        } else {
            "GOTO Destination"
        },
        10,
    );
    let mut destination = 100;
    if !matches!(state, BindingState::Cold) {
        interpreter.bind_symbolic_command(&command).unwrap();
    }
    if matches!(state, BindingState::Stale) {
        interpreter
            .program
            .add_source_line("100 REM old declaration")
            .unwrap();
        interpreter
            .program
            .add_source_line("200 Destination:")
            .unwrap();
        interpreter.rebuild_command_cache();
        destination = 200;
        assert_ne!(
            command.bound.borrow().as_ref().unwrap().0,
            interpreter.label_generation
        );
    }
    interpreter.current_line = Some(10);
    (interpreter, command, destination)
}

#[test]
fn symbolic_simple_jumps_match_numeric_cold_current_stale_and_return() {
    for state in BINDINGS {
        for gosub in [false, true] {
            let (mut actual, command, destination) = root_fixture(state, gosub);
            let (mut expected, _, _) = root_fixture(state, gosub);
            expected.ensure_routine_catalog().unwrap();
            let numeric = if gosub {
                CachedCommand::GosubConst {
                    line: destination,
                    target: expected.cursor_for_line(destination),
                }
            } else {
                CachedCommand::GotoConst {
                    line: destination,
                    target: expected.cursor_for_line(destination),
                }
            };
            let original = actual.cursor_for_line(10).unwrap();
            let mut actual_cursor = original;
            let mut expected_cursor = original;
            compare_result(
                actual.execute_cached_command(
                    &CachedCommand::Symbolic(command.clone()),
                    &mut actual_cursor,
                    &[],
                ),
                expected.execute_cached_command(&numeric, &mut expected_cursor, &[]),
            );
            assert_eq!(actual_cursor, expected_cursor, "{state:?}, gosub={gosub}");
            assert_eq!(actual_cursor, actual.cursor_for_line(destination).unwrap());
            compare_flow(&actual, &expected);
            assert_eq!(
                command.bound.borrow().as_ref().unwrap().0,
                actual.label_generation
            );
            if gosub {
                compare_result(
                    actual.execute_return(&mut actual_cursor),
                    expected.execute_return(&mut expected_cursor),
                );
                assert_eq!(
                    actual_cursor,
                    Cursor {
                        cmd_idx: original.cmd_idx + 1,
                        ..original
                    }
                );
                assert_eq!(actual_cursor, expected_cursor);
                compare_flow(&actual, &expected);
            }
        }
    }
}

#[test]
fn symbolic_simple_jumps_preserve_scope_validation_and_failed_gosub_frames() {
    for state in BINDINGS {
        for gosub in [false, true] {
            for wrong_scope in [false, true] {
                let prepare = || {
                    let mut interpreter = Interpreter::new();
                    interpreter.program.load_text("10 END\n100 DEF SUB Work\n110 Destination:\n120 RETURN\n130 SUBEND\n200 DEF SUB Other\n210 Destination:\n220 RETURN\n230 SUBEND").unwrap();
                    interpreter.rebuild_command_cache();
                    interpreter.ensure_routine_catalog().unwrap();
                    let command = symbolic(
                        if gosub {
                            "GOSUB Destination"
                        } else {
                            "GOTO Destination"
                        },
                        if wrong_scope { 220 } else { 120 },
                    );
                    if !matches!(state, BindingState::Cold) {
                        interpreter.bind_symbolic_command(&command).unwrap();
                    }
                    if matches!(state, BindingState::Stale) {
                        interpreter
                            .program
                            .add_source_line("110 REM old work label")
                            .unwrap();
                        interpreter
                            .program
                            .add_source_line("115 Destination:")
                            .unwrap();
                        interpreter
                            .program
                            .add_source_line("210 REM old other label")
                            .unwrap();
                        interpreter
                            .program
                            .add_source_line("215 Destination:")
                            .unwrap();
                        interpreter.rebuild_command_cache();
                    }
                    interpreter.active_subs.push(ActiveSubFrame {
                        name: Rc::from("WORK"),
                    });
                    interpreter.current_line = Some(120);
                    (interpreter, command)
                };
                let (mut actual, command) = prepare();
                let (mut expected, _) = prepare();
                expected.ensure_routine_catalog().unwrap();
                let destination = match (wrong_scope, matches!(state, BindingState::Stale)) {
                    (false, false) => 110,
                    (false, true) => 115,
                    (true, false) => 210,
                    (true, true) => 215,
                };
                let numeric = if gosub {
                    CachedCommand::GosubConst {
                        line: destination,
                        target: expected.cursor_for_line(destination),
                    }
                } else {
                    CachedCommand::GotoConst {
                        line: destination,
                        target: expected.cursor_for_line(destination),
                    }
                };
                let original = actual.cursor_for_line(120).unwrap();
                let mut actual_cursor = original;
                let mut expected_cursor = original;
                let actual_result = actual.execute_cached_command(
                    &CachedCommand::Symbolic(command),
                    &mut actual_cursor,
                    &[],
                );
                let expected_result =
                    expected.execute_cached_command(&numeric, &mut expected_cursor, &[]);
                if wrong_scope {
                    assert_eq!(
                        actual_result.as_ref().unwrap_err().code,
                        ErrorCode::InvalidTargetLine
                    );
                    assert_eq!(actual_cursor, original);
                    assert_eq!(actual.gosub_stack.len(), usize::from(gosub));
                } else {
                    assert!(actual_result.is_ok());
                    assert_eq!(actual_cursor, actual.cursor_for_line(destination).unwrap());
                }
                compare_result(actual_result, expected_result);
                assert_eq!(actual_cursor, expected_cursor);
                compare_flow(&actual, &expected);
            }
        }
    }
}

#[test]
fn symbolic_simple_self_jumps_preserve_repeat_flag_in_every_binding_state() {
    for state in BINDINGS {
        let (mut actual, command, destination) = root_fixture(state, false);
        let (mut expected, _, _) = root_fixture(state, false);
        expected.ensure_routine_catalog().unwrap();
        actual.current_line = Some(destination);
        expected.current_line = Some(destination);
        let mut actual_cursor = actual.cursor_for_line(destination).unwrap();
        let mut expected_cursor = actual_cursor;
        let numeric = CachedCommand::GotoConst {
            line: destination,
            target: Some(expected_cursor),
        };
        compare_result(
            actual.execute_cached_command(
                &CachedCommand::Symbolic(command.clone()),
                &mut actual_cursor,
                &[],
            ),
            expected.execute_cached_command(&numeric, &mut expected_cursor, &[]),
        );
        assert_eq!(actual_cursor, expected_cursor);
        assert!(actual.repeat_current_command);
        compare_flow(&actual, &expected);
        // No RefCell guard may survive a dispatch into the jump helper.
        assert!(command.bound.try_borrow_mut().is_ok());
    }
}
