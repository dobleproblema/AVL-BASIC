#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Small,
    Big,
}

pub fn font_dimensions(font: FontKind) -> (i32, i32) {
    match font {
        FontKind::Small => (8, 16),
        FontKind::Big => (16, 16),
    }
}

include!(concat!(env!("OUT_DIR"), "/font_tables.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_source_generates_expected_tables() {
        assert_eq!(font_dimensions(FontKind::Small), (8, 16));
        assert_eq!(font_dimensions(FontKind::Big), (16, 16));
        assert_eq!(glyph_count(FontKind::Small), glyph_count(FontKind::Big));
        assert_eq!(glyph_count(FontKind::Small), 177);
        assert!(glyph_rows(FontKind::Small, '\u{25A1}').is_some());
        assert!(glyph_rows(FontKind::Big, '\u{25A1}').is_some());
    }
}
