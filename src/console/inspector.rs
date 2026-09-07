//! Snapshot-based inspector navigation, presentation and literal-value editing.
//! Edits only produce typed proposals; this module never evaluates or executes BASIC.
use super::*;
use crate::debugger::{
    DebugDataSnapshot, DebugEditTarget, DebugSnapshot, DebugValue, DebugVariable,
};

mod edit;
use edit::{EditField, ScalarEdit};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum Category {
    Pinned,
    Variables,
    Arrays,
    Stack,
    State,
    Timers,
    Data,
}

impl Category {
    fn title(self) -> &'static str {
        match self {
            Self::Pinned => "PINNED",
            Self::Variables => "VARIABLES",
            Self::Arrays => "ARRAYS",
            Self::Stack => "STACK",
            Self::State => "STATE",
            Self::Timers => "TIMERS",
            Self::Data => "DATA",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EntryId {
    Heading(Category),
    Scalar(Category, String),
    ArrayElement(Category, String),
    Array(String),
    Frame(usize),
    State,
    Timer(i32),
    Data,
}

impl EntryId {
    fn category(&self) -> Category {
        match self {
            Self::Heading(category)
            | Self::Scalar(category, _)
            | Self::ArrayElement(category, _) => *category,
            Self::Array(_) => Category::Arrays,
            Self::Frame(_) => Category::Stack,
            Self::State => Category::State,
            Self::Timer(_) => Category::Timers,
            Self::Data => Category::Data,
        }
    }

    fn selectable(&self) -> bool {
        matches!(
            self,
            Self::Heading(_) | Self::Scalar(_, _) | Self::ArrayElement(_, _)
        )
    }

    fn variable(&self) -> Option<(bool, &str)> {
        match self {
            Self::Scalar(_, key) => Some((false, key)),
            Self::ArrayElement(_, key) => Some((true, key)),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Pin {
    key: String,
    array: bool,
    display_name: String,
}

impl Pin {
    fn entry_id(&self) -> EntryId {
        if self.array {
            EntryId::ArrayElement(Category::Pinned, self.key.clone())
        } else {
            EntryId::Scalar(Category::Pinned, self.key.clone())
        }
    }
}

fn array_base(name: &str) -> &str {
    name.split('(').next().unwrap_or(name)
}

fn visible_array_target<'a>(snapshot: &'a DebugSnapshot, name: &str) -> Option<&'a str> {
    // The runtime snapshot already resolves aliases to their physical target.
    // Resolve again on every pause: a visible alias can change scope or target.
    snapshot
        .arrays
        .iter()
        .find(|array| array.name.eq_ignore_ascii_case(name))
        .map(|array| array.alias_of.as_deref().unwrap_or(&array.name))
}

fn pinned_array_value<'a>(snapshot: &'a DebugSnapshot, name: &str) -> Option<&'a DebugVariable> {
    let target = visible_array_target(snapshot, name)?;
    snapshot.array_elements.iter().find(|variable| {
        visible_array_target(snapshot, array_base(&variable.name))
            .is_some_and(|element_target| element_target.eq_ignore_ascii_case(target))
    })
}

#[derive(Debug)]
struct Entry {
    id: Option<EntryId>,
    text: String,
    changed: bool,
}

impl Entry {
    fn value(id: EntryId, text: String, changed: bool) -> Self {
        Self {
            id: Some(id),
            text,
            changed,
        }
    }

    fn blank(text: &str) -> Self {
        Self {
            id: None,
            text: text.into(),
            changed: false,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct InspectorState {
    pub focused: bool,
    pub scroll: usize,
    collapsed: HashSet<Category>,
    pins: Vec<Pin>,
    selected: Option<EntryId>,
    hint: &'static str,
    manual_scroll: bool,
    editable: bool,
    editing: Option<ActiveEdit>,
}

#[derive(Debug)]
struct ActiveEdit {
    entry: EntryId,
    target: DebugEditTarget,
    prefix: String,
    input: ScalarEdit,
    visible: bool,
}

#[derive(Debug)]
struct PanelEdit {
    column: usize,
    row: usize,
    start: usize,
    prefix: String,
    field: EditField,
}

#[derive(Debug)]
pub(super) struct InspectorPanel {
    columns: Vec<Vec<Entry>>,
    width: usize,
    selected: Option<EntryId>,
    edit: Option<PanelEdit>,
}

impl InspectorState {
    pub(super) fn status_hint(&self) -> &'static str {
        if self.is_editing() {
            return "Enter Apply Esc Cancel";
        }
        if self.focused {
            self.hint
        } else {
            ""
        }
    }

    pub(super) fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
        if !editable {
            self.editing = None;
        }
    }

    pub(super) fn is_editing(&self) -> bool {
        self.editing.is_some()
    }

    pub(super) fn edit_status(&self) -> Option<&str> {
        self.editing.as_ref().and_then(|edit| edit.input.error())
    }

    pub(super) fn edit_failed(&mut self, message: String) {
        if let Some(edit) = &mut self.editing {
            edit.input.fail(message);
        }
    }

    pub(super) fn edit_committed(&mut self) {
        self.editing = None;
    }

    pub(super) fn handle_edit_key(
        &mut self,
        event: KeyEvent,
    ) -> Option<(DebugEditTarget, DebugValue)> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        if event.code == KeyCode::Esc {
            self.editing = None;
            return None;
        }
        let edit = self.editing.as_mut()?;
        if !edit.visible {
            return None;
        }
        edit.input
            .key(event)
            .map(|variable| (edit.target.clone(), variable.value))
    }

    pub(super) fn handle_edit_paste(&mut self, text: &str) {
        if let Some(edit) = self.editing.as_mut().filter(|edit| edit.visible) {
            edit.input.paste(text);
        }
    }

    pub(super) fn begin_edit(
        &mut self,
        snapshot: &DebugSnapshot,
        changes: &DebugPanelChanges,
        width: usize,
        rows: usize,
    ) -> bool {
        if !self.editable || !self.focused || self.is_editing() || width == 0 || rows == 0 {
            return false;
        }
        let panel = self.panel(snapshot, changes, width, rows);
        let Some(entry) = panel.selected.clone() else {
            return false;
        };
        let Some((array, key)) = entry.variable() else {
            return false;
        };
        let variable = if array {
            pinned_array_value(snapshot, key)
        } else {
            snapshot
                .variables
                .iter()
                .find(|variable| variable.name.eq_ignore_ascii_case(key))
        };
        let Some(variable) = variable else {
            return false;
        };
        let Some((column, row)) = panel.position(&entry) else {
            return false;
        };
        let text = &panel.columns[column][row].text;
        let Some(value_start) = text.find(" = ") else {
            return false;
        };
        let prefix = text[..value_start + 3].to_string();
        let target = if array {
            let Some((_, indexes)) = variable.name.split_once('(') else {
                return false;
            };
            let Some(indexes) = indexes.strip_suffix(')') else {
                return false;
            };
            let Ok(indexes) = indexes
                .split(',')
                .map(|index| index.trim().parse::<i32>())
                .collect::<Result<Vec<_>, _>>()
            else {
                return false;
            };
            DebugEditTarget::ArrayElement {
                name: key.into(),
                indexes,
            }
        } else {
            DebugEditTarget::Scalar(variable.name.clone())
        };
        self.editing = Some(ActiveEdit {
            entry,
            target,
            prefix,
            input: ScalarEdit::new(variable),
            visible: false,
        });
        if self
            .panel(snapshot, changes, width, rows)
            .edit_cursor()
            .is_none()
        {
            self.editing = None;
            return false;
        }
        true
    }

