//! Syntax shared by numbered-line label preparation and the editor.

/// Labels have their own namespace, with the language's reserved words excluded.
pub fn label_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !crate::reserved::is_reserved_identifier_name(name)
}

/// Accept code only, with an optional apostrophe comment after the colon.
/// The numbered prefix is handled by the caller.
pub fn label_definition(source: &str) -> Option<&str> {
    let declaration = source.split_once('\'').map_or(source, |(code, _)| code);
    let name = declaration.trim().strip_suffix(':')?;
    label_identifier(name).then_some(name)
}

fn trimmed_range(source: &str, start: usize, end: usize) -> Option<std::ops::Range<usize>> {
    let part = &source[start..end];
    let text = part.trim();
    (!text.is_empty()).then(|| {
        let left = part.len() - part.trim_start().len();
        start + left..start + left + text.len()
    })
}

/// Split comma operands without mistaking commas in strings or expressions for separators.
fn arguments(source: &str, start: usize) -> Vec<std::ops::Range<usize>> {
    let mut result = Vec::new();
    let mut begin = start;
    let mut quoted = false;
    let mut depth = 0usize;
    for (i, ch) in source[start..].char_indices() {
        let i = i + start;
        match ch {
            '"' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth = depth.saturating_sub(1),
            ',' if !quoted && depth == 0 => {
                // Keep operand positions while editing incomplete source.
                // RENUM's third/fourth slots and CHAIN's optional entry must
                // never shift when an earlier operand is empty.
                result.push(trimmed_range(source, begin, i).unwrap_or(i..i));
                begin = i + 1;
            }
            _ => {}
        }
    }
    result.push(trimmed_range(source, begin, source.len()).unwrap_or(source.len()..source.len()));
    result
}

fn range_operands(source: &str, range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    if let Some(dash) = source[range.clone()].find('-') {
        [
            trimmed_range(source, range.start, range.start + dash),
            trimmed_range(source, range.start + dash + 1, range.end),
        ]
        .into_iter()
        .flatten()
        .collect()
    } else {
        vec![range]
    }
}

fn words(source: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let (mut i, mut depth, mut quoted) = (0, 0usize, false);
    while i < bytes.len() {
        match bytes[i] {
            b'"' => quoted = !quoted,
            b'(' | b'[' if !quoted => depth += 1,
            b')' | b']' if !quoted => depth = depth.saturating_sub(1),
            b'\'' if !quoted => break,
            ch if !quoted && depth == 0 && (ch.is_ascii_alphabetic() || ch == b'_') => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'$'))
                {
                    i += 1;
                }
                result.push(start..i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    result
}

/// Complete operands identifying existing lines. Offsets refer to the supplied
/// unnumbered command; callers distinguish expressions from bare label names.
pub(crate) fn target_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    let tokens = words(source);
    let Some(first) = tokens.first() else {
        return Vec::new();
    };
    if !source[..first.start].trim().is_empty() {
        return Vec::new();
    }
    match source[first.clone()].to_ascii_uppercase().as_str() {
        "GOTO" | "GOSUB" | "RESUME" => arguments(source, first.end),
        "ON" | "AFTER" | "EVERY" | "IF" => tokens
            .iter()
            .skip(1)
            .find(|r| {
                matches!(
                    source[(*r).clone()].to_ascii_uppercase().as_str(),
                    "GOTO" | "GOSUB"
                )
            })
            .map_or_else(Vec::new, |word| arguments(source, word.end)),
        "RESTORE" => trimmed_range(source, first.end, source.len())
            .filter(|range| source[range.clone()].bytes().all(|ch| ch.is_ascii_digit()))
            .into_iter()
            .collect(),
        "RUN" | "EDIT" => trimmed_range(source, first.end, source.len())
            .into_iter()
            .collect(),
        "LIST" | "DELETE" => trimmed_range(source, first.end, source.len())
            .map_or_else(Vec::new, |range| range_operands(source, range)),
        "RENUM" => arguments(source, first.end)
            .into_iter()
            .skip(2)
            .take(2)
            .collect(),
        "CHAIN" => {
            let start = tokens
                .get(1)
                .filter(|r| source[(*r).clone()].eq_ignore_ascii_case("MERGE"))
                .map_or(first.end, |word| word.end);
            let args = arguments(source, start);
            let mut result: Vec<_> = args.get(1).cloned().into_iter().collect();
            for arg in args.iter().skip(2) {
                let text = &source[arg.clone()];
                if text
                    .get(..6)
                    .is_some_and(|s| s.eq_ignore_ascii_case("DELETE"))
                    && text.as_bytes().get(6).is_some_and(u8::is_ascii_whitespace)
                {
                    if let Some(range) = trimmed_range(source, arg.start + 6, arg.end) {
                        result.extend(range_operands(source, range));
                    }
                }
            }
            result
        }
        _ => Vec::new(),
    }
}

