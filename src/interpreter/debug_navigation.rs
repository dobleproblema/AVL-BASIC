//! Cold debugger navigation. No expression evaluation or normal-run bookkeeping.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct NavigationContext {
    routine: Option<(DebugFrameKind, String, Cursor)>,
    fors: Vec<Cursor>,
    whiles: Vec<Cursor>,
    branches: Vec<Cursor>,
    flow_region: Option<Cursor>,
    ambiguous: bool,
}

struct NavigationEntry {
    cursor: Cursor,
    target: DebugStatementTarget,
}

#[derive(Debug)]
struct DebugControlIdentity {
    run_depth: usize,
    gosubs: Vec<GosubFrame>,
    subs: Vec<Rc<str>>,
    functions: Vec<Rc<str>>,
    timer_markers: Vec<usize>,
    mouse: bool,
    handling_error: bool,
    error: Option<(i32, i32, Cursor, Cursor)>,
}

impl DebugControlIdentity {
    fn error(interpreter: &Interpreter) -> Option<(i32, i32, Cursor, Cursor)> {
        interpreter
            .handling_error
            .then(|| interpreter.last_error.as_ref())
            .flatten()
            .map(|error| (error.number, error.line, error.retry, error.next))
    }

    fn matches(&self, interpreter: &Interpreter) -> bool {
        self.run_depth == interpreter.run_depth
            && self.gosubs == interpreter.gosub_stack
            && self.subs.iter().map(Rc::as_ref).eq(interpreter
                .active_subs
                .iter()
                .map(|frame| frame.name.as_ref()))
            && self.functions.iter().map(Rc::as_ref).eq(interpreter
                .active_functions
                .iter()
                .map(|frame| frame.name.as_ref()))
            && self.timer_markers == interpreter.timer_isr_markers
            && self.mouse == interpreter.handling_mouse_event
            && self.handling_error == interpreter.handling_error
            && self.error == Self::error(interpreter)
    }

    fn is_parent_of(&self, interpreter: &Interpreter) -> bool {
        let error_entry = !self.handling_error && interpreter.handling_error;
        let deeper = interpreter.run_depth > self.run_depth
            || interpreter.gosub_stack.len() > self.gosubs.len()
            || error_entry;
        deeper
            && interpreter.run_depth >= self.run_depth
            && interpreter.gosub_stack.starts_with(&self.gosubs)
            && interpreter.active_subs.len() >= self.subs.len()
            && self
                .subs
                .iter()
                .zip(&interpreter.active_subs)
                .all(|(name, frame)| *name == frame.name)
            && interpreter.active_functions.len() >= self.functions.len()
            && self
                .functions
                .iter()
                .zip(&interpreter.active_functions)
                .all(|(name, frame)| *name == frame.name)
            && interpreter
                .timer_isr_markers
                .starts_with(&self.timer_markers)
            && (!self.mouse || interpreter.handling_mouse_event)
            && (error_entry
                || (self.handling_error == interpreter.handling_error
                    && self.error == Self::error(interpreter)))
    }

    fn capture(interpreter: &Interpreter) -> Self {
        Self {
            run_depth: interpreter.run_depth,
            gosubs: interpreter.gosub_stack.clone(),
            subs: interpreter
                .active_subs
                .iter()
                .map(|frame| frame.name.clone())
                .collect(),
            functions: interpreter
                .active_functions
                .iter()
                .map(|frame| frame.name.clone())
                .collect(),
            timer_markers: interpreter.timer_isr_markers.clone(),
            mouse: interpreter.handling_mouse_event,
            handling_error: interpreter.handling_error,
            error: Self::error(interpreter),
        }
    }
}

/// Actual inherited frames, observed on entry rather than guessed from source
/// ranges. A handler may execute shared code after GOTO without leaving its
/// dynamic GOSUB/ISR/error continuation.
#[derive(Debug)]
pub(super) struct DebugControlScope {
    identity: DebugControlIdentity,
    known_entry: bool,
    fors: Vec<Cursor>,
    whiles: Vec<Cursor>,
    branches: Vec<Cursor>,
}

impl DebugControlScope {
    pub(super) fn preserve_return_plans(&mut self, frames: &[GosubFrame]) {
        for (snapshot, frame) in self.identity.gosubs.iter_mut().zip(frames) {
            if snapshot.return_cursor == frame.return_cursor && snapshot.if_depth == frame.if_depth
            {
                snapshot.return_plan = frame.return_plan.clone();
            }
        }
    }

    pub(super) fn relocate(
        &mut self,
        relocate: &impl Fn(&mut Cursor),
        line_index: &impl Fn(i32) -> usize,
    ) {
        for frame in &mut self.identity.gosubs {
            if let Some(plan) = &frame.return_plan {
                frame.return_cursor.line_idx = line_index(plan.line);
            } else {
                relocate(&mut frame.return_cursor);
            }
        }
        if let Some((_, line, retry, next)) = &mut self.identity.error {
            retry.line_idx = line_index(*line);
            next.line_idx = line_index(*line);
        }
        for cursor in self
            .fors
            .iter_mut()
            .chain(&mut self.whiles)
            .chain(&mut self.branches)
        {
            relocate(cursor);
        }
    }
}

fn for_header(frame: &ForFrame) -> Cursor {
    Cursor {
        line_idx: frame.resume.line_idx,
        cmd_idx: frame.resume.cmd_idx.saturating_sub(1),
    }
}

