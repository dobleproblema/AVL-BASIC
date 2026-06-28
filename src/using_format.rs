use crate::value::format_basic_number;

pub(crate) fn format_using(value: f64, fmt: &str) -> String {
    let spec = parse_using_format(fmt);
    let Some(number) = normalize_using_number(value) else {
        return format_basic_number(value);
    };

    let (mut formatted, is_negative, decimal_char) = if spec.exp_digits > 0 {
        format_using_scientific(number, &spec)
    } else {
        format_using_fixed(number, &spec)
    };

    if spec.force_sign {
        formatted = left_pad_chars(&formatted, spec.visible_length, ' ');
    }

    if is_negative {
        formatted = insert_using_sign(&formatted, '-', decimal_char);
    } else if spec.force_sign {
        formatted = insert_using_sign(&formatted, '+', decimal_char);
    }

    formatted
}

pub(crate) fn valid_using_format(fmt: &str) -> bool {
    !fmt.is_empty() && fmt.chars().any(|ch| matches!(ch, '#' | '0'))
}

#[derive(Debug)]
struct UsingFormatSpec {
    force_sign: bool,
    european: bool,
    visible_length: usize,
    exp_digits: usize,
    int_format: String,
    frac_format: String,
    total_int_digits: usize,
    frac_digits: usize,
    group_positions: Vec<usize>,
}

fn parse_using_format(format_str: &str) -> UsingFormatSpec {
    let chars: Vec<char> = format_str.chars().collect();
    let force_sign = chars.first() == Some(&'+');
    let mut start_index = usize::from(force_sign);
    let european = chars.get(start_index) == Some(&',');
    if european {
        start_index += 1;
    }

    let visible_length = chars.len().saturating_sub(usize::from(european));
    let mut body: String = chars.iter().skip(start_index).collect();

    let exp_digits = if body.ends_with("^^^^^") {
        body.truncate(body.len() - 5);
        3
    } else if body.ends_with("^^^^") {
        body.truncate(body.len() - 4);
        2
    } else {
        0
    };

    let (int_format, frac_format) = extract_using_format_parts(&body);
    let total_int_digits = int_format
        .chars()
        .filter(|ch| matches!(*ch, '0' | '#'))
        .count();
    let frac_digits = frac_format
        .chars()
        .filter(|ch| matches!(*ch, '0' | '#'))
        .count();
    let group_positions = find_using_group_separator_positions(&int_format);

    UsingFormatSpec {
        force_sign,
        european,
        visible_length,
        exp_digits,
        int_format,
        frac_format,
        total_int_digits,
        frac_digits,
        group_positions,
    }
}

fn normalize_using_number(value: f64) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    format!("{value:.13e}").parse::<f64>().ok()
}

fn extract_using_format_parts(format_str: &str) -> (String, String) {
    if let Some(split_index) = format_str.chars().position(|ch| ch == '.') {
        let int_format: String = format_str.chars().take(split_index).collect();
        let mut frac_format = String::from(".");
        frac_format.extend(format_str.chars().skip(split_index + 1));
        (int_format, frac_format)
    } else {
        (format_str.to_string(), String::new())
    }
}

fn find_using_group_separator_positions(int_format: &str) -> Vec<usize> {
    let chars: Vec<char> = int_format.chars().collect();
    let mut positions = Vec::new();
    for (idx, ch) in chars.iter().enumerate() {
        if *ch != ',' {
            continue;
        }
        let left_placeholders = chars[..idx]
            .iter()
            .filter(|ch| matches!(**ch, '0' | '#'))
            .count();
        let right_placeholders = chars[idx + 1..]
            .iter()
            .filter(|ch| matches!(**ch, '0' | '#'))
            .count();
        if left_placeholders > 0 && right_placeholders > 0 && right_placeholders % 3 == 0 {
            positions.push(idx);
        }
    }
    positions
}

fn decimal_to_fixed_parts(value: f64, frac_digits: usize) -> (f64, String, String) {
    let mut rounded = round_to_fraction_digits(value, frac_digits);
    if rounded == 0.0 {
        rounded = 0.0;
    }
    let abs_rounded = rounded.abs();
    let fixed = if frac_digits > 0 {
        format!("{abs_rounded:.frac_digits$}")
    } else {
        format!("{abs_rounded:.0}")
    };
    let (int_str, frac_str) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    (rounded, int_str.to_string(), frac_str.to_string())
}

fn round_to_fraction_digits(value: f64, frac_digits: usize) -> f64 {
    let rounded = if frac_digits > 0 {
        format!("{value:.frac_digits$}")
    } else {
        format!("{value:.0}")
    };
    rounded.parse::<f64>().unwrap_or(value)
}