pub(crate) fn global_target(source: &str) -> bool {
    let tokens = words(source);
    let Some(first) = tokens.first() else {
        return false;
    };
    match source[first.clone()].to_ascii_uppercase().as_str() {
        "AFTER" | "EVERY" | "RUN" | "CHAIN" | "LIST" | "EDIT" | "DELETE" | "RENUM" => true,
        "ON" => tokens.get(1).is_some_and(|r| {
            matches!(
                source[r.clone()].to_ascii_uppercase().as_str(),
                "MOUSE" | "SQ"
            )
        }),
        _ => false,
    }
}

pub(crate) fn normalize_control_source(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut quoted = false;
    let mut whitespace = false;
    for ch in source.chars() {
        if ch == '"' {
            quoted = !quoted;
        }
        if !quoted && ch.is_ascii_whitespace() {
            whitespace = true;
        } else {
            if whitespace && !output.is_empty() {
                output.push(' ');
            }
            whitespace = false;
            output.push(ch);
        }
    }
    output
}

/// Line operands in a complete source line, excluding strings, comments and DATA.
/// Bare names are destinations only immediately after THEN or ELSE.
fn collect_line_ranges(source: &str, renumber: bool) -> Vec<std::ops::Range<usize>> {
    fn segment(
        source: &str,
        begin: usize,
        end: usize,
        short: bool,
        renumber: bool,
        result: &mut Vec<std::ops::Range<usize>>,
    ) {
        let part = &source[begin..end];
        let mut ranges = target_ranges(part);
        if renumber
            && part
                .trim_start()
                .split_whitespace()
                .next()
                .is_some_and(|word| word.eq_ignore_ascii_case("CHAIN"))
            && !ranges.is_empty()
        {
            // Entry belongs to the loaded/merged file. DELETE operands still
            // refer to the original program and remain eligible for RENUM.
            ranges.remove(0);
        }
        result.extend(
            ranges
                .into_iter()
                .map(|range| begin + range.start..begin + range.end),
        );
        if short
            && (label_identifier(part.trim())
                || (renumber
                    && !part.trim().is_empty()
                    && part.trim().bytes().all(|ch| ch.is_ascii_digit())))
        {
            if let Some(range) = trimmed_range(source, begin, end) {
                result.push(range);
            }
        }
    }
    let bytes = source.as_bytes();
    let (mut i, mut begin, mut depth) = (0, 0, 0usize);
    let (mut quoted, mut data, mut short) = (false, false, false);
    let mut result = Vec::new();
    while i < bytes.len() {
        match bytes[i] {
            b'"' => quoted = !quoted,
            b'\'' if !quoted => {
                segment(source, begin, i, short, renumber, &mut result);
                return result;
            }
            b'(' | b'[' if !quoted => depth += 1,
            b')' | b']' if !quoted => depth = depth.saturating_sub(1),
            b':' if !quoted && depth == 0 => {
                segment(source, begin, i, short, renumber, &mut result);
                begin = i + 1;
                short = false;
                data = false;
            }
            ch if !quoted && depth == 0 && (ch.is_ascii_alphabetic() || ch == b'_') => {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'$'))
                {
                    i += 1;
                }
                let word = &source[start..i];
                if !data && word.eq_ignore_ascii_case("REM") {
                    segment(source, begin, start, short, renumber, &mut result);
                    return result;
                }
                if word.eq_ignore_ascii_case("DATA") {
                    data = true;
                }
                if !data && (word.eq_ignore_ascii_case("THEN") || word.eq_ignore_ascii_case("ELSE"))
                {
                    segment(source, begin, start, short, renumber, &mut result);
                    begin = i;
                    short = true;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    segment(source, begin, source.len(), short, renumber, &mut result);
    result
}

pub(crate) fn renumber_target_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    collect_line_ranges(source, true)
}

/// Identifier casing belongs to variables, not to declarations or destinations.
pub(crate) fn symbolic_name_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    let trimmed = source.trim_start();
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    let code = if digits > 0 {
        &trimmed[digits..]
    } else {
        source
    };
    let offset = source.len() - code.len();
    if let Some(name) = label_definition(code) {
        let start = offset + code.len() - code.trim_start().len();
        return vec![start..start + name.len()];
    }
    collect_line_ranges(code, false)
        .into_iter()
        .filter(|range| label_identifier(&code[range.clone()]))
        .map(|range| offset + range.start..offset + range.end)
        .collect()
}
