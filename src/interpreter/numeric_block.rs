//! A straight-line sequence shares scalar operands across adjacent assignments.
//! Each store commits one BASIC command. A failing command leaves the cursor at
//! its original position for diagnostics, ON ERROR, RESUME and RESUME NEXT.
use super::*;

const REGISTERS: usize = 32;
const MAX_STATEMENTS: usize = 8;

#[derive(Debug, Clone, Copy)]
enum Instruction {
    EnterLine(i32),
    Literal(u8, f64),
    Load(u8, u8),
    Read(u8, u8, u8),
    Binary(u8, BinaryOp, u8, u8),
    Unary(u8, UnaryOp, u8),
    Function(u8, FastNumberFunction, u8),
    CachedTrig(u8, u8, u8),
    Store(u8, u8),
    Write(u8, u8, u8),
}

#[derive(Debug, Clone, Default)]
pub(super) struct Plan {
    pub(super) statements: u8,
    // Cross-line blocks retain all original command locations and the fall-through.
    pub(super) positions: Vec<Cursor>,
    code: Vec<Instruction>,
    scalar_names: Vec<String>,
    scalar_slots: Vec<Cell<usize>>,
    scalar_table_id: Cell<u64>,
    arrays: Vec<(String, CachedArraySlot)>,
    trig_sites: Vec<TrigSite>,
}

#[derive(Debug, Clone)]
struct TrigSite {
    function: FastNumberFunction,
    last: Cell<Option<TrigValue>>,
}

#[derive(Debug, Clone, Copy)]
struct TrigValue {
    input_bits: u64,
    degrees: bool,
    value: f64,
}

impl TrigSite {
    #[inline]
    fn evaluate(&self, input: f64, degrees: bool) -> BasicResult<f64> {
        let input_bits = input.to_bits();
        if let Some(last) = self.last.get() {
            if last.input_bits == input_bits && last.degrees == degrees {
                return Ok(last.value);
            }
        }
        let value = eval_fast_number_function(self.function, input, degrees)?;
        self.last.set(Some(TrigValue { input_bits, degrees, value }));
        Ok(value)
    }
}

impl Plan {
    #[cold]
    fn bind_scalar_slots(&self, variables: &mut NumericVariables) {
        for (name, slot) in self.scalar_names.iter().zip(&self.scalar_slots) {
            // A slot reserved by name remains absent until BASIC assigns it.
            slot.set(variables.slot_for_name(name));
        }
        self.scalar_table_id.set(variables.table_id);
    }

