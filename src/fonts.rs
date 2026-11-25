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
        // The upper halves of the three ISO pages, in byte order from A0 to FF.
        let latin2 = concat!(
            "\u{a0}Ą˘Ł¤ĽŚ§¨ŠŞŤŹ\u{ad}ŽŻ°ą˛ł´ľśˇ¸šşťź˝žż",
            "ŔÁÂĂÄĹĆÇČÉĘËĚÍÎĎĐŃŇÓÔŐÖ×ŘŮÚŰÜÝŢß",
            "ŕáâăäĺćçčéęëěíîďđńňóôőö÷řůúűüýţ˙",
        );
        let latin9 = concat!(
            "\u{a0}¡¢£€¥Š§š©ª«¬\u{ad}®¯°±²³Žµ¶·ž¹º»ŒœŸ¿",
            "ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞß",
            "àáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿ",
        );
        let latin10 = concat!(
            "\u{a0}ĄąŁ€„Š§š©Ș«Ź\u{ad}źŻ°±ČłŽ”¶·žčș»ŒœŸż",
            "ÀÁÂĂÄĆÆÇÈÉÊËÌÍÎÏĐŃÒÓÔŐÖŚŰÙÚÛÜĘȚß",
            "àáâăäćæçèéêëìíîïđńòóôőöśűùúûüęțÿ",
        );
        for font in [FontKind::Small, FontKind::Big] {
            assert_eq!(glyph_count(font), 320);
            let extended = [
                latin2,
                latin9,
                latin10,
                "ẞ□",
                "ΓΔΘΛΠΣΦΩαβγδεζηθκλμνξπρστφχψω",
                "←↑→↓↔↕⇒⇔",
                "∀∂∃∇∈∉∏∑∓√∝∞∧∨∩∪∫≃≈≠≡≤≥",
            ];
            let latin1 = (' '..='~').chain('\u{a0}'..='ÿ');
            for ch in latin1.chain(extended.into_iter().flat_map(str::chars)) {
                assert!(glyph_rows(font, ch).is_some(), "{font:?} missing {ch:?}");
            }
        }
    }
}
