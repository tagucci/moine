use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use moine_core::{Arc, DistanceError, Lattice, LatticeError, Symbol};

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

/// Limits used when explicitly expanding romaji candidates into strings.
///
/// Lattice-building APIs keep variants in a DAG and do not use these limits.
/// These limits only guard APIs that must materialize every accepted romaji
/// candidate as a `String`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RomajiExpansionLimits {
    /// Maximum input characters accepted by explicit expansion APIs.
    pub max_input_chars: usize,
    /// Maximum complete romaji paths returned by explicit expansion APIs.
    pub max_expanded_paths: usize,
    /// Maximum total Unicode scalar values across all expanded romaji paths.
    pub max_total_symbols: usize,
}

impl Default for RomajiExpansionLimits {
    fn default() -> Self {
        Self {
            max_input_chars: 1024,
            max_expanded_paths: 65_536,
            max_total_symbols: 1_048_576,
        }
    }
}

/// Explicit romaji expansion limit that was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RomajiExpansionLimit {
    /// The input character count exceeded `max_input_chars`.
    InputChars,
    /// The complete candidate path count exceeded `max_expanded_paths`.
    ExpandedPaths,
    /// The total output symbol count exceeded `max_total_symbols`.
    TotalSymbols,
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
    /// Distance computation exceeded the configured matrix-size limit.
    Distance(DistanceError),
    /// A dictionary artifact payload failed while expanding readings.
    ArtifactPayload(String),
    /// Explicit romaji path expansion exceeded a configured limit.
    ExpansionLimitExceeded {
        /// Limit that was exceeded.
        limit: RomajiExpansionLimit,
        /// Observed value that crossed the limit.
        value: usize,
        /// Configured maximum value.
        max: usize,
    },
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
            Self::Distance(err) => write!(f, "{err}"),
            Self::ArtifactPayload(err) => write!(f, "{err}"),
            Self::ExpansionLimitExceeded { limit, value, max } => {
                let limit_name = match limit {
                    RomajiExpansionLimit::InputChars => "input characters",
                    RomajiExpansionLimit::ExpandedPaths => "expanded romaji paths",
                    RomajiExpansionLimit::TotalSymbols => "expanded romaji symbols",
                };
                write!(
                    f,
                    "romaji expansion exceeded {limit_name} limit: {value} > {max}"
                )
            }
        }
    }
}

impl Error for JaLatticeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Lattice(err) => Some(err),
            Self::Distance(err) => Some(err),
            Self::UnsupportedChar { .. }
            | Self::MissingVariant { .. }
            | Self::EmptyReadings
            | Self::ArtifactPayload(_)
            | Self::ExpansionLimitExceeded { .. } => None,
        }
    }
}

impl From<LatticeError> for JaLatticeError {
    fn from(value: LatticeError) -> Self {
        Self::Lattice(value)
    }
}

impl From<DistanceError> for JaLatticeError {
    fn from(value: DistanceError) -> Self {
        Self::Distance(value)
    }
}

/// Builds a compact romaji lattice from kana or ASCII romaji input.
///
/// The lattice is built directly from romaji variant fragments instead of
/// materializing every accepted romaji string first.
pub fn romaji_lattice(input: &str) -> Result<Lattice, JaLatticeError> {
    RomajiVariantTable.build_lattice(input)
}

pub(crate) fn can_build_direct_romaji_path(input: &str) -> bool {
    segment(input, RomajiSegmentMode::Surface)
        .and_then(|units| validate_romaji_units(&units))
        .is_ok()
}

pub(crate) fn can_build_romaji_reading(input: &str) -> bool {
    segment(input, RomajiSegmentMode::Reading)
        .and_then(|units| validate_romaji_units(&units))
        .is_ok()
}

/// Builds a compact romaji lattice from one or more kana readings.
///
/// The lattice is built directly from romaji variant fragments instead of
/// materializing every accepted romaji string first.
pub fn romaji_lattice_from_readings<I, S>(readings: I) -> Result<Lattice, JaLatticeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut builder = RomajiLatticeBuilder::new();
    for reading in readings {
        let units = segment(reading.as_ref(), RomajiSegmentMode::Reading)?;
        builder.add_units(&units)?;
    }
    builder.into_lattice()
}

