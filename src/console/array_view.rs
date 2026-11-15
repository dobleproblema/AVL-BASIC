use super::*;
use crate::debugger::{
    DebugArrayKind, DebugArraySummary, DebugArrayWindow, DebugArrayWindowRequest, DebugPauseAccess,
    DebugValue, MAX_DEBUG_ARRAY_PREVIEW_WIDTH, MAX_DEBUG_ARRAY_WINDOW_CELLS,
};

const GRID_CELL_WIDTH: usize = 12;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ArrayViewAction {
    Stay,
    Close,
    TogglePin(Vec<i32>),
    JumpToWrite,
    ToggleFollow,
}

#[derive(Debug, Default)]
struct CoordinateInput {
    text: String,
    cursor: usize,
}

impl CoordinateInput {
    fn insert(&mut self, text: &str, limit: usize) -> bool {
        if self.text.len().saturating_add(text.len()) > limit
            || !text
                .bytes()
                .all(|ch| matches!(ch, b'0'..=b'9' | b',' | b' ' | b'+' | b'-'))
        {
            return false;
        }
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        true
    }
}

#[derive(Debug)]
struct Detail {
    indexes: Vec<i32>,
    text: String,
    offset: usize,
}

impl Detail {
    fn page(&mut self, columns: usize, forward: bool) {
        let width = columns.max(1);
        if forward {
            if let Some((next, _)) = self.text[self.offset..].char_indices().nth(width) {
                self.offset += next;
            }
        } else {
            self.offset = self.text[..self.offset]
                .char_indices()
                .rev()
                .nth(width - 1)
                .map_or(0, |(start, _)| start);
        }
    }
}

#[derive(Debug)]
pub(super) struct ArrayView {
    summary: DebugArraySummary,
    selected: Vec<i32>,
    first_row: i32,
    first_col: i32,
    input: Option<CoordinateInput>,
    message: Option<String>,
    window: Option<(DebugArrayWindowRequest, Result<DebugArrayWindow, String>)>,
    detail: Option<Detail>,
}

#[derive(Clone, Copy)]
struct Layout {
    draw_grid: bool,
    show_ranges: bool,
    show_slice: bool,
    show_detail: bool,
    row_label_width: usize,
    cell_width: usize,
    row_count: usize,
    col_count: usize,
}

#[derive(Clone, Copy, Default)]
struct GridPalette {
    label: &'static str,
    inactive: &'static str,
    changed: &'static str,
}

impl GridPalette {
    fn for_theme(theme: SyntaxTheme, ansi: bool) -> Self {
        if !ansi {
            return Self::default();
        }
        Self {
            label: syntax_palette_for(theme).variable,
            inactive: match theme {
                SyntaxTheme::Dark => "\x1b[48;5;238m",
                SyntaxTheme::Light => "\x1b[48;5;250m",
            },
            changed: match theme {
                SyntaxTheme::Dark => debug_panel_changed_style(theme),
                SyntaxTheme::Light => "\x1b[1m\x1b[38;5;88m",
            },
        }
    }
}

struct Frame {
    content: Vec<String>,
    detail: Option<String>,
    status: String,
    input_cursor: Option<usize>,
}

impl ArrayView {
    pub(super) fn new(summary: DebugArraySummary) -> Self {
        let selected = vec![0; summary.dimensions.len().max(1)];
        Self {
            summary,
            selected,
            first_row: 0,
            first_col: 0,
            input: None,
            message: None,
            window: None,
            detail: None,
        }
    }

    pub(super) fn summary(&self) -> &DebugArraySummary {
        &self.summary
    }

    pub(super) fn reopen(&mut self, summary: DebugArraySummary) {
        self.summary = summary;
        self.selected
            .resize(self.summary.dimensions.len().max(1), 0);
        for (index, &upper) in self.selected.iter_mut().zip(&self.summary.dimensions) {
            *index = (*index).min(index_bound(upper));
        }
        self.input = None;
        self.message = None;
        self.window = None;
        self.detail = None;
    }

    pub(super) fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub(super) fn is_jumping(&self) -> bool {
        self.input.is_some()
    }

    pub(super) fn handle_paste(&mut self, text: &str) {
        let limit = self.input_limit();
        if let Some(input) = self.input.as_mut() {
            self.message = None;
            if !input.insert(text.trim(), limit) {
                self.set_message("Enter comma-separated integer indices.");
            }
        }
    }