    fn is_pinned(&self, key: &str, array: bool) -> bool {
        self.pins
            .iter()
            .any(|pin| pin.key == key && pin.array == array)
    }

    fn category(&self, category: Category, entries: Vec<Entry>) -> Vec<Entry> {
        let collapsed = self.collapsed.contains(&category);
        let mut result = vec![Entry::value(
            EntryId::Heading(category),
            format!(
                "{} {} ({})",
                if collapsed { '▸' } else { '▾' },
                category.title(),
                entries.len()
            ),
            false,
        )];
        if !collapsed {
            if entries.is_empty() {
                result.push(Entry::blank("  (none)"));
            } else {
                result.extend(entries);
            }
        }
        result
    }

    fn columns(
        &self,
        snapshot: &DebugSnapshot,
        changes: &DebugPanelChanges,
        width: usize,
    ) -> Vec<Vec<Entry>> {
        let pinned = self
            .pins
            .iter()
            .map(|pin| {
                let variable = if pin.array {
                    pinned_array_value(snapshot, &pin.key)
                } else {
                    snapshot
                        .variables
                        .iter()
                        .find(|variable| variable.name.eq_ignore_ascii_case(&pin.key))
                };
                let changed = variable.is_some_and(|variable| {
                    changes
                        .variables
                        .contains(&variable.name.to_ascii_uppercase())
                });
                let name = if pin.array {
                    variable.map_or_else(
                        || pin.display_name.clone(),
                        |variable| {
                            let indices = variable
                                .name
                                .find('(')
                                .map_or("", |position| &variable.name[position..]);
                            format!("{}{indices}", array_base(&pin.display_name))
                        },
                    )
                } else {
                    variable
                        .map_or(pin.display_name.as_str(), |variable| variable.name.as_str())
                        .to_string()
                };
                let value = variable.map_or_else(
                    || "<unavailable>".into(),
                    |variable| format_debug_value(&variable.value),
                );
                Entry::value(
                    pin.entry_id(),
                    format!("{}{name} = {value}", debug_panel_change_prefix(changed)),
                    changed,
                )
            })
            .collect();
        let mut values = self.category(Category::Pinned, pinned);
        let variables = snapshot
            .variables
            .iter()
            .map(|variable| {
                let key = variable.name.to_ascii_uppercase();
                let changed = changes.variables.contains(&key);
                Entry::value(
                    EntryId::Scalar(Category::Variables, key.clone()),
                    format!(
                        "{}{}{} = {}",
                        debug_panel_change_prefix(changed),
                        if self.is_pinned(&key, false) {
                            "[+] "
                        } else {
                            ""
                        },
                        variable.name,
                        format_debug_value(&variable.value)
                    ),
                    changed,
                )
            })
            .chain(snapshot.array_elements.iter().map(|variable| {
                let key = variable.name.to_ascii_uppercase();
                let changed = changes.variables.contains(&key);
                // A different last-written index is still the same inspector entry.
                let base = array_base(&key).to_string();
                Entry::value(
                    EntryId::ArrayElement(Category::Variables, base.clone()),
                    format!(
                        "{}{}{} = {}",
                        debug_panel_change_prefix(changed),
                        if self.is_pinned(&base, true) {
                            "[+] "
                        } else {
                            ""
                        },
                        variable.name,
                        format_debug_value(&variable.value)
                    ),
                    changed,
                )
            }))
            .collect();
        values.extend(self.category(Category::Variables, variables));
        let arrays = snapshot
            .arrays
            .iter()
            .map(|array| {
                let key = array.name.to_ascii_uppercase();
                let changed = changes.arrays.contains(&key);
                let dimensions = array
                    .dimensions
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let alias = array
                    .alias_of
                    .as_deref()
                    .map_or_else(String::new, |target| format!(" -> {target}"));
                Entry::value(
                    EntryId::Array(key),
                    format!(
                        "{}{}{}({}) {} [{}]",
                        debug_panel_change_prefix(changed),
                        array.name,
                        alias,
                        dimensions,
                        debug_array_kind_label(array.kind),
                        array.elements
                    ),
                    changed,
                )
            })
            .collect();
        values.extend(self.category(Category::Arrays, arrays));

        let frames = snapshot
            .stack
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let name = frame.name.as_deref().unwrap_or("");
                let line = frame
                    .line
                    .map_or_else(String::new, |line| format!(" @{line}"));
                let separator = if name.is_empty() { "" } else { " " };
                Entry::value(
                    EntryId::Frame(index),
                    format!(
                        "  {index}: {}{separator}{name}{line}",
                        debug_frame_kind_label(frame.kind)
                    ),
                    false,
                )
            })
            .collect();
        let mut stack = self.category(Category::Stack, frames);
        stack.extend(self.category(
            Category::State,
            vec![Entry::value(
                EntryId::State,
                format!(
                    "{}ERR={} ERL={}",
                    debug_panel_change_prefix(changes.state),
                    snapshot.err,
                    snapshot.erl
                ),
                changes.state,
            )],
        ));

        let timers = snapshot
            .timers
            .iter()
            .map(|timer| {
                let changed = changes.timers.contains(&timer.number);
                Entry::value(
                    EntryId::Timer(timer.number),
                    format!(
                        "{}#{} {} -> {} {} {}/{}ms",
                        debug_panel_change_prefix(changed),
                        timer.number,
                        if timer.repeat { "EVERY" } else { "AFTER" },
                        timer.target,
                        if timer.active { "active" } else { "stopped" },
                        timer.remaining.as_millis(),
                        timer.interval.as_millis()
                    ),
                    changed,
                )
            })
            .collect();
        let mut timers = self.category(Category::Timers, timers);
        let data = match &snapshot.data {
            DebugDataSnapshot::Empty => Vec::new(),
            DebugDataSnapshot::Next {
                line,
                line_item,
                value,
                ..
            } => vec![Entry::value(
                EntryId::Data,
                format!(
                    "{}Ln {line} Item {line_item}: {}",
                    debug_panel_change_prefix(changes.data),
                    format_debug_value(value)
                ),
                changes.data,
            )],
            DebugDataSnapshot::Exhausted { .. } => vec![Entry::value(
                EntryId::Data,
                format!("{}(end)", debug_panel_change_prefix(changes.data)),
                changes.data,
            )],
        };
        timers.extend(self.category(Category::Data, data));