pub(crate) fn romaji_lattice_from_supported_readings<I, S>(
    readings: I,
) -> Result<Option<Lattice>, JaLatticeError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut builder = RomajiLatticeBuilder::new();
    for reading in readings {
        match segment(reading.as_ref(), RomajiSegmentMode::Reading)
            .and_then(|units| builder.add_units(&units))
        {
            Ok(()) => {}
            Err(err) if is_unsupported_reading_error(&err) => continue,
            Err(err) => return Err(err),
        }
    }
    builder.into_optional_lattice()
}

fn is_unsupported_reading_error(err: &JaLatticeError) -> bool {
    matches!(
        err,
        JaLatticeError::UnsupportedChar { .. } | JaLatticeError::MissingVariant { .. }
    )
}

pub(crate) fn romaji_paths_from_segmented_readings<I, P, S>(
    reading_paths: I,
) -> Result<Vec<String>, JaLatticeError>
where
    I: IntoIterator<Item = P>,
    P: IntoIterator<Item = (S, RomajiSegmentMode)>,
    S: AsRef<str>,
{
    let mut paths = Vec::new();
    let limits = RomajiExpansionLimits::default();
    for reading_path in reading_paths {
        let mut units = Vec::new();
        let mut input_chars = 0usize;
        for (segment_reading, mode) in reading_path {
            let segment_reading = segment_reading.as_ref();
            input_chars = checked_add_limit(
                input_chars,
                segment_reading.chars().count(),
                RomajiExpansionLimit::InputChars,
                limits.max_input_chars,
            )?;
            check_expansion_limit(
                RomajiExpansionLimit::InputChars,
                input_chars,
                limits.max_input_chars,
            )?;
            units.extend(segment(segment_reading, mode)?);
        }
        let expanded = romaji_paths_from_units(&units, limits)?;
        extend_expanded_paths(&mut paths, expanded, limits)?;
    }
    if paths.is_empty() {
        return Err(JaLatticeError::EmptyReadings);
    }
    Ok(paths)
}

pub(crate) fn romaji_lattice_from_segmented_readings<I, P, S>(
    reading_paths: I,
) -> Result<Lattice, JaLatticeError>
where
    I: IntoIterator<Item = P>,
    P: IntoIterator<Item = (S, RomajiSegmentMode)>,
    S: AsRef<str>,
{
    let mut builder = RomajiLatticeBuilder::new();
    for reading_path in reading_paths {
        let mut units = Vec::new();
        for (segment_reading, mode) in reading_path {
            units.extend(segment(segment_reading.as_ref(), mode)?);
        }
        builder.add_units(&units)?;
    }
    builder.into_lattice()
}

pub(crate) fn romaji_lattice_from_supported_segmented_readings<I, P, S>(
    reading_paths: I,
) -> Result<Option<Lattice>, JaLatticeError>
where
    I: IntoIterator<Item = P>,
    P: IntoIterator<Item = (S, RomajiSegmentMode)>,
    S: AsRef<str>,
{
    let mut builder = RomajiLatticeBuilder::new();
    for reading_path in reading_paths {
        let mut units = Vec::new();
        let mut supported = true;
        for (segment_reading, mode) in reading_path {
            match segment(segment_reading.as_ref(), mode) {
                Ok(segment_units) => units.extend(segment_units),
                Err(err) if is_unsupported_reading_error(&err) => {
                    supported = false;
                    break;
                }
                Err(err) => return Err(err),
            }
        }
        if !supported {
            continue;
        }
        match builder.add_units(&units) {
            Ok(()) => {}
            Err(err) if is_unsupported_reading_error(&err) => continue,
            Err(err) => return Err(err),
        }
    }
    builder.into_optional_lattice()
}

