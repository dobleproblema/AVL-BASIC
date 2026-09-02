use crate::language::{self, LanguageTopic};

const MAX_LINE_WIDTH: usize = 80;

/// Renders the deliberately brief, immediate-mode HELP response.
pub(crate) fn render(query: &str) -> Result<Vec<String>, String> {
    let key = normalized_key(query);
    if key.is_empty() {
        return Ok(vec![
            "HELP [topic]".to_string(),
            "Shows syntax, parameters and a short description.".to_string(),
        ]);
    }

    let Some(entry) = language::topic(&key) else {
        let mut lines = Vec::new();
        push_wrapped(
            &mut lines,
            "No HELP entry for ",
            &safe_display_query(&key),
            "  ",
        );
        lines.push("Use HELP with an instruction or function name.".to_string());
        return Ok(lines);
    };
    Ok(render_entry(entry))
}

#[cfg(test)]
pub(crate) fn has_topic(topic: &str) -> bool {
    language::topic(&normalized_key(topic)).is_some()
}

fn render_entry(entry: &LanguageTopic) -> Vec<String> {
    let mut lines = Vec::new();

    for syntax in entry.syntax {
        push_wrapped(&mut lines, "", syntax, "  ");
    }

    for parameter in entry.parameters {
        push_wrapped(&mut lines, "  ", parameter, "    ");
    }

    push_wrapped(&mut lines, "", entry.summary, "  ");
    if !entry.related.is_empty() {
        push_wrapped(&mut lines, "Related: ", &entry.related.join(", "), "  ");
    }
    lines
}

fn normalized_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn safe_display_query(key: &str) -> String {
    key.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn push_wrapped(lines: &mut Vec<String>, prefix: &str, text: &str, continuation: &str) {
    let mut current = prefix.to_string();
    let mut has_text = false;

    for word in text.split_whitespace() {
        let mut remaining = word;
        loop {
            let separator = if has_text && !current.ends_with(char::is_whitespace) {
                " "
            } else {
                ""
            };
            let available = MAX_LINE_WIDTH.saturating_sub(char_len(&current) + char_len(separator));
            let remaining_len = char_len(remaining);

            if remaining_len <= available {
                current.push_str(separator);
                current.push_str(remaining);
                has_text = true;
                break;
            }

            if has_text {
                lines.push(current.trim_end().to_string());
                current = continuation.to_string();
                has_text = false;
                continue;
            }

            if available == 0 {
                if !current.trim().is_empty() {
                    lines.push(current.trim_end().to_string());
                }
                current = continuation.to_string();
                continue;
            }

            let (chunk, rest) = split_at_char(remaining, available);
            current.push_str(chunk);
            lines.push(current.trim_end().to_string());
            current = continuation.to_string();
            remaining = rest;
            if remaining.is_empty() {
                has_text = false;
                break;
            }
        }
    }

    if has_text || !current.trim().is_empty() {
        lines.push(current.trim_end().to_string());
    }
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn split_at_char(text: &str, count: usize) -> (&str, &str) {
    if count >= text.chars().count() {
        return (text, "");
    }
    let byte_index = text
        .char_indices()
        .nth(count)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.split_at(byte_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_names_are_available_to_help() {
        assert!(!language::topics().is_empty());
        for entry in language::topics() {
            assert!(std::ptr::eq(
                language::topic(&normalized_key(entry.topic)).unwrap(),
                entry
            ));
            for alias in entry.aliases {
                assert!(std::ptr::eq(
                    language::topic(&normalized_key(alias)).unwrap(),
                    entry
                ));
            }
        }
    }

    #[test]
    fn lookups_ignore_case_and_repeated_spaces_and_accept_aliases() {
        assert!(render("  right$  ").unwrap()[0].starts_with("RIGHT$("));
        assert_eq!(render("else    if").unwrap(), render("ELSEIF").unwrap());
        assert_eq!(render("cat").unwrap(), render("FILES").unwrap());
    }

    #[test]
    fn every_rendered_catalog_line_fits_the_console_width() {
        for entry in language::topics() {
            for line in render_entry(entry) {
                assert!(
                    char_len(&line) <= MAX_LINE_WIDTH,
                    "{} HELP line is too wide: {line}",
                    entry.topic
                );
            }
        }
    }

    #[test]
    fn wrapping_does_not_split_utf8_at_byte_boundaries() {
        let text = "á".repeat(MAX_LINE_WIDTH + 1);
        let mut lines = Vec::new();
        push_wrapped(&mut lines, "", &text, "");
        assert_eq!(lines.len(), 2);
        assert_eq!(char_len(&lines[0]), MAX_LINE_WIDTH);
        assert_eq!(char_len(&lines[1]), 1);
    }
}