        match debug_panel_column_count(width) {
            3 => vec![values, stack, timers],
            2 => {
                stack.push(Entry::blank(""));
                stack.extend(timers);
                vec![values, stack]
            }
            _ => {
                values.push(Entry::blank(""));
                values.extend(stack);
                values.push(Entry::blank(""));
                values.extend(timers);
                vec![values]
            }
        }
    }

    pub(super) fn panel(
        &mut self,
        snapshot: &DebugSnapshot,
        changes: &DebugPanelChanges,
        width: usize,
        rows: usize,
    ) -> InspectorPanel {
        let mut panel = InspectorPanel {
            columns: self.columns(snapshot, changes, width),
            width,
            selected: None,
            edit: None,
        };
        if self
            .selected
            .as_ref()
            .and_then(|id| panel.position(id))
            .is_none()
        {
            self.selected = Some(EntryId::Heading(
                self.selected
                    .as_ref()
                    .map_or(Category::Pinned, EntryId::category),
            ));
        }
        // A temporarily hidden inspector must not reset the user's viewport.
        if width > 0 && rows > 0 {
            self.scroll = self.scroll.min(panel.height().saturating_sub(rows));
        }
        if self.focused {
            if width > 0 && rows > 0 && !self.manual_scroll {
                if let Some((_, row)) = self.selected.as_ref().and_then(|id| panel.position(id)) {
                    if row < self.scroll {
                        self.scroll = row;
                    } else if row >= self.scroll.saturating_add(rows) {
                        self.scroll = row + 1 - rows;
                    }
                }
            }
            panel.selected = self
                .selected
                .as_ref()
                .filter(|id| {
                    width > 0
                        && rows > 0
                        && panel.position(id).is_some_and(|(_, row)| {
                            row >= self.scroll && row < self.scroll.saturating_add(rows)
                        })
                })
                .cloned();
        }
        self.hint = match panel.selected.as_ref() {
            Some(EntryId::Heading(category)) => {
                if self.collapsed.contains(category) {
                    "Space Expand"
                } else {
                    "Space Collapse"
                }
            }
            Some(id) if id.variable().is_some() => {
                let (array, key) = id.variable().unwrap();
                let available = if array {
                    pinned_array_value(snapshot, key).is_some()
                } else {
                    snapshot
                        .variables
                        .iter()
                        .any(|variable| variable.name.eq_ignore_ascii_case(key))
                };
                match (self.editable && available, self.is_pinned(key, array)) {
                    (true, true) => "Enter Edit Space Unpin",
                    (true, false) => "Enter Edit Space Pin",
                    (false, true) => "Space Unpin",
                    (false, false) => "Space Pin",
                }
            }
            _ => "",
        };
        if let Some(edit) = &mut self.editing {
            edit.visible = false;
            if let Some((column, row)) = panel.position(&edit.entry).filter(|(_, row)| {
                self.focused
                    && width > 0
                    && rows > 0
                    && *row >= self.scroll
                    && *row < self.scroll.saturating_add(rows)
            }) {
                {
                    let start = 1 + edit.prefix.chars().count();
                    let column_width = debug_panel_column_width(width, panel.columns.len());
                    let field_width = column_width
                        .min(width.saturating_sub(column * (column_width + 3)))
                        .saturating_sub(start);
                    if let Some(field) = edit.input.field(field_width) {
                        edit.visible = true;
                        panel.edit = Some(PanelEdit {
                            column,
                            row,
                            start,
                            prefix: edit.prefix.clone(),
                            field,
                        });
                    }
                }
            }
        }
        panel
    }

    pub(super) fn handle_key(
        &mut self,
        code: KeyCode,
        snapshot: &DebugSnapshot,
        changes: &DebugPanelChanges,
        width: usize,
        rows: usize,
    ) -> bool {
        if !self.focused || width == 0 || rows == 0 || code == KeyCode::Enter || self.is_editing() {
            return false;
        }
        let panel = self.panel(snapshot, changes, width, rows);
        if matches!(code, KeyCode::PageUp | KeyCode::PageDown) {
            self.manual_scroll = true;
            let max_scroll = panel.height().saturating_sub(rows);
            self.scroll = if code == KeyCode::PageUp {
                self.scroll.saturating_sub(rows)
            } else {
                self.scroll.saturating_add(rows).min(max_scroll)
            };
            self.panel(snapshot, changes, width, rows);
            return true;
        }
        let Some(id) = self.selected.clone() else {
            return false;
        };
        let Some((column, row)) = panel.position(&id) else {
            return false;
        };
        let selectable = |column: usize| -> Vec<usize> {
            panel.columns[column]
                .iter()
                .enumerate()
                .filter_map(|(row, entry)| {
                    entry.id.as_ref().filter(|id| id.selectable()).map(|_| row)
                })
                .collect()
        };
        let current_rows = selectable(column);
        let visible = panel.selected.is_some();
        if !visible && code == KeyCode::Char(' ') {
            return true;
        }
        // Paging can put the selection outside the viewport. An arrow first
        // recovers an actionable entry near the visible page, rather than
        // jumping back to the remembered, possibly distant selection.
        if !visible
            && matches!(
                code,
                KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right
            )
        {
            let target_column = match code {
                KeyCode::Left => column.saturating_sub(1),
                KeyCode::Right => (column + 1).min(panel.columns.len() - 1),
                _ => column,
            };
            let target_rows = selectable(target_column);
            let bottom = self.scroll.saturating_add(rows);
            let visible_rows = target_rows
                .iter()
                .copied()
                .filter(|row| *row >= self.scroll && *row < bottom)
                .collect::<Vec<_>>();
            let target = if code == KeyCode::Up {
                visible_rows.last().copied().or_else(|| {
                    target_rows
                        .iter()
                        .copied()
                        .rev()
                        .find(|row| *row < self.scroll)
                })
            } else if code == KeyCode::Down {
                visible_rows
                    .first()
                    .copied()
                    .or_else(|| target_rows.iter().copied().find(|row| *row >= bottom))
            } else {
                visible_rows.first().copied().or_else(|| {
                    target_rows
                        .iter()
                        .copied()
                        .min_by_key(|row| row.abs_diff(self.scroll))
                })
            };
            let position = target.map(|row| (target_column, row)).or_else(|| {
                if code == KeyCode::Up {
                    target_column
                        .checked_sub(1)
                        .and_then(|column| selectable(column).last().map(|row| (column, *row)))
                } else if code == KeyCode::Down && target_column + 1 < panel.columns.len() {
                    Some((target_column + 1, 0))
                } else {
                    target_rows
                        .iter()
                        .copied()
                        .min_by_key(|row| row.abs_diff(self.scroll))
                        .map(|row| (target_column, row))
                }
            });
            if let Some((column, row)) = position {
                self.selected = panel.columns[column][row].id.clone();
                self.manual_scroll = false;
                self.panel(snapshot, changes, width, rows);
            }
            return true;
        }
        let target = match code {
            KeyCode::Up => current_rows
                .iter()
                .rev()
                .find(|candidate| **candidate < row)
                .map(|row| (column, *row))
                .or_else(|| {
                    column
                        .checked_sub(1)
                        .and_then(|column| selectable(column).last().map(|row| (column, *row)))
                }),
            KeyCode::Down => current_rows
                .iter()
                .find(|candidate| **candidate > row)
                .map(|row| (column, *row))
                .or_else(|| (column + 1 < panel.columns.len()).then(|| (column + 1, 0))),
            KeyCode::Left | KeyCode::Right => {
                let adjacent = if code == KeyCode::Left {
                    column.checked_sub(1)
                } else {
                    (column + 1 < panel.columns.len()).then_some(column + 1)
                };
                adjacent.and_then(|column| {
                    selectable(column)
                        .into_iter()
                        .min_by_key(|candidate| candidate.abs_diff(row))
                        .map(|row| (column, row))
                })
            }
            KeyCode::Home => current_rows.first().map(|row| (column, *row)),
            KeyCode::End => current_rows.last().map(|row| (column, *row)),
            KeyCode::Char(' ') => {
                if let EntryId::Heading(category) = id {
                    if !self.collapsed.remove(&category) {
                        self.collapsed.insert(category);
                    }
                } else if let Some((array, key)) = id.variable() {
                    if let Some(index) = self
                        .pins
                        .iter()
                        .position(|pin| pin.key == key && pin.array == array)
                    {
                        self.pins.remove(index);
                        if id.category() == Category::Pinned {
                            self.selected = Some(
                                self.pins
                                    .get(index.min(self.pins.len().saturating_sub(1)))
                                    .map_or(EntryId::Heading(Category::Pinned), |pin| {
                                        pin.entry_id()
                                    }),
                            );
                        }
                    } else {
                        let variable = if array {
                            snapshot.array_elements.iter().find(|variable| {
                                array_base(&variable.name).eq_ignore_ascii_case(key)
                            })
                        } else {
                            snapshot
                                .variables
                                .iter()
                                .find(|variable| variable.name.eq_ignore_ascii_case(key))
                        };
                        if let Some(variable) = variable {
                            self.pins.push(Pin {
                                key: key.to_string(),
                                array,
                                display_name: if array {
                                    format!("{}()", array_base(&variable.name))
                                } else {
                                    variable.name.clone()
                                },
                            });
                        }
                    }
                }
                None
            }
            _ => return false,
        };
        if let Some((column, row)) = target {
            self.selected = panel.columns[column][row].id.clone();
        }
        self.manual_scroll = false;
        self.panel(snapshot, changes, width, rows);
        true
    }
}

