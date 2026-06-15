use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use moine_core::{distance_with_trace, try_distance_with_trace, DistanceTrace, Lattice, Symbol};
use moine_ja::{
    compare_with_overrides, compare_with_unidic_index, unidic_or_direct_lattice,
    DictionaryReadingOptions, DictionaryReadingStats, JapaneseDistance, OverrideDictionary,
    UnidicIndexOptions, UnidicReadingIndex,
};
use moine_zh::{
    compare_with_zh_index, zh_or_direct_lattice, CedictReadingIndex, ChineseDistance,
    PinyinReadingOptions, PinyinReadingStats,
};

use crate::archive::{ensure_output_parent, write_output_file};
use crate::args::{
    max_readings_per_segment_label, max_readings_per_surface_label, unidic_reading_field_name,
    ArtifactPayloadFormat, CedictReadingsOptions, CedictSequencesOptions, ChineseCompareOptions,
    CliError, CompareOptions, RomajiLatticeOutputFormat, SudachiCsvReadingsOptions,
    SudachiCsvSequencesOptions, UnidicCsvReadingsOptions, UnidicCsvSequencesOptions,
    UnidicReadingsOptions,
};
use crate::commands::unidic_artifact::{
    dictionary_options_from_metadata, load_artifact_payload_by_format,
    load_unidic_artifact_bundle_for_runtime,
};
use crate::commands::zh_artifact::load_zh_index;

const ROMAJI_DOT_BEST_PATH_COLOR: &str = "#9a5b38";
const ROMAJI_DOT_DEFAULT_NODE_COLOR: &str = "#495057";
const ROMAJI_DOT_MUTED_EDGE_COLOR: &str = "#868e96";

pub(crate) fn run_cedict_readings(options: CedictReadingsOptions) -> Result<(), Box<dyn Error>> {
    let index =
        CedictReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;

    println!("surface: {}", options.surface);
    println!(
        "pinyin_view: {}",
        options.index_options.pinyin_view.as_str()
    );
    println!("entries: {}", index.len());
    println!("readings:");
    if let Some(readings) = index.readings(&options.surface) {
        for reading in readings.as_ref() {
            println!("  - {reading}");
        }
    }

    Ok(())
}

pub(crate) fn run_cedict_sequences(options: CedictSequencesOptions) -> Result<(), Box<dyn Error>> {
    let index =
        CedictReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;
    let expansion = index.hybrid_reading_paths_with_stats(&options.text, options.reading_options);

    println!("text: {}", options.text);
    println!(
        "pinyin_view: {}",
        options.index_options.pinyin_view.as_str()
    );
    println!(
        "max_readings_segment: {}",
        max_readings_per_segment_label(options.reading_options.max_readings_per_segment)
    );
    println!("entries: {}", index.len());
    print_pinyin_stats("expansion", &expansion.stats);
    println!("paths:");
    for path in expansion.paths {
        println!("  - reading: {}", path.joined_reading);
        println!("    segments: {}", format_pinyin_segments(&path.segments));
    }

    Ok(())
}

pub(crate) fn run_chinese_compare(options: ChineseCompareOptions) -> Result<(), Box<dyn Error>> {
    let index = load_zh_index(&options.source, options.index_options)?;
    let distances = compare_with_zh_index(
        &options.left,
        &options.right,
        &index,
        options.reading_options,
    )?;
    let left_lattice = zh_or_direct_lattice(&options.left, &index, options.reading_options)?;
    let right_lattice = zh_or_direct_lattice(&options.right, &index, options.reading_options)?;
    let trace = distance_with_trace(&left_lattice, &right_lattice);
    let left_expansion = query_pinyin_expansion(&options.left, &index, options.reading_options);
    let right_expansion = query_pinyin_expansion(&options.right, &index, options.reading_options);
    let (source_label, source_path) = options.source.label();

    println!("left:  {}", options.left);
    println!("right: {}", options.right);
    println!();
    println!("{source_label}: {source_path}");
    println!("pinyin_view: {}", index.pinyin_view().as_str());
    println!(
        "max_readings_surface: {}",
        max_readings_per_surface_label(options.index_options.max_readings_per_surface)
    );
    println!(
        "max_readings_segment: {}",
        max_readings_per_segment_label(options.reading_options.max_readings_per_segment)
    );
    println!(
        "longest_only: {}",
        options.reading_options.longest_match_only
    );
    println!("entries: {}", index.len());
    println!();
    println!("surface_levenshtein: {}", distances.surface_levenshtein);
    println!("surface_damerau:     {}", distances.surface_damerau);
    print_pinyin_query_stats("left_expansion", &left_expansion);
    print_pinyin_query_stats("right_expansion", &right_expansion);
    print_chinese_lattice_result("cn_pinyin_lattice", distances, &trace);

    Ok(())
}

