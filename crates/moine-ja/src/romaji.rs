use std::error::Error;
use std::fmt;

use moine_core::{Lattice, LatticeError, Symbol};

use crate::kana::{is_kana, normalize_kana_char};

/// Kana-to-romaji variant table used by the Japanese adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct RomajiVariantTable;

impl RomajiVariantTable {
    /// Returns the accepted romaji variants for a kana unit.
    pub fn variants(&self, unit: &str) -> Option<&'static [&'static str]> {
        variants_for(unit)
    }
}

/// Errors returned while building Japanese romaji lattices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JaLatticeError {
    /// The input contains a character that is neither ASCII nor supported kana.
    UnsupportedChar {
        /// Unsupported character.
        ch: char,
        /// Character offset in the input.
        index: usize,
    },
    /// No romaji mapping exists for a segmented kana unit.
    MissingVariant {
        /// Kana unit with no mapping.
        unit: String,
    },
    /// No readings were provided when constructing a lattice from readings.
    EmptyReadings,
    /// The underlying lattice shape was invalid.
    Lattice(LatticeError),
    /// A dictionary artifact payload failed while expanding readings.
    ArtifactPayload(String),
}

impl fmt::Display for JaLatticeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedChar { ch, index } => {
                write!(f, "unsupported character {ch:?} at char index {index}")
            }
            Self::MissingVariant { unit } => {
                write!(f, "missing romaji variant for kana unit {unit:?}")
            }
            Self::EmptyReadings => write!(f, "at least one reading is required"),
            Self::Lattice(err) => write!(f, "{err}"),
            Self::ArtifactPayload(err) => write!(f, "{err}"),
        }
    }
}

impl Error for JaLatticeError {}

impl From<LatticeError> for JaLatticeError {
    fn from(value: LatticeError) -> Self {
        Self::Lattice(value)
    }
}

/// Builds a compact romaji lattice from kana or ASCII romaji input.
pub fn romaji_lattice(input: &str) -> Result<Lattice, JaLatticeError> {
    RomajiVariantTable.build_lattice(input)
}

pub(crate) fn can_build_romaji_paths(input: &str) -> bool {
    romaji_paths(input).is_ok()
}

/// Builds a compact romaji lattice from one or more kana readings.
pub fn romaji_lattice_from_readings<I, S>(readings: I) -> Result<Lattice, JaLatticeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut paths = Vec::new();
    for reading in readings {
        paths.extend(romaji_symbol_paths(reading.as_ref())?);
    }
    if paths.is_empty() {
        return Err(JaLatticeError::EmptyReadings);
    }
    Ok(Lattice::from_symbol_paths_compact(paths))
}

pub(crate) fn romaji_paths_from_reading_segments<I, P, S>(
    reading_paths: I,
) -> Result<Vec<String>, JaLatticeError>
where
    I: IntoIterator<Item = P>,
    P: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut paths = Vec::new();
    for reading_path in reading_paths {
        let mut units = Vec::new();
        for segment_reading in reading_path {
            units.extend(segment(segment_reading.as_ref())?);
        }
        paths.extend(romaji_paths_from_units(&units)?);
    }
    if paths.is_empty() {
        return Err(JaLatticeError::EmptyReadings);
    }
    Ok(paths)
}

pub(crate) fn romaji_symbol_paths_from_reading_segments<I, P, S>(
    reading_paths: I,
) -> Result<Vec<Vec<Symbol>>, JaLatticeError>
where
    I: IntoIterator<Item = P>,
    P: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut paths = Vec::new();
    for reading_path in reading_paths {
        let mut units = Vec::new();
        for segment_reading in reading_path {
            units.extend(segment(segment_reading.as_ref())?);
        }
        paths.extend(romaji_symbol_paths_from_units(&units)?);
    }
    if paths.is_empty() {
        return Err(JaLatticeError::EmptyReadings);
    }
    Ok(paths)
}

