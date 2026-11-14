//! Bounded reads of live array storage during a debugger pause.
use super::*;
use crate::debugger::{
    DebugArrayPin, DebugArrayScope, DebugArrayWindow, DebugArrayWindowRequest,
    MAX_DEBUG_ARRAY_PREVIEW_WIDTH, MAX_DEBUG_ARRAY_WINDOW_CELLS,
};
use std::ops::Range;

static NEXT_BINDING_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(super) enum DebugArrayChange {
    Cell(usize),
    Cells(HashSet<usize>),
    Rectangle {
        rows: Range<usize>,
        columns: Range<usize>,
    },
    BaseOne,
    All,
}

impl DebugArrayChange {
    fn contains(&self, array: &ArrayValue, flat: usize, row: usize, column: usize) -> bool {
        match self {
            Self::Cell(cell) => *cell == flat,
            Self::Cells(cells) => cells.contains(&flat),
            Self::Rectangle { rows, columns } => rows.contains(&row) && columns.contains(&column),
            Self::BaseOne => {
                let mut offset = flat;
                array.dims.iter().rev().all(|bound| {
                    let extent = bound + 1;
                    let active = offset % extent != 0;
                    offset /= extent;
                    active
                })
            }
            Self::All => true,
        }
    }

    fn nearest_to_origin(&self, array: &ArrayValue) -> Option<Vec<i32>> {
        let indexes = match self {
            Self::Cell(flat) => array.indexes_for_flat(*flat),
            Self::Cells(cells) => {
                // Row-major offsets sort in the same order as the coordinates.
                let flat = cells
                    .iter()
                    .copied()
                    .min_by_key(|&flat| (squared_distance_to_origin(flat, &array.dims), flat))?;
                array.indexes_for_flat(flat)
            }
            Self::Rectangle { rows, columns } => {
                let mut indexes = vec![0; array.dims.len()];
                *indexes.first_mut()? = rows.start as i32;
                if let Some(index) = indexes.get_mut(1) {
                    *index = columns.start as i32;
                }
                indexes
            }
            Self::BaseOne => vec![1; array.dims.len()],
            Self::All => vec![0; array.dims.len()],
        };
        Some(indexes)
    }
}

fn squared_distance_to_origin(mut flat: usize, dimensions: &[usize]) -> u128 {
    let mut distance = 0;
    for &bound in dimensions.iter().rev() {
        let span = bound + 1;
        let index = (flat % span) as u128;
        distance += index * index;
        flat /= span;
    }
    distance
}

#[derive(Debug)]
pub(super) struct DebugArrayWrites {
    stamp: u64,
    change: DebugArrayChange,
}

impl ArrayValue {
    pub(super) fn remember_debug_array_change(&mut self, stamp: u64, change: DebugArrayChange) {
        if stamp == 0 {
            return;
        }
        let Some(writes) = self.debug_writes.as_mut() else {
            self.debug_writes = Some(Box::new(DebugArrayWrites { stamp, change }));
            return;
        };
        if writes.stamp == stamp {
            if let DebugArrayChange::Cell(cell) = change {
                match &mut writes.change {
                    DebugArrayChange::Cell(previous) if *previous != cell => {
                        writes.change = DebugArrayChange::Cells(HashSet::from([*previous, cell]));
                    }
                    DebugArrayChange::Cells(cells) => {
                        cells.insert(cell);
                    }
                    _ => {}
                }
                return;
            }
        }
        **writes = DebugArrayWrites { stamp, change };
    }
}

impl Interpreter {
    #[inline(always)]
    pub(super) fn debug_array_write_stamp(&self) -> Option<u64> {
        let state = self.debugger_state.as_ref()?;
        state.debugger.as_ref()?;
        Some(state.active_array_command)
    }

    pub(super) fn with_debug_array_command<T>(
        &mut self,
        boundary: bool,
        execute: impl FnOnce(&mut Self) -> T,
    ) -> T {
        if !boundary {
            return execute(self);
        }
        let Some(state) = self
            .debugger_state
            .as_mut()
            .filter(|state| state.debugger.is_some())
        else {
            return execute(self);
        };
        state.next_array_command += 1;
        let stamp = state.next_array_command;
        let previous = std::mem::replace(&mut state.active_array_command, stamp);
        let result = execute(self);
        if let Some(state) = self.debugger_state.as_mut() {
            state.last_array_command = stamp;
            state.active_array_command = previous;
        }
        result
    }

