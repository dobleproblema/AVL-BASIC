//! Bounded array reads and exact change tracking between debugger pauses.
use super::*;
use crate::debugger::{
    DebugArrayPin, DebugArrayScope, DebugArrayWindow, DebugArrayWindowRequest,
    MAX_DEBUG_ARRAY_PREVIEW_WIDTH, MAX_DEBUG_ARRAY_WINDOW_CELLS,
};

static NEXT_BINDING_ID: AtomicU64 = AtomicU64::new(1);

// Record a small page only when an indexed write first touches it. Bulk
// operations retain the complete original and fold in any pages already saved.
const ARRAY_DEBUG_PAGE_CELLS: usize = 128;

#[derive(Debug)]
pub(super) enum DebugArrayBefore {
    Pages(HashMap<usize, ArrayData>),
    Whole { dims: Vec<usize>, data: ArrayData },
}

impl DebugArrayBefore {
    fn remember_cell(&mut self, data: &ArrayData, flat: usize) {
        if let Self::Pages(pages) = self {
            let start = flat / ARRAY_DEBUG_PAGE_CELLS * ARRAY_DEBUG_PAGE_CELLS;
            pages.entry(start).or_insert_with(|| match data {
                ArrayData::Number(values) => ArrayData::Number(
                    values[start..(start + ARRAY_DEBUG_PAGE_CELLS).min(values.len())].to_vec(),
                ),
                ArrayData::Str(values) => ArrayData::Str(
                    values[start..(start + ARRAY_DEBUG_PAGE_CELLS).min(values.len())].to_vec(),
                ),
            });
        }
    }

    fn remember_whole(&mut self, array: &ArrayValue) {
        let Self::Pages(pages) = self else {
            return;
        };
        let mut data = array.data.clone();
        for (start, page) in pages.drain() {
            match (&mut data, page) {
                (ArrayData::Number(values), ArrayData::Number(mut page)) => {
                    values[start..start + page.len()].swap_with_slice(&mut page);
                }
                (ArrayData::Str(values), ArrayData::Str(mut page)) => {
                    values[start..start + page.len()].swap_with_slice(&mut page);
                }
                _ => unreachable!("recorded array pages retain their element type"),
            }
        }
        *self = Self::Whole {
            dims: array.dims.clone(),
            data,
        };
    }

    fn differs(&self, array: &ArrayValue, flat: usize) -> bool {
        match self {
            Self::Pages(pages) => {
                let start = flat / ARRAY_DEBUG_PAGE_CELLS * ARRAY_DEBUG_PAGE_CELLS;
                pages
                    .get(&start)
                    .is_some_and(|page| array_cell_differs(page, flat - start, &array.data, flat))
            }
            Self::Whole { dims, data } => offset_in_previous_shape(flat, &array.dims, dims)
                .map_or_else(
                    || differs_from_default(&array.data, flat),
                    |old| array_cell_differs(data, old, &array.data, flat),
                ),
        }
    }

    fn nearest_to_origin(before: Option<&Self>, array: &ArrayValue) -> Option<Vec<i32>> {
        let mut nearest = None;
        let mut consider = |flat| {
            let key = (squared_distance_to_origin(flat, &array.dims), flat);
            nearest = Some(nearest.map_or(key, |current| key.min(current)));
        };
        match before {
            Some(Self::Pages(pages)) => {
                for (&start, data) in pages {
                    let len = match data {
                        ArrayData::Number(values) => values.len(),
                        ArrayData::Str(values) => values.len(),
                    };
                    for offset in 0..len {
                        let flat = start + offset;
                        if array_cell_differs(data, offset, &array.data, flat) {
                            consider(flat);
                        }
                    }
                }
            }
            _ => {
                for flat in 0..array.data_len() {
                    if cell_changed(before, array, flat) {
                        consider(flat);
                    }
                }
            }
        }
        nearest.map(|(_, flat)| array.indexes_for_flat(flat))
    }
}

fn cell_changed(before: Option<&DebugArrayBefore>, array: &ArrayValue, flat: usize) -> bool {
    match before {
        Some(before) => before.differs(array, flat),
        None => differs_from_default(&array.data, flat),
    }
}

fn differs_from_default(data: &ArrayData, flat: usize) -> bool {
    match data {
        ArrayData::Number(values) => values[flat] != 0.0,
        ArrayData::Str(values) => !values[flat].is_empty(),
    }
}

fn array_cell_differs(before: &ArrayData, old: usize, current: &ArrayData, new: usize) -> bool {
    match (before, current) {
        (ArrayData::Number(previous), ArrayData::Number(values)) => {
            let (before, after) = (previous[old], values[new]);
            before != after && before.to_bits() != after.to_bits()
        }
        (ArrayData::Str(previous), ArrayData::Str(values)) => previous[old] != values[new],
        _ => true,
    }
}

