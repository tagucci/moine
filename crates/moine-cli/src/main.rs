use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, Read, Write as _};
use std::path::{Path, PathBuf};
use std::process;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::time::Instant;

use flate2::{read::GzDecoder, write::GzEncoder, Compression, GzBuilder};
use moine_core::{
    damerau_levenshtein_str, distance_with_trace, levenshtein_str, try_distance_with_trace,
};
use moine_ja::{
    artifact_file_digest_path, compare_with_overrides, compare_with_unidic_index,
    unidic_or_direct_lattice, DictionaryReadingOptions, DictionaryReadingStats, JapaneseDistance,
    OverrideDictionary, UnidicArtifactLicense, UnidicArtifactMetadata,
    UnidicArtifactMetadataOptions, UnidicIndexOptions, UnidicReadingField, UnidicReadingIndex,
    ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM, ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM,
};
use moine_zh::{
    artifact_file_digest_path as zh_artifact_file_digest_path, compare_with_zh_index,
    zh_or_direct_lattice, CedictIndexOptions, CedictReadingIndex, ChineseDistance,
    PinyinReadingOptions, PinyinReadingStats, PinyinView, ZhArtifactLicense, ZhArtifactMetadata,
    ZhArtifactMetadataOptions, ZhReadingIndex,
    ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM as ZH_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM,
    ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM as ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM,
};
use serde::Deserialize;
use sha2::Digest;

const YAML_PAYLOAD_FORMAT: &str = "yaml.surface-readings.v1";
const BINARY_PAYLOAD_FORMAT: &str = "binary.surface-readings.v1";
const INDEXED_PAYLOAD_FORMAT: &str = "indexed-fst.surface-readings.v1";
const ZSTD_COMPRESSION_LEVEL: i32 = 19;
const JWTD_UNSCORABLE_DISTANCE: usize = usize::MAX / 4;
const DOWNLOAD_TIMEOUT_SECS: u64 = 60;
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CHECKSUM_MANIFEST_BYTES: u64 = 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactLanguage {
    Japanese,
    Chinese,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DownloadArtifactSpec {
    language: ArtifactLanguage,
    artifact_name: &'static str,
    archive_name: &'static str,
    archive_url: &'static str,
    checksum_url: Option<&'static str>,
}

const DOWNLOAD_ARTIFACT_SPECS: &[DownloadArtifactSpec] = &[
    DownloadArtifactSpec {
        language: ArtifactLanguage::Japanese,
        artifact_name: "moine-unidic-cwj-202512",
        archive_name: "moine-unidic-cwj-202512.tar.gz",
        archive_url: concat!(
            "https://github.com/tagucci/moine/releases/download/",
            "unidic-cwj-202512-v0.1.0/moine-unidic-cwj-202512.tar.gz"
        ),
        checksum_url: None,
    },
    DownloadArtifactSpec {
        language: ArtifactLanguage::Chinese,
        artifact_name: "moine-cedict-20260520",
        archive_name: "moine-cedict-20260520.tar.gz",
        archive_url: concat!(
            "https://github.com/tagucci/moine/releases/download/",
            "moine-cedict-20260520-v0.1.0/moine-cedict-20260520.tar.gz"
        ),
        checksum_url: None,
    },
];

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_usage();
        return Err(Box::new(CliError::MissingCommand));
    };

    match command.as_str() {
        "cedict-readings" => run_cedict_readings(args.collect()),
        "cedict-sequences" => run_cedict_sequences(args.collect()),
        "chinese-compare" => run_chinese_compare(args.collect()),
        "compare" => run_compare(args.collect()),
        "download" => run_download(args.collect()),
        "list" => run_download_list(args.collect()),
        "where" => run_download_where(args.collect()),
        "zh-artifact-archive" => run_zh_artifact_archive(args.collect()),
        "zh-artifact-bundle" => run_zh_artifact_bundle(args.collect()),
        "zh-artifact-inspect" => run_zh_artifact_inspect(args.collect()),
        "zh-artifact-metadata" => run_zh_artifact_metadata(args.collect()),
        "zh-artifact-payload" => run_zh_artifact_payload(args.collect()),
        "zh-artifact-release-checksums" => run_unidic_artifact_release_checksums(args.collect()),
        "zh-artifact-verify" => run_zh_artifact_verify(args.collect()),
        "japanese-report" => run_japanese_report(args.collect()),
        "jwtd-scorer-report" => run_jwtd_scorer_report(args.collect()),
        "jwtd-summary" => run_jwtd_summary(args.collect()),
        "unidic-artifact-binary-inspect" => run_unidic_artifact_binary_inspect(args.collect()),
        "unidic-artifact-binary-payload" => run_unidic_artifact_binary_payload(args.collect()),
        "unidic-artifact-archive" => run_unidic_artifact_archive(args.collect()),
        "unidic-artifact-bundle" => run_unidic_artifact_bundle(args.collect()),
        "unidic-artifact-metadata" => run_unidic_artifact_metadata(args.collect()),
        "unidic-artifact-inspect" => run_unidic_artifact_inspect(args.collect()),
        "unidic-artifact-payload" => run_unidic_artifact_payload(args.collect()),
        "unidic-artifact-release-checksums" => {
            run_unidic_artifact_release_checksums(args.collect())
        }
        "unidic-artifact-runtime-measure" => run_unidic_artifact_runtime_measure(args.collect()),
        "unidic-artifact-verify" => run_unidic_artifact_verify(args.collect()),
        "unidic-csv-readings" => run_unidic_csv_readings(args.collect()),
        "unidic-csv-sequences" => run_unidic_csv_sequences(args.collect()),
        "unidic-readings" => run_unidic_readings(args.collect()),
        "-h" | "--help" | "help" => {
            print_usage();
            Ok(())
        }
        _ => Err(Box::new(CliError::UnknownCommand(command))),
    }
}

fn run_download(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = DownloadCliOptions::parse(args)?;
    let cache_dir = options
        .cache_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_dir);
    let archive_url = options.url.as_deref().unwrap_or(options.spec.archive_url);
    let archive_name = uri_file_name(archive_url).unwrap_or(options.spec.archive_name);
    let temp = TempDir::new("moine-download")?;
    let archive_path = temp.path().join(archive_name);

    copy_uri_to_path(archive_url, &archive_path)?;
    if let Some(expected_sha256) = download_expected_sha256(&options, archive_name)? {
        let actual_sha256 = sha256_file(&archive_path)?;
        if actual_sha256 != expected_sha256 {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "checksum mismatch for {archive_name}: expected {expected_sha256}, got {actual_sha256}"
            ))));
        }
    }

    let extracted_root = extract_artifact_archive(&archive_path, &temp.path().join("extract"))?;
    let metadata = extracted_root.join("metadata.yaml");
    if !metadata.is_file() {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "downloaded artifact has no metadata.yaml: {}",
            extracted_root.display()
        ))));
    }
    verify_downloaded_bundle(options.spec.language, &metadata)?;

    fs::create_dir_all(&cache_dir)?;
    let destination = cache_dir.join(extracted_root.file_name().ok_or_else(|| {
        CliError::ArtifactVerificationFailed(format!(
            "extracted artifact root {} has no file name",
            extracted_root.display()
        ))
    })?);
    if destination.exists() {
        if !options.force {
            println!("{}", destination.display());
            return Ok(());
        }
        fs::remove_dir_all(&destination)?;
    }
    move_dir(&extracted_root, &destination)?;
    println!("{}", destination.display());
    Ok(())
}

fn run_download_list(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = CacheCliOptions::parse(args)?;
    let cache_dir = options
        .cache_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_dir);
    for metadata in installed_metadata_paths(&cache_dir)? {
        if let Some(parent) = metadata.parent() {
            println!("{}", parent.display());
        }
    }
    Ok(())
}

fn run_download_where(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = WhereCliOptions::parse(args)?;
    let cache_dir = options
        .cache_dir
        .map(PathBuf::from)
        .unwrap_or_else(default_cache_dir);
    let Some(language) = options.language else {
        println!("{}", cache_dir.display());
        return Ok(());
    };
    let spec = download_spec_for_language(language);
    if let Some(metadata) = find_metadata_by_prefix(&cache_dir, spec.artifact_name)? {
        if let Some(parent) = metadata.parent() {
            println!("{}", parent.display());
            return Ok(());
        }
    }
    println!("{}", cache_dir.join(spec.artifact_name).display());
    Ok(())
}

fn run_unidic_artifact_archive(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactArchiveCliOptions::parse(args)?;
    let verified =
        verify_unidic_artifact_bundle(&options.metadata, options.bundle_dir.as_deref(), false)?;
    let root_name = options
        .root_name
        .unwrap_or_else(|| verified.metadata.artifact_name.clone());
    let entries = release_archive_entries(&verified)?;
    let output = create_output_file(&options.output)?;
    write_release_archive(output, options.compression, &root_name, &entries)?;

    println!("archive: {}", options.output);
    println!("compression: {}", options.compression.as_str());
    println!("root: {}", root_name);
    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("entries: {}", entries.len());
    println!("verified: true");

    Ok(())
}