fn inherited_and_local_match(actual: &[Cursor], inherited: &[Cursor], lexical: &[Cursor]) -> bool {
    // Some source regions lexically include an inherited caller block. Count
    // that exact shared prefix only once; never accept an arbitrary suffix.
    let shared = inherited
        .iter()
        .zip(lexical)
        .take_while(|(a, b)| a == b)
        .count();
    actual.starts_with(inherited) && actual.get(inherited.len()..) == Some(&lexical[shared..])
}

impl Interpreter {
    #[cold]
    pub(super) fn sync_debug_control_scope(&mut self) {
        let Some(state) = self.debugger_state.as_ref() else {
            return;
        };
        if state.debugger.is_none() {
            return;
        }
        if state
            .control_scopes
            .last()
            .is_some_and(|scope| scope.identity.matches(self))
        {
            return;
        }
        if let Some(index) = state
            .control_scopes
            .iter()
            .rposition(|scope| scope.identity.matches(self))
        {
            self.debugger_state
                .as_mut()
                .unwrap()
                .control_scopes
                .truncate(index + 1);
            return;
        }
        // Two sibling FN calls can occur in one expression without a command
        // boundary in their caller. Find their actual observed ancestor rather
        // than treating the preceding, already returned sibling as the parent.
        let parent = state
            .control_scopes
            .iter()
            .rposition(|scope| scope.known_entry && scope.identity.is_parent_of(self));
        let known_entry = parent.is_some()
            || (state.control_scopes.is_empty() && {
                self.run_depth <= 1
                    && self.gosub_stack.is_empty()
                    && self.active_subs.is_empty()
                    && self.active_functions.is_empty()
                    && !self.handling_error
                    && !self.handling_mouse_event
                    && self.for_stack.is_empty()
                    && self.while_stack.is_empty()
                    && self.if_stack.is_empty()
            });
        // Cloning happens only at a new dynamic context, not at every command.
        let scope = DebugControlScope {
            identity: DebugControlIdentity::capture(self),
            known_entry,
            fors: self.for_stack.iter().map(for_header).collect(),
            whiles: self.while_stack.iter().map(|frame| frame.header).collect(),
            branches: self.if_stack.clone(),
        };
        let scopes = &mut self.debugger_state.as_mut().unwrap().control_scopes;
        if let Some(parent) = parent {
            scopes.truncate(parent + 1);
        }
        scopes.push(scope);
    }

    fn debug_navigation_entries(&self) -> Vec<NavigationEntry> {
        let mut entries = Vec::new();
        for (line_idx, &line) in self.line_numbers_cache.iter().enumerate() {
            let Some(code) = self.program.get(line) else {
                continue;
            };
            let Some(metadata) = self.execution_source_cache.get(&line) else {
                continue;
            };
            let Some(commands) = self.compiled_line_cache.get(line_idx) else {
                continue;
            };
            let prefix = line.to_string().len();
            for range in split_command_ranges(code) {
                let text = &code[range.clone()];
                let start = range.start + text.len() - text.trim_start().len();
                let span = start..start + text.trim().len();
                let Some(cmd_idx) = metadata.iter().position(|entry| {
                    entry.debugger_boundary && entry.source_span() == Some(span.clone())
                }) else {
                    continue;
                };
                if matches!(commands[cmd_idx].as_ref(), CachedCommand::Noop) {
                    continue;
                }
                entries.push(NavigationEntry {
                    cursor: Cursor { line_idx, cmd_idx },
                    target: DebugStatementTarget {
                        line,
                        source_span: prefix + span.start..prefix + span.end,
                    },
                });
            }
        }
        entries
    }

    #[cold]
    pub(super) fn debug_statement_targets(&self) -> Vec<DebugStatementTarget> {
        self.debug_navigation_entries()
            .into_iter()
            .map(|entry| entry.target)
            .collect()
    }

