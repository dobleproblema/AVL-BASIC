use crate::language;

pub(crate) fn is_reserved_identifier_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let base_name = upper.strip_suffix('$').unwrap_or(&upper);
    if is_legacy_short_variable_name(base_name) {
        return false;
    }
    language::is_reserved_base_name(base_name)
}

fn is_legacy_short_variable_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    match bytes {
        [first] => first.is_ascii_uppercase(),
        [first, second] if first.is_ascii_uppercase() && second.is_ascii_digit() => true,
        [first, second] if first.is_ascii_uppercase() && second.is_ascii_uppercase() => {
            !matches!(name, "TO" | "IF" | "ON" | "PI" | "OR")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_reserved_names_keep_legacy_short_variable_exceptions() {
        for name in ["A", "A1", "AB", "X$"] {
            assert!(!is_reserved_identifier_name(name), "{name}");
        }
        for name in ["IF", "ON", "OR", "PI", "TO", "PRINT", "RIGHT$"] {
            assert!(is_reserved_identifier_name(name), "{name}");
        }
    }
}