    pub(super) fn handle_key(
        &mut self,
        event: KeyEvent,
        cols: usize,
        rows: usize,
    ) -> ArrayViewAction {
        if self.input.is_some() {
            self.handle_jump_key(event, cols, rows);
            return ArrayViewAction::Stay;
        }

        self.message = None;
        let layout = self.layout(cols, rows);
        let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        match event.code {
            KeyCode::Esc => {
                self.window = None;
                self.detail = None;
                return ArrayViewAction::Close;
            }
            KeyCode::Char(' ') if event.modifiers.is_empty() => {
                return ArrayViewAction::TogglePin(self.selected.clone());
            }
            KeyCode::Char('g' | 'G') if !ctrl => self.input = Some(CoordinateInput::default()),
            KeyCode::Char('j' | 'J') if !ctrl => return ArrayViewAction::JumpToWrite,
            KeyCode::Char('t' | 'T') if !ctrl => return ArrayViewAction::ToggleFollow,
            KeyCode::Up => self.move_axis(0, -1),
            KeyCode::Down => self.move_axis(0, 1),
            KeyCode::Left if ctrl => self.page_axis(1, -(layout.col_count as i32)),
            KeyCode::Right if ctrl => self.page_axis(1, layout.col_count as i32),
            KeyCode::Left => self.move_axis(1, -1),
            KeyCode::Right => self.move_axis(1, 1),
            KeyCode::PageUp => self.page_axis(0, -(layout.row_count as i32)),
            KeyCode::PageDown => self.page_axis(0, layout.row_count as i32),
            KeyCode::Char(key @ ('[' | ']')) if !ctrl => {
                if let Some(detail) = self.detail.as_mut() {
                    detail.page(cols, key == ']');
                }
            }
            _ => {}
        }
        ArrayViewAction::Stay
    }

    pub(super) fn jump_to_write(&mut self, indexes: Option<Vec<i32>>, cols: usize, rows: usize) {
        let indexes = indexes.filter(|indexes| self.valid_indexes(indexes));
        if let Some(indexes) = indexes {
            self.go_to(indexes, cols, rows);
        } else {
            self.set_message("No changes for this array.");
        }
    }

    pub(super) fn follow_write(&mut self, indexes: Vec<i32>, cols: usize, rows: usize) {
        if indexes != self.selected && self.valid_indexes(&indexes) {
            self.go_to(indexes, cols, rows);
        }
    }

    fn valid_indexes(&self, indexes: &[i32]) -> bool {
        !indexes.is_empty()
            && indexes.len() == self.summary.dimensions.len()
            && indexes
                .iter()
                .zip(&self.summary.dimensions)
                .all(|(&index, &bound)| index >= 0 && index as usize <= bound)
    }

    fn go_to(&mut self, indexes: Vec<i32>, cols: usize, rows: usize) {
        let layout = self.layout(cols, rows);
        self.selected = indexes;
        self.first_row = self.selected[0].saturating_sub(layout.row_count as i32 / 2);
        self.first_col = self
            .selected
            .get(1)
            .copied()
            .unwrap_or(0)
            .saturating_sub(layout.col_count as i32 / 2);
        self.message = None;
    }

    fn input_limit(&self) -> usize {
        self.selected.len().saturating_mul(14)
    }

