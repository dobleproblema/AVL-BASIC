//! Selection of runtime-provided, complete statements in the debugger source
//! view. The UI never reparses BASIC or invents executable destinations.
use crate::debugger::DebugStatementTarget;
use std::ops::Range;

#[derive(Debug, Default)]
pub(super) struct DebugCodeSelection {
    targets: Vec<DebugStatementTarget>,
    line: Option<i32>,
    selected: Option<usize>,
}

impl DebugCodeSelection {
    pub(super) fn new(targets: &[DebugStatementTarget]) -> Self {
        let mut selection = Self::default();
        selection.replace_targets(targets);
        selection
    }

    pub(super) fn replace_targets(&mut self, targets: &[DebugStatementTarget]) {
        let previous = self.selected().cloned();
        self.targets = targets
            .iter()
            .filter(|target| target.source_span.start < target.source_span.end)
            .cloned()
            .collect();
        self.targets.sort_by_key(|target| {
            (
                target.line,
                target.source_span.start,
                target.source_span.end,
            )
        });
        self.targets
            .dedup_by(|left, right| same_target(left, right));
        self.selected = previous
            .as_ref()
            .and_then(|previous| {
                self.targets
                    .iter()
                    .position(|target| same_target(target, previous))
            })
            .or_else(|| self.first_on_line());
    }

    /// Vertical navigation starts with the first statement of the new line.
    /// A preferred current execution span instead selects its complete enclosing
    /// destination, for example the outer IF when execution is inside THEN.
    pub(super) fn sync_line(&mut self, line: Option<i32>, prefer_span: Option<&Range<usize>>) {
        let changed_line = self.line != line;
        self.line = line;
        if let Some(span) = prefer_span.filter(|span| span.start < span.end) {
            self.selected = self
                .targets
                .iter()
                .enumerate()
                .filter(|(_, target)| {
                    Some(target.line) == line
                        && target.source_span.start <= span.start
                        && span.end <= target.source_span.end
                })
                .min_by_key(|(_, target)| target.source_span.end - target.source_span.start)
                .map(|(index, _)| index)
                .or_else(|| self.first_on_line());
        } else if changed_line || self.selected().is_none() {
            self.selected = self.first_on_line();
        }
    }

    pub(super) fn move_on_line(&mut self, forward: bool) -> bool {
        let candidates = self
            .targets
            .iter()
            .enumerate()
            .filter_map(|(index, target)| (Some(target.line) == self.line).then_some(index))
            .collect::<Vec<_>>();
        let Some(first) = candidates.first().copied() else {
            return false;
        };
        let next = self
            .selected
            .and_then(|selected| {
                candidates
                    .iter()
                    .position(|candidate| *candidate == selected)
            })
            .map_or(first, |position| {
                let next = if forward {
                    position.saturating_add(1).min(candidates.len() - 1)
                } else {
                    position.saturating_sub(1)
                };
                candidates[next]
            });
        let changed = self.selected != Some(next);
        self.selected = Some(next);
        changed
    }

    pub(super) fn selected(&self) -> Option<&DebugStatementTarget> {
        self.selected
            .and_then(|index| self.targets.get(index))
            .filter(|target| Some(target.line) == self.line)
    }

    #[cfg(test)]
    pub(super) fn ordinal(&self) -> Option<usize> {
        let selected = self.selected?;
        self.targets
            .iter()
            .enumerate()
            .filter(|(_, target)| Some(target.line) == self.line)
            .position(|(index, _)| index == selected)
            .map(|position| position + 1)
    }

    pub(super) fn selected_char_span(&self, source: &str) -> Option<Range<usize>> {
        statement_char_span(source, &self.selected()?.source_span)
    }

    fn first_on_line(&self) -> Option<usize> {
        self.targets
            .iter()
            .position(|target| Some(target.line) == self.line)
    }
}

fn same_target(left: &DebugStatementTarget, right: &DebugStatementTarget) -> bool {
    left.line == right.line && left.source_span == right.source_span
}