fn run_cedict_readings(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = CedictReadingsOptions::parse(args)?;
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

fn run_cedict_sequences(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = CedictSequencesOptions::parse(args)?;
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

fn run_chinese_compare(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ChineseCompareOptions::parse(args)?;
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

fn run_zh_artifact_metadata(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ZhArtifactMetadataCliOptions::parse(args)?;
    let index =
        ZhReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;
    let metadata = index.artifact_metadata(ZhArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name: options.payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_cedict: options.cedict.clone(),
        index_options: options.index_options,
        query_defaults: options.reading_options,
        license: ZhArtifactLicense::default(),
    });
    let yaml = serde_yaml::to_string(&metadata)?;

    if let Some(output) = &options.output {
        write_output_file(output, yaml)?;
    } else {
        print!("{yaml}");
    }

    Ok(())
}

fn run_zh_artifact_payload(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ZhArtifactPayloadCliOptions::parse(args)?;
    let index =
        ZhReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;

    if let Some(output) = &options.output {
        write_zh_artifact_payload_file(&index, options.payload_format, Path::new(output))?;
    } else {
        if options.payload_format != ArtifactPayloadFormat::Yaml {
            return Err(Box::new(CliError::ArtifactVerificationFailed(
                "non-YAML zh payload output requires --output".to_string(),
            )));
        }
        print!("{}", serde_yaml::to_string(&index.artifact_payload())?);
    }

    Ok(())
}

fn run_zh_artifact_archive(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ZhArtifactArchiveCliOptions::parse(args)?;
    let verified = verify_zh_artifact_bundle(&options.metadata, options.bundle_dir.as_deref())?;
    let root_name = options
        .root_name
        .unwrap_or_else(|| verified.metadata.artifact_name.clone());
    let entries = zh_release_archive_entries(&verified)?;
    let output = create_output_file(&options.output)?;
    write_release_archive(output, options.compression, &root_name, &entries)?;

    println!("archive: {}", options.output);
    println!("compression: {}", options.compression.as_str());
    println!("root: {}", root_name);
    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("entries: {}", entries.len());
    println!("verified: true");

    Ok(())
}

fn run_zh_artifact_inspect(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ZhArtifactInspectCliOptions::parse(args)?;
    let index = load_zh_artifact_payload_by_format(
        Path::new(&options.payload),
        options.payload_format.as_str(),
    )?;

    println!("payload: {}", options.payload);
    println!("format: {}", options.payload_format.as_str());
    println!("pinyin_view: {}", index.pinyin_view().as_str());
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ZH_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

fn run_zh_artifact_bundle(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ZhArtifactBundleCliOptions::parse(args)?;
    let index =
        ZhReadingIndex::from_cedict_path_with_options(&options.cedict, options.index_options)?;
    let output_dir = PathBuf::from(&options.output_dir);
    let payload_file_name =
        default_zh_payload_file_name(&options.artifact_name, options.payload_format);
    let payload_path = output_dir.join(&payload_file_name);
    let metadata_path = output_dir.join("metadata.yaml");
    let license_output_dir = output_dir.join("license");

    fs::create_dir_all(&license_output_dir)?;
    write_zh_artifact_payload_file(&index, options.payload_format, &payload_path)?;
    let file_digest = zh_artifact_file_digest_path(&payload_path)?;

    let mut metadata = index.artifact_metadata(ZhArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_cedict: options.cedict.clone(),
        index_options: options.index_options,
        query_defaults: options.reading_options,
        license: ZhArtifactLicense::default(),
    });
    metadata.payload.file_digest_algorithm =
        Some(ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM.to_string());
    metadata.payload.file_digest = Some(file_digest.clone());
    fs::write(&metadata_path, serde_yaml::to_string(&metadata)?)?;

    write_zh_license_reference(options.license_file.as_deref(), &license_output_dir)?;

    println!("bundle: {}", output_dir.display());
    println!("metadata: {}", metadata_path.display());
    println!("payload: {}", payload_path.display());
    println!("payload_format: {}", options.payload_format.as_str());
    println!("pinyin_view: {}", index.pinyin_view().as_str());
    println!("entries: {}", index.len());
    println!("file_digest_algorithm: {ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
    println!("file_digest: {file_digest}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

fn run_zh_artifact_verify(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = ZhArtifactVerifyCliOptions::parse(args)?;
    let verified = verify_zh_artifact_bundle(&options.metadata, options.bundle_dir.as_deref())?;

    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("payload: {}", verified.payload_path.display());
    println!("pinyin_view: {}", verified.index.pinyin_view().as_str());
    println!("entries: {}", verified.index.len());
    if let Some(file_digest) = &verified.file_digest {
        println!("file_digest_algorithm: {ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
        println!("file_digest: {file_digest}");
    } else {
        println!("file_digest: skipped");
    }
    println!(
        "checksum_algorithm: {}",
        verified.metadata.payload.checksum_algorithm
    );
    println!("checksum: {}", verified.checksum);
    println!(
        "license_references: {}",
        verified.metadata.license.references.len()
    );
    println!("verified: true");

    Ok(())
}

fn run_japanese_report(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = JapaneseReportOptions::parse(args)?;
    let override_yaml = fs::read_to_string(&options.overrides)?;
    let overrides = OverrideDictionary::from_yaml_str(&override_yaml)?;
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let report = render_japanese_report(&overrides, &index, &options)?;

    if let Some(output) = &options.output {
        write_output_file(output, report)?;
    } else {
        print!("{report}");
    }

    Ok(())
}

fn run_jwtd_summary(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = JwtdSummaryOptions::parse(args)?;
    let mut summaries = Vec::with_capacity(options.splits.len());
    for split in &options.splits {
        summaries.push(summarize_jwtd_split(split)?);
    }
    let report = render_jwtd_summary_report(&summaries);

    if let Some(output) = &options.output {
        write_output_file(output, report)?;
    } else {
        print!("{report}");
    }

    Ok(())
}

fn run_jwtd_scorer_report(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = JwtdScorerReportOptions::parse(args)?;
    let report = build_jwtd_scorer_report(&options)?;

    if let Some(output) = &options.output {
        write_output_file(output, report)?;
    } else {
        print!("{report}");
    }

    Ok(())
}

fn run_unidic_artifact_metadata(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactMetadataCliOptions::parse(args)?;
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let metadata = index.artifact_metadata(UnidicArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name: options.payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_lex_csv: options.lex_csv.clone(),
        index_options: options.index_options,
        query_defaults: options.dictionary_options,
        license: UnidicArtifactLicense::default(),
    });
    let yaml = serde_yaml::to_string(&metadata)?;

    if let Some(output) = &options.output {
        write_output_file(output, yaml)?;
    } else {
        print!("{yaml}");
    }

    Ok(())
}

fn run_unidic_artifact_bundle(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactBundleCliOptions::parse(args)?;
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let output_dir = PathBuf::from(&options.output_dir);
    let payload_file_name =
        default_unidic_payload_file_name(&options.artifact_name, options.payload_format);
    let payload_path = output_dir.join(&payload_file_name);
    let metadata_path = output_dir.join("metadata.yaml");
    let license_output_dir = output_dir.join("license");

    fs::create_dir_all(&license_output_dir)?;
    write_artifact_payload_file(&index, options.payload_format, &payload_path)?;
    let file_digest = artifact_file_digest_path(&payload_path)?;

    let mut metadata = index.artifact_metadata(UnidicArtifactMetadataOptions {
        artifact_name: options.artifact_name,
        generator: "moine-cli".to_string(),
        payload_file_name,
        payload_format: options.payload_format.as_str().to_string(),
        source_name: options.source_name,
        source_version: options.source_version,
        source_lex_csv: options.lex_csv.clone(),
        index_options: options.index_options,
        query_defaults: options.dictionary_options,
        license: UnidicArtifactLicense::default(),
    });
    metadata.payload.file_digest_algorithm =
        Some(ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM.to_string());
    metadata.payload.file_digest = Some(file_digest.clone());
    fs::write(&metadata_path, serde_yaml::to_string(&metadata)?)?;

    let license_dir = options
        .license_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| default_unidic_license_dir(&options.lex_csv));
    copy_unidic_license_file(&license_dir, &license_output_dir, "BSD")?;
    copy_unidic_license_file(&license_dir, &license_output_dir, "COPYING")?;

    println!("bundle: {}", output_dir.display());
    println!("metadata: {}", metadata_path.display());
    println!("payload: {}", payload_path.display());
    println!("payload_format: {}", options.payload_format.as_str());
    println!("entries: {}", index.len());
    println!("file_digest_algorithm: {ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
    println!("file_digest: {file_digest}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

fn run_unidic_artifact_verify(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactVerifyCliOptions::parse(args)?;
    let verified = verify_unidic_artifact_bundle(
        &options.metadata,
        options.bundle_dir.as_deref(),
        options.canonical_checksum,
    )?;

    println!("metadata: {}", verified.metadata_path.display());
    println!("bundle: {}", verified.bundle_dir.display());
    println!("payload: {}", verified.payload_path.display());
    println!("payload_format: {}", verified.metadata.payload.format);
    println!("entries: {}", verified.entries);
    if verified.used_binary_header {
        println!("entry_count_source: binary_header");
    } else {
        println!("entry_count_source: decoded_payload");
    }
    if let Some(file_digest) = &verified.file_digest {
        println!("file_digest_algorithm: {ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM}");
        println!("file_digest: {file_digest}");
    }
    if let Some(checksum) = &verified.checksum {
        println!(
            "checksum_algorithm: {}",
            verified.metadata.payload.checksum_algorithm
        );
        println!("checksum: {checksum}");
    } else {
        println!("canonical_checksum: skipped");
    }
    println!(
        "license_references: {}",
        verified.metadata.license.references.len()
    );
    println!("verified: true");

    Ok(())
}

fn run_unidic_artifact_release_checksums(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactReleaseChecksumsCliOptions::parse(args)?;
    let mut output = String::new();

    for asset in &options.assets {
        let path = Path::new(asset);
        let digest = artifact_file_digest_path(path)?;
        let label = release_checksum_asset_label(path)?;
        writeln!(output, "{digest}  {label}")?;
    }

    if let Some(path) = &options.output {
        write_output_file(path, output)?;
    } else {
        print!("{output}");
    }

    Ok(())
}

fn run_unidic_artifact_runtime_measure(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactRuntimeMeasureCliOptions::parse(args)?;
    let loaded =
        load_unidic_artifact_bundle_for_runtime(&options.metadata, options.bundle_dir.as_deref())?;
    let dictionary_options = dictionary_options_from_metadata(&loaded.metadata);

    let first_start = Instant::now();
    let first_distances = compare_with_unidic_index(
        &options.pairs[0].left,
        &options.pairs[0].right,
        &loaded.index,
        dictionary_options,
    )?;
    let first_compare = first_start.elapsed();

    let mut result_checksum = first_distances
        .surface_levenshtein
        .wrapping_add(first_distances.surface_damerau)
        .wrapping_add(first_distances.lattice)
        .wrapping_add(first_distances.lattice_damerau)
        .wrapping_add(first_distances.combined);
    for _ in 0..options.warmups {
        for pair in &options.pairs {
            let distances = compare_with_unidic_index(
                &pair.left,
                &pair.right,
                &loaded.index,
                dictionary_options,
            )?;
            result_checksum = result_checksum.wrapping_add(distances.combined);
        }
    }

    let mut compare_total = Duration::ZERO;
    let mut compare_min: Option<Duration> = None;
    let mut compare_max: Option<Duration> = None;
    for _ in 0..options.iterations {
        for pair in &options.pairs {
            let compare_start = Instant::now();
            let distances = compare_with_unidic_index(
                &pair.left,
                &pair.right,
                &loaded.index,
                dictionary_options,
            )?;
            let elapsed = compare_start.elapsed();
            compare_total += elapsed;
            compare_min = Some(compare_min.map_or(elapsed, |value| value.min(elapsed)));
            compare_max = Some(compare_max.map_or(elapsed, |value| value.max(elapsed)));
            result_checksum = result_checksum
                .wrapping_add(distances.surface_levenshtein)
                .wrapping_add(distances.surface_damerau)
                .wrapping_add(distances.lattice)
                .wrapping_add(distances.lattice_damerau)
                .wrapping_add(distances.combined);
        }
    }
    let measured_comparisons = options.iterations * options.pairs.len();
    let compare_avg = compare_total.as_secs_f64() * 1000.0 / measured_comparisons as f64;

    println!("metadata: {}", loaded.metadata_path.display());
    println!("bundle: {}", loaded.bundle_dir.display());
    println!("payload: {}", loaded.payload_path.display());
    println!("payload_format: {}", loaded.metadata.payload.format);
    println!("entries: {}", loaded.index.len());
    println!("file_digest_verified: {}", loaded.file_digest_verified);
    println!("query_defaults:");
    println!("  max_span_chars: {}", dictionary_options.max_span_chars);
    println!("  max_paths: {}", dictionary_options.max_paths);
    println!(
        "  longest_match_only: {}",
        dictionary_options.longest_match_only
    );
    println!(
        "  max_readings_per_segment: {}",
        max_readings_per_segment_label(dictionary_options.max_readings_per_segment)
    );
    println!(
        "timing_read_metadata_ms: {:.3}",
        duration_ms(loaded.timing.read_metadata)
    );
    println!(
        "timing_file_digest_ms: {:.3}",
        duration_ms(loaded.timing.file_digest)
    );
    println!(
        "timing_decode_payload_ms: {:.3}",
        duration_ms(loaded.timing.decode_payload)
    );
    if let Some(canonical_checksum) = loaded.timing.canonical_checksum {
        println!(
            "timing_canonical_checksum_ms: {:.3}",
            duration_ms(canonical_checksum)
        );
    }
    println!(
        "timing_load_total_ms: {:.3}",
        duration_ms(loaded.timing.total)
    );
    println!("pair_count: {}", options.pairs.len());
    println!("warmup_iterations: {}", options.warmups);
    println!("measure_iterations: {}", options.iterations);
    println!("measured_comparisons: {measured_comparisons}");
    println!("first_compare_ms: {:.3}", duration_ms(first_compare));
    println!("compare_total_ms: {:.3}", duration_ms(compare_total));
    println!("compare_avg_ms: {compare_avg:.3}");
    println!(
        "compare_min_ms: {:.3}",
        duration_ms(compare_min.expect("iterations should be non-zero"))
    );
    println!(
        "compare_max_ms: {:.3}",
        duration_ms(compare_max.expect("iterations should be non-zero"))
    );
    println!("result_checksum: {result_checksum}");
    println!("first_pair:");
    println!("  left: {}", options.pairs[0].left);
    println!("  right: {}", options.pairs[0].right);
    println!(
        "  surface_levenshtein: {}",
        first_distances.surface_levenshtein
    );
    println!("  surface_damerau: {}", first_distances.surface_damerau);
    println!("  lattice: {}", first_distances.lattice);
    println!("  lattice_damerau: {}", first_distances.lattice_damerau);
    println!("  combined: {}", first_distances.combined);

    Ok(())
}

fn run_unidic_artifact_binary_payload(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactBinaryPayloadCliOptions::parse(args)?;
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let output = create_output_file(&options.output)?;
    index.write_artifact_binary_payload(output)?;

    println!("payload: {}", options.output);
    println!("format: binary.surface-readings.v1");
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

fn run_unidic_artifact_binary_inspect(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactBinaryInspectCliOptions::parse(args)?;
    let (index, timing) = if options.timing {
        let read_start = Instant::now();
        let bytes = fs::read(&options.payload)?;
        let read_file = read_start.elapsed();

        let decode_start = Instant::now();
        let index = UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice())?;
        let decode_binary = decode_start.elapsed();

        (
            index,
            Some(BinaryInspectTiming {
                read_file,
                decode_binary,
            }),
        )
    } else {
        (
            UnidicReadingIndex::from_binary_artifact_payload_path(&options.payload)?,
            None,
        )
    };

    let checksum_start = Instant::now();
    let checksum = index.artifact_payload_checksum();
    let checksum_elapsed = checksum_start.elapsed();

    println!("payload: {}", options.payload);
    println!("format: binary.surface-readings.v1");
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {checksum}");
    if let Some(timing) = timing {
        println!("timing_read_file_ms: {:.3}", duration_ms(timing.read_file));
        println!(
            "timing_decode_binary_ms: {:.3}",
            duration_ms(timing.decode_binary)
        );
        println!("timing_checksum_ms: {:.3}", duration_ms(checksum_elapsed));
    }

    Ok(())
}

fn run_unidic_artifact_inspect(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactInspectCliOptions::parse(args)?;
    let index = UnidicReadingIndex::from_artifact_payload_path(&options.payload)?;

    println!("payload: {}", options.payload);
    println!("entries: {}", index.len());
    println!("checksum_algorithm: {ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM}");
    println!("checksum: {}", index.artifact_payload_checksum());

    Ok(())
}

fn run_unidic_artifact_payload(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicArtifactPayloadCliOptions::parse(args)?;
    let index = UnidicReadingIndex::from_lex_csv_path_with_options(
        &options.lex_csv,
        options.index_options,
    )?;
    let yaml = serde_yaml::to_string(&index.artifact_payload())?;

    if let Some(output) = &options.output {
        write_output_file(output, yaml)?;
    } else {
        print!("{yaml}");
    }

    Ok(())
}

fn run_unidic_csv_sequences(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicCsvSequencesOptions::parse(args)?;
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

fn run_unidic_csv_readings(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicCsvReadingsOptions::parse(args)?;
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

fn run_unidic_readings(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = UnidicReadingsOptions::parse(args)?;
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

fn render_japanese_report(
    overrides: &OverrideDictionary,
    index: &UnidicReadingIndex,
    options: &JapaneseReportOptions,
) -> Result<String, Box<dyn Error>> {
    let mut rows = Vec::with_capacity(JAPANESE_REPORT_PAIRS.len());

    for pair in JAPANESE_REPORT_PAIRS {
        let override_distances = compare_with_overrides(pair.left, pair.right, overrides)?;
        let override_left = overrides.romaji_lattice(pair.left)?;
        let override_right = overrides.romaji_lattice(pair.right)?;
        let override_trace = distance_with_trace(&override_left, &override_right);

        let dict_distances =
            compare_with_unidic_index(pair.left, pair.right, index, options.dictionary_options)?;
        let dict_left = unidic_or_direct_lattice(pair.left, index, options.dictionary_options)?;
        let dict_right = unidic_or_direct_lattice(pair.right, index, options.dictionary_options)?;
        let dict_trace = distance_with_trace(&dict_left, &dict_right);

        rows.push(JapaneseReportRow {
            pair,
            override_distances,
            dict_distances,
            override_best_path: best_path(&override_trace),
            dict_best_path: best_path(&dict_trace),
        });
    }

    let mut report = String::new();
    writeln!(report, "# Japanese Functional Validation")?;
    writeln!(report)?;
    writeln!(report, "This phase is not a reproduction of the original paper's private query-log experiment. It is a functional validation of the language-independent LPED core and the Japanese romaji lattice adapter using small examples inspired by the paper.")?;
    writeln!(report)?;
    writeln!(report, "The current validation uses:")?;
    writeln!(report)?;
    writeln!(report, "- kana / katakana normalization")?;
    writeln!(report, "- kana-to-romaji variants")?;
    writeln!(report, "- ASCII identity paths")?;
    writeln!(
        report,
        "- manual override readings from `{}`",
        options.overrides
    )?;
    writeln!(
        report,
        "- UniDic CWJ 2025.12 full `lex.csv` as a diagnostic dictionary lattice"
    )?;
    writeln!(report, "- surface Levenshtein")?;
    writeln!(report, "- surface Damerau-Levenshtein")?;
    writeln!(report, "- combined distance: `min(surface Damerau, LPED)`")?;
    writeln!(report)?;
    writeln!(report, "Manual overrides remain the stable golden-test layer. The UniDic CSV dictionary path is reported separately so we can see what the real dictionary can reproduce without making the golden tests depend on dictionary noise.")?;
    writeln!(report)?;
    writeln!(report, "## Command")?;
    writeln!(report)?;
    writeln!(report, "```bash")?;
    writeln!(report, "cargo run -p moine-cli -- japanese-report \\")?;
    writeln!(report, "  --overrides {} \\", options.overrides)?;
    writeln!(report, "  --lex-csv {} \\", options.lex_csv)?;
    if let Some(max_readings) = options.index_options.max_readings_per_surface {
        writeln!(report, "  --max-readings-per-surface {} \\", max_readings)?;
    }
    if let Some(max_readings) = options.dictionary_options.max_readings_per_segment {
        writeln!(report, "  --max-readings-per-segment {} \\", max_readings)?;
    }
    if !options.index_options.exclude_ascii_surfaces {
        writeln!(report, "  --include-ascii-surfaces \\")?;
    }
    if !options.index_options.exclude_symbol_pos {
        writeln!(report, "  --include-symbol-pos \\")?;
    }
    writeln!(
        report,
        "  --max-span-chars {} \\",
        options.dictionary_options.max_span_chars
    )?;
    writeln!(
        report,
        "  --max-paths {} \\",
        options.dictionary_options.max_paths
    )?;
    writeln!(report, "  --longest-only \\")?;
    writeln!(report, "  --output reports/japanese_validation.md")?;
    writeln!(report, "```")?;
    writeln!(report)?;
    writeln!(report, "Dictionary settings:")?;
    writeln!(report)?;
    writeln!(
        report,
        "- UniDic field: `{}`",
        unidic_reading_field_name(options.index_options.reading_field)
    )?;
    writeln!(
        report,
        "- Max readings per surface: `{}`",
        max_readings_per_surface_label(options.index_options.max_readings_per_surface)
    )?;
    writeln!(
        report,
        "- Max readings per segment: `{}`",
        max_readings_per_segment_label(options.dictionary_options.max_readings_per_segment)
    )?;
    writeln!(
        report,
        "- Exclude ASCII surfaces: `{}`",
        options.index_options.exclude_ascii_surfaces
    )?;
    writeln!(
        report,
        "- Exclude symbol POS: `{}`",
        options.index_options.exclude_symbol_pos
    )?;
    writeln!(
        report,
        "- Longest-match segmentation only: `{}`",
        options.dictionary_options.longest_match_only
    )?;
    writeln!(
        report,
        "- Max span chars: `{}`",
        options.dictionary_options.max_span_chars
    )?;
    writeln!(
        report,
        "- Max paths: `{}`",
        options.dictionary_options.max_paths
    )?;
    writeln!(report)?;
    writeln!(report, "## Results")?;
    writeln!(report)?;
    writeln!(report, "| Pair | Surface Lev | Surface Dam | Override Lattice | Dict Lattice | Combined | Override best path | Dict best path |")?;
    writeln!(report, "|---|---:|---:|---:|---:|---:|---|---|")?;
    for row in &rows {
        writeln!(
            report,
            "| `{}` / `{}` | {} | {} | {} | {} | {} | `{}` / `{}` | `{}` / `{}` |",
            row.pair.left,
            row.pair.right,
            row.override_distances.surface_levenshtein,
            row.override_distances.surface_damerau,
            row.override_distances.lattice,
            row.dict_distances.lattice,
            row.override_distances
                .combined
                .min(row.dict_distances.combined),
            row.override_best_path.left,
            row.override_best_path.right,
            row.dict_best_path.left,
            row.dict_best_path.right,
        )?;
    }
    writeln!(report)?;
    writeln!(report, "## Interpretation")?;
    writeln!(report)?;
    writeln!(report, "The first four examples validate the main LPED behavior: strings that are far on the surface can be identical or close in romaji lattice space.")?;
    writeln!(report)?;
    writeln!(report, "The UniDic CSV path currently reproduces the same distances for these examples with `--longest-only`. In particular, the full CSV dictionary contains readings such as `刃 -> ヤイバ`, so `鬼滅の刃` can reach `キメツノヤイバ` without a manual override. This is useful evidence that the dictionary path is viable, but it is still noisier than the override fixture.")?;
    writeln!(report)?;
    writeln!(report, "`愛知家コロナ` / `愛知県コロナ` is close in both surface and lattice space. This is still a useful smoke test for override readings, but it is not evidence that LPED beats surface distance.")?;
    writeln!(report)?;
    writeln!(report, "`マトリッツォ` / `マリトッツォ` validates the complementary role of Damerau-Levenshtein. The adjacent transposition is cheap on the surface, and the core now also exposes an explicit lattice-side Damerau-Levenshtein helper. The default validation table still reports Levenshtein-style LPED for continuity with the earlier Phase 0 report.")?;
    writeln!(report)?;
    writeln!(report, "## Limitations")?;
    writeln!(report)?;
    writeln!(report, "- Override readings are hand-written fixtures. They validate distance behavior, not dictionary quality.")?;
    writeln!(report, "- UniDic CSV dictionary matching intentionally does not use MeCab costs. `--longest-only`, ASCII/symbol filtering, `max_readings_per_surface`, and `max_readings_per_segment` are bounded candidate-expansion controls, not ranking features.")?;
    writeln!(report, "- Mixed kanji/kana/ASCII inputs use hybrid fallback: dictionary spans cover known kanji while directly convertible kana/ASCII spans remain identity or kana-romaji paths. Unknown kanji are still not guessed.")?;
    writeln!(report, "- The current romaji lattice now compacts generated romaji candidates by sharing common prefixes and equivalent suffix subgraphs. Candidate generation itself still happens before this compaction step, so very large dictionaries will need earlier pruning or a more direct builder.")?;
    writeln!(
        report,
        "- Long vowel and sokuon handling are intentionally simple:"
    )?;
    writeln!(
        report,
        "  - `っ` adds next-consonant gemination candidates."
    )?;
    writeln!(
        report,
        "  - `ー` adds vowel-lengthening candidates from the previous romaji path."
    )?;
    writeln!(report, "- Lattice-side Damerau-Levenshtein is available as an explicit core helper; the default `distance(...)` API still uses Levenshtein-style LPED.")?;
    writeln!(report)?;
    writeln!(report, "## Next Step")?;
    writeln!(report)?;
    writeln!(report, "The Phase 0 core behavior is now reproducible through this report command. The dictionary path exposes segment-level reading paths, applies query-time reading limits before romaji expansion, reports expansion statistics in the diagnostic CLI, builds romaji lattices from structured paths, supports hybrid dictionary/direct fallback for mixed inputs, and compacts the generated lattice. The persistent artifact work now includes metadata, deterministic YAML and binary surface-reading payloads, SHA-256 canonical checksums with legacy FNV verification, SHA-256 file digests for fast bundle verification, binary-header entry-count verification, deterministic release tar/gzip packaging, Python bundle loading, runtime load/use measurement, release checksum manifest emission, and a checked UniDic-CWJ release recipe. The current eager binary payload is sufficient for a first prebuilt artifact on the measured validation workload; the next implementation step is choosing the actual release/publishing policy that requires maintainer input.")?;

    Ok(report)
}

fn summarize_jwtd_split(input: &JwtdSplitInput) -> Result<JwtdSplitSummary, Box<dyn Error>> {
    let file = fs::File::open(&input.path)?;
    let reader = std::io::BufReader::new(file);
    let mut summary = JwtdSplitSummary {
        name: input.name.clone(),
        path: input.path.clone(),
        ..JwtdSplitSummary::default()
    };

    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let record =
            serde_json::from_str::<JwtdRecord>(&line).map_err(|err| CliError::JwtdJsonLine {
                path: input.path.clone(),
                line: line_index + 1,
                message: err.to_string(),
            })?;
        summary.records += 1;
        if !record.diffs.is_empty() {
            summary.records_with_diffs += 1;
        }

        for diff in record.diffs {
            summary.add_diff(&diff);
        }
    }

    Ok(summary)
}

impl JwtdSplitSummary {
    fn add_diff(&mut self, diff: &JwtdDiff) {
        self.diffs += 1;
        *self
            .category_counts
            .entry(diff.category.clone())
            .or_default() += 1;

        let pre_empty = diff.pre_str.is_empty();
        let post_empty = diff.post_str.is_empty();
        if pre_empty {
            self.empty_pre += 1;
        }
        if post_empty {
            self.empty_post += 1;
        }
        if pre_empty && post_empty {
            self.empty_both += 1;
        }
        if pre_empty || post_empty {
            return;
        }

        self.nonempty_pairs += 1;
        *self
            .nonempty_category_counts
            .entry(diff.category.clone())
            .or_default() += 1;

        let pre_len = diff.pre_str.chars().count();
        let post_len = diff.post_str.chars().count();
        self.pre_len.add(pre_len);
        self.post_len.add(post_len);
        *self
            .pair_length_buckets
            .entry(pair_length_bucket(pre_len.max(post_len)))
            .or_default() += 1;
    }
}

impl LengthStats {
    fn add(&mut self, value: usize) {
        self.count += 1;
        self.sum += value;
        self.max = self.max.max(value);
    }

    fn mean(self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        self.sum as f64 / self.count as f64
    }
}

fn pair_length_bucket(length: usize) -> &'static str {
    match length {
        0 => "0",
        1 => "1",
        2 => "2",
        3 | 4 => "3-4",
        5..=8 => "5-8",
        9..=16 => "9-16",
        17..=32 => "17-32",
        _ => "33+",
    }
}

fn build_jwtd_scorer_report(options: &JwtdScorerReportOptions) -> Result<String, Box<dyn Error>> {
    let (mut examples, candidate_pool) = load_jwtd_examples(&options.split)?;
    if let Some(max_examples) = options.max_examples {
        examples.truncate(max_examples);
    }
    let lped_bundle = if let Some(metadata) = &options.artifact_metadata {
        Some(load_unidic_artifact_bundle_for_runtime(
            metadata,
            options.bundle_dir.as_deref(),
        )?)
    } else {
        None
    };
    let lped_context = lped_bundle.as_ref().map(|bundle| JwtdLpedContext {
        index: &bundle.index,
        options: dictionary_options_from_metadata(&bundle.metadata),
    });

    let mut metrics = BTreeMap::<(String, &'static str), JwtdMetricAccumulator>::new();

    for (example_index, example) in examples.iter().enumerate() {
        let candidates = jwtd_candidates_for_example(
            example,
            &candidate_pool,
            options.negative_policy,
            options.negative_count,
            options.seed,
            example_index,
        );
        let surface_levenshtein_scores = candidates
            .iter()
            .map(|candidate| (levenshtein_str(&example.query, candidate), candidate))
            .collect::<Vec<_>>();
        push_jwtd_rank(
            &mut metrics,
            &example.category,
            JwtdScorer::SurfaceLevenshtein,
            jwtd_gold_rank_from_scores(
                &example.gold,
                &surface_levenshtein_scores,
                options.tie_policy,
            ),
        );

        let surface_damerau_scores = candidates
            .iter()
            .map(|candidate| {
                (
                    damerau_levenshtein_str(&example.query, candidate),
                    candidate,
                )
            })
            .collect::<Vec<_>>();
        push_jwtd_rank(
            &mut metrics,
            &example.category,
            JwtdScorer::SurfaceDamerau,
            jwtd_gold_rank_from_scores(&example.gold, &surface_damerau_scores, options.tie_policy),
        );

        if let Some(context) = lped_context.as_ref() {
            let lped_scores = candidates
                .iter()
                .map(|candidate| {
                    (
                        jwtd_lped_score(&example.query, candidate, context),
                        candidate,
                    )
                })
                .collect::<Vec<_>>();
            push_jwtd_rank(
                &mut metrics,
                &example.category,
                JwtdScorer::Lped,
                jwtd_gold_rank_from_scores(&example.gold, &lped_scores, options.tie_policy),
            );

            let combined_scores = surface_damerau_scores
                .iter()
                .zip(lped_scores.iter())
                .map(
                    |((surface_score, surface_candidate), (lped_score, lped_candidate))| {
                        debug_assert_eq!(surface_candidate, lped_candidate);
                        ((*surface_score).min(*lped_score), *surface_candidate)
                    },
                )
                .collect::<Vec<_>>();
            push_jwtd_rank(
                &mut metrics,
                &example.category,
                JwtdScorer::CombinedSurfaceDamerauLped,
                jwtd_gold_rank_from_scores(&example.gold, &combined_scores, options.tie_policy),
            );
        }
    }

    Ok(render_jwtd_scorer_report(
        options,
        examples.len(),
        candidate_pool.len(),
        lped_bundle.as_ref(),
        &metrics,
    ))
}

fn load_jwtd_examples(
    input: &JwtdSplitInput,
) -> Result<(Vec<JwtdExample>, Vec<String>), Box<dyn Error>> {
    let file = fs::File::open(&input.path)?;
    let reader = std::io::BufReader::new(file);
    let mut examples = Vec::new();
    let mut candidate_pool = BTreeSet::new();

    for (line_index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let record =
            serde_json::from_str::<JwtdRecord>(&line).map_err(|err| CliError::JwtdJsonLine {
                path: input.path.clone(),
                line: line_index + 1,
                message: err.to_string(),
            })?;
        for diff in record.diffs {
            if diff.pre_str.is_empty() || diff.post_str.is_empty() {
                continue;
            }
            candidate_pool.insert(diff.post_str.clone());
            examples.push(JwtdExample {
                query: diff.pre_str,
                gold: diff.post_str,
                category: diff.category,
            });
        }
    }

    Ok((examples, candidate_pool.into_iter().collect()))
}

fn jwtd_candidates_for_example(
    example: &JwtdExample,
    candidate_pool: &[String],
    negative_policy: JwtdNegativePolicy,
    negative_count: usize,
    seed: u64,
    example_index: usize,
) -> Vec<String> {
    let mut candidates = Vec::with_capacity(negative_count + 1);
    candidates.push(example.gold.clone());
    let mut seen = BTreeSet::new();
    seen.insert(example.gold.clone());

    let negatives = match negative_policy {
        JwtdNegativePolicy::Length => {
            jwtd_length_negatives(example, candidate_pool, negative_count, seed, example_index)
        }
        JwtdNegativePolicy::SurfaceHard => jwtd_surface_hard_negatives(
            example,
            candidate_pool,
            negative_count,
            seed,
            example_index,
        ),
    };
    for negative in negatives {
        if seen.insert(negative.clone()) {
            candidates.push(negative);
        }
    }

    candidates
}

fn jwtd_length_negatives(
    example: &JwtdExample,
    candidate_pool: &[String],
    negative_count: usize,
    seed: u64,
    example_index: usize,
) -> Vec<String> {
    let gold_len = example.gold.chars().count();
    let mut ranked = candidate_pool
        .iter()
        .filter(|candidate| *candidate != &example.gold)
        .map(|candidate| {
            let len_diff = candidate.chars().count().abs_diff(gold_len);
            let tie_break = stable_candidate_hash(seed, example_index, &example.query, candidate);
            (len_diff, tie_break, candidate)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(right.2))
    });
    ranked
        .into_iter()
        .take(negative_count)
        .map(|(_, _, candidate)| candidate.clone())
        .collect()
}

fn jwtd_surface_hard_negatives(
    example: &JwtdExample,
    candidate_pool: &[String],
    negative_count: usize,
    seed: u64,
    example_index: usize,
) -> Vec<String> {
    let mut ranked = candidate_pool
        .iter()
        .filter(|candidate| *candidate != &example.gold)
        .map(|candidate| {
            let distance = damerau_levenshtein_str(&example.query, candidate);
            let len_diff = candidate
                .chars()
                .count()
                .abs_diff(example.gold.chars().count());
            let tie_break = stable_candidate_hash(seed, example_index, &example.query, candidate);
            (distance, len_diff, tie_break, candidate)
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(right.3))
    });
    ranked
        .into_iter()
        .take(negative_count)
        .map(|(_, _, _, candidate)| candidate.clone())
        .collect()
}

fn stable_candidate_hash(seed: u64, example_index: usize, query: &str, candidate: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64 ^ seed ^ example_index as u64;
    for byte in query.as_bytes().iter().chain(candidate.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
fn jwtd_gold_rank_with_context(
    example: &JwtdExample,
    candidates: &[String],
    scorer: JwtdScorer,
    tie_policy: JwtdTiePolicy,
    lped_context: Option<&JwtdLpedContext<'_>>,
) -> Result<usize, Box<dyn Error>> {
    let mut scored_candidates = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        scored_candidates.push((
            scorer.score(&example.query, candidate, lped_context)?,
            candidate,
        ));
    }
    Ok(jwtd_gold_rank_from_scores(
        &example.gold,
        &scored_candidates,
        tie_policy,
    ))
}

fn jwtd_gold_rank_from_scores(
    gold: &str,
    scored_candidates: &[(usize, &String)],
    tie_policy: JwtdTiePolicy,
) -> usize {
    let gold_score = scored_candidates
        .iter()
        .find(|(_, candidate)| candidate.as_str() == gold)
        .map(|(score, _)| *score)
        .expect("gold candidate should be present");
    match tie_policy {
        JwtdTiePolicy::Stable => {
            let mut ranked = scored_candidates.to_vec();
            ranked.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(right.1)));
            ranked
                .iter()
                .position(|(_, candidate)| candidate.as_str() == gold)
                .map(|index| index + 1)
                .expect("gold candidate should be present")
        }
        JwtdTiePolicy::Pessimistic => scored_candidates
            .iter()
            .filter(|(score, _)| *score <= gold_score)
            .count(),
    }
}

fn push_jwtd_rank(
    metrics: &mut BTreeMap<(String, &'static str), JwtdMetricAccumulator>,
    category: &str,
    scorer: JwtdScorer,
    rank: usize,
) {
    metrics
        .entry(("ALL".to_string(), scorer.as_str()))
        .or_default()
        .push(rank);
    metrics
        .entry((category.to_string(), scorer.as_str()))
        .or_default()
        .push(rank);
}

impl JwtdMetricAccumulator {
    fn push(&mut self, rank: usize) {
        self.ranks.push(rank);
    }

    fn n_examples(&self) -> usize {
        self.ranks.len()
    }

    fn recall_at(&self, k: usize) -> f64 {
        if self.ranks.is_empty() {
            return 0.0;
        }
        self.ranks.iter().filter(|&&rank| rank <= k).count() as f64 / self.ranks.len() as f64
    }

    fn mrr(&self) -> f64 {
        if self.ranks.is_empty() {
            return 0.0;
        }
        self.ranks
            .iter()
            .map(|&rank| 1.0 / rank as f64)
            .sum::<f64>()
            / self.ranks.len() as f64
    }

    fn mean_rank(&self) -> f64 {
        if self.ranks.is_empty() {
            return 0.0;
        }
        self.ranks.iter().sum::<usize>() as f64 / self.ranks.len() as f64
    }

    fn median_rank(&self) -> f64 {
        if self.ranks.is_empty() {
            return 0.0;
        }
        let mut ranks = self.ranks.clone();
        ranks.sort_unstable();
        let mid = ranks.len() / 2;
        if ranks.len() % 2 == 1 {
            ranks[mid] as f64
        } else {
            (ranks[mid - 1] + ranks[mid]) as f64 / 2.0
        }
    }
}

fn render_jwtd_scorer_report(
    options: &JwtdScorerReportOptions,
    example_count: usize,
    candidate_pool_size: usize,
    lped_bundle: Option<&RuntimeLoadedArtifactBundle>,
    metrics: &BTreeMap<(String, &'static str), JwtdMetricAccumulator>,
) -> String {
    let mut report = String::new();

    writeln!(report, "# JWTD Scorer Report").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "This is a scorer-only ranking benchmark over artificial candidate sets. It does not evaluate candidate generation, end-to-end typo correction, or the original LPED private query-log task.").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "## Metadata").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "dataset_name: Japanese Wikipedia Typo Dataset").expect("write to string");
    writeln!(report, "dataset_version: v2.0").expect("write to string");
    writeln!(report, "split: {}", options.split.name).expect("write to string");
    writeln!(report, "split_path: {}", options.split.path).expect("write to string");
    writeln!(
        report,
        "scorer_version: {}",
        if lped_bundle.is_some() {
            "moine-cli lped-v1"
        } else {
            "moine-cli surface-v1"
        }
    )
    .expect("write to string");
    if let Some(bundle) = lped_bundle {
        writeln!(
            report,
            "unidic_source_name: {}",
            bundle.metadata.source.name
        )
        .expect("write to string");
        writeln!(
            report,
            "unidic_source_version: {}",
            bundle.metadata.source.version
        )
        .expect("write to string");
        writeln!(
            report,
            "artifact_metadata: {}",
            bundle.metadata_path.display()
        )
        .expect("write to string");
        writeln!(
            report,
            "artifact_payload: {}",
            bundle.payload_path.display()
        )
        .expect("write to string");
        writeln!(report, "lped_unscorable_policy: max_distance").expect("write to string");
    } else {
        writeln!(report, "unidic_source_name: none").expect("write to string");
        writeln!(report, "unidic_source_version: none").expect("write to string");
        writeln!(report, "lped_unscorable_policy: none").expect("write to string");
    }
    writeln!(report, "normalization: raw_distance").expect("write to string");
    writeln!(
        report,
        "negative_policy: {}",
        options.negative_policy.as_str()
    )
    .expect("write to string");
    writeln!(report, "negative_count: {}", options.negative_count).expect("write to string");
    writeln!(report, "random_seed: {}", options.seed).expect("write to string");
    writeln!(report, "candidate_count: {}", options.negative_count + 1).expect("write to string");
    writeln!(report, "candidate_pool_size: {candidate_pool_size}").expect("write to string");
    writeln!(report, "tie_policy: {}", options.tie_policy.as_str()).expect("write to string");
    writeln!(
        report,
        "max_examples: {}",
        options
            .max_examples
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unbounded".to_string())
    )
    .expect("write to string");
    writeln!(report, "n_examples: {example_count}").expect("write to string");
    writeln!(report).expect("write to string");

    writeln!(report, "## Metrics").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "| Split | Category | Method | Normalization | Negative policy | Negative count | Tie policy | N examples | Recall@1 | Recall@5 | MRR | Mean gold rank | Median gold rank |").expect("write to string");
    writeln!(
        report,
        "|---|---|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|"
    )
    .expect("write to string");
    for ((category, method), accumulator) in metrics {
        writeln!(
            report,
            "| `{}` | `{}` | `{}` | `raw_distance` | `{}` | {} | `{}` | {} | {:.4} | {:.4} | {:.4} | {:.2} | {:.2} |",
            options.split.name,
            category,
            method,
            options.negative_policy.as_str(),
            options.negative_count,
            options.tie_policy.as_str(),
            accumulator.n_examples(),
            accumulator.recall_at(1),
            accumulator.recall_at(5),
            accumulator.mrr(),
            accumulator.mean_rank(),
            accumulator.median_rank(),
        )
        .expect("write to string");
    }

    report
}

fn render_jwtd_summary_report(summaries: &[JwtdSplitSummary]) -> String {
    let mut report = String::new();
    let total_records = summaries
        .iter()
        .map(|summary| summary.records)
        .sum::<usize>();
    let total_diffs = summaries.iter().map(|summary| summary.diffs).sum::<usize>();
    let total_nonempty = summaries
        .iter()
        .map(|summary| summary.nonempty_pairs)
        .sum::<usize>();

    writeln!(report, "# JWTD Summary").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "This report is a corpus-shape summary for a scorer-only JWTD benchmark. It does not evaluate end-to-end typo correction and does not reproduce the original LPED private query-log experiment.").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "total_records: {total_records}").expect("write to string");
    writeln!(report, "total_diffs: {total_diffs}").expect("write to string");
    writeln!(report, "total_nonempty_pairs: {total_nonempty}").expect("write to string");
    writeln!(report).expect("write to string");

    writeln!(report, "## Splits").expect("write to string");
    writeln!(report).expect("write to string");
    writeln!(report, "| Split | Records | Records with diffs | Diffs | Non-empty pairs | Empty pre | Empty post | Empty both | Mean pre chars | Mean post chars | Max pre chars | Max post chars |").expect("write to string");
    writeln!(
        report,
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
    )
    .expect("write to string");
    for summary in summaries {
        writeln!(
            report,
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {:.2} | {:.2} | {} | {} |",
            summary.name,
            summary.records,
            summary.records_with_diffs,
            summary.diffs,
            summary.nonempty_pairs,
            summary.empty_pre,
            summary.empty_post,
            summary.empty_both,
            summary.pre_len.mean(),
            summary.post_len.mean(),
            summary.pre_len.max,
            summary.post_len.max,
        )
        .expect("write to string");
    }
    writeln!(report).expect("write to string");

    for summary in summaries {
        writeln!(report, "## Split `{}`", summary.name).expect("write to string");
        writeln!(report).expect("write to string");
        writeln!(report, "path: `{}`", summary.path).expect("write to string");
        writeln!(report).expect("write to string");
        write_jwtd_category_table(&mut report, "Diff Categories", &summary.category_counts);
        write_jwtd_category_table(
            &mut report,
            "Non-Empty Pair Categories",
            &summary.nonempty_category_counts,
        );
        write_jwtd_length_bucket_table(&mut report, &summary.pair_length_buckets);
    }

    report
}

fn write_jwtd_category_table(report: &mut String, title: &str, counts: &BTreeMap<String, usize>) {
    writeln!(report, "### {title}").expect("write to string");
    writeln!(report).expect("write to string");
    if counts.is_empty() {
        writeln!(report, "_No entries._").expect("write to string");
        writeln!(report).expect("write to string");
        return;
    }

    writeln!(report, "| Category | Count |").expect("write to string");
    writeln!(report, "|---|---:|").expect("write to string");
    for (category, count) in counts {
        writeln!(report, "| `{category}` | {count} |").expect("write to string");
    }
    writeln!(report).expect("write to string");
}

fn write_jwtd_length_bucket_table(report: &mut String, counts: &BTreeMap<&'static str, usize>) {
    writeln!(report, "### Non-Empty Pair Length Buckets").expect("write to string");
    writeln!(report).expect("write to string");
    if counts.is_empty() {
        writeln!(report, "_No non-empty pairs._").expect("write to string");
        writeln!(report).expect("write to string");
        return;
    }

    writeln!(report, "| Max char length | Count |").expect("write to string");
    writeln!(report, "|---|---:|").expect("write to string");
    for bucket in ["1", "2", "3-4", "5-8", "9-16", "17-32", "33+"] {
        if let Some(count) = counts.get(bucket) {
            writeln!(report, "| `{bucket}` | {count} |").expect("write to string");
        }
    }
    writeln!(report).expect("write to string");
}

fn run_compare(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let options = CompareOptions::parse(args)?;
    if options.overrides.is_none()
        && options.lex_csv.is_none()
        && options.artifact_payload.is_none()
        && options.artifact_metadata.is_none()
    {
        return Err(Box::new(CliError::MissingComparisonMethod));
    }
    let dictionary_sources = [
        ("--lex-csv", options.lex_csv.is_some()),
        ("--artifact-payload", options.artifact_payload.is_some()),
        ("--artifact-metadata", options.artifact_metadata.is_some()),
    ];
    let selected_dictionary_sources = dictionary_sources
        .iter()
        .filter_map(|(name, present)| present.then_some(*name))
        .collect::<Vec<_>>();
    if selected_dictionary_sources.len() > 1 {
        return Err(Box::new(CliError::ConflictingArguments(
            selected_dictionary_sources[0],
            selected_dictionary_sources[1],
        )));
    }

    let override_result = if let Some(overrides_path) = &options.overrides {
        let override_yaml = fs::read_to_string(overrides_path)?;
        let overrides = OverrideDictionary::from_yaml_str(&override_yaml)?;
        let distances = compare_with_overrides(&options.left, &options.right, &overrides)?;
        let left_lattice = overrides.romaji_lattice(&options.left)?;
        let right_lattice = overrides.romaji_lattice(&options.right)?;
        let trace = distance_with_trace(&left_lattice, &right_lattice);
        Some((distances, trace))
    } else {
        None
    };

    let dict_result = if options.lex_csv.is_some()
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
        } else if let Some(metadata_path) = &options.artifact_metadata {
            let loaded = load_unidic_artifact_bundle_for_runtime(metadata_path, None)?;
            let payload_path = loaded.payload_path.display().to_string();
            let payload_format = loaded.metadata.payload.format.clone();
            let dictionary_options = options
                .dictionary_option_overrides
                .apply_to(dictionary_options_from_metadata(&loaded.metadata));
            (
                loaded.index,
                DictComparisonSource::ArtifactMetadata {
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
            "unidic_longest_only: {}",
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

    Ok(())
}

fn best_path(trace: &moine_core::DistanceTrace) -> BestPath {
    BestPath {
        left: symbols_to_string(&trace.left_symbols()),
        right: symbols_to_string(&trace.right_symbols()),
    }
}

fn print_lattice_result(
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

fn query_reading_expansion(
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

fn print_query_reading_stats(label: &str, expansion: &QueryReadingExpansion) {
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

fn print_dict_comparison_source(source: &DictComparisonSource) {
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
        DictComparisonSource::ArtifactPayload {
            path,
            payload_format,
        } => {
            println!("unidic_source:      artifact_payload");
            println!("artifact_payload:   {path}");
            println!("payload_format:     {}", payload_format.as_str());
        }
        DictComparisonSource::ArtifactMetadata {
            metadata_path,
            payload_path,
            payload_format,
            file_digest_verified,
        } => {
            println!("unidic_source:      artifact_metadata");
            println!("artifact_metadata:  {metadata_path}");
            println!("artifact_payload:   {payload_path}");
            println!("payload_format:     {payload_format}");
            println!("file_digest:        verified={file_digest_verified}");
        }
    }
}

fn print_reading_stats(label: &str, stats: &DictionaryReadingStats) {
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

fn query_pinyin_expansion(
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

fn print_pinyin_query_stats(label: &str, expansion: &PinyinQueryExpansion) {
    match expansion {
        PinyinQueryExpansion::DirectAscii => println!("{label}: direct_ascii"),
        PinyinQueryExpansion::Dictionary { path_count, stats } => {
            print_pinyin_stats(label, stats);
            println!("{label}_paths: {path_count}");
        }
    }
}

fn print_pinyin_stats(label: &str, stats: &PinyinReadingStats) {
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

fn print_chinese_lattice_result(
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
struct MecabToken {
    surface: String,
    reading: Option<String>,
}

fn parse_mecab_tokens(output: &str) -> Vec<MecabToken> {
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

fn symbols_to_string(symbols: &[moine_core::Symbol]) -> String {
    symbols
        .iter()
        .map(|&symbol| char::from_u32(symbol).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn format_reading_segments(segments: &[moine_ja::DictionaryReadingSegment]) -> String {
    segments
        .iter()
        .map(|segment| format!("{}/{}", segment.surface, segment.reading))
        .collect::<Vec<_>>()
        .join(" + ")
}

fn format_pinyin_segments(segments: &[moine_zh::PinyinReadingSegment]) -> String {
    segments
        .iter()
        .map(|segment| format!("{}/{}", segment.surface, segment.reading))
        .collect::<Vec<_>>()
        .join(" + ")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompareOptions {
    left: String,
    right: String,
    overrides: Option<String>,
    lex_csv: Option<String>,
    artifact_payload: Option<String>,
    artifact_metadata: Option<String>,
    payload_format: ArtifactPayloadFormat,
    index_options: UnidicIndexOptions,
    dictionary_options: DictionaryReadingOptions,
    dictionary_option_overrides: DictionaryReadingOptionOverrides,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JapaneseReportOptions {
    overrides: String,
    lex_csv: String,
    output: Option<String>,
    index_options: UnidicIndexOptions,
    dictionary_options: DictionaryReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JwtdSummaryOptions {
    splits: Vec<JwtdSplitInput>,
    output: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JwtdScorerReportOptions {
    split: JwtdSplitInput,
    artifact_metadata: Option<String>,
    bundle_dir: Option<String>,
    negative_policy: JwtdNegativePolicy,
    negative_count: usize,
    tie_policy: JwtdTiePolicy,
    seed: u64,
    max_examples: Option<usize>,
    output: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DownloadCliOptions {
    spec: DownloadArtifactSpec,
    url: Option<String>,
    checksum_url: Option<String>,
    sha256: Option<String>,
    cache_dir: Option<String>,
    force: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CacheCliOptions {
    cache_dir: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WhereCliOptions {
    language: Option<ArtifactLanguage>,
    cache_dir: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JwtdSplitInput {
    name: String,
    path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JwtdNegativePolicy {
    Length,
    SurfaceHard,
}

impl JwtdNegativePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::SurfaceHard => "surface-hard",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JwtdTiePolicy {
    Stable,
    Pessimistic,
}

impl JwtdTiePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Pessimistic => "pessimistic",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JwtdExample {
    query: String,
    gold: String,
    category: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum JwtdScorer {
    SurfaceLevenshtein,
    SurfaceDamerau,
    Lped,
    CombinedSurfaceDamerauLped,
}

impl JwtdScorer {
    fn as_str(self) -> &'static str {
        match self {
            Self::SurfaceLevenshtein => "surface_levenshtein",
            Self::SurfaceDamerau => "surface_damerau",
            Self::Lped => "lped",
            Self::CombinedSurfaceDamerauLped => "combined_surface_damerau_lped",
        }
    }

    #[cfg(test)]
    fn score(
        self,
        left: &str,
        right: &str,
        lped_context: Option<&JwtdLpedContext<'_>>,
    ) -> Result<usize, Box<dyn Error>> {
        match self {
            Self::SurfaceLevenshtein => Ok(levenshtein_str(left, right)),
            Self::SurfaceDamerau => Ok(damerau_levenshtein_str(left, right)),
            Self::Lped => {
                let context =
                    lped_context.ok_or(CliError::MissingArgument("--artifact-metadata"))?;
                Ok(jwtd_lped_score(left, right, context))
            }
            Self::CombinedSurfaceDamerauLped => {
                let context =
                    lped_context.ok_or(CliError::MissingArgument("--artifact-metadata"))?;
                let surface = damerau_levenshtein_str(left, right);
                let lped = jwtd_lped_score(left, right, context);
                Ok(surface.min(lped))
            }
        }
    }
}

fn jwtd_lped_score(left: &str, right: &str, context: &JwtdLpedContext<'_>) -> usize {
    match compare_with_unidic_index(left, right, context.index, context.options) {
        Ok(distances) => distances.lattice,
        Err(_) => JWTD_UNSCORABLE_DISTANCE,
    }
}

#[derive(Clone, Copy, Debug)]
struct JwtdLpedContext<'a> {
    index: &'a UnidicReadingIndex,
    options: DictionaryReadingOptions,
}

#[derive(Clone, Debug, Default)]
struct JwtdMetricAccumulator {
    ranks: Vec<usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct JwtdSplitSummary {
    name: String,
    path: String,
    records: usize,
    records_with_diffs: usize,
    diffs: usize,
    nonempty_pairs: usize,
    empty_pre: usize,
    empty_post: usize,
    empty_both: usize,
    category_counts: BTreeMap<String, usize>,
    nonempty_category_counts: BTreeMap<String, usize>,
    pair_length_buckets: BTreeMap<&'static str, usize>,
    pre_len: LengthStats,
    post_len: LengthStats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CedictReadingsOptions {
    surface: String,
    cedict: String,
    index_options: CedictIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CedictSequencesOptions {
    text: String,
    cedict: String,
    index_options: CedictIndexOptions,
    reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ChineseCompareOptions {
    left: String,
    right: String,
    source: ZhIndexSource,
    index_options: CedictIndexOptions,
    reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ZhIndexSource {
    Cedict(String),
    ArtifactPayload {
        path: String,
        payload_format: ArtifactPayloadFormat,
    },
    ArtifactMetadata(String),
}

impl ZhIndexSource {
    fn label(&self) -> (&'static str, &str) {
        match self {
            Self::Cedict(path) => ("cedict", path),
            Self::ArtifactPayload { path, .. } => ("artifact_payload", path),
            Self::ArtifactMetadata(path) => ("artifact_metadata", path),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZhArtifactMetadataCliOptions {
    cedict: String,
    output: Option<String>,
    artifact_name: String,
    payload_file_name: String,
    payload_format: ArtifactPayloadFormat,
    source_name: String,
    source_version: String,
    index_options: CedictIndexOptions,
    reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZhArtifactBundleCliOptions {
    cedict: String,
    output_dir: String,
    artifact_name: String,
    payload_format: ArtifactPayloadFormat,
    source_name: String,
    source_version: String,
    license_file: Option<String>,
    index_options: CedictIndexOptions,
    reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZhArtifactPayloadCliOptions {
    cedict: String,
    output: Option<String>,
    payload_format: ArtifactPayloadFormat,
    index_options: CedictIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZhArtifactArchiveCliOptions {
    metadata: String,
    output: String,
    bundle_dir: Option<String>,
    root_name: Option<String>,
    compression: ArchiveCompression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZhArtifactInspectCliOptions {
    payload: String,
    payload_format: ArtifactPayloadFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ZhArtifactVerifyCliOptions {
    metadata: String,
    bundle_dir: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LengthStats {
    count: usize,
    sum: usize,
    max: usize,
}

#[derive(Debug, Deserialize)]
struct JwtdRecord {
    #[serde(default)]
    diffs: Vec<JwtdDiff>,
}

#[derive(Debug, Deserialize)]
struct JwtdDiff {
    pre_str: String,
    post_str: String,
    category: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactMetadataCliOptions {
    lex_csv: String,
    output: Option<String>,
    artifact_name: String,
    payload_file_name: String,
    payload_format: ArtifactPayloadFormat,
    source_name: String,
    source_version: String,
    index_options: UnidicIndexOptions,
    dictionary_options: DictionaryReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactBundleCliOptions {
    lex_csv: String,
    output_dir: String,
    artifact_name: String,
    payload_format: ArtifactPayloadFormat,
    source_name: String,
    source_version: String,
    license_dir: Option<String>,
    index_options: UnidicIndexOptions,
    dictionary_options: DictionaryReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactArchiveCliOptions {
    metadata: String,
    output: String,
    bundle_dir: Option<String>,
    root_name: Option<String>,
    compression: ArchiveCompression,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactPayloadFormat {
    Yaml,
    Binary,
    Indexed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArchiveCompression {
    None,
    Gzip,
    Zstd,
}

impl ArchiveCompression {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactBinaryPayloadCliOptions {
    lex_csv: String,
    output: String,
    index_options: UnidicIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactBinaryInspectCliOptions {
    payload: String,
    timing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactPayloadCliOptions {
    lex_csv: String,
    output: Option<String>,
    index_options: UnidicIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactInspectCliOptions {
    payload: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactVerifyCliOptions {
    metadata: String,
    bundle_dir: Option<String>,
    canonical_checksum: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactReleaseChecksumsCliOptions {
    assets: Vec<String>,
    output: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicArtifactRuntimeMeasureCliOptions {
    metadata: String,
    bundle_dir: Option<String>,
    pairs: Vec<RuntimeMeasurePair>,
    warmups: usize,
    iterations: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RuntimeMeasurePair {
    left: String,
    right: String,
}

#[derive(Clone, Debug)]
struct VerifiedArtifactBundle {
    metadata_path: PathBuf,
    bundle_dir: PathBuf,
    payload_path: PathBuf,
    metadata: UnidicArtifactMetadata,
    entries: usize,
    file_digest: Option<String>,
    checksum: Option<String>,
    used_binary_header: bool,
}

#[derive(Clone, Debug)]
struct VerifiedZhArtifactBundle {
    metadata_path: PathBuf,
    bundle_dir: PathBuf,
    payload_path: PathBuf,
    metadata: ZhArtifactMetadata,
    index: ZhReadingIndex,
    file_digest: Option<String>,
    checksum: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ArchiveEntry {
    source: PathBuf,
    path: String,
}

#[derive(Clone, Copy, Debug)]
struct BinaryInspectTiming {
    read_file: Duration,
    decode_binary: Duration,
}

#[derive(Clone, Copy, Debug)]
struct RuntimeLoadTiming {
    read_metadata: Duration,
    file_digest: Duration,
    decode_payload: Duration,
    canonical_checksum: Option<Duration>,
    total: Duration,
}

#[derive(Clone, Debug)]
struct RuntimeLoadedArtifactBundle {
    metadata_path: PathBuf,
    bundle_dir: PathBuf,
    payload_path: PathBuf,
    metadata: UnidicArtifactMetadata,
    index: UnidicReadingIndex,
    file_digest_verified: bool,
    timing: RuntimeLoadTiming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JapaneseReportPair {
    left: &'static str,
    right: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JapaneseReportRow {
    pair: &'static JapaneseReportPair,
    override_distances: JapaneseDistance,
    dict_distances: JapaneseDistance,
    override_best_path: BestPath,
    dict_best_path: BestPath,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BestPath {
    left: String,
    right: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DictComparisonResult {
    source: DictComparisonSource,
    dictionary_options: DictionaryReadingOptions,
    distances: JapaneseDistance,
    trace: Option<moine_core::DistanceTrace>,
    trace_error: Option<String>,
    left_expansion: QueryReadingExpansion,
    right_expansion: QueryReadingExpansion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum DictComparisonSource {
    LexCsv {
        path: String,
        index_options: UnidicIndexOptions,
    },
    ArtifactPayload {
        path: String,
        payload_format: ArtifactPayloadFormat,
    },
    ArtifactMetadata {
        metadata_path: String,
        payload_path: String,
        payload_format: String,
        file_digest_verified: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DictionaryReadingOptionOverrides {
    max_span_chars: Option<usize>,
    max_paths: Option<usize>,
    longest_match_only: bool,
    max_readings_per_segment: Option<usize>,
}

impl DictionaryReadingOptionOverrides {
    fn apply_to(self, mut options: DictionaryReadingOptions) -> DictionaryReadingOptions {
        if let Some(max_span_chars) = self.max_span_chars {
            options.max_span_chars = max_span_chars;
        }
        if let Some(max_paths) = self.max_paths {
            options.max_paths = max_paths;
        }
        if self.longest_match_only {
            options.longest_match_only = true;
        }
        if let Some(max_readings_per_segment) = self.max_readings_per_segment {
            options.max_readings_per_segment = Some(max_readings_per_segment);
        }
        options
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum QueryReadingExpansion {
    DirectRomaji,
    Dictionary {
        has_direct_romaji: bool,
        path_count: usize,
        stats: DictionaryReadingStats,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PinyinQueryExpansion {
    DirectAscii,
    Dictionary {
        path_count: usize,
        stats: PinyinReadingStats,
    },
}

const JAPANESE_REPORT_PAIRS: &[JapaneseReportPair] = &[
    JapaneseReportPair {
        left: "きめつのやいば",
        right: "鬼滅の刃",
    },
    JapaneseReportPair {
        left: "いんさt",
        right: "印刷",
    },
    JapaneseReportPair {
        left: "chadougu",
        right: "茶道具",
    },
    JapaneseReportPair {
        left: "とうきょうと",
        right: "東京都",
    },
    JapaneseReportPair {
        left: "愛知家コロナ",
        right: "愛知県コロナ",
    },
    JapaneseReportPair {
        left: "マトリッツォ",
        right: "マリトッツォ",
    },
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicReadingsOptions {
    text: String,
    dic_dir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicCsvReadingsOptions {
    surface: String,
    lex_csv: String,
    index_options: UnidicIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct UnidicCsvSequencesOptions {
    text: String,
    lex_csv: String,
    index_options: UnidicIndexOptions,
    dictionary_options: DictionaryReadingOptions,
}

impl DownloadCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut language = None;
        let mut url = None;
        let mut checksum_url = None;
        let mut sha256 = None;
        let mut cache_dir = None;
        let mut force = false;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--url" => {
                    url = Some(value_after(&args, i, "--url")?);
                    i += 2;
                }
                "--checksum-url" => {
                    checksum_url = Some(value_after(&args, i, "--checksum-url")?);
                    i += 2;
                }
                "--sha256" => {
                    sha256 = Some(value_after(&args, i, "--sha256")?);
                    i += 2;
                }
                "--cache-dir" => {
                    cache_dir = Some(value_after(&args, i, "--cache-dir")?);
                    i += 2;
                }
                "--force" => {
                    force = true;
                    i += 1;
                }
                value if value.starts_with('-') => {
                    return Err(CliError::UnknownArgument(value.to_string()));
                }
                value => {
                    if language.is_some() {
                        return Err(CliError::UnknownArgument(value.to_string()));
                    }
                    language = Some(parse_artifact_language(value)?);
                    i += 1;
                }
            }
        }

        let language = language.ok_or(CliError::MissingArgument("lang"))?;
        Ok(Self {
            spec: *download_spec_for_language(language),
            url,
            checksum_url,
            sha256,
            cache_dir,
            force,
        })
    }
}

impl CacheCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut cache_dir = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--cache-dir" => {
                    cache_dir = Some(value_after(&args, i, "--cache-dir")?);
                    i += 2;
                }
                value if value.starts_with('-') => {
                    return Err(CliError::UnknownArgument(value.to_string()));
                }
                value => return Err(CliError::UnknownArgument(value.to_string())),
            }
        }
        Ok(Self { cache_dir })
    }
}

impl WhereCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut language = None;
        let mut cache_dir = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--cache-dir" => {
                    cache_dir = Some(value_after(&args, i, "--cache-dir")?);
                    i += 2;
                }
                value if value.starts_with('-') => {
                    return Err(CliError::UnknownArgument(value.to_string()));
                }
                value => {
                    if language.is_some() {
                        return Err(CliError::UnknownArgument(value.to_string()));
                    }
                    language = Some(parse_artifact_language(value)?);
                    i += 1;
                }
            }
        }
        Ok(Self {
            language,
            cache_dir,
        })
    }
}

impl CedictReadingsOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut surface = None;
        let mut cedict = None;
        let mut index_options = CedictIndexOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--surface" => {
                    surface = Some(value_after(&args, i, "--surface")?);
                    i += 2;
                }
                "--cedict" => {
                    cedict = Some(value_after(&args, i, "--cedict")?);
                    i += 2;
                }
                "--pinyin-view" => {
                    index_options.pinyin_view =
                        parse_pinyin_view(&value_after(&args, i, "--pinyin-view")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_cedict_readings_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            surface: surface.ok_or(CliError::MissingArgument("--surface"))?,
            cedict: cedict.ok_or(CliError::MissingArgument("--cedict"))?,
            index_options,
        })
    }
}

impl CedictSequencesOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut text = None;
        let mut cedict = None;
        let mut index_options = CedictIndexOptions::default();
        let mut reading_options = PinyinReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--text" => {
                    text = Some(value_after(&args, i, "--text")?);
                    i += 2;
                }
                "--cedict" => {
                    cedict = Some(value_after(&args, i, "--cedict")?);
                    i += 2;
                }
                "--pinyin-view" => {
                    index_options.pinyin_view =
                        parse_pinyin_view(&value_after(&args, i, "--pinyin-view")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    reading_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--max-span-chars" => {
                    reading_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    reading_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    reading_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_cedict_sequences_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            text: text.ok_or(CliError::MissingArgument("--text"))?,
            cedict: cedict.ok_or(CliError::MissingArgument("--cedict"))?,
            index_options,
            reading_options,
        })
    }
}

impl ChineseCompareOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut left = None;
        let mut right = None;
        let mut cedict = None;
        let mut artifact_payload = None;
        let mut artifact_metadata = None;
        let mut payload_format = ArtifactPayloadFormat::Yaml;
        let mut index_options = CedictIndexOptions::default();
        let mut reading_options = PinyinReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--left" => {
                    left = Some(value_after(&args, i, "--left")?);
                    i += 2;
                }
                "--right" => {
                    right = Some(value_after(&args, i, "--right")?);
                    i += 2;
                }
                "--cedict" => {
                    cedict = Some(value_after(&args, i, "--cedict")?);
                    i += 2;
                }
                "--artifact-payload" => {
                    artifact_payload = Some(value_after(&args, i, "--artifact-payload")?);
                    i += 2;
                }
                "--payload-format" => {
                    payload_format = parse_zh_artifact_payload_format(&value_after(
                        &args,
                        i,
                        "--payload-format",
                    )?)?;
                    i += 2;
                }
                "--artifact-metadata" => {
                    artifact_metadata = Some(value_after(&args, i, "--artifact-metadata")?);
                    i += 2;
                }
                "--pinyin-view" => {
                    index_options.pinyin_view =
                        parse_pinyin_view(&value_after(&args, i, "--pinyin-view")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    reading_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--max-span-chars" => {
                    reading_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    reading_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    reading_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_chinese_compare_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        let source = match (cedict, artifact_payload, artifact_metadata) {
            (Some(path), None, None) => ZhIndexSource::Cedict(path),
            (None, Some(path), None) => ZhIndexSource::ArtifactPayload {
                path,
                payload_format,
            },
            (None, None, Some(path)) => ZhIndexSource::ArtifactMetadata(path),
            (None, None, None) => {
                return Err(CliError::MissingArgument(
                    "--cedict, --artifact-payload, or --artifact-metadata",
                ))
            }
            (Some(_), Some(_), _) => {
                return Err(CliError::ConflictingArguments(
                    "--cedict",
                    "--artifact-payload",
                ))
            }
            (Some(_), _, Some(_)) => {
                return Err(CliError::ConflictingArguments(
                    "--cedict",
                    "--artifact-metadata",
                ))
            }
            (None, Some(_), Some(_)) => {
                return Err(CliError::ConflictingArguments(
                    "--artifact-payload",
                    "--artifact-metadata",
                ))
            }
        };

        Ok(Self {
            left: left.ok_or(CliError::MissingArgument("--left"))?,
            right: right.ok_or(CliError::MissingArgument("--right"))?,
            source,
            index_options,
            reading_options,
        })
    }
}

impl ZhArtifactMetadataCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut cedict = None;
        let mut output = None;
        let mut artifact_name = "moine-cedict-reading-index".to_string();
        let mut payload_format = ArtifactPayloadFormat::Indexed;
        let mut payload_file_name = None;
        let mut source_name = "CC-CEDICT".to_string();
        let mut source_version = None;
        let mut index_options = CedictIndexOptions::default();
        let mut reading_options = PinyinReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--cedict" => {
                    cedict = Some(value_after(&args, i, "--cedict")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--artifact-name" => {
                    artifact_name = value_after(&args, i, "--artifact-name")?;
                    i += 2;
                }
                "--payload-file-name" => {
                    payload_file_name = Some(value_after(&args, i, "--payload-file-name")?);
                    i += 2;
                }
                "--payload-format" => {
                    payload_format = parse_zh_artifact_payload_format(&value_after(
                        &args,
                        i,
                        "--payload-format",
                    )?)?;
                    i += 2;
                }
                "--source-name" => {
                    source_name = value_after(&args, i, "--source-name")?;
                    i += 2;
                }
                "--source-version" => {
                    source_version = Some(value_after(&args, i, "--source-version")?);
                    i += 2;
                }
                "--pinyin-view" => {
                    index_options.pinyin_view =
                        parse_pinyin_view(&value_after(&args, i, "--pinyin-view")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    reading_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--max-span-chars" => {
                    reading_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    reading_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    reading_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_zh_artifact_metadata_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            cedict: cedict.ok_or(CliError::MissingArgument("--cedict"))?,
            output,
            payload_file_name: payload_file_name
                .unwrap_or_else(|| default_zh_payload_file_name(&artifact_name, payload_format)),
            payload_format,
            artifact_name,
            source_name,
            source_version: source_version.ok_or(CliError::MissingArgument("--source-version"))?,
            index_options,
            reading_options,
        })
    }
}

impl ZhArtifactBundleCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut cedict = None;
        let mut output_dir = None;
        let mut artifact_name = "moine-cedict-reading-index".to_string();
        let mut payload_format = ArtifactPayloadFormat::Indexed;
        let mut source_name = "CC-CEDICT".to_string();
        let mut source_version = None;
        let mut license_file = None;
        let mut index_options = CedictIndexOptions::default();
        let mut reading_options = PinyinReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--cedict" => {
                    cedict = Some(value_after(&args, i, "--cedict")?);
                    i += 2;
                }
                "--output-dir" => {
                    output_dir = Some(value_after(&args, i, "--output-dir")?);
                    i += 2;
                }
                "--artifact-name" => {
                    artifact_name = value_after(&args, i, "--artifact-name")?;
                    i += 2;
                }
                "--payload-format" => {
                    payload_format = parse_zh_artifact_payload_format(&value_after(
                        &args,
                        i,
                        "--payload-format",
                    )?)?;
                    i += 2;
                }
                "--source-name" => {
                    source_name = value_after(&args, i, "--source-name")?;
                    i += 2;
                }
                "--source-version" => {
                    source_version = Some(value_after(&args, i, "--source-version")?);
                    i += 2;
                }
                "--license-file" => {
                    license_file = Some(value_after(&args, i, "--license-file")?);
                    i += 2;
                }
                "--pinyin-view" => {
                    index_options.pinyin_view =
                        parse_pinyin_view(&value_after(&args, i, "--pinyin-view")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    reading_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--max-span-chars" => {
                    reading_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    reading_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    reading_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_zh_artifact_bundle_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            cedict: cedict.ok_or(CliError::MissingArgument("--cedict"))?,
            output_dir: output_dir.ok_or(CliError::MissingArgument("--output-dir"))?,
            artifact_name,
            payload_format,
            source_name,
            source_version: source_version.ok_or(CliError::MissingArgument("--source-version"))?,
            license_file,
            index_options,
            reading_options,
        })
    }
}

impl ZhArtifactPayloadCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut cedict = None;
        let mut output = None;
        let mut payload_format = ArtifactPayloadFormat::Yaml;
        let mut index_options = CedictIndexOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--cedict" => {
                    cedict = Some(value_after(&args, i, "--cedict")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--payload-format" => {
                    payload_format = parse_zh_artifact_payload_format(&value_after(
                        &args,
                        i,
                        "--payload-format",
                    )?)?;
                    i += 2;
                }
                "--pinyin-view" => {
                    index_options.pinyin_view =
                        parse_pinyin_view(&value_after(&args, i, "--pinyin-view")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_zh_artifact_payload_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            cedict: cedict.ok_or(CliError::MissingArgument("--cedict"))?,
            output,
            payload_format,
            index_options,
        })
    }
}

impl ZhArtifactArchiveCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut metadata = None;
        let mut output = None;
        let mut bundle_dir = None;
        let mut root_name = None;
        let mut compression = ArchiveCompression::None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--metadata" => {
                    metadata = Some(value_after(&args, i, "--metadata")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--bundle-dir" => {
                    bundle_dir = Some(value_after(&args, i, "--bundle-dir")?);
                    i += 2;
                }
                "--root-name" => {
                    root_name = Some(value_after(&args, i, "--root-name")?);
                    i += 2;
                }
                "--compression" => {
                    compression =
                        parse_archive_compression(&value_after(&args, i, "--compression")?)?;
                    i += 2;
                }
                "-h" | "--help" => {
                    print_zh_artifact_archive_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            metadata: metadata.ok_or(CliError::MissingArgument("--metadata"))?,
            output: output.ok_or(CliError::MissingArgument("--output"))?,
            bundle_dir,
            root_name,
            compression,
        })
    }
}

impl ZhArtifactInspectCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut payload = None;
        let mut payload_format = ArtifactPayloadFormat::Yaml;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--payload" => {
                    payload = Some(value_after(&args, i, "--payload")?);
                    i += 2;
                }
                "--payload-format" => {
                    payload_format = parse_zh_artifact_payload_format(&value_after(
                        &args,
                        i,
                        "--payload-format",
                    )?)?;
                    i += 2;
                }
                "-h" | "--help" => {
                    print_zh_artifact_inspect_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            payload: payload.ok_or(CliError::MissingArgument("--payload"))?,
            payload_format,
        })
    }
}

impl ZhArtifactVerifyCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut metadata = None;
        let mut bundle_dir = None;
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--metadata" => {
                    metadata = Some(value_after(&args, i, "--metadata")?);
                    i += 2;
                }
                "--bundle-dir" => {
                    bundle_dir = Some(value_after(&args, i, "--bundle-dir")?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_zh_artifact_verify_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            metadata: metadata.ok_or(CliError::MissingArgument("--metadata"))?,
            bundle_dir,
        })
    }
}

impl UnidicCsvSequencesOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut text = None;
        let mut lex_csv = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut dictionary_options = DictionaryReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--text" => {
                    text = Some(value_after(&args, i, "--text")?);
                    i += 2;
                }
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    dictionary_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "--max-span-chars" => {
                    dictionary_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    dictionary_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    dictionary_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_csv_sequences_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            text: text.ok_or(CliError::MissingArgument("--text"))?,
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            index_options,
            dictionary_options,
        })
    }
}

impl UnidicCsvReadingsOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut surface = None;
        let mut lex_csv = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--surface" => {
                    surface = Some(value_after(&args, i, "--surface")?);
                    i += 2;
                }
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_csv_readings_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            surface: surface.ok_or(CliError::MissingArgument("--surface"))?,
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            index_options,
        })
    }
}

impl JapaneseReportOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut overrides = None;
        let mut lex_csv = None;
        let mut output = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut dictionary_options = DictionaryReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--overrides" => {
                    overrides = Some(value_after(&args, i, "--overrides")?);
                    i += 2;
                }
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    dictionary_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "--max-span-chars" => {
                    dictionary_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    dictionary_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    dictionary_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_japanese_report_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            overrides: overrides.ok_or(CliError::MissingArgument("--overrides"))?,
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            output,
            index_options,
            dictionary_options,
        })
    }
}

impl JwtdSummaryOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut splits = Vec::new();
        let mut output = None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--split" => {
                    let name = value_after(&args, i, "--split")?;
                    let path = args
                        .get(i + 2)
                        .cloned()
                        .ok_or(CliError::MissingArgumentValue("--split"))?;
                    splits.push(JwtdSplitInput { name, path });
                    i += 3;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_jwtd_summary_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        if splits.is_empty() {
            return Err(CliError::MissingArgument("--split"));
        }

        Ok(Self { splits, output })
    }
}

impl JwtdScorerReportOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut split = None;
        let mut artifact_metadata = None;
        let mut bundle_dir = None;
        let mut negative_policy = JwtdNegativePolicy::Length;
        let mut negative_count = 10;
        let mut tie_policy = JwtdTiePolicy::Stable;
        let mut seed = 0;
        let mut max_examples = Some(1000);
        let mut output = None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--split" => {
                    let name = value_after(&args, i, "--split")?;
                    let path = args
                        .get(i + 2)
                        .cloned()
                        .ok_or(CliError::MissingArgumentValue("--split"))?;
                    split = Some(JwtdSplitInput { name, path });
                    i += 3;
                }
                "--negative-policy" => {
                    negative_policy =
                        parse_jwtd_negative_policy(&value_after(&args, i, "--negative-policy")?)?;
                    i += 2;
                }
                "--artifact-metadata" => {
                    artifact_metadata = Some(value_after(&args, i, "--artifact-metadata")?);
                    i += 2;
                }
                "--bundle-dir" => {
                    bundle_dir = Some(value_after(&args, i, "--bundle-dir")?);
                    i += 2;
                }
                "--negative-count" => {
                    negative_count = parse_usize_argument(
                        "--negative-count",
                        &value_after(&args, i, "--negative-count")?,
                    )?;
                    i += 2;
                }
                "--tie-policy" => {
                    tie_policy = parse_jwtd_tie_policy(&value_after(&args, i, "--tie-policy")?)?;
                    i += 2;
                }
                "--seed" => {
                    seed = parse_u64_argument("--seed", &value_after(&args, i, "--seed")?)?;
                    i += 2;
                }
                "--max-examples" => {
                    max_examples = Some(parse_usize_argument(
                        "--max-examples",
                        &value_after(&args, i, "--max-examples")?,
                    )?);
                    i += 2;
                }
                "--all-examples" => {
                    max_examples = None;
                    i += 1;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_jwtd_scorer_report_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            split: split.ok_or(CliError::MissingArgument("--split"))?,
            artifact_metadata,
            bundle_dir,
            negative_policy,
            negative_count,
            tie_policy,
            seed,
            max_examples,
            output,
        })
    }
}

impl UnidicArtifactMetadataCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut lex_csv = None;
        let mut output = None;
        let mut artifact_name = "moine-unidic-reading-index".to_string();
        let mut payload_file_name = None;
        let mut payload_format = ArtifactPayloadFormat::Yaml;
        let mut source_name = "UniDic-CWJ".to_string();
        let mut source_version = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut dictionary_options = DictionaryReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--artifact-name" => {
                    artifact_name = value_after(&args, i, "--artifact-name")?;
                    i += 2;
                }
                "--payload-file-name" => {
                    payload_file_name = Some(value_after(&args, i, "--payload-file-name")?);
                    i += 2;
                }
                "--payload-format" => {
                    payload_format =
                        parse_artifact_payload_format(&value_after(&args, i, "--payload-format")?)?;
                    i += 2;
                }
                "--source-name" => {
                    source_name = value_after(&args, i, "--source-name")?;
                    i += 2;
                }
                "--source-version" => {
                    source_version = Some(value_after(&args, i, "--source-version")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    dictionary_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "--max-span-chars" => {
                    dictionary_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    dictionary_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    dictionary_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_metadata_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            output,
            payload_file_name: payload_file_name.unwrap_or_else(|| {
                default_unidic_payload_file_name(&artifact_name, payload_format)
            }),
            payload_format,
            artifact_name,
            source_name,
            source_version: source_version.ok_or(CliError::MissingArgument("--source-version"))?,
            index_options,
            dictionary_options,
        })
    }
}

impl UnidicArtifactBundleCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut lex_csv = None;
        let mut output_dir = None;
        let mut artifact_name = "moine-unidic-reading-index".to_string();
        let mut payload_format = ArtifactPayloadFormat::Yaml;
        let mut source_name = "UniDic-CWJ".to_string();
        let mut source_version = None;
        let mut license_dir = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut dictionary_options = DictionaryReadingOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--output-dir" => {
                    output_dir = Some(value_after(&args, i, "--output-dir")?);
                    i += 2;
                }
                "--artifact-name" => {
                    artifact_name = value_after(&args, i, "--artifact-name")?;
                    i += 2;
                }
                "--payload-format" => {
                    payload_format =
                        parse_artifact_payload_format(&value_after(&args, i, "--payload-format")?)?;
                    i += 2;
                }
                "--source-name" => {
                    source_name = value_after(&args, i, "--source-name")?;
                    i += 2;
                }
                "--source-version" => {
                    source_version = Some(value_after(&args, i, "--source-version")?);
                    i += 2;
                }
                "--license-dir" => {
                    license_dir = Some(value_after(&args, i, "--license-dir")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    dictionary_options.max_readings_per_segment = Some(parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "--max-span-chars" => {
                    dictionary_options.max_span_chars = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    i += 2;
                }
                "--max-paths" => {
                    dictionary_options.max_paths = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    i += 2;
                }
                "--longest-only" => {
                    dictionary_options.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_bundle_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            output_dir: output_dir.ok_or(CliError::MissingArgument("--output-dir"))?,
            artifact_name,
            payload_format,
            source_name,
            source_version: source_version.ok_or(CliError::MissingArgument("--source-version"))?,
            license_dir,
            index_options,
            dictionary_options,
        })
    }
}

impl UnidicArtifactArchiveCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut metadata = None;
        let mut output = None;
        let mut bundle_dir = None;
        let mut root_name = None;
        let mut compression = ArchiveCompression::None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--metadata" => {
                    metadata = Some(value_after(&args, i, "--metadata")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--bundle-dir" => {
                    bundle_dir = Some(value_after(&args, i, "--bundle-dir")?);
                    i += 2;
                }
                "--root-name" => {
                    root_name = Some(value_after(&args, i, "--root-name")?);
                    i += 2;
                }
                "--compression" => {
                    compression =
                        parse_archive_compression(&value_after(&args, i, "--compression")?)?;
                    i += 2;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_archive_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            metadata: metadata.ok_or(CliError::MissingArgument("--metadata"))?,
            output: output.ok_or(CliError::MissingArgument("--output"))?,
            bundle_dir,
            root_name,
            compression,
        })
    }
}

impl UnidicArtifactBinaryPayloadCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut lex_csv = None;
        let mut output = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_binary_payload_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            output: output.ok_or(CliError::MissingArgument("--output"))?,
            index_options,
        })
    }
}

impl UnidicArtifactBinaryInspectCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut payload = None;
        let mut timing = false;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--payload" => {
                    payload = Some(value_after(&args, i, "--payload")?);
                    i += 2;
                }
                "--timing" => {
                    timing = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_binary_inspect_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            payload: payload.ok_or(CliError::MissingArgument("--payload"))?,
            timing,
        })
    }
}

impl UnidicArtifactPayloadCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut lex_csv = None;
        let mut output = None;
        let mut index_options = UnidicIndexOptions::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_payload_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            lex_csv: lex_csv.ok_or(CliError::MissingArgument("--lex-csv"))?,
            output,
            index_options,
        })
    }
}

impl UnidicArtifactInspectCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut payload = None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--payload" => {
                    payload = Some(value_after(&args, i, "--payload")?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_inspect_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            payload: payload.ok_or(CliError::MissingArgument("--payload"))?,
        })
    }
}

impl UnidicArtifactVerifyCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut metadata = None;
        let mut bundle_dir = None;
        let mut canonical_checksum = false;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--metadata" => {
                    metadata = Some(value_after(&args, i, "--metadata")?);
                    i += 2;
                }
                "--bundle-dir" => {
                    bundle_dir = Some(value_after(&args, i, "--bundle-dir")?);
                    i += 2;
                }
                "--canonical-checksum" => {
                    canonical_checksum = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_verify_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            metadata: metadata.ok_or(CliError::MissingArgument("--metadata"))?,
            bundle_dir,
            canonical_checksum,
        })
    }
}

impl UnidicArtifactReleaseChecksumsCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut assets = Vec::new();
        let mut output = None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--asset" => {
                    assets.push(value_after(&args, i, "--asset")?);
                    i += 2;
                }
                "--output" => {
                    output = Some(value_after(&args, i, "--output")?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_release_checksums_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        if assets.is_empty() {
            return Err(CliError::MissingArgument("--asset"));
        }

        Ok(Self { assets, output })
    }
}

impl UnidicArtifactRuntimeMeasureCliOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut metadata = None;
        let mut bundle_dir = None;
        let mut left = None;
        let mut right = None;
        let mut pairs = Vec::new();
        let mut warmups = 5;
        let mut iterations = 100;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--metadata" => {
                    metadata = Some(value_after(&args, i, "--metadata")?);
                    i += 2;
                }
                "--bundle-dir" => {
                    bundle_dir = Some(value_after(&args, i, "--bundle-dir")?);
                    i += 2;
                }
                "--left" => {
                    left = Some(value_after(&args, i, "--left")?);
                    i += 2;
                }
                "--right" => {
                    right = Some(value_after(&args, i, "--right")?);
                    i += 2;
                }
                "--pair" => {
                    let pair_left = value_after(&args, i, "--pair")?;
                    let pair_right = args
                        .get(i + 2)
                        .cloned()
                        .ok_or(CliError::MissingArgumentValue("--pair"))?;
                    pairs.push(RuntimeMeasurePair {
                        left: pair_left,
                        right: pair_right,
                    });
                    i += 3;
                }
                "--warmups" => {
                    warmups =
                        parse_usize_argument("--warmups", &value_after(&args, i, "--warmups")?)?;
                    i += 2;
                }
                "--iterations" => {
                    iterations = parse_usize_argument(
                        "--iterations",
                        &value_after(&args, i, "--iterations")?,
                    )?;
                    if iterations == 0 {
                        return Err(CliError::InvalidArgumentValue {
                            name: "--iterations",
                            value: "0".to_string(),
                            expected: "a positive integer",
                        });
                    }
                    i += 2;
                }
                "-h" | "--help" => {
                    print_unidic_artifact_runtime_measure_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        match (left, right) {
            (Some(left), Some(right)) => pairs.push(RuntimeMeasurePair { left, right }),
            (None, None) => {}
            (None, Some(_)) => return Err(CliError::MissingArgument("--left")),
            (Some(_), None) => return Err(CliError::MissingArgument("--right")),
        }
        if pairs.is_empty() {
            return Err(CliError::MissingArgument("--pair"));
        }

        Ok(Self {
            metadata: metadata.ok_or(CliError::MissingArgument("--metadata"))?,
            bundle_dir,
            pairs,
            warmups,
            iterations,
        })
    }
}