    fn handle_jump_key(&mut self, event: KeyEvent, cols: usize, rows: usize) {
        self.message = None;
        let limit = self.input_limit();
        let Some(input) = self.input.as_mut() else {
            return;
        };
        match event.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Enter if event.modifiers.is_empty() => {
                match parse_indexes(&input.text, &self.summary.dimensions) {
                    Ok(indexes) => {
                        self.input = None;
                        self.go_to(indexes, cols, rows);
                    }
                    Err(message) => self.set_message(message),
                }
            }
            KeyCode::Left => input.cursor = input.cursor.saturating_sub(1),
            KeyCode::Right => input.cursor = (input.cursor + 1).min(input.text.len()),
            KeyCode::Home => input.cursor = 0,
            KeyCode::End => input.cursor = input.text.len(),
            KeyCode::Backspace if input.cursor > 0 => {
                input.cursor -= 1;
                input.text.remove(input.cursor);
            }
            KeyCode::Delete if input.cursor < input.text.len() => {
                input.text.remove(input.cursor);
            }
            KeyCode::Char(ch)
                if !event.modifiers.contains(KeyModifiers::CONTROL) && ch.is_ascii() =>
            {
                let mut bytes = [0; 1];
                input.insert(ch.encode_utf8(&mut bytes), limit);
            }
            _ => {}
        }
    }

    fn move_axis(&mut self, axis: usize, delta: i32) {
        if let Some((selected, bound)) = self
            .selected
            .get_mut(axis)
            .zip(self.summary.dimensions.get(axis))
        {
            *selected = selected.saturating_add(delta).clamp(0, index_bound(*bound));
        }
    }

    fn page_axis(&mut self, axis: usize, delta: i32) {
        self.move_axis(axis, delta);
        let first = if axis == 0 {
            &mut self.first_row
        } else {
            &mut self.first_col
        };
        *first = first.saturating_add(delta);
    }

    fn row_bound(&self) -> i32 {
        index_bound(self.summary.dimensions.first().copied().unwrap_or(0))
    }

    fn col_bound(&self) -> i32 {
        index_bound(self.summary.dimensions.get(1).copied().unwrap_or(0))
    }

    fn layout(&self, cols: usize, rows: usize) -> Layout {
        let row_label_width = (self.row_bound().to_string().len() + 2).max(5);
        let data_width = cols.saturating_sub(row_label_width);
        let show_ranges = rows >= 8;
        let show_slice = self.selected.len() > 2 && rows >= 6;
        let show_detail = rows >= 5;
        // Leave room for the title, grid labels, and the prompt on short panels.
        let grid_rows = rows.saturating_sub(
            3 + usize::from(show_ranges) + usize::from(show_slice) + usize::from(show_detail),
        );
        let minimum_cell_width = (self.col_bound().to_string().len() + 2).max(5);
        let draw_grid = grid_rows > 0 && data_width >= minimum_cell_width;
        let cell_width = if self.summary.dimensions.len() <= 1 {
            data_width.clamp(3, MAX_DEBUG_ARRAY_PREVIEW_WIDTH + 2)
        } else {
            data_width.clamp(3, GRID_CELL_WIDTH)
        };
        let col_count = if draw_grid {
            (data_width / cell_width)
                .clamp(1, MAX_DEBUG_ARRAY_WINDOW_CELLS)
                .min(self.col_bound() as usize + 1)
        } else {
            1
        };
        let row_count = if draw_grid {
            grid_rows
                .min(self.row_bound() as usize + 1)
                .min(MAX_DEBUG_ARRAY_WINDOW_CELLS / col_count)
        } else {
            1
        };
        Layout {
            draw_grid,
            show_ranges,
            show_slice,
            show_detail,
            row_label_width,
            cell_width,
            row_count,
            col_count,
        }
    }

    fn read_frame(
        &mut self,
        access: &dyn DebugPauseAccess,
        cols: usize,
        rows: usize,
        palette: GridPalette,
        focused: bool,
        follow_writes: bool,
    ) -> Frame {
        let layout = self.layout(cols, rows);
        if layout.draw_grid {
            self.first_row = window_start(
                self.first_row,
                self.selected[0],
                self.row_bound(),
                layout.row_count,
            );
            self.first_col = window_start(
                self.first_col,
                self.selected.get(1).copied().unwrap_or(0),
                self.col_bound(),
                layout.col_count,
            );
            let request = DebugArrayWindowRequest {
                first_row: self.first_row,
                first_col: self.first_col,
                row_count: layout.row_count,
                col_count: layout.col_count,
                higher_indexes: self.selected.get(2..).unwrap_or_default().to_vec(),
                preview_width: layout.cell_width - 2,
            };
            if self
                .window
                .as_ref()
                .is_none_or(|(previous, _)| previous != &request)
            {
                let cells = access.array_window(&self.summary.name, &request);
                self.window = Some((request, cells));
            }
        }
        if layout.show_detail
            && self
                .detail
                .as_ref()
                .is_none_or(|previous| previous.indexes != self.selected)
        {
            let text = match access.array_cell(&self.summary.name, &self.selected) {
                Ok(value) => format!("{} = {}", self.selected_name(), detail_value(&value)),
                Err(message) => format!("{}  {message}", self.selected_name()),
            };
            self.detail = Some(Detail {
                indexes: self.selected.clone(),
                text,
                offset: 0,
            });
        }

        let dimensions = self
            .summary
            .dimensions
            .iter()
            .map(|bound| (bound + 1).to_string())
            .collect::<Vec<_>>()
            .join("x");
        let alias = self
            .summary
            .alias_of
            .as_ref()
            .map_or_else(String::new, |name| format!(" -> {name}()"));
        let mut title = format!(
            "{}() {} {dimensions}",
            self.summary.name,
            debug_array_kind_label(self.summary.kind)
        );
        let fixed = self
            .selected
            .get(2..)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(index, value)| format!("D{}={value}", index + 3))
            .collect::<Vec<_>>()
            .join(", ");
        if !fixed.is_empty() && !(layout.draw_grid && layout.show_slice) {
            title.push_str(" | ");
            title.push_str(&fixed);
        }
        title.push_str(&alias);
        let mut content = vec![title];
        if layout.draw_grid {
            if layout.show_ranges {
                let mut ranges = format!(
                    "Rows {}-{}/{}",
                    self.first_row,
                    self.first_row as usize + layout.row_count - 1,
                    self.row_bound()
                );
                if self.selected.len() > 1 {
                    use std::fmt::Write as _;
                    let _ = write!(
                        ranges,
                        "  Cols {}-{}/{}",
                        self.first_col,
                        self.first_col as usize + layout.col_count - 1,
                        self.col_bound()
                    );
                }
                content.push(ranges);
            }
            if layout.show_slice {
                content.push(format!("Slice {fixed} | G Go to"));
            }
            match self.window.as_ref().map(|(_, cells)| cells) {
                Some(Ok(window)) => self.render_grid(&mut content, layout, window, palette),
                Some(Err(message)) => content.push(message.clone()),
                None => {}
            }
        }

        let mut detail_has_pages = false;
        let detail = self
            .detail
            .as_ref()
            .filter(|_| layout.show_detail)
            .map(|detail| {
                let remaining = &detail.text[detail.offset..];
                let text: String = remaining.chars().take(cols).collect();
                detail_has_pages = detail.offset != 0 || text.len() < remaining.len();
                text
            });
        let follow_status = if follow_writes {
            "Follow ON"
        } else {
            "Follow OFF"
        };
        let (status, input_cursor) = if let Some(input) = self.input.as_ref() {
            let prefix = if cols >= 16 { "Go to " } else { "" };
            let width = cols.saturating_sub(prefix.len()).max(1);
            let start = (input.cursor + 1).saturating_sub(width);
            let end = (start + width).min(input.text.len());
            let mut prompt = format!("{prefix}{}", &input.text[start..end]);
            prompt.push_str("  ");
            prompt.push_str(self.message.as_deref().unwrap_or("Enter Jump  Esc Cancel"));
            append_status_hint(&mut prompt, follow_status, cols);
            (
                prompt,
                Some((prefix.len() + input.cursor - start).min(cols.saturating_sub(1))),
            )
        } else if !focused {
            let mut status = follow_status.to_string();
            append_status_hint(&mut status, "Tab Array", cols);
            (status, None)
        } else if let Some(message) = &self.message {
            let mut status = message.clone();
            append_status_hint(&mut status, follow_status, cols);
            (status, None)
        } else {
            let mut status = format!("T {follow_status}");
            if detail_has_pages {
                append_status_hint(&mut status, "[/] Value", cols);
            }
            for hint in [
                "J Nearest change",
                "G Go to",
                "Space Pin",
                "Esc Arrays",
                "Tab Code",
            ] {
                append_status_hint(&mut status, hint, cols);
            }
            (status, None)
        };
        Frame {
            content,
            detail,
            status,
            input_cursor: input_cursor.filter(|_| focused),
        }
    }

    fn selected_name(&self) -> String {
        let indexes = self
            .selected
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!("{}({indexes})", self.summary.name)
    }

    fn render_grid(
        &self,
        content: &mut Vec<String>,
        layout: Layout,
        window: &DebugArrayWindow,
        palette: GridPalette,
    ) {
        let label_width = layout.row_label_width;
        let cell_width = layout.cell_width;
        let inner_width = cell_width - 2;
        let matrix = self.selected.len() == 2;
        let shade_zero = self.selected.len() <= 2 && window.mat_base == 1;
        let mut headers = " ".repeat(label_width);
        if self.selected.len() == 1 {
            headers.clear();
            push_grid_field(
                &mut headers,
                "",
                palette.label,
                format_args!("{:<label_width$}Value", "Idx"),
            );
        } else {
            for column in 0..layout.col_count {
                let label = self.first_col as usize + column;
                push_grid_field(
                    &mut headers,
                    "",
                    palette.label,
                    format_args!(" {label:>inner_width$} "),
                );
            }
        }
        content.push(headers);

        for (index, row) in window.cells.chunks(layout.col_count).enumerate() {
            let row_index = self.first_row as usize + index;
            let mut text = String::new();
            push_grid_field(
                &mut text,
                "",
                palette.label,
                format_args!("{row_index:>width$}  ", width = label_width - 2),
            );
            for (column, value) in row.iter().enumerate() {
                let col_index = self.first_col as usize + column;
                let selected = row_index == self.selected[0] as usize
                    && col_index == self.selected.get(1).copied().unwrap_or(0) as usize;
                let (left, right) = if selected { ('[', ']') } else { (' ', ' ') };
                let inactive = shade_zero && (row_index == 0 || (matrix && col_index == 0));
                let background = if inactive { palette.inactive } else { "" };
                let changed = window
                    .changed
                    .get(index * layout.col_count + column)
                    .copied()
                    .unwrap_or(false);
                let foreground = if changed { palette.changed } else { "" };
                if self.summary.kind == DebugArrayKind::String {
                    push_grid_field(
                        &mut text,
                        background,
                        foreground,
                        format_args!("{left}{value:<inner_width$}{right}"),
                    );
                } else {
                    push_grid_field(
                        &mut text,
                        background,
                        foreground,
                        format_args!("{left}{value:>inner_width$}{right}"),
                    );
                }
            }
            content.push(text);
        }
    }

    pub(super) fn render_panel(
        &mut self,
        stdout: &mut impl Write,
        access: &dyn DebugPauseAccess,
        ansi: bool,
        rect: (u16, u16, u16, u16),
        focused: bool,
        follow_writes: bool,
    ) -> io::Result<Option<(u16, u16)>> {
        let (x, y, width, height) = rect;
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let cols = usize::from(width);
        let rows = usize::from(height);
        let palette = GridPalette::for_theme(current_syntax_theme(), ansi);
        let frame = self.read_frame(access, cols, rows, palette, focused, follow_writes);
        let content_rows = rows - 1 - usize::from(frame.detail.is_some());
        for row in 0..content_rows {
            queue!(stdout, MoveTo(x, y + row as u16))?;
            let line = frame.content.get(row).map_or("", String::as_str);
            let width = visible_width(line);
            if width > cols {
                write!(stdout, "{}", rendered_line_window(line, 0, cols))?;
            } else {
                write!(stdout, "{line}{}", " ".repeat(cols - width))?;
            }
        }
        if let Some(detail) = frame.detail {
            queue!(stdout, MoveTo(x, y + content_rows as u16))?;
            write!(stdout, "{}", fit_plain_text(&detail, cols))?;
        }
        let status_y = y + (height - 1);
        queue!(stdout, MoveTo(x, status_y))?;
        let status = fit_plain_text(&frame.status, cols);
        if ansi && focused {
            write!(stdout, "{STATUS_BAR_STYLE}{status}{RESET}")?;
        } else {
            write!(stdout, "{status}")?;
        }
        Ok(frame
            .input_cursor
            .map(|column| (x + column as u16, status_y)))
    }
}