impl RomajiVariantTable {
    /// Builds a compact romaji lattice using this variant table.
    pub fn build_lattice(&self, input: &str) -> Result<Lattice, JaLatticeError> {
        let paths = romaji_symbol_paths(input)?;
        Ok(Lattice::from_symbol_paths_compact(paths))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Unit {
    Ascii(char),
    Kana(String),
}

fn segment(input: &str) -> Result<Vec<Unit>, JaLatticeError> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut units = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if let Some(normalized) = normalize_whitespace_char(ch) {
            units.push(Unit::Ascii(normalized));
            i += 1;
            continue;
        }

        if ch.is_ascii() {
            units.push(Unit::Ascii(ch));
            i += 1;
            continue;
        }

        let normalized = normalize_kana_char(ch);
        if !is_kana(normalized) {
            return Err(JaLatticeError::UnsupportedChar { ch, index: i });
        }

        if i + 1 < chars.len() {
            let next = normalize_kana_char(chars[i + 1]);
            let pair = [normalized, next].iter().collect::<String>();
            if variants_for(&pair).is_some() {
                units.push(Unit::Kana(pair));
                i += 2;
                continue;
            }
        }

        units.push(Unit::Kana(normalized.to_string()));
        i += 1;
    }

    Ok(units)
}

fn normalize_whitespace_char(ch: char) -> Option<char> {
    ch.is_whitespace().then_some(' ')
}

/// Expands kana or ASCII romaji input into explicit romaji paths.
pub fn romaji_paths(input: &str) -> Result<Vec<String>, JaLatticeError> {
    let units = segment(input)?;
    romaji_paths_from_units(&units)
}

fn romaji_symbol_paths(input: &str) -> Result<Vec<Vec<Symbol>>, JaLatticeError> {
    let units = segment(input)?;
    romaji_symbol_paths_from_units(&units)
}

fn romaji_paths_from_units(units: &[Unit]) -> Result<Vec<String>, JaLatticeError> {
    let mut paths = vec![String::new()];
    let mut i = 0;

    while i < units.len() {
        if matches!(&units[i], Unit::Kana(unit) if unit == "ー") {
            paths = append_contextual_long_vowel(paths);
            i += 1;
            continue;
        }

        let mut consumed_units = 1;
        let variants = if let Some(variants) = units
            .get(i + 1)
            .and_then(|next| ascii_small_kana_variants(&units[i], next))
        {
            consumed_units = 2;
            variants
        } else if matches!(&units[i], Unit::Kana(unit) if unit == "っ") {
            sokuon_variants(units.get(i + 1))?
        } else {
            variants_for_unit(&units[i])?
        };

        let mut next_paths = Vec::with_capacity(paths.len() * variants.len());
        for prefix in &paths {
            for variant in &variants {
                let mut path = String::with_capacity(prefix.len() + variant.len());
                path.push_str(prefix);
                path.push_str(variant);
                next_paths.push(path);
            }
        }
        paths = next_paths;
        i += consumed_units;
    }
    Ok(paths)
}

fn romaji_symbol_paths_from_units(units: &[Unit]) -> Result<Vec<Vec<Symbol>>, JaLatticeError> {
    let mut paths = vec![Vec::new()];
    let mut i = 0;

    while i < units.len() {
        if matches!(&units[i], Unit::Kana(unit) if unit == "ー") {
            paths = append_contextual_long_vowel_symbols(paths);
            i += 1;
            continue;
        }

        let mut consumed_units = 1;
        let variants = if let Some(variants) = units
            .get(i + 1)
            .and_then(|next| ascii_small_kana_variants(&units[i], next))
        {
            consumed_units = 2;
            variants
        } else if matches!(&units[i], Unit::Kana(unit) if unit == "っ") {
            sokuon_variants(units.get(i + 1))?
        } else {
            variants_for_unit(&units[i])?
        };

        let mut next_paths = Vec::with_capacity(paths.len() * variants.len());
        for prefix in &paths {
            for variant in &variants {
                let mut path = Vec::with_capacity(prefix.len() + variant.chars().count());
                path.extend_from_slice(prefix);
                path.extend(variant.chars().map(|ch| ch as Symbol));
                next_paths.push(path);
            }
        }
        paths = next_paths;
        i += consumed_units;
    }
    Ok(paths)
}

fn variants_for_unit(unit: &Unit) -> Result<Vec<String>, JaLatticeError> {
    match unit {
        Unit::Ascii(ch) => Ok(vec![ch.to_string()]),
        Unit::Kana(unit) => variants_for(unit)
            .ok_or_else(|| JaLatticeError::MissingVariant { unit: unit.clone() })
            .map(to_owned_variants),
    }
}

