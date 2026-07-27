#[cfg(any(unix, test))]
use std::time::{Duration, Instant};

pub(crate) const HOME: u8 = 1;
pub(crate) const END: u8 = 4;
pub(crate) const BACKSPACE: u8 = 8;
pub(crate) const TAB: u8 = 9;
pub(crate) const PAGE_UP: u8 = 11;
pub(crate) const PAGE_DOWN: u8 = 12;
pub(crate) const ENTER: u8 = 13;
pub(crate) const INSERT: u8 = 22;
pub(crate) const ESCAPE: u8 = 27;
pub(crate) const LEFT: u8 = 28;
pub(crate) const RIGHT: u8 = 29;
pub(crate) const UP: u8 = 30;
pub(crate) const DOWN: u8 = 31;
pub(crate) const DELETE: u8 = 127;
pub(crate) const F1: u8 = 128;
pub(crate) const F12: u8 = 139;

#[cfg(any(unix, test))]
const ESCAPE_SEQUENCE_TIMEOUT: Duration = Duration::from_millis(25);

#[cfg(any(unix, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalDecode {
    NeedMore,
    Complete { code: Option<u8>, consumed: usize },
}

#[cfg(any(unix, test))]
#[derive(Debug, Default)]
pub(crate) struct TerminalInputDecoder {
    pending: Vec<u8>,
    escape_started: Option<Instant>,
}

#[cfg(any(unix, test))]
impl TerminalInputDecoder {
    pub(crate) fn push(&mut self, byte: u8) {
        self.pending.push(byte);
    }

    pub(crate) fn clear(&mut self) {
        self.pending.clear();
        self.escape_started = None;
    }

    pub(crate) fn next_code(&mut self, now: Instant) -> Option<u8> {
        loop {
            if self.pending.is_empty() {
                self.escape_started = None;
                return None;
            }

            match decode_terminal_input(&self.pending) {
                TerminalDecode::Complete { code, consumed } => {
                    self.pending.drain(..consumed);
                    self.escape_started = None;
                    if code.is_some() {
                        return code;
                    }
                }
                TerminalDecode::NeedMore => {
                    let started = self.escape_started.get_or_insert(now);
                    if now.duration_since(*started) < ESCAPE_SEQUENCE_TIMEOUT {
                        return None;
                    }

                    if self.pending.as_slice() == [ESCAPE] {
                        self.pending.clear();
                        self.escape_started = None;
                        return Some(ESCAPE);
                    }

                    // An incomplete escape sequence must not leak its bytes as
                    // ordinary BASIC characters.
                    self.clear();
                    return None;
                }
            }
        }
    }
}

pub(crate) fn function_key_code(number: u8) -> Option<u8> {
    (1..=12).contains(&number).then_some(F1 + number - 1)
}

#[cfg(any(windows, test))]
pub(crate) fn windows_extended_key_code(scan_code: u8) -> Option<u8> {
    match scan_code {
        59..=68 => function_key_code(scan_code - 58),
        133 => function_key_code(11),
        134 => function_key_code(12),
        71 => Some(HOME),
        72 => Some(UP),
        73 => Some(PAGE_UP),
        75 => Some(LEFT),
        77 => Some(RIGHT),
        79 => Some(END),
        80 => Some(DOWN),
        81 => Some(PAGE_DOWN),
        82 => Some(INSERT),
        83 => Some(DELETE),
        _ => None,
    }
}

#[cfg(any(unix, test))]
fn decode_terminal_input(input: &[u8]) -> TerminalDecode {
    let Some(&first) = input.first() else {
        return TerminalDecode::NeedMore;
    };
    if first != ESCAPE {
        return TerminalDecode::Complete {
            code: Some(first),
            consumed: 1,
        };
    }
    if input.len() == 1 {
        return TerminalDecode::NeedMore;
    }

    match input[1] {
        b'[' => decode_csi(input),
        b'O' => decode_ss3(input),
        _ => TerminalDecode::Complete {
            code: None,
            consumed: 2,
        },
    }
}