fn render_using_integer(
    int_format: &str,
    total_int_digits: usize,
    int_str: &str,
    group_positions: &[usize],
) -> (String, Vec<usize>) {
    let first_placeholder = int_format.chars().position(|ch| matches!(ch, '0' | '#'));
    let mut extra_len = 0usize;

    let int_part = if total_int_digits == 0 {
        let mut text = int_format.to_string();
        if int_str != "0" {
            text.push_str(int_str);
        }
        text
    } else if int_str.chars().count() > total_int_digits {
        let int_chars: Vec<char> = int_str.chars().collect();
        let split_at = int_chars.len() - total_int_digits;
        let extra_digits: String = int_chars[..split_at].iter().collect();
        let main_digits: String = int_chars[split_at..].iter().collect();
        let mapped_main = map_using_format(int_format, &main_digits, true);
        extra_len = extra_digits.chars().count();

        if let Some(first_placeholder) = first_placeholder {
            insert_str_at_char(&mapped_main, first_placeholder, &extra_digits)
        } else {
            format!("{extra_digits}{mapped_main}")
        }
    } else {
        let pad_char = if int_format.chars().any(|ch| ch == '0') {
            '0'
        } else {
            ' '
        };
        let int_str_padded = left_pad_chars(int_str, total_int_digits, pad_char);
        map_using_format(int_format, &int_str_padded, true)
    };

    let output_group_positions = group_positions
        .iter()
        .map(|idx| {
            if first_placeholder.is_some_and(|first| *idx >= first) {
                idx + extra_len
            } else {
                *idx
            }
        })
        .collect();

    (int_part, output_group_positions)
}

fn suppress_unused_group_separators(int_part: &str, output_group_positions: &[usize]) -> String {
    if output_group_positions.is_empty() {
        return int_part.to_string();
    }

    let mut chars: Vec<char> = int_part.chars().collect();
    let mut has_digit_left = vec![false; chars.len()];
    let mut seen_digit = false;
    for (idx, ch) in chars.iter().enumerate() {
        has_digit_left[idx] = seen_digit;
        if ch.is_ascii_digit() {
            seen_digit = true;
        }
    }

    let mut has_digit_right = vec![false; chars.len()];
    seen_digit = false;
    for (idx, ch) in chars.iter().enumerate().rev() {
        has_digit_right[idx] = seen_digit;
        if ch.is_ascii_digit() {
            seen_digit = true;
        }
    }

    for idx in output_group_positions {
        if *idx < chars.len()
            && chars[*idx] == ','
            && (!has_digit_left[*idx] || !has_digit_right[*idx])
        {
            chars[*idx] = ' ';
        }
    }

    chars.iter().collect()
}

fn apply_using_locale(
    int_part: String,
    frac_part: String,
    output_group_positions: &[usize],
    european: bool,
) -> (String, String, char) {
    if !european {
        return (int_part, frac_part, '.');
    }

    let mut int_chars: Vec<char> = int_part.chars().collect();
    for idx in output_group_positions {
        if *idx < int_chars.len() && int_chars[*idx] == ',' {
            int_chars[*idx] = '.';
        }
    }

    let mut frac_chars: Vec<char> = frac_part.chars().collect();
    let decimal_char = if frac_chars.first() == Some(&'.') {
        frac_chars[0] = ',';
        ','
    } else {
        '.'
    };

    (
        int_chars.iter().collect(),
        frac_chars.iter().collect(),
        decimal_char,
    )
}

fn format_using_fixed(number: f64, spec: &UsingFormatSpec) -> (String, bool, char) {
    let (rounded_number, int_str, frac_str) = decimal_to_fixed_parts(number, spec.frac_digits);
    let is_negative = rounded_number < 0.0 && rounded_number != 0.0;

    let (int_part, output_group_positions) = render_using_integer(
        &spec.int_format,
        spec.total_int_digits,
        &int_str,
        &spec.group_positions,
    );
    let int_part = suppress_unused_group_separators(&int_part, &output_group_positions);

    let frac_part = if spec.frac_format.is_empty() {
        String::new()
    } else {
        map_using_format(&spec.frac_format, &frac_str, false)
    };

    let (int_part, frac_part, decimal_char) =
        apply_using_locale(int_part, frac_part, &output_group_positions, spec.european);
    (format!("{int_part}{frac_part}"), is_negative, decimal_char)
}