    #[inline(never)]
    pub(super) fn execute(
        &self,
        interpreter: &mut Interpreter,
        cursor: &mut Cursor,
        values: &mut [f64; REGISTERS],
    ) -> BasicResult<()> {
        // No instruction in a numeric block can replace the scalar table.
        if self.scalar_table_id.get() != interpreter.numeric_variables.table_id {
            self.bind_scalar_slots(&mut interpreter.numeric_variables);
        }
        for &instruction in &self.code {
            match instruction {
                Instruction::EnterLine(number) => interpreter.current_line = Some(number),
                Instruction::Literal(out, number) => values[out as usize] = number,
                Instruction::Load(out, scalar) => {
                    let slot = self.scalar_slots[scalar as usize].get();
                    values[out as usize] = interpreter.numeric_variables.get_resolved_or_zero(slot);
                }
                Instruction::Read(out, array, index) => {
                    let (name, slot) = &self.arrays[array as usize];
                    let raw = values[index as usize];
                    let index = raw as i32;
                    if raw != f64::from(index) && raw.fract() != 0.0 {
                        return Err(BasicError::new(ErrorCode::InvalidIndex));
                    }
                    values[out as usize] =
                        interpreter.get_array_number_cached_1d(name, slot, index)?;
                }
                Instruction::Binary(out, op, left, right) => {
                    values[out as usize] =
                        eval_fast_binary(op, values[left as usize], values[right as usize])?;
                }
                Instruction::Unary(out, op, operand) => {
                    values[out as usize] = eval_fast_unary(op, values[operand as usize]);
                }
                Instruction::Function(out, function, operand) => {
                    values[out as usize] = eval_fast_number_function(
                        function,
                        values[operand as usize],
                        interpreter.angle_degrees,
                    )?;
                }
                Instruction::CachedTrig(out, site, operand) => {
                    values[out as usize] = self.trig_sites[site as usize]
                        .evaluate(values[operand as usize], interpreter.angle_degrees)?;
                }
                Instruction::Store(scalar, value) => {
                    let slot = self.scalar_slots[scalar as usize].get();
                    interpreter
                        .numeric_variables
                        .insert_resolved(slot, values[value as usize]);
                    cursor.cmd_idx += 1;
                    if console::interrupt_requested() {
                        return Ok(());
                    }
                }
                Instruction::Write(array, index, value) => {
                    let (name, slot) = &self.arrays[array as usize];
                    // Destination subscripts follow the assignment path's truncating cast.
                    interpreter.assign_compiled_numeric_array_value(
                        name,
                        slot,
                        &[values[index as usize] as i32],
                        values[value as usize],
                    )?;
                    cursor.cmd_idx += 1;
                    if console::interrupt_requested() {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct Builder {
    plan: Plan,
    registers: usize,
    current_values: Vec<Option<u8>>,
}

impl Builder {
    fn register(&mut self) -> Option<u8> {
        if self.registers >= REGISTERS {
            return None;
        }
        let register = self.registers as u8;
        self.registers += 1;
        Some(register)
    }

    fn scalar(&mut self, name: &str) -> u8 {
        if let Some(index) = self.plan.scalar_names.iter().position(|n| n == name) {
            return index as u8;
        }
        let index = self.plan.scalar_names.len();
        self.plan.scalar_names.push(name.to_owned());
        self.plan.scalar_slots.push(Cell::new(0));
        self.current_values.push(None);
        index as u8
    }

    fn array(&mut self, name: &str) -> u8 {
        if let Some(index) = self.plan.arrays.iter().position(|(n, _)| n == name) {
            return index as u8;
        }
        let index = self.plan.arrays.len();
        self.plan
            .arrays
            .push((name.to_owned(), CachedArraySlot::default()));
        index as u8
    }

    fn expression(&mut self, expression: &Expr) -> Option<u8> {
        let instruction = match expression {
            Expr::Number(value) => Instruction::Literal(self.register()?, *value),
            Expr::Var(name) => {
                if is_pi_constant_name(name)
                    || name == "INF"
                    || is_zero_arg_function(name)
                    || compile_fast_zero_arg_function(name).is_some()
                    || name.starts_with("FN")
                    || name.ends_with('$')
                {
                    return None;
                }
                let scalar = self.scalar(name);
                if let Some(reg) = self.current_values[scalar as usize] {
                    return Some(reg);
                }
                let out = self.register()?;
                self.current_values[scalar as usize] = Some(out);
                Instruction::Load(out, scalar)
            }
            Expr::Binary { op, left, right } => {
                let left = self.expression(left)?;
                let right = self.expression(right)?;
                Instruction::Binary(self.register()?, *op, left, right)
            }
            Expr::Unary { op, expr } => {
                let operand = self.expression(expr)?;
                Instruction::Unary(self.register()?, *op, operand)
            }
            Expr::ArrayOrCall { args, name, kind } if args.len() == 1 => {
                let operand = self.expression(&args[0])?;
                if *kind == ArrayOrCallKind::Array && !name.ends_with('$') {
                    Instruction::Read(self.register()?, self.array(name), operand)
                } else if *kind == ArrayOrCallKind::BuiltinFunction {
                    let function = FastNumberFunction::from_name(name)?;
                    let out = self.register()?;
                    if matches!(&args[0], Expr::Var(_))
                        && matches!(function, FastNumberFunction::Sin | FastNumberFunction::Cos
                            | FastNumberFunction::Tan | FastNumberFunction::Atn)
                    {
                        // Each call has a destination register, so its site index also fits in u8.
                        let site = self.plan.trig_sites.len() as u8;
                        self.plan.trig_sites.push(TrigSite { function, last: Cell::default() });
                        Instruction::CachedTrig(out, site, operand)
                    } else {
                        Instruction::Function(out, function, operand)
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        };
        // Each successful arm has just allocated its destination register.
        let out = (self.registers - 1) as u8;
        self.plan.code.push(instruction);
        Some(out)
    }

    fn assignment(&mut self, assignment: &CompiledNumericAssignment) -> Option<()> {
        let source = assignment.fallback();
        let value = self.expression(&source.rhs)?;
        let store = match &source.targets[0] {
            CompiledLValue::Scalar {
                name,
                is_string: false,
                ..
            } => {
                let scalar = self.scalar(name);
                self.current_values[scalar as usize] = Some(value);
                Instruction::Store(scalar, value)
            }
            CompiledLValue::Array {
                name,
                is_string: false,
                indexes,
                ..
            } if indexes.len() == 1 => {
                let index = self.expression(&indexes[0].expr)?;
                Instruction::Write(self.array(name), index, value)
            }
            _ => return None,
        };
        self.plan.code.push(store);
        self.plan.statements += 1;
        Some(())
    }
}

// The inline IF executor owns its branch commands. Leave them as ordinary
// statements, including exceptional entries via RESUME in the middle of a branch.
fn next_unconditional(commands: &[Rc<CachedCommand>], mut index: usize) -> usize {
    while let Some(CachedCommand::InlineIfGuard { end_cmd_idx, .. }) =
        commands.get(index).map(Rc::as_ref)
    {
        index = *end_cmd_idx;
    }
    index
}

// Store same-line alternatives as well as spans, so jumps into an interior
// numbered line can still use its own group without skipping an instruction.
pub(super) fn prepare(lines: &mut [Rc<[Rc<CachedCommand>]>], numbers: &[i32]) -> HashSet<usize> {
    let mut catalog: HashMap<usize, Vec<Option<Box<Plan>>>> = HashMap::new();
    for crosses_lines in [false, true] {
        let mut start = Cursor { line_idx: 0, cmd_idx: 0 };
        while start.line_idx < lines.len() {
            start.cmd_idx = next_unconditional(&lines[start.line_idx], start.cmd_idx);
            if start.cmd_idx >= lines[start.line_idx].len() {
                start.line_idx += 1;
                start.cmd_idx = 0;
                continue;
            }
            let mut at = start;
            let mut builder = Builder::default();
            let mut positions = Vec::new();
            while positions.len() < MAX_STATEMENTS && at.line_idx < lines.len() {
                let Some(CachedCommand::NumericAssignment(assignment)) =
                    lines[at.line_idx].get(at.cmd_idx).map(Rc::as_ref)
                else {
                    break;
                };
                let mut next = builder.clone();
                if at.line_idx != start.line_idx && at.cmd_idx == 0 {
                    next.plan.code.push(Instruction::EnterLine(numbers[at.line_idx]));
                }
                if next.assignment(assignment).is_none() {
                    break;
                }
                builder = next;
                positions.push(at);
                at.cmd_idx += 1;
                if at.cmd_idx == lines[at.line_idx].len() {
                    if !crosses_lines {
                        break;
                    }
                    at.line_idx += 1;
                    at.cmd_idx = 0;
                }
            }
            let Some(last) = positions.last().copied() else {
                start.cmd_idx += 1;
                continue;
            };
            if positions.len() >= 2 && crosses_lines == (last.line_idx != start.line_idx) {
                if crosses_lines {
                    positions.push(Cursor { cmd_idx: last.cmd_idx + 1, ..last });
                    builder.plan.positions = positions;
                }
                let line = catalog.entry(start.line_idx).or_insert_with(|| {
                    std::iter::repeat_with(|| None).take(lines[start.line_idx].len()).collect()
                });
                line[start.cmd_idx] = Some(Box::new(builder.plan));
            }
            start = at;
        }
    }
    let mut tagged = HashSet::with_capacity(catalog.len());
    for (idx, plans) in catalog {
        let commands = Rc::make_mut(&mut lines[idx]);
        for (command, plan) in commands.iter_mut().zip(plans) {
            if let Some(plan) = plan {
                let CachedCommand::NumericAssignment(first) = command.as_ref() else {
                    unreachable!("a prepared block starts on its original numeric assignment");
                };
                *command = Rc::new(CachedCommand::NumericBlock { first: first.clone(), plan: Rc::from(plan) });
            }
        }
        tagged.insert(idx);
    }
    tagged
}

#[cfg(test)]
mod trig_tests {
    use super::*;

    #[test]
    fn cached_trig_matches_function_across_input_and_mode_changes() {
        for function in [FastNumberFunction::Sin, FastNumberFunction::Cos,
                         FastNumberFunction::Tan, FastNumberFunction::Atn] {
            let site = TrigSite { function, last: Cell::default() };
            for input in [0.25, 0.75, -0.25, 0.0, -0.0, 0.0,
                          f64::from_bits(1), f64::MAX, f64::NEG_INFINITY,
                          f64::from_bits(0x7ff8_0000_0000_0017), -8.75] {
                for degrees in [false, false, true, true, false] {
                    let reference = eval_fast_number_function(function, input, degrees)
                        .map(f64::to_bits).map_err(|error| error.code);
                    assert_eq!(site.evaluate(input, degrees).map(f64::to_bits)
                        .map_err(|error| error.code), reference);
                }
            }
        }
    }

    #[test]
    fn direct_trig_call_is_prepared_across_a_line_boundary() {
        let mut interpreter = Interpreter::new();
        interpreter.program.load_text(
            "10 DIM k(1),e(1)\n230 FOR i=1 TO 1\n240 k=k(i):e=e(i)\n\
             250 d=(k*k+e*e)/99+SIN(t)/6+0.5\n260 NEXT i"
        ).unwrap();
        interpreter.rebuild_command_cache();
        let line = interpreter.line_index_cache[&240];
        let CachedCommand::NumericBlock { plan, .. } = interpreter.compiled_line_cache[line][0].as_ref() else {
            panic!("expected the three assignments on lines 240 and 250 to form a plan")
        };
        assert_eq!(plan.statements, 3);
        let mut entered_250 = false;
        let mut sin_t = false;
        for &op in &plan.code {
            match op {
                Instruction::EnterLine(number) => entered_250 = number == 250,
                Instruction::CachedTrig(_, site, _) if entered_250 => {
                    sin_t |= matches!(plan.trig_sites[site as usize].function, FastNumberFunction::Sin);
                }
                _ => {}
            }
        }
        assert!(sin_t);
    }
}

// Separate runner: adding dedicated block dispatch here leaves the generated
// ordinary RUN/PROFILE loop completely independent of planning state.
impl Interpreter {
    #[inline(never)]
    #[cfg_attr(all(windows, target_arch = "x86_64", target_env = "msvc"), unsafe(link_section = ".avlprep"))]
    pub(super) fn run_from_inner_prepared(&mut self, mut cursor: Cursor) -> BasicResult<RunOutcome> {
        use super::profile::Observer as _;
        let mut observer = ();
        let mut lines = self.line_numbers_cache.clone();
        let mut compiled_lines = self.compiled_line_cache.clone();
        let mut refresh_compiled_lines = false;
        let mut runtime_poll_skip = 0u8;
        // The builder emits every register definition before its first use.
        // Nested invocations have their own workspace.
        let mut values = [0.0; REGISTERS];
        'run_loop: loop {
            // A MERGE can replace the catalog while the current line still
            // borrows its original commands. Refresh after that borrow ends.
            if refresh_compiled_lines {
                compiled_lines = self.compiled_line_cache.clone();
                refresh_compiled_lines = false;
            }
            let returning = self.return_line_plan.take();
            if cursor.line_idx >= lines.len() && returning.is_none() {
                break;
            }
            let line_no = returning
                .as_ref()
                .map_or_else(|| lines[cursor.line_idx], |plan| plan.line);
            let line_commands = returning.as_ref().map_or_else(
                || compiled_lines[cursor.line_idx].as_ref(),
                |plan| plan.commands.as_ref(),
            );
            let commands_len = line_commands.len();
            let mut merged_line = returning.is_some();
            self.active_line_plan = returning.clone();
            if cursor.cmd_idx >= commands_len {
                cursor.line_idx = if merged_line {
                    lines.partition_point(|number| *number <= line_no)
                } else {
                    cursor.line_idx + 1
                };
                cursor.cmd_idx = 0;
                continue;
            }
            self.current_line = Some(line_no);
            if self.trace {
                self.write_trace_line(line_no);
            }
            let mut entering_line = true;
            while cursor.cmd_idx < commands_len {
                let poll_now = runtime_poll_skip == 0
                    || self.test_interrupt_requested
                    || !self.timers.is_empty()
                    || self.should_poll_sound_events()
                    || console::interrupt_requested();
                if poll_now {
                    if self.poll_interrupts_and_timers(&mut cursor)? {
                        // Polling may run a mouse handler before dispatching
                        // a timer, so its MERGE must reach the next line too.
                        refresh_compiled_lines = true;
                        continue 'run_loop;
                    }
                    runtime_poll_skip = RUNTIME_POLL_COMMAND_SKIP;
                } else {
                    runtime_poll_skip -= 1;
                }
                if self.end_requested {
                    self.present_dirty_graphics_at_run_boundary()?;
                    return Ok(RunOutcome::End);
                }
                if self.stopped_cursor.is_some() {
                    self.present_dirty_graphics_at_run_boundary()?;
                    return Ok(RunOutcome::Stop);
                }
                if entering_line {
                    observer.visit(cursor.line_idx, self.active_line_plan.as_deref());
                    entering_line = false;
                }
                let command = line_commands[cursor.cmd_idx].as_ref();
                let inline_if = matches!(command, CachedCommand::InlineIfGuard { .. });
                let mut before = cursor;
                let mut next = Cursor {
                    line_idx: cursor.line_idx,
                    cmd_idx: cursor.cmd_idx + 1,
                };
                let block = match command {
                    CachedCommand::NumericBlock { plan, .. }
                        if self.function_call_stack.is_empty()
                            && self.timers.is_empty()
                            && !self.has_sound_handlers
                            && runtime_poll_skip >= plan.statements - 1
                            && !merged_line
                            && (plan.positions.is_empty() || !self.trace)
                            && Rc::ptr_eq(&lines, &self.line_numbers_cache)
                            && self.debugger_state.is_none() => Some(plan),
                    _ => None,
                };
                let mut failure = if let Some(plan) = block {
                    if !plan.positions.is_empty() {
                        // The bytecode increments this index on each committed
                        // assignment. Map it back to the exact BASIC cursor.
                        let mut progress = Cursor { line_idx: 0, cmd_idx: 0 };
                        let result = plan.execute(self, &mut progress, &mut values);
                        cursor = plan.positions[progress.cmd_idx];
                        match result {
                            Ok(()) => {
                                runtime_poll_skip -= (progress.cmd_idx - 1) as u8;
                            }
                            Err(error) => {
                                runtime_poll_skip -= progress.cmd_idx as u8;
                                let retry = cursor;
                                let next = Cursor { cmd_idx: retry.cmd_idx + 1, ..retry };
                                let error = self.with_numbered_statement(error, lines[retry.line_idx], retry.cmd_idx);
                                if error.code == ErrorCode::KeyboardInterrupt {
                                    return Err(error);
                                }
                                self.handle_runtime_error(error, &mut cursor, retry, next)?;
                            }
                        }
                        continue 'run_loop;
                    }
                    match plan.execute(self, &mut cursor, &mut values) {
                        Ok(()) => {
                            runtime_poll_skip -= (cursor.cmd_idx - before.cmd_idx - 1) as u8;
                            continue;
                        }
                        Err(error) => {
                            runtime_poll_skip -= (cursor.cmd_idx - before.cmd_idx) as u8;
                            Some(CachedExecutionFailure {
                                error,
                                retry: cursor,
                                next: Cursor { cmd_idx: cursor.cmd_idx + 1, ..cursor },
                            })
                        }
                    }
                } else if inline_if {
                    self.execute_cached_inline_if_fast(line_commands, &mut cursor, &observer)?
                } else {
                    observer
                        .execute(self, command, &mut cursor)
                        .err()
                        .map(|error| CachedExecutionFailure {
                            error,
                            retry: before,
                            next,
                        })
                };
                if !Rc::ptr_eq(&lines, &self.line_numbers_cache) && !self.restart_run_loop {
                    merged_line = true;
                    before.line_idx = self.merge_line_index(line_no);
                    next.line_idx = before.line_idx;
                    if let Some(failure) = failure.as_mut() {
                        failure.retry.line_idx = before.line_idx;
                        failure.next.line_idx = before.line_idx;
                    }
                    lines = self.line_numbers_cache.clone();
                    observer.set_catalog(self);
                    refresh_compiled_lines = true;
                }
                if let Some(failure) = failure {
                    let CachedExecutionFailure {
                        error: err,
                        retry,
                        next: error_next,
                    } = failure;
                    let err = self.with_numbered_statement(err, line_no, retry.cmd_idx);
                    if err.code == ErrorCode::KeyboardInterrupt {
                        return Err(err);
                    }
                    self.handle_runtime_error(err, &mut cursor, retry, error_next)?;
                    if cursor.line_idx != before.line_idx || self.return_line_plan.is_some() {
                        continue 'run_loop;
                    }
                    continue;
                }
                if self.end_requested {
                    self.present_dirty_graphics_at_run_boundary()?;
                    return Ok(RunOutcome::End);
                }
                if self.stopped_cursor.is_some() {
                    self.present_dirty_graphics_at_run_boundary()?;
                    return Ok(RunOutcome::Stop);
                }
                if self.function_return_requested {
                    return Ok(RunOutcome::End);
                }
                if self.sub_return_requested {
                    return Ok(RunOutcome::End);
                }
                if self.restart_run_loop {
                    self.restart_run_loop = false;
                    lines = self.line_numbers_cache.clone();
                    observer.set_catalog(self);
                    refresh_compiled_lines = true;
                    continue 'run_loop;
                }
                if self.return_line_plan.is_some() {
                    continue 'run_loop;
                }
                if self.repeat_current_command {
                    self.repeat_current_command = false;
                    self.idle_wait_for_graphics_event_loop(&cursor)?;
                    continue;
                }
                if inline_if {
                    if cursor.line_idx != before.line_idx
                        || (merged_line && cursor.cmd_idx <= before.cmd_idx)
                    {
                        continue 'run_loop;
                    }
                    entering_line = cursor.cmd_idx <= before.cmd_idx;
                    continue;
                }
                if cursor != before {
                    if cursor.line_idx != before.line_idx || merged_line {
                        continue 'run_loop;
                    }
                    entering_line = cursor.cmd_idx <= before.cmd_idx;
                    continue;
                }
                cursor = next;
            }
            cursor.line_idx = if merged_line {
                lines.partition_point(|number| *number <= line_no)
            } else {
                cursor.line_idx + 1
            };
            cursor.cmd_idx = 0;
        }
        if let Some(frame) = self
            .if_stack
            .last()
            .filter(|_| cursor.line_idx != usize::MAX)
        {
            let line = lines
                .get(frame.line_idx)
                .copied()
                .or_else(|| self.line_numbers_cache.get(frame.line_idx).copied())
                .unwrap_or_default();
            return Err(BasicError::new(ErrorCode::IfWithoutEndIf).at_line(line));
        }
        self.present_dirty_graphics_at_run_boundary()?;
        if cursor.line_idx != usize::MAX {
            self.finish_output_line();
        }
        Ok(RunOutcome::End)
    }

}