    pub(super) fn debug_array_window(
        &self,
        name: &str,
        request: &DebugArrayWindowRequest,
    ) -> Result<DebugArrayWindow, String> {
        let array = self.debug_existing_array(&name.to_ascii_uppercase())?;
        let Some((&row_bound, rest)) = array.dims.split_first() else {
            return Err("This array has no axes.".into());
        };
        let count = request
            .row_count
            .checked_mul(request.col_count)
            .filter(|&count| (1..=MAX_DEBUG_ARRAY_WINDOW_CELLS).contains(&count))
            .ok_or("The requested array window is too large or empty.")?;
        validate_window_axis(request.first_row, request.row_count, row_bound)?;
        validate_window_axis(
            request.first_col,
            request.col_count,
            rest.first().copied().unwrap_or(0),
        )?;
        if request.higher_indexes.len() != array.dims.len().saturating_sub(2) {
            return Err("The requested array slice has the wrong number of fixed axes.".into());
        }
        let mut indexes = Vec::with_capacity(array.dims.len());
        indexes.push(request.first_row);
        if !rest.is_empty() {
            indexes.push(request.first_col);
        }
        indexes.extend_from_slice(&request.higher_indexes);
        // Validate the complete coordinate once, including fixed higher axes.
        let start = array.flat_index(&indexes).map_err(array_index_error)?;
        let col_stride = rest
            .iter()
            .skip(1)
            .map(|bound| bound + 1)
            .product::<usize>();
        let row_stride = (rest.first().copied().unwrap_or(0) + 1) * col_stride;

        let width = request
            .preview_width
            .clamp(1, MAX_DEBUG_ARRAY_PREVIEW_WIDTH);
        let mut cells = Vec::with_capacity(count);
        let recent = self.debug_recent_array_change(array);
        let mut changed = Vec::with_capacity(count);
        for row in 0..request.row_count {
            for col in 0..request.col_count {
                let flat = start + row * row_stride + col * col_stride;
                cells.push(cell_preview(array, flat, width));
                changed.push(recent.is_some_and(|change| {
                    change.contains(
                        array,
                        flat,
                        request.first_row as usize + row,
                        request.first_col as usize + col,
                    )
                }));
            }
        }
        Ok(DebugArrayWindow {
            cells,
            mat_base: self.mat_base,
            changed,
        })
    }

