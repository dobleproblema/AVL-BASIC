use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

include!(concat!(env!("OUT_DIR"), "/error_codes.rs"));

#[derive(Debug, Clone)]
pub struct ErrorStatement {
    /// Full source line as it was executed, including its BASIC line number if present.
    pub source: String,
    /// UTF-8 byte range of the failed instruction within `source`.
    pub span: Range<usize>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StaticErrorDetail {
    ContinueWithoutLoop,
    ExitForWithoutLoop,
    ExitWhileWithoutLoop,
    ScreenFullToggle,
    ScreenSyncToggle,
}

impl StaticErrorDetail {
    fn message(self) -> &'static str {
        match self {
            Self::ContinueWithoutLoop => "CONTINUE outside an active loop.",
            Self::ExitForWithoutLoop => "EXIT FOR without an active FOR loop.",
            Self::ExitWhileWithoutLoop => "EXIT WHILE without an active WHILE loop.",
            Self::ScreenFullToggle => "SCREEN FULL expects ON or OFF.",
            Self::ScreenSyncToggle => "SCREEN SYNC expects ON or OFF.",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BasicError {
    pub code: ErrorCode,
    pub line: Option<i32>,
    pub detail: Option<String>,
    /// Source file when the error came from an imported routine.
    pub source_path: Option<PathBuf>,
    /// Source of the innermost instruction associated with this error.
    pub statement: Option<Box<ErrorStatement>>,
    static_detail: Option<StaticErrorDetail>,
}

impl BasicError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            line: None,
            detail: None,
            source_path: None,
            statement: None,
            static_detail: None,
        }
    }

    pub fn at_line(mut self, line: i32) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub(crate) fn with_static_detail(mut self, detail: StaticErrorDetail) -> Self {
        self.static_detail = Some(detail);
        self
    }

    pub(crate) fn message_for_basic(&self) -> &str {
        self.detail
            .as_deref()
            .or_else(|| self.static_detail.map(StaticErrorDetail::message))
            .unwrap_or_else(|| self.code.message())
    }

    /// Attaches source context without replacing an origin from a deeper call.
    pub fn with_statement(mut self, source: impl Into<String>, span: Range<usize>) -> Self {
        if self.statement.is_none() && !span.is_empty() {
            let source = source.into();
            if source.get(span.clone()).is_some() {
                self.statement = Some(Box::new(ErrorStatement { source, span }));
            }
        }
        self
    }

    pub fn display_for_basic(&self) -> String {
        let message = self.message_for_basic();
        let source = self
            .source_path
            .as_ref()
            .map_or_else(String::new, |path| format!("{}: ", path.display()));
        match self.line {
            Some(line) => format!("Line {line}. {source}{message}"),
            None => format!("{source}{message}"),
        }
    }

    /// Appends the source and its underline to the usual BASIC error message.
    pub fn display_diagnostic(&self) -> String {
        let header = self.display_for_basic();
        match self.diagnostic_context() {
            Some((source, underline)) => format!("{header}\n{source}\n{underline}"),
            None => header,
        }
    }

    pub(crate) fn diagnostic_context(&self) -> Option<(String, String)> {
        let statement = self.statement.as_deref()?;
        let instruction = statement.source.get(statement.span.clone())?;
        if instruction.is_empty() {
            return None;
        }

        let mut source = String::with_capacity(statement.source.len());
        let start =
            append_diagnostic_source(&mut source, &statement.source[..statement.span.start], 0);
        let end = append_diagnostic_source(&mut source, instruction, start);
        append_diagnostic_source(&mut source, &statement.source[statement.span.end..], end);
        let mut underline = " ".repeat(start);
        underline.extend(std::iter::repeat_n('^', end - start));
        Some((source, underline))
    }
}

fn append_diagnostic_source(output: &mut String, text: &str, mut column: usize) -> usize {
    for ch in text.chars() {
        match ch {
            '\t' => {
                let spaces = 8 - column % 8;
                output.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            // As in the debugger's value editor, preserve ordinary Western text
            // and escape variable-width characters and terminal controls.
            ' '..='~' | '\u{a0}'..='\u{ac}' | '\u{ae}'..='\u{24f}' => {
                output.push(ch);
                column += 1;
            }
            _ => {
                let before = output.len();
                output.extend(ch.escape_default());
                column += output.len() - before;
            }
        }
    }
    column
}

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_for_basic())
    }
}