fn offset_in_previous_shape(
    mut flat: usize,
    current: &[usize],
    previous: &[usize],
) -> Option<usize> {
    if current == previous {
        return Some(flat);
    }
    if current.len() != previous.len() {
        return None;
    }
    let mut old = 0;
    let mut stride = 1;
    for (&bound, &old_bound) in current.iter().zip(previous).rev() {
        let span = bound + 1;
        let index = flat % span;
        if index > old_bound {
            return None;
        }
        old += index * stride;
        stride *= old_bound + 1;
        flat /= span;
    }
    Some(old)
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

impl ArrayValue {
    #[inline]
    pub(super) fn remember_debug_cell_before(&self, flat: usize) {
        if let Some(before) = self.debug_before.upgrade() {
            before.borrow_mut().remember_cell(&self.data, flat);
        }
    }

    #[inline]
    pub(super) fn remember_debug_array_before(&self) {
        if let Some(before) = self.debug_before.upgrade() {
            before.borrow_mut().remember_whole(self);
        }
    }
}

impl Interpreter {
    #[cold]
    pub(super) fn begin_debug_array_interval(&mut self) {
        let baselines = self
            .arrays
            .slots
            .iter_mut()
            .flatten()
            .map(|array| {
                let before = Rc::new(RefCell::new(DebugArrayBefore::Pages(HashMap::new())));
                array.debug_before = Rc::downgrade(&before);
                before
            })
            .collect();
        // The debugger owns the originals. Stopping DEBUG, restarting, or
        // starting another interval drops them even for arrays moved between
        // modules or temporarily hidden by a LOCAL binding.
        self.debugger_state.as_mut().unwrap().array_baselines = Some(baselines);
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
        let has_baseline = self
            .debugger_state
            .as_ref()
            .is_some_and(|state| state.array_baselines.is_some());
        let before = array.debug_before.upgrade();
        let before = before.as_ref().map(|before| before.borrow());
        let mut changed = Vec::with_capacity(count);
        for row in 0..request.row_count {
            for col in 0..request.col_count {
                let flat = start + row * row_stride + col * col_stride;
                cells.push(cell_preview(array, flat, width));
                changed.push(has_baseline && cell_changed(before.as_deref(), array, flat));
            }
        }
        Ok(DebugArrayWindow {
            cells,
            mat_base: self.mat_base,
            changed,
        })
    }

    pub(super) fn debug_nearest_array_change(&self, name: &str) -> Option<Vec<i32>> {
        self.debugger_state.as_ref()?.array_baselines.as_ref()?;
        let array = self.debug_existing_array(&name.to_ascii_uppercase()).ok()?;
        let before = array.debug_before.upgrade();
        let before = before.as_ref().map(|before| before.borrow());
        DebugArrayBefore::nearest_to_origin(before.as_deref(), array)
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
    fn stepping_compares_with_the_previous_visible_pause() {
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
                70 => (&[0, 1, 2, 3, 5, 6, 7, 8], &[]),
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
    fn step_out_tracks_net_changes_across_writes_and_bulk_operations() {
        let mut interpreter = Interpreter::new();
        interpreter
            .program
            .load_text(
                "10 DIM A(2,200),S$(2)\n\
                 20 A(0,2)=7 : A(1,0)=8 : S$(1)=\"old\"\n\
                 30 GOSUB 100\n\
                 40 END\n\
                 100 A(0,0)=9 : A(0,1)=0 : S$(1)=\"temporary\"\n\
                 110 A(0,0)=0 : A(0,2)=0 : S$(1)=\"old\"\n\
                 120 MAT A=MAP(INT,A)\n\
                 130 REDIM A(2,201)\n\
                 140 A(0,201)=3 : A(1,0)=8 : S$(2)=\"new\" : B(3)=6\n\
                 150 RETURN",
            )
            .unwrap();
        let mut debugger = Debugger::interactive_editable(|snapshot, _, access| {
            let window = |name, first_row, first_col, row_count, col_count| {
                access
                    .array_window(
                        name,
                        &DebugArrayWindowRequest {
                            first_row,
                            first_col,
                            row_count,
                            col_count,
                            higher_indexes: vec![],
                            preview_width: 10,
                        },
                    )
                    .unwrap()
            };
            match snapshot.location.line {
                100 => {
                    assert_eq!(window("A", 0, 0, 1, 3).changed, [false; 3]);
                    assert_eq!(access.last_array_write("A"), None);
                }
                150 => {
                    assert_eq!(window("A", 0, 0, 1, 3).changed, [false, false, true]);
                    assert_eq!(window("A", 0, 200, 1, 2).changed, [false, true]);
                    assert_eq!(window("A", 1, 0, 1, 1).changed, [false]);
                    assert_eq!(window("S$", 0, 0, 3, 1).changed, [false, false, true]);
                    assert_eq!(window("B", 0, 0, 4, 1).changed, [false, false, false, true]);
                    assert_eq!(access.last_array_write("A"), Some(vec![0, 2]));
                    assert_eq!(access.last_array_write("S$"), Some(vec![2]));
                    assert_eq!(access.last_array_write("B"), Some(vec![3]));
                }
                40 => {
                    assert_eq!(window("A", 0, 0, 1, 3).changed, [false; 3]);
                    assert_eq!(access.last_array_write("A"), None);
                    return Ok(DebugAction::Continue);
                }
                line => panic!("unexpected debugger pause at {line}"),
            }
            Ok(DebugAction::StepOut)
        });
        debugger.set_breakpoint(100, true);
        debugger.set_breakpoint(150, true);
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