fn append_status_hint(status: &mut String, hint: &str, cols: usize) {
    if status.chars().count() + 2 + hint.len() <= cols {
        status.push_str("  ");
        status.push_str(hint);
    }
}

fn push_grid_field(
    line: &mut String,
    background: &str,
    foreground: &str,
    field: std::fmt::Arguments<'_>,
) {
    use std::fmt::Write as _;

    line.push_str(background);
    line.push_str(foreground);
    let _ = line.write_fmt(field);
    if !background.is_empty() || !foreground.is_empty() {
        line.push_str(RESET);
    }
}

fn index_bound(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn window_start(previous: i32, selected: i32, upper: i32, count: usize) -> i32 {
    let tail = (count - 1) as i32;
    previous
        .clamp(0, upper - tail)
        .clamp(selected.saturating_sub(tail).max(0), selected)
}

fn parse_indexes(input: &str, dimensions: &[usize]) -> Result<Vec<i32>, String> {
    let parts = input.split(',').collect::<Vec<_>>();
    if parts.len() != dimensions.len() {
        return Err(format!(
            "Enter {} comma-separated indices.",
            dimensions.len()
        ));
    }
    parts
        .into_iter()
        .zip(dimensions)
        .enumerate()
        .map(|(axis, (text, &upper))| {
            let index: i32 = text
                .trim()
                .parse()
                .map_err(|_| format!("Index {} must be an integer.", axis + 1))?;
            if index < 0 || index as usize > upper {
                return Err(format!("Index {} must be in 0-{upper}.", axis + 1));
            }
            Ok(index)
        })
        .collect()
}

fn detail_value(value: &DebugValue) -> String {
    let text = format_debug_value(value);
    let ordinary = |ch| matches!(ch, ' '..='~' | '\u{a0}'..='\u{ac}' | '\u{ae}'..='\u{24f}');
    if text.chars().all(ordinary) {
        return text;
    }
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if ordinary(ch) {
            escaped.push(ch);
        } else {
            escaped.extend(ch.escape_default());
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct WindowAccess {
        window_reads: RefCell<Vec<DebugArrayWindowRequest>>,
        cell_reads: Cell<usize>,
    }

    impl DebugPauseAccess for WindowAccess {
        fn idle(&mut self) -> io::Result<()> {
            Ok(())
        }

        fn set_variable(
            &mut self,
            _: &crate::debugger::DebugEditTarget,
            _: DebugValue,
        ) -> Result<crate::debugger::DebugSnapshot, String> {
            unreachable!("The viewer never writes to the interpreter")
        }

        fn array_window(
            &self,
            _: &str,
            request: &DebugArrayWindowRequest,
        ) -> Result<DebugArrayWindow, String> {
            self.window_reads.borrow_mut().push(request.clone());
            Ok(DebugArrayWindow {
                cells: vec!["0.12".into(); request.row_count * request.col_count],
                mat_base: 0,
                changed: Vec::new(),
            })
        }

        fn array_cell(&self, _: &str, _: &[i32]) -> Result<DebugValue, String> {
            self.cell_reads.set(self.cell_reads.get() + 1);
            Ok(DebugValue::Number(0.123456789))
        }
    }

    fn numeric_array(dimensions: Vec<usize>) -> DebugArraySummary {
        DebugArraySummary {
            name: "M".into(),
            alias_of: None,
            kind: DebugArrayKind::Number,
            elements: dimensions.iter().map(|bound| bound + 1).product(),
            dimensions,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn window_scrolls_and_clamps_to_the_inclusive_bounds() {
        assert_eq!(window_start(0, 6, 99, 5), 2);
        assert_eq!(window_start(50, 1, 99, 5), 1);
        assert_eq!(window_start(99, 99, 99, 5), 95);
        assert_eq!(window_start(0, 0, 0, 1), 0);
    }

    #[test]
    fn direct_jump_uses_all_axes_and_can_pin_the_selected_cell() {
        let mut view = ArrayView::new(numeric_array(vec![99999, 79, 2]));
        view.handle_key(key(KeyCode::Char('g')), 80, 24);
        view.handle_paste("50000, 12, 2");
        view.handle_key(key(KeyCode::Enter), 80, 24);
        assert!(!view.is_jumping());
        assert!(view.first_row > 49_900);
        assert_eq!(
            view.handle_key(key(KeyCode::Char(' ')), 80, 24),
            ArrayViewAction::TogglePin(vec![50000, 12, 2])
        );
        assert_eq!(
            view.handle_key(key(KeyCode::Esc), 80, 24),
            ArrayViewAction::Close
        );
        view.reopen(numeric_array(vec![40000, 10, 2]));
        assert_eq!(view.selected, [40000, 10, 2]);
    }

    #[test]
    fn nearest_change_jump_reveals_the_cell_in_its_slice() {
        let mut view = ArrayView::new(numeric_array(vec![99999, 79, 2]));
        let access = WindowAccess::default();
        assert_eq!(
            view.handle_key(key(KeyCode::Char('j')), 36, 14),
            ArrayViewAction::JumpToWrite
        );
        view.jump_to_write(Some(vec![50000, 12, 2]), 36, 14);

        let frame = view.read_frame(&access, 36, 14, GridPalette::default(), true, false);
        assert!(frame.status.contains("J Nearest change"));
        assert!(frame.detail.unwrap().starts_with("M(50000,12,2)"));
        let reads = access.window_reads.borrow();
        let request = &reads[0];
        assert_eq!(request.first_row, 50000 - request.row_count as i32 / 2);
        assert_eq!(request.first_col, 12 - request.col_count as i32 / 2);
        assert_eq!(request.higher_indexes, [2]);
    }

    #[test]
    fn missing_or_stale_change_leaves_selection_in_place() {
        let mut view = ArrayView::new(numeric_array(vec![3]));
        view.handle_key(key(KeyCode::Down), 36, 14);
        for indexes in [None, Some(vec![4])] {
            view.jump_to_write(indexes, 36, 14);
            assert_eq!(view.selected, [1]);
            assert_eq!(view.message.as_deref(), Some("No changes for this array."));
        }
    }

    #[test]
    fn follow_uses_the_current_slice_coordinates_and_session_mode() {
        let mut view = ArrayView::new(numeric_array(vec![99, 79, 2]));
        let access = WindowAccess::default();
        let palette = GridPalette::default();
        assert_eq!(
            view.handle_key(key(KeyCode::Char('t')), 36, 14),
            ArrayViewAction::ToggleFollow
        );
        view.follow_write(vec![50, 12, 2], 36, 14);
        let frame = view.read_frame(&access, 36, 14, palette, true, true);
        assert!(frame.status.starts_with("T Follow ON"));
        assert!(frame.detail.unwrap().starts_with("M(50,12,2)"));
        assert_eq!(access.window_reads.borrow()[0].higher_indexes, [2]);

        view.follow_write(vec![50, 12, 3], 36, 14);
        let frame = view.read_frame(&access, 36, 14, palette, false, false);
        assert!(frame.status.starts_with("Follow OFF"));
        assert_eq!(view.selected, [50, 12, 2]);
        assert!(view.message.is_none());
    }

    #[test]
    fn invalid_jump_stays_open_for_correction_and_escape_only_cancels_it() {
        let mut view = ArrayView::new(numeric_array(vec![3]));
        view.handle_key(key(KeyCode::Char('g')), 80, 24);
        view.handle_paste("4");
        view.handle_key(key(KeyCode::Enter), 80, 24);
        assert!(view.is_jumping());
        assert_eq!(view.message.as_deref(), Some("Index 1 must be in 0-3."));
        assert_eq!(
            view.handle_key(key(KeyCode::Esc), 80, 24),
            ArrayViewAction::Stay
        );
        assert!(!view.is_jumping());
        assert_eq!(view.selected, [0]);
    }

    #[test]
    fn layout_limits_large_terminal_requests_and_supports_vectors() {
        let view = ArrayView::new(numeric_array(vec![99999, 7999]));
        let layout = view.layout(65535, 65535);
        assert!(layout.row_count * layout.col_count <= MAX_DEBUG_ARRAY_WINDOW_CELLS);
        let layout = view.layout(3, 2);
        assert!(!layout.draw_grid);
        assert_eq!((layout.row_count, layout.col_count), (1, 1));

        let view = ArrayView::new(numeric_array(vec![99999]));
        let layout = view.layout(80, 24);
        assert!(layout.draw_grid);
        assert_eq!(layout.col_count, 1);
        assert_eq!(layout.row_count, 19);
    }

    #[test]
    fn grid_styles_keep_the_matrix_border_and_changed_values_visible() {
        let view = ArrayView::new(numeric_array(vec![1, 1]));
        let layout = view.layout(80, 24);
        let window = DebugArrayWindow {
            cells: ["10", "20", "30", "40"].map(str::to_string).to_vec(),
            mat_base: 1,
            changed: vec![true, false, false, true],
        };
        let mut plain = Vec::new();
        view.render_grid(&mut plain, layout, &window, GridPalette::default());
        assert!(plain.iter().all(|line| !line.contains('\x1b')));
        assert!(plain[1].contains("10]"));
        for theme in [SyntaxTheme::Dark, SyntaxTheme::Light] {
            let palette = GridPalette::for_theme(theme, true);
            let mut styled = Vec::new();
            view.render_grid(&mut styled, layout, &window, palette);
            assert_eq!(styled[0].matches(palette.label).count(), 2);
            assert!(styled[1].starts_with(palette.label));
            assert_eq!(styled[1].matches(palette.inactive).count(), 2);
            assert_eq!(styled[2].matches(palette.inactive).count(), 1);
            assert!(styled[1].contains(&format!("{}{}[", palette.inactive, palette.changed)));
            assert!(styled[2].contains(&format!(" 40 {RESET}")));
            assert!(styled[2].contains(palette.changed));
            assert!(styled
                .iter()
                .zip(&plain)
                .all(|(styled, plain)| visible_width(styled) == plain.chars().count()));
        }
    }

    #[test]
    fn base_one_shades_vector_index_zero_and_not_higher_dimensional_slices() {
        let palette = GridPalette::for_theme(SyntaxTheme::Dark, true);
        let vector = ArrayView::new(numeric_array(vec![1]));
        let window = DebugArrayWindow {
            cells: vec!["0".into(); 2],
            mat_base: 1,
            changed: vec![false; 2],
        };
        let mut content = Vec::new();
        vector.render_grid(&mut content, vector.layout(80, 24), &window, palette);
        assert!(content[1].contains(palette.inactive));
        assert!(!content[2].contains(palette.inactive));

        let cube = ArrayView::new(numeric_array(vec![1, 0, 1]));
        content.clear();
        cube.render_grid(&mut content, cube.layout(80, 24), &window, palette);
        assert!(content.iter().all(|line| !line.contains(palette.inactive)));
    }

    #[test]
    fn the_view_reads_only_visible_cells_and_caches_unchanged_requests() {
        let mut view = ArrayView::new(numeric_array(vec![99999, 79, 2]));
        let access = WindowAccess::default();
        let palette = GridPalette::default();
        let frame = view.read_frame(&access, 80, 24, palette, true, true);
        assert_eq!(frame.detail.as_deref(), Some("M(0,0,0) = 0.123456789"));
        assert!(frame
            .content
            .iter()
            .any(|line| line == "Slice D3=0 | G Go to"));

        view.handle_key(key(KeyCode::Right), 80, 24);
        view.read_frame(&access, 80, 24, palette, true, true);
        assert_eq!(access.window_reads.borrow().len(), 1);
        assert_eq!(access.cell_reads.get(), 2);

        view.handle_key(key(KeyCode::PageDown), 80, 24);
        view.read_frame(&access, 80, 24, palette, true, true);
        let requests = access.window_reads.borrow();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].row_count * requests[0].col_count, 108);
        assert_eq!(requests[1].first_row, 18);
        assert_eq!(requests[1].higher_indexes, [0]);
        assert_eq!(access.cell_reads.get(), 3);
    }

    #[test]
    fn short_panel_keeps_a_grid_row_and_identifies_the_slice() {
        let mut view = ArrayView::new(numeric_array(vec![9, 9, 3]));
        let access = WindowAccess::default();
        let frame = view.read_frame(&access, 36, 4, GridPalette::default(), true, true);

        assert_eq!(frame.content.len(), 3);
        assert_eq!(frame.content[0], "M() NUM 10x10x4 | D3=0");
        assert!(frame.content[2].contains("0.12"));
        assert!(frame.detail.is_none());
        assert_eq!(access.cell_reads.get(), 0);
        let requests = access.window_reads.borrow();
        assert_eq!((requests[0].row_count, requests[0].col_count), (1, 2));
    }

    #[test]
    fn jump_cursor_appears_only_when_panel_has_focus() {
        let mut view = ArrayView::new(numeric_array(vec![9, 9]));
        let access = WindowAccess::default();
        view.handle_key(key(KeyCode::Char('g')), 36, 4);
        view.handle_paste("3,4");
        let palette = GridPalette::default();

        let focused = view.read_frame(&access, 36, 4, palette, true, true);
        assert!(focused.status.starts_with("Go to 3,4"));
        assert_eq!(focused.input_cursor, Some("Go to 3,4".len()));
        let unfocused = view.read_frame(&access, 36, 4, palette, false, true);
        assert!(unfocused.status.starts_with("Go to 3,4"));
        assert_eq!(unfocused.input_cursor, None);
    }

    #[test]
    fn detail_paging_preserves_utf8_boundaries() {
        let mut detail = Detail {
            indexes: vec![0],
            text: "M(0) = \"áéíóú\"".into(),
            offset: 0,
        };
        detail.page(9, true);
        assert_eq!(&detail.text[detail.offset..], "éíóú\"");
        detail.page(9, false);
        assert_eq!(detail.offset, 0);
    }
}