    fn debug_recent_array_change<'a>(&self, array: &'a ArrayValue) -> Option<&'a DebugArrayChange> {
        let state = self.debugger_state.as_ref()?;
        let writes = array.debug_writes.as_ref()?;
        (writes.stamp == state.last_array_command).then_some(&writes.change)
    }

    pub(super) fn debug_last_array_write(&self, name: &str) -> Option<Vec<i32>> {
        let array = self.debug_existing_array(&name.to_ascii_uppercase()).ok()?;
        self.debug_recent_array_change(array)?
            .nearest_to_origin(array)
    }

    pub(super) fn debug_array_cell(
        &self,
        name: &str,
        indexes: &[i32],
    ) -> Result<DebugValue, String> {
        match self
            .debug_existing_array(&name.to_ascii_uppercase())?
            .get(indexes)
            .map_err(array_index_error)?
        {
            Value::Number(value) => Ok(DebugValue::Number(value)),
            Value::Str(value) => Ok(DebugValue::String(value)),
            Value::ArrayRef(_) => unreachable!("Arrays store scalar cells"),
        }
    }

    pub(super) fn debug_pin_array_cell(
        &self,
        name: &str,
        indexes: &[i32],
    ) -> Result<DebugArrayPin, String> {
        let key = name.to_ascii_uppercase();
        let array = self.debug_existing_array(&key)?;
        array.flat_index(indexes).map_err(array_index_error)?;
        let mut binding = array.debug_binding_id.get();
        if binding == 0 {
            binding = NEXT_BINDING_ID.fetch_add(1, Ordering::Relaxed);
            array.debug_binding_id.set(binding);
        }
        Ok(DebugArrayPin {
            name: self.debug_identifier_name(&key),
            indexes: indexes.to_vec(),
            binding,
            scope: self.debug_array_scope(&key),
        })
    }

    pub(super) fn debug_pinned_array_cell(
        &self,
        pin: &DebugArrayPin,
        preview_width: usize,
    ) -> Option<String> {
        let key = pin.name.to_ascii_uppercase();
        let array = self.debug_existing_array(&key).ok()?;
        if array.debug_binding_id.get() != pin.binding || self.debug_array_scope(&key) != pin.scope
        {
            return None;
        }
        let flat = array.flat_index(&pin.indexes).ok()?;
        Some(cell_preview(
            array,
            flat,
            preview_width.clamp(1, MAX_DEBUG_ARRAY_PREVIEW_WIDTH),
        ))
    }

    fn debug_existing_array(&self, name: &str) -> Result<&ArrayValue, String> {
        if !name.contains('\0') {
            if let Some(array) = self.array_ref(name) {
                return Ok(array);
            }
        }
        Err("This array is not available in the current scope.".into())
    }

    fn debug_array_scope(&self, key: &str) -> DebugArrayScope {
        let mut scope = DebugArrayScope {
            module: self
                .active_module
                .as_ref()
                .map(|_| self.sub_runtime.sub_catalog_id),
            ..DebugArrayScope::default()
        };
        if self.array_aliases.contains_key(key) || self.active_function_name() == Some(key) {
            // A reference parameter belongs to the invocation where it is
            // visible. Equal routine names do not identify the same call.
            scope.sub = self.active_subs.last().map(|frame| frame.name.clone());
            scope.function = self.active_functions.last().map(|frame| frame.name.clone());
        }
        scope
    }

    pub(super) fn debug_call_identity(&self, name: &Rc<str>) -> Rc<str> {
        if self.debugger().is_some() {
            Rc::from(name.as_ref())
        } else {
            name.clone()
        }
    }
}

fn validate_window_axis(first: i32, count: usize, bound: usize) -> Result<(), String> {
    i32::try_from(count - 1)
        .ok()
        .and_then(|offset| first.checked_add(offset))
        .filter(|&last| first >= 0 && last as usize <= bound)
        .map(|_| ())
        .ok_or_else(|| "The requested array window is outside the stored bounds.".into())
}

fn array_index_error(error: BasicError) -> String {
    error.display_for_basic()
}

fn cell_preview(array: &ArrayValue, flat: usize, width: usize) -> String {
    match &array.data {
        ArrayData::Number(values) => {
            let full = format_basic_number(values[flat]);
            let full = full.trim_start();
            if full.len() <= width {
                full.into()
            } else {
                format!("{}…", &full[..width - 1])
            }
        }
        ArrayData::Str(values) => preview_string(&values[flat], width),
    }
}