fn sokuon_variants(next: Option<&Unit>) -> Result<Vec<String>, JaLatticeError> {
    let mut variants = variants_for("っ")
        .expect("small tsu must have explicit variants")
        .iter()
        .map(|variant| (*variant).to_string())
        .collect::<Vec<_>>();

    if let Some(next) = next {
        for next_variant in variants_for_unit(next)? {
            if let Some(prefix) = geminate_prefix(&next_variant) {
                push_unique(&mut variants, prefix.to_string());
            }
        }
    }

    Ok(variants)
}

fn ascii_small_kana_variants(current: &Unit, next: &Unit) -> Option<Vec<String>> {
    let Unit::Ascii(ch) = current else {
        return None;
    };
    let Unit::Kana(kana) = next else {
        return None;
    };

    let suffix = match kana.as_str() {
        "ゃ" => "ya",
        "ゅ" => "yu",
        "ょ" => "yo",
        _ => return None,
    };
    if !is_ascii_consonant(*ch) {
        return None;
    }

    let mut variants = vec![format!("{ch}{suffix}")];
    for small_kana_variant in variants_for_unit(next).ok()? {
        variants.push(format!("{ch}{small_kana_variant}"));
    }
    Some(variants)
}

fn is_ascii_consonant(ch: char) -> bool {
    ch.is_ascii_alphabetic() && !matches!(ch.to_ascii_lowercase(), 'a' | 'i' | 'u' | 'e' | 'o')
}

fn geminate_prefix(variant: &str) -> Option<char> {
    let first = variant.chars().next()?;
    if matches!(first, 'a' | 'i' | 'u' | 'e' | 'o' | 'n' | '-') {
        None
    } else {
        Some(first)
    }
}

fn append_contextual_long_vowel(paths: Vec<String>) -> Vec<String> {
    let mut next_paths = Vec::with_capacity(paths.len() * 3);
    for prefix in paths {
        for suffix in long_vowel_suffixes(&prefix) {
            let mut path = String::with_capacity(prefix.len() + suffix.len());
            path.push_str(&prefix);
            path.push_str(suffix);
            next_paths.push(path);
        }
    }
    next_paths
}

fn append_contextual_long_vowel_symbols(paths: Vec<Vec<Symbol>>) -> Vec<Vec<Symbol>> {
    let mut next_paths = Vec::with_capacity(paths.len() * 3);
    for prefix in paths {
        for suffix in long_vowel_suffixes_for_symbols(&prefix) {
            let mut path = Vec::with_capacity(prefix.len() + suffix.len());
            path.extend_from_slice(&prefix);
            path.extend(suffix.chars().map(|ch| ch as Symbol));
            next_paths.push(path);
        }
    }
    next_paths
}

fn long_vowel_suffixes(prefix: &str) -> &'static [&'static str] {
    match last_vowel(prefix) {
        Some('a') => &["a", "-"],
        Some('i') => &["i", "-"],
        Some('u') => &["u", "-"],
        Some('e') => &["e", "i", "-"],
        Some('o') => &["o", "u", "-"],
        _ => &["-"],
    }
}

fn last_vowel(path: &str) -> Option<char> {
    path.chars()
        .rev()
        .find(|ch| matches!(ch, 'a' | 'i' | 'u' | 'e' | 'o'))
}

fn long_vowel_suffixes_for_symbols(prefix: &[Symbol]) -> &'static [&'static str] {
    match last_vowel_symbol(prefix) {
        Some('a') => &["a", "-"],
        Some('i') => &["i", "-"],
        Some('u') => &["u", "-"],
        Some('e') => &["e", "i", "-"],
        Some('o') => &["o", "u", "-"],
        _ => &["-"],
    }
}

fn last_vowel_symbol(path: &[Symbol]) -> Option<char> {
    path.iter()
        .rev()
        .filter_map(|&symbol| char::from_u32(symbol))
        .find(|ch| matches!(ch, 'a' | 'i' | 'u' | 'e' | 'o'))
}