impl RomajiVariantTable {
    /// Builds a compact romaji lattice using this variant table.
    pub fn build_lattice(&self, input: &str) -> Result<Lattice, JaLatticeError> {
        let units = segment(input, RomajiSegmentMode::Surface)?;
        let mut builder = RomajiLatticeBuilder::new();
        builder.add_units(&units)?;
        builder.into_lattice()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Unit {
    Ascii(char),
    NeutralLiteral(char),
    Kana(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RomajiSegmentMode {
    Surface,
    Reading,
}

const END_NODE: usize = usize::MAX;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LastVowel {
    None,
    A,
    I,
    U,
    E,
    O,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FrontierState {
    node: usize,
    last_vowel: LastVowel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RomajiStep {
    Variants(Vec<String>),
    LongVowel,
}

#[derive(Clone, Debug)]
struct RomajiTransition {
    src: usize,
    symbols: Vec<Symbol>,
    last_vowel: LastVowel,
}

#[derive(Debug)]
struct RomajiLatticeBuilder {
    next_node: usize,
    arcs: Vec<Arc>,
    seen_arcs: BTreeSet<(usize, usize, Symbol)>,
    has_empty_path: bool,
    has_non_empty_path: bool,
}

impl RomajiLatticeBuilder {
    fn new() -> Self {
        Self {
            next_node: 1,
            arcs: Vec::new(),
            seen_arcs: BTreeSet::new(),
            has_empty_path: false,
            has_non_empty_path: false,
        }
    }

    fn add_units(&mut self, units: &[Unit]) -> Result<(), JaLatticeError> {
        let steps = romaji_steps_from_units(units)?;
        if steps.is_empty() {
            self.has_empty_path = true;
            return Ok(());
        }

        self.has_non_empty_path = true;
        let mut frontier = vec![FrontierState {
            node: 0,
            last_vowel: LastVowel::None,
        }];
        for (idx, step) in steps.iter().enumerate() {
            if idx + 1 == steps.len() {
                self.add_final_step(&frontier, step);
            } else {
                frontier = self.add_intermediate_step(&frontier, step);
            }
        }

        Ok(())
    }

    fn into_optional_lattice(self) -> Result<Option<Lattice>, JaLatticeError> {
        if !self.has_empty_path && !self.has_non_empty_path {
            return Ok(None);
        }
        self.into_lattice().map(Some)
    }

    fn into_lattice(self) -> Result<Lattice, JaLatticeError> {
        if !self.has_empty_path && !self.has_non_empty_path {
            return Err(JaLatticeError::EmptyReadings);
        }
        if self.has_empty_path && !self.has_non_empty_path {
            return Lattice::from_edges(1, 0, 0, Vec::new()).map_err(JaLatticeError::from);
        }
        if self.has_empty_path && self.has_non_empty_path {
            return Err(JaLatticeError::Lattice(
                LatticeError::MixedEmptyAndNonEmptyPaths,
            ));
        }

        let end = self.next_node;
        let arcs = self
            .arcs
            .into_iter()
            .map(|arc| {
                let src = arc.src;
                let symbol = arc.symbol;
                let dst = arc.dst;
                let dst = if dst == END_NODE { end } else { dst };
                Arc::new(src, dst, symbol)
            })
            .collect::<Vec<_>>();
        Lattice::from_edges(end + 1, 0, end, arcs).map_err(JaLatticeError::from)
    }

    fn add_intermediate_step(
        &mut self,
        frontier: &[FrontierState],
        step: &RomajiStep,
    ) -> Vec<FrontierState> {
        let transitions = self.transitions(frontier, step);
        let intermediate_count = transitions
            .iter()
            .map(|transition| transition.symbols.len().saturating_sub(1))
            .sum::<usize>();
        let target_vowels = transitions
            .iter()
            .map(|transition| transition.last_vowel)
            .collect::<BTreeSet<_>>();
        let target_base = self.next_node + intermediate_count;
        let target_nodes = target_vowels
            .iter()
            .enumerate()
            .map(|(idx, &last_vowel)| (last_vowel, target_base + idx))
            .collect::<BTreeMap<_, _>>();

        let mut intermediate = self.next_node;
        for transition in transitions {
            let dst = target_nodes[&transition.last_vowel];
            self.add_symbol_path(transition.src, dst, &transition.symbols, &mut intermediate);
        }

        self.next_node = target_base + target_vowels.len();
        target_vowels
            .iter()
            .map(|last_vowel| FrontierState {
                node: target_nodes[last_vowel],
                last_vowel: *last_vowel,
            })
            .collect()
    }

    fn add_final_step(&mut self, frontier: &[FrontierState], step: &RomajiStep) {
        let transitions = self.transitions(frontier, step);
        let mut intermediate = self.next_node;
        for transition in transitions {
            self.add_symbol_path(
                transition.src,
                END_NODE,
                &transition.symbols,
                &mut intermediate,
            );
        }
        self.next_node = intermediate;
    }

    fn transitions(&self, frontier: &[FrontierState], step: &RomajiStep) -> Vec<RomajiTransition> {
        let mut transitions = Vec::new();
        for state in frontier {
            match step {
                RomajiStep::Variants(variants) => {
                    for variant in variants {
                        transitions.push(RomajiTransition::new(
                            state.node,
                            state.last_vowel,
                            variant,
                        ));
                    }
                }
                RomajiStep::LongVowel => {
                    for suffix in long_vowel_suffixes_for_last_vowel(state.last_vowel) {
                        transitions.push(RomajiTransition::new(
                            state.node,
                            state.last_vowel,
                            suffix,
                        ));
                    }
                }
            }
        }
        transitions
    }

    fn add_symbol_path(
        &mut self,
        src: usize,
        dst: usize,
        symbols: &[Symbol],
        next_intermediate: &mut usize,
    ) {
        debug_assert!(!symbols.is_empty());

        let mut current = src;
        for (idx, &symbol) in symbols.iter().enumerate() {
            let arc_dst = if idx + 1 == symbols.len() {
                dst
            } else {
                let node = *next_intermediate;
                *next_intermediate += 1;
                node
            };
            if self.seen_arcs.insert((current, arc_dst, symbol)) {
                self.arcs.push(Arc::new(current, arc_dst, symbol));
            }
            current = arc_dst;
        }
    }
}

impl RomajiTransition {
    fn new(src: usize, last_vowel: LastVowel, variant: &str) -> Self {
        let symbols = variant.chars().map(|ch| ch as Symbol).collect::<Vec<_>>();
        let last_vowel = symbols.iter().fold(last_vowel, |current, &symbol| {
            last_vowel_after(current, symbol)
        });
        Self {
            src,
            symbols,
            last_vowel,
        }
    }
}

fn segment(input: &str, mode: RomajiSegmentMode) -> Result<Vec<Unit>, JaLatticeError> {
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

        if mode == RomajiSegmentMode::Surface && is_neutral_literal(ch) {
            units.push(Unit::NeutralLiteral(ch));
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

/// Expands kana or ASCII romaji input into explicit romaji paths.
///
/// This materializes every accepted romaji candidate and therefore uses
/// [`RomajiExpansionLimits::default`] to reject inputs that would expand too
/// broadly. Use [`romaji_paths_with_limits`] to choose a different budget.
pub fn romaji_paths(input: &str) -> Result<Vec<String>, JaLatticeError> {
    romaji_paths_with_limits(input, RomajiExpansionLimits::default())
}

/// Expands kana or ASCII romaji input into explicit romaji paths with limits.
pub fn romaji_paths_with_limits(
    input: &str,
    limits: RomajiExpansionLimits,
) -> Result<Vec<String>, JaLatticeError> {
    check_expansion_limit(
        RomajiExpansionLimit::InputChars,
        input.chars().count(),
        limits.max_input_chars,
    )?;
    let units = segment(input, RomajiSegmentMode::Surface)?;
    romaji_paths_from_units(&units, limits)
}

fn validate_romaji_units(units: &[Unit]) -> Result<(), JaLatticeError> {
    let mut i = 0;

    while i < units.len() {
        if matches!(&units[i], Unit::Kana(unit) if unit == "ー") {
            i += 1;
            continue;
        }

        if units
            .get(i + 1)
            .is_some_and(|next| can_combine_ascii_small_kana(&units[i], next))
        {
            i += 2;
            continue;
        }

        if matches!(&units[i], Unit::Kana(unit) if unit == "っ") {
            if let Some(next) = units.get(i + 1) {
                validate_variants_for_unit(next)?;
            }
        }

        validate_variants_for_unit(&units[i])?;
        i += 1;
    }

    Ok(())
}

fn validate_variants_for_unit(unit: &Unit) -> Result<(), JaLatticeError> {
    match unit {
        Unit::Ascii(_) | Unit::NeutralLiteral(_) => Ok(()),
        Unit::Kana(unit) => variants_for(unit)
            .map(|_| ())
            .ok_or_else(|| JaLatticeError::MissingVariant { unit: unit.clone() }),
    }
}

fn romaji_steps_from_units(units: &[Unit]) -> Result<Vec<RomajiStep>, JaLatticeError> {
    let mut steps = Vec::new();
    let mut i = 0;

    while i < units.len() {
        if matches!(&units[i], Unit::Kana(unit) if unit == "ー") {
            steps.push(RomajiStep::LongVowel);
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

        steps.push(RomajiStep::Variants(unique_variants(variants)));
        i += consumed_units;
    }

    Ok(steps)
}

fn romaji_paths_from_units(
    units: &[Unit],
    limits: RomajiExpansionLimits,
) -> Result<Vec<String>, JaLatticeError> {
    let mut paths = vec![String::new()];

    for step in romaji_steps_from_units(units)? {
        paths = match step {
            RomajiStep::LongVowel => append_contextual_long_vowel(paths, limits)?,
            RomajiStep::Variants(variants) => append_path_variants(paths, &variants, limits)?,
        };
    }

    Ok(paths)
}

fn append_path_variants(
    paths: Vec<String>,
    variants: &[String],
    limits: RomajiExpansionLimits,
) -> Result<Vec<String>, JaLatticeError> {
    let next_path_count = checked_mul_limit(
        paths.len(),
        variants.len(),
        RomajiExpansionLimit::ExpandedPaths,
        limits.max_expanded_paths,
    )?;
    check_expansion_limit(
        RomajiExpansionLimit::ExpandedPaths,
        next_path_count,
        limits.max_expanded_paths,
    )?;

    let mut next_paths = Vec::with_capacity(next_path_count);
    let mut total_symbols = 0usize;
    for prefix in &paths {
        let prefix_symbols = prefix.chars().count();
        for variant in variants {
            let path_symbols = prefix_symbols.saturating_add(variant.chars().count());
            total_symbols = checked_add_limit(
                total_symbols,
                path_symbols,
                RomajiExpansionLimit::TotalSymbols,
                limits.max_total_symbols,
            )?;
            check_expansion_limit(
                RomajiExpansionLimit::TotalSymbols,
                total_symbols,
                limits.max_total_symbols,
            )?;

            let mut path = String::with_capacity(prefix.len() + variant.len());
            path.push_str(prefix);
            path.push_str(variant);
            next_paths.push(path);
        }
    }
    Ok(next_paths)
}

fn variants_for_unit(unit: &Unit) -> Result<Vec<String>, JaLatticeError> {
    match unit {
        Unit::Ascii(ch) | Unit::NeutralLiteral(ch) => Ok(vec![ch.to_string()]),
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
    if !can_combine_ascii_small_kana(current, next) {
        return None;
    }

    let Unit::Ascii(ch) = current else {
        unreachable!("can_combine_ascii_small_kana requires an ASCII current unit");
    };
    let Unit::Kana(kana) = next else {
        unreachable!("can_combine_ascii_small_kana requires a kana next unit");
    };

    let suffix = small_kana_ascii_suffix(kana)
        .expect("can_combine_ascii_small_kana requires a supported small kana");

    let mut variants = vec![format!("{ch}{suffix}")];
    for small_kana_variant in variants_for_unit(next).ok()? {
        variants.push(format!("{ch}{small_kana_variant}"));
    }
    Some(variants)
}

fn can_combine_ascii_small_kana(current: &Unit, next: &Unit) -> bool {
    let Unit::Ascii(ch) = current else {
        return false;
    };
    let Unit::Kana(kana) = next else {
        return false;
    };

    is_ascii_consonant(*ch) && small_kana_ascii_suffix(kana).is_some()
}

fn small_kana_ascii_suffix(kana: &str) -> Option<&'static str> {
    Some(match kana {
        "ゃ" => "ya",
        "ゅ" => "yu",
        "ょ" => "yo",
        _ => return None,
    })
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

fn append_contextual_long_vowel(
    paths: Vec<String>,
    limits: RomajiExpansionLimits,
) -> Result<Vec<String>, JaLatticeError> {
    let mut next_paths = Vec::new();
    let mut total_symbols = 0usize;
    for prefix in paths {
        for suffix in long_vowel_suffixes(&prefix) {
            check_expansion_limit(
                RomajiExpansionLimit::ExpandedPaths,
                next_paths.len().saturating_add(1),
                limits.max_expanded_paths,
            )?;
            let path_symbols = prefix
                .chars()
                .count()
                .saturating_add(suffix.chars().count());
            total_symbols = checked_add_limit(
                total_symbols,
                path_symbols,
                RomajiExpansionLimit::TotalSymbols,
                limits.max_total_symbols,
            )?;
            check_expansion_limit(
                RomajiExpansionLimit::TotalSymbols,
                total_symbols,
                limits.max_total_symbols,
            )?;

            let mut path = String::with_capacity(prefix.len() + suffix.len());
            path.push_str(&prefix);
            path.push_str(suffix);
            next_paths.push(path);
        }
    }
    Ok(next_paths)
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

fn long_vowel_suffixes_for_last_vowel(last_vowel: LastVowel) -> &'static [&'static str] {
    match last_vowel {
        LastVowel::A => &["a", "-"],
        LastVowel::I => &["i", "-"],
        LastVowel::U => &["u", "-"],
        LastVowel::E => &["e", "i", "-"],
        LastVowel::O => &["o", "u", "-"],
        LastVowel::None => &["-"],
    }
}

fn last_vowel(path: &str) -> Option<char> {
    path.chars()
        .rev()
        .find(|ch| matches!(ch, 'a' | 'i' | 'u' | 'e' | 'o'))
}

fn to_owned_variants(variants: &'static [&'static str]) -> Vec<String> {
    variants
        .iter()
        .map(|variant| (*variant).to_string())
        .collect()
}

fn unique_variants(variants: Vec<String>) -> Vec<String> {
    let mut unique = Vec::with_capacity(variants.len());
    for variant in variants {
        push_unique(&mut unique, variant);
    }
    unique
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn last_vowel_after(current: LastVowel, symbol: Symbol) -> LastVowel {
    match char::from_u32(symbol) {
        Some('a') => LastVowel::A,
        Some('i') => LastVowel::I,
        Some('u') => LastVowel::U,
        Some('e') => LastVowel::E,
        Some('o') => LastVowel::O,
        _ => current,
    }
}

fn extend_expanded_paths(
    paths: &mut Vec<String>,
    expanded: Vec<String>,
    limits: RomajiExpansionLimits,
) -> Result<(), JaLatticeError> {
    let path_count = checked_add_limit(
        paths.len(),
        expanded.len(),
        RomajiExpansionLimit::ExpandedPaths,
        limits.max_expanded_paths,
    )?;
    check_expansion_limit(
        RomajiExpansionLimit::ExpandedPaths,
        path_count,
        limits.max_expanded_paths,
    )?;

    let current_symbols = total_path_symbols(paths);
    let expanded_symbols = total_path_symbols(&expanded);
    let total_symbols = checked_add_limit(
        current_symbols,
        expanded_symbols,
        RomajiExpansionLimit::TotalSymbols,
        limits.max_total_symbols,
    )?;
    check_expansion_limit(
        RomajiExpansionLimit::TotalSymbols,
        total_symbols,
        limits.max_total_symbols,
    )?;

    paths.extend(expanded);
    Ok(())
}

fn total_path_symbols(paths: &[String]) -> usize {
    paths.iter().map(|path| path.chars().count()).sum()
}

fn check_expansion_limit(
    limit: RomajiExpansionLimit,
    value: usize,
    max: usize,
) -> Result<(), JaLatticeError> {
    if value > max {
        Err(JaLatticeError::ExpansionLimitExceeded { limit, value, max })
    } else {
        Ok(())
    }
}

fn checked_add_limit(
    left: usize,
    right: usize,
    limit: RomajiExpansionLimit,
    max: usize,
) -> Result<usize, JaLatticeError> {
    left.checked_add(right)
        .ok_or(JaLatticeError::ExpansionLimitExceeded {
            limit,
            value: usize::MAX,
            max,
        })
}

fn checked_mul_limit(
    left: usize,
    right: usize,
    limit: RomajiExpansionLimit,
    max: usize,
) -> Result<usize, JaLatticeError> {
    left.checked_mul(right)
        .ok_or(JaLatticeError::ExpansionLimitExceeded {
            limit,
            value: usize::MAX,
            max,
        })
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
    fn surface_neutral_literals_are_kept_as_literal_symbols() {
        let left = romaji_lattice("はい，「です」。").expect("punctuated kana should build");
        let right = Lattice::from_paths(["hai，「desu」。"]);

        assert_eq!(distance(&left, &right), 0);
    }

    #[test]
    fn middle_dot_readings_stay_out_of_strict_romaji_reading_validation() {
        // This is a converter policy, not a claim about dictionary validity:
        // Sudachi can contain readings with separators, but dictionary-reading
        // separators need a separate normalize/preserve/drop decision.
        assert!(can_build_direct_romaji_path("ジョニー・ウォーカー"));
        assert!(!can_build_romaji_reading("ジョニー・ウォーカー"));
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
    fn repeated_variant_lattice_does_not_enumerate_all_paths() {
        let input = "キー".repeat(32);
        let lattice = romaji_lattice(&input).expect("repeated variant input should build");

        assert!(lattice.node_count() < 512);
        assert!(lattice.arcs().len() < 1024);
        assert!(matches!(
            romaji_paths(&input),
            Err(JaLatticeError::ExpansionLimitExceeded { .. })
        ));
    }

    #[test]
    fn explicit_romaji_path_expansion_reports_limits() {
        let input_limit = RomajiExpansionLimits {
            max_input_chars: 1,
            ..RomajiExpansionLimits::default()
        };
        assert!(matches!(
            romaji_paths_with_limits("ちゃ", input_limit),
            Err(JaLatticeError::ExpansionLimitExceeded {
                limit: RomajiExpansionLimit::InputChars,
                ..
            })
        ));

        let path_limit = RomajiExpansionLimits {
            max_expanded_paths: 2,
            ..RomajiExpansionLimits::default()
        };
        assert!(matches!(
            romaji_paths_with_limits("ちゃ", path_limit),
            Err(JaLatticeError::ExpansionLimitExceeded {
                limit: RomajiExpansionLimit::ExpandedPaths,
                ..
            })
        ));

        assert_eq!(
            romaji_paths_with_limits("キー", path_limit).unwrap(),
            vec!["kii", "ki-"]
        );
    }

    #[test]
    #[ignore]
    fn romaji_lattice_expansion_shape_smoke_benchmark() {
        use std::time::Instant;

        for input in ["キー".repeat(32), "ちゃ".repeat(24)] {
            let start = Instant::now();
            let lattice = romaji_lattice(&input).expect("benchmark input should build");
            let elapsed = start.elapsed();
            eprintln!(
                "input_chars={} nodes={} arcs={} build={elapsed:?}",
                input.chars().count(),
                lattice.node_count(),
                lattice.arcs().len(),
            );

            assert!(lattice.node_count() < 1024);
            assert!(lattice.arcs().len() < 2048);
        }
    }

    #[test]
    fn support_check_does_not_expand_all_romaji_paths() {
        let input = "キー".repeat(32);

        assert!(can_build_direct_romaji_path(&input));
        assert!(can_build_romaji_reading(&input));
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
