use std::fmt;

include!(concat!(env!("OUT_DIR"), "/error_codes.rs"));

#[derive(Debug, Clone)]
pub struct BasicError {
    pub code: ErrorCode,
    pub line: Option<i32>,
    pub detail: Option<String>,
}

impl BasicError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            line: None,
            detail: None,
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

    pub fn display_for_basic(&self) -> String {
        let message = self
            .detail
            .as_deref()
            .unwrap_or_else(|| self.code.message());
        match self.line {
            Some(line) => format!("Line {line}. {message}"),
            None => message.to_string(),
        }
    }
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
