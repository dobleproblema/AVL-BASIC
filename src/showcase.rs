use std::collections::HashSet;

const CATALOG_SOURCE: &str = include_str!("../samples/catalog.tsv");
const CATALOG_HEADER: &str = "filename\tcategory\tfeatured_order\ttitle\tdescription\ttechniques";
const EXPECTED_SAMPLE_COUNT: usize = 115;
const EXPECTED_FEATURED_COUNT: usize = 20;
const OUTPUT_WIDTH: usize = 80;

#[derive(Debug, Clone, Copy)]
struct Sample<'a> {
    filename: &'a str,
    category: &'a str,
    featured_order: Option<usize>,
    title: &'a str,
    description: &'a str,
    techniques: &'a str,
}

pub(crate) fn samples_lines(samples_available: bool) -> Result<Vec<String>, String> {
    let samples = parse_catalog()?;
    let categories = category_order(&samples);
    let heading = format!("AVL BASIC SAMPLES - {} programs", samples.len());
    let mut lines = vec![
        heading.clone(),
        "=".repeat(heading.chars().count()),
        String::new(),
    ];

    for category in categories {
        lines.push(format!("[{category}]"));
        for sample in samples.iter().filter(|sample| sample.category == category) {
            let prefix = format!("  {} - ", sample.filename);
            let continuation = " ".repeat(prefix.chars().count());
            append_wrapped(&mut lines, &prefix, &continuation, sample.title);
        }
        lines.push(String::new());
    }

    lines.push(format!(
        "Type TOUR for {EXPECTED_FEATURED_COUNT} highlights from this catalog."
    ));
    append_run_guidance(&mut lines, samples_available);
    Ok(lines)
}

pub(crate) fn tour_lines(samples_available: bool) -> Result<Vec<String>, String> {
    let samples = parse_catalog()?;
    let mut featured: Vec<&Sample<'_>> = samples
        .iter()
        .filter(|sample| sample.featured_order.is_some())
        .collect();
    featured.sort_by_key(|sample| sample.featured_order);

    let heading = format!("AVL BASIC TOUR - {} highlights", featured.len());
    let mut lines = vec![
        heading.clone(),
        "=".repeat(heading.chars().count()),
        String::new(),
    ];

    for sample in featured {
        let number = sample
            .featured_order
            .expect("featured sample without order");
        let heading = format!("{number}. {} [{}]", sample.title, sample.category);
        append_wrapped(&mut lines, "", "   ", &heading);
        append_wrapped(&mut lines, "   ", "   ", sample.description);
        let techniques = sample
            .techniques
            .split(';')
            .map(str::trim)
            .collect::<Vec<_>>()
            .join("; ");
        append_wrapped(&mut lines, "   Shows: ", "          ", &techniques);
        lines.push(format!("   RUN \"/samples/{}\"", sample.filename));
        lines.push(String::new());
    }

    lines.push("Type SAMPLES for the full catalog.".to_string());
    append_run_guidance(&mut lines, samples_available);
    Ok(lines)
}

fn append_run_guidance(lines: &mut Vec<String>, samples_available: bool) {
    if samples_available {
        lines.push("Visual gallery and annotated catalog: samples/README.md".to_string());
    } else {
        lines.push(
            "The catalog is available, but /samples is not present in this session.".to_string(),
        );
        lines
            .push("Start AVL BASIC from a package or checkout that includes samples/.".to_string());
    }
}

fn parse_catalog() -> Result<Vec<Sample<'static>>, String> {
    let mut lines = CATALOG_SOURCE.lines();
    let header = lines
        .next()
        .unwrap_or_default()
        .trim_start_matches('\u{feff}')
        .trim_end_matches('\r');
    if header != CATALOG_HEADER {
        return Err("Invalid embedded sample catalog header.".to_string());
    }

    let mut samples = Vec::new();
    let mut filenames = HashSet::new();
    let mut featured_orders = HashSet::new();

    for (index, raw_line) in lines.enumerate() {
        let line_number = index + 2;
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 6 {
            return Err(format!(
                "Invalid embedded sample catalog row {line_number}: expected 6 fields."
            ));
        }

        let filename = fields[0].trim();
        let category = fields[1].trim();
        let featured_text = fields[2].trim();
        let title = fields[3].trim();
        let description = fields[4].trim();
        let techniques = fields[5].trim();
        if !filename.ends_with(".bas")
            || category.is_empty()
            || title.is_empty()
            || description.is_empty()
            || techniques.is_empty()
        {
            return Err(format!(
                "Invalid embedded sample catalog row {line_number}: missing or invalid data."
            ));
        }
        if !filenames.insert(filename) {
            return Err(format!(
                "Invalid embedded sample catalog: duplicate file {filename}."
            ));
        }

        let featured_order = if featured_text.is_empty() {
            None
        } else {
            let order = featured_text.parse::<usize>().map_err(|_| {
                format!("Invalid embedded sample catalog row {line_number}: bad featured order.")
            })?;
            if order == 0 || !featured_orders.insert(order) {
                return Err(format!(
                    "Invalid embedded sample catalog row {line_number}: bad featured order."
                ));
            }
            Some(order)
        };

        samples.push(Sample {
            filename,
            category,
            featured_order,
            title,
            description,
            techniques,
        });
    }

    if samples.len() != EXPECTED_SAMPLE_COUNT {
        return Err(format!(
            "Invalid embedded sample catalog: expected {EXPECTED_SAMPLE_COUNT} programs, found {}.",
            samples.len()
        ));
    }
    if featured_orders.len() != EXPECTED_FEATURED_COUNT
        || !(1..=EXPECTED_FEATURED_COUNT).all(|order| featured_orders.contains(&order))
    {
        return Err(format!(
            "Invalid embedded sample catalog: featured_order must contain 1 through {EXPECTED_FEATURED_COUNT}."
        ));
    }

    Ok(samples)
}

