//! Language metadata generated at build time and embedded in the executable.

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TopicKind {
    Statement,
    Metacommand,
    Function,
    PrintFunction,
    Constant,
    Operator,
}

#[derive(Debug)]
pub(crate) struct LanguageTopic {
    #[cfg(test)]
    pub(crate) kind: TopicKind,
    #[cfg(test)]
    pub(crate) topic: &'static str,
    #[cfg(test)]
    pub(crate) aliases: &'static [&'static str],
    pub(crate) syntax: &'static [&'static str],
    pub(crate) parameters: &'static [&'static str],
    pub(crate) summary: &'static str,
    pub(crate) related: &'static [&'static str],
}

include!(concat!(env!("OUT_DIR"), "/language_catalog.rs"));

#[cfg(test)]
pub(crate) fn topics() -> &'static [LanguageTopic] {
    LANGUAGE_TOPICS
}

pub(crate) fn topic(name: &str) -> Option<&'static LanguageTopic> {
    let position = HELP_TOPIC_LOOKUP
        .binary_search_by_key(&name, |(topic_name, _)| topic_name)
        .ok()?;
    let topic_index = HELP_TOPIC_LOOKUP[position].1;
    LANGUAGE_TOPICS.get(topic_index)
}

#[inline]
pub(crate) fn is_keyword(word: &str) -> bool {
    KEYWORD_WORDS.binary_search(&word).is_ok()
}

#[inline]
pub(crate) fn is_other_word(word: &str) -> bool {
    OTHER_WORDS.binary_search(&word).is_ok()
}

#[inline]
pub(crate) fn is_known_word(word: &str) -> bool {
    is_keyword(word) || is_other_word(word)
}

#[inline]
pub(crate) fn is_reserved_base_name(word: &str) -> bool {
    RESERVED_BASE_NAMES.binary_search(&word).is_ok()
}

#[inline(always)]
pub(crate) fn is_immediate_command_word(word: &str) -> bool {
    generated_is_immediate_command_word(word)
}

#[inline(always)]
pub(crate) fn is_program_command_word(word: &str) -> bool {
    generated_is_program_command_word(word)
}

#[inline]
pub(crate) fn is_builtin_function(word: &str) -> bool {
    generated_is_builtin_function(word)
}

#[inline]
pub(crate) fn is_pure_function(word: &str) -> bool {
    generated_is_pure_function(word)
}

#[inline]
pub(crate) fn is_array_name_function(word: &str) -> bool {
    generated_is_array_name_function(word)
}

#[inline]
pub(crate) fn is_zero_arg_function(word: &str) -> bool {
    generated_is_zero_arg_function(word)
}

#[inline]
pub(crate) fn is_pi_constant_name(word: &str) -> bool {
    generated_is_pi_constant_name(word)
}

#[cfg(test)]
pub(crate) fn other_words() -> &'static [&'static str] {
    OTHER_WORDS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn generated_topics_are_unique_and_related_names_resolve() {
        let mut lookup = HashMap::new();
        for (index, topic) in topics().iter().enumerate() {
            for name in std::iter::once(topic.topic).chain(topic.aliases.iter().copied()) {
                assert_eq!(lookup.insert(name, index), None, "duplicate topic {name}");
            }
        }
        for topic in topics() {
            for related in topic.related {
                assert!(
                    lookup.contains_key(related),
                    "{} refers to missing topic {related}",
                    topic.topic
                );
            }
        }
        assert_eq!(lookup.len(), HELP_TOPIC_LOOKUP.len());
        for (name, expected_index) in HELP_TOPIC_LOOKUP {
            assert!(std::ptr::eq(
                topic(name).expect("generated HELP lookup must resolve"),
                &LANGUAGE_TOPICS[*expected_index]
            ));
        }
    }

    #[test]
    fn generated_word_classes_are_disjoint_and_reserved() {
        let keywords: HashSet<&str> = KEYWORD_WORDS.iter().copied().collect();
        let others: HashSet<&str> = OTHER_WORDS.iter().copied().collect();
        assert!(keywords.is_disjoint(&others));
        for word in keywords.union(&others) {
            assert!(is_reserved_base_name(word.trim_end_matches('$')), "{word}");
        }
    }

    #[test]
    fn generated_parser_function_classes_are_known_and_consistent() {
        let builtins: HashSet<&str> = BUILTIN_FUNCTION_WORDS.iter().copied().collect();
        for class in [
            PURE_FUNCTION_WORDS,
            ARRAY_NAME_FUNCTION_WORDS,
            ZERO_ARG_FUNCTION_WORDS,
            PI_CONSTANT_WORDS,
        ] {
            for word in class {
                assert!(builtins.contains(word), "{word} is not a built-in function");
            }
        }
        for word in BUILTIN_FUNCTION_WORDS {
            assert!(is_other_word(word), "{word} is not highlighted or reserved");
        }
        assert_eq!(PI_CONSTANT_WORDS, ["PI"]);
    }

    #[test]
    fn catalog_context_indexes_match_both_manuals() {
        for manual in [
            include_str!("../MANUAL.txt"),
            include_str!("../MANUAL.es.txt"),
        ] {
            assert!(
                manual.contains(&format!("+ {IMMEDIATE_MANUAL_INDEX}")),
                "immediate-command index is not generated from the catalog"
            );
            assert!(
                manual.contains(&format!("+ {PROGRAM_MANUAL_INDEX}")),
                "program-only index is not generated from the catalog"
            );
        }
    }

    #[test]
    fn help_is_the_only_contextual_metacommand() {
        let metacommands = topics()
            .iter()
            .filter(|topic| topic.kind == TopicKind::Metacommand)
            .map(|topic| topic.topic)
            .collect::<Vec<_>>();
        assert_eq!(metacommands, ["HELP"]);
        assert!(!is_known_word("HELP"));
        assert!(!is_known_word("TOUR"));
        assert!(!is_known_word("SAMPLES"));
        assert!(!is_immediate_command_word("HELP"));
        assert!(!is_immediate_command_word("EXIT"));
        assert!(is_immediate_command_word("QUIT"));
        assert!(is_immediate_command_word("SYSTEM"));
    }
}
