/// Normalizes katakana in `input` to hiragana.
pub fn normalize_kana(input: &str) -> String {
    input.chars().map(normalize_kana_char).collect()
}

/// Normalizes one katakana character to hiragana when possible.
pub fn normalize_kana_char(ch: char) -> char {
    match ch {
        '\u{30a1}'..='\u{30f6}' => {
            char::from_u32(ch as u32 - 0x60).expect("katakana maps to hiragana")
        }
        _ => ch,
    }
}

/// Returns whether `ch` is hiragana, katakana, or the long-vowel mark.
pub fn is_kana(ch: char) -> bool {
    matches!(normalize_kana_char(ch), '\u{3041}'..='\u{3096}' | 'ー')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katakana_normalizes_to_hiragana() {
        assert_eq!(
            normalize_kana("アイウエオカタカナヴヵヶ"),
            "あいうえおかたかなゔゕゖ"
        );
    }

    #[test]
    fn detects_hiragana_katakana_and_long_vowel_mark() {
        assert!(is_kana('あ'));
        assert!(is_kana('ア'));
        assert!(is_kana('ー'));
        assert!(!is_kana('a'));
        assert!(!is_kana('漢'));
    }
}
