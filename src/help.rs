use std::collections::HashMap;
use std::sync::OnceLock;

const MAX_LINE_WIDTH: usize = 80;
const STATEMENTS_TSV: &str = include_str!("help/statements.tsv");
const FUNCTIONS_TSV: &str = include_str!("help/functions.tsv");

#[derive(Debug)]
struct HelpEntry {
    topic: String,
    aliases: Vec<String>,
    syntax: Vec<String>,
    parameters: Vec<String>,
    summary: String,
    related: Vec<String>,
}

#[derive(Debug)]
struct HelpCatalog {
    entries: Vec<HelpEntry>,
    lookup: HashMap<String, usize>,
}

static CATALOG: OnceLock<Result<HelpCatalog, String>> = OnceLock::new();

/// Renders the deliberately brief, immediate-mode HELP response.
pub(crate) fn render(query: &str) -> Result<Vec<String>, String> {
    let key = normalized_key(query);
    if key.is_empty() {
        return Ok(vec![
            "HELP [topic]".to_string(),
            "Shows syntax, parameters and a short description.".to_string(),
        ]);
    }

    let catalog = catalog()?;
    let Some(index) = catalog.lookup.get(&key).copied() else {
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
    Ok(render_entry(&catalog.entries[index]))
}

#[cfg(test)]
pub(crate) fn has_topic(topic: &str) -> bool {
    catalog()
        .ok()
        .is_some_and(|catalog| catalog.lookup.contains_key(&normalized_key(topic)))
}

fn catalog() -> Result<&'static HelpCatalog, String> {
    CATALOG
        .get_or_init(HelpCatalog::load)
        .as_ref()
        .map_err(Clone::clone)
}

impl HelpCatalog {
    fn load() -> Result<Self, String> {
        let mut entries = Vec::new();
        parse_catalog("statements.tsv", STATEMENTS_TSV, &mut entries)?;
        parse_catalog("functions.tsv", FUNCTIONS_TSV, &mut entries)?;

        let mut lookup = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            for name in std::iter::once(&entry.topic).chain(entry.aliases.iter()) {
                let key = normalized_key(name);
                if key.is_empty() {
                    return Err(format!("empty HELP topic or alias in {}", entry.topic));
                }
                if let Some(previous) = lookup.insert(key.clone(), index) {
                    return Err(format!(
                        "duplicate HELP name {key}: {} and {}",
                        entries[previous].topic, entry.topic
                    ));
                }
            }
        }

        for entry in &entries {
            for related in &entry.related {
                let key = normalized_key(related);
                if !lookup.contains_key(&key) {
                    return Err(format!(
                        "unresolved related HELP topic {related} in {}",
                        entry.topic
                    ));
                }
            }
        }

        Ok(Self { entries, lookup })
    }
}

fn parse_catalog(
    source_name: &str,
    source: &str,
    entries: &mut Vec<HelpEntry>,
) -> Result<(), String> {
    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() != 6 {
            return Err(format!(
                "{source_name}:{line_number}: expected 6 tab-separated columns, found {}",
                columns.len()
            ));
        }
        if [0usize, 2, 3, 4, 5]
            .into_iter()
            .any(|index| columns[index].trim().is_empty())
        {
            return Err(format!(
                "{source_name}:{line_number}: only the aliases column may be empty; use - for other empty lists"
            ));
        }

        let topic = columns[0].trim().to_string();
        let aliases = parse_list(columns[1], ',');
        let syntax = parse_list(columns[2], '|');
        let parameters = parse_list(columns[3], ';');
        let summary = columns[4].trim().to_string();
        let related = parse_list(columns[5], ',');

        if syntax.is_empty() {
            return Err(format!(
                "{source_name}:{line_number}: {} has no syntax",
                topic
            ));
        }
        if !summary.ends_with('.') || summary.lines().count() != 1 {
            return Err(format!(
                "{source_name}:{line_number}: {} summary must be one sentence ending in a period",
                topic
            ));
        }

        entries.push(HelpEntry {
            topic,
            aliases,
            syntax,
            parameters,
            summary,
            related,
        });
    }
    Ok(())
}

fn parse_list(field: &str, separator: char) -> Vec<String> {
    let field = field.trim();
    if field == "-" {
        return Vec::new();
    }
    if separator == '|' {
        field
            .split(" || ")
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        field
            .split(separator)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect()
    }
}

fn render_entry(entry: &HelpEntry) -> Vec<String> {
    let mut lines = Vec::new();

    for syntax in &entry.syntax {
        push_wrapped(&mut lines, "", syntax, "  ");
    }

    for parameter in &entry.parameters {
        push_wrapped(&mut lines, "  ", parameter, "    ");
    }

    push_wrapped(&mut lines, "", &entry.summary, "  ");
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
    fn catalog_is_valid_unique_and_related_topics_resolve() {
        let catalog = catalog().unwrap();
        assert!(!catalog.entries.is_empty());
        for (index, entry) in catalog.entries.iter().enumerate() {
            assert_eq!(
                catalog.lookup.get(&normalized_key(&entry.topic)),
                Some(&index)
            );
            for alias in &entry.aliases {
                assert_eq!(catalog.lookup.get(&normalized_key(alias)), Some(&index));
            }
            for related in &entry.related {
                assert!(catalog.lookup.contains_key(&normalized_key(related)));
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
    fn only_aliases_may_use_an_empty_catalog_column() {
        let mut entries = Vec::new();
        parse_catalog(
            "test.tsv",
            "THING\t\tTHING value\tvalue: Value\tDoes a thing.\t-\n",
            &mut entries,
        )
        .unwrap();
        assert!(entries[0].aliases.is_empty());

        let mut entries = Vec::new();
        assert!(parse_catalog(
            "test.tsv",
            "THING\t\tTHING value\t\tDoes a thing.\t-\n",
            &mut entries,
        )
        .is_err());
    }

    #[test]
    fn every_rendered_catalog_line_fits_the_console_width() {
        let catalog = catalog().unwrap();
        for entry in &catalog.entries {
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
