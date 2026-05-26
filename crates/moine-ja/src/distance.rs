use moine_core::{
    damerau_distance, damerau_levenshtein_str, distance, levenshtein_str,
    normalized_similarity_str, Lattice,
};

use crate::overrides::OverrideDictionary;
use crate::romaji::{romaji_lattice, romaji_paths, JaLatticeError};
use crate::unidic::{
    romaji_paths_from_reading_paths, DictionaryReadingOptions, UnidicReadingIndex,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Distances computed for one Japanese comparison.
pub struct JapaneseDistance {
    /// Plain Levenshtein distance over the original strings.
    pub surface_levenshtein: usize,
    /// Plain Damerau-Levenshtein distance over the original strings.
    pub surface_damerau: usize,
    /// Lattice Path Edit Distance over romaji reading lattices.
    pub lattice: usize,
    /// Lattice-aware Damerau-Levenshtein distance over reading lattices.
    pub lattice_damerau: usize,
    /// Minimum of surface Damerau-Levenshtein and non-Damerau LPED.
    ///
    /// This intentionally does not include `lattice_damerau`; use that field
    /// directly when lattice-side adjacent transpositions should count as one
    /// edit.
    pub combined: usize,
}

/// Compares two strings using direct kana/romaji handling plus overrides.
pub fn compare_with_overrides(
    left: &str,
    right: &str,
    overrides: &OverrideDictionary,
) -> Result<JapaneseDistance, JaLatticeError> {
    let left_lattice = overrides.romaji_lattice(left)?;
    let right_lattice = overrides.romaji_lattice(right)?;
    Ok(compare_lattices(left, right, &left_lattice, &right_lattice))
}

/// Compares two strings using direct handling and a UniDic reading index.
pub fn compare_with_unidic_index(
    left: &str,
    right: &str,
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> Result<JapaneseDistance, JaLatticeError> {
    let left_lattice = unidic_or_direct_lattice(left, index, options)?;
    let right_lattice = unidic_or_direct_lattice(right, index, options)?;
    Ok(compare_lattices(left, right, &left_lattice, &right_lattice))
}

/// Computes the best normalized similarity across UniDic-backed readings.
pub fn normalized_similarity_with_unidic_index(
    left: &str,
    right: &str,
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> Result<f64, JaLatticeError> {
    let left_paths = unidic_or_direct_romaji_paths(left, index, options)?;
    let right_paths = unidic_or_direct_romaji_paths(right, index, options)?;
    Ok(max_normalized_similarity(&left_paths, &right_paths))
}

/// Builds a romaji lattice from direct input, dictionary readings, or both.
pub fn unidic_or_direct_lattice(
    input: &str,
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> Result<Lattice, JaLatticeError> {
    if let Ok(lattice) = romaji_lattice(input) {
        return Ok(lattice);
    }

    if let Some(lattice) = index.romaji_lattice(input, options)? {
        return Ok(lattice);
    }

    if let Some(lattice) = index.hybrid_romaji_lattice(input, options)? {
        return Ok(lattice);
    }

    romaji_lattice(input)
}

/// Returns romaji paths from direct input, dictionary readings, or both.
pub fn unidic_or_direct_romaji_paths(
    input: &str,
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> Result<Vec<String>, JaLatticeError> {
    if let Ok(paths) = romaji_paths(input) {
        return Ok(paths);
    }

    let paths = index
        .try_reading_paths_with_stats(input, options)
        .map_err(|err| JaLatticeError::ArtifactPayload(err.to_string()))?
        .paths;
    if !paths.is_empty() {
        return romaji_paths_from_reading_paths(&paths);
    }

    let paths = index
        .try_hybrid_reading_paths_with_stats(input, options)
        .map_err(|err| JaLatticeError::ArtifactPayload(err.to_string()))?
        .paths;
    if !paths.is_empty() {
        return romaji_paths_from_reading_paths(&paths);
    }

    romaji_paths(input)
}

fn max_normalized_similarity(left_paths: &[String], right_paths: &[String]) -> f64 {
    left_paths
        .iter()
        .flat_map(|left| {
            right_paths
                .iter()
                .map(move |right| normalized_similarity_str(left, right))
        })
        .fold(0.0, f64::max)
}

fn compare_lattices(
    left: &str,
    right: &str,
    left_lattice: &Lattice,
    right_lattice: &Lattice,
) -> JapaneseDistance {
    let lattice = distance(left_lattice, right_lattice);
    let lattice_damerau = damerau_distance(left_lattice, right_lattice);
    let surface_levenshtein = levenshtein_str(left, right);
    let surface_damerau = damerau_levenshtein_str(left, right);

    JapaneseDistance {
        surface_levenshtein,
        surface_damerau,
        lattice,
        lattice_damerau,
        combined: surface_damerau.min(lattice),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_takes_the_better_surface_or_lattice_distance() {
        let overrides = OverrideDictionary::from_entries([("印刷", ["インサツ"])]);
        let distances =
            compare_with_overrides("いんさt", "印刷", &overrides).expect("should compare");

        assert_eq!(distances.lattice, 1);
        assert_eq!(distances.lattice_damerau, 1);
        assert!(distances.surface_damerau > distances.lattice);
        assert_eq!(distances.combined, distances.lattice);
    }

    #[test]
    fn lattice_damerau_counts_adjacent_romaji_transposition() {
        let distances = compare_with_overrides("モイネ", "モニエ", &OverrideDictionary::default())
            .expect("kana input should compare");

        assert_eq!(distances.lattice, 2);
        assert_eq!(distances.lattice_damerau, 1);
    }

    #[test]
    fn unidic_index_can_compare_ascii_to_dictionary_reading() {
        let csv = "\
茶,1,2,3,名詞,普通名詞,一般,*,*,*,チャ,茶,茶,チャ,茶,チャ,和
道具,1,2,3,名詞,普通名詞,一般,*,*,*,ドウグ,道具,道具,ドーグ,道具,ドーグ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let distances = compare_with_unidic_index(
            "chadougu",
            "茶道具",
            &index,
            DictionaryReadingOptions {
                longest_match_only: true,
                ..DictionaryReadingOptions::default()
            },
        )
        .expect("should compare");

        assert_eq!(distances.lattice, 0);
        assert!(distances.surface_damerau > distances.lattice);
    }

    #[test]
    fn unidic_index_computes_candidate_pair_similarity() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let similarity = normalized_similarity_with_unidic_index(
            "いんさt",
            "印刷",
            &index,
            DictionaryReadingOptions::default(),
        )
        .unwrap();

        assert!((similarity - 6.0 / 7.0).abs() < 1e-12);
    }

    #[test]
    fn artifact_payload_loaders_preserve_comparison_behavior() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let yaml_index =
            UnidicReadingIndex::from_artifact_payload(index.artifact_payload()).unwrap();
        let mut binary = Vec::new();
        index.write_artifact_binary_payload(&mut binary).unwrap();
        let binary_index =
            UnidicReadingIndex::from_binary_artifact_payload_reader(binary.as_slice()).unwrap();
        let options = DictionaryReadingOptions::default();

        let csv_distances = compare_with_unidic_index("いんさt", "印刷", &index, options).unwrap();
        let yaml_distances =
            compare_with_unidic_index("いんさt", "印刷", &yaml_index, options).unwrap();
        let binary_distances =
            compare_with_unidic_index("いんさt", "印刷", &binary_index, options).unwrap();

        assert_eq!(yaml_distances, csv_distances);
        assert_eq!(binary_distances, csv_distances);
    }

    #[test]
    fn unidic_comparison_keeps_ascii_as_identity_path() {
        let csv = "\
c,1,2,3,記号,文字,*,*,*,*,シー,c,c,シー,c,シー,外
h,1,2,3,記号,文字,*,*,*,*,エイチ,h,h,エイチ,h,エイチ,外
a,1,2,3,記号,文字,*,*,*,*,エー,a,a,エー,a,エー,外
d,1,2,3,記号,文字,*,*,*,*,ディー,d,d,ディー,d,ディー,外
o,1,2,3,記号,文字,*,*,*,*,オー,o,o,オー,o,オー,外
u,1,2,3,記号,文字,*,*,*,*,ユー,u,u,ユー,u,ユー,外
g,1,2,3,記号,文字,*,*,*,*,ジー,g,g,ジー,g,ジー,外
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let lattice =
            unidic_or_direct_lattice("chadougu", &index, DictionaryReadingOptions::default())
                .unwrap();
        let trace = moine_core::distance_with_trace(&lattice, &Lattice::from_paths(["chadougu"]));

        assert_eq!(trace.distance, 0);
    }

    #[test]
    fn hybrid_lattice_allows_dictionary_prefix_and_direct_tail() {
        let csv = "\
印,1,2,3,名詞,普通名詞,一般,*,*,*,イン,印,印,イン,印,イン,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let lattice =
            unidic_or_direct_lattice("印さt", &index, DictionaryReadingOptions::default()).unwrap();
        let trace = moine_core::distance_with_trace(&lattice, &Lattice::from_paths(["insat"]));

        assert_eq!(trace.distance, 0);
    }

    #[test]
    fn hybrid_lattice_allows_direct_prefix_and_dictionary_tail() {
        let csv = "\
具,1,2,3,名詞,普通名詞,一般,*,*,*,グ,具,具,グ,具,グ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let lattice =
            unidic_or_direct_lattice("chadou具", &index, DictionaryReadingOptions::default())
                .unwrap();
        let trace = moine_core::distance_with_trace(&lattice, &Lattice::from_paths(["chadougu"]));

        assert_eq!(trace.distance, 0);
    }

    #[test]
    fn hybrid_lattice_supports_mixed_middle_ascii() {
        let csv = "\
東,1,2,3,名詞,普通名詞,一般,*,*,*,トウ,東,東,トウ,東,トウ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let lattice =
            unidic_or_direct_lattice("東kょう", &index, DictionaryReadingOptions::default())
                .unwrap();
        let trace = moine_core::distance_with_trace(&lattice, &Lattice::from_paths(["toukyou"]));

        assert_eq!(trace.distance, 0);
    }

    #[test]
    fn hybrid_lattice_does_not_guess_unknown_kanji() {
        let index = UnidicReadingIndex::default();
        let err = unidic_or_direct_lattice("未知z", &index, DictionaryReadingOptions::default())
            .unwrap_err();

        assert!(matches!(
            err,
            JaLatticeError::UnsupportedChar {
                ch: '未', index: 0
            }
        ));
    }

    #[test]
    fn hybrid_comparison_handles_dictionary_and_ascii_mixture() {
        let csv = "\
鬼滅,1,2,3,名詞,普通名詞,一般,*,*,*,キメツ,鬼滅,鬼滅,キメツ,鬼滅,キメツ,固
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let distances = compare_with_unidic_index(
            "鬼滅のyaiba",
            "鬼滅の刃",
            &index,
            DictionaryReadingOptions {
                longest_match_only: true,
                ..DictionaryReadingOptions::default()
            },
        )
        .unwrap();

        assert_eq!(distances.lattice, 0);
    }
}