fn to_owned_variants(variants: &'static [&'static str]) -> Vec<String> {
    variants
        .iter()
        .map(|variant| (*variant).to_string())
        .collect()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

const ROMAJI_VARIANTS: &[(&str, &[&str])] = &[
    ("あ", &["a"]),
    ("い", &["i"]),
    ("う", &["u"]),
    ("え", &["e"]),
    ("お", &["o"]),
    ("か", &["ka"]),
    ("き", &["ki"]),
    ("く", &["ku"]),
    ("け", &["ke"]),
    ("こ", &["ko"]),
    ("さ", &["sa"]),
    ("し", &["si", "shi", "ci"]),
    ("す", &["su"]),
    ("せ", &["se"]),
    ("そ", &["so"]),
    ("た", &["ta"]),
    ("ち", &["ti", "chi"]),
    ("つ", &["tu", "tsu"]),
    ("て", &["te"]),
    ("と", &["to"]),
    ("な", &["na"]),
    ("に", &["ni"]),
    ("ぬ", &["nu"]),
    ("ね", &["ne"]),
    ("の", &["no"]),
    ("は", &["ha"]),
    ("ひ", &["hi"]),
    ("ふ", &["hu", "fu"]),
    ("へ", &["he"]),
    ("ほ", &["ho"]),
    ("ま", &["ma"]),
    ("み", &["mi"]),
    ("む", &["mu"]),
    ("め", &["me"]),
    ("も", &["mo"]),
    ("や", &["ya"]),
    ("ゆ", &["yu"]),
    ("よ", &["yo"]),
    ("ら", &["ra"]),
    ("り", &["ri"]),
    ("る", &["ru"]),
    ("れ", &["re"]),
    ("ろ", &["ro"]),
    ("わ", &["wa"]),
    ("を", &["wo", "o"]),
    ("ん", &["n", "nn", "m"]),
    ("が", &["ga"]),
    ("ぎ", &["gi"]),
    ("ぐ", &["gu"]),
    ("げ", &["ge"]),
    ("ご", &["go"]),
    ("ざ", &["za"]),
    ("じ", &["zi", "ji"]),
    ("ず", &["zu"]),
    ("ぜ", &["ze"]),
    ("ぞ", &["zo"]),
    ("だ", &["da"]),
    ("ぢ", &["di", "ji"]),
    ("づ", &["du", "zu"]),
    ("で", &["de"]),
    ("ど", &["do"]),
    ("ば", &["ba"]),
    ("び", &["bi"]),
    ("ぶ", &["bu"]),
    ("べ", &["be"]),
    ("ぼ", &["bo"]),
    ("ぱ", &["pa"]),
    ("ぴ", &["pi"]),
    ("ぷ", &["pu"]),
    ("ぺ", &["pe"]),
    ("ぽ", &["po"]),
    ("ゔ", &["vu"]),
    ("ぁ", &["xa", "la"]),
    ("ぃ", &["xi", "li"]),
    ("ぅ", &["xu", "lu"]),
    ("ぇ", &["xe", "le"]),
    ("ぉ", &["xo", "lo"]),
    ("ゃ", &["xya", "lya"]),
    ("ゅ", &["xyu", "lyu"]),
    ("ょ", &["xyo", "lyo"]),
    ("っ", &["xtu", "ltu", "ttu"]),
    ("ー", &["-"]),
    ("きゃ", &["kya"]),
    ("きゅ", &["kyu"]),
    ("きょ", &["kyo"]),
    ("しゃ", &["sya", "sha", "cya"]),
    ("しゅ", &["syu", "shu", "cyu"]),
    ("しょ", &["syo", "sho", "cyo"]),
    ("ちゃ", &["tya", "cha", "cya"]),
    ("ちゅ", &["tyu", "chu", "cyu"]),
    ("ちょ", &["tyo", "cho", "cyo"]),
    ("にゃ", &["nya"]),
    ("にゅ", &["nyu"]),
    ("にょ", &["nyo"]),
    ("ひゃ", &["hya"]),
    ("ひゅ", &["hyu"]),
    ("ひょ", &["hyo"]),
    ("みゃ", &["mya"]),
    ("みゅ", &["myu"]),
    ("みょ", &["myo"]),
    ("りゃ", &["rya"]),
    ("りゅ", &["ryu"]),
    ("りょ", &["ryo"]),
    ("ぎゃ", &["gya"]),
    ("ぎゅ", &["gyu"]),
    ("ぎょ", &["gyo"]),
    ("じゃ", &["zya", "ja", "jya"]),
    ("じゅ", &["zyu", "ju", "jyu"]),
    ("じょ", &["zyo", "jo", "jyo"]),
    ("びゃ", &["bya"]),
    ("びゅ", &["byu"]),
    ("びょ", &["byo"]),
    ("ぴゃ", &["pya"]),
    ("ぴゅ", &["pyu"]),
    ("ぴょ", &["pyo"]),
];

fn variants_for(unit: &str) -> Option<&'static [&'static str]> {
    ROMAJI_VARIANTS
        .iter()
        .find_map(|&(key, variants)| (key == unit).then_some(variants))
}