impl ArtifactPayloadFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Yaml => YAML_PAYLOAD_FORMAT,
            Self::Binary => BINARY_PAYLOAD_FORMAT,
            Self::Indexed => INDEXED_PAYLOAD_FORMAT,
        }
    }
}

fn parse_artifact_payload_format(value: &str) -> Result<ArtifactPayloadFormat, CliError> {
    match value {
        "yaml" | YAML_PAYLOAD_FORMAT => Ok(ArtifactPayloadFormat::Yaml),
        "binary" | BINARY_PAYLOAD_FORMAT => Ok(ArtifactPayloadFormat::Binary),
        "indexed" | "fst" | INDEXED_PAYLOAD_FORMAT => Ok(ArtifactPayloadFormat::Indexed),
        _ => Err(CliError::InvalidArgumentValue {
            name: "--payload-format",
            value: value.to_string(),
            expected: "yaml, binary, or indexed",
        }),
    }
}

fn parse_zh_artifact_payload_format(value: &str) -> Result<ArtifactPayloadFormat, CliError> {
    let format = parse_artifact_payload_format(value)?;
    if format == ArtifactPayloadFormat::Binary {
        return Err(CliError::InvalidArgumentValue {
            name: "--payload-format",
            value: value.to_string(),
            expected: "yaml or indexed",
        });
    }
    Ok(format)
}

fn parse_archive_compression(value: &str) -> Result<ArchiveCompression, CliError> {
    match value {
        "none" | "tar" => Ok(ArchiveCompression::None),
        "gzip" | "gz" => Ok(ArchiveCompression::Gzip),
        "zstd" | "zst" => Ok(ArchiveCompression::Zstd),
        _ => Err(CliError::InvalidArgumentValue {
            name: "--compression",
            value: value.to_string(),
            expected: "none, gzip, or zstd",
        }),
    }
}

fn default_unidic_payload_file_name(
    artifact_name: &str,
    payload_format: ArtifactPayloadFormat,
) -> String {
    match payload_format {
        ArtifactPayloadFormat::Yaml => format!("{artifact_name}.readings.yaml"),
        ArtifactPayloadFormat::Binary => format!("{artifact_name}.readings.moinebin"),
        ArtifactPayloadFormat::Indexed => format!("{artifact_name}.readings.moineidx"),
    }
}

fn default_zh_payload_file_name(
    artifact_name: &str,
    payload_format: ArtifactPayloadFormat,
) -> String {
    match payload_format {
        ArtifactPayloadFormat::Yaml => format!("{artifact_name}.readings.yaml"),
        ArtifactPayloadFormat::Binary => format!("{artifact_name}.readings.moinebin"),
        ArtifactPayloadFormat::Indexed => format!("{artifact_name}.readings.moineidx"),
    }
}

fn default_unidic_license_dir(lex_csv: &str) -> PathBuf {
    Path::new(lex_csv)
        .parent()
        .map(|parent| parent.join("license"))
        .unwrap_or_else(|| PathBuf::from("license"))
}

fn parse_artifact_language(value: &str) -> Result<ArtifactLanguage, CliError> {
    match value {
        "ja" | "japanese" => Ok(ArtifactLanguage::Japanese),
        "zh" | "chinese" => Ok(ArtifactLanguage::Chinese),
        _ => Err(CliError::InvalidArgumentValue {
            name: "lang",
            value: value.to_string(),
            expected: "ja or zh",
        }),
    }
}

fn download_spec_for_language(language: ArtifactLanguage) -> &'static DownloadArtifactSpec {
    DOWNLOAD_ARTIFACT_SPECS
        .iter()
        .find(|spec| spec.language == language)
        .expect("download spec should exist for language")
}

fn default_cache_dir() -> PathBuf {
    if let Some(cache_dir) = env::var_os("MOINE_CACHE_DIR") {
        return PathBuf::from(cache_dir);
    }
    if let Some(cache_home) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(cache_home).join("moine").join("dictionaries");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("moine")
            .join("dictionaries");
    }
    PathBuf::from(".moine").join("dictionaries")
}

fn uri_file_name(uri: &str) -> Option<&str> {
    uri.rsplit('/')
        .next()
        .filter(|name| !name.is_empty() && !name.contains('\\'))
}

fn ensure_output_parent(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn write_output_file(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> Result<(), Box<dyn Error>> {
    let path = path.as_ref();
    ensure_output_parent(path)?;
    fs::write(path, contents)?;
    Ok(())
}

fn create_output_file(path: impl AsRef<Path>) -> Result<fs::File, Box<dyn Error>> {
    let path = path.as_ref();
    ensure_output_parent(path)?;
    Ok(fs::File::create(path)?)
}

fn copy_uri_to_path(uri: &str, output: &Path) -> Result<(), Box<dyn Error>> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        let response = ureq::get(uri)
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .call()?;
        let mut reader = response.into_reader();
        let mut file = fs::File::create(output)?;
        let copied = std::io::copy(&mut reader.by_ref().take(MAX_DOWNLOAD_BYTES + 1), &mut file)?;
        if copied > MAX_DOWNLOAD_BYTES {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "download exceeded maximum size of {MAX_DOWNLOAD_BYTES} bytes"
            ))));
        }
        return Ok(());
    }
    if let Some(path) = uri.strip_prefix("file://") {
        fs::copy(path, output)?;
        return Ok(());
    }
    fs::copy(uri, output)?;
    Ok(())
}

fn read_uri_text(uri: &str) -> Result<String, Box<dyn Error>> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        let response = ureq::get(uri)
            .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
            .call()?;
        let mut text = String::new();
        let read = response
            .into_reader()
            .take(MAX_CHECKSUM_MANIFEST_BYTES + 1)
            .read_to_string(&mut text)?;
        if read as u64 > MAX_CHECKSUM_MANIFEST_BYTES {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "checksum manifest exceeded maximum size of {MAX_CHECKSUM_MANIFEST_BYTES} bytes"
            ))));
        }
        return Ok(text);
    }
    if let Some(path) = uri.strip_prefix("file://") {
        return Ok(fs::read_to_string(path)?);
    }
    Ok(fs::read_to_string(uri)?)
}

fn expected_sha256(checksum_url: &str, archive_name: &str) -> Result<String, Box<dyn Error>> {
    for line in read_uri_text(checksum_url)?.lines() {
        let mut parts = line.split_whitespace();
        let Some(digest) = parts.next() else {
            continue;
        };
        let Some(label) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            continue;
        }
        if label == archive_name
            || Path::new(label).file_name().and_then(|name| name.to_str()) == Some(archive_name)
        {
            return Ok(digest.to_ascii_lowercase());
        }
    }
    Err(Box::new(CliError::ArtifactVerificationFailed(format!(
        "{archive_name} not found in checksum manifest: {checksum_url}"
    ))))
}

fn download_expected_sha256(
    options: &DownloadCliOptions,
    archive_name: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    if let Some(value) = &options.sha256 {
        return Ok(Some(value.to_ascii_lowercase()));
    }
    if let Some(checksum_url) = options
        .checksum_url
        .as_deref()
        .or(options.spec.checksum_url)
    {
        return Ok(Some(expected_sha256(checksum_url, archive_name)?));
    }
    Ok(None)
}

fn sha256_file(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = fs::File::open(path)?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sha2::Digest::update(&mut digest, &buffer[..read]);
    }
    Ok(format!("{:x}", sha2::Digest::finalize(digest)))
}

fn extract_artifact_archive(archive: &Path, output_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            CliError::ArtifactVerificationFailed(format!(
                "artifact archive path is not UTF-8: {}",
                archive.display()
            ))
        })?;
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let file = fs::File::open(archive)?;
        let decoder = GzDecoder::new(file);
        return extract_tar_stream(decoder, output_dir);
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tzst") {
        let file = fs::File::open(archive)?;
        let decoder = zstd::stream::read::Decoder::new(file)?;
        return extract_tar_stream(decoder, output_dir);
    }
    if name.ends_with(".tar.xz") || name.ends_with(".txz") {
        return extract_xz_tar_archive(archive, output_dir);
    }
    if name.ends_with(".tar") {
        let file = fs::File::open(archive)?;
        return extract_tar_stream(file, output_dir);
    }
    Err(Box::new(CliError::ArtifactVerificationFailed(format!(
        "unsupported artifact archive extension: {name}"
    ))))
}

fn extract_xz_tar_archive(archive: &Path, output_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let mut child = Command::new("xz")
        .arg("-dc")
        .arg(archive)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|err| {
            CliError::ArtifactVerificationFailed(format!(
                "failed to start xz for {}: {err}",
                archive.display()
            ))
        })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        CliError::ArtifactVerificationFailed("failed to capture xz stdout".to_string())
    })?;
    let extracted = extract_tar_stream(stdout, output_dir);
    let status = child.wait()?;
    let extracted = extracted?;
    if !status.success() {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "xz failed for {} with status {status}",
            archive.display()
        ))));
    }
    Ok(extracted)
}

fn extract_tar_stream(reader: impl Read, output_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    fs::create_dir_all(output_dir)?;
    let mut archive = tar::Archive::new(reader);
    let mut root_name = None::<PathBuf>;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "unsupported archive entry type for {}",
                entry.path()?.display()
            ))));
        }
        let path = entry.path()?.to_path_buf();
        let first_component = path.components().next().ok_or_else(|| {
            CliError::ArtifactVerificationFailed("empty archive path".to_string())
        })?;
        let std::path::Component::Normal(root_component) = first_component else {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "unsafe archive path: {}",
                path.display()
            ))));
        };
        let current_root = PathBuf::from(root_component);
        match &root_name {
            Some(root) if root != &current_root => {
                return Err(Box::new(CliError::ArtifactVerificationFailed(
                    "artifact archive must contain exactly one top-level directory".to_string(),
                )));
            }
            None => root_name = Some(current_root),
            _ => {}
        }
        if path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        }) || path.is_absolute()
        {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "unsafe archive path: {}",
                path.display()
            ))));
        }
        let target = output_dir.join(&path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(target)?;
    }
    let root = root_name.ok_or_else(|| {
        CliError::ArtifactVerificationFailed("artifact archive is empty".to_string())
    })?;
    Ok(output_dir.join(root))
}

fn verify_downloaded_bundle(
    language: ArtifactLanguage,
    metadata: &Path,
) -> Result<(), Box<dyn Error>> {
    let metadata = metadata.to_str().ok_or_else(|| {
        CliError::ArtifactVerificationFailed("metadata path is not UTF-8".to_string())
    })?;
    match language {
        ArtifactLanguage::Japanese => {
            verify_unidic_artifact_bundle(metadata, None, false)?;
        }
        ArtifactLanguage::Chinese => {
            verify_zh_artifact_bundle(metadata, None)?;
        }
    }
    Ok(())
}

fn installed_metadata_paths(cache_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut metadata_paths = Vec::new();
    if !cache_dir.is_dir() {
        return Ok(metadata_paths);
    }
    let root_metadata = cache_dir.join("metadata.yaml");
    if root_metadata.is_file() {
        metadata_paths.push(root_metadata);
    }
    for entry in fs::read_dir(cache_dir)? {
        let path = entry?.path();
        let metadata = path.join("metadata.yaml");
        if path.is_dir() && metadata.is_file() {
            metadata_paths.push(metadata);
        }
    }
    metadata_paths.sort();
    metadata_paths.dedup();
    Ok(metadata_paths)
}

fn find_metadata_by_prefix(
    cache_dir: &Path,
    artifact_name: &str,
) -> Result<Option<PathBuf>, Box<dyn Error>> {
    Ok(installed_metadata_paths(cache_dir)?
        .into_iter()
        .find(|metadata| {
            metadata
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(artifact_name))
        }))
}

fn move_dir(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir_all(source, destination)?;
            fs::remove_dir_all(source)?;
            Ok(())
        }
    }
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Result<Self, std::io::Error> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{nanos}", process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn copy_unidic_license_file(
    license_dir: &Path,
    output_license_dir: &Path,
    file_name: &str,
) -> Result<(), std::io::Error> {
    fs::copy(
        license_dir.join(file_name),
        output_license_dir.join(file_name),
    )?;
    Ok(())
}

fn write_zh_license_reference(
    license_file: Option<&str>,
    output_license_dir: &Path,
) -> Result<(), std::io::Error> {
    let output = output_license_dir.join("CC-CEDICT.md");
    if let Some(license_file) = license_file {
        fs::copy(license_file, output)?;
    } else {
        fs::write(
            output,
            "CC-CEDICT dictionary data is distributed under CC BY-SA 4.0.\n\
This moine artifact is derived from CC-CEDICT and keeps its license separate \
from the moine source-code license.\n",
        )?;
    }
    Ok(())
}

fn write_artifact_payload_file(
    index: &UnidicReadingIndex,
    payload_format: ArtifactPayloadFormat,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    match payload_format {
        ArtifactPayloadFormat::Yaml => {
            write_output_file(path, serde_yaml::to_string(&index.artifact_payload())?)?;
        }
        ArtifactPayloadFormat::Binary => {
            let output = create_output_file(path)?;
            index.write_artifact_binary_payload(output)?;
        }
        ArtifactPayloadFormat::Indexed => {
            let output = create_output_file(path)?;
            index.write_indexed_artifact_payload(output)?;
        }
    }
    Ok(())
}

fn write_zh_artifact_payload_file(
    index: &ZhReadingIndex,
    payload_format: ArtifactPayloadFormat,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    match payload_format {
        ArtifactPayloadFormat::Yaml => {
            write_output_file(path, serde_yaml::to_string(&index.artifact_payload())?)?;
        }
        ArtifactPayloadFormat::Indexed => {
            let output = create_output_file(path)?;
            index.write_indexed_artifact_payload(output)?;
        }
        ArtifactPayloadFormat::Binary => {
            return Err(Box::new(CliError::ArtifactVerificationFailed(
                "binary zh payload format is not implemented; use yaml or indexed".to_string(),
            )));
        }
    }
    Ok(())
}

fn load_zh_index(
    source: &ZhIndexSource,
    index_options: CedictIndexOptions,
) -> Result<ZhReadingIndex, Box<dyn Error>> {
    match source {
        ZhIndexSource::Cedict(path) => Ok(ZhReadingIndex::from_cedict_path_with_options(
            path,
            index_options,
        )?),
        ZhIndexSource::ArtifactPayload {
            path,
            payload_format,
        } => load_zh_artifact_payload_by_format(Path::new(path), payload_format.as_str()),
        ZhIndexSource::ArtifactMetadata(path) => Ok(verify_zh_artifact_bundle(path, None)?.index),
    }
}

