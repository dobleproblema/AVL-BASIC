use super::{
    current_syntax_palette, highlight_line_body, is_label_declaration, push_styled, size,
    split_single_quote_comment, SyntaxPalette, RESET,
};
use std::io::{self, IsTerminal};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Column {
    Text,
    Number,
    ModuleId,
    Line,
    Routine,
    Source,
}

pub(crate) struct Report {
    text: String,
    palette: Option<SyntaxPalette>,
    width: Option<usize>,
    show_modules: bool,
}

impl Report {
    pub(crate) fn new(ansi: bool, stream: bool, show_modules: bool) -> Self {
        // Classic Windows consoles can wrap immediately after the last cell.
        let width = (stream && io::stdout().is_terminal()).then(|| {
            let cols = match size() {
                Ok((cols, _)) if cols > 0 => cols,
                _ => 80,
            };
            usize::from(cols.saturating_sub(1).max(1))
        });
        Self {
            text: String::new(),
            palette: ansi.then(current_syntax_palette),
            width,
            show_modules,
        }
    }

    pub(crate) fn heading(&mut self, title: &str, detail: &str) {
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        let mut line = String::new();
        if let Some(palette) = self.palette {
            push_styled(&mut line, palette.keyword, title);
        } else {
            line.push_str(title);
        }
        line.push_str(detail);
        self.line(&line);
    }

    pub(crate) fn table<const N: usize>(
        &mut self,
        columns: [(&str, Column); N],
        mut rows: Vec<[String; N]>,
    ) {
        for cell in rows.iter_mut().flatten() {
            if cell.chars().any(char::is_control) {
                *cell = printable(cell);
            }
        }
        let widths: [usize; N] = std::array::from_fn(|i| {
            rows.iter()
                .map(|r| r[i].width())
                .fold(columns[i].0.width(), usize::max)
        });
        let header = columns.map(|(label, _)| label.to_owned());
        for (index, row) in std::iter::once(&header).chain(&rows).enumerate() {
            let mut line = String::new();
            let mut gap = 0;
            for (i, &(_, kind)) in columns.iter().enumerate() {
                if kind == Column::ModuleId && !self.show_modules {
                    continue;
                }
                let cell = &row[i];
                let pad = widths[i] - cell.width();
                let numeric = matches!(kind, Column::Number | Column::Line | Column::ModuleId);
                line.extend(std::iter::repeat_n(
                    ' ',
                    gap + if numeric { pad } else { 0 },
                ));
                self.cell(&mut line, cell, kind, index == 0);
                gap = 2 + if numeric { 0 } else { pad };
            }
            self.line(&line);
        }
    }

    fn cell(&self, line: &mut String, cell: &str, kind: Column, header: bool) {
        let Some(palette) = self.palette else {
            line.push_str(cell);
            return;
        };
        if header {
            push_styled(line, palette.import_alias, cell);
            return;
        }
        match kind {
            Column::Line => push_styled(line, palette.line_number, cell),
            Column::ModuleId => push_styled(line, palette.import_alias, cell),
            Column::Routine => {
                if let Some(name) = cell.strip_prefix("SUB ") {
                    push_styled(line, palette.keyword, "SUB");
                    line.push(' ');
                    push_styled(line, palette.keyword, name);
                } else {
                    push_styled(line, palette.other, cell);
                }
            }
            Column::Source => {
                let (body, comment) = split_single_quote_comment(cell);
                highlight_line_body(
                    line,
                    body,
                    comment,
                    is_label_declaration(cell),
                    None,
                    palette,
                );
            }
            Column::Text | Column::Number => line.push_str(cell),
        }
    }

    fn line(&mut self, rendered: &str) {
        if let Some(width) = self.width {
            append_clipped(&mut self.text, rendered, width);
        } else {
            self.text.push_str(rendered);
        }
        self.text.push('\n');
    }

    pub(crate) fn finish(self) -> String {
        self.text
    }
}

// Use printable text even for BASIC string literals and module paths containing
// terminal controls. Expand tabs relative to the start of their cell.
fn printable(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if ch == '\t' {
            out.extend(std::iter::repeat_n(' ', 8 - out.as_str().width() % 8));
        } else if ch.is_control() {
            out.extend(ch.escape_default());
        } else {
            out.push(ch);
        }
    }
    out
}