#[cfg(test)]
mod tests {
    use moine_core::{distance, distance_with_trace, Lattice, Symbol};

    use super::*;

    fn symbols_to_string(symbols: &[Symbol]) -> String {
        symbols
            .iter()
            .map(|&symbol| char::from_u32(symbol).expect("test symbol should be a char"))
            .collect()
    }

    #[test]
    fn ascii_is_identity_path() {
        let lattice = romaji_lattice("chadougu").expect("ascii should build");
        let trace = distance_with_trace(&lattice, &Lattice::from_paths(["chadougu"]));

        assert_eq!(trace.distance, 0);
        assert_eq!(symbols_to_string(&trace.left_symbols()), "chadougu");
    }

    #[test]
    fn katakana_and_hiragana_share_romaji_lattice() {
        let hira = romaji_lattice("ちゃ").expect("hiragana should build");
        let kata = romaji_lattice("チャ").expect("katakana should build");

        assert_eq!(distance(&hira, &kata), 0);
        assert_eq!(distance(&hira, &Lattice::from_paths(["cha"])), 0);
        assert_eq!(distance(&hira, &Lattice::from_paths(["tya"])), 0);
        assert_eq!(distance(&hira, &Lattice::from_paths(["cya"])), 0);
    }

    #[test]
    fn variants_make_si_shi_ci_equivalent_for_shi() {
        let lattice = romaji_lattice("し").expect("shi should build");

        assert_eq!(distance(&lattice, &Lattice::from_paths(["si"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["shi"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["ci"])), 0);
    }

    #[test]
    fn kana_and_ascii_can_mix() {
        let lattice = romaji_lattice("いんさt").expect("mixed input should build");
        let trace = distance_with_trace(&lattice, &Lattice::from_paths(["insat"]));

        assert_eq!(trace.distance, 0);
        assert_eq!(symbols_to_string(&trace.left_symbols()), "insat");
    }

    #[test]
    fn unicode_whitespace_normalizes_to_ascii_space() {
        for whitespace in [' ', '\u{00a0}', '\u{2003}', '\u{2009}', '\u{3000}'] {
            let left = format!("ピーテッド{whitespace}ウイスキー");
            let right = format!("ぴーてっど{whitespace}ういすきー");
            let left_lattice = romaji_lattice(&left).expect("unicode whitespace should build");
            let right_lattice = romaji_lattice(&right).expect("unicode whitespace should build");

            assert_eq!(distance(&left_lattice, &right_lattice), 0);
        }
    }

    #[test]
    fn ascii_consonant_can_combine_with_small_kana() {
        let lattice = romaji_lattice("kょう").expect("mixed small kana input should build");

        assert_eq!(distance(&lattice, &Lattice::from_paths(["kyou"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["kxyou"])), 0);
    }

    #[test]
    fn sokuon_adds_next_consonant_prefix() {
        let lattice = romaji_lattice("まっちゃ").expect("sokuon input should build");

        assert_eq!(distance(&lattice, &Lattice::from_paths(["maccha"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["mattya"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["maxtucha"])), 0);
    }

    #[test]
    fn long_vowel_mark_adds_contextual_vowels() {
        let lattice = romaji_lattice("チャドーグ").expect("long vowel input should build");

        assert_eq!(distance(&lattice, &Lattice::from_paths(["chadougu"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["chadoogu"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["chado-gu"])), 0);
    }

    #[test]
    fn unsupported_kanji_errors_until_dictionary_support_exists() {
        let result = romaji_lattice("印刷");

        assert!(matches!(
            result,
            Err(JaLatticeError::UnsupportedChar {
                ch: '印', index: 0
            })
        ));
    }

    #[test]
    fn default_table_contains_issue_two_variants() {
        let table = RomajiVariantTable;

        assert_eq!(table.variants("ん"), Some(&["n", "nn", "m"][..]));
        assert_eq!(table.variants("つ"), Some(&["tu", "tsu"][..]));
        assert_eq!(table.variants("ちゃ"), Some(&["tya", "cha", "cya"][..]));
    }
}