#[cfg(any(unix, test))]
fn decode_csi(input: &[u8]) -> TerminalDecode {
    if input.len() >= 3 && input[2] == b'[' {
        if input.len() < 4 {
            return TerminalDecode::NeedMore;
        }
        let code = match input[3] {
            b'A'..=b'E' => function_key_code(input[3] - b'A' + 1),
            _ => None,
        };
        return TerminalDecode::Complete { code, consumed: 4 };
    }

    let Some(final_index) = input
        .iter()
        .enumerate()
        .skip(2)
        .find_map(|(index, byte)| (0x40..=0x7e).contains(byte).then_some(index))
    else {
        return TerminalDecode::NeedMore;
    };

    let final_byte = input[final_index];
    let params = &input[2..final_index];
    let code = match final_byte {
        b'A' => Some(UP),
        b'B' => Some(DOWN),
        b'C' => Some(RIGHT),
        b'D' => Some(LEFT),
        b'H' => Some(HOME),
        b'F' => Some(END),
        b'Z' => Some(TAB),
        b'~' => decode_csi_tilde(params),
        _ => None,
    };
    TerminalDecode::Complete {
        code,
        consumed: final_index + 1,
    }
}

#[cfg(any(unix, test))]
fn decode_csi_tilde(params: &[u8]) -> Option<u8> {
    let first_param = params
        .split(|byte| *byte == b';')
        .next()
        .unwrap_or_default();
    let value = std::str::from_utf8(first_param).ok()?.parse::<u8>().ok()?;
    match value {
        1 | 7 => Some(HOME),
        2 => Some(INSERT),
        3 => Some(DELETE),
        4 | 8 => Some(END),
        5 => Some(PAGE_UP),
        6 => Some(PAGE_DOWN),
        11..=15 => function_key_code(value - 10),
        17..=21 => function_key_code(value - 11),
        23 | 24 => function_key_code(value - 12),
        _ => None,
    }
}

#[cfg(any(unix, test))]
fn decode_ss3(input: &[u8]) -> TerminalDecode {
    if input.len() < 3 {
        return TerminalDecode::NeedMore;
    }
    let code = match input[2] {
        b'A' => Some(UP),
        b'B' => Some(DOWN),
        b'C' => Some(RIGHT),
        b'D' => Some(LEFT),
        b'H' => Some(HOME),
        b'F' => Some(END),
        b'P'..=b'S' => function_key_code(input[2] - b'P' + 1),
        _ => None,
    };
    TerminalDecode::Complete { code, consumed: 3 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_fragments(bytes: &[u8]) -> Option<u8> {
        let mut decoder = TerminalInputDecoder::default();
        let now = Instant::now();
        for byte in bytes {
            decoder.push(*byte);
            if let Some(code) = decoder.next_code(now) {
                return Some(code);
            }
        }
        None
    }

    #[test]
    fn windows_extended_keys_use_canonical_codes() {
        assert_eq!(windows_extended_key_code(80), Some(DOWN));
        assert_eq!(windows_extended_key_code(82), Some(INSERT));
        assert_eq!(windows_extended_key_code(83), Some(DELETE));
        assert_eq!(windows_extended_key_code(59), Some(F1));
        assert_eq!(windows_extended_key_code(134), Some(F12));
        assert_eq!(windows_extended_key_code(84), None);
    }

    #[test]
    fn ansi_navigation_sequences_are_one_logical_key() {
        assert_eq!(decode_fragments(b"\x1b[B"), Some(DOWN));
        assert_eq!(decode_fragments(b"\x1b[1;2D"), Some(LEFT));
        assert_eq!(decode_fragments(b"\x1b[2~"), Some(INSERT));
        assert_eq!(decode_fragments(b"\x1b[3~"), Some(DELETE));
        assert_eq!(decode_fragments(b"\x1bOH"), Some(HOME));
        assert_eq!(decode_fragments(b"\x1bOF"), Some(END));
    }

    #[test]
    fn ansi_function_keys_use_reserved_codes() {
        assert_eq!(decode_fragments(b"\x1bOP"), function_key_code(1));
        assert_eq!(decode_fragments(b"\x1b[15~"), function_key_code(5));
        assert_eq!(decode_fragments(b"\x1b[24~"), function_key_code(12));
        assert_eq!(decode_fragments(b"\x1b[[E"), function_key_code(5));
    }

    #[test]
    fn escape_is_delayed_and_unknown_sequences_do_not_leak() {
        let mut decoder = TerminalInputDecoder::default();
        let now = Instant::now();
        decoder.push(ESCAPE);
        assert_eq!(decoder.next_code(now), None);
        assert_eq!(
            decoder.next_code(now + ESCAPE_SEQUENCE_TIMEOUT),
            Some(ESCAPE)
        );

        decoder.push(ESCAPE);
        decoder.push(b'[');
        decoder.push(b'9');
        decoder.push(b'9');
        decoder.push(b'~');
        assert_eq!(decoder.next_code(now), None);
        assert!(decoder.pending.is_empty());
    }
}
