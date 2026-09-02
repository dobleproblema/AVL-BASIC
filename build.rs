use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct FontSource {
    width: usize,
    height: usize,
    glyphs: BTreeMap<u32, Vec<u32>>,
}

#[derive(Debug)]
struct TopicSource {
    kind: String,
    context: String,
    topic: String,
    aliases: Vec<String>,
    syntax: Vec<String>,
    parameters: Vec<String>,
    summary: String,
    related: Vec<String>,
    flags: Vec<String>,
}

#[derive(Debug)]
struct ErrorSource {
    variant: String,
    python_variant: String,
    number: i32,
    message_en: String,
    message_es: String,
}

#[derive(Debug, Default)]
struct LanguageSource {
    keywords: Vec<String>,
    topics: Vec<TopicSource>,
    errors: Vec<ErrorSource>,
}

fn main() {
    println!("cargo:rerun-if-changed=windows.rc");
    println!("cargo:rerun-if-changed=assets/avl-basic.ico");
    println!("cargo:rerun-if-changed=assets/linux/hicolor");
    println!("cargo:rerun-if-changed=assets/fonts/avl-basic-fonts.txt");
    println!("cargo:rerun-if-changed=src/language/catalog.tsv");

    generate_language_tables();
    generate_font_tables();

    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile("windows.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}

fn generate_language_tables() {
    let source_path = Path::new("src/language/catalog.tsv");
    let source = fs::read_to_string(source_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
    let catalog = parse_language_catalog(&source);
    validate_language_catalog(&catalog);

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
    fs::write(
        out_dir.join("language_catalog.rs"),
        render_language_catalog(&catalog),
    )
    .expect("failed to write language_catalog.rs");
    fs::write(
        out_dir.join("error_codes.rs"),
        render_error_codes(&catalog.errors),
    )
    .expect("failed to write error_codes.rs");
}

fn parse_language_catalog(source: &str) -> LanguageSource {
    let mut catalog = LanguageSource::default();
    let mut section = "";

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if matches!(trimmed, "[keywords]" | "[topics]" | "[errors]") {
            section = trimmed;
            continue;
        }

        match section {
            "[keywords]" => {
                if trimmed.contains('\t') {
                    panic!("language catalog line {line_number}: keyword rows have one field");
                }
                catalog.keywords.push(trimmed.to_string());
            }
            "[topics]" => {
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() != 9 {
                    panic!(
                        "language catalog line {line_number}: expected 9 topic fields, found {}",
                        fields.len()
                    );
                }
                if [0usize, 1, 2, 4, 5, 6, 7, 8]
                    .into_iter()
                    .any(|field| fields[field].trim().is_empty())
                {
                    panic!(
                        "language catalog line {line_number}: only the aliases field may be empty"
                    );
                }
                catalog.topics.push(TopicSource {
                    kind: fields[0].trim().to_string(),
                    context: fields[1].trim().to_string(),
                    topic: normalized_catalog_name(fields[2]),
                    aliases: parse_catalog_list(fields[3], ','),
                    syntax: parse_catalog_syntax(fields[4]),
                    parameters: parse_catalog_text_list(fields[5], ';'),
                    summary: fields[6].trim().to_string(),
                    related: parse_catalog_list(fields[7], ','),
                    flags: parse_catalog_flags(fields[8]),
                });
            }
            "[errors]" => {
                let fields: Vec<&str> = line.split('\t').collect();
                if fields.len() != 5 || fields.iter().any(|field| field.trim().is_empty()) {
                    panic!(
                        "language catalog line {line_number}: expected 5 non-empty error fields"
                    );
                }
                catalog.errors.push(ErrorSource {
                    variant: fields[0].trim().to_string(),
                    number: fields[1].trim().parse::<i32>().unwrap_or_else(|_| {
                        panic!("language catalog line {line_number}: invalid error number")
                    }),
                    message_en: fields[2].trim().to_string(),
                    message_es: fields[3].trim().to_string(),
                    python_variant: fields[4].trim().to_string(),
                });
            }
            _ => panic!("language catalog line {line_number}: row outside a section"),
        }
    }

    catalog
}

fn parse_catalog_list(field: &str, separator: char) -> Vec<String> {
    let field = field.trim();
    if field == "-" {
        return Vec::new();
    }
    field
        .split(separator)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(normalized_catalog_name)
        .collect()
}

fn parse_catalog_text_list(field: &str, separator: char) -> Vec<String> {
    let field = field.trim();
    if field == "-" {
        return Vec::new();
    }
    field
        .split(separator)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_catalog_flags(field: &str) -> Vec<String> {
    let field = field.trim();
    if field == "-" {
        return Vec::new();
    }
    field
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_catalog_syntax(field: &str) -> Vec<String> {
    field
        .trim()
        .split(" || ")
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn normalized_catalog_name(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn validate_language_catalog(catalog: &LanguageSource) {
    if catalog.keywords.is_empty() || catalog.topics.is_empty() || catalog.errors.is_empty() {
        panic!("language catalog sections must not be empty");
    }

    let mut keywords = BTreeSet::new();
    for keyword in &catalog.keywords {
        if keyword != &normalized_catalog_name(keyword)
            || keyword.chars().any(char::is_whitespace)
            || !keywords.insert(keyword.clone())
        {
            panic!("invalid or duplicate language keyword `{keyword}`");
        }
    }

    let mut names = BTreeMap::<String, String>::new();
    let valid_flags = BTreeSet::from([
        "array-name",
        "builtin",
        "fast-constant",
        "mat-only",
        "pi",
        "print-only",
        "pure",
        "zero-arg",
    ]);
    let mut pi_topics = Vec::new();
    for topic in &catalog.topics {
        let valid_kind_and_context = matches!(
            (topic.kind.as_str(), topic.context.as_str()),
            ("statement", "both" | "program" | "immediate")
                | ("metacommand", "immediate")
                | ("function" | "constant" | "operator", "expression")
                | ("print-function", "print")
        );
        if !valid_kind_and_context {
            panic!(
                "invalid kind/context pair for {}: {}/{}",
                topic.topic, topic.kind, topic.context
            );
        }
        if topic.syntax.is_empty()
            || !topic.summary.ends_with('.')
            || topic.summary.lines().count() != 1
        {
            panic!(
                "invalid syntax or summary for language topic {}",
                topic.topic
            );
        }
        let mut topic_flags = BTreeSet::new();
        for flag in &topic.flags {
            if !valid_flags.contains(flag.as_str()) || !topic_flags.insert(flag.as_str()) {
                panic!("invalid or duplicate flag `{flag}` in {}", topic.topic);
            }
        }
        if matches!(
            topic.kind.as_str(),
            "statement" | "metacommand" | "operator"
        ) && !topic.flags.is_empty()
        {
            panic!("{} cannot have parser function flags", topic.topic);
        }
        let function_class_count = ["builtin", "fast-constant", "mat-only"]
            .into_iter()
            .filter(|flag| topic_flags.contains(flag))
            .count();
        if matches!(
            topic.kind.as_str(),
            "function" | "constant" | "print-function"
        ) && function_class_count != 1
        {
            panic!(
                "{} must have exactly one of builtin, fast-constant or mat-only",
                topic.topic
            );
        }
        if topic_flags.contains("fast-constant") && topic.kind != "constant" {
            panic!("fast-constant requires constant kind in {}", topic.topic);
        }
        if topic_flags.contains("mat-only") && topic.kind != "function" {
            panic!("mat-only requires function kind in {}", topic.topic);
        }
        if (topic.kind == "print-function") != topic_flags.contains("print-only") {
            panic!(
                "{} must use print-only exactly when it is a print-function",
                topic.topic
            );
        }
        for dependent in ["array-name", "pi", "pure", "zero-arg"] {
            if topic_flags.contains(dependent) && !topic_flags.contains("builtin") {
                panic!("flag `{dependent}` requires `builtin` in {}", topic.topic);
            }
        }
        if topic_flags.contains("pi") {
            pi_topics.push(topic.topic.as_str());
        }
        for name in std::iter::once(&topic.topic).chain(topic.aliases.iter()) {
            if name.is_empty() {
                panic!("empty topic name or alias in {}", topic.topic);
            }
            if let Some(previous) = names.insert(name.clone(), topic.topic.clone()) {
                panic!(
                    "duplicate language topic name `{name}` in {previous} and {}",
                    topic.topic
                );
            }
        }
    }
    if pi_topics != ["PI"] {
        panic!("the catalog must classify only PI with the `pi` flag");
    }
    for topic in &catalog.topics {
        if topic.kind != "statement" {
            continue;
        }
        for name in std::iter::once(&topic.topic).chain(topic.aliases.iter()) {
            if name.chars().all(|ch| !ch.is_ascii_alphanumeric()) {
                continue;
            }
            let Some(first) = name.split_whitespace().next() else {
                panic!("empty statement name in {}", topic.topic);
            };
            if !keywords.contains(first)
                && !catalog.topics.iter().any(|candidate| {
                    matches!(
                        candidate.kind.as_str(),
                        "function" | "constant" | "operator" | "print-function"
                    ) && std::iter::once(&candidate.topic)
                        .chain(candidate.aliases.iter())
                        .any(|candidate_name| candidate_name == first)
                })
            {
                panic!(
                    "statement name `{name}` is missing its first word `{first}` from the language surface"
                );
            }
        }
    }
    for topic in &catalog.topics {
        for related in &topic.related {
            if !names.contains_key(related) {
                panic!(
                    "unresolved related language topic `{related}` in {}",
                    topic.topic
                );
            }
        }
    }

    let mut variants = BTreeSet::new();
    let mut python_variants = BTreeSet::new();
    let mut numbers = BTreeSet::new();
    for error in &catalog.errors {
        if !is_rust_identifier(&error.variant) || !variants.insert(error.variant.clone()) {
            panic!("invalid or duplicate error variant `{}`", error.variant);
        }
        if !is_python_error_variant(&error.python_variant)
            || !python_variants.insert(error.python_variant.clone())
        {
            panic!(
                "invalid or duplicate Python error variant `{}`",
                error.python_variant
            );
        }
        if !numbers.insert(error.number) {
            panic!("duplicate BASIC error number {}", error.number);
        }
        if !error.message_en.ends_with('.') || !error.message_es.ends_with('.') {
            panic!("error {} messages must end in a period", error.variant);
        }
    }
    for (index, error) in catalog.errors.iter().enumerate() {
        let expected = i32::try_from(index + 1).expect("too many errors");
        if error.number != expected {
            panic!(
                "BASIC error numbers must be contiguous: expected {expected}, found {}",
                error.number
            );
        }
    }
}

fn is_rust_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_python_error_variant(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
}

fn render_language_catalog(catalog: &LanguageSource) -> String {
    let keyword_words: BTreeSet<String> = catalog.keywords.iter().cloned().collect();
    let mut other_words = BTreeSet::new();
    let mut reserved_base_names = BTreeSet::new();
    let mut immediate_commands = BTreeSet::new();
    let mut program_commands = BTreeSet::new();
    let mut immediate_topics = BTreeSet::new();
    let mut program_topics = BTreeSet::new();
    let mut builtin_function_words = BTreeSet::new();
    let mut pure_function_words = BTreeSet::new();
    let mut array_name_function_words = BTreeSet::new();
    let mut zero_arg_function_words = BTreeSet::new();
    let mut pi_constant_words = BTreeSet::new();
    let mut help_topic_lookup = BTreeMap::new();

    for keyword in &keyword_words {
        reserved_base_names.insert(keyword.trim_end_matches('$').to_string());
    }

    for (topic_index, topic) in catalog.topics.iter().enumerate() {
        let names = std::iter::once(&topic.topic).chain(topic.aliases.iter());
        for name in names.clone() {
            help_topic_lookup.insert(name.clone(), topic_index);
        }
        if matches!(
            topic.kind.as_str(),
            "function" | "constant" | "operator" | "print-function"
        ) {
            for name in names.clone() {
                if !name.chars().any(char::is_whitespace) {
                    other_words.insert(name.clone());
                    reserved_base_names.insert(name.trim_end_matches('$').to_string());
                }
            }
        }

        for name in names.clone() {
            if topic.flags.iter().any(|flag| flag == "builtin") {
                builtin_function_words.insert(name.clone());
            }
            if topic.flags.iter().any(|flag| flag == "pure") {
                pure_function_words.insert(name.clone());
            }
            if topic.flags.iter().any(|flag| flag == "array-name") {
                array_name_function_words.insert(name.clone());
            }
            if topic.flags.iter().any(|flag| flag == "zero-arg") {
                zero_arg_function_words.insert(name.clone());
            }
            if topic.flags.iter().any(|flag| flag == "pi") {
                pi_constant_words.insert(name.clone());
            }
        }

        if topic.context == "immediate" {
            for name in names.clone() {
                immediate_topics.insert(name.clone());
                let needs_contextual_check =
                    name == &topic.topic && (topic.kind == "metacommand" || topic.topic == "EXIT");
                if !needs_contextual_check {
                    if let Some(first) = name.split_whitespace().next() {
                        immediate_commands.insert(first.to_string());
                    }
                }
            }
        } else if topic.context == "program" {
            for name in names {
                program_topics.insert(name.clone());
                if let Some(first) = name.split_whitespace().next() {
                    program_commands.insert(first.to_string());
                }
            }
        }
    }

    let mut out = String::new();
    out.push_str("// @generated by build.rs from src/language/catalog.tsv.\n");
    out.push_str("// Do not edit by hand; edit the catalog instead.\n\n");
    out.push_str("pub(crate) static LANGUAGE_TOPICS: &[LanguageTopic] = &[\n");
    for topic in &catalog.topics {
        out.push_str("    LanguageTopic {\n");
        out.push_str(&format!(
            "        #[cfg(test)]\n        kind: TopicKind::{},\n",
            topic_kind_variant(&topic.kind)
        ));
        out.push_str(&format!(
            "        #[cfg(test)]\n        topic: {},\n",
            rust_string(&topic.topic)
        ));
        out.push_str(&format!(
            "        #[cfg(test)]\n        aliases: {},\n",
            rust_string_slice(&topic.aliases)
        ));
        out.push_str(&format!(
            "        syntax: {},\n",
            rust_string_slice(&topic.syntax)
        ));
        out.push_str(&format!(
            "        parameters: {},\n",
            rust_string_slice(&topic.parameters)
        ));
        out.push_str(&format!(
            "        summary: {},\n",
            rust_string(&topic.summary)
        ));
        out.push_str(&format!(
            "        related: {},\n",
            rust_string_slice(&topic.related)
        ));
        out.push_str("    },\n");
    }
    out.push_str("];\n\n");
    render_string_set(&mut out, "KEYWORD_WORDS", &keyword_words);
    render_string_set(&mut out, "OTHER_WORDS", &other_words);
    render_string_set(&mut out, "RESERVED_BASE_NAMES", &reserved_base_names);
    render_string_matcher(
        &mut out,
        "generated_is_immediate_command_word",
        &immediate_commands,
        true,
    );
    render_string_matcher(
        &mut out,
        "generated_is_program_command_word",
        &program_commands,
        true,
    );
    render_test_string_set(&mut out, "BUILTIN_FUNCTION_WORDS", &builtin_function_words);
    render_test_string_set(&mut out, "PURE_FUNCTION_WORDS", &pure_function_words);
    render_test_string_set(
        &mut out,
        "ARRAY_NAME_FUNCTION_WORDS",
        &array_name_function_words,
    );
    render_test_string_set(
        &mut out,
        "ZERO_ARG_FUNCTION_WORDS",
        &zero_arg_function_words,
    );
    render_test_string_set(&mut out, "PI_CONSTANT_WORDS", &pi_constant_words);
    render_string_matcher(
        &mut out,
        "generated_is_builtin_function",
        &builtin_function_words,
        false,
    );
    render_string_matcher(
        &mut out,
        "generated_is_pure_function",
        &pure_function_words,
        false,
    );
    render_string_matcher(
        &mut out,
        "generated_is_array_name_function",
        &array_name_function_words,
        false,
    );
    render_string_matcher(
        &mut out,
        "generated_is_zero_arg_function",
        &zero_arg_function_words,
        false,
    );
    render_ascii_case_insensitive_matcher(
        &mut out,
        "generated_is_pi_constant_name",
        &pi_constant_words,
    );
    out.push_str("pub(crate) static HELP_TOPIC_LOOKUP: &[(&str, usize)] = &[\n");
    for (name, topic_index) in help_topic_lookup {
        out.push_str(&format!("    ({}, {}),\n", rust_string(&name), topic_index));
    }
    out.push_str("];\n\n");
    out.push_str(&format!(
        "#[cfg(test)]\npub(crate) const IMMEDIATE_MANUAL_INDEX: &str = {};\n",
        rust_string(&immediate_topics.into_iter().collect::<Vec<_>>().join(" | "))
    ));
    out.push_str(&format!(
        "#[cfg(test)]\npub(crate) const PROGRAM_MANUAL_INDEX: &str = {};\n",
        rust_string(&program_topics.into_iter().collect::<Vec<_>>().join(" | "))
    ));
    out
}

fn render_error_codes(errors: &[ErrorSource]) -> String {
    let mut out = String::new();
    out.push_str("// @generated by build.rs from src/language/catalog.tsv.\n");
    out.push_str("// Do not edit by hand; edit the catalog instead.\n\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum ErrorCode {\n");
    for error in errors {
        out.push_str(&format!("    {},\n", error.variant));
    }
    out.push_str("}\n\nimpl ErrorCode {\n");
    out.push_str("    pub const ALL: &'static [Self] = &[\n");
    for error in errors {
        out.push_str(&format!("        Self::{},\n", error.variant));
    }
    out.push_str("    ];\n\n");
    out.push_str("    pub fn number(self) -> i32 {\n        match self {\n");
    for error in errors {
        out.push_str(&format!(
            "            Self::{} => {},\n",
            error.variant, error.number
        ));
    }
    out.push_str("        }\n    }\n\n");
    out.push_str("    pub fn from_number(number: i32) -> Option<Self> {\n        match number {\n");
    for error in errors {
        out.push_str(&format!(
            "            {} => Some(Self::{}),\n",
            error.number, error.variant
        ));
    }
    out.push_str("            _ => None,\n        }\n    }\n\n");
    out.push_str("    pub fn message(self) -> &'static str {\n        match self {\n");
    for error in errors {
        out.push_str(&format!(
            "            Self::{} => {},\n",
            error.variant,
            rust_string(&error.message_en)
        ));
    }
    out.push_str("        }\n    }\n\n");
    out.push_str("    #[cfg(test)]\n    pub(crate) fn message_es(self) -> &'static str {\n        match self {\n");
    for error in errors {
        out.push_str(&format!(
            "            Self::{} => {},\n",
            error.variant,
            rust_string(&error.message_es)
        ));
    }
    out.push_str("        }\n    }\n}\n");
    out
}

fn topic_kind_variant(kind: &str) -> &'static str {
    match kind {
        "statement" => "Statement",
        "metacommand" => "Metacommand",
        "function" => "Function",
        "print-function" => "PrintFunction",
        "constant" => "Constant",
        "operator" => "Operator",
        _ => unreachable!("validated topic kind"),
    }
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}

fn rust_string_slice(values: &[String]) -> String {
    format!(
        "&[{}]",
        values
            .iter()
            .map(|value| rust_string(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn render_string_set(out: &mut String, name: &str, values: &BTreeSet<String>) {
    out.push_str(&format!("pub(crate) static {name}: &[&str] = &[\n"));
    for value in values {
        out.push_str(&format!("    {},\n", rust_string(value)));
    }
    out.push_str("];\n\n");
}

fn render_test_string_set(out: &mut String, name: &str, values: &BTreeSet<String>) {
    out.push_str("#[cfg(test)]\n");
    render_string_set(out, name, values);
}

fn render_string_matcher(
    out: &mut String,
    name: &str,
    values: &BTreeSet<String>,
    always_inline: bool,
) {
    assert!(
        !values.is_empty(),
        "generated string matcher must not be empty"
    );
    let mut by_length = BTreeMap::<usize, Vec<&String>>::new();
    for value in values {
        assert!(value.is_ascii(), "generated match words must be ASCII");
        by_length.entry(value.len()).or_default().push(value);
    }
    let inline_attribute = if always_inline {
        "#[inline(always)]"
    } else {
        "#[inline]"
    };
    out.push_str(&format!(
        "{inline_attribute}\npub(crate) fn {name}(word: &str) -> bool {{\n    match word.len() {{\n"
    ));
    for (length, group) in by_length {
        let patterns = group
            .into_iter()
            .map(|value| rust_string(value))
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!(
            "        {length} => matches!(word, {patterns}),\n"
        ));
    }
    out.push_str("        _ => false,\n    }\n}\n\n");
}

fn render_ascii_case_insensitive_matcher(out: &mut String, name: &str, values: &BTreeSet<String>) {
    assert!(
        !values.is_empty(),
        "generated string matcher must not be empty"
    );
    let comparisons = values
        .iter()
        .map(|value| format!("word.eq_ignore_ascii_case({})", rust_string(value)))
        .collect::<Vec<_>>()
        .join(" || ");
    out.push_str(&format!(
        "#[inline]\npub(crate) fn {name}(word: &str) -> bool {{\n    {comparisons}\n}}\n\n"
    ));
}

fn generate_font_tables() {
    let source_path = Path::new("assets/fonts/avl-basic-fonts.txt");
    let source = fs::read_to_string(source_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {}", source_path.display(), err));
    let fonts = parse_fonts(&source);
    validate_fonts(&fonts);

    let generated = render_font_tables(&fonts);
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
    fs::write(out_dir.join("font_tables.rs"), generated).expect("failed to write font_tables.rs");
}

fn parse_fonts(source: &str) -> BTreeMap<String, FontSource> {
    let mut fonts: BTreeMap<String, FontSource> = BTreeMap::new();
    let mut current_font: Option<String> = None;
    let mut pending: Option<PendingGlyph> = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            if let Some(glyph) = pending.as_ref() {
                panic!(
                    "incomplete glyph U+{:04X} before line {}: expected {} rows, found {}",
                    glyph.codepoint,
                    line_number,
                    font(&fonts, &glyph.font).height,
                    glyph.rows.len()
                );
            }
            continue;
        }

        if trimmed.starts_with("font ") {
            if let Some(glyph) = pending.take() {
                panic!(
                    "incomplete glyph U+{:04X} before line {}: expected {} rows, found {}",
                    glyph.codepoint,
                    line_number,
                    font(&fonts, &glyph.font).height,
                    glyph.rows.len()
                );
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 4 {
                panic!(
                    "line {}: expected `font <name> <width> <height>`",
                    line_number
                );
            }
            let name = parts[1].to_string();
            let width = parse_usize(parts[2], line_number, "width");
            let height = parse_usize(parts[3], line_number, "height");
            if fonts
                .insert(
                    name.clone(),
                    FontSource {
                        width,
                        height,
                        glyphs: BTreeMap::new(),
                    },
                )
                .is_some()
            {
                panic!("line {}: duplicate font `{}`", line_number, name);
            }
            current_font = Some(name);
            continue;
        }

        if trimmed.starts_with("glyph ") {
            if let Some(glyph) = pending.take() {
                panic!(
                    "incomplete glyph U+{:04X} before line {}: expected {} rows, found {}",
                    glyph.codepoint,
                    line_number,
                    font(&fonts, &glyph.font).height,
                    glyph.rows.len()
                );
            }
            let font_name = current_font
                .clone()
                .unwrap_or_else(|| panic!("line {}: glyph before font section", line_number));
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() != 2 {
                panic!("line {}: expected `glyph U+XXXX`", line_number);
            }
            let codepoint = parse_codepoint(parts[1], line_number);
            pending = Some(PendingGlyph {
                font: font_name,
                codepoint,
                rows: Vec::new(),
            });
            continue;
        }

        let glyph = pending
            .as_mut()
            .unwrap_or_else(|| panic!("line {}: row outside glyph", line_number));
        let font_info = font(&fonts, &glyph.font);
        if trimmed.len() != font_info.width {
            panic!(
                "line {}: glyph U+{:04X} row width is {}, expected {}",
                line_number,
                glyph.codepoint,
                trimmed.len(),
                font_info.width
            );
        }
        let row_value = row_to_bits(trimmed, line_number);
        glyph.rows.push(row_value);
        if glyph.rows.len() == font_info.height {
            let glyph = pending.take().unwrap();
            let target = fonts.get_mut(&glyph.font).unwrap();
            if target.glyphs.insert(glyph.codepoint, glyph.rows).is_some() {
                panic!(
                    "line {}: duplicate glyph U+{:04X} in font `{}`",
                    line_number, glyph.codepoint, glyph.font
                );
            }
        }
    }

    if let Some(glyph) = pending {
        panic!(
            "incomplete glyph U+{:04X} at end of file: expected {} rows, found {}",
            glyph.codepoint,
            font(&fonts, &glyph.font).height,
            glyph.rows.len()
        );
    }

    fonts
}

#[derive(Debug)]
struct PendingGlyph {
    font: String,
    codepoint: u32,
    rows: Vec<u32>,
}

fn font<'a>(fonts: &'a BTreeMap<String, FontSource>, name: &str) -> &'a FontSource {
    fonts
        .get(name)
        .unwrap_or_else(|| panic!("unknown font `{}`", name))
}

fn parse_usize(text: &str, line_number: usize, field: &str) -> usize {
    text.parse()
        .unwrap_or_else(|_| panic!("line {}: invalid {} `{}`", line_number, field, text))
}

fn parse_codepoint(text: &str, line_number: usize) -> u32 {
    let Some(hex) = text.strip_prefix("U+") else {
        panic!("line {}: glyph codepoint must use U+XXXX", line_number);
    };
    let codepoint = u32::from_str_radix(hex, 16)
        .unwrap_or_else(|_| panic!("line {}: invalid codepoint `{}`", line_number, text));
    if char::from_u32(codepoint).is_none() {
        panic!("line {}: invalid Unicode scalar `{}`", line_number, text);
    }
    codepoint
}

fn row_to_bits(row: &str, line_number: usize) -> u32 {
    let mut value = 0u32;
    for ch in row.chars() {
        value <<= 1;
        match ch {
            '.' => {}
            '1' => value |= 1,
            _ => panic!(
                "line {}: rows may contain only `.` and `1`, found `{}`",
                line_number, ch
            ),
        }
    }
    value
}

fn validate_fonts(fonts: &BTreeMap<String, FontSource>) {
    let small = fonts.get("small").expect("missing `small` font");
    let big = fonts.get("big").expect("missing `big` font");
    assert_dimensions("small", small, 8, 16);
    assert_dimensions("big", big, 16, 16);

    let small_glyphs: BTreeSet<u32> = small.glyphs.keys().copied().collect();
    let big_glyphs: BTreeSet<u32> = big.glyphs.keys().copied().collect();
    if small_glyphs != big_glyphs {
        panic!("small and big fonts must define the same glyph set");
    }
    if !small_glyphs.contains(&0x25A1) {
        panic!("replacement glyph U+25A1 is required");
    }
}

fn assert_dimensions(name: &str, font: &FontSource, width: usize, height: usize) {
    if font.width != width || font.height != height {
        panic!(
            "`{}` font must be {}x{}, found {}x{}",
            name, width, height, font.width, font.height
        );
    }
}

fn render_font_tables(fonts: &BTreeMap<String, FontSource>) -> String {
    let mut out = String::new();
    out.push_str("// @generated by build.rs from assets/fonts/avl-basic-fonts.txt.\n");
    out.push_str("// Do not edit by hand; edit the source font file instead.\n\n");
    render_font_constants(&mut out, "SMALL", font(fonts, "small"));
    render_font_constants(&mut out, "BIG", font(fonts, "big"));
    render_glyph_chars(&mut out, fonts);
    render_glyph_count(&mut out, fonts);
    render_glyph_rows(&mut out, fonts);
    out
}

fn render_font_constants(out: &mut String, prefix: &str, font: &FontSource) {
    for (codepoint, rows) in &font.glyphs {
        out.push_str(&format!(
            "const {}_{:04X}: [u32; {}] = [{}];\n",
            prefix,
            codepoint,
            font.height,
            rows.iter()
                .map(u32::to_string)
                .collect::<Vec<String>>()
                .join(", ")
        ));
    }
    out.push('\n');
}

fn render_glyph_chars(out: &mut String, fonts: &BTreeMap<String, FontSource>) {
    render_font_chars_constant(out, "SMALL", font(fonts, "small"));
    render_font_chars_constant(out, "BIG", font(fonts, "big"));
    out.push_str("pub fn glyph_chars(font: FontKind) -> &'static [char] {\n");
    out.push_str("    match font {\n");
    out.push_str("        FontKind::Small => &SMALL_CHARS,\n");
    out.push_str("        FontKind::Big => &BIG_CHARS,\n");
    out.push_str("    }\n");
    out.push_str("}\n\n");
}

fn render_font_chars_constant(out: &mut String, prefix: &str, font: &FontSource) {
    out.push_str(&format!(
        "const {}_CHARS: [char; {}] = [",
        prefix,
        font.glyphs.len()
    ));
    for (idx, codepoint) in font.glyphs.keys().enumerate() {
        if idx > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("'\\u{{{:X}}}'", codepoint));
    }
    out.push_str("];\n");
}

fn render_glyph_rows(out: &mut String, fonts: &BTreeMap<String, FontSource>) {
    out.push_str("pub fn glyph_rows(font: FontKind, ch: char) -> Option<&'static [u32]> {\n");
    out.push_str("    match font {\n");
    render_font_match(out, "Small", "SMALL", font(fonts, "small"));
    render_font_match(out, "Big", "BIG", font(fonts, "big"));
    out.push_str("    }\n");
    out.push_str("}\n");
}

fn render_glyph_count(out: &mut String, fonts: &BTreeMap<String, FontSource>) {
    out.push_str("pub fn glyph_count(font: FontKind) -> usize {\n");
    out.push_str("    match font {\n");
    out.push_str(&format!(
        "        FontKind::Small => {},\n",
        font(fonts, "small").glyphs.len()
    ));
    out.push_str(&format!(
        "        FontKind::Big => {},\n",
        font(fonts, "big").glyphs.len()
    ));
    out.push_str("    }\n");
    out.push_str("}\n\n");
}

fn render_font_match(out: &mut String, variant: &str, prefix: &str, font: &FontSource) {
    out.push_str(&format!("        FontKind::{} => match ch {{\n", variant));
    for codepoint in font.glyphs.keys() {
        out.push_str(&format!(
            "            '\\u{{{:X}}}' => Some(&{}_{:04X}),\n",
            codepoint, prefix, codepoint
        ));
    }
    out.push_str("            _ => None,\n");
    out.push_str("        },\n");
}
