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

/// Target operands of the explicitly supported symbolic control statements.
pub(crate) fn target_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    let words: Vec<_> = source.split_whitespace().collect();
    let Some(first) = words.first() else {
        return Vec::new();
    };
    let destination_keyword = if first.eq_ignore_ascii_case("GOTO")
        || first.eq_ignore_ascii_case("GOSUB")
    {
        Some(*first)
    } else if first.eq_ignore_ascii_case("RESUME") {
        (!words
            .get(1)
            .is_some_and(|word| word.eq_ignore_ascii_case("NEXT")))
        .then_some(*first)
    } else if first.eq_ignore_ascii_case("ON")
        && !words.get(1).is_some_and(|word| {
            word.eq_ignore_ascii_case("MOUSE")
                || word.eq_ignore_ascii_case("SQ")
                || word.to_ascii_uppercase().starts_with("SQ(")
        })
    {
        words
            .iter()
            .skip(1)
            .find(|word| word.eq_ignore_ascii_case("GOTO") || word.eq_ignore_ascii_case("GOSUB"))
            .copied()
    } else {
        None
    };
    let start = destination_keyword
        .map(|word| word.as_ptr() as usize - source.as_ptr() as usize + word.len());
    let Some(start) = start else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut offset = start;
    for part in source[start..].split(',') {
        let trimmed = part.trim();
        let left = part.len() - part.trim_start().len();
        ranges.push(offset + left..offset + left + trimmed.len());
        offset += part.len() + 1;
    }
    ranges
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

/// Identifier casing belongs to variables, not to declarations or destinations.
pub(crate) fn symbolic_name_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    let trimmed = source.trim_start();
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    let code = if digits > 0 {
        &trimmed[digits..]
    } else {
        source
    };
    if let Some(name) = label_definition(code) {
        let start = source.len() - code.len() + code.len() - code.trim_start().len();
        return vec![start..start + name.len()];
    }
    let bytes = source.as_bytes();
    let mut result = Vec::new();
    let mut i = 0;
    let mut quoted = false;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            quoted = !quoted;
            i += 1;
            continue;
        }
        if !quoted && bytes[i] == b'\'' {
            break;
        }
        if !quoted && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'_') {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            let word = &source[start..i];
            if word.eq_ignore_ascii_case("REM") {
                break;
            }
            if ["GOTO", "GOSUB", "RESUME"]
                .iter()
                .any(|w| word.eq_ignore_ascii_case(w))
            {
                loop {
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let start = i;
                    while i < bytes.len()
                        && (bytes[i].is_ascii_alphanumeric()
                            || bytes[i] == b'_'
                            || bytes[i] == b'$')
                    {
                        i += 1;
                    }
                    if start == i {
                        break;
                    }
                    if label_identifier(&source[start..i]) {
                        result.push(start..i);
                    }
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if bytes.get(i) != Some(&b',') {
                        break;
                    }
                    i += 1;
                }
            }
            continue;
        }
        i += 1;
    }
    result
}