pub(crate) fn run_unidic_csv_sequences(
    options: UnidicCsvSequencesOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let expansion = index.reading_paths_with_stats(&options.text, options.dictionary_options);

    println!("text: {}", options.text);
    println!(
        "field: {}",
        unidic_reading_field_name(options.index_options.reading_field)
    );
    println!(
        "max_readings_segment: {}",
        max_readings_per_segment_label(options.dictionary_options.max_readings_per_segment)
    );
    println!("entries: {}", index.len());
    print_reading_stats("expansion", &expansion.stats);
    println!("paths:");
    for path in expansion.paths {
        println!("  - reading: {}", path.joined_reading);
        println!("    segments: {}", format_reading_segments(&path.segments));
    }

    Ok(())
}

pub(crate) fn run_unidic_csv_readings(
    options: UnidicCsvReadingsOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;

    println!("surface: {}", options.surface);
    println!(
        "field: {}",
        unidic_reading_field_name(options.index_options.reading_field)
    );
    println!("entries: {}", index.len());
    println!("readings:");
    if let Some(readings) = index.readings(&options.surface) {
        for reading in readings.as_ref() {
            println!("  - {reading}");
        }
    }

    Ok(())
}

pub(crate) fn run_sudachi_csv_sequences(
    options: SudachiCsvSequencesOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_sudachi_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let expansion = index.reading_paths_with_stats(&options.text, options.dictionary_options);

    println!("text: {}", options.text);
    println!("source: sudachi_lex_csv");
    println!(
        "max_readings_segment: {}",
        max_readings_per_segment_label(options.dictionary_options.max_readings_per_segment)
    );
    println!("entries: {}", index.len());
    print_reading_stats("expansion", &expansion.stats);
    println!("paths:");
    for path in expansion.paths {
        println!("  - reading: {}", path.joined_reading);
        println!("    segments: {}", format_reading_segments(&path.segments));
    }

    Ok(())
}

pub(crate) fn run_sudachi_csv_readings(
    options: SudachiCsvReadingsOptions,
) -> Result<(), Box<dyn Error>> {
    let index = UnidicReadingIndex::from_sudachi_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;

    println!("surface: {}", options.surface);
    println!("source: sudachi_lex_csv");
    println!("entries: {}", index.len());
    println!("readings:");
    if let Some(readings) = index.readings(&options.surface) {
        for reading in readings.as_ref() {
            println!("  - {reading}");
        }
    }

    Ok(())
}

