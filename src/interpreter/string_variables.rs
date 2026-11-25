use super::FastHashMap;

#[derive(Debug)]
struct StringVariable {
    text: String,
    ascii: Option<bool>,
}

impl StringVariable {
    fn is_ascii(&mut self) -> bool {
        *self.ascii.get_or_insert_with(|| self.text.is_ascii())
    }
}

#[derive(Debug, Default)]
pub(super) struct StringVariables {
    slots: FastHashMap<String, StringVariable>,
}

impl StringVariables {
    pub(super) fn get(&self, name: &str) -> Option<&String> {
        self.slots.get(name).map(|slot| &slot.text)
    }

    pub(super) fn get_mut(&mut self, name: &str) -> Option<&mut String> {
        let slot = self.slots.get_mut(name)?;
        // Any caller borrowing the text mutably may change its encoding class.
        slot.ascii = None;
        Some(&mut slot.text)
    }

    pub(super) fn insert(&mut self, name: String, text: String) {
        self.slots
            .insert(name, StringVariable { text, ascii: None });
    }

    pub(super) fn remove(&mut self, name: &str) {
        self.slots.remove(name);
    }

    pub(super) fn clear(&mut self) {
        self.slots.clear();
    }

    pub(super) fn contains_key(&self, name: &str) -> bool {
        self.slots.contains_key(name)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.slots.iter().map(|(name, slot)| (name, &slot.text))
    }

    pub(super) fn char_at(&mut self, name: &str, start: usize) -> Option<char> {
        let slot = self.slots.get_mut(name)?;
        if slot.is_ascii() {
            slot.text
                .as_bytes()
                .get(start)
                .map(|byte| char::from(*byte))
        } else {
            slot.text.chars().nth(start)
        }
    }

    pub(super) fn slice(&mut self, name: &str, start: usize, count: Option<usize>) -> String {
        let Some(slot) = self.slots.get_mut(name) else {
            return String::new();
        };
        if start == 0 {
            return crate::expr::left_string(&slot.text, count.unwrap_or(usize::MAX));
        }
        if start >= slot.text.len() {
            return String::new();
        }
        if slot.is_ascii() {
            let end = count.map_or(slot.text.len(), |n| {
                start.saturating_add(n).min(slot.text.len())
            });
            slot.text[start..end].to_owned()
        } else {
            slot.text
                .chars()
                .skip(start)
                .take(count.unwrap_or(usize::MAX))
                .collect()
        }
    }

    pub(super) fn replace_mid(
        &mut self,
        name: &str,
        start: usize,
        count: Option<usize>,
        replacement: &str,
    ) {
        let Some(slot) = self.slots.get_mut(name) else {
            return;
        };
        if start >= slot.text.len() || count == Some(0) || replacement.is_empty() {
            return;
        }
        if slot.is_ascii() && replacement.is_ascii() {
            let n = count
                .unwrap_or(usize::MAX)
                .min(slot.text.len() - start)
                .min(replacement.len());
            slot.text.replace_range(start..start + n, &replacement[..n]);
        } else {
            slot.ascii = None;
            super::replace_mid_general(&mut slot.text, start, count, replacement);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StringVariables;
    use crate::Interpreter;

    #[test]
    fn left_and_right_extract_unicode_scalars_on_both_sides_of_an_ascii_window() {
        let mut basic = Interpreter::new();
        for line in [
            r#"10 S$="ABé😀XY""#,
            r#"20 PRINT LEFT$(S$,2);"|";RIGHT$(S$,2)"#,
            r#"30 PRINT LEFT$(S$+"",4);"|";RIGHT$(""+S$,4)"#,
            r#"40 PRINT LEFT$(S$,7);"|";RIGHT$(S$,7)"#,
            r#"50 PRINT LEFT$(S$,0);"|";RIGHT$(S$,0)"#,
        ] {
            basic.process_immediate(line).unwrap();
        }
        basic.run_loaded().unwrap();
        assert_eq!(
            basic.take_output(),
            "AB|XY\nABé😀|é😀XY\nABé😀XY|ABé😀XY\n|\n"
        );
    }

    #[test]
    fn mid_captures_a_scalar_character_before_an_index_function_changes_its_source() {
        let mut basic = Interpreter::new();
        for line in [
            r#"10 S$="🍏source" : R$="東rest""#,
            r#"20 MID$(S$,FNPos(),1)=S$ : PRINT S$"#,
            r#"30 R$="東rest" : MID$(S$,FNPos(),1)=R$ : PRINT S$"#,
            r#"40 MID$(S$,3,16)="🦀αexpands" : PRINT S$"#,
            r#"50 END"#,
            r#"100 DEF FNPos()"#,
            r#"110 S$="aébc" : R$="" : FNPos=2"#,
            r#"120 FNEND"#,
        ] {
            basic.process_immediate(line).unwrap();
        }
        basic.run_loaded().unwrap();
        assert_eq!(basic.take_output(), "a🍏bc\na東bc\na東🦀α\n");
    }

    #[test]
    fn mid_indices_follow_characters_after_mutating_or_rebinding_a_classified_string() {
        let mut vars = StringVariables::default();
        vars.insert("S$".into(), "abcd".into());
        assert_eq!(vars.char_at("S$", 2), Some('c'));
        vars.replace_mid("S$", 1, Some(1), "😀");
        assert_eq!(vars.char_at("S$", 2), Some('c'));
        assert_eq!(vars.slice("S$", 1, Some(2)), "😀c");
        vars.replace_mid("S$", 1, None, "xy");
        assert_eq!(vars.slice("S$", 1, None), "xyd");
        *vars.get_mut("S$").unwrap() = "é東🙂".into();
        assert_eq!(vars.char_at("S$", 1), Some('東'));
        vars.insert("S$".into(), "Áb".into());
        assert_eq!(vars.slice("S$", 1, Some(1)), "\u{301}");
        assert_eq!(vars.char_at("S$", 3), None);
    }
}