fn category_order<'a>(samples: &'a [Sample<'a>]) -> Vec<&'a str> {
    let mut categories = Vec::new();
    for sample in samples {
        if !categories.contains(&sample.category) {
            categories.push(sample.category);
        }
    }
    categories
}

fn append_wrapped(
    lines: &mut Vec<String>,
    first_prefix: &str,
    continuation_prefix: &str,
    text: &str,
) {
    let mut current = first_prefix.to_string();
    let mut has_content = false;

    for original_word in text.split_whitespace() {
        let mut word = original_word;
        loop {
            let separator_width = usize::from(has_content);
            let available = OUTPUT_WIDTH
                .saturating_sub(current.chars().count() + separator_width)
                .max(1);
            let word_width = word.chars().count();

            if word_width <= available {
                if has_content {
                    current.push(' ');
                }
                current.push_str(word);
                has_content = true;
                break;
            }

            if has_content {
                lines.push(current);
                current = continuation_prefix.to_string();
                has_content = false;
                continue;
            }

            let (head, tail) = split_at_chars(word, available);
            current.push_str(head);
            lines.push(current);
            current = continuation_prefix.to_string();
            word = tail;
            if word.is_empty() {
                has_content = false;
                break;
            }
        }
    }

    if has_content || current != first_prefix {
        lines.push(current);
    }
}

fn split_at_chars(text: &str, count: usize) -> (&str, &str) {
    let split = text
        .char_indices()
        .nth(count)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    text.split_at(split)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn embedded_catalog_is_complete_and_points_to_real_programs() {
        let samples = parse_catalog().expect("valid embedded catalog");
        let samples_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");

        assert_eq!(samples.len(), EXPECTED_SAMPLE_COUNT);
        assert_eq!(
            samples
                .iter()
                .filter(|sample| sample.featured_order.is_some())
                .count(),
            EXPECTED_FEATURED_COUNT
        );
        let catalog_files: HashSet<String> = samples
            .iter()
            .map(|sample| sample.filename.to_string())
            .collect();
        let actual_files: HashSet<String> = fs::read_dir(&samples_dir)
            .expect("read samples directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .filter_map(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("bas"))
                    .then(|| entry.file_name().to_string_lossy().to_string())
            })
            .collect();
        assert_eq!(catalog_files, actual_files);

        for sample in samples {
            assert!(
                samples_dir.join(sample.filename).is_file(),
                "missing {}",
                sample.filename
            );
            if sample.featured_order.is_some() {
                let image = samples_dir
                    .join("showcase")
                    .join(sample.filename.replace(".bas", ".png"));
                assert!(image.is_file(), "missing {}", image.display());
            }
        }
        assert!(samples_dir.join("README.md").is_file());
    }

    #[test]
    fn rendered_showcase_stays_within_console_width() {
        for lines in [
            samples_lines(true).expect("sample catalog"),
            tour_lines(true).expect("sample tour"),
            samples_lines(false).expect("sample catalog without files"),
            tour_lines(false).expect("sample tour without files"),
        ] {
            for line in lines {
                assert!(
                    line.chars().count() <= OUTPUT_WIDTH,
                    "line is too wide ({}): {line}",
                    line.chars().count()
                );
            }
        }
    }

    #[test]
    fn tour_contains_one_run_command_per_featured_sample() {
        let lines = tour_lines(true).expect("sample tour");
        let run_lines: Vec<&String> = lines
            .iter()
            .filter(|line| line.trim_start().starts_with("RUN \"/samples/"))
            .collect();

        assert_eq!(run_lines.len(), EXPECTED_FEATURED_COUNT);
    }
}