fn preview_string(text: &str, width: usize) -> String {
    if width < 3 {
        return if text.is_empty() && width == 2 {
            "\"\"".into()
        } else {
            "…".into()
        };
    }
    let mut output = String::with_capacity(width);
    output.push('"');
    let mut remaining = width - 2;
    let mut text = text.chars().peekable();
    while let Some(ch) = text.next() {
        // Keep ordinary Western characters in one terminal column.
        let plain = matches!(ch, ' '..='!' | '#'..='[' | ']'..='~' | '\u{a0}'..='\u{ac}' | '\u{ae}'..='\u{24f}');
        let escaped = ch.escape_default();
        let columns = if plain { 1 } else { escaped.len() };
        if columns + usize::from(text.peek().is_some()) > remaining {
            output.push('…');
            break;
        }
        if plain {
            output.push(ch);
        } else {
            output.extend(escaped);
        }
        remaining -= columns;
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_reads_a_bounded_slice_with_explicit_zero_based_indexes() {
        let mut interpreter = Interpreter::new();
        interpreter.mat_base = 1;
        let mut array = ArrayValue::new("A", vec![2, 3, 1]);
        for (indexes, value) in [
            ([0, 0, 1], 1.0),
            ([0, 1, 1], 2.0),
            ([1, 0, 1], 11.0),
            ([1, 1, 1], 12.0),
        ] {
            array.set(&indexes, Value::number(value)).unwrap();
        }
        interpreter.arrays.insert("A".to_string(), array);
        let mut request = DebugArrayWindowRequest {
            first_row: 0,
            first_col: 0,
            row_count: 2,
            col_count: 2,
            higher_indexes: vec![1],
            preview_width: 10,
        };
        assert_eq!(
            interpreter.debug_array_window("a", &request).unwrap().cells,
            ["1", "2", "11", "12"]
        );
        assert!(interpreter.debug_array_window("missing", &request).is_err());
        assert!(!interpreter.arrays.contains_key("MISSING"));
        request.higher_indexes[0] = 2;
        assert!(interpreter.debug_array_window("a", &request).is_err());
        request.higher_indexes[0] = 1;
        request.row_count = usize::MAX;
        assert!(interpreter.debug_array_window("a", &request).is_err());
    }

    #[test]
    fn window_highlights_only_writes_from_the_previous_statement() {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(
                "10 DIM A(2,2),B(2,2)\n\
                 20 MAT BASE 1\n\
                 30 IF 1 THEN A(2,1)=7 : X=1\n\
                 40 MAT READ A\n\
                 50 MAT B(1:2,2)=A(1:2,1)\n\
                 60 MAT A=CON\n\
                 70 A(2,2)=FNValue(5)\n\
                 80 END\n\
                 90 DATA 1,2,3,4\n\
                 100 DEF FNValue(N)\n\
                 110 A(1,1)=N\n\
                 120 FNValue=N\n\
                 130 FNEND",
            )
            .unwrap();
        let mut debugger = Debugger::interactive_editable(|snapshot, _, access| {
            let request = DebugArrayWindowRequest {
                first_row: 0,
                first_col: 0,
                row_count: 3,
                col_count: 3,
                higher_indexes: vec![],
                preview_width: 10,
            };
            let location = &snapshot.location;
            let (a_changed, b_changed): (&[usize], &[usize]) = match location.line {
                30 if location.command.trim() == "X=1" => (&[7], &[]),
                50 => (&[4, 5, 7, 8], &[]),
                60 => (&[], &[5, 8]),
                70 | 110 => (&[0, 1, 2, 3, 4, 5, 6, 7, 8], &[]),
                80 => (&[8], &[]),
                120 => (&[4], &[]),
                _ => (&[], &[]),
            };
            for (name, expected) in [("A", a_changed), ("B", b_changed)] {
                let window = access.array_window(name, &request).unwrap();
                assert_eq!(window.mat_base, 1);
                assert_eq!(
                    window.changed,
                    (0..9)
                        .map(|flat| expected.contains(&flat))
                        .collect::<Vec<_>>(),
                    "{name} at {} {}",
                    location.line,
                    location.command,
                );
                assert_eq!(
                    access.last_array_write(name),
                    expected
                        .first()
                        .map(|&flat| vec![(flat / 3) as i32, (flat % 3) as i32]),
                    "{name} at {} {}",
                    location.line,
                    location.command,
                );
            }
            Ok(DebugAction::StepInto)
        });
        debugger.set_breakpoint(30, true);
        interpreter.set_debugger(debugger);
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    }

    #[test]
    fn nearest_array_write_considers_all_dimensions_of_an_alias() {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(
                "10 DIM A(3,3,6)\n\
                 20 CALL UPDATE(A)\n\
                 30 END\n\
                 100 DEF SUB UPDATE(P)\n\
                 110 READ P(0,0,4),P(2,1,2),P(1,2,2),P(0,3,1)\n\
                 120 X=1\n\
                 130 SUBEND\n\
                 140 DATA 1,2,3,4",
            )
            .unwrap();
        let mut debugger = Debugger::interactive_editable(|snapshot, _, access| {
            if snapshot.location.line == 120 {
                let visible = DebugArrayWindowRequest {
                    first_row: 0,
                    first_col: 0,
                    row_count: 4,
                    col_count: 4,
                    higher_indexes: vec![0],
                    preview_width: 10,
                };
                let window = access.array_window("p", &visible).unwrap();
                assert!(window.changed.iter().all(|&changed| !changed));
                assert_eq!(access.last_array_write("p"), Some(vec![1, 2, 2]));
                assert_eq!(access.last_array_write("A"), Some(vec![1, 2, 2]));
            } else {
                assert_eq!(access.last_array_write("P"), None);
            }
            Ok(DebugAction::StepInto)
        });
        debugger.set_breakpoint(110, true);
        interpreter.set_debugger(debugger);
        assert_eq!(interpreter.run_loaded().unwrap(), RunOutcome::End);
    }

    #[test]
    fn fixed_pin_keeps_its_binding_through_mat_copy_and_redim() {
        let mut interpreter = Interpreter::new();
        interpreter
            .process_immediate("DIM A(4,4),B(4,4) : B(2,3)=19")
            .unwrap();
        let pin_a = interpreter.debug_pin_array_cell("A", &[2, 3]).unwrap();
        let pin_b = interpreter.debug_pin_array_cell("B", &[2, 3]).unwrap();
        interpreter.process_immediate("MAT A=B").unwrap();
        interpreter.process_immediate("B(2,3)=91").unwrap();
        assert_eq!(
            interpreter.debug_pinned_array_cell(&pin_a, 8).as_deref(),
            Some("19")
        );
        assert_eq!(
            interpreter.debug_pinned_array_cell(&pin_b, 8).as_deref(),
            Some("91")
        );
        interpreter.process_immediate("REDIM A(1,4)").unwrap();
        assert_eq!(interpreter.debug_pinned_array_cell(&pin_a, 8), None);
        interpreter.process_immediate("REDIM A(4,4)").unwrap();
        assert_eq!(
            interpreter.debug_pinned_array_cell(&pin_a, 8).as_deref(),
            Some("0")
        );
        interpreter.process_immediate("CLEAR : DIM A(4,4)").unwrap();
        assert_eq!(interpreter.debug_pinned_array_cell(&pin_a, 8), None);
    }

    #[test]
    fn an_alias_pin_does_not_follow_another_call_of_the_same_subroutine() {
        let mut interpreter = Interpreter::new();
        interpreter.set_debugger(Debugger::scripted([]));
        interpreter
            .arrays
            .insert("A".to_string(), ArrayValue::new("A", vec![1]));
        interpreter.set_array_alias_binding("P", "A".to_string());
        let name = Rc::from("SUBROUTINE");
        let first = interpreter.debug_call_identity(&name);
        interpreter.active_subs.push(ActiveSubFrame { name: first });
        let pin = interpreter.debug_pin_array_cell("P", &[0]).unwrap();
        assert_eq!(
            interpreter.debug_pinned_array_cell(&pin, 8).as_deref(),
            Some("0")
        );
        interpreter.active_subs.pop();
        let second = interpreter.debug_call_identity(&name);
        interpreter
            .active_subs
            .push(ActiveSubFrame { name: second });
        assert_eq!(interpreter.debug_pinned_array_cell(&pin, 8), None);
    }

    #[test]
    fn string_preview_is_bounded_but_selected_cell_returns_the_full_value() {
        let mut interpreter = Interpreter::new();
        let mut array = ArrayValue::new("A$", vec![0]);
        let value = "mañana\n".repeat(100_000);
        array.set(&[0], Value::string(value.clone())).unwrap();
        interpreter.arrays.insert("A$".to_string(), array);
        let pin = interpreter.debug_pin_array_cell("A$", &[0]).unwrap();
        assert_eq!(
            interpreter.debug_pinned_array_cell(&pin, 8).as_deref(),
            Some("\"mañan…\"")
        );
        assert_eq!(
            interpreter.debug_array_cell("A$", &[0]).unwrap(),
            DebugValue::String(value)
        );
    }
}