impl InspectorPanel {
    pub(super) fn edit_cursor(&self) -> Option<(usize, usize)> {
        self.edit.as_ref().map(|edit| {
            let column_width = debug_panel_column_width(self.width, self.columns.len());
            (
                edit.column * (column_width + 3) + edit.start + edit.field.cursor,
                edit.row,
            )
        })
    }
    fn height(&self) -> usize {
        self.columns.iter().map(Vec::len).max().unwrap_or(0)
    }

    fn position(&self, id: &EntryId) -> Option<(usize, usize)> {
        self.columns
            .iter()
            .enumerate()
            .find_map(|(column, entries)| {
                entries
                    .iter()
                    .position(|entry| entry.id.as_ref() == Some(id) && id.selectable())
                    .map(|row| (column, row))
            })
    }

    pub(super) fn render_lines(&self, ansi: bool, theme: SyntaxTheme) -> Vec<String> {
        let column_width = debug_panel_column_width(self.width, self.columns.len());
        (0..self.height())
            .map(|row| {
                let mut line = String::new();
                let mut used = 0;
                for (column, entries) in self.columns.iter().enumerate() {
                    if column > 0 {
                        let gap = fit_plain_text(" | ", self.width.saturating_sub(used).min(3));
                        used += gap.chars().count();
                        line.push_str(&gap);
                    }
                    let width = column_width.min(self.width.saturating_sub(used));
                    if width == 0 {
                        continue;
                    }
                    let entry = entries.get(row);
                    let selected =
                        entry.is_some_and(|entry| entry.id.is_some() && entry.id == self.selected);
                    let header = entry.and_then(|entry| match &entry.id {
                        Some(EntryId::Heading(category)) => Some(*category),
                        _ => None,
                    });
                    let text = entry.map_or_else(String::new, |entry| {
                        if selected && !ansi {
                            if let Some(category) = header {
                                return entry.text.replacen(
                                    category.title(),
                                    &format!("[{}]", category.title()),
                                    1,
                                );
                            }
                        }
                        entry.text.clone()
                    });
                    let plain = fit_plain_text(
                        &format!(
                            "{}{}",
                            if selected && header.is_none() {
                                '▶'
                            } else {
                                ' '
                            },
                            text
                        ),
                        width,
                    );
                    if ansi {
                        if selected {
                            line.push_str("\x1b[4m");
                        }
                        if let Some(entry) = entry {
                            if entry.changed {
                                line.push_str(debug_panel_changed_style(theme));
                            } else if matches!(entry.id, Some(EntryId::Heading(_))) {
                                line.push_str(DEBUG_PANEL_HEADER_STYLE);
                            }
                        }
                    }
                    if let Some(edit) = self
                        .edit
                        .as_ref()
                        .filter(|edit| edit.column == column && edit.row == row)
                    {
                        line.push('▶');
                        line.push_str(&edit.prefix);
                        line.push_str(&edit.field.render(ansi));
                    } else {
                        line.push_str(&plain);
                    }
                    if ansi
                        && (selected
                            || entry.is_some_and(|entry| {
                                entry.changed || matches!(entry.id, Some(EntryId::Heading(_)))
                            }))
                    {
                        line.push_str(RESET);
                    }
                    used += width;
                }
                line.push_str(&" ".repeat(self.width.saturating_sub(used)));
                line
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debugger::{
        DebugArrayKind, DebugArraySummary, DebugLocation, DebugPauseReason, DebugValue,
        DebugVariable,
    };

    fn snapshot() -> DebugSnapshot {
        DebugSnapshot {
            reason: DebugPauseReason::Step,
            location: DebugLocation {
                line: 10,
                statement: 0,
                source_span: None,
                source: "10 A=1".into(),
                command: "A=1".into(),
            },
            source_lines: vec!["10 A=1".into()],
            variables: vec![
                DebugVariable {
                    name: "A".into(),
                    value: DebugValue::Number(1.0),
                },
                DebugVariable {
                    name: "Name$".into(),
                    value: DebugValue::String("X | Y".into()),
                },
            ],
            array_elements: vec![DebugVariable {
                name: "P(7)".into(),
                value: DebugValue::Number(19.0),
            }],
            arrays: vec![DebugArraySummary {
                name: "P".into(),
                alias_of: Some("A".into()),
                kind: DebugArrayKind::Number,
                dimensions: vec![20],
                elements: 21,
            }],
            stack: vec![],
            err: 0,
            erl: 0,
            timers: vec![],
            data: DebugDataSnapshot::Empty,
        }
    }

    fn state_at(id: EntryId) -> InspectorState {
        InspectorState {
            focused: true,
            selected: Some(id),
            ..InspectorState::default()
        }
    }

    fn key(state: &mut InspectorState, snapshot: &DebugSnapshot, code: KeyCode) {
        assert!(state.handle_key(code, snapshot, &DebugPanelChanges::default(), 42, 8));
    }

    fn text(state: &mut InspectorState, snapshot: &DebugSnapshot) -> String {
        state
            .panel(snapshot, &DebugPanelChanges::default(), 42, 8)
            .render_lines(false, SyntaxTheme::Dark)
            .join("\n")
    }

    fn strip_sgr(text: &str) -> String {
        let mut result = String::new();
        let mut escape = false;
        for ch in text.chars() {
            if ch == '\x1b' {
                escape = true;
            } else if escape {
                if ch == 'm' {
                    escape = false;
                }
            } else {
                result.push(ch);
            }
        }
        result
    }

    #[test]
    fn every_category_folds_to_a_counted_heading() {
        let snapshot = snapshot();
        let mut state = InspectorState {
            focused: true,
            ..InspectorState::default()
        };
        for category in [
            Category::Pinned,
            Category::Variables,
            Category::Arrays,
            Category::Stack,
            Category::State,
            Category::Timers,
            Category::Data,
        ] {
            state.selected = Some(EntryId::Heading(category));
            key(&mut state, &snapshot, KeyCode::Char(' '));
            assert_eq!(state.status_hint(), "Space Expand");
        }
        let rendered = text(&mut state, &snapshot);
        assert_eq!(
            rendered
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count(),
            7
        );
        assert!(rendered.contains("▸ VARIABLES (3)"));
        assert!(!rendered.contains("A = 1"));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(state.status_hint(), "Space Collapse");
    }

    #[test]
    fn enter_is_inert_on_headings_and_variables_even_with_a_displaced_viewport() {
        let snapshot = snapshot();
        let changes = DebugPanelChanges::default();
        for selected in [
            EntryId::Heading(Category::Variables),
            EntryId::Scalar(Category::Variables, "A".into()),
            EntryId::ArrayElement(Category::Variables, "P".into()),
        ] {
            let mut state = state_at(selected.clone());
            state.panel(&snapshot, &changes, 42, 4);
            assert!(!state.handle_key(KeyCode::Enter, &snapshot, &changes, 42, 4));
            assert!(state.collapsed.is_empty());
            assert!(state.pins.is_empty());
            key(&mut state, &snapshot, KeyCode::Char(' '));
            let collapsed = state.collapsed.clone();
            let pin_count = state.pins.len();
            assert!(pin_count == 1 || collapsed.contains(&Category::Variables));
            for manual_scroll in [false, true] {
                state.manual_scroll = manual_scroll;
                state.scroll = 999;
                let hint = state.hint;
                assert!(!state.handle_key(KeyCode::Enter, &snapshot, &changes, 42, 4));
                assert_eq!(state.scroll, 999);
                assert_eq!(state.manual_scroll, manual_scroll);
                assert_eq!(state.selected, Some(selected.clone()));
                assert_eq!(state.hint, hint);
                assert_eq!(state.collapsed, collapsed);
                assert_eq!(state.pins.len(), pin_count);
            }
            state.manual_scroll = false;
            state.scroll = 0;
            key(&mut state, &snapshot, KeyCode::Char(' '));
            assert!(state.collapsed.is_empty());
            assert!(state.pins.is_empty());
        }
    }

    #[test]
    fn pinning_is_case_insensitive_stable_and_refreshes_without_evaluation() {
        let mut snapshot = snapshot();
        let selected = EntryId::Scalar(Category::Variables, "A".into());
        let mut state = state_at(selected.clone());
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(state.selected, Some(selected));
        assert_eq!(state.status_hint(), "Space Unpin");
        state.selected = Some(EntryId::Scalar(Category::Variables, "NAME$".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(
            state
                .pins
                .iter()
                .map(|pin| pin.key.as_str())
                .collect::<Vec<_>>(),
            ["A", "NAME$"]
        );
        assert_eq!(text(&mut state, &snapshot).matches("[+] A = 1").count(), 1);
        snapshot.variables[0].name = "a".into();
        snapshot.variables[0].value = DebugValue::Number(2.0);
        let changes = DebugPanelChanges {
            variables: HashSet::from(["A".into()]),
            ..DebugPanelChanges::default()
        };
        let rendered = state
            .panel(&snapshot, &changes, 42, 8)
            .render_lines(false, SyntaxTheme::Dark)
            .join("\n");
        assert_eq!(rendered.matches("* [+] a = 2").count(), 1);
        assert!(rendered.contains("* a = 2"));
        snapshot.variables.remove(0);
        assert!(text(&mut state, &snapshot).contains("A = <unavailable>"));
        state.selected = Some(EntryId::Scalar(Category::Pinned, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(
            state.selected,
            Some(EntryId::Scalar(Category::Pinned, "NAME$".into()))
        );
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Pinned)));
        assert!(state.pins.is_empty());
    }

    #[test]
    fn a_pinned_name_survives_scope_exit_and_reads_the_returning_scalar() {
        let mut snapshot = snapshot();
        let mut state = state_at(EntryId::Scalar(Category::Variables, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        state.selected = Some(EntryId::Scalar(Category::Pinned, "A".into()));
        snapshot.variables.clear();
        assert!(text(&mut state, &snapshot).contains("A = <unavailable>"));
        assert_eq!(
            state.selected,
            Some(EntryId::Scalar(Category::Pinned, "A".into()))
        );
        // An array with the same base name cannot supply a pinned scalar value.
        snapshot.array_elements[0].name = "A(7)".into();
        assert!(text(&mut state, &snapshot).contains("A = <unavailable>"));
        snapshot.variables.push(DebugVariable {
            name: "a".into(),
            value: DebugValue::Number(23.0),
        });
        let rendered = text(&mut state, &snapshot);
        assert!(rendered.contains("[+] a = 23"));
        assert!(!rendered.contains("<unavailable>"));
        assert_eq!(state.pins.len(), 1);
    }

    #[test]
    fn array_pins_follow_the_last_element_without_selecting_array_summaries() {
        let mut snapshot = snapshot();
        let mut state = state_at(EntryId::ArrayElement(Category::Variables, "P".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(state.pins.len(), 1);
        snapshot.array_elements[0].name = "p(12)".into();
        snapshot.array_elements[0].value = DebugValue::Number(8.0);
        state.panel(&snapshot, &DebugPanelChanges::default(), 42, 8);
        assert_eq!(
            state.selected,
            Some(EntryId::ArrayElement(Category::Variables, "P".into()))
        );
        assert!(text(&mut state, &snapshot).contains("P(12) = 8"));
        assert!(!text(&mut state, &snapshot).contains("P(7)"));
        snapshot.array_elements.clear();
        assert!(text(&mut state, &snapshot).contains("P() = <unavailable>"));
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Variables)));
        state.selected = Some(EntryId::Array("P".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Arrays)));
        assert_eq!(state.pins.len(), 1);
        state.selected = Some(EntryId::Scalar(Category::Variables, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(state.pins.len(), 2);
    }

    #[test]
    fn navigation_skips_padding_and_survives_columns_resizing_and_missing_entries() {
        let mut snapshot = snapshot();
        let mut state = state_at(EntryId::Heading(Category::Pinned));
        key(&mut state, &snapshot, KeyCode::Down);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Variables)));
        for index in 0..30 {
            snapshot.variables.push(DebugVariable {
                name: format!("V{index}"),
                value: DebugValue::Number(index as f64),
            });
        }
        state.selected = Some(EntryId::Scalar(Category::Variables, "V29".into()));
        for width in [42, 80, 120, 42] {
            let panel = state.panel(&snapshot, &DebugPanelChanges::default(), width, 5);
            let (_, row) = panel.position(state.selected.as_ref().unwrap()).unwrap();
            assert!(row >= state.scroll && row < state.scroll + 5);
            assert_eq!(
                state.selected,
                Some(EntryId::Scalar(Category::Variables, "V29".into()))
            );
        }
        let old_scroll = state.scroll;
        state.panel(&snapshot, &DebugPanelChanges::default(), 0, 0);
        assert_eq!(state.scroll, old_scroll);
        assert!(!state.handle_key(
            KeyCode::Down,
            &snapshot,
            &DebugPanelChanges::default(),
            0,
            0
        ));
        assert_eq!(
            state.selected,
            Some(EntryId::Scalar(Category::Variables, "V29".into()))
        );
        state.handle_key(
            KeyCode::Right,
            &snapshot,
            &DebugPanelChanges::default(),
            120,
            5,
        );
        assert_eq!(state.selected, Some(EntryId::Heading(Category::State)));
        state.handle_key(
            KeyCode::Home,
            &snapshot,
            &DebugPanelChanges::default(),
            120,
            5,
        );
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Stack)));
        state.handle_key(
            KeyCode::Left,
            &snapshot,
            &DebugPanelChanges::default(),
            120,
            5,
        );
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Pinned)));
        state.selected = Some(EntryId::Scalar(Category::Variables, "V29".into()));
        snapshot.variables.pop();
        state.panel(&snapshot, &DebugPanelChanges::default(), 42, 5);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Variables)));
    }

    #[test]
    fn all_actionable_entries_are_reachable_with_arrows() {
        let snapshot = snapshot();
        let changes = DebugPanelChanges::default();
        let mut state = state_at(EntryId::Heading(Category::Pinned));
        let panel = state.panel(&snapshot, &changes, 120, 3);
        let expected = panel
            .columns
            .iter()
            .flat_map(|column| {
                column
                    .iter()
                    .filter_map(|entry| entry.id.clone().filter(EntryId::selectable))
            })
            .collect::<Vec<_>>();
        for id in &expected {
            assert_eq!(state.selected.as_ref(), Some(id));
            assert!(state.handle_key(KeyCode::Down, &snapshot, &changes, 120, 3));
        }
        assert_eq!(state.selected.as_ref(), expected.last());
        for id in expected.iter().rev() {
            assert_eq!(state.selected.as_ref(), Some(id));
            assert!(state.handle_key(KeyCode::Up, &snapshot, &changes, 120, 3));
        }
        state.handle_key(KeyCode::PageDown, &snapshot, &changes, 120, usize::MAX);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Pinned)));
        state.handle_key(KeyCode::PageUp, &snapshot, &changes, 120, usize::MAX);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Pinned)));
        state.handle_key(KeyCode::End, &snapshot, &changes, 120, 3);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Arrays)));
        state.handle_key(KeyCode::Home, &snapshot, &changes, 120, 3);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Pinned)));
        state.focused = false;
        assert!(!state.handle_key(KeyCode::Down, &snapshot, &changes, 120, 3));
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Pinned)));
    }

    #[test]
    fn scalar_and_array_pins_are_independent_and_aliases_use_only_the_current_scope() {
        let mut snapshot = snapshot();
        let summary = |name: &str, alias: Option<&str>| DebugArraySummary {
            name: name.into(),
            alias_of: alias.map(str::to_string),
            kind: DebugArrayKind::Number,
            dimensions: vec![20],
            elements: 21,
        };
        snapshot.arrays = vec![
            summary("A", None),
            summary("P", Some("A")),
            summary("B", None),
        ];
        snapshot.array_elements[0].name = "A(7)".into();
        let mut state = state_at(EntryId::Scalar(Category::Variables, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        state.selected = Some(EntryId::ArrayElement(Category::Variables, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        snapshot.array_elements[0].name = "P(7)".into();
        state.selected = Some(EntryId::ArrayElement(Category::Variables, "P".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert_eq!(
            state
                .pins
                .iter()
                .map(|pin| (pin.key.as_str(), pin.array))
                .collect::<Vec<_>>(),
            [("A", false), ("A", true), ("P", true)]
        );

        snapshot.array_elements[0].name = "p(12)".into();
        snapshot.array_elements[0].value = DebugValue::Number(8.0);
        let changes = DebugPanelChanges {
            variables: HashSet::from(["P(12)".into()]),
            ..DebugPanelChanges::default()
        };
        let panel = state.panel(&snapshot, &changes, 42, 8);
        let rendered = panel.render_lines(false, SyntaxTheme::Dark).join("\n");
        assert!(rendered.contains("* A(12) = 8"));
        assert!(rendered.contains("* P(12) = 8"));
        assert!(rendered.contains("* [+] p(12) = 8"));
        let pins = &panel.columns[0][1..4];
        assert!(pins.iter().all(|entry| !entry.text.contains("[+]")));
        assert!(!pins[0].changed);
        assert!(pins[1].changed && pins[2].changed);

        snapshot.arrays[1].alias_of = Some("B".into());
        snapshot.array_elements = vec![
            DebugVariable {
                name: "A(2)".into(),
                value: crate::debugger::DebugValue::Number(11.0),
            },
            DebugVariable {
                name: "B(3)".into(),
                value: crate::debugger::DebugValue::Number(31.0),
            },
        ];
        let rendered = text(&mut state, &snapshot);
        assert!(rendered.contains("A(2) = 11"));
        assert!(rendered.contains("P(3) = 31"));
        snapshot.arrays.remove(1);
        assert!(text(&mut state, &snapshot).contains("P() = <unavailable>"));
        snapshot.arrays.push(summary("P", Some("P")));
        snapshot.array_elements.push(DebugVariable {
            name: "P(4)".into(),
            value: crate::debugger::DebugValue::Number(44.0),
        });
        assert!(text(&mut state, &snapshot).contains("P(4) = 44"));
        snapshot.array_elements.clear();
        let rendered = text(&mut state, &snapshot);
        assert!(rendered.contains("A() = <unavailable>"));
        assert!(rendered.contains("P() = <unavailable>"));
        assert!(rendered.contains("A = 1"));
        state.selected = Some(EntryId::ArrayElement(Category::Pinned, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        assert!(state.is_pinned("A", false));
        assert!(!state.is_pinned("A", true));
        assert!(state.is_pinned("P", true));
    }

    #[test]
    fn paging_reads_all_information_rows_without_activating_hidden_selections() {
        let mut snapshot = snapshot();
        snapshot.variables.clear();
        snapshot.array_elements.clear();
        snapshot.arrays.clear();
        snapshot.stack = (0..80)
            .map(|index| crate::debugger::DebugStackFrame {
                kind: crate::debugger::DebugFrameKind::Gosub,
                name: None,
                line: Some(100 + index),
            })
            .collect();
        let changes = DebugPanelChanges::default();
        for width in [42, 80, 120] {
            let mut state = state_at(EntryId::Heading(Category::Stack));
            let mut seen = String::new();
            loop {
                let panel = state.panel(&snapshot, &changes, width, 5);
                seen.push_str(
                    &panel
                        .render_lines(false, SyntaxTheme::Dark)
                        .iter()
                        .skip(state.scroll)
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                let before = state.scroll;
                state.handle_key(KeyCode::PageDown, &snapshot, &changes, width, 5);
                if state.scroll == before {
                    break;
                }
            }
            for index in 0..80 {
                assert!(
                    seen.contains(&format!("{index}: GOSUB @{}", 100 + index)),
                    "missing frame {index}, width {width}"
                );
            }
            assert_eq!(state.selected, Some(EntryId::Heading(Category::Stack)));
            assert_eq!(state.status_hint(), "");
            let before = state.scroll;
            let collapsed = state.collapsed.clone();
            state.handle_key(KeyCode::Enter, &snapshot, &changes, width, 5);
            state.handle_key(KeyCode::Char(' '), &snapshot, &changes, width, 5);
            assert_eq!(state.scroll, before);
            assert_eq!(state.collapsed, collapsed);
            assert!(state.pins.is_empty());
            for _ in 0..3 {
                state.panel(&snapshot, &changes, width, 5);
                assert_eq!(state.scroll, before);
            }
            let mut changed_snapshot = snapshot.clone();
            changed_snapshot.err = 5;
            state.panel(&changed_snapshot, &changes, width, 5);
            assert_eq!(state.scroll, before);
            state.panel(&changed_snapshot, &changes, 0, 0);
            assert_eq!(state.scroll, before);
            state.panel(&changed_snapshot, &changes, width, 4);
            assert_eq!(state.scroll, before);
            state.focused = false;
            state.panel(&snapshot, &changes, width, 5);
            state.focused = true;
            state.panel(&snapshot, &changes, width, 5);
            assert_eq!(state.scroll, before);

            state.scroll = 20;
            state.panel(&snapshot, &changes, width, 5);
            state.handle_key(KeyCode::Down, &snapshot, &changes, width, 5);
            assert_eq!(state.selected, Some(EntryId::Heading(Category::State)));
            assert!(state
                .panel(&snapshot, &changes, width, 5)
                .selected
                .is_some());
            state.selected = Some(EntryId::Heading(Category::Stack));
            state.manual_scroll = true;
            state.scroll = 20;
            state.handle_key(KeyCode::Up, &snapshot, &changes, width, 5);
            assert_eq!(state.selected, Some(EntryId::Heading(Category::Stack)));
            assert!(state
                .panel(&snapshot, &changes, width, 5)
                .selected
                .is_some());
        }
    }

    #[test]
    fn a_hidden_variable_cannot_be_pinned_and_page_mode_survives_reconciliation() {
        let mut snapshot = snapshot();
        for index in 0..60 {
            snapshot.stack.push(crate::debugger::DebugStackFrame {
                kind: crate::debugger::DebugFrameKind::Gosub,
                name: None,
                line: Some(index),
            });
        }
        let changes = DebugPanelChanges::default();
        let mut state = state_at(EntryId::Scalar(Category::Variables, "A".into()));
        state.panel(&snapshot, &changes, 120, 4);
        state.handle_key(KeyCode::PageDown, &snapshot, &changes, 120, 4);
        state.handle_key(KeyCode::PageDown, &snapshot, &changes, 120, 4);
        let scroll = state.scroll;
        assert_eq!(state.status_hint(), "");
        state.handle_key(KeyCode::Char(' '), &snapshot, &changes, 120, 4);
        assert!(state.pins.is_empty());
        snapshot.variables.clear();
        state.panel(&snapshot, &changes, 120, 4);
        assert_eq!(state.selected, Some(EntryId::Heading(Category::Variables)));
        assert_eq!(state.scroll, scroll);
        state.panel(&snapshot, &changes, 80, 4);
        assert_eq!(state.scroll, scroll);
        state.handle_key(KeyCode::Left, &snapshot, &changes, 80, 4);
        assert!(state.panel(&snapshot, &changes, 80, 4).selected.is_some());
    }

    #[test]
    fn selection_marker_is_reserved_for_pinnable_values_and_plain_headers_use_brackets() {
        let snapshot = snapshot();
        let changes = DebugPanelChanges::default();
        for category in [
            Category::Pinned,
            Category::Variables,
            Category::Arrays,
            Category::Stack,
            Category::State,
            Category::Timers,
            Category::Data,
        ] {
            let mut state = state_at(EntryId::Heading(category));
            let panel = state.panel(&snapshot, &changes, 120, 20);
            let plain = panel.render_lines(false, SyntaxTheme::Dark).join("\n");
            assert!(plain.contains(&format!("[{}]", category.title())));
            assert!(!plain.contains('▶'));
            for theme in [SyntaxTheme::Dark, SyntaxTheme::Light] {
                let styled = panel.render_lines(true, theme).join("\n");
                assert!(styled.contains("\x1b[4m"));
                assert!(!styled.contains('▶'));
                assert!(!styled.contains(&format!("[{}]", category.title())));
            }
        }
        for selected in [
            EntryId::Scalar(Category::Variables, "A".into()),
            EntryId::ArrayElement(Category::Variables, "P".into()),
        ] {
            let mut state = state_at(selected);
            for width in [0, 1, 42, 80, 120] {
                let panel = state.panel(&snapshot, &changes, width, 8);
                let plain = panel.render_lines(false, SyntaxTheme::Dark).join("\n");
                assert_eq!(plain.matches('▶').count(), usize::from(width > 0));
            }
        }
    }

    #[test]
    fn inline_editing_is_opt_in_and_proposes_literal_values_without_mutating_snapshot() {
        let snapshot = snapshot();
        let original = snapshot.clone();
        let changes = DebugPanelChanges::default();
        let mut state = state_at(EntryId::Scalar(Category::Variables, "A".into()));
        assert!(!state.begin_edit(&snapshot, &changes, 42, 8));
        state.set_editable(true);
        state.panel(&snapshot, &changes, 42, 8);
        assert_eq!(state.status_hint(), "Enter Edit Space Pin");
        assert!(state.begin_edit(&snapshot, &changes, 42, 8));
        assert!(state.is_editing());
        assert_eq!(state.status_hint(), "Enter Apply Esc Cancel");
        assert!(!state.handle_key(KeyCode::Char(' '), &snapshot, &changes, 42, 8));
        assert!(state.pins.is_empty());
        state.handle_edit_key(KeyEvent::new(KeyCode::Char('9'), KeyModifiers::NONE));
        assert_eq!(
            state.handle_edit_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some((DebugEditTarget::Scalar("A".into()), DebugValue::Number(9.0)))
        );
        assert_eq!(snapshot, original);
        assert!(state.is_editing());
        state.edit_failed("Rejected\x1b[31m".into());
        assert!(!state.edit_status().unwrap().contains('\x1b'));
        assert!(state.is_editing());
        state.edit_committed();
        assert!(!state.is_editing());
        assert!(state.edit_status().is_none());
        assert!(state
            .panel(&snapshot, &changes, 42, 8)
            .edit_cursor()
            .is_none());
    }

    #[test]
    fn inline_edit_cancel_invalid_input_and_scope_checks_keep_debugger_state() {
        let mut snapshot = snapshot();
        let changes = DebugPanelChanges::default();
        let mut state = state_at(EntryId::Heading(Category::Variables));
        state.set_editable(true);
        assert!(!state.begin_edit(&snapshot, &changes, 42, 8));
        state.selected = Some(EntryId::Scalar(Category::Variables, "A".into()));
        key(&mut state, &snapshot, KeyCode::Char(' '));
        state.selected = Some(EntryId::Scalar(Category::Pinned, "A".into()));
        assert!(state.begin_edit(&snapshot, &changes, 42, 8));
        let scroll = state.scroll;
        state.handle_edit_paste("1+2");
        assert!(state
            .handle_edit_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .is_none());
        assert!(state.edit_status().unwrap().contains("finite decimal"));
        state.handle_edit_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!state.is_editing());
        assert!(state.is_pinned("A", false));
        assert_eq!(state.scroll, scroll);
        assert_eq!(snapshot.variables[0].value, DebugValue::Number(1.0));
        snapshot.variables.remove(0);
        assert!(!state.begin_edit(&snapshot, &changes, 42, 8));
        assert_eq!(state.status_hint(), "Space Unpin");
        state.selected = Some(EntryId::ArrayElement(Category::Variables, "P".into()));
        assert!(state.begin_edit(&snapshot, &changes, 42, 8));
        state.set_editable(false);
        assert!(!state.is_editing());
    }

    #[test]
    fn inline_array_edit_captures_visible_name_and_exact_indexes_even_for_pinned_aliases() {
        let mut snapshot = snapshot();
        snapshot.arrays.push(DebugArraySummary {
            name: "A".into(),
            alias_of: None,
            kind: DebugArrayKind::Number,
            dimensions: vec![20],
            elements: 21,
        });
        snapshot.array_elements[0].name = "A(7)".into();
        let changes = DebugPanelChanges::default();
        let mut state = state_at(EntryId::ArrayElement(Category::Variables, "A".into()));
        state.set_editable(true);
        key(&mut state, &snapshot, KeyCode::Char(' '));
        state.selected = Some(EntryId::ArrayElement(Category::Pinned, "A".into()));
        snapshot.array_elements[0].name = "P(7)".into();
        assert!(state.begin_edit(&snapshot, &changes, 42, 8));
        state.handle_edit_paste("17");
        // Rebuilding a panel cannot retarget an edit to a different array index.
        snapshot.array_elements[0].name = "P(12)".into();
        let panel = state.panel(&snapshot, &changes, 42, 8);
        let edit_row = panel.edit_cursor().unwrap().1;
        let edited_line = &panel.render_lines(false, SyntaxTheme::Dark)[edit_row];
        assert!(edited_line.contains("A(7) = 17"));
        assert_eq!(
            state.handle_edit_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some((
                DebugEditTarget::ArrayElement {
                    name: "A".into(),
                    indexes: vec![7]
                },
                DebugValue::Number(17.0)
            ))
        );
        state.edit_committed();
        snapshot.array_elements.clear();
        assert!(!state.begin_edit(&snapshot, &changes, 42, 8));
        assert_eq!(state.status_hint(), "Space Unpin");
    }

    #[test]
    fn inline_field_stays_within_its_cell_and_preserves_buffer_across_hidden_resizes() {
        let mut snapshot = snapshot();
        snapshot.variables[1].value =
            DebugValue::String("0123456789abcdefghijklmnopqrstuvwxyz".into());
        let changes = DebugPanelChanges::default();
        let mut state = state_at(EntryId::Scalar(Category::Variables, "NAME$".into()));
        state.set_editable(true);
        assert!(state.begin_edit(&snapshot, &changes, 120, 8));
        let initial_scroll = state.scroll;
        for width in [42, 80, 120] {
            let panel = state.panel(&snapshot, &changes, width, 8);
            let (x, y) = panel.edit_cursor().unwrap();
            assert!(x < debug_panel_column_width(width, debug_panel_column_count(width)));
            assert!(y >= state.scroll && y < state.scroll + 8);
            let plain = panel.render_lines(false, SyntaxTheme::Dark);
            for theme in [SyntaxTheme::Dark, SyntaxTheme::Light] {
                for (plain, styled) in plain.iter().zip(panel.render_lines(true, theme)) {
                    assert_eq!(plain.chars().count(), width);
                    assert_eq!(strip_sgr(&styled), *plain);
                }
            }
        }
        assert_eq!(state.scroll, initial_scroll);
        let hidden = state.panel(&snapshot, &changes, 0, 0);
        assert!(hidden.edit_cursor().is_none());
        state.handle_edit_paste("lost");
        state.handle_edit_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(state
            .handle_edit_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .is_none());
        assert!(state.is_editing());
        assert!(state
            .panel(&snapshot, &changes, 1, 8)
            .edit_cursor()
            .is_none());
        assert!(state.is_editing());
        state.panel(&snapshot, &changes, 120, 8);
        assert_eq!(
            state
                .handle_edit_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
                .unwrap()
                .1,
            snapshot.variables[1].value
        );
        state.handle_edit_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        state.selected = Some(EntryId::Scalar(Category::Variables, "A".into()));
        state.manual_scroll = true;
        state.scroll = 8;
        assert!(!state.begin_edit(&snapshot, &changes, 42, 2));
        assert_eq!(state.scroll, 8);
    }

    #[test]
    fn rendering_uses_cell_metadata_preserves_width_and_resets_styles() {
        let snapshot = snapshot();
        let changes = DebugPanelChanges {
            variables: HashSet::from(["NAME$".into()]),
            state: true,
            ..DebugPanelChanges::default()
        };
        let mut state = state_at(EntryId::Scalar(Category::Variables, "NAME$".into()));
        for width in [0, 1, 42, 60, 80, 96, 120] {
            let panel = state.panel(&snapshot, &changes, width, 8);
            let plain = panel.render_lines(false, SyntaxTheme::Dark);
            for theme in [SyntaxTheme::Dark, SyntaxTheme::Light] {
                let styled = panel.render_lines(true, theme);
                for (plain, styled) in plain.iter().zip(styled) {
                    assert_eq!(plain.chars().count(), width);
                    assert_eq!(strip_sgr(&styled), *plain);
                    if styled.contains('\x1b') {
                        assert!(
                            styled.rfind(RESET).unwrap() > styled.rfind("\x1b[4m").unwrap_or(0)
                        );
                    }
                }
            }
        }
        let panel = state.panel(&snapshot, &changes, 120, 8);
        let styled = panel.render_lines(true, SyntaxTheme::Dark);
        let selected = styled.iter().find(|line| line.contains("Name$")).unwrap();
        assert!(selected.contains("X | Y"));
        assert!(selected.contains("\x1b[4m"));
        assert!(selected.contains(debug_panel_changed_style(SyntaxTheme::Dark)));
        assert!(selected.contains(&format!("{RESET} | ")));
        state.focused = false;
        let unfocused = state
            .panel(&snapshot, &changes, 120, 8)
            .render_lines(true, SyntaxTheme::Dark)
            .join("\n");
        assert!(!unfocused.contains("\x1b[4m"));
        assert_eq!(state.status_hint(), "");
    }
}