impl std::error::Error for BasicError {}

pub type BasicResult<T> = Result<T, BasicError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_marks_the_selected_statement_and_retains_the_innermost_origin() {
        let source = "120 A=8 : PRINT A/0 : PRINT \"Fin\"";
        let error = BasicError::new(ErrorCode::DivisionByZero)
            .at_line(120)
            .with_statement(source, 10..19)
            .with_statement("20 CALL Example", 3..15);

        assert_eq!(error.display_for_basic(), "Line 120. Division by zero.");
        assert_eq!(
            error.display_diagnostic(),
            "Line 120. Division by zero.\n120 A=8 : PRINT A/0 : PRINT \"Fin\"\n          ^^^^^^^^^"
        );
    }

    #[test]
    fn diagnostic_aligns_tabs_and_accented_characters() {
        let source = "20\tPRINT \"é\" :\tPRINT 1/0";
        let start = source.find("PRINT 1/0").unwrap();
        let error = BasicError::new(ErrorCode::DivisionByZero)
            .at_line(20)
            .with_statement(source, start..source.len());

        assert_eq!(
            error.display_diagnostic(),
            "Line 20. Division by zero.\n20      PRINT \"é\" :     PRINT 1/0\n                        ^^^^^^^^^"
        );
    }

    #[test]
    fn diagnostic_escapes_terminal_controls_and_variable_width_characters() {
        let source = "\x1b界e\u{301}\tPRINT 1/0";
        let start = source.find("PRINT 1/0").unwrap();
        let error =
            BasicError::new(ErrorCode::DivisionByZero).with_statement(source, start..source.len());

        assert_eq!(
            error.display_diagnostic(),
            "Division by zero.\n\\u{1b}\\u{754c}e\\u{301}  PRINT 1/0\n                        ^^^^^^^^^"
        );
    }

    #[test]
    fn diagnostic_falls_back_to_the_header_when_the_span_is_invalid() {
        let error = BasicError::new(ErrorCode::DivisionByZero).with_statement("éx", 1..2);

        assert!(error.statement.is_none());
        assert_eq!(error.display_diagnostic(), error.display_for_basic());
    }

    fn manual_error_rows<'a>(manual: &'a str, heading: &str) -> Vec<(i32, &'a str)> {
        let remainder = manual
            .split_once(heading)
            .unwrap_or_else(|| panic!("missing manual heading {heading:?}"))
            .1;
        let mut rows = Vec::new();
        for line in remainder.lines().map(str::trim) {
            let Some((number, message)) = line.split_once(char::is_whitespace) else {
                if !rows.is_empty() {
                    break;
                }
                continue;
            };
            let Ok(number) = number.parse::<i32>() else {
                if !rows.is_empty() {
                    break;
                }
                continue;
            };
            rows.push((number, message.trim()));
        }
        rows
    }

    #[test]
    fn generated_error_catalog_is_contiguous_and_round_trips() {
        assert_eq!(ErrorCode::ALL.len(), 64);
        for (index, code) in ErrorCode::ALL.iter().copied().enumerate() {
            let number = i32::try_from(index + 1).unwrap();
            assert_eq!(code.number(), number);
            assert_eq!(ErrorCode::from_number(number), Some(code));
            assert!(!code.message().is_empty());
        }
        assert_eq!(ErrorCode::from_number(0), None);
        assert_eq!(ErrorCode::from_number(65), None);
    }

    #[test]
    fn both_manual_error_tables_come_from_the_catalog() {
        let english = include_str!("../MANUAL.txt");
        let spanish = include_str!("../MANUAL.es.txt");
        let english_rows = manual_error_rows(english, "Complete table of error codes");
        let spanish_rows = manual_error_rows(spanish, "Tabla completa de códigos de error");

        let expected_english = ErrorCode::ALL
            .iter()
            .map(|code| (code.number(), code.message()))
            .collect::<Vec<_>>();
        let expected_spanish = ErrorCode::ALL
            .iter()
            .map(|code| (code.number(), code.message_es()))
            .collect::<Vec<_>>();
        assert_eq!(english_rows, expected_english);
        assert_eq!(spanish_rows, expected_spanish);
        assert!(english.contains("`ERROR 15` raises a syntax error"));
        assert!(spanish.contains("`ERROR 15` provoca un error de sintaxis"));
    }
}