pub(crate) fn run_unidic_readings(options: UnidicReadingsOptions) -> Result<(), Box<dyn Error>> {
    let mut child = Command::new("mecab")
        .arg("-d")
        .arg(&options.dic_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(options.text.as_bytes())?;
        stdin.write_all(b"\n")?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        return Err(Box::new(CliError::CommandFailed {
            command: "mecab".to_string(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }));
    }

    let stdout = String::from_utf8(output.stdout)?;
    let tokens = parse_mecab_tokens(&stdout);
    let reading = tokens
        .iter()
        .filter_map(|token| token.reading.as_deref())
        .collect::<String>();

    println!("text: {}", options.text);
    println!("reading: {}", reading);
    println!();
    println!("tokens:");
    for token in tokens {
        println!(
            "  - surface: {}\n    reading: {}",
            token.surface,
            token.reading.as_deref().unwrap_or("*")
        );
    }

    Ok(())
}

pub(crate) fn run_compare(options: CompareOptions) -> Result<(), Box<dyn Error>> {
    let mut romaji_lattice_data = None;

    let override_result = if let Some(overrides_path) = &options.overrides {
        let override_yaml = fs::read_to_string(overrides_path)?;
        let overrides = OverrideDictionary::from_yaml_str(&override_yaml)?;
        let distances = compare_with_overrides(&options.left, &options.right, &overrides)?;
        let left_lattice = overrides.romaji_lattice(&options.left)?;
        let right_lattice = overrides.romaji_lattice(&options.right)?;
        let trace = distance_with_trace(&left_lattice, &right_lattice);
        if options.romaji_lattice.is_some() {
            romaji_lattice_data = Some(RomajiLatticeData {
                left_input: options.left.clone(),
                right_input: options.right.clone(),
                left_lattice: left_lattice.clone(),
                right_lattice: right_lattice.clone(),
                distance: distances.lattice,
                trace: Some(trace.clone()),
                trace_error: None,
            });
        }
        Some((distances, trace))
    } else {
        None
    };

    let dict_result = if options.lex_csv.is_some()
        || options.sudachi_lex_csv.is_some()
        || options.artifact_payload.is_some()
        || options.artifact_metadata.is_some()
    {
        let (index, source, dictionary_options) = if let Some(lex_csv) = &options.lex_csv {
            (
                UnidicReadingIndex::from_lex_csv_path_with_options(lex_csv, options.index_options)?,
                DictComparisonSource::LexCsv {
                    path: lex_csv.clone(),
                    index_options: options.index_options,
                },
                options.dictionary_options,
            )
        } else if let Some(lex_csv) = &options.sudachi_lex_csv {
            (
                UnidicReadingIndex::from_sudachi_lex_csv_path_with_options(
                    lex_csv,
                    options.sudachi_index_options,
                )?,
                DictComparisonSource::SudachiLexCsv {
                    path: lex_csv.clone(),
                    index_options: options.sudachi_index_options,
                },
                options.dictionary_options,
            )
        } else if let Some(metadata_path) = &options.artifact_metadata {
            let loaded = load_unidic_artifact_bundle_for_runtime(metadata_path, None)?;
            let payload_path = loaded.payload_path.display().to_string();
            let payload_format = loaded.metadata.payload.format.clone();
            let source_name = loaded.metadata.source.name.clone();
            let dictionary_options = options
                .dictionary_option_overrides
                .apply_to(dictionary_options_from_metadata(&loaded.metadata));
            (
                loaded.index,
                DictComparisonSource::ArtifactMetadata {
                    source_name,
                    metadata_path: metadata_path.clone(),
                    payload_path,
                    payload_format,
                    file_digest_verified: loaded.file_digest_verified,
                },
                dictionary_options,
            )
        } else {
            let payload = options
                .artifact_payload
                .as_ref()
                .expect("artifact payload should be present");
            (
                load_artifact_payload_by_format(
                    Path::new(payload),
                    options.payload_format.as_str(),
                )?,
                DictComparisonSource::ArtifactPayload {
                    path: payload.clone(),
                    payload_format: options.payload_format,
                },
                options.dictionary_options,
            )
        };
        let distances =
            compare_with_unidic_index(&options.left, &options.right, &index, dictionary_options)?;
        let left_lattice = unidic_or_direct_lattice(&options.left, &index, dictionary_options)?;
        let right_lattice = unidic_or_direct_lattice(&options.right, &index, dictionary_options)?;
        let (trace, trace_error) = match try_distance_with_trace(&left_lattice, &right_lattice) {
            Ok(trace) => (Some(trace), None),
            Err(err) => (None, Some(err.to_string())),
        };
        if options.romaji_lattice.is_some() {
            romaji_lattice_data = Some(RomajiLatticeData {
                left_input: options.left.clone(),
                right_input: options.right.clone(),
                left_lattice: left_lattice.clone(),
                right_lattice: right_lattice.clone(),
                distance: distances.lattice,
                trace: trace.clone(),
                trace_error: trace_error.clone(),
            });
        }
        let left_expansion = query_reading_expansion(&options.left, &index, dictionary_options);
        let right_expansion = query_reading_expansion(&options.right, &index, dictionary_options);
        Some(DictComparisonResult {
            source,
            dictionary_options,
            distances,
            trace,
            trace_error,
            left_expansion,
            right_expansion,
        })
    } else {
        None
    };

    let surface_distances = override_result
        .as_ref()
        .map(|(distances, _)| *distances)
        .or_else(|| dict_result.as_ref().map(|result| result.distances))
        .expect("comparison method should be present");

    println!("left:  {}", options.left);
    println!("right: {}", options.right);
    println!();
    println!(
        "surface_levenshtein: {}",
        surface_distances.surface_levenshtein
    );
    println!("surface_damerau:     {}", surface_distances.surface_damerau);

    if let Some((distances, trace)) = override_result {
        print_lattice_result("ja_override_lattice", distances, Some(&trace), None);
    }

    if let Some(result) = dict_result {
        println!();
        print_dict_comparison_source(&result.source);
        println!(
            "max_readings_segment: {}",
            max_readings_per_segment_label(result.dictionary_options.max_readings_per_segment)
        );
        println!(
            "dictionary_longest_only: {}",
            result.dictionary_options.longest_match_only
        );
        print_query_reading_stats("left_expansion", &result.left_expansion);
        print_query_reading_stats("right_expansion", &result.right_expansion);
        print_lattice_result(
            "ja_dict_lattice",
            result.distances,
            result.trace.as_ref(),
            result.trace_error.as_deref(),
        );
    }

    if let Some(path) = &options.romaji_lattice {
        let data = romaji_lattice_data
            .as_ref()
            .expect("comparison method should provide lattices for graph output");
        let dot = romaji_lattice_dot(data);
        match options.output_format {
            RomajiLatticeOutputFormat::Dot => write_output_file(Path::new(path), &dot)?,
            RomajiLatticeOutputFormat::Svg | RomajiLatticeOutputFormat::Png => {
                write_romaji_lattice_graph(Path::new(path), &dot, options.output_format)?;
            }
        }
        println!();
        println!(
            "romaji_lattice: {path} ({})",
            options.output_format.as_str()
        );
    }

    Ok(())
}

pub(crate) fn print_lattice_result(
    label: &str,
    distances: JapaneseDistance,
    trace: Option<&moine_core::DistanceTrace>,
    trace_error: Option<&str>,
) {
    println!();
    println!("{label}: {}", distances.lattice);
    println!("{label}_damerau: {}", distances.lattice_damerau);
    println!("{label}_combined: {}", distances.combined);
    if let Some(trace) = trace {
        println!("{label}_best_path:");
        println!("  left:  {}", symbols_to_string(&trace.left_symbols()));
        println!("  right: {}", symbols_to_string(&trace.right_symbols()));
    } else if let Some(error) = trace_error {
        println!("{label}_best_path: unavailable ({error})");
    }
}

pub(crate) fn query_reading_expansion(
    input: &str,
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> QueryReadingExpansion {
    let has_direct_romaji = moine_ja::romaji_lattice(input).is_ok();
    if has_direct_romaji && !input.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        return QueryReadingExpansion::DirectRomaji;
    }

    let dictionary_expansion = index.reading_paths_with_stats(input, options);
    let expansion = if dictionary_expansion.paths.is_empty() {
        index.hybrid_reading_paths_with_stats(input, options)
    } else {
        dictionary_expansion
    };

    if expansion.paths.is_empty() && has_direct_romaji {
        return QueryReadingExpansion::DirectRomaji;
    }

    QueryReadingExpansion::Dictionary {
        has_direct_romaji,
        path_count: expansion.paths.len(),
        stats: expansion.stats,
    }
}

pub(crate) fn print_query_reading_stats(label: &str, expansion: &QueryReadingExpansion) {
    match expansion {
        QueryReadingExpansion::DirectRomaji => println!("{label}: direct_romaji"),
        QueryReadingExpansion::Dictionary {
            has_direct_romaji,
            path_count,
            stats,
        } => {
            println!("{label}_direct_romaji: {has_direct_romaji}");
            print_reading_stats(label, stats);
            println!("{label}_paths: {path_count}");
        }
    }
}

pub(crate) fn print_dict_comparison_source(source: &DictComparisonSource) {
    match source {
        DictComparisonSource::LexCsv {
            path,
            index_options,
        } => {
            println!("unidic_source:      lex_csv");
            println!("unidic_lex_csv:     {path}");
            println!(
                "unidic_field:       {}",
                unidic_reading_field_name(index_options.reading_field)
            );
            println!(
                "max_readings_surface: {}",
                max_readings_per_surface_label(index_options.max_readings_per_surface)
            );
            println!(
                "exclude_ascii:      {}",
                index_options.exclude_ascii_surfaces
            );
            println!("exclude_symbol_pos: {}", index_options.exclude_symbol_pos);
        }
        DictComparisonSource::SudachiLexCsv {
            path,
            index_options,
        } => {
            println!("sudachi_source:     lex_csv");
            println!("sudachi_lex_csv:    {path}");
            println!(
                "max_readings_surface: {}",
                max_readings_per_surface_label(index_options.max_readings_per_surface)
            );
            println!(
                "exclude_ascii:      {}",
                index_options.exclude_ascii_surfaces
            );
            println!("exclude_symbol_pos: {}", index_options.exclude_symbol_pos);
            println!(
                "normalized_surfaces: {}",
                index_options.include_normalized_surfaces
            );
            println!(
                "exclude_unsupported_readings: {}",
                index_options.exclude_unsupported_readings
            );
        }
        DictComparisonSource::ArtifactPayload {
            path,
            payload_format,
        } => {
            println!("dictionary_source: artifact_payload");
            println!("artifact_payload:   {path}");
            println!("payload_format:     {}", payload_format.as_str());
        }
        DictComparisonSource::ArtifactMetadata {
            source_name,
            metadata_path,
            payload_path,
            payload_format,
            file_digest_verified,
        } => {
            println!("dictionary_source: artifact_metadata");
            println!("source_name:        {source_name}");
            println!("artifact_metadata:  {metadata_path}");
            println!("artifact_payload:   {payload_path}");
            println!("payload_format:     {payload_format}");
            println!("file_digest:        verified={file_digest_verified}");
        }
    }
}

pub(crate) fn print_reading_stats(label: &str, stats: &DictionaryReadingStats) {
    println!("{label}_stats:");
    println!("  matched_spans: {}", stats.matched_spans);
    println!("  direct_fallback_spans: {}", stats.direct_fallback_spans);
    println!(
        "  longest_match_pruned_spans: {}",
        stats.longest_match_pruned_spans
    );
    println!("  raw_segment_readings: {}", stats.raw_segment_readings);
    println!("  used_segment_readings: {}", stats.used_segment_readings);
    println!(
        "  pruned_segment_readings: {}",
        stats.pruned_segment_readings
    );
    println!("  candidate_combinations: {}", stats.candidate_combinations);
    println!("  unique_paths: {}", stats.unique_paths);
    println!(
        "  duplicate_joined_readings: {}",
        stats.duplicate_joined_readings
    );
    println!("  max_paths_hit_count: {}", stats.max_paths_hit_count);
}

pub(crate) fn query_pinyin_expansion(
    input: &str,
    index: &CedictReadingIndex,
    options: PinyinReadingOptions,
) -> PinyinQueryExpansion {
    if input.is_ascii() && !input.is_empty() {
        return PinyinQueryExpansion::DirectAscii;
    }

    let expansion = index.reading_paths_with_stats(input, options);
    let expansion = if expansion.paths.is_empty() {
        index.hybrid_reading_paths_with_stats(input, options)
    } else {
        expansion
    };
    PinyinQueryExpansion::Dictionary {
        path_count: expansion.paths.len(),
        stats: expansion.stats,
    }
}

pub(crate) fn print_pinyin_query_stats(label: &str, expansion: &PinyinQueryExpansion) {
    match expansion {
        PinyinQueryExpansion::DirectAscii => println!("{label}: direct_ascii"),
        PinyinQueryExpansion::Dictionary { path_count, stats } => {
            print_pinyin_stats(label, stats);
            println!("{label}_paths: {path_count}");
        }
    }
}

pub(crate) fn print_pinyin_stats(label: &str, stats: &PinyinReadingStats) {
    println!("{label}_stats:");
    println!("  matched_spans: {}", stats.matched_spans);
    println!("  direct_fallback_spans: {}", stats.direct_fallback_spans);
    println!(
        "  longest_match_pruned_spans: {}",
        stats.longest_match_pruned_spans
    );
    println!("  raw_segment_readings: {}", stats.raw_segment_readings);
    println!("  used_segment_readings: {}", stats.used_segment_readings);
    println!(
        "  pruned_segment_readings: {}",
        stats.pruned_segment_readings
    );
    println!("  candidate_combinations: {}", stats.candidate_combinations);
    println!("  unique_paths: {}", stats.unique_paths);
    println!(
        "  duplicate_joined_readings: {}",
        stats.duplicate_joined_readings
    );
    println!("  max_paths_hit_count: {}", stats.max_paths_hit_count);
}

pub(crate) fn print_chinese_lattice_result(
    label: &str,
    distances: ChineseDistance,
    trace: &moine_core::DistanceTrace,
) {
    println!();
    println!("{label}: {}", distances.lattice);
    println!("{label}_damerau: {}", distances.lattice_damerau);
    println!("{label}_combined: {}", distances.combined);
    println!("{label}_best_path:");
    println!("  left:  {}", symbols_to_string(&trace.left_symbols()));
    println!("  right: {}", symbols_to_string(&trace.right_symbols()));
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MecabToken {
    pub(crate) surface: String,
    pub(crate) reading: Option<String>,
}

pub(crate) fn parse_mecab_tokens(output: &str) -> Vec<MecabToken> {
    output
        .lines()
        .filter(|line| *line != "EOS")
        .filter_map(|line| {
            let (surface, features) = line.split_once('\t')?;
            let fields = features.split(',').collect::<Vec<_>>();
            let reading = fields
                .get(6)
                .filter(|reading| **reading != "*")
                .map(|reading| (*reading).to_string());
            Some(MecabToken {
                surface: surface.to_string(),
                reading,
            })
        })
        .collect()
}

pub(crate) fn symbols_to_string(symbols: &[moine_core::Symbol]) -> String {
    symbols
        .iter()
        .map(|&symbol| char::from_u32(symbol).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

pub(crate) fn romaji_lattice_dot(data: &RomajiLatticeData) -> String {
    let left_symbols = data.trace.as_ref().map(DistanceTrace::left_symbols);
    let right_symbols = data.trace.as_ref().map(DistanceTrace::right_symbols);
    let left_best_arcs = left_symbols
        .as_deref()
        .map(|symbols| best_arc_keys(&data.left_lattice, symbols))
        .unwrap_or_default();
    let right_best_arcs = right_symbols
        .as_deref()
        .map(|symbols| best_arc_keys(&data.right_lattice, symbols))
        .unwrap_or_default();
    let left_best_nodes = best_nodes(&left_best_arcs);
    let right_best_nodes = best_nodes(&right_best_arcs);

    let best_path_label = match (&left_symbols, &right_symbols, &data.trace_error) {
        (Some(left), Some(right), _) => format!(
            "best_left={}\\nbest_right={}",
            dot_escape(&symbols_to_string(left)),
            dot_escape(&symbols_to_string(right))
        ),
        (_, _, Some(error)) => format!("best path unavailable: {}", dot_escape(error)),
        _ => "best path unavailable".to_string(),
    };
    let graph_label = format!("distance={}\\n{}", data.distance, best_path_label);

    let mut dot = String::new();
    dot.push_str("digraph moine_romaji_lattice {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  graph [fontname=\"Helvetica\", labelloc=\"t\", label=\"");
    dot.push_str(&graph_label);
    dot.push_str("\"];\n");
    dot.push_str(&format!(
        "  node [fontname=\"Helvetica\", shape=circle, width=0.48, fixedsize=true, color=\"{ROMAJI_DOT_DEFAULT_NODE_COLOR}\"];\n",
    ));
    dot.push_str(&format!(
        "  edge [fontname=\"Helvetica\", color=\"{ROMAJI_DOT_DEFAULT_NODE_COLOR}\", arrowsize=0.7];\n\n"
    ));

    append_lattice_cluster(
        &mut dot,
        "right",
        "RIGHT",
        &data.right_input,
        &data.right_lattice,
        &right_best_arcs,
        &right_best_nodes,
    );
    dot.push('\n');
    append_lattice_cluster(
        &mut dot,
        "left",
        "LEFT",
        &data.left_input,
        &data.left_lattice,
        &left_best_arcs,
        &left_best_nodes,
    );
    dot.push_str("}\n");
    dot
}

pub(crate) fn write_romaji_lattice_graph(
    path: &Path,
    dot: &str,
    output_format: RomajiLatticeOutputFormat,
) -> Result<(), Box<dyn Error>> {
    write_romaji_lattice_graph_with_dot_command(path, dot, output_format, "dot")
}

pub(crate) fn write_romaji_lattice_graph_with_dot_command(
    path: &Path,
    dot: &str,
    output_format: RomajiLatticeOutputFormat,
    dot_command: &str,
) -> Result<(), Box<dyn Error>> {
    let Some(graphviz_format) = output_format.graphviz_format() else {
        write_output_file(path, dot)?;
        return Ok(());
    };

    ensure_output_parent(path)?;
    let mut child = Command::new(dot_command)
        .arg(format!("-T{graphviz_format}"))
        .arg("-o")
        .arg(path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                Box::new(CliError::CommandUnavailable {
                    command: dot_command.to_string(),
                    hint: "install Graphviz to use --output-format svg or --output-format png, or use --output-format dot instead".to_string(),
                }) as Box<dyn Error>
            } else {
                Box::new(err) as Box<dyn Error>
            }
        })?;

    {
        let stdin = child.stdin.as_mut().expect("dot stdin should be piped");
        stdin.write_all(dot.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(Box::new(CliError::CommandFailed {
            command: format!("{dot_command} -T{graphviz_format}"),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }));
    }

    Ok(())
}

fn append_lattice_cluster(
    dot: &mut String,
    prefix: &str,
    lane_label: &str,
    input: &str,
    lattice: &Lattice,
    best_arcs: &BTreeSet<ArcKey>,
    best_nodes: &BTreeSet<usize>,
) {
    dot.push_str(&format!("  subgraph cluster_{prefix} {{\n"));
    dot.push_str("    style=\"rounded\";\n");
    dot.push_str("    color=\"#ced4da\";\n");
    dot.push_str("    label=\"");
    dot.push_str(&format!("{lane_label}\\ninput={}", dot_escape(input)));
    dot.push_str("\";\n");
    for node in 0..lattice.node_count() {
        let label = if node == lattice.start() {
            "BOS".to_string()
        } else if node == lattice.end() {
            "EOS".to_string()
        } else {
            node.to_string()
        };
        let shape = if node == lattice.end() {
            "doublecircle"
        } else {
            "circle"
        };
        let color = if best_nodes.contains(&node) {
            ROMAJI_DOT_BEST_PATH_COLOR
        } else {
            ROMAJI_DOT_DEFAULT_NODE_COLOR
        };
        let penwidth = if best_nodes.contains(&node) {
            "2.4"
        } else {
            "1.2"
        };
        dot.push_str(&format!(
            "    {prefix}_{node} [label=\"{}\", shape={shape}, color=\"{color}\", penwidth={penwidth}];\n",
            dot_escape(&label)
        ));
    }
    for arc in lattice.arcs() {
        let key = arc_key(arc.src, arc.dst, arc.symbol);
        let is_best = best_arcs.contains(&key);
        let color = if is_best {
            ROMAJI_DOT_BEST_PATH_COLOR
        } else {
            ROMAJI_DOT_MUTED_EDGE_COLOR
        };
        let penwidth = if is_best { "3.0" } else { "1.1" };
        dot.push_str(&format!(
            "    {prefix}_{} -> {prefix}_{} [label=\"{}\", color=\"{color}\", fontcolor=\"{color}\", penwidth={penwidth}];\n",
            arc.src,
            arc.dst,
            dot_escape(&symbol_to_string(arc.symbol))
        ));
    }
    dot.push_str("  }\n");
}

type ArcKey = (usize, usize, Symbol);

fn arc_key(src: usize, dst: usize, symbol: Symbol) -> ArcKey {
    (src, dst, symbol)
}

fn best_arc_keys(lattice: &Lattice, symbols: &[Symbol]) -> BTreeSet<ArcKey> {
    let mut path = Vec::new();
    if find_arc_path(lattice, lattice.start(), symbols, 0, &mut path) {
        path.into_iter().collect()
    } else {
        BTreeSet::new()
    }
}

fn find_arc_path(
    lattice: &Lattice,
    node: usize,
    symbols: &[Symbol],
    symbol_idx: usize,
    path: &mut Vec<ArcKey>,
) -> bool {
    if symbol_idx == symbols.len() {
        return node == lattice.end();
    }

    for arc in lattice.outgoing_arcs(node) {
        if arc.symbol != symbols[symbol_idx] {
            continue;
        }
        path.push(arc_key(arc.src, arc.dst, arc.symbol));
        if find_arc_path(lattice, arc.dst, symbols, symbol_idx + 1, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn best_nodes(best_arcs: &BTreeSet<ArcKey>) -> BTreeSet<usize> {
    let mut nodes = BTreeSet::new();
    for &(src, dst, _) in best_arcs {
        nodes.insert(src);
        nodes.insert(dst);
    }
    nodes
}

fn symbol_to_string(symbol: Symbol) -> String {
    char::from_u32(symbol)
        .unwrap_or(char::REPLACEMENT_CHARACTER)
        .to_string()
}

fn dot_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => {}
            _ => escaped.push(ch),
        }
    }
    escaped
}

pub(crate) fn format_reading_segments(segments: &[moine_ja::DictionaryReadingSegment]) -> String {
    segments
        .iter()
        .map(|segment| format!("{}/{}", segment.surface, segment.reading))
        .collect::<Vec<_>>()
        .join(" + ")
}

pub(crate) fn format_pinyin_segments(segments: &[moine_zh::PinyinReadingSegment]) -> String {
    segments
        .iter()
        .map(|segment| format!("{}/{}", segment.surface, segment.reading))
        .collect::<Vec<_>>()
        .join(" + ")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DictComparisonResult {
    pub(crate) source: DictComparisonSource,
    pub(crate) dictionary_options: DictionaryReadingOptions,
    pub(crate) distances: JapaneseDistance,
    pub(crate) trace: Option<moine_core::DistanceTrace>,
    pub(crate) trace_error: Option<String>,
    pub(crate) left_expansion: QueryReadingExpansion,
    pub(crate) right_expansion: QueryReadingExpansion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DictComparisonSource {
    LexCsv {
        path: String,
        index_options: UnidicIndexOptions,
    },
    SudachiLexCsv {
        path: String,
        index_options: moine_ja::SudachiIndexOptions,
    },
    ArtifactPayload {
        path: String,
        payload_format: ArtifactPayloadFormat,
    },
    ArtifactMetadata {
        source_name: String,
        metadata_path: String,
        payload_path: String,
        payload_format: String,
        file_digest_verified: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum QueryReadingExpansion {
    DirectRomaji,
    Dictionary {
        has_direct_romaji: bool,
        path_count: usize,
        stats: DictionaryReadingStats,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PinyinQueryExpansion {
    DirectAscii,
    Dictionary {
        path_count: usize,
        stats: PinyinReadingStats,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RomajiLatticeData {
    pub(crate) left_input: String,
    pub(crate) right_input: String,
    pub(crate) left_lattice: Lattice,
    pub(crate) right_lattice: Lattice,
    pub(crate) distance: usize,
    pub(crate) trace: Option<DistanceTrace>,
    pub(crate) trace_error: Option<String>,
}