    // Interpret only already compiled structure, never BASIC expressions. Each
    // signature describes the frames required immediately BEFORE the command.
    fn debug_navigation_contexts(&self) -> HashMap<Cursor, NavigationContext> {
        let mut contexts = HashMap::new();
        let top_level: HashSet<_> = self
            .debug_navigation_entries()
            .into_iter()
            .map(|entry| entry.cursor)
            .collect();
        let mut entry_lines: HashSet<_> = self
            .timers
            .iter()
            .map(|timer| timer.target)
            .chain(self.mouse_handlers.values().copied())
            .chain(self.error_handler_line)
            .collect();
        for commands in self.compiled_line_cache.iter() {
            for command in commands.iter() {
                match command.as_ref() {
                    CachedCommand::GosubConst { line, .. } => {
                        entry_lines.insert(*line);
                    }
                    CachedCommand::OnGosub { targets, .. } => entry_lines.extend(targets),
                    CachedCommand::Raw(source) => {
                        let upper = source.trim().to_ascii_uppercase();
                        let literal = if upper.starts_with("AFTER ")
                            || upper.starts_with("EVERY ")
                            || upper.starts_with("ON MOUSE ")
                        {
                            upper.rsplit_once(" GOSUB ").map(|(_, target)| target)
                        } else {
                            upper.strip_prefix("ON ERROR GOTO ")
                        };
                        if let Some(line) = literal.and_then(|text| text.trim().parse::<i32>().ok())
                        {
                            entry_lines.insert(line);
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut context = NavigationContext::default();
        let mut enclosing_routines = Vec::new();
        for (line_idx, commands) in self.compiled_line_cache.iter().enumerate() {
            let mut routine_boundary_line = false;
            for (cmd_idx, command) in commands.iter().enumerate() {
                let cursor = Cursor { line_idx, cmd_idx };
                if cmd_idx == 0 && entry_lines.contains(&self.line_numbers_cache[line_idx]) {
                    context.flow_region = Some(cursor);
                }
                let structure = classify_cached_structure(command.as_ref());
                let mut before = context.clone();
                // Multiline routines start at the next line and own the full
                // ending line. Trailing commands on their boundary lines are
                // not ordinary body/main entry points.
                before.ambiguous |= routine_boundary_line;
                // A branch header has two distinct runtime states (a pending
                // false branch or an already executed preceding branch).
                if matches!(
                    structure,
                    Some(CachedStructureKind::ElseIf | CachedStructureKind::Else)
                ) {
                    before.ambiguous = true;
                }
                contexts.insert(cursor, before);
                match structure {
                    Some(CachedStructureKind::For) => context.fors.push(cursor),
                    Some(CachedStructureKind::Next) => {
                        if context.fors.pop().is_none() {
                            context.ambiguous = true;
                        }
                    }
                    Some(CachedStructureKind::While) => context.whiles.push(cursor),
                    Some(CachedStructureKind::Wend) => {
                        if context.whiles.pop().is_none() {
                            context.ambiguous = true;
                        }
                    }
                    Some(CachedStructureKind::IfStart) => context.branches.push(cursor),
                    Some(CachedStructureKind::ElseIf | CachedStructureKind::Else) => {
                        if let Some(branch) = context.branches.last_mut() {
                            *branch = cursor;
                        } else {
                            context.ambiguous = true;
                        }
                    }
                    Some(CachedStructureKind::EndIf) => {
                        if context.branches.pop().is_none() {
                            context.ambiguous = true;
                        }
                    }
                    None => {}
                }
                if matches!(
                    command.as_ref(),
                    CachedCommand::FnEnd | CachedCommand::SubEnd
                ) {
                    routine_boundary_line = true;
                    context = enclosing_routines
                        .pop()
                        .unwrap_or_else(|| NavigationContext {
                            ambiguous: true,
                            ..NavigationContext::default()
                        });
                } else if let CachedCommand::Raw(source) = command.as_ref() {
                    let source = strip_comment(source);
                    let source = source.trim();
                    let upper = source.to_ascii_uppercase();
                    let routine = if upper.starts_with("DEF SUB") {
                        parse_sub_header(source[7..].trim())
                            .ok()
                            .map(|(name, _)| (DebugFrameKind::Sub, name))
                    } else if upper.starts_with("DEF FN") && !source.contains('=') {
                        parse_function_header(source[4..].trim())
                            .ok()
                            .map(|(name, _)| (DebugFrameKind::Function, name))
                    } else {
                        None
                    };
                    if let Some((kind, name)) = routine {
                        routine_boundary_line = true;
                        let nested = context.routine.is_some();
                        enclosing_routines.push(context);
                        context = NavigationContext {
                            routine: Some((kind, name, cursor)),
                            ambiguous: nested,
                            ..NavigationContext::default()
                        };
                    }
                }
                // Unstructured BASIC has no explicit GOSUB body declaration.
                // Use known entry points and terminating whole statements as
                // conservative barriers, without claiming to solve dynamic CFGs.
                if top_level.contains(&cursor)
                    && (matches!(command.as_ref(), CachedCommand::Return)
                        || matches!(command.as_ref(), CachedCommand::Raw(source)
                            if source.split_whitespace().next().is_some_and(|word|
                                word.eq_ignore_ascii_case("END") || word.eq_ignore_ascii_case("RESUME"))))
                {
                    context.flow_region = Some(Cursor {
                        line_idx,
                        cmd_idx: cmd_idx + 1,
                    });
                }
            }
        }
        contexts
    }

    #[cold]
    pub(super) fn debug_validate_next(
        &self,
        current: Cursor,
        target: &DebugStatementTarget,
    ) -> Result<Cursor, String> {
        let destination = self
            .debug_navigation_entries()
            .into_iter()
            .find(|entry| entry.target == *target)
            .ok_or("Select a complete top-level statement")?
            .cursor;
        if destination == current {
            return Ok(destination);
        }
        let depth = self.run_depth + self.gosub_stack.len();
        let suspended_here = self
            .debugger_state
            .as_ref()
            .is_some_and(|state| state.resume_guards.contains(&(current, depth)))
            && self
                .debug_wait_command(
                    &current,
                    self.line_numbers_cache
                        .get(current.line_idx)
                        .and_then(|line| self.execution_source_cache.get(line))
                        .and_then(|metadata| metadata.get(current.cmd_idx)),
                    None,
                )
                .is_some();
        if (self.last_error.is_some() && !self.handling_error)
            || self
                .pause_deadline
                .is_some_and(|(owner, _)| owner == current)
            || suspended_here
            || self.repeat_current_command
            || self.pending_if_branch.is_some()
        {
            return Err("Cannot move the next statement from this suspended wait or pending control transition".into());
        }
        let contexts = self.debug_navigation_contexts();
        let source = contexts
            .get(&current)
            .ok_or("The current statement is no longer available")?;
        let destination_context = contexts
            .get(&destination)
            .ok_or("The target statement is no longer available")?;
        if source.ambiguous || destination_context.ambiguous || source != destination_context {
            return Err("Cannot cross a loop, IF branch, routine or handler boundary".into());
        }
        let active_routine = self
            .debugger_state
            .as_ref()
            .and_then(|state| state.active_routines.last());
        let handler_active =
            !self.gosub_stack.is_empty() || self.handling_error || self.handling_mouse_event;
        let routine_matches = match (&source.routine, active_routine) {
            (None, None) => true,
            (None, Some(_)) if handler_active => true,
            (Some((kind, name, _)), Some(active)) => {
                *kind == active.kind && name.as_str() == active.name.as_ref()
            }
            _ => false,
        };
        let actual_fors: Vec<_> = self.for_stack.iter().map(for_header).collect();
        let actual_whiles: Vec<_> = self.while_stack.iter().map(|frame| frame.header).collect();
        if handler_active {
            let scope = self
                .debugger_state
                .as_ref()
                .and_then(|state| state.control_scopes.last())
                .filter(|scope| scope.known_entry && scope.identity.matches(self))
                .ok_or("The debugger did not observe entry into this active handler")?;
            if !routine_matches
                || !inherited_and_local_match(&actual_fors, &scope.fors, &source.fors)
                || !inherited_and_local_match(&actual_whiles, &scope.whiles, &source.whiles)
                || !inherited_and_local_match(&self.if_stack, &scope.branches, &source.branches)
            {
                return Err(
                    "The active control-flow frames do not match this source region".into(),
                );
            }
            return Ok(destination);
        }
        // Bases are captured by the debugger at routine entry, including
        // recursive calls. Compare the whole local stack, not just a suffix:
        // stale frames left by BASIC jumps must not make an unsafe move legal.
        let for_base = active_routine.map_or(0, |frame| frame.for_base);
        let while_base = active_routine.map_or(0, |frame| frame.while_base);
        let loops_match = actual_fors.get(for_base..) == Some(source.fors.as_slice())
            && actual_whiles.get(while_base..) == Some(source.whiles.as_slice());
        if !routine_matches || !loops_match || self.if_stack != source.branches {
            return Err("The active control-flow frames do not match this source region".into());
        }
        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed_interpreter(source: &str) -> Interpreter {
        let mut interpreter = Interpreter::new();
        interpreter.program.load_text(source).unwrap();
        interpreter.rebuild_command_cache();
        interpreter.set_debugger(Debugger::scripted([]));
        interpreter.run_depth = 1;
        interpreter.sync_debug_control_scope();
        interpreter
    }

    fn navigation_target(interpreter: &Interpreter, line: i32) -> DebugStatementTarget {
        interpreter
            .debug_statement_targets()
            .into_iter()
            .find(|target| target.line == line)
            .unwrap()
    }

    fn numeric_for(header: Cursor) -> ForFrame {
        ForFrame {
            var: Rc::from("I"),
            var_slot: 0,
            end: 2.0,
            step: 1.0,
            resume: Cursor {
                line_idx: header.line_idx,
                cmd_idx: header.cmd_idx + 1,
            },
        }
    }

    #[test]
    fn observed_handler_prefixes_are_exact_and_stale_local_frames_are_not_accepted() {
        let mut interpreter = observed_interpreter(
            "10 FOR I=1 TO 2\n20 GOSUB 100\n30 NEXT I\n40 END\n100 FOR J=1 TO 2\n110 A=1\n120 NEXT J\n130 A=2\n140 A=3\n150 RETURN"
        );
        let caller_for = Cursor {
            line_idx: 0,
            cmd_idx: 0,
        };
        let local_for = Cursor {
            line_idx: 4,
            cmd_idx: 0,
        };
        let return_cursor = Cursor {
            line_idx: 2,
            cmd_idx: 0,
        };
        interpreter.for_stack.push(numeric_for(caller_for));
        interpreter.push_gosub_return(return_cursor);
        interpreter.sync_debug_control_scope();
        let current = Cursor {
            line_idx: 7,
            cmd_idx: 0,
        };
        let target = navigation_target(&interpreter, 140);
        assert!(interpreter.debug_validate_next(current, &target).is_ok());
        // A BASIC GOTO out of the local loop can leave its frame behind. It is
        // not another inherited caller frame just because it precedes a suffix.
        interpreter.for_stack.push(numeric_for(local_for));
        assert!(interpreter.debug_validate_next(current, &target).is_err());
        interpreter.for_stack.pop();
        interpreter.for_stack[0] = numeric_for(local_for);
        assert!(interpreter.debug_validate_next(current, &target).is_err());
        interpreter.for_stack.clear();
        assert!(interpreter.debug_validate_next(current, &target).is_err());
        assert_eq!(
            interpreter
                .gosub_stack
                .iter()
                .map(|frame| frame.return_cursor)
                .collect::<Vec<_>>(),
            [return_cursor]
        );
    }

    #[test]
    fn a_handler_keeps_inherited_if_frames_but_requires_its_own_active_branch() {
        let mut interpreter = observed_interpreter(
            "10 IF 1 THEN\n20 GOSUB 100\n30 END IF\n40 END\n100 IF 1 THEN\n110 A=1\n120 A=2\n130 END IF\n140 RETURN"
        );
        let caller_if = Cursor {
            line_idx: 0,
            cmd_idx: 0,
        };
        let local_if = Cursor {
            line_idx: 4,
            cmd_idx: 0,
        };
        interpreter.if_stack.push(caller_if);
        interpreter.push_gosub_return(Cursor {
            line_idx: 2,
            cmd_idx: 0,
        });
        interpreter.sync_debug_control_scope();
        interpreter.if_stack.push(local_if);
        let current = Cursor {
            line_idx: 5,
            cmd_idx: 0,
        };
        let target = navigation_target(&interpreter, 120);
        assert!(interpreter.debug_validate_next(current, &target).is_ok());
        interpreter.if_stack.remove(0);
        assert!(interpreter.debug_validate_next(current, &target).is_err());
    }

    #[test]
    fn caller_wait_is_preserved_in_timer_and_mouse_handlers_but_own_wait_is_rejected() {
        for mouse in [false, true] {
            let mut interpreter = observed_interpreter(
                "10 AFTER 1,1 GOSUB 100\n20 PAUSE 1000\n30 END\n100 A=1\n110 A=2\n120 FRAME 60\n130 RETURN"
            );
            let waiting = Cursor {
                line_idx: 1,
                cmd_idx: 0,
            };
            let deadline = Instant::now() + Duration::from_secs(1);
            interpreter.pause_deadline = Some((waiting, deadline));
            interpreter
                .debugger_state
                .as_mut()
                .unwrap()
                .resume_guards
                .push((waiting, 1));
            interpreter.push_gosub_return(waiting);
            if mouse {
                interpreter.handling_mouse_event = true;
                interpreter.run_depth += 1;
                interpreter
                    .debugger_state
                    .as_mut()
                    .unwrap()
                    .mouse_isr_markers
                    .push(0);
            } else {
                interpreter.timer_isr_markers.push(0);
                interpreter.timer_isr_stack.push(TimerInterruptState {
                    priority: -1,
                    interrupts_enabled: true,
                });
                interpreter.current_interrupt_priority = 1;
            }
            interpreter.sync_debug_control_scope();
            let current = Cursor {
                line_idx: 3,
                cmd_idx: 0,
            };
            let target = navigation_target(&interpreter, 110);
            assert!(
                interpreter.debug_validate_next(current, &target).is_ok(),
                "mouse={mouse}"
            );
            assert_eq!(interpreter.pause_deadline, Some((waiting, deadline)));
            assert_eq!(
                interpreter
                    .gosub_stack
                    .iter()
                    .map(|frame| frame.return_cursor)
                    .collect::<Vec<_>>(),
                [waiting]
            );
            assert_eq!(interpreter.handling_mouse_event, mouse);
            assert_eq!(
                interpreter.timer_isr_markers,
                if mouse { vec![] } else { vec![0] }
            );
            let frame_wait = Cursor {
                line_idx: 5,
                cmd_idx: 0,
            };
            let depth = interpreter.run_depth + interpreter.gosub_stack.len();
            interpreter
                .debugger_state
                .as_mut()
                .unwrap()
                .resume_guards
                .push((frame_wait, depth));
            assert!(interpreter
                .debug_validate_next(frame_wait, &target)
                .is_err());
        }
    }

    #[test]
    fn unobserved_handler_entries_are_not_guessed_from_a_matching_stack_suffix() {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text("10 GOSUB 100\n20 END\n100 A=1\n110 A=2\n120 RETURN")
            .unwrap();
        interpreter.rebuild_command_cache();
        interpreter.run_depth = 1;
        interpreter.push_gosub_return(Cursor {
            line_idx: 1,
            cmd_idx: 0,
        });
        interpreter.set_debugger(Debugger::scripted([]));
        interpreter.sync_debug_control_scope();
        let current = Cursor {
            line_idx: 2,
            cmd_idx: 0,
        };
        let target = navigation_target(&interpreter, 110);
        assert!(interpreter
            .debug_validate_next(current, &target)
            .unwrap_err()
            .contains("did not observe"));
    }

    #[test]
    fn inline_return_sentinel_preserves_caller_if_across_nested_early_returns() {
        for debugging in [false, true] {
            let mut interpreter = Interpreter::new();
            interpreter.program.load_text(
                "10 IF 1 THEN\n20 A=99\n30 END IF\n40 END\n100 IF 1 THEN\n110 GOSUB 200\n120 RETURN\n130 END IF\n140 RETURN\n200 IF 1 THEN\n210 A=A+1\n220 RETURN\n230 END IF\n240 RETURN"
            ).unwrap();
            interpreter.rebuild_command_cache();
            let caller_if = Cursor {
                line_idx: 0,
                cmd_idx: 0,
            };
            interpreter.if_stack.push(caller_if);
            if debugging {
                interpreter.set_debugger(Debugger::scripted([]));
            }
            interpreter
                .execute_inline_gosub_line(100, false, None)
                .unwrap();
            assert_eq!(interpreter.if_stack, [caller_if], "debugging={debugging}");
            assert!(interpreter.gosub_stack.is_empty());
            assert_eq!(interpreter.numeric_variables.get("A"), Some(&1.0));
        }
    }

    #[test]
    fn an_outer_gosub_floor_does_not_belong_to_a_replaced_function_or_sub_if_stack() {
        let mut interpreter = observed_interpreter(
            "10 IF 1 THEN\n20 GOSUB 100\n30 END IF\n40 END\n100 IF 1 THEN\n110 A=1\n120 END IF\n130 RETURN\n200 A=2\n210 RETURN"
        );
        let caller_if = interpreter.cursor_for_line(10).unwrap();
        let local_if = interpreter.cursor_for_line(100).unwrap();
        let caller_return = interpreter.cursor_for_line(30).unwrap();
        interpreter.if_stack.push(caller_if);
        interpreter.push_gosub_return(caller_return);
        let outer_frame = interpreter.gosub_stack[0].clone();
        // Model the exact IF-stack replacement at DEF FN / SUB entry.
        interpreter.if_gosub_base = interpreter.gosub_stack.len();
        interpreter.if_stack = vec![local_if];
        assert_eq!(interpreter.protected_if_depth(), 0);
        let mut cursor = interpreter.cursor_for_line(110).unwrap();
        interpreter.jump_to_line(200, &mut cursor).unwrap();
        assert!(interpreter.if_stack.is_empty());
        assert_eq!(interpreter.gosub_stack, [outer_frame.clone()]);

        interpreter.if_stack.push(local_if);
        let local_return = interpreter.cursor_for_line(120).unwrap();
        interpreter.push_gosub_return(local_return);
        assert_eq!(interpreter.protected_if_depth(), 1);
        interpreter.jump_to_line(200, &mut cursor).unwrap();
        assert_eq!(interpreter.if_stack, [local_if]);
        interpreter.execute_return(&mut cursor).unwrap();
        assert_eq!(cursor, local_return);
        assert_eq!(interpreter.gosub_stack, [outer_frame.clone()]);
        assert_eq!(interpreter.protected_if_depth(), 0);
        let unchanged_cursor = cursor;
        assert_eq!(
            interpreter.execute_return(&mut cursor).unwrap_err().code,
            ErrorCode::ReturnWithoutGosub
        );
        assert_eq!(cursor, unchanged_cursor);
        assert_eq!(interpreter.gosub_stack, [outer_frame]);
        assert_eq!(interpreter.if_stack, [local_if]);
    }

    #[test]
    fn nested_timer_gotos_preserve_each_suspended_if_and_return_priority() {
        let mut interpreter = observed_interpreter(
            "10 IF 1 THEN\n20 PAUSE 1000\n30 END IF\n40 END\n100 IF 1 THEN\n110 A=1\n120 END IF\n140 RETURN\n200 IF 1 THEN\n210 A=2\n220 END IF\n240 RETURN"
        );
        let caller_if = interpreter.cursor_for_line(10).unwrap();
        let timer_if = interpreter.cursor_for_line(100).unwrap();
        let nested_if = interpreter.cursor_for_line(200).unwrap();
        let waiting = interpreter.cursor_for_line(20).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        interpreter.pause_deadline = Some((waiting, deadline));
        interpreter.if_stack.push(caller_if);
        let mut cursor = waiting;
        interpreter.timers.push(BasicTimer {
            number: 1,
            interval: Duration::from_secs(1),
            next_fire: Instant::now(),
            target: 100,
            repeat: false,
            active: true,
        });
        assert!(interpreter.process_timers(&mut cursor).unwrap());
        assert_eq!(interpreter.protected_if_depth(), 1);
        interpreter.if_stack.push(timer_if);
        cursor = interpreter.cursor_for_line(110).unwrap();
        let timer_return = cursor;
        interpreter.timers.push(BasicTimer {
            number: 2,
            interval: Duration::from_secs(1),
            next_fire: Instant::now(),
            target: 200,
            repeat: false,
            active: true,
        });
        assert!(interpreter.process_timers(&mut cursor).unwrap());
        assert_eq!(interpreter.protected_if_depth(), 2);
        interpreter.if_stack.push(nested_if);
        interpreter.jump_to_line(240, &mut cursor).unwrap();
        assert_eq!(interpreter.if_stack, [caller_if, timer_if]);
        interpreter.interrupts_enabled = false;
        interpreter.execute_return(&mut cursor).unwrap();
        assert_eq!(cursor, timer_return);
        assert_eq!(interpreter.current_interrupt_priority, 1);
        assert!(interpreter.interrupts_enabled);
        interpreter.jump_to_line(140, &mut cursor).unwrap();
        assert_eq!(interpreter.if_stack, [caller_if]);
        interpreter.execute_return(&mut cursor).unwrap();
        assert_eq!(cursor, waiting);
        assert_eq!(interpreter.current_interrupt_priority, -1);
        assert!(interpreter.timer_isr_stack.is_empty());
        assert!(interpreter.timer_isr_markers.is_empty());
        assert!(interpreter.gosub_stack.is_empty());
        assert_eq!(interpreter.pause_deadline, Some((waiting, deadline)));
        assert_eq!(interpreter.if_stack, [caller_if]);
    }

    #[test]
    fn unmatched_callee_endif_and_else_cannot_pop_a_suspended_caller_if() {
        let mut interpreter = observed_interpreter(
            "10 IF 1 THEN\n20 GOSUB 100\n30 END IF\n40 END\n100 IF 1 THEN\n110 A=1\n120 ELSE\n130 A=2\n140 END IF\n150 RETURN"
        );
        let caller_if = interpreter.cursor_for_line(10).unwrap();
        interpreter.if_stack.push(caller_if);
        let returning = interpreter.cursor_for_line(30).unwrap();
        interpreter.push_gosub_return(returning);
        let mut end_if = interpreter.cursor_for_line(140).unwrap();
        let mut else_cursor = interpreter.cursor_for_line(120).unwrap();
        assert_eq!(
            interpreter.execute_end_if(&mut end_if).unwrap_err().code,
            ErrorCode::EndIfWithoutIf
        );
        assert!(interpreter.execute_else(&mut else_cursor).is_err());
        assert_eq!(interpreter.if_stack, [caller_if]);
        assert_eq!(interpreter.gosub_stack[0].return_cursor, returning);
    }

    #[test]
    fn restart_rebuilds_declarations_after_nested_bindings_have_unwound() {
        let mut interpreter = Interpreter::new();
        interpreter.program.load_text(
            "10 DEF FNA(X)=X+1\n20 DEF SUB WORK(A)\n30 LOCAL TEMP(2)\n40 TEMP(1)=7\n50 A(1)=5\n60 SUBEND\n70 DIM BUF(2):BUF(1)=9:X=8\n80 ON ERROR GOTO 200\n90 EVERY 100,1 GOSUB 210:READ D\n100 CALL WORK(BUF)\n110 END\n200 RESUME NEXT\n210 RETURN\n220 DATA 9"
        ).unwrap();
        let directory = interpreter.current_dir.clone();
        let mut debugger = Debugger::scripted([DebugAction::Restart, DebugAction::Abort]);
        debugger.replace_breakpoints([50]);
        interpreter.set_debugger(debugger);
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
        let snapshots = interpreter.debugger().unwrap().snapshots();
        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.location.line)
                .collect::<Vec<_>>(),
            [50, 10]
        );
        assert!(snapshots[1].variables.is_empty());
        assert!(snapshots[1].arrays.is_empty());
        assert_eq!(snapshots[1].stack.len(), 1);
        assert!(snapshots[1].timers.is_empty());
        assert_eq!(snapshots[1].err, 0);
        assert!(matches!(
            snapshots[1].data,
            DebugDataSnapshot::Next {
                position: 0,
                line: 220,
                line_item: 1,
                ..
            }
        ));
        assert!(interpreter.functions.contains_key("FNA"));
        assert!(interpreter.subs.contains_key("WORK"));
        assert!(interpreter.single_line_function_cache.is_empty());
        assert!(interpreter.fn_line_owner.is_empty());
        assert_eq!(
            interpreter.sub_line_owner.get(&50).map(String::as_str),
            Some("WORK")
        );
        assert!(interpreter.function_call_stack.is_empty());
        assert!(interpreter.sub_call_stack.is_empty());
        assert!(interpreter.active_functions.is_empty());
        assert!(interpreter.active_subs.is_empty());
        assert!(interpreter.for_stack.is_empty());
        assert!(interpreter.while_stack.is_empty());
        assert!(interpreter.if_stack.is_empty());
        assert!(interpreter.gosub_stack.is_empty());
        assert!(interpreter.array_aliases.is_empty());
        assert!(interpreter.error_handler_line.is_none());
        assert!(interpreter.last_error.is_none());
        assert!(interpreter.pause_deadline.is_none());
        assert_eq!(interpreter.current_dir, directory);
        assert!(interpreter.debugger().unwrap().is_breakpoint(50));
    }

    #[test]
    fn restart_from_an_attached_debugger_continuation_does_not_leak_its_sentinel() {
        for command in ["CONT", "GOTO 30"] {
            let mut interpreter = Interpreter::new();
            interpreter
                .program
                .load_text("10 A=1\n20 STOP\n30 A=2\n40 END")
                .unwrap();
            let mut debugger = Debugger::scripted([DebugAction::Restart, DebugAction::Abort]);
            debugger.replace_breakpoints([30]);
            interpreter.set_debugger(debugger);
            assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::Stop);
            interpreter.process_immediate(command).unwrap();
            assert!(interpreter.numeric_variables.get("A").is_none());
            assert!(interpreter.stopped_cursor.is_none());
            assert_eq!(
                interpreter
                    .debugger()
                    .unwrap()
                    .snapshots()
                    .iter()
                    .map(|snapshot| snapshot.location.line)
                    .collect::<Vec<_>>(),
                [30, 10]
            );
        }
    }

    #[test]
    fn only_the_outermost_run_boundary_can_consume_a_restart_request() {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text("10 A=1\n20 DEF FNA(X)=X+1")
            .unwrap();
        let mut debugger = Debugger::scripted([DebugAction::Restart]);
        debugger.replace_breakpoints([10]);
        interpreter.set_debugger(debugger);
        // RUN is immediate-only in BASIC; model a nested embedding boundary
        // directly to lock down the unwind invariant independently of syntax.
        interpreter.run_depth = 1;
        let result = interpreter.run_loaded();
        assert_eq!(
            result.as_ref().unwrap_err().detail.as_deref(),
            Some(DEBUG_RESTART_DETAIL)
        );
        assert_eq!(interpreter.run_depth, 1);
        assert!(!interpreter.functions.is_empty());
        assert!(!interpreter.prepare_debug_restart(&result));
        interpreter.run_depth = 0;
        assert!(interpreter.prepare_debug_restart(&result));
        assert!(interpreter.functions.is_empty());
        assert!(interpreter.debugger_mut().unwrap().take_pause_request());
    }

    #[test]
    fn navigation_and_edits_share_one_pause_and_one_deadline_shift() {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text("10 A=A+1:A=A+2\n20 END")
            .unwrap();
        interpreter.rebuild_command_cache();
        interpreter.numeric_variables.insert("A".into(), 1.0);
        let mut debugger = Debugger::interactive_editable(|initial, _, access| {
            let target = access
                .statement_targets()
                .into_iter()
                .find(|target| {
                    target.line == 10
                        && target.source_span.start
                            > initial.location.source_span.as_ref().unwrap().start
                })
                .unwrap();
            let moved = access.set_next(&target).unwrap();
            assert_eq!(moved.location.command, "A=A+2");
            assert_eq!(moved.timers, initial.timers);
            std::thread::sleep(Duration::from_millis(10));
            let edited = access.set_scalar("A", DebugValue::Number(5.0)).unwrap();
            assert_eq!(edited.location, moved.location);
            assert_eq!(edited.timers, initial.timers);
            std::thread::sleep(Duration::from_millis(10));
            Ok(DebugAction::StepInto)
        });
        debugger.request_pause();
        interpreter.set_debugger(debugger);
        let cursor = Cursor {
            line_idx: 0,
            cmd_idx: 0,
        };
        let base = Instant::now();
        let deadline = base + Duration::from_secs(1);
        interpreter.timers.push(BasicTimer {
            number: 1,
            interval: Duration::from_secs(1),
            next_fire: deadline,
            target: 20,
            repeat: true,
            active: true,
        });
        interpreter.last_frame_command_present = Some(base);
        let started = Instant::now();
        assert_eq!(
            interpreter
                .debug_before_cached_command(&cursor, 10, None, None)
                .unwrap(),
            DebugHookControl::Relocate(Cursor {
                line_idx: 0,
                cmd_idx: 1
            })
        );
        let duration = started.elapsed();
        let shifted = interpreter.timers[0].next_fire.duration_since(deadline);
        assert!(shifted >= Duration::from_millis(20));
        assert!(shifted <= duration);
        assert_eq!(interpreter.last_frame_command_present, Some(base + shifted));
        assert_eq!(interpreter.numeric_variables.get("A"), Some(&5.0));
    }

    #[test]
    fn navigation_rejects_suspended_waits_and_changed_inline_source_atomically() {
        let mut interpreter = Interpreter::new();
        interpreter.program.load_text("10 A=1:A=2\n20 END").unwrap();
        interpreter.rebuild_command_cache();
        let current = Cursor {
            line_idx: 0,
            cmd_idx: 0,
        };
        let target = interpreter.debug_statement_targets().pop().unwrap();
        let deadline = Instant::now();
        interpreter.pause_deadline = Some((current, deadline));
        assert!(interpreter.debug_validate_next(current, &target).is_err());
        assert_eq!(interpreter.pause_deadline, Some((current, deadline)));
        interpreter.pause_deadline = None;
        interpreter.handling_mouse_event = true;
        assert!(interpreter.debug_validate_next(current, &target).is_err());
        interpreter.handling_mouse_event = false;
        let snapshot = interpreter.build_debug_snapshot_at(
            &current,
            10,
            None,
            Some(" OLD=1"),
            DebugPauseReason::Step,
        );
        let mut access = InterpreterDebugPause {
            interpreter: &mut interpreter,
            cursor: current,
            line: 10,
            metadata: None,
            source_code: Some(" OLD=1"),
            initial_snapshot: &snapshot,
        };
        assert!(access
            .set_next(&target)
            .unwrap_err()
            .contains("source has changed"));
        assert_eq!(access.cursor, current);
        assert_eq!(access.snapshot().location, snapshot.location);
    }

    #[test]
    fn known_subroutine_and_handler_entries_and_terminators_are_navigation_barriers() {
        for source in [
            "10 A=1\n20 GOSUB 100\n30 B=2\n100 C=3\n110 RETURN",
            "10 A=1\n20 ON 1 GOSUB 100\n30 B=2\n100 C=3\n110 RETURN",
            "10 A=1\n20 AFTER 1,1 GOSUB 100\n30 B=2\n100 C=3\n110 RETURN",
            "10 A=1\n20 EVERY 1,1 GOSUB 100\n30 B=2\n100 C=3\n110 RETURN",
            "10 A=1\n20 ON MOUSE LEFT GOSUB 100\n30 B=2\n100 C=3\n110 RETURN",
            "10 A=1\n20 ON ERROR GOTO 100\n30 B=2\n100 C=3\n110 RESUME NEXT",
            "10 A=1\n20 END\n100 B=2",
            "10 A=1\n20 RETURN\n100 B=2",
        ] {
            let mut interpreter = Interpreter::new();
            interpreter.program.load_text(source).unwrap();
            interpreter.rebuild_command_cache();
            let destination = interpreter
                .debug_statement_targets()
                .into_iter()
                .find(|target| target.line == 100)
                .unwrap();
            assert!(
                interpreter
                    .debug_validate_next(
                        Cursor {
                            line_idx: 0,
                            cmd_idx: 0
                        },
                        &destination
                    )
                    .is_err(),
                "{source}"
            );
        }
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text("10 A=1\n20 B=2\n100 C=3")
            .unwrap();
        interpreter.rebuild_command_cache();
        let destination = interpreter.debug_statement_targets().pop().unwrap();
        let current = Cursor {
            line_idx: 0,
            cmd_idx: 0,
        };
        assert!(interpreter
            .debug_validate_next(current, &destination)
            .is_ok());
        interpreter.error_handler_line = Some(100);
        assert!(interpreter
            .debug_validate_next(current, &destination)
            .is_err());
        interpreter.error_handler_line = None;
        interpreter.mouse_handlers.insert("LEFT".into(), 100);
        assert!(interpreter
            .debug_validate_next(current, &destination)
            .is_err());
    }
}
