use crate::error::{BasicError, BasicResult, ErrorCode};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Str(String),
    Ident(String),
    Op(String),
    Comma,
    Semi,
    Colon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Eof,
}

pub fn strip_comment(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if ch == '"' {
            in_string = !in_string;
            out.push(ch);
            continue;
        }
        if !in_string && ch == '\'' {
            break;
        }
        if !in_string && (ch == 'R' || ch == 'r') {
            let mut probe = String::from(ch);
            let mut clone = chars.clone();
            while probe.len() < 3 {
                if let Some(c) = clone.next() {
                    probe.push(c);
                } else {
                    break;
                }
            }
            if probe.eq_ignore_ascii_case("REM") {
                let before_ok = out.chars().last().map_or(true, |c| !is_ident_char(c));
                let after_ok = clone.peek().map_or(true, |c| !is_ident_char(*c));
                if before_ok && after_ok {
                    break;
                }
            }
        }
        out.push(ch);
    }
    out.trim_end().to_string()
}

pub fn split_top_level(text: &str, separators: &[char]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        let (idx, ch) = bytes[i];
        if ch == '"' {
            in_string = !in_string;
        } else if !in_string {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ if depth == 0 && separators.contains(&ch) => {
                    parts.push(text[start..idx].trim().to_string());
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }
        i += 1;
    }
    parts.push(text[start..].trim().to_string());
    parts
}

pub fn split_commands(line: &str) -> Vec<String> {
    let stripped = strip_comment(line);
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;

    for (idx, ch) in stripped.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ':' if depth == 0 => {
                let current = stripped[start..].trim_start();
                if current.to_ascii_uppercase().starts_with("IF ") {
                    continue;
                }
                let part = stripped[start..idx].trim();
                if !part.is_empty() {
                    parts.push(part.to_string());
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    let tail = stripped[start..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }
    parts
}

pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    pub fn tokenize(mut self) -> BasicResult<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let eof = token == Token::Eof;
            tokens.push(token);
            if eof {
                return Ok(tokens);
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn next_token(&mut self) -> BasicResult<Token> {
        self.skip_ws();
        let Some(ch) = self.peek_char() else {
            return Ok(Token::Eof);
        };

        if ch == '"' {
            return self.string();
        }
        if ch.is_ascii_digit() || ch == '.' {
            return self.number();
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            return Ok(self.ident());
        }
        if ch == '&' {
            return self.radix_number();
        }

        match ch {
            ',' => {
                self.bump();
                Ok(Token::Comma)
            }
            ';' => {
                self.bump();
                Ok(Token::Semi)
            }
            ':' => {
                self.bump();
                Ok(Token::Colon)
            }
            '(' => {
                self.bump();
                Ok(Token::LParen)
            }
            ')' => {
                self.bump();
                Ok(Token::RParen)
            }
            '[' => {
                self.bump();
                Ok(Token::LBracket)
            }
            ']' => {
                self.bump();
                Ok(Token::RBracket)
            }
            '<' | '>' | '=' => self.compare(),
            '+' | '-' | '*' | '/' | '\\' | '^' => {
                self.bump();
                Ok(Token::Op(ch.to_string()))
            }
            _ => Err(BasicError::new(ErrorCode::Syntax)),
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek_char(), Some(c) if c.is_whitespace()) {
            self.bump();
        }
    }

    fn string(&mut self) -> BasicResult<Token> {
        self.bump();
        let mut out = String::new();
        while let Some(ch) = self.bump() {
            if ch == '"' {
                if self.peek_char() == Some('"') {
                    self.bump();
                    continue;
                }
                return Ok(Token::Str(out));
            }
            out.push(ch);
        }
        Err(BasicError::new(ErrorCode::Syntax))
    }

    fn number(&mut self) -> BasicResult<Token> {
        let start = self.pos;
        let mut saw_digit = false;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            saw_digit = true;
            self.bump();
        }
        if self.peek_char() == Some('.') {
            self.bump();
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                saw_digit = true;
                self.bump();
            }
        }
        if !saw_digit {
            return Err(BasicError::new(ErrorCode::Syntax));
        }
        if matches!(self.peek_char(), Some('e' | 'E')) {
            let save = self.pos;
            self.bump();
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.bump();
            }
            let mut exp_digit = false;
            while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
                exp_digit = true;
                self.bump();
            }
            if !exp_digit {
                self.pos = save;
            }
        }
        let raw = &self.src[start..self.pos];
        let value = raw
            .parse::<f64>()
            .map_err(|_| BasicError::new(ErrorCode::InvalidValue))?;
        Ok(Token::Number(value))
    }

    fn radix_number(&mut self) -> BasicResult<Token> {
        self.bump();
        let Some(kind) = self.bump() else {
            return Err(BasicError::new(ErrorCode::Syntax));
        };
        let radix = match kind {
            'h' | 'H' => 16,
            'x' | 'X' => 2,
            _ => return Err(BasicError::new(ErrorCode::Syntax)),
        };
        let start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_digit(radix)) {
            self.bump();
        }
        if self.pos == start {
            return Err(BasicError::new(ErrorCode::InvalidValue));
        }
        let raw = &self.src[start..self.pos];
        let value = i64::from_str_radix(raw, radix)
            .map_err(|_| BasicError::new(ErrorCode::InvalidValue))?;
        Ok(Token::Number(value as f64))
    }

    fn ident(&mut self) -> Token {
        let start = self.pos;
        while matches!(self.peek_char(), Some(c) if is_ident_char(c)) {
            self.bump();
        }
        Token::Ident(self.src[start..self.pos].to_ascii_uppercase())
    }

    fn compare(&mut self) -> BasicResult<Token> {
        let first = self.bump().unwrap();
        let op = match (first, self.peek_char()) {
            ('<', Some('=')) | ('>', Some('=')) | ('<', Some('>')) => {
                let second = self.bump().unwrap();
                format!("{first}{second}")
            }
            _ => first.to_string(),
        };
        Ok(Token::Op(op))
    }
}

pub fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}