fn format_using_scientific(number: f64, spec: &UsingFormatSpec) -> (String, bool, char) {
    let mantissa_slots = spec.total_int_digits.max(1);
    let is_negative = number < 0.0;
    let abs_number = number.abs();

    let (mut exponent, mantissa) = if abs_number == 0.0 {
        (0i32, 0.0)
    } else {
        let mut exponent = abs_number.log10().floor() as i32 - (mantissa_slots as i32 - 1);
        let mut mantissa = abs_number / 10f64.powi(exponent);
        mantissa = round_to_fraction_digits(mantissa, spec.frac_digits);
        let threshold = 10f64.powi(mantissa_slots as i32);
        if mantissa >= threshold {
            exponent += 1;
            mantissa = round_to_fraction_digits(mantissa / 10.0, spec.frac_digits);
        }
        (exponent, mantissa)
    };

    if mantissa == 0.0 {
        exponent = 0;
    }

    let (_, int_str, frac_str) = decimal_to_fixed_parts(mantissa, spec.frac_digits);
    let (int_part, output_group_positions) = render_using_integer(
        &spec.int_format,
        spec.total_int_digits,
        &int_str,
        &spec.group_positions,
    );
    let int_part = suppress_unused_group_separators(&int_part, &output_group_positions);
    let frac_part = if spec.frac_format.is_empty() {
        String::new()
    } else {
        map_using_format(&spec.frac_format, &frac_str, false)
    };
    let (int_part, frac_part, decimal_char) =
        apply_using_locale(int_part, frac_part, &output_group_positions, spec.european);

    let exponent_str = left_pad_chars(&exponent.abs().to_string(), spec.exp_digits, '0');
    let exp_sign = if exponent >= 0 { '+' } else { '-' };
    (
        format!("{int_part}{frac_part}E{exp_sign}{exponent_str}"),
        is_negative && mantissa != 0.0,
        decimal_char,
    )
}

fn map_using_format(format_part: &str, number_part: &str, reverse: bool) -> String {
    let mut output: Vec<char> = format_part.chars().collect();
    let num_digits: Vec<char> = number_part.chars().collect();

    if reverse {
        let mut num_index = num_digits.len();
        for fmt_index in (0..output.len()).rev() {
            if matches!(output[fmt_index], '0' | '#') {
                if num_index > 0 {
                    num_index -= 1;
                    output[fmt_index] = num_digits[num_index];
                } else {
                    output[fmt_index] = if output[fmt_index] == '0' { '0' } else { ' ' };
                }
            }
        }
    } else {
        let mut num_index = 0usize;
        for ch in &mut output {
            if matches!(*ch, '0' | '#') {
                if num_index < num_digits.len() {
                    *ch = num_digits[num_index];
                    num_index += 1;
                } else {
                    *ch = if *ch == '0' { '0' } else { ' ' };
                }
            }
        }
    }

    output.iter().collect()
}

fn insert_using_sign(formatted_number: &str, sign: char, decimal_char: char) -> String {
    let chars: Vec<char> = formatted_number.chars().collect();
    let Some(first_digit) = chars.iter().position(|ch| ch.is_ascii_digit()) else {
        return format!("{sign}{formatted_number}");
    };

    if first_digit > 0 && matches!(chars[first_digit - 1], ' ' | '+') {
        return replace_char_at(&chars, first_digit - 1, sign);
    }

    if let Some(last_space) = chars[..first_digit].iter().rposition(|ch| *ch == ' ') {
        return replace_char_at(&chars, last_space, sign);
    }

    if first_digit > 0 && chars[first_digit - 1] == decimal_char {
        return insert_char_at(&chars, first_digit - 1, sign);
    }

    let int_part_end = chars
        .iter()
        .position(|ch| *ch == decimal_char)
        .unwrap_or(chars.len());
    let search_limit = (first_digit + 1).min(int_part_end);
    for idx in 0..search_limit {
        if chars[idx] == '0' {
            if int_part_end - idx == 1 {
                continue;
            }
            return replace_char_at(&chars, idx, sign);
        }
    }

    insert_char_at(&chars, first_digit, sign)
}

fn left_pad_chars(text: &str, width: usize, pad: char) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_string()
    } else {
        format!("{}{}", pad.to_string().repeat(width - len), text)
    }
}

fn insert_str_at_char(text: &str, char_index: usize, insertion: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx == char_index {
            out.push_str(insertion);
        }
        out.push(ch);
    }
    if char_index >= text.chars().count() {
        out.push_str(insertion);
    }
    out
}

fn insert_char_at(chars: &[char], char_index: usize, insertion: char) -> String {
    let mut out = String::new();
    for (idx, ch) in chars.iter().enumerate() {
        if idx == char_index {
            out.push(insertion);
        }
        out.push(*ch);
    }
    if char_index >= chars.len() {
        out.push(insertion);
    }
    out
}

fn replace_char_at(chars: &[char], char_index: usize, replacement: char) -> String {
    chars
        .iter()
        .enumerate()
        .map(|(idx, ch)| if idx == char_index { replacement } else { *ch })
        .collect()
}
