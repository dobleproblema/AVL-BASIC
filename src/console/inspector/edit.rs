//! A literal-value field. This never invokes the BASIC expression evaluator.
use crate::debugger::{DebugValue, DebugVariable};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::ops::Range;

#[derive(Debug)]
pub(super) struct ScalarEdit {
    name: String,
    string: bool,
    buffer: Vec<char>,
    cursor: usize,
    anchor: Option<usize>,
    left: usize,
    error: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct EditField {
    pub text: String,
    pub cursor: usize,
    selected: Range<usize>,
}

impl ScalarEdit {
    pub fn new(variable: &DebugVariable) -> Self {
        let (string, text) = match &variable.value {
            DebugValue::String(value) => (true, encode_string(value, true)),
            DebugValue::Number(value) => (false, value.to_string()),
        };
        let buffer: Vec<char> = text.chars().collect();
        Self {
            name: variable.name.clone(),
            string,
            cursor: buffer.len(),
            anchor: Some(0),
            buffer,
            left: 0,
            error: None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn fail(&mut self, message: String) {
        self.error = Some(encode_string(&message, false));
    }

    fn selection(&self) -> Range<usize> {
        self.anchor.map_or(self.cursor..self.cursor, |anchor| {
            anchor.min(self.cursor)..anchor.max(self.cursor)
        })
    }

    fn erase_selection(&mut self) -> bool {
        let range = self.selection();
        self.anchor = None;
        if range.is_empty() {
            return false;
        }
        self.buffer.drain(range.clone());
        self.cursor = range.start;
        true
    }

    fn insert(&mut self, text: &str) {
        self.erase_selection();
        let chars: Vec<char> = encode_string(text, false).chars().collect();
        let len = chars.len();
        self.buffer.splice(self.cursor..self.cursor, chars);
        self.cursor += len;
        self.error = None;
    }

    pub fn paste(&mut self, text: &str) {
        self.insert(text);
    }

    fn move_cursor(&mut self, cursor: usize, shift: bool) {
        if shift {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = cursor.min(self.buffer.len());
    }

    pub fn key(&mut self, event: KeyEvent) -> Option<DebugVariable> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        let shift = event.modifiers.contains(KeyModifiers::SHIFT);
        if event
            .modifiers
            .contains(KeyModifiers::ALT | KeyModifiers::CONTROL)
        {
            // Windows reports AltGr text with both modifiers set.
            if let KeyCode::Char(ch) = event.code {
                if !ch.is_control() {
                    self.insert(&ch.to_string());
                }
            }
            return None;
        }
        if event.modifiers.contains(KeyModifiers::CONTROL) {
            if matches!(event.code, KeyCode::Char('a' | 'A')) {
                self.anchor = Some(0);
                self.cursor = self.buffer.len();
            }
            return None;
        }
        if event.modifiers.contains(KeyModifiers::ALT) {
            return None;
        }
        match event.code {
            KeyCode::Left | KeyCode::Up => {
                let cursor = if !shift && !self.selection().is_empty() {
                    self.selection().start
                } else {
                    self.cursor.saturating_sub(1)
                };
                self.move_cursor(cursor, shift);
            }
            KeyCode::Right | KeyCode::Down => {
                let cursor = if !shift && !self.selection().is_empty() {
                    self.selection().end
                } else {
                    self.cursor.saturating_add(1)
                };
                self.move_cursor(cursor, shift);
            }
            KeyCode::Home => self.move_cursor(0, shift),
            KeyCode::End => self.move_cursor(self.buffer.len(), shift),
            KeyCode::Backspace => {
                if !self.erase_selection() && self.cursor > 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                }
                self.error = None;
            }
            KeyCode::Delete => {
                if !self.erase_selection() && self.cursor < self.buffer.len() {
                    self.buffer.remove(self.cursor);
                }
                self.error = None;
            }
            KeyCode::Char(ch) => self.insert(&ch.to_string()),
            KeyCode::Enter => {
                let input: String = self.buffer.iter().collect();
                let value = if self.string {
                    decode_string(&input).map(DebugValue::String)
                } else {
                    input
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .map(DebugValue::Number)
                        .ok_or_else(|| "Enter a finite decimal number".to_string())
                };
                match value {
                    Ok(value) => {
                        return Some(DebugVariable {
                            name: self.name.clone(),
                            value,
                        })
                    }
                    Err(message) => self.error = Some(message),
                }
            }
            _ => {}
        }
        None
    }

    pub fn field(&mut self, width: usize) -> Option<EditField> {
        if width == 0 {
            return None;
        }
        if self.cursor < self.left {
            self.left = self.cursor;
        }
        if self.cursor >= self.left.saturating_add(width) {
            self.left = self.cursor + 1 - width;
        }
        let selected = self.selection();
        let mut text: String = self.buffer.iter().skip(self.left).take(width).collect();
        let content_len = text.chars().count();
        text.push_str(&" ".repeat(width - content_len));
        Some(EditField {
            text,
            cursor: self.cursor.saturating_sub(self.left).min(width - 1),
            selected: selected.start.saturating_sub(self.left).min(content_len)
                ..selected.end.saturating_sub(self.left).min(content_len),
        })
    }
}

impl EditField {
    pub fn render(&self, ansi: bool) -> String {
        if !ansi || self.selected.is_empty() {
            return self.text.clone();
        }
        let chars: Vec<char> = self.text.chars().collect();
        format!(
            "{}\x1b[7m{}\x1b[27m{}",
            chars[..self.selected.start].iter().collect::<String>(),
            chars[self.selected.clone()].iter().collect::<String>(),
            chars[self.selected.end..].iter().collect::<String>()
        )
    }
}

fn encode_string(value: &str, escape_backslash: bool) -> String {
    let mut text = String::new();
    for ch in value.chars() {
        match ch {
            '\\' if escape_backslash => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            '\0' => text.push_str("\\0"),
            // Keep ordinary Western text readable. Other Unicode characters
            // use reversible ASCII escapes so combining marks, wide glyphs,
            // emoji and format controls cannot alter this field's cell width.
            ch if !matches!(ch, ' '..='~' | '\u{a0}'..='\u{ac}' | '\u{ae}'..='\u{24f}') => {
                text.push_str(&format!("\\u{{{:X}}}", ch as u32));
            }
            ch => text.push(ch),
        }
    }
    text
}

fn decode_string(text: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        let escaped = match chars.next() {
            Some('\\') => '\\',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some('0') => '\0',
            Some('x') => {
                let digits: String = chars.by_ref().take(2).collect();
                if digits.len() != 2 || !digits.chars().all(|ch| ch.is_ascii_hexdigit()) {
                    return Err("Use two hexadecimal digits after \\x".into());
                }
                char::from(u8::from_str_radix(&digits, 16).unwrap())
            }
            Some('u') => {
                if chars.next() != Some('{') {
                    return Err("Use \\u{HEX} for a Unicode escape".into());
                }
                let mut digits = String::new();
                loop {
                    match chars.next() {
                        Some('}') if !digits.is_empty() => break,
                        Some(ch) if ch.is_ascii_hexdigit() && digits.len() < 6 => digits.push(ch),
                        _ => return Err("Invalid Unicode escape; use \\u{HEX}".into()),
                    }
                }
                u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| "Invalid Unicode code point".to_string())?
            }
            Some(_) => return Err("Invalid escape; use \\\\ for a backslash".into()),
            None => return Err("Incomplete escape; use \\\\ for a backslash".into()),
        };
        result.push(escaped);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn edit(value: DebugValue) -> ScalarEdit {
        ScalarEdit::new(&DebugVariable {
            name: "A".into(),
            value,
        })
    }

