//! Streaming UTF-8 sequential data channels. BASIC syntax lives in the interpreter.
use crate::error::{BasicError, BasicResult, ErrorCode};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileMode {
    Input,
    Output,
    Append,
}

#[derive(Debug)]
enum Stream {
    Input(BufReader<File>),
    Output(File),
}
#[derive(Debug)]
struct Channel {
    path: PathBuf,
    stream: Stream,
    column: usize,
}
#[derive(Default, Debug)]
pub(crate) struct DataFiles {
    channels: BTreeMap<u8, Channel>,
}

fn error(code: ErrorCode) -> BasicError {
    BasicError::new(code)
}
fn io_error(_: std::io::Error) -> BasicError {
    error(ErrorCode::FileIoError)
}
fn invalid() -> BasicError {
    error(ErrorCode::InvalidFileData)
}

pub(crate) fn channel_number(value: f64) -> BasicResult<u8> {
    if !value.is_finite() || value.fract() != 0.0 || !(1.0..=255.0).contains(&value) {
        Err(error(ErrorCode::InvalidArgument))
    } else {
        Ok(value as u8)
    }
}

impl DataFiles {
    pub(crate) fn check_free(&self, number: u8) -> BasicResult<()> {
        if self.channels.contains_key(&number) {
            Err(error(ErrorCode::FileAlreadyOpen))
        } else {
            Ok(())
        }
    }
    pub(crate) fn open(&mut self, number: u8, path: PathBuf, mode: FileMode) -> BasicResult<()> {
        self.check_free(number)?;
        if self.channels.values().any(|channel| {
            channel.path == path
                && (mode != FileMode::Input || matches!(channel.stream, Stream::Output(_)))
        }) {
            return Err(error(ErrorCode::FileAlreadyOpen));
        }
        let stream = match mode {
            FileMode::Input => {
                let file = File::open(&path).map_err(|err| {
                    if err.kind() == std::io::ErrorKind::NotFound {
                        error(ErrorCode::FileNotFound)
                    } else {
                        io_error(err)
                    }
                })?;
                let mut reader = BufReader::new(file);
                if reader
                    .fill_buf()
                    .map_err(io_error)?
                    .starts_with(&[0xef, 0xbb, 0xbf])
                {
                    reader.consume(3);
                }
                Stream::Input(reader)
            }
            FileMode::Output => Stream::Output(
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&path)
                    .map_err(io_error)?,
            ),
            FileMode::Append => Stream::Output(
                OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .map_err(io_error)?,
            ),
        };
        self.channels.insert(
            number,
            Channel {
                path,
                stream,
                column: 0,
            },
        );
        Ok(())
    }
    fn channel(&self, number: u8) -> BasicResult<&Channel> {
        self.channels
            .get(&number)
            .ok_or_else(|| error(ErrorCode::FileNotOpen))
    }
    fn reader(&mut self, number: u8) -> BasicResult<&mut BufReader<File>> {
        match &mut self
            .channels
            .get_mut(&number)
            .ok_or_else(|| error(ErrorCode::FileNotOpen))?
            .stream
        {
            Stream::Input(reader) => Ok(reader),
            Stream::Output(_) => Err(error(ErrorCode::BadFileMode)),
        }
    }
    pub(crate) fn check_output(&self, number: u8) -> BasicResult<()> {
        match self.channel(number)?.stream {
            Stream::Output(_) => Ok(()),
            _ => Err(error(ErrorCode::BadFileMode)),
        }
    }
    pub(crate) fn check_open(&self, number: u8) -> BasicResult<()> {
        self.channel(number).map(|_| ())
    }
    pub(crate) fn check_input(&self, number: u8) -> BasicResult<()> {
        match self.channel(number)?.stream {
            Stream::Input(_) => Ok(()),
            _ => Err(error(ErrorCode::BadFileMode)),
        }
    }
    pub(crate) fn column(&self, number: u8) -> usize {
        self.channels.get(&number).map_or(0, |c| c.column)
    }
    pub(crate) fn write(&mut self, number: u8, text: &str) -> BasicResult<()> {
        let channel = self
            .channels
            .get_mut(&number)
            .ok_or_else(|| error(ErrorCode::FileNotOpen))?;
        let Stream::Output(file) = &mut channel.stream else {
            return Err(error(ErrorCode::BadFileMode));
        };
        file.write_all(text.as_bytes()).map_err(io_error)?;
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                channel.column = 0;
            } else {
                channel.column += 1;
            }
        }
        Ok(())
    }
    pub(crate) fn flush(&mut self, number: u8) -> BasicResult<()> {
        let channel = self
            .channels
            .get_mut(&number)
            .ok_or_else(|| error(ErrorCode::FileNotOpen))?;
        if let Stream::Output(file) = &mut channel.stream {
            file.flush().map_err(io_error)?;
        }
        Ok(())
    }
    pub(crate) fn close(&mut self, numbers: &[u8]) -> BasicResult<()> {
        for &number in numbers {
            self.channel(number)?;
        }
        let mut first_error = None;
        for &number in numbers {
            if self.channels.contains_key(&number) {
                if let Err(err) = self.flush(number) {
                    first_error.get_or_insert(err);
                }
                self.channels.remove(&number);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
    pub(crate) fn close_all(&mut self) -> BasicResult<()> {
        let numbers = self.channels.keys().copied().collect::<Vec<_>>();
        self.close(&numbers)
    }
    pub(crate) fn eof(&mut self, number: u8) -> BasicResult<bool> {
        Ok(peek(self.reader(number)?)?.is_none())
    }
    pub(crate) fn line(&mut self, number: u8) -> BasicResult<String> {
        let reader = self.reader(number)?;
        if peek(reader)?.is_none() {
            return Err(error(ErrorCode::InputPastEnd));
        }
        let mut bytes = Vec::new();
        while let Some(byte) = next(reader)? {
            if byte == b'\r' || byte == b'\n' {
                finish_newline(reader, byte)?;
                break;
            }
            bytes.push(byte);
        }
        String::from_utf8(bytes).map_err(|_| invalid())
    }
    pub(crate) fn record(&mut self, number: u8) -> BasicResult<Vec<String>> {
        let reader = self.reader(number)?;
        if peek(reader)?.is_none() {
            return Err(error(ErrorCode::InputPastEnd));
        }
        let mut fields = Vec::new();
        let mut bytes = Vec::new();
        let mut physical = physical_line(reader)?;
        let mut offset = 0;
        // 0 leading whitespace, 1 unquoted, 2 quoted, 3 closing quote, 4 whitespace after quote.
        let mut state = 0;
        loop {
            if offset == physical.len() && state == 2 {
                physical = physical_line(reader)?;
                offset = 0;
            }
            let byte = physical.get(offset).copied();
            if byte.is_some() {
                offset += 1;
            }
            if state == 2 {
                match byte {
                    Some(b'"') => state = 3,
                    Some(b) => bytes.push(b),
                    None => return Err(invalid()),
                }
                continue;
            }
            if byte.is_none() || matches!(byte, Some(b',' | b'\r' | b'\n')) {
                let field = String::from_utf8(std::mem::take(&mut bytes)).map_err(|_| invalid())?;
                fields.push(if state <= 1 {
                    field.trim_matches([' ', '\t']).to_string()
                } else {
                    field
                });
                match byte {
                    Some(b',') => {
                        state = 0;
                        continue;
                    }
                    Some(_) => (),
                    None => (),
                }
                return Ok(fields);
            }
            let b = byte.unwrap();
            match state {
                0 if b == b' ' || b == b'\t' => bytes.push(b),
                0 if b == b'"' => {
                    bytes.clear();
                    state = 2;
                }
                0 => {
                    bytes.push(b);
                    state = 1;
                }
                1 if b == b'"' => return Err(invalid()),
                1 => bytes.push(b),
                3 if b == b'"' => {
                    bytes.push(b);
                    state = 2;
                }
                3 | 4 if b == b' ' || b == b'\t' => state = 4,
                _ => return Err(invalid()),
            }
        }
    }
}

fn peek(reader: &mut BufReader<File>) -> BasicResult<Option<u8>> {
    Ok(reader.fill_buf().map_err(io_error)?.first().copied())
}
fn next(reader: &mut BufReader<File>) -> BasicResult<Option<u8>> {
    let byte = peek(reader)?;
    if byte.is_some() {
        reader.consume(1);
    }
    Ok(byte)
}
fn finish_newline(reader: &mut BufReader<File>, byte: u8) -> BasicResult<()> {
    if byte == b'\r' && peek(reader)? == Some(b'\n') {
        reader.consume(1);
    }
    Ok(())
}

// Read ahead only one physical line. A malformed record consumes the whole line
// where its error is detected, so ON ERROR can advance to the next record.
fn physical_line(reader: &mut BufReader<File>) -> BasicResult<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(byte) = next(reader)? {
        bytes.push(byte);
        if byte == b'\r' {
            if peek(reader)? == Some(b'\n') {
                reader.consume(1);
                bytes.push(b'\n');
            }
            break;
        }
        if byte == b'\n' {
            break;
        }
    }
    std::str::from_utf8(&bytes).map_err(|_| invalid())?;
    Ok(bytes)
}

pub(crate) fn parse_number(text: &str) -> BasicResult<f64> {
    let bytes = text.as_bytes();
    let mut i = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    let mut digits = 0;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1;
    }
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return Err(invalid());
    }
    if bytes.get(i).is_some_and(|b| matches!(b, b'e' | b'E')) {
        i += 1;
        if bytes.get(i).is_some_and(|b| matches!(b, b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return Err(invalid());
        }
    }
    if i != bytes.len() {
        return Err(invalid());
    }
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(invalid)
}

pub(crate) fn format_number(value: f64) -> BasicResult<String> {
    if !value.is_finite() {
        return Err(invalid());
    }
    if value == 0.0 {
        return Ok("0".into());
    }
    let scientific = format!("{:.16e}", value.abs());
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let digits = mantissa.replace('.', "").trim_end_matches('0').to_string();
    let body = if (-4..17).contains(&exponent) {
        let position = exponent + 1;
        if position <= 0 {
            format!("0.{}{}", "0".repeat((-position) as usize), digits)
        } else if position as usize >= digits.len() {
            format!("{}{}", digits, "0".repeat(position as usize - digits.len()))
        } else {
            format!(
                "{}.{}",
                &digits[..position as usize],
                &digits[position as usize..]
            )
        }
    } else {
        let fraction = if digits.len() > 1 {
            format!(".{}", &digits[1..])
        } else {
            String::new()
        };
        format!(
            "{}{}e{}{exponent:02}",
            &digits[..1],
            fraction,
            if exponent >= 0 { "+" } else { "-" },
            exponent = exponent.abs()
        )
    };
    Ok(if value < 0.0 {
        format!("-{body}")
    } else {
        body
    })
}
