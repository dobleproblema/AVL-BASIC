use crate::error::{BasicError, BasicResult, ErrorCode};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Program {
    lines: BTreeMap<i32, String>,
}

impl Program {
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn add_source_line(&mut self, line: &str) -> BasicResult<()> {
        let trimmed_end = line.trim_end_matches(&['\r', '\n'][..]);
        let source = trimmed_end.trim_start();
        if source.is_empty() {
            return Ok(());
        }
        let digit_end = source
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_digit())
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .ok_or_else(|| BasicError::new(ErrorCode::InvalidLineFormat))?;
        let number_raw = &source[..digit_end];
        let number = number_raw
            .parse::<i32>()
            .map_err(|_| BasicError::new(ErrorCode::InvalidLineFormat))?;
        if number <= 0 {
            return Err(BasicError::new(ErrorCode::InvalidLineFormat));
        }
        let code = &source[digit_end..];
        if !code.is_empty() && !code.chars().next().is_some_and(char::is_whitespace) {
            return Err(BasicError::new(ErrorCode::InvalidLineFormat));
        }
        if code.trim().is_empty() {
            self.lines.remove(&number);
        } else {
            self.lines.insert(number, code.to_string());
        }
        Ok(())
    }

    pub fn load_text(&mut self, text: &str) -> BasicResult<()> {
        self.clear();
        for line in text.lines() {
            self.add_source_line(line)?;
        }
        Ok(())
    }

    pub fn merge_text(&mut self, text: &str) -> BasicResult<()> {
        for line in text.lines() {
            self.add_source_line(line)?;
        }
        Ok(())
    }

    pub fn delete_range(&mut self, start: i32, end: i32) {
        let keys: Vec<i32> = self
            .lines
            .range(start..=end)
            .map(|(line, _)| *line)
            .collect();
        for line in keys {
            self.lines.remove(&line);
        }
    }

    pub fn line_numbers(&self) -> Vec<i32> {
        self.lines.keys().copied().collect()
    }

    pub fn get(&self, line: i32) -> Option<&str> {
        self.lines.get(&line).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn list(&self) -> String {
        let mut out = String::new();
        for (line, code) in &self.lines {
            out.push_str(&format!("{line}{code}\n"));
        }
        out
    }

    pub fn index_of(&self, target: i32) -> Option<usize> {
        self.lines.keys().position(|line| *line == target)
    }
}