    #[test]
    fn initial_selection_is_replaced_and_numbers_are_literals_only() {
        let mut edit = edit(DebugValue::Number(123.0));
        assert!(edit
            .field(20)
            .unwrap()
            .render(true)
            .contains("\x1b[7m123\x1b[27m"));
        edit.paste("-1.25e2");
        assert_eq!(
            edit.key(key(KeyCode::Enter)).unwrap().value,
            DebugValue::Number(-125.0)
        );
        for invalid in ["", "1+2", "RND", "NaN", "inf", "1e999"] {
            edit.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
            edit.paste(invalid);
            assert!(
                edit.key(key(KeyCode::Enter)).is_none(),
                "accepted {invalid}"
            );
            assert!(edit.error().is_some());
        }
    }

    #[test]
    fn strings_round_trip_escapes_quotes_controls_and_unicode_without_ansi_injection() {
        let original = "A\\B\n\r\t\0\x1b[31m\u{85}é\"中文🙂e\u{301}\u{200d}\u{ad}";
        let mut edit = edit(DebugValue::String(original.into()));
        let field = edit.field(100).unwrap();
        assert!(!field.text.chars().any(char::is_control));
        assert!(field.text.contains("é"));
        assert!(!field.text.contains('中'));
        assert!(!field.text.contains('🙂'));
        assert!(!field.text.contains('\u{301}'));
        assert_eq!(
            edit.key(key(KeyCode::Enter)).unwrap().value,
            DebugValue::String(original.into())
        );
        edit.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        edit.paste("raw\x1b[0m\n");
        assert!(!edit.field(100).unwrap().text.contains('\x1b'));
        assert_eq!(
            edit.key(key(KeyCode::Enter)).unwrap().value,
            DebugValue::String("raw\x1b[0m\n".into())
        );
        for invalid in [
            "\\",
            "\\q",
            "\\xG1",
            "\\x0",
            "\\u{}",
            "\\u{D800}",
            "\\u{110000}",
        ] {
            edit.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
            edit.paste(invalid);
            assert!(
                edit.key(key(KeyCode::Enter)).is_none(),
                "accepted {invalid}"
            );
        }
    }