fn statement_char_span(source: &str, span: &Range<usize>) -> Option<Range<usize>> {
    if span.start >= span.end
        || span.end > source.len()
        || !source.is_char_boundary(span.start)
        || !source.is_char_boundary(span.end)
    {
        return None;
    }
    Some(source[..span.start].chars().count()..source[..span.end].chars().count())
}

/// Return character ranges within the rendered horizontal window. `marker`
/// describes an insertion at a source character offset and its displayed width
/// (currently two characters: the execution triangle plus its following space).
/// The inserted marker is excluded even when it lies inside a selected IF.
pub(super) fn visible_statement_ranges(
    source: &str,
    span: &Range<usize>,
    left: usize,
    width: usize,
    marker: Option<(usize, usize)>,
) -> Vec<Range<usize>> {
    let Some(selected) = statement_char_span(source, span) else {
        return Vec::new();
    };
    if width == 0 {
        return Vec::new();
    }
    let mut ranges = Vec::with_capacity(2);
    let mut append = |range: Range<usize>| {
        let start = range.start.max(left);
        let end = range.end.min(left.saturating_add(width));
        if start < end {
            ranges.push(start - left..end - left);
        }
    };
    if let Some((position, inserted_width)) = marker.filter(|(position, inserted_width)| {
        *position <= source.chars().count() && *inserted_width > 0
    }) {
        if selected.start < position {
            append(selected.start..selected.end.min(position));
        }
        if selected.end > position {
            append(
                selected.start.max(position).saturating_add(inserted_width)
                    ..selected.end.saturating_add(inserted_width),
            );
        }
    } else {
        append(selected);
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(line: i32, source: &str, statement: &str) -> DebugStatementTarget {
        let start = source.find(statement).unwrap();
        DebugStatementTarget {
            line,
            source_span: start..start + statement.len(),
        }
    }

    #[test]
    fn horizontal_selection_uses_only_supplied_statements_and_never_wraps() {
        let source = "10 A=1:IF A THEN GOSUB 100:PRINT Z ELSE PRINT Y:B=2";
        let first = target(10, source, "A=1");
        let whole_if = target(10, source, "IF A THEN GOSUB 100:PRINT Z ELSE PRINT Y:B=2");
        let mut selection = DebugCodeSelection::new(&[whole_if.clone(), first.clone()]);
        selection.sync_line(Some(10), None);
        assert!(same_target(selection.selected().unwrap(), &first));
        assert_eq!(selection.ordinal(), Some(1));
        assert!(!selection.move_on_line(false));
        assert!(selection.move_on_line(true));
        assert!(same_target(selection.selected().unwrap(), &whole_if));
        assert_eq!(selection.ordinal(), Some(2));
        assert!(!selection.move_on_line(true));
        assert!(selection.move_on_line(false));
        assert!(same_target(selection.selected().unwrap(), &first));
    }

    #[test]
    fn initial_nested_execution_selects_the_enclosing_top_level_if() {
        let source = "10 A=1:IF A THEN GOSUB 100:PRINT Z ELSE PRINT Y";
        let whole_if = target(10, source, "IF A THEN GOSUB 100:PRINT Z ELSE PRINT Y");
        let mut selection = DebugCodeSelection::new(&[target(10, source, "A=1"), whole_if.clone()]);
        for nested in ["GOSUB 100", "PRINT Z", "PRINT Y"] {
            let nested = target(10, source, nested);
            selection.sync_line(Some(10), Some(&nested.source_span));
            assert!(same_target(selection.selected().unwrap(), &whole_if));
        }
    }

    #[test]
    fn vertical_navigation_starts_at_first_statement_and_handles_non_executable_lines() {
        let one = "10 A=1:B=2";
        let two = "20 PRINT A:PRINT B";
        let targets = [
            target(10, one, "A=1"),
            target(10, one, "B=2"),
            target(20, two, "PRINT A"),
            target(20, two, "PRINT B"),
        ];
        let mut selection = DebugCodeSelection::new(&targets);
        selection.sync_line(Some(10), None);
        selection.move_on_line(true);
        selection.sync_line(Some(10), None);
        assert_eq!(selection.ordinal(), Some(2));
        selection.sync_line(Some(20), None);
        assert_eq!(selection.ordinal(), Some(1));
        selection.sync_line(Some(10), None);
        assert_eq!(selection.ordinal(), Some(1));
        for line in [None, Some(15), Some(100)] {
            selection.sync_line(line, None);
            assert!(selection.selected().is_none());
            assert!(selection.ordinal().is_none());
            assert!(!selection.move_on_line(true));
            assert!(!selection.move_on_line(false));
        }
    }

    #[test]
    fn refreshed_targets_preserve_identity_and_reconcile_removed_candidates() {
        let source = "10 A=1:B=2:C=3";
        let first = target(10, source, "A=1");
        let second = target(10, source, "B=2");
        let third = target(10, source, "C=3");
        let mut selection = DebugCodeSelection::new(&[first.clone(), second.clone()]);
        selection.sync_line(Some(10), None);
        selection.move_on_line(true);
        selection.replace_targets(&[third.clone(), first.clone(), second.clone(), first.clone()]);
        assert!(same_target(selection.selected().unwrap(), &second));
        assert_eq!(selection.ordinal(), Some(2));
        selection.replace_targets(&[first.clone(), third]);
        assert!(same_target(selection.selected().unwrap(), &first));
        selection.replace_targets(&[]);
        assert!(selection.selected().is_none());
    }

    #[test]
    fn source_offsets_are_utf8_bytes_but_view_ranges_are_character_columns() {
        let source = "10 PRINT \"á中🙂\":A=2";
        let statement = target(10, source, "A=2");
        let start = source[..statement.source_span.start].chars().count();
        let mut selection = DebugCodeSelection::new(&[statement.clone()]);
        selection.sync_line(Some(10), None);
        assert_eq!(selection.selected_char_span(source), Some(start..start + 3));
        assert_eq!(
            visible_statement_ranges(source, &statement.source_span, start - 1, 10, None),
            [1..4]
        );
        let inside_unicode = source.find('中').unwrap() + 1;
        assert!(
            visible_statement_ranges(source, &(inside_unicode..source.len()), 0, 100, None)
                .is_empty()
        );
        assert!(visible_statement_ranges(source, &(0..source.len() + 1), 0, 100, None).is_empty());
        assert!(visible_statement_ranges(source, &(2..2), 0, 100, None).is_empty());
    }

    #[test]
    fn selection_is_clipped_around_an_inserted_execution_marker() {
        let source = "10 A=1:B=2:C=3";
        let selected = target(10, source, "B=2");
        assert_eq!(
            visible_statement_ranges(source, &selected.source_span, 0, 99, Some((3, 2))),
            [9..12]
        );
        assert_eq!(
            visible_statement_ranges(source, &selected.source_span, 0, 99, Some((7, 2))),
            [9..12]
        );
        assert_eq!(
            visible_statement_ranges(source, &selected.source_span, 0, 99, Some((11, 2))),
            [7..10]
        );
        assert_eq!(
            visible_statement_ranges(source, &selected.source_span, 9, 2, Some((7, 2))),
            [0..2]
        );
        assert_eq!(
            visible_statement_ranges(source, &selected.source_span, 10, 1, Some((7, 2))),
            [0..1]
        );
        assert!(
            visible_statement_ranges(source, &selected.source_span, 0, 0, Some((7, 2))).is_empty()
        );
        assert!(visible_statement_ranges(
            source,
            &selected.source_span,
            usize::MAX,
            usize::MAX,
            None
        )
        .is_empty());
        let whole_if = "10 IF A THEN PRINT 1:PRINT 2 ELSE PRINT 3";
        let selected = target(10, whole_if, "IF A THEN PRINT 1:PRINT 2 ELSE PRINT 3");
        let inside = whole_if.find("PRINT 2").unwrap();
        assert_eq!(
            visible_statement_ranges(whole_if, &selected.source_span, 0, 100, Some((inside, 2))),
            [3..inside, inside + 2..whole_if.len() + 2]
        );
        assert_eq!(
            visible_statement_ranges(
                whole_if,
                &selected.source_span,
                inside,
                2,
                Some((inside, 2))
            ),
            Vec::<Range<usize>>::new()
        );
    }
}