// The only escape sequences left after printable() are the palette's own SGRs.
fn spans(mut text: &str) -> impl Iterator<Item = (&str, bool)> {
    std::iter::from_fn(move || {
        if text.is_empty() {
            return None;
        }
        let sgr = text.starts_with('\x1b');
        let end = if sgr {
            text.find('m').map_or(text.len(), |i| i + 1)
        } else {
            text.find('\x1b').unwrap_or(text.len())
        };
        let (span, rest) = text.split_at(end);
        text = rest;
        Some((span, sgr))
    })
}

fn append_clipped(out: &mut String, rendered: &str, width: usize) {
    let unstyled;
    let plain = if rendered.contains('\x1b') {
        unstyled = spans(rendered)
            .filter_map(|(s, sgr)| (!sgr).then_some(s))
            .collect::<String>();
        unstyled.as_str()
    } else {
        rendered
    };
    if plain.width() <= width {
        out.push_str(rendered);
        return;
    }
    let mut cells = 0;
    let mut bytes = 0;
    for cluster in plain.graphemes(true) {
        cells += cluster.width();
        if cells > width {
            break;
        }
        bytes += cluster.len();
    }
    let mut styled = false;
    for (span, sgr) in spans(rendered) {
        if bytes == 0 {
            break;
        }
        if sgr {
            out.push_str(span);
            styled = span != RESET;
        } else {
            let keep = bytes.min(span.len());
            out.push_str(&span[..keep]);
            bytes -= keep;
        }
    }
    if styled {
        out.push_str(RESET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::{syntax_palette_for, SyntaxTheme};

    fn plain(text: &str) -> String {
        spans(text)
            .filter_map(|(s, sgr)| (!sgr).then_some(s))
            .collect()
    }

    #[test]
    fn numbers_align_at_the_right_edge_and_source_starts_at_the_header() {
        let mut report = Report::new(false, false, false);
        report.table(
            [
                ("Line", Column::Line),
                ("Visits", Column::Number),
                ("Source", Column::Source),
            ],
            vec![
                ["9".into(), "123456".into(), "PRINT 1".into()],
                ["1200".into(), "7".into(), "END".into()],
            ],
        );
        assert_eq!(
            report.finish(),
            "Line  Visits  Source\n   9  123456  PRINT 1\n1200       7  END\n"
        );
    }

    #[test]
    fn both_themes_style_headings_subroutines_numbers_and_basic_tokens() {
        for theme in [SyntaxTheme::Dark, SyntaxTheme::Light] {
            let palette = syntax_palette_for(theme);
            let mut report = Report {
                text: String::new(),
                palette: Some(palette),
                width: None,
                show_modules: false,
            };
            report.table(
                [
                    ("Line", Column::Line),
                    ("Routine", Column::Routine),
                    ("Source", Column::Source),
                ],
                vec![[
                    "10".into(),
                    "SUB DRAW".into(),
                    "  CALL DRAW : PRINT \"red\"".into(),
                ]],
            );
            let text = report.finish();
            for (style, token) in [
                (palette.import_alias, "Line"),
                (palette.line_number, "10"),
                (palette.keyword, "CALL"),
                (palette.keyword, "PRINT"),
                (palette.string, "\"red\""),
            ] {
                assert!(text.contains(&format!("{style}{token}{RESET}")), "{text:?}");
            }
            assert_eq!(
                text.matches(&format!("{}DRAW{RESET}", palette.keyword))
                    .count(),
                2
            );
            assert!(plain(&text).ends_with("  CALL DRAW : PRINT \"red\"\n"));
        }
    }

    #[test]
    fn a_clipped_token_retains_its_style_and_whole_display_cells() {
        let style = syntax_palette_for(SyntaxTheme::Dark).string;
        let colored = format!("{style}a界e\u{301}👩‍💻z{RESET}");
        for (width, expected) in [
            (0, ""),
            (2, "a"),
            (3, "a界"),
            (4, "a界e\u{301}"),
            (5, "a界e\u{301}"),
            (6, "a界e\u{301}👩‍💻"),
        ] {
            let mut result = String::new();
            append_clipped(&mut result, &colored, width);
            assert_eq!(plain(&result), expected);
            if !expected.is_empty() {
                assert!(result.ends_with(RESET));
            }
        }
        assert_eq!(printable("界\tA\r\n\u{1b}"), "界      A\\r\\n\\u{1b}");
    }
}