    #[test]
    fn cursor_selection_deletion_and_horizontal_scrolling_are_bounded() {
        let mut edit = edit(DebugValue::String("abcdefghijk".into()));
        for width in [1, 4, 2, 100] {
            let field = edit.field(width).unwrap();
            assert_eq!(field.text.chars().count(), width);
            assert!(field.cursor < width);
        }
        assert!(edit.field(0).is_none());
        edit.key(key(KeyCode::Home));
        edit.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        edit.key(key(KeyCode::Char('Z')));
        edit.key(key(KeyCode::Delete));
        edit.key(key(KeyCode::End));
        edit.key(key(KeyCode::Backspace));
        assert_eq!(
            edit.key(key(KeyCode::Enter)).unwrap().value,
            DebugValue::String("Zcdefghij".into())
        );
        let before: String = edit.buffer.iter().collect();
        for code in [
            KeyCode::F(5),
            KeyCode::Tab,
            KeyCode::PageDown,
            KeyCode::PageUp,
        ] {
            edit.key(key(code));
        }
        assert_eq!(edit.buffer.iter().collect::<String>(), before);
    }

    #[test]
    fn altgr_inserts_text_and_wide_unicode_has_a_reversible_single_cell_buffer() {
        let mut edit = edit(DebugValue::String(String::new()));
        edit.key(KeyEvent::new(
            KeyCode::Char('@'),
            KeyModifiers::ALT | KeyModifiers::CONTROL,
        ));
        edit.paste("áéñ中🙂e\u{301}");
        assert!(edit
            .buffer
            .iter()
            .all(|ch| matches!(*ch, ' '..='~' | '\u{a0}'..='\u{ac}' | '\u{ae}'..='\u{24f}')));
        for width in [1, 7, 20, 80] {
            let field = edit.field(width).unwrap();
            assert_eq!(field.text.chars().count(), width);
            assert!(field.cursor < width);
        }
        assert_eq!(
            edit.key(key(KeyCode::Enter)).unwrap().value,
            DebugValue::String("@áéñ中🙂e\u{301}".into())
        );
    }
}