fn verify_zh_artifact_bundle(
    metadata: &str,
    bundle_dir: Option<&str>,
) -> Result<VerifiedZhArtifactBundle, Box<dyn Error>> {
    let metadata_path = PathBuf::from(metadata);
    let metadata_yaml = fs::read_to_string(&metadata_path)?;
    let metadata = serde_yaml::from_str::<ZhArtifactMetadata>(&metadata_yaml)?;
    if metadata.schema_version != 1 {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported zh metadata schema version {}",
            metadata.schema_version
        ))));
    }
    if metadata.artifact_type != "moine.zh.reading-index" {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported zh artifact type {:?}",
            metadata.artifact_type
        ))));
    }
    let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
        metadata_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;
    let file_digest = verify_zh_payload_file_digest(&metadata, &payload_path)?;
    let index = load_zh_artifact_payload_by_format(&payload_path, &metadata.payload.format)?;
    if index.len() != metadata.build.entries {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries,
            index.len()
        ))));
    }
    if index.pinyin_view().as_str() != metadata.build.pinyin_view {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "pinyin view mismatch: metadata has {}, payload has {}",
            metadata.build.pinyin_view,
            index.pinyin_view().as_str()
        ))));
    }
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            CliError::ArtifactVerificationFailed(format!(
                "unsupported checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum != metadata.payload.checksum {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "payload checksum mismatch: metadata has {}, recomputed {}",
            metadata.payload.checksum, checksum
        ))));
    }
    for reference in &metadata.license.references {
        let path = checked_bundle_path(&bundle_dir, &reference.path)?;
        if !path.is_file() {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "missing license reference {} at {}",
                reference.label,
                path.display()
            ))));
        }
    }

    Ok(VerifiedZhArtifactBundle {
        metadata_path,
        bundle_dir,
        payload_path,
        metadata,
        index,
        file_digest,
        checksum,
    })
}

fn verify_zh_payload_file_digest(
    metadata: &ZhArtifactMetadata,
    payload_path: &Path,
) -> Result<Option<String>, Box<dyn Error>> {
    match (
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(algorithm), Some(expected)) => {
            if algorithm != ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "unsupported zh payload file digest algorithm {algorithm:?}"
                ))));
            }
            let digest = zh_artifact_file_digest_path(payload_path)?;
            if digest != expected {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "payload file digest mismatch: metadata has {expected}, recomputed {digest}"
                ))));
            }
            Ok(Some(digest))
        }
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(
            "zh payload file digest algorithm and digest must be provided together".to_string(),
        ))),
    }
}

fn load_zh_artifact_payload_by_format(
    path: &Path,
    payload_format: &str,
) -> Result<ZhReadingIndex, Box<dyn Error>> {
    match payload_format {
        YAML_PAYLOAD_FORMAT => Ok(ZhReadingIndex::from_artifact_payload_path(path)?),
        INDEXED_PAYLOAD_FORMAT => Ok(ZhReadingIndex::from_indexed_artifact_payload_path(path)?),
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported zh payload format {payload_format:?}"
        )))),
    }
}

fn zh_release_archive_entries(
    verified: &VerifiedZhArtifactBundle,
) -> Result<Vec<ArchiveEntry>, Box<dyn Error>> {
    let mut entries = vec![
        ArchiveEntry {
            source: verified.metadata_path.clone(),
            path: "metadata.yaml".to_string(),
        },
        ArchiveEntry {
            source: verified.payload_path.clone(),
            path: normalized_relative_archive_path(&verified.metadata.payload.path)?,
        },
    ];
    for reference in &verified.metadata.license.references {
        entries.push(ArchiveEntry {
            source: checked_bundle_path(&verified.bundle_dir, &reference.path)?,
            path: normalized_relative_archive_path(&reference.path)?,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries.dedup_by(|left, right| left.path == right.path);
    Ok(entries)
}

fn load_artifact_payload_by_format(
    path: &Path,
    payload_format: &str,
) -> Result<UnidicReadingIndex, Box<dyn Error>> {
    match payload_format {
        YAML_PAYLOAD_FORMAT => Ok(UnidicReadingIndex::from_artifact_payload_path(path)?),
        BINARY_PAYLOAD_FORMAT => Ok(UnidicReadingIndex::from_binary_artifact_payload_path(path)?),
        INDEXED_PAYLOAD_FORMAT => Ok(UnidicReadingIndex::from_indexed_artifact_payload_path(
            path,
        )?),
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "unsupported payload format {payload_format:?}"
        )))),
    }
}

fn verify_unidic_artifact_bundle(
    metadata: &str,
    bundle_dir: Option<&str>,
    _force_canonical_checksum: bool,
) -> Result<VerifiedArtifactBundle, Box<dyn Error>> {
    let metadata_path = PathBuf::from(metadata);
    let metadata_yaml = fs::read_to_string(&metadata_path)?;
    let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(&metadata_yaml)?;
    let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
        metadata_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;

    let file_digest = verify_payload_file_digest(&metadata, &payload_path)?;
    let (entries, checksum, used_binary_header) = {
        let index = load_artifact_payload_by_format(&payload_path, &metadata.payload.format)?;
        let checksum = index
            .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
            .ok_or_else(|| {
                CliError::ArtifactVerificationFailed(format!(
                    "unsupported checksum algorithm {:?}",
                    metadata.payload.checksum_algorithm
                ))
            })?;
        if checksum != metadata.payload.checksum {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "payload checksum mismatch: metadata has {}, recomputed {}",
                metadata.payload.checksum, checksum
            ))));
        }
        (index.len(), Some(checksum), false)
    };
    if entries != metadata.build.entries {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries, entries
        ))));
    }
    for reference in &metadata.license.references {
        let path = checked_bundle_path(&bundle_dir, &reference.path)?;
        if !path.is_file() {
            return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                "missing license reference {} at {}",
                reference.label,
                path.display()
            ))));
        }
    }

    Ok(VerifiedArtifactBundle {
        metadata_path,
        bundle_dir,
        payload_path,
        entries,
        file_digest,
        checksum,
        used_binary_header,
        metadata,
    })
}

fn load_unidic_artifact_bundle_for_runtime(
    metadata: &str,
    bundle_dir: Option<&str>,
) -> Result<RuntimeLoadedArtifactBundle, Box<dyn Error>> {
    let load_start = Instant::now();
    let metadata_path = PathBuf::from(metadata);

    let read_metadata_start = Instant::now();
    let metadata_yaml = fs::read_to_string(&metadata_path)?;
    let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(&metadata_yaml)?;
    let read_metadata = read_metadata_start.elapsed();

    let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
        metadata_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf()
    });
    let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;

    let file_digest_start = Instant::now();
    let file_digest = verify_payload_file_digest(&metadata, &payload_path)?;
    let file_digest_elapsed = file_digest_start.elapsed();

    let decode_start = Instant::now();
    let index = load_artifact_payload_by_format(&payload_path, &metadata.payload.format)?;
    let decode_payload = decode_start.elapsed();

    let checksum_start = Instant::now();
    verify_loaded_artifact_payload_checksum(&metadata, &index)?;
    let canonical_checksum = Some(checksum_start.elapsed());

    if index.len() != metadata.build.entries {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries,
            index.len()
        ))));
    }

    Ok(RuntimeLoadedArtifactBundle {
        metadata_path,
        bundle_dir,
        payload_path,
        file_digest_verified: file_digest.is_some(),
        timing: RuntimeLoadTiming {
            read_metadata,
            file_digest: file_digest_elapsed,
            decode_payload,
            canonical_checksum,
            total: load_start.elapsed(),
        },
        metadata,
        index,
    })
}

fn verify_loaded_artifact_payload_checksum(
    metadata: &UnidicArtifactMetadata,
    index: &UnidicReadingIndex,
) -> Result<(), Box<dyn Error>> {
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            CliError::ArtifactVerificationFailed(format!(
                "unsupported checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum != metadata.payload.checksum {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "payload checksum mismatch: metadata has {}, recomputed {}",
            metadata.payload.checksum, checksum
        ))));
    }
    Ok(())
}

fn dictionary_options_from_metadata(metadata: &UnidicArtifactMetadata) -> DictionaryReadingOptions {
    DictionaryReadingOptions {
        max_span_chars: metadata.query_defaults.max_span_chars,
        max_paths: metadata.query_defaults.max_paths,
        longest_match_only: metadata.query_defaults.longest_match_only,
        max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
    }
}

fn verify_payload_file_digest(
    metadata: &UnidicArtifactMetadata,
    payload_path: &Path,
) -> Result<Option<String>, Box<dyn Error>> {
    match (
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(algorithm), Some(expected)) => {
            if algorithm != ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "unsupported payload file digest algorithm {algorithm:?}"
                ))));
            }
            let digest = artifact_file_digest_path(payload_path)?;
            if digest != expected {
                return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
                    "payload file digest mismatch: metadata has {expected}, recomputed {digest}"
                ))));
            }
            Ok(Some(digest))
        }
        _ => Err(Box::new(CliError::ArtifactVerificationFailed(
            "payload file digest algorithm and digest must be provided together".to_string(),
        ))),
    }
}

fn release_archive_entries(
    verified: &VerifiedArtifactBundle,
) -> Result<Vec<ArchiveEntry>, Box<dyn Error>> {
    let mut entries = vec![
        ArchiveEntry {
            source: verified.metadata_path.clone(),
            path: "metadata.yaml".to_string(),
        },
        ArchiveEntry {
            source: verified.payload_path.clone(),
            path: normalized_relative_archive_path(&verified.metadata.payload.path)?,
        },
    ];
    for reference in &verified.metadata.license.references {
        entries.push(ArchiveEntry {
            source: checked_bundle_path(&verified.bundle_dir, &reference.path)?,
            path: normalized_relative_archive_path(&reference.path)?,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries.dedup_by(|left, right| left.path == right.path);
    Ok(entries)
}

fn checked_bundle_path(bundle_dir: &Path, relative_path: &str) -> Result<PathBuf, Box<dyn Error>> {
    let relative = Path::new(relative_path);
    if relative_path.contains('\\')
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "bundle path {relative_path:?} must be relative and stay inside the bundle"
        ))));
    }
    Ok(bundle_dir.join(relative))
}

fn normalized_relative_archive_path(relative_path: &str) -> Result<String, Box<dyn Error>> {
    checked_bundle_path(Path::new(""), relative_path)?;
    Ok(relative_path.to_string())
}

fn release_checksum_asset_label(path: &Path) -> Result<&str, Box<dyn Error>> {
    path.file_name()
        .and_then(|file_name| file_name.to_str())
        .filter(|file_name| !file_name.is_empty())
        .ok_or_else(|| {
            Box::new(CliError::ArtifactVerificationFailed(format!(
                "release asset path {} has no file name",
                path.display()
            ))) as Box<dyn Error>
        })
}

fn write_release_archive(
    writer: impl std::io::Write,
    compression: ArchiveCompression,
    root_name: &str,
    entries: &[ArchiveEntry],
) -> Result<(), Box<dyn Error>> {
    match compression {
        ArchiveCompression::None => {
            let mut writer = writer;
            write_tar_archive(&mut writer, root_name, entries)?;
        }
        ArchiveCompression::Gzip => {
            let mut encoder = gzip_encoder(writer);
            write_tar_archive(&mut encoder, root_name, entries)?;
            encoder.finish()?;
        }
        ArchiveCompression::Zstd => {
            let mut encoder = zstd::stream::write::Encoder::new(writer, ZSTD_COMPRESSION_LEVEL)?;
            write_tar_archive(&mut encoder, root_name, entries)?;
            encoder.finish()?;
        }
    }
    Ok(())
}

fn gzip_encoder(writer: impl std::io::Write) -> GzEncoder<impl std::io::Write> {
    GzBuilder::new()
        .mtime(0)
        .write(writer, Compression::default())
}

fn write_tar_archive(
    writer: &mut impl std::io::Write,
    root_name: &str,
    entries: &[ArchiveEntry],
) -> Result<(), Box<dyn Error>> {
    let root_name = sanitize_archive_root(root_name)?;
    for entry in entries {
        let data = fs::read(&entry.source)?;
        let archive_path = format!("{root_name}/{}", entry.path);
        write_tar_file_entry(writer, &archive_path, &data)?;
    }
    writer.write_all(&[0_u8; 1024])?;
    Ok(())
}

fn sanitize_archive_root(root_name: &str) -> Result<String, Box<dyn Error>> {
    if root_name.is_empty()
        || root_name == "."
        || root_name == ".."
        || root_name.contains('/')
        || root_name.contains('\\')
    {
        return Err(Box::new(CliError::InvalidArgumentValue {
            name: "--root-name",
            value: root_name.to_string(),
            expected: "single relative path component",
        }));
    }
    Ok(root_name.to_string())
}

fn write_tar_file_entry(
    writer: &mut impl std::io::Write,
    path: &str,
    data: &[u8],
) -> Result<(), Box<dyn Error>> {
    let mut header = [0_u8; 512];
    write_tar_name(&mut header, path)?;
    write_tar_octal(&mut header[100..108], 0o644)?;
    write_tar_octal(&mut header[108..116], 0)?;
    write_tar_octal(&mut header[116..124], 0)?;
    write_tar_octal(&mut header[124..136], data.len() as u64)?;
    write_tar_octal(&mut header[136..148], 0)?;
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum = header.iter().map(|byte| u32::from(*byte)).sum::<u32>() as u64;
    write_tar_checksum(&mut header[148..156], checksum)?;

    writer.write_all(&header)?;
    writer.write_all(data)?;
    let padding = (512 - (data.len() % 512)) % 512;
    if padding > 0 {
        writer.write_all(&vec![0_u8; padding])?;
    }
    Ok(())
}

fn write_tar_name(header: &mut [u8; 512], path: &str) -> Result<(), Box<dyn Error>> {
    let bytes = path.as_bytes();
    if bytes.len() > 100 {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "archive path {path:?} exceeds the current 100-byte tar name limit"
        ))));
    }
    header[..bytes.len()].copy_from_slice(bytes);
    Ok(())
}

fn write_tar_octal(field: &mut [u8], value: u64) -> Result<(), Box<dyn Error>> {
    let digits = field.len() - 1;
    let text = format!("{value:0digits$o}");
    if text.len() > digits {
        return Err(Box::new(CliError::ArtifactVerificationFailed(format!(
            "tar numeric field overflow for value {value}"
        ))));
    }
    field[..digits].copy_from_slice(text.as_bytes());
    field[digits] = 0;
    Ok(())
}

fn write_tar_checksum(field: &mut [u8], value: u64) -> Result<(), Box<dyn Error>> {
    let text = format!("{value:06o}\0 ");
    if text.len() != field.len() {
        return Err(Box::new(CliError::ArtifactVerificationFailed(
            "tar checksum field overflow".to_string(),
        )));
    }
    field.copy_from_slice(text.as_bytes());
    Ok(())
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn parse_pinyin_view(value: &str) -> Result<PinyinView, CliError> {
    match value {
        "no-tone" | "notone" | "normal" => Ok(PinyinView::NoTone),
        "tone3" => Ok(PinyinView::Tone3),
        _ => Err(CliError::InvalidArgumentValue {
            name: "--pinyin-view",
            value: value.to_string(),
            expected: "no-tone or tone3",
        }),
    }
}

fn parse_unidic_reading_field(value: &str) -> Result<UnidicReadingField, CliError> {
    match value {
        "lform" => Ok(UnidicReadingField::LForm),
        "pron" => Ok(UnidicReadingField::Pron),
        _ => Err(CliError::InvalidArgumentValue {
            name: "--field",
            value: value.to_string(),
            expected: "lform or pron",
        }),
    }
}

fn parse_usize_argument(name: &'static str, value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidArgumentValue {
            name,
            value: value.to_string(),
            expected: "non-negative integer",
        })
}

fn parse_u64_argument(name: &'static str, value: &str) -> Result<u64, CliError> {
    value
        .parse::<u64>()
        .map_err(|_| CliError::InvalidArgumentValue {
            name,
            value: value.to_string(),
            expected: "non-negative integer",
        })
}

fn parse_jwtd_negative_policy(value: &str) -> Result<JwtdNegativePolicy, CliError> {
    match value {
        "length" => Ok(JwtdNegativePolicy::Length),
        "surface-hard" => Ok(JwtdNegativePolicy::SurfaceHard),
        _ => Err(CliError::InvalidArgumentValue {
            name: "--negative-policy",
            value: value.to_string(),
            expected: "length or surface-hard",
        }),
    }
}

fn parse_jwtd_tie_policy(value: &str) -> Result<JwtdTiePolicy, CliError> {
    match value {
        "stable" => Ok(JwtdTiePolicy::Stable),
        "pessimistic" => Ok(JwtdTiePolicy::Pessimistic),
        _ => Err(CliError::InvalidArgumentValue {
            name: "--tie-policy",
            value: value.to_string(),
            expected: "stable or pessimistic",
        }),
    }
}

fn unidic_reading_field_name(field: UnidicReadingField) -> &'static str {
    field.as_str()
}

fn max_readings_per_surface_label(max_readings: Option<usize>) -> String {
    max_readings
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unbounded".to_string())
}

fn max_readings_per_segment_label(max_readings: Option<usize>) -> String {
    max_readings
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unbounded".to_string())
}

impl UnidicReadingsOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut text = None;
        let mut dic_dir = None;
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--text" => {
                    text = Some(value_after(&args, i, "--text")?);
                    i += 2;
                }
                "--dic-dir" => {
                    dic_dir = Some(value_after(&args, i, "--dic-dir")?);
                    i += 2;
                }
                "-h" | "--help" => {
                    print_unidic_readings_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            text: text.ok_or(CliError::MissingArgument("--text"))?,
            dic_dir: dic_dir.ok_or(CliError::MissingArgument("--dic-dir"))?,
        })
    }
}

impl CompareOptions {
    fn parse(args: Vec<String>) -> Result<Self, CliError> {
        let mut left = None;
        let mut right = None;
        let mut overrides = None;
        let mut lex_csv = None;
        let mut artifact_payload = None;
        let mut artifact_metadata = None;
        let mut payload_format = ArtifactPayloadFormat::Yaml;
        let mut index_options = UnidicIndexOptions::default();
        let mut dictionary_options = DictionaryReadingOptions::default();
        let mut dictionary_option_overrides = DictionaryReadingOptionOverrides::default();
        let mut i = 0;

        while i < args.len() {
            match args[i].as_str() {
                "--left" => {
                    left = Some(value_after(&args, i, "--left")?);
                    i += 2;
                }
                "--right" => {
                    right = Some(value_after(&args, i, "--right")?);
                    i += 2;
                }
                "--overrides" => {
                    overrides = Some(value_after(&args, i, "--overrides")?);
                    i += 2;
                }
                "--lex-csv" => {
                    lex_csv = Some(value_after(&args, i, "--lex-csv")?);
                    i += 2;
                }
                "--artifact-payload" => {
                    artifact_payload = Some(value_after(&args, i, "--artifact-payload")?);
                    i += 2;
                }
                "--artifact-metadata" => {
                    artifact_metadata = Some(value_after(&args, i, "--artifact-metadata")?);
                    i += 2;
                }
                "--payload-format" => {
                    payload_format =
                        parse_artifact_payload_format(&value_after(&args, i, "--payload-format")?)?;
                    i += 2;
                }
                "--field" => {
                    index_options.reading_field =
                        parse_unidic_reading_field(&value_after(&args, i, "--field")?)?;
                    i += 2;
                }
                "--max-readings-per-surface" => {
                    index_options.max_readings_per_surface = Some(parse_usize_argument(
                        "--max-readings-per-surface",
                        &value_after(&args, i, "--max-readings-per-surface")?,
                    )?);
                    i += 2;
                }
                "--max-readings-per-segment" => {
                    let value = parse_usize_argument(
                        "--max-readings-per-segment",
                        &value_after(&args, i, "--max-readings-per-segment")?,
                    )?;
                    dictionary_options.max_readings_per_segment = Some(value);
                    dictionary_option_overrides.max_readings_per_segment = Some(value);
                    i += 2;
                }
                "--include-ascii-surfaces" => {
                    index_options.exclude_ascii_surfaces = false;
                    i += 1;
                }
                "--include-symbol-pos" => {
                    index_options.exclude_symbol_pos = false;
                    i += 1;
                }
                "--max-span-chars" => {
                    let value = parse_usize_argument(
                        "--max-span-chars",
                        &value_after(&args, i, "--max-span-chars")?,
                    )?;
                    dictionary_options.max_span_chars = value;
                    dictionary_option_overrides.max_span_chars = Some(value);
                    i += 2;
                }
                "--max-paths" => {
                    let value = parse_usize_argument(
                        "--max-paths",
                        &value_after(&args, i, "--max-paths")?,
                    )?;
                    dictionary_options.max_paths = value;
                    dictionary_option_overrides.max_paths = Some(value);
                    i += 2;
                }
                "--longest-only" => {
                    dictionary_options.longest_match_only = true;
                    dictionary_option_overrides.longest_match_only = true;
                    i += 1;
                }
                "-h" | "--help" => {
                    print_compare_usage();
                    return Err(CliError::HelpRequested);
                }
                arg => return Err(CliError::UnknownArgument(arg.to_string())),
            }
        }

        Ok(Self {
            left: left.ok_or(CliError::MissingArgument("--left"))?,
            right: right.ok_or(CliError::MissingArgument("--right"))?,
            overrides,
            lex_csv,
            artifact_payload,
            artifact_metadata,
            payload_format,
            index_options,
            dictionary_options,
            dictionary_option_overrides,
        })
    }
}

