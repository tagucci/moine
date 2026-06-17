use moine_core::{Lattice, Symbol};

use crate::{CnLatticeError, PinyinReadingPath, PinyinView};

/// Builds a pinyin lattice from expanded reading paths.
///
/// Each path contributes one complete pinyin string to the compact lattice.
/// Segment boundaries are used before this step and are not represented in the
/// returned lattice.
pub fn pinyin_lattice_from_reading_paths(
    paths: &[PinyinReadingPath],
) -> Result<Lattice, CnLatticeError> {
    if paths.is_empty() {
        return Err(CnLatticeError::EmptyReadings);
    }

    Ok(Lattice::from_symbol_paths_compact(paths.iter().map(
        |path| {
            path.joined_reading
                .chars()
                .map(|ch| ch as Symbol)
                .collect::<Vec<_>>()
        },
    )))
}

/// Normalizes a whitespace-separated CC-CEDICT pinyin field.
///
/// In [`PinyinView::NoTone`], tone digits that follow Latin letters are
/// removed while numeric tokens such as `11` are preserved. In
/// [`PinyinView::Tone3`], tone digits are retained.
pub fn normalize_pinyin(raw: &str, view: PinyinView) -> String {
    let mut normalized = String::new();
    for token in raw.split_whitespace() {
        normalized.push_str(&normalize_pinyin_token(token, view));
    }
    match view {
        PinyinView::NoTone => strip_no_tone_digits(&normalized),
        PinyinView::Tone3 => normalized,
    }
}

pub(crate) fn direct_pinyin_lattice(input: &str) -> Option<Lattice> {
    if input.is_empty() || !can_build_direct_pinyin_input(input) {
        return None;
    }
    Some(Lattice::from_paths([normalize_direct_pinyin_input(input)]))
}

fn normalize_pinyin_token(token: &str, view: PinyinView) -> String {
    let lowered = token.to_lowercase().replace("u:", "v").replace('ü', "v");
    let contains_letters = lowered.chars().any(|ch| ch.is_ascii_alphabetic());
    if view == PinyinView::NoTone && contains_letters {
        lowered
            .chars()
            .filter(|ch| !matches!(ch, '1'..='5'))
            .collect()
    } else {
        lowered
    }
}

pub(crate) fn normalize_direct_pinyin_input(input: &str) -> String {
    input
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>()
        .replace("u:", "v")
}

pub(crate) fn normalize_artifact_reading(reading: &str, view: PinyinView) -> String {
    let lowered = reading
        .to_lowercase()
        .replace("u:", "v")
        .replace('ü', "v")
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    match view {
        PinyinView::NoTone => strip_no_tone_digits(&lowered),
        PinyinView::Tone3 => lowered,
    }
}

fn strip_no_tone_digits(reading: &str) -> String {
    let mut previous = None;
    let mut normalized = String::with_capacity(reading.len());
    for ch in reading.chars() {
        if matches!(ch, '1'..='5') && previous.is_some_and(|prev: char| prev.is_ascii_alphabetic())
        {
            continue;
        }
        normalized.push(ch);
        previous = Some(ch);
    }
    normalized
}

pub(crate) fn can_build_direct_pinyin_input(surface: &str) -> bool {
    !surface.is_empty()
        && surface
            .chars()
            .all(|ch| ch.is_ascii() || ch.is_whitespace() || is_neutral_literal(ch))
}

fn is_neutral_literal(ch: char) -> bool {
    is_fullwidth_ascii_punctuation(ch)
        || matches!(
            ch,
            '\u{00b7}'
                | '\u{2010}'..='\u{2015}'
                | '\u{2018}'..='\u{201f}'
                | '\u{2026}'
                | '\u{3001}'..='\u{3002}'
                | '\u{3008}'..='\u{3011}'
                | '\u{3014}'..='\u{301f}'
                | '\u{3030}'
                | '\u{30fb}'
        )
}

fn is_fullwidth_ascii_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '\u{ff01}'..='\u{ff0f}'
            | '\u{ff1a}'..='\u{ff20}'
            | '\u{ff3b}'..='\u{ff40}'
            | '\u{ff5b}'..='\u{ff65}'
    )
}