fn value_after(args: &[String], index: usize, name: &'static str) -> Result<String, CliError> {
    args.get(index + 1)
        .cloned()
        .ok_or(CliError::MissingArgumentValue(name))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CliError {
    MissingCommand,
    MissingComparisonMethod,
    UnknownCommand(String),
    UnknownArgument(String),
    ConflictingArguments(&'static str, &'static str),
    MissingArgument(&'static str),
    MissingArgumentValue(&'static str),
    InvalidArgumentValue {
        name: &'static str,
        value: String,
        expected: &'static str,
    },
    CommandFailed {
        command: String,
        status: Option<i32>,
        stderr: String,
    },
    JwtdJsonLine {
        path: String,
        line: usize,
        message: String,
    },
    ArtifactVerificationFailed(String),
    HelpRequested,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand => write!(f, "missing command"),
            Self::MissingComparisonMethod => {
                write!(
                    f,
                    "compare requires --overrides, --lex-csv, --artifact-payload, --artifact-metadata, or a combination"
                )
            }
            Self::UnknownCommand(command) => write!(f, "unknown command {command:?}"),
            Self::UnknownArgument(arg) => write!(f, "unknown argument {arg:?}"),
            Self::ConflictingArguments(left, right) => {
                write!(f, "arguments {left} and {right} cannot be used together")
            }
            Self::MissingArgument(arg) => write!(f, "missing required argument {arg}"),
            Self::MissingArgumentValue(arg) => write!(f, "missing value for argument {arg}"),
            Self::InvalidArgumentValue {
                name,
                value,
                expected,
            } => write!(
                f,
                "invalid value {value:?} for argument {name}; expected {expected}"
            ),
            Self::CommandFailed {
                command,
                status,
                stderr,
            } => write!(
                f,
                "command {command:?} failed with status {status:?}: {stderr}"
            ),
            Self::JwtdJsonLine {
                path,
                line,
                message,
            } => write!(f, "failed to parse JWTD JSONL {path}:{line}: {message}"),
            Self::ArtifactVerificationFailed(message) => {
                write!(f, "artifact verification failed: {message}")
            }
            Self::HelpRequested => write!(f, "help requested"),
        }
    }
}

impl Error for CliError {}

fn print_usage() {
    eprintln!("usage:");
    print_cedict_readings_usage();
    print_cedict_sequences_usage();
    print_chinese_compare_usage();
    print_compare_usage();
    print_download_usage();
    print_list_usage();
    print_where_usage();
    print_zh_artifact_archive_usage();
    print_zh_artifact_bundle_usage();
    print_zh_artifact_inspect_usage();
    print_zh_artifact_metadata_usage();
    print_zh_artifact_payload_usage();
    print_zh_artifact_release_checksums_usage();
    print_zh_artifact_verify_usage();
    print_japanese_report_usage();
    print_jwtd_scorer_report_usage();
    print_jwtd_summary_usage();
    print_unidic_artifact_archive_usage();
    print_unidic_artifact_binary_inspect_usage();
    print_unidic_artifact_binary_payload_usage();
    print_unidic_artifact_bundle_usage();
    print_unidic_artifact_metadata_usage();
    print_unidic_artifact_inspect_usage();
    print_unidic_artifact_payload_usage();
    print_unidic_artifact_release_checksums_usage();
    print_unidic_artifact_runtime_measure_usage();
    print_unidic_artifact_verify_usage();
    print_unidic_csv_readings_usage();
    print_unidic_csv_sequences_usage();
    print_unidic_readings_usage();
}

fn print_cedict_readings_usage() {
    eprintln!(
        "  moine cedict-readings --surface <TEXT> --cedict <PATH_TO_CC_CEDICT> [--pinyin-view no-tone|tone3] [--max-readings-per-surface N]"
    );
}

fn print_cedict_sequences_usage() {
    eprintln!(
        "  moine cedict-sequences --text <TEXT> --cedict <PATH_TO_CC_CEDICT> [--pinyin-view no-tone|tone3] [--max-readings-per-surface N] [--max-readings-per-segment N] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_chinese_compare_usage() {
    eprintln!(
        "  moine chinese-compare --left <TEXT> --right <TEXT> (--cedict <PATH_TO_CC_CEDICT> | --artifact-payload <PATH_TO_PAYLOAD> [--payload-format yaml|indexed] | --artifact-metadata <PATH_TO_METADATA_YAML>) [--pinyin-view no-tone|tone3] [--max-readings-per-surface N] [--max-readings-per-segment N] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_zh_artifact_bundle_usage() {
    eprintln!(
        "  moine zh-artifact-bundle --cedict <PATH_TO_CC_CEDICT> --source-version <VERSION> --output-dir <DIR> [--artifact-name <NAME>] [--payload-format yaml|indexed] [--source-name <NAME>] [--license-file <PATH>] [--pinyin-view no-tone|tone3] [--max-readings-per-surface N] [--max-readings-per-segment N] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_zh_artifact_archive_usage() {
    eprintln!(
        "  moine zh-artifact-archive --metadata <PATH_TO_METADATA_YAML> --output <PATH_TO_TAR> [--bundle-dir <DIR>] [--root-name <NAME>] [--compression none|gzip|zstd]"
    );
}

fn print_zh_artifact_inspect_usage() {
    eprintln!(
        "  moine zh-artifact-inspect --payload <PATH_TO_PAYLOAD> [--payload-format yaml|indexed]"
    );
}

fn print_zh_artifact_metadata_usage() {
    eprintln!(
        "  moine zh-artifact-metadata --cedict <PATH_TO_CC_CEDICT> --source-version <VERSION> [--output <PATH>] [--artifact-name <NAME>] [--payload-format yaml|indexed] [--payload-file-name <NAME>] [--source-name <NAME>] [--pinyin-view no-tone|tone3] [--max-readings-per-surface N] [--max-readings-per-segment N] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_zh_artifact_payload_usage() {
    eprintln!(
        "  moine zh-artifact-payload --cedict <PATH_TO_CC_CEDICT> [--output <PATH>] [--payload-format yaml|indexed] [--pinyin-view no-tone|tone3] [--max-readings-per-surface N]"
    );
}

fn print_zh_artifact_release_checksums_usage() {
    eprintln!(
        "  moine zh-artifact-release-checksums --asset <PATH_TO_RELEASE_ASSET>... [--output <PATH_TO_SHA256SUMS>]"
    );
}

fn print_zh_artifact_verify_usage() {
    eprintln!("  moine zh-artifact-verify --metadata <PATH_TO_METADATA_YAML> [--bundle-dir <DIR>]");
}

fn print_unidic_artifact_archive_usage() {
    eprintln!(
        "  moine unidic-artifact-archive --metadata <PATH_TO_METADATA_YAML> --output <PATH_TO_TAR> [--bundle-dir <DIR>] [--root-name <NAME>] [--compression none|gzip|zstd]"
    );
}

fn print_compare_usage() {
    eprintln!(
        "  moine compare --left <TEXT> --right <TEXT> [--overrides <PATH_TO_OVERRIDES_YAML>] [--lex-csv <PATH_TO_LEX_CSV> | --artifact-payload <PATH> [--payload-format yaml|binary|indexed] | --artifact-metadata <PATH_TO_METADATA_YAML>] [--field lform|pron] [--max-readings-per-surface N] [--max-readings-per-segment N] [--include-ascii-surfaces] [--include-symbol-pos] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_download_usage() {
    eprintln!(
        "  moine download <ja|zh> [--cache-dir <DIR>] [--force] [--url <URL_OR_PATH>] [--checksum-url <URL_OR_PATH>] [--sha256 <HEX>]"
    );
}

fn print_list_usage() {
    eprintln!("  moine list [--cache-dir <DIR>]");
}

fn print_where_usage() {
    eprintln!("  moine where [ja|zh] [--cache-dir <DIR>]");
}

fn print_japanese_report_usage() {
    eprintln!(
        "  moine japanese-report --overrides <PATH_TO_OVERRIDES_YAML> --lex-csv <PATH_TO_LEX_CSV> [--output <PATH>] [--field lform|pron] [--max-readings-per-surface N] [--max-readings-per-segment N] [--include-ascii-surfaces] [--include-symbol-pos] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_jwtd_summary_usage() {
    eprintln!(
        "  moine jwtd-summary --split <NAME> <PATH_TO_JSONL>... [--output <PATH_TO_MARKDOWN>]"
    );
}

fn print_jwtd_scorer_report_usage() {
    eprintln!(
        "  moine jwtd-scorer-report --split <NAME> <PATH_TO_JSONL> [--artifact-metadata <PATH_TO_METADATA_YAML>] [--bundle-dir <DIR>] [--negative-policy length|surface-hard] [--negative-count N] [--tie-policy stable|pessimistic] [--seed N] [--max-examples N | --all-examples] [--output <PATH_TO_MARKDOWN>]"
    );
}

fn print_unidic_artifact_binary_inspect_usage() {
    eprintln!(
        "  moine unidic-artifact-binary-inspect --payload <PATH_TO_BINARY_PAYLOAD> [--timing]"
    );
}

fn print_unidic_artifact_binary_payload_usage() {
    eprintln!(
        "  moine unidic-artifact-binary-payload --lex-csv <PATH_TO_LEX_CSV> --output <PATH> [--field lform|pron] [--max-readings-per-surface N] [--include-ascii-surfaces] [--include-symbol-pos]"
    );
}

fn print_unidic_artifact_bundle_usage() {
    eprintln!(
        "  moine unidic-artifact-bundle --lex-csv <PATH_TO_LEX_CSV> --source-version <VERSION> --output-dir <DIR> [--artifact-name <NAME>] [--payload-format yaml|binary|indexed] [--source-name <NAME>] [--license-dir <DIR>] [--field lform|pron] [--max-readings-per-surface N] [--max-readings-per-segment N] [--include-ascii-surfaces] [--include-symbol-pos] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_unidic_artifact_metadata_usage() {
    eprintln!(
        "  moine unidic-artifact-metadata --lex-csv <PATH_TO_LEX_CSV> --source-version <VERSION> [--output <PATH>] [--artifact-name <NAME>] [--payload-format yaml|binary|indexed] [--payload-file-name <NAME>] [--source-name <NAME>] [--field lform|pron] [--max-readings-per-surface N] [--max-readings-per-segment N] [--include-ascii-surfaces] [--include-symbol-pos] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

fn print_unidic_artifact_inspect_usage() {
    eprintln!("  moine unidic-artifact-inspect --payload <PATH_TO_PAYLOAD_YAML>");
}

fn print_unidic_artifact_payload_usage() {
    eprintln!(
        "  moine unidic-artifact-payload --lex-csv <PATH_TO_LEX_CSV> [--output <PATH>] [--field lform|pron] [--max-readings-per-surface N] [--include-ascii-surfaces] [--include-symbol-pos]"
    );
}

fn print_unidic_artifact_release_checksums_usage() {
    eprintln!(
        "  moine unidic-artifact-release-checksums --asset <PATH_TO_RELEASE_ASSET>... [--output <PATH_TO_SHA256SUMS>]"
    );
}

fn print_unidic_artifact_runtime_measure_usage() {
    eprintln!(
        "  moine unidic-artifact-runtime-measure --metadata <PATH_TO_METADATA_YAML> [--bundle-dir <DIR>] (--left <TEXT> --right <TEXT> | --pair <LEFT> <RIGHT>...) [--warmups N] [--iterations N]"
    );
}

fn print_unidic_artifact_verify_usage() {
    eprintln!(
        "  moine unidic-artifact-verify --metadata <PATH_TO_METADATA_YAML> [--bundle-dir <DIR>] [--canonical-checksum]"
    );
}

fn print_unidic_readings_usage() {
    eprintln!("  moine unidic-readings --text <TEXT> --dic-dir <PATH_TO_COMPILED_UNIDIC>");
}

fn print_unidic_csv_readings_usage() {
    eprintln!(
        "  moine unidic-csv-readings --surface <TEXT> --lex-csv <PATH_TO_LEX_CSV> [--field lform|pron] [--max-readings-per-surface N] [--include-ascii-surfaces] [--include-symbol-pos]"
    );
}

fn print_unidic_csv_sequences_usage() {
    eprintln!(
        "  moine unidic-csv-sequences --text <TEXT> --lex-csv <PATH_TO_LEX_CSV> [--field lform|pron] [--max-readings-per-surface N] [--max-readings-per-segment N] [--include-ascii-surfaces] [--include-symbol-pos] [--max-span-chars N] [--max-paths N] [--longest-only]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mecab_lform_as_reading() {
        let output = "印刷\t名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,印刷,インサツ,インサツ,漢,*,*,*,*\nEOS\n";
        let tokens = parse_mecab_tokens(output);

        assert_eq!(
            tokens,
            vec![MecabToken {
                surface: "印刷".to_string(),
                reading: Some("インサツ".to_string()),
            }]
        );
    }

    #[test]
    fn skips_unknown_reading_marker() {
        let output = "マリトッツォ\t名詞,普通名詞,一般,*,*,*,*\nEOS\n";
        let tokens = parse_mecab_tokens(output);

        assert_eq!(
            tokens,
            vec![MecabToken {
                surface: "マリトッツォ".to_string(),
                reading: None,
            }]
        );
    }

    #[test]
    fn write_output_file_creates_parent_directories() {
        let temp = TempDir::new("moine-cli-test").unwrap();
        let output_path = temp.path().join("reports").join("nested").join("report.md");

        write_output_file(&output_path, "ok\n").unwrap();

        assert_eq!(fs::read_to_string(output_path).unwrap(), "ok\n");
    }

    #[test]
    fn create_output_file_creates_parent_directories() {
        let temp = TempDir::new("moine-cli-test").unwrap();
        let output_path = temp.path().join("artifacts").join("payload.bin");

        let mut file = create_output_file(&output_path).unwrap();
        file.write_all(b"ok").unwrap();
        drop(file);

        assert_eq!(fs::read(output_path).unwrap(), b"ok");
    }

    #[test]
    fn parses_download_options() {
        let options = DownloadCliOptions::parse(vec![
            "zh".to_string(),
            "--url".to_string(),
            "/tmp/moine-cedict.tar.gz".to_string(),
            "--checksum-url".to_string(),
            "/tmp/SHA256SUMS".to_string(),
            "--cache-dir".to_string(),
            "/tmp/moine-cache".to_string(),
            "--force".to_string(),
        ])
        .unwrap();

        assert_eq!(options.spec.language, ArtifactLanguage::Chinese);
        assert_eq!(options.spec.artifact_name, "moine-cedict-20260520");
        assert_eq!(options.url, Some("/tmp/moine-cedict.tar.gz".to_string()));
        assert_eq!(options.checksum_url, Some("/tmp/SHA256SUMS".to_string()));
        assert_eq!(options.cache_dir, Some("/tmp/moine-cache".to_string()));
        assert!(options.force);
    }

    #[test]
    fn parses_cache_lookup_options() {
        let list = CacheCliOptions::parse(vec![
            "--cache-dir".to_string(),
            "/tmp/moine-cache".to_string(),
        ])
        .unwrap();
        let where_options = WhereCliOptions::parse(vec![
            "ja".to_string(),
            "--cache-dir".to_string(),
            "/tmp/moine-cache".to_string(),
        ])
        .unwrap();

        assert_eq!(list.cache_dir, Some("/tmp/moine-cache".to_string()));
        assert_eq!(where_options.language, Some(ArtifactLanguage::Japanese));
        assert_eq!(
            where_options.cache_dir,
            Some("/tmp/moine-cache".to_string())
        );
    }

    #[test]
    fn extract_artifact_archive_rejects_links() {
        let temp = TempDir::new("moine-cli-test").unwrap();
        let archive_path = temp.path().join("unsafe.tar.gz");
        {
            let file = fs::File::create(&archive_path).unwrap();
            let encoder = GzEncoder::new(file, Compression::default());
            let mut builder = tar::Builder::new(encoder);
            fs::create_dir(temp.path().join("bundle")).unwrap();
            builder
                .append_dir("bundle", temp.path().join("bundle"))
                .unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(0);
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_cksum();
            builder
                .append_link(&mut header, "bundle/payload", "../outside")
                .unwrap();
            builder.finish().unwrap();
        }

        let err = extract_artifact_archive(&archive_path, &temp.path().join("extract"))
            .expect_err("symlink entries should be rejected");
        assert!(err.to_string().contains("unsupported archive entry type"));
    }

    #[test]
    fn parses_jwtd_summary_options() {
        let options = JwtdSummaryOptions::parse(vec![
            "--split".to_string(),
            "train".to_string(),
            "jwtd_v2.0/train.jsonl".to_string(),
            "--split".to_string(),
            "test".to_string(),
            "jwtd_v2.0/test.jsonl".to_string(),
            "--output".to_string(),
            "/tmp/jwtd_summary.md".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.splits,
            vec![
                JwtdSplitInput {
                    name: "train".to_string(),
                    path: "jwtd_v2.0/train.jsonl".to_string(),
                },
                JwtdSplitInput {
                    name: "test".to_string(),
                    path: "jwtd_v2.0/test.jsonl".to_string(),
                },
            ]
        );
        assert_eq!(options.output, Some("/tmp/jwtd_summary.md".to_string()));
    }

    #[test]
    fn parses_jwtd_scorer_report_options() {
        let options = JwtdScorerReportOptions::parse(vec![
            "--split".to_string(),
            "test".to_string(),
            "jwtd_v2.0/test.jsonl".to_string(),
            "--negative-policy".to_string(),
            "surface-hard".to_string(),
            "--artifact-metadata".to_string(),
            "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
            "--bundle-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--negative-count".to_string(),
            "10".to_string(),
            "--tie-policy".to_string(),
            "pessimistic".to_string(),
            "--seed".to_string(),
            "42".to_string(),
            "--max-examples".to_string(),
            "100".to_string(),
            "--output".to_string(),
            "/tmp/jwtd_scorer.md".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.split,
            JwtdSplitInput {
                name: "test".to_string(),
                path: "jwtd_v2.0/test.jsonl".to_string(),
            }
        );
        assert_eq!(
            options.artifact_metadata,
            Some("dist/moine-unidic-cwj-202512/metadata.yaml".to_string())
        );
        assert_eq!(
            options.bundle_dir,
            Some("dist/moine-unidic-cwj-202512".to_string())
        );
        assert_eq!(options.negative_policy, JwtdNegativePolicy::SurfaceHard);
        assert_eq!(options.negative_count, 10);
        assert_eq!(options.tie_policy, JwtdTiePolicy::Pessimistic);
        assert_eq!(options.seed, 42);
        assert_eq!(options.max_examples, Some(100));
        assert_eq!(options.output, Some("/tmp/jwtd_scorer.md".to_string()));
    }

    #[test]
    fn parses_cedict_readings_options() {
        let options = CedictReadingsOptions::parse(vec![
            "--surface".to_string(),
            "威士忌".to_string(),
            "--cedict".to_string(),
            "cedict_1_0_ts_utf-8_mdbg.txt".to_string(),
            "--pinyin-view".to_string(),
            "tone3".to_string(),
            "--max-readings-per-surface".to_string(),
            "4".to_string(),
        ])
        .unwrap();

        assert_eq!(options.surface, "威士忌");
        assert_eq!(options.cedict, "cedict_1_0_ts_utf-8_mdbg.txt");
        assert_eq!(options.index_options.pinyin_view, PinyinView::Tone3);
        assert_eq!(options.index_options.max_readings_per_surface, Some(4));
    }

    #[test]
    fn parses_cedict_sequences_options() {
        let options = CedictSequencesOptions::parse(vec![
            "--text".to_string(),
            "布納哈本".to_string(),
            "--cedict".to_string(),
            "cedict_1_0_ts_utf-8_mdbg.txt".to_string(),
            "--max-readings-per-segment".to_string(),
            "8".to_string(),
            "--max-paths".to_string(),
            "128".to_string(),
            "--longest-only".to_string(),
        ])
        .unwrap();

        assert_eq!(options.text, "布納哈本");
        assert_eq!(options.index_options.pinyin_view, PinyinView::NoTone);
        assert_eq!(options.reading_options.max_readings_per_segment, Some(8));
        assert_eq!(options.reading_options.max_paths, 128);
        assert!(options.reading_options.longest_match_only);
    }

    #[test]
    fn parses_chinese_compare_options() {
        let options = ChineseCompareOptions::parse(vec![
            "--left".to_string(),
            "weishiji".to_string(),
            "--right".to_string(),
            "威士忌".to_string(),
            "--cedict".to_string(),
            "cedict_1_0_ts_utf-8_mdbg.txt".to_string(),
        ])
        .unwrap();

        assert_eq!(options.left, "weishiji");
        assert_eq!(options.right, "威士忌");
        assert_eq!(
            options.source,
            ZhIndexSource::Cedict("cedict_1_0_ts_utf-8_mdbg.txt".to_string())
        );
        assert_eq!(options.index_options.pinyin_view, PinyinView::NoTone);
    }

    #[test]
    fn parses_chinese_compare_artifact_metadata_options() {
        let options = ChineseCompareOptions::parse(vec![
            "--left".to_string(),
            "布那哈本".to_string(),
            "--right".to_string(),
            "布納哈本".to_string(),
            "--artifact-metadata".to_string(),
            "dist/moine-cedict/metadata.yaml".to_string(),
            "--max-paths".to_string(),
            "128".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.source,
            ZhIndexSource::ArtifactMetadata("dist/moine-cedict/metadata.yaml".to_string())
        );
        assert_eq!(options.reading_options.max_paths, 128);
    }

    #[test]
    fn parses_zh_artifact_bundle_options() {
        let options = ZhArtifactBundleCliOptions::parse(vec![
            "--cedict".to_string(),
            "cedict_1_0_ts_utf-8_mdbg.txt".to_string(),
            "--source-version".to_string(),
            "2026-05-20".to_string(),
            "--output-dir".to_string(),
            "dist/moine-cedict".to_string(),
            "--artifact-name".to_string(),
            "moine-cedict".to_string(),
            "--pinyin-view".to_string(),
            "tone3".to_string(),
            "--max-readings-per-surface".to_string(),
            "4".to_string(),
            "--max-readings-per-segment".to_string(),
            "8".to_string(),
            "--longest-only".to_string(),
        ])
        .unwrap();

        assert_eq!(options.cedict, "cedict_1_0_ts_utf-8_mdbg.txt");
        assert_eq!(options.output_dir, "dist/moine-cedict");
        assert_eq!(options.artifact_name, "moine-cedict");
        assert_eq!(options.payload_format, ArtifactPayloadFormat::Indexed);
        assert_eq!(options.index_options.pinyin_view, PinyinView::Tone3);
        assert_eq!(options.index_options.max_readings_per_surface, Some(4));
        assert_eq!(options.reading_options.max_readings_per_segment, Some(8));
        assert!(options.reading_options.longest_match_only);
    }

    #[test]
    fn parses_zh_artifact_metadata_options() {
        let options = ZhArtifactMetadataCliOptions::parse(vec![
            "--cedict".to_string(),
            "cedict_1_0_ts_utf-8_mdbg.txt".to_string(),
            "--source-version".to_string(),
            "2026-05-20".to_string(),
            "--artifact-name".to_string(),
            "moine-cedict".to_string(),
        ])
        .unwrap();

        assert_eq!(options.artifact_name, "moine-cedict");
        assert_eq!(options.payload_file_name, "moine-cedict.readings.moineidx");
        assert_eq!(options.payload_format, ArtifactPayloadFormat::Indexed);
        assert_eq!(options.source_name, "CC-CEDICT");
    }

    #[test]
    fn parses_zh_artifact_archive_options() {
        let options = ZhArtifactArchiveCliOptions::parse(vec![
            "--metadata".to_string(),
            "dist/moine-cedict/metadata.yaml".to_string(),
            "--output".to_string(),
            "dist/moine-cedict.tar.gz".to_string(),
            "--compression".to_string(),
            "gzip".to_string(),
        ])
        .unwrap();

        assert_eq!(options.metadata, "dist/moine-cedict/metadata.yaml");
        assert_eq!(options.output, "dist/moine-cedict.tar.gz");
        assert_eq!(options.compression, ArchiveCompression::Gzip);
    }

    #[test]
    fn jwtd_summary_counts_diffs_and_empty_spans() {
        let temp_dir =
            std::env::temp_dir().join(format!("moine-jwtd-summary-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("sample.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"diffs":[{"pre_str":"固体","post_str":"個体","category":"kanji-conversion_a"},{"pre_str":"","post_str":"追記","category":"insertion"}]}"#,
                "\n",
                r#"{"diffs":[{"pre_str":"いんさt","post_str":"印刷","category":"kanji-conversion_b"},{"pre_str":"削除","post_str":"","category":"deletion"}]}"#,
                "\n",
                r#"{"page":"synthetic-no-diffs"}"#,
                "\n",
            ),
        )
        .unwrap();

        let summary = summarize_jwtd_split(&JwtdSplitInput {
            name: "sample".to_string(),
            path: path.display().to_string(),
        })
        .unwrap();

        assert_eq!(summary.records, 3);
        assert_eq!(summary.records_with_diffs, 2);
        assert_eq!(summary.diffs, 4);
        assert_eq!(summary.nonempty_pairs, 2);
        assert_eq!(summary.empty_pre, 1);
        assert_eq!(summary.empty_post, 1);
        assert_eq!(summary.empty_both, 0);
        assert_eq!(summary.category_counts.get("kanji-conversion_a"), Some(&1));
        assert_eq!(
            summary.nonempty_category_counts.get("kanji-conversion_b"),
            Some(&1)
        );
        assert_eq!(summary.pair_length_buckets.get("2"), Some(&1));
        assert_eq!(summary.pair_length_buckets.get("3-4"), Some(&1));

        let report = render_jwtd_summary_report(&[summary]);
        assert!(report.contains("scorer-only JWTD benchmark"));
        assert!(report.contains("| `sample` | 3 | 2 | 4 | 2 | 1 | 1 | 0 |"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn jwtd_length_negatives_are_deterministic_and_length_near() {
        let example = JwtdExample {
            query: "いんさt".to_string(),
            gold: "印刷".to_string(),
            category: "kanji-conversion_b".to_string(),
        };
        let candidate_pool = vec![
            "印刷".to_string(),
            "個体".to_string(),
            "東京都".to_string(),
            "インターンシップ".to_string(),
            "刃".to_string(),
        ];

        let first = jwtd_length_negatives(&example, &candidate_pool, 3, 7, 0);
        let second = jwtd_length_negatives(&example, &candidate_pool, 3, 7, 0);

        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
        assert!(!first.contains(&"印刷".to_string()));
        assert!(first.contains(&"個体".to_string()));
    }

    #[test]
    fn jwtd_surface_hard_negatives_prefer_surface_close_candidates() {
        let example = JwtdExample {
            query: "abcd".to_string(),
            gold: "wxyz".to_string(),
            category: "synthetic".to_string(),
        };
        let candidate_pool = vec![
            "wxyz".to_string(),
            "abce".to_string(),
            "abdd".to_string(),
            "zzzz".to_string(),
            "長い候補".to_string(),
        ];

        let negatives = jwtd_surface_hard_negatives(&example, &candidate_pool, 2, 0, 0);

        assert_eq!(negatives.len(), 2);
        assert!(negatives.contains(&"abce".to_string()));
        assert!(negatives.contains(&"abdd".to_string()));
        assert!(!negatives.contains(&"wxyz".to_string()));
    }

    #[test]
    fn jwtd_lped_scorer_uses_dictionary_readings() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let context = JwtdLpedContext {
            index: &index,
            options: DictionaryReadingOptions::default(),
        };

        assert_eq!(
            JwtdScorer::Lped
                .score("いんさt", "印刷", Some(&context))
                .unwrap(),
            1
        );
        assert_eq!(
            JwtdScorer::CombinedSurfaceDamerauLped
                .score("いんさt", "印刷", Some(&context))
                .unwrap(),
            1
        );
        assert_eq!(
            JwtdScorer::Lped
                .score("阮", "印刷", Some(&context))
                .unwrap(),
            JWTD_UNSCORABLE_DISTANCE
        );
        assert_eq!(
            JwtdScorer::CombinedSurfaceDamerauLped
                .score("阮", "印刷", Some(&context))
                .unwrap(),
            damerau_levenshtein_str("阮", "印刷")
        );
    }

    #[test]
    fn jwtd_gold_rank_supports_stable_and_pessimistic_ties() {
        let example = JwtdExample {
            query: "ab".to_string(),
            gold: "ac".to_string(),
            category: "synthetic".to_string(),
        };
        let candidates = vec!["bb".to_string(), "ac".to_string(), "ad".to_string()];

        assert_eq!(
            jwtd_gold_rank_with_context(
                &example,
                &candidates,
                JwtdScorer::SurfaceLevenshtein,
                JwtdTiePolicy::Stable,
                None,
            )
            .unwrap(),
            1
        );
        assert_eq!(
            jwtd_gold_rank_with_context(
                &example,
                &candidates,
                JwtdScorer::SurfaceLevenshtein,
                JwtdTiePolicy::Pessimistic,
                None,
            )
            .unwrap(),
            3
        );
    }

    #[test]
    fn jwtd_scorer_report_renders_surface_metrics() {
        let temp_dir =
            std::env::temp_dir().join(format!("moine-jwtd-scorer-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let path = temp_dir.join("sample.jsonl");
        fs::write(
            &path,
            concat!(
                r#"{"diffs":[{"pre_str":"固体","post_str":"個体","category":"kanji-conversion_a"},{"pre_str":"ab","post_str":"ac","category":"substitution"}]}"#,
                "\n",
                r#"{"diffs":[{"pre_str":"","post_str":"追記","category":"insertion"}]}"#,
                "\n",
            ),
        )
        .unwrap();

        let report = build_jwtd_scorer_report(&JwtdScorerReportOptions {
            split: JwtdSplitInput {
                name: "sample".to_string(),
                path: path.display().to_string(),
            },
            artifact_metadata: None,
            bundle_dir: None,
            negative_policy: JwtdNegativePolicy::Length,
            negative_count: 1,
            tie_policy: JwtdTiePolicy::Stable,
            seed: 0,
            max_examples: None,
            output: None,
        })
        .unwrap();

        assert!(report.contains("negative_policy: length"));
        assert!(report.contains("| `sample` | `ALL` | `surface_levenshtein`"));
        assert!(report.contains("| `sample` | `kanji-conversion_a` |"));
        assert!(!report.contains("insertion"));

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn parses_compare_artifact_payload_options() {
        let options = CompareOptions::parse(vec![
            "--left".to_string(),
            "いんさt".to_string(),
            "--right".to_string(),
            "印刷".to_string(),
            "--artifact-payload".to_string(),
            "moine-unidic-cwj-202512.readings.moinebin".to_string(),
            "--payload-format".to_string(),
            "binary".to_string(),
            "--max-readings-per-segment".to_string(),
            "16".to_string(),
            "--longest-only".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.artifact_payload,
            Some("moine-unidic-cwj-202512.readings.moinebin".to_string())
        );
        assert_eq!(options.payload_format, ArtifactPayloadFormat::Binary);
        assert_eq!(
            options.dictionary_options.max_readings_per_segment,
            Some(16)
        );
        assert!(options.dictionary_options.longest_match_only);
    }

    #[test]
    fn parses_compare_artifact_metadata_options() {
        let options = CompareOptions::parse(vec![
            "--left".to_string(),
            "いんさt".to_string(),
            "--right".to_string(),
            "印刷".to_string(),
            "--artifact-metadata".to_string(),
            "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
            "--max-paths".to_string(),
            "128".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.artifact_metadata,
            Some("dist/moine-unidic-cwj-202512/metadata.yaml".to_string())
        );
        assert_eq!(options.dictionary_options.max_paths, 128);
        assert_eq!(options.dictionary_option_overrides.max_paths, Some(128));
    }

    #[test]
    fn compare_rejects_multiple_dictionary_sources() {
        let err = run_compare(vec![
            "--left".to_string(),
            "いんさt".to_string(),
            "--right".to_string(),
            "印刷".to_string(),
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--artifact-payload".to_string(),
            "moine-unidic-cwj-202512.readings.yaml".to_string(),
        ])
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("arguments --lex-csv and --artifact-payload cannot be used together"));
    }

    #[test]
    fn parses_unidic_artifact_metadata_options() {
        let options = UnidicArtifactMetadataCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--source-version".to_string(),
            "2025.12".to_string(),
            "--artifact-name".to_string(),
            "moine-unidic-cwj-202512".to_string(),
            "--field".to_string(),
            "pron".to_string(),
            "--max-readings-per-surface".to_string(),
            "16".to_string(),
            "--max-readings-per-segment".to_string(),
            "8".to_string(),
            "--longest-only".to_string(),
        ])
        .unwrap();

        assert_eq!(options.lex_csv, "unidic-cwj-202512_full/lex.csv");
        assert_eq!(options.source_version, "2025.12");
        assert_eq!(options.artifact_name, "moine-unidic-cwj-202512");
        assert_eq!(
            options.payload_file_name,
            "moine-unidic-cwj-202512.readings.yaml"
        );
        assert_eq!(options.payload_format, ArtifactPayloadFormat::Yaml);
        assert_eq!(
            options.index_options.reading_field,
            UnidicReadingField::Pron
        );
        assert_eq!(options.index_options.max_readings_per_surface, Some(16));
        assert_eq!(options.dictionary_options.max_readings_per_segment, Some(8));
        assert!(options.dictionary_options.longest_match_only);
    }

    #[test]
    fn parses_unidic_artifact_bundle_options() {
        let options = UnidicArtifactBundleCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--source-version".to_string(),
            "2025.12".to_string(),
            "--output-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--artifact-name".to_string(),
            "moine-unidic-cwj-202512".to_string(),
            "--license-dir".to_string(),
            "unidic-cwj-202512_full/license".to_string(),
            "--max-readings-per-surface".to_string(),
            "16".to_string(),
            "--max-readings-per-segment".to_string(),
            "8".to_string(),
            "--longest-only".to_string(),
        ])
        .unwrap();

        assert_eq!(options.lex_csv, "unidic-cwj-202512_full/lex.csv");
        assert_eq!(options.source_version, "2025.12");
        assert_eq!(options.output_dir, "dist/moine-unidic-cwj-202512");
        assert_eq!(options.artifact_name, "moine-unidic-cwj-202512");
        assert_eq!(options.payload_format, ArtifactPayloadFormat::Yaml);
        assert_eq!(
            options.license_dir,
            Some("unidic-cwj-202512_full/license".to_string())
        );
        assert_eq!(options.index_options.max_readings_per_surface, Some(16));
        assert_eq!(options.dictionary_options.max_readings_per_segment, Some(8));
        assert!(options.dictionary_options.longest_match_only);
    }

    #[test]
    fn derives_default_unidic_license_dir_from_lex_csv() {
        assert_eq!(
            default_unidic_license_dir("unidic-cwj-202512_full/lex.csv"),
            PathBuf::from("unidic-cwj-202512_full/license")
        );
    }

    #[test]
    fn parses_binary_payload_format_for_metadata() {
        let options = UnidicArtifactMetadataCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--source-version".to_string(),
            "2025.12".to_string(),
            "--artifact-name".to_string(),
            "moine-unidic-cwj-202512".to_string(),
            "--payload-format".to_string(),
            "binary".to_string(),
        ])
        .unwrap();

        assert_eq!(options.payload_format, ArtifactPayloadFormat::Binary);
        assert_eq!(
            options.payload_file_name,
            "moine-unidic-cwj-202512.readings.moinebin"
        );
    }

    #[test]
    fn parses_binary_payload_format_for_bundle() {
        let options = UnidicArtifactBundleCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--source-version".to_string(),
            "2025.12".to_string(),
            "--output-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--payload-format".to_string(),
            "binary".to_string(),
        ])
        .unwrap();

        assert_eq!(options.payload_format, ArtifactPayloadFormat::Binary);
    }

    #[test]
    fn parses_indexed_payload_format_for_bundle() {
        let options = UnidicArtifactBundleCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--source-version".to_string(),
            "2025.12".to_string(),
            "--output-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--payload-format".to_string(),
            "indexed".to_string(),
        ])
        .unwrap();

        assert_eq!(options.payload_format, ArtifactPayloadFormat::Indexed);
        assert_eq!(
            default_unidic_payload_file_name("moine-unidic-cwj-202512", options.payload_format),
            "moine-unidic-cwj-202512.readings.moineidx"
        );
    }

    #[test]
    fn parses_unidic_artifact_archive_options() {
        let options = UnidicArtifactArchiveCliOptions::parse(vec![
            "--metadata".to_string(),
            "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
            "--output".to_string(),
            "dist/moine-unidic-cwj-202512.tar".to_string(),
            "--bundle-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--root-name".to_string(),
            "moine-unidic-cwj-202512".to_string(),
            "--compression".to_string(),
            "gzip".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.metadata,
            "dist/moine-unidic-cwj-202512/metadata.yaml"
        );
        assert_eq!(options.output, "dist/moine-unidic-cwj-202512.tar");
        assert_eq!(
            options.bundle_dir,
            Some("dist/moine-unidic-cwj-202512".to_string())
        );
        assert_eq!(
            options.root_name,
            Some("moine-unidic-cwj-202512".to_string())
        );
        assert_eq!(options.compression, ArchiveCompression::Gzip);
    }

    #[test]
    fn parses_zstd_artifact_archive_compression() {
        let options = UnidicArtifactArchiveCliOptions::parse(vec![
            "--metadata".to_string(),
            "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
            "--output".to_string(),
            "dist/moine-unidic-cwj-202512.tar.zst".to_string(),
            "--compression".to_string(),
            "zstd".to_string(),
        ])
        .unwrap();

        assert_eq!(options.compression, ArchiveCompression::Zstd);
    }

    #[test]
    fn tar_writer_uses_deterministic_file_headers() {
        let mut archive = Vec::new();
        write_tar_file_entry(&mut archive, "bundle/metadata.yaml", b"abc").unwrap();

        assert_eq!(&archive[0..20], b"bundle/metadata.yaml");
        assert_eq!(&archive[100..108], b"0000644\0");
        assert_eq!(&archive[124..136], b"00000000003\0");
        assert_eq!(&archive[136..148], b"00000000000\0");
        assert_eq!(archive[156], b'0');
        assert_eq!(&archive[257..263], b"ustar\0");
        assert_eq!(&archive[512..515], b"abc");
        assert_eq!(archive.len(), 1024);
    }

    #[test]
    fn gzip_encoder_uses_deterministic_header() {
        let mut compressed = Vec::new();
        {
            let mut encoder = gzip_encoder(&mut compressed);
            encoder.write_all(b"abc").unwrap();
            encoder.finish().unwrap();
        }

        assert_eq!(&compressed[0..4], &[0x1f, 0x8b, 0x08, 0x00]);
        assert_eq!(&compressed[4..8], &[0, 0, 0, 0]);
    }

    #[test]
    fn zstd_archive_is_deterministic_and_decodable() {
        let temp_dir =
            std::env::temp_dir().join(format!("moine-zstd-archive-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();
        let source = temp_dir.join("metadata.yaml");
        fs::write(&source, b"schema_version: 1\n").unwrap();
        let entries = vec![ArchiveEntry {
            source,
            path: "metadata.yaml".to_string(),
        }];

        let mut first = Vec::new();
        let mut second = Vec::new();
        write_release_archive(&mut first, ArchiveCompression::Zstd, "bundle", &entries).unwrap();
        write_release_archive(&mut second, ArchiveCompression::Zstd, "bundle", &entries).unwrap();

        assert_eq!(first, second);
        let decoded = zstd::decode_all(first.as_slice()).unwrap();
        assert_eq!(&decoded[0..20], b"bundle/metadata.yaml");
        assert!(decoded.ends_with(&[0_u8; 1024]));
    }

    #[test]
    fn archive_root_rejects_nested_paths() {
        let err = sanitize_archive_root("nested/root").unwrap_err();

        assert!(err.to_string().contains("--root-name"));
    }

    #[test]
    fn archive_paths_reject_parent_segments_after_normalization() {
        let err = normalized_relative_archive_path(r"..\payload.moinebin").unwrap_err();

        assert!(err.to_string().contains("stay inside the bundle"));
    }

    #[test]
    fn bundle_paths_reject_backslash_separators() {
        let err = checked_bundle_path(Path::new("bundle"), r"license\BSD").unwrap_err();

        assert!(err.to_string().contains("stay inside the bundle"));
    }

    #[test]
    fn parses_unidic_artifact_metadata_payload_file_name_override() {
        let options = UnidicArtifactMetadataCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--source-version".to_string(),
            "2025.12".to_string(),
            "--payload-file-name".to_string(),
            "payload.yaml".to_string(),
        ])
        .unwrap();

        assert_eq!(options.payload_file_name, "payload.yaml");
    }

    #[test]
    fn parses_unidic_artifact_payload_options() {
        let options = UnidicArtifactPayloadCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--output".to_string(),
            "payload.yaml".to_string(),
            "--field".to_string(),
            "pron".to_string(),
            "--max-readings-per-surface".to_string(),
            "4".to_string(),
        ])
        .unwrap();

        assert_eq!(options.lex_csv, "unidic-cwj-202512_full/lex.csv");
        assert_eq!(options.output, Some("payload.yaml".to_string()));
        assert_eq!(
            options.index_options.reading_field,
            UnidicReadingField::Pron
        );
        assert_eq!(options.index_options.max_readings_per_surface, Some(4));
    }

    #[test]
    fn parses_unidic_artifact_binary_payload_options() {
        let options = UnidicArtifactBinaryPayloadCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
            "--output".to_string(),
            "payload.moinebin".to_string(),
            "--field".to_string(),
            "pron".to_string(),
            "--max-readings-per-surface".to_string(),
            "4".to_string(),
        ])
        .unwrap();

        assert_eq!(options.lex_csv, "unidic-cwj-202512_full/lex.csv");
        assert_eq!(options.output, "payload.moinebin");
        assert_eq!(
            options.index_options.reading_field,
            UnidicReadingField::Pron
        );
        assert_eq!(options.index_options.max_readings_per_surface, Some(4));
    }

    #[test]
    fn binary_payload_options_require_output_path() {
        let err = UnidicArtifactBinaryPayloadCliOptions::parse(vec![
            "--lex-csv".to_string(),
            "unidic-cwj-202512_full/lex.csv".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(err, CliError::MissingArgument("--output")));
    }

    #[test]
    fn parses_unidic_artifact_binary_inspect_options() {
        let options = UnidicArtifactBinaryInspectCliOptions::parse(vec![
            "--payload".to_string(),
            "moine-unidic-cwj-202512.readings.moinebin".to_string(),
            "--timing".to_string(),
        ])
        .unwrap();

        assert_eq!(options.payload, "moine-unidic-cwj-202512.readings.moinebin");
        assert!(options.timing);
    }

    #[test]
    fn parses_unidic_artifact_inspect_options() {
        let options = UnidicArtifactInspectCliOptions::parse(vec![
            "--payload".to_string(),
            "moine-unidic-cwj-202512.readings.yaml".to_string(),
        ])
        .unwrap();

        assert_eq!(options.payload, "moine-unidic-cwj-202512.readings.yaml");
    }

    #[test]
    fn parses_unidic_artifact_verify_options() {
        let options = UnidicArtifactVerifyCliOptions::parse(vec![
            "--metadata".to_string(),
            "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
            "--bundle-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--canonical-checksum".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.metadata,
            "dist/moine-unidic-cwj-202512/metadata.yaml"
        );
        assert_eq!(
            options.bundle_dir,
            Some("dist/moine-unidic-cwj-202512".to_string())
        );
        assert!(options.canonical_checksum);
    }

    #[test]
    fn parses_unidic_artifact_release_checksums_options() {
        let options = UnidicArtifactReleaseChecksumsCliOptions::parse(vec![
            "--asset".to_string(),
            "dist/moine-unidic-cwj-202512.tar".to_string(),
            "--asset".to_string(),
            "dist/moine-unidic-cwj-202512.tar.gz".to_string(),
            "--output".to_string(),
            "dist/SHA256SUMS".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.assets,
            vec![
                "dist/moine-unidic-cwj-202512.tar".to_string(),
                "dist/moine-unidic-cwj-202512.tar.gz".to_string(),
            ]
        );
        assert_eq!(options.output, Some("dist/SHA256SUMS".to_string()));
    }

    #[test]
    fn release_checksum_asset_label_uses_file_name() {
        assert_eq!(
            release_checksum_asset_label(Path::new("dist/moine-unidic-cwj-202512.tar.gz")).unwrap(),
            "moine-unidic-cwj-202512.tar.gz"
        );
    }

    #[test]
    fn parses_unidic_artifact_runtime_measure_options() {
        let options = UnidicArtifactRuntimeMeasureCliOptions::parse(vec![
            "--metadata".to_string(),
            "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
            "--bundle-dir".to_string(),
            "dist/moine-unidic-cwj-202512".to_string(),
            "--pair".to_string(),
            "いんさt".to_string(),
            "印刷".to_string(),
            "--pair".to_string(),
            "とうきょうと".to_string(),
            "東京都".to_string(),
            "--warmups".to_string(),
            "2".to_string(),
            "--iterations".to_string(),
            "10".to_string(),
        ])
        .unwrap();

        assert_eq!(
            options.metadata,
            "dist/moine-unidic-cwj-202512/metadata.yaml"
        );
        assert_eq!(
            options.bundle_dir,
            Some("dist/moine-unidic-cwj-202512".to_string())
        );
        assert_eq!(
            options.pairs,
            vec![
                RuntimeMeasurePair {
                    left: "いんさt".to_string(),
                    right: "印刷".to_string(),
                },
                RuntimeMeasurePair {
                    left: "とうきょうと".to_string(),
                    right: "東京都".to_string(),
                },
            ]
        );
        assert_eq!(options.warmups, 2);
        assert_eq!(options.iterations, 10);
    }
}
