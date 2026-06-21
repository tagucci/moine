use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use clap::error::ErrorKind;
use flate2::{write::GzEncoder, Compression};
use moine_core::{distance_with_trace, Lattice};
use moine_ja::{
    DictionaryReadingOptions, SudachiIndexOptions, UnidicIndexOptions, UnidicReadingField,
};
use moine_zh::{CedictIndexOptions, PinyinReadingOptions, PinyinView};

use crate::archive::*;
use crate::args::*;
use crate::commands::compare::*;
use crate::commands::download::{
    copy_uri_to_path, read_uri_text, MAX_CHECKSUM_MANIFEST_BYTES, MAX_DOWNLOAD_BYTES,
};
use crate::commands::unidic_artifact::{
    run_sudachi_artifact_bundle, run_unidic_artifact_bundle, run_unidic_artifact_metadata,
    run_unidic_artifact_runtime_measure,
};
use crate::commands::zh_artifact::{run_zh_artifact_bundle, run_zh_artifact_metadata};

fn over_budget_dictionary_options() -> DictionaryReadingOptions {
    DictionaryReadingOptions {
        max_paths: usize::MAX,
        ..DictionaryReadingOptions::default()
    }
}

fn over_budget_pinyin_options() -> PinyinReadingOptions {
    PinyinReadingOptions {
        max_paths: usize::MAX,
        ..PinyinReadingOptions::default()
    }
}

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
    assert!(options
        .spec
        .archive_url
        .contains("moine-cedict-20260520-v0.1.1"));
    assert_eq!(options.url, Some("/tmp/moine-cedict.tar.gz".to_string()));
    assert_eq!(options.checksum_url, Some("/tmp/SHA256SUMS".to_string()));
    assert_eq!(options.cache_dir, Some("/tmp/moine-cache".to_string()));
    assert!(options.force);
}

#[test]
fn parses_sudachi_download_options() {
    let options = DownloadCliOptions::parse(vec!["ja-sudachi".to_string()]).unwrap();

    assert_eq!(options.spec.language, ArtifactLanguage::JapaneseSudachi);
    assert_eq!(options.spec.artifact_name, "moine-sudachi-full-20260428");
    assert_eq!(
        options.spec.archive_name,
        "moine-sudachi-full-20260428.tar.gz"
    );
    assert!(options
        .spec
        .archive_url
        .contains("moine-sudachi-full-20260428-v0.2.0"));
    assert!(options
        .spec
        .checksum_url
        .is_some_and(|url| url.contains("moine-sudachi-full-20260428-v0.2.0/SHA256SUMS")));
}

#[test]
fn parses_unidic_download_options() {
    let options = DownloadCliOptions::parse(vec!["ja".to_string()]).unwrap();

    assert_eq!(options.spec.language, ArtifactLanguage::Japanese);
    assert_eq!(options.spec.artifact_name, "moine-unidic-cwj-202512");
    assert!(options
        .spec
        .archive_url
        .contains("unidic-cwj-202512-v0.1.1"));
    assert!(options
        .spec
        .checksum_url
        .is_some_and(|url| url.contains("unidic-cwj-202512-v0.1.1/SHA256SUMS")));

    let explicit = DownloadCliOptions::parse(vec!["ja-unidic".to_string()]).unwrap();
    assert_eq!(explicit.spec.language, ArtifactLanguage::Japanese);
    assert_eq!(explicit.spec.artifact_name, "moine-unidic-cwj-202512");
}

#[test]
fn default_download_specs_point_to_current_artifact_releases() {
    let ja = download_spec_for_language(ArtifactLanguage::Japanese);
    let sudachi = download_spec_for_language(ArtifactLanguage::JapaneseSudachi);
    let zh = download_spec_for_language(ArtifactLanguage::Chinese);

    assert_eq!(ja.artifact_name, "moine-unidic-cwj-202512");
    assert!(ja
        .archive_url
        .contains("unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.gz"));
    assert!(ja
        .checksum_url
        .is_some_and(|url| url.contains("unidic-cwj-202512-v0.1.1/SHA256SUMS")));
    assert_eq!(sudachi.artifact_name, "moine-sudachi-full-20260428");
    assert!(sudachi
        .archive_url
        .contains("moine-sudachi-full-20260428-v0.2.0/moine-sudachi-full-20260428.tar.gz"));
    assert!(sudachi
        .checksum_url
        .is_some_and(|url| url.contains("moine-sudachi-full-20260428-v0.2.0/SHA256SUMS")));
    assert_eq!(zh.artifact_name, "moine-cedict-20260520");
    assert!(zh
        .archive_url
        .contains("moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.gz"));
    assert!(zh
        .checksum_url
        .is_some_and(|url| url.contains("moine-cedict-20260520-v0.1.1/SHA256SUMS")));
}

#[test]
fn local_download_copy_enforces_size_limit() {
    let temp = TempDir::new("moine-download-test").unwrap();
    let source = temp.path().join("oversized.tar.gz");
    fs::File::create(&source)
        .unwrap()
        .set_len(MAX_DOWNLOAD_BYTES + 1)
        .unwrap();

    let err = copy_uri_to_path(source.to_str().unwrap(), &temp.path().join("plain.tar.gz"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("download exceeded maximum size"));

    let file_uri = format!("file://{}", source.display());
    let err = copy_uri_to_path(&file_uri, &temp.path().join("uri.tar.gz"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("download exceeded maximum size"));
}

#[test]
fn local_checksum_manifest_enforces_size_limit() {
    let temp = TempDir::new("moine-download-test").unwrap();
    let source = temp.path().join("SHA256SUMS");
    fs::File::create(&source)
        .unwrap()
        .set_len(MAX_CHECKSUM_MANIFEST_BYTES + 1)
        .unwrap();

    let err = read_uri_text(source.to_str().unwrap())
        .unwrap_err()
        .to_string();
    assert!(err.contains("checksum manifest exceeded maximum size"));

    let file_uri = format!("file://{}", source.display());
    let err = read_uri_text(&file_uri).unwrap_err().to_string();
    assert!(err.contains("checksum manifest exceeded maximum size"));
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

    let sudachi_where_options = WhereCliOptions::parse(vec![
        "sudachi".to_string(),
        "--cache-dir".to_string(),
        "/tmp/moine-cache".to_string(),
    ])
    .unwrap();
    assert_eq!(
        sudachi_where_options.language,
        Some(ArtifactLanguage::JapaneseSudachi)
    );

    let unidic_where_options = WhereCliOptions::parse(vec![
        "unidic".to_string(),
        "--cache-dir".to_string(),
        "/tmp/moine-cache".to_string(),
    ])
    .unwrap();
    assert_eq!(
        unidic_where_options.language,
        Some(ArtifactLanguage::Japanese)
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
fn extract_artifact_archive_rejects_too_many_entries() {
    let temp = TempDir::new("moine-cli-test").unwrap();
    let archive_path = temp.path().join("too-many.tar");
    {
        let file = fs::File::create(&archive_path).unwrap();
        let mut builder = tar::Builder::new(file);
        for index in 0..=MAX_ARCHIVE_ENTRIES {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Directory);
            header.set_size(0);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("bundle/dir-{index}"), std::io::empty())
                .unwrap();
        }
        builder.finish().unwrap();
    }

    let err = extract_artifact_archive(&archive_path, &temp.path().join("extract"))
        .expect_err("excessive archive entry counts should be rejected");
    assert!(err.to_string().contains("archive entry count"));
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
    assert_eq!(options.pinyin_lattice, None);
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
        "--pinyin-lattice".to_string(),
        "/tmp/moine-pinyin-lattice.svg".to_string(),
        "--output-format".to_string(),
        "svg".to_string(),
    ])
    .unwrap();

    assert_eq!(
        options.source,
        ZhIndexSource::ArtifactMetadata("dist/moine-cedict/metadata.yaml".to_string())
    );
    assert_eq!(options.reading_options.max_paths, 128);
    assert_eq!(
        options.pinyin_lattice,
        Some("/tmp/moine-pinyin-lattice.svg".to_string())
    );
    assert_eq!(options.output_format, RomajiLatticeOutputFormat::Svg);
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
fn zh_artifact_generators_reject_over_budget_query_defaults() {
    let metadata_err = run_zh_artifact_metadata(ZhArtifactMetadataCliOptions {
        cedict: "/does/not/exist/cedict.txt".to_string(),
        output: None,
        artifact_name: "moine-cedict-test".to_string(),
        payload_file_name: "readings.yaml".to_string(),
        payload_format: ArtifactPayloadFormat::Yaml,
        source_name: "CC-CEDICT".to_string(),
        source_version: "test".to_string(),
        index_options: CedictIndexOptions::default(),
        reading_options: over_budget_pinyin_options(),
    })
    .unwrap_err();

    assert!(metadata_err.to_string().contains("max_paths"));

    let bundle_err = run_zh_artifact_bundle(ZhArtifactBundleCliOptions {
        cedict: "/does/not/exist/cedict.txt".to_string(),
        output_dir: "/does/not/matter".to_string(),
        artifact_name: "moine-cedict-test".to_string(),
        payload_format: ArtifactPayloadFormat::Indexed,
        source_name: "CC-CEDICT".to_string(),
        source_version: "test".to_string(),
        license_file: None,
        index_options: CedictIndexOptions::default(),
        reading_options: over_budget_pinyin_options(),
    })
    .unwrap_err();

    assert!(bundle_err.to_string().contains("max_paths"));
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
fn parses_compare_romaji_lattice_options() {
    let options = CompareOptions::parse(vec![
        "--left".to_string(),
        "きめつのやいば".to_string(),
        "--right".to_string(),
        "鬼滅の刃".to_string(),
        "--artifact-metadata".to_string(),
        "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
        "--romaji-lattice".to_string(),
        "/tmp/moine-romaji-lattice.svg".to_string(),
        "--output-format".to_string(),
        "svg".to_string(),
    ])
    .unwrap();

    assert_eq!(
        options.romaji_lattice,
        Some("/tmp/moine-romaji-lattice.svg".to_string())
    );
    assert_eq!(options.output_format, RomajiLatticeOutputFormat::Svg);
}

#[test]
fn renders_romaji_lattice_dot_with_best_path() {
    let left_lattice = Lattice::from_paths(["insatu", "insatsu"]);
    let right_lattice = Lattice::from_paths(["insatsu"]);
    let trace = distance_with_trace(&left_lattice, &right_lattice);
    let dot = romaji_lattice_dot(&RomajiLatticeData {
        left_input: "印刷".to_string(),
        right_input: "いんさつ".to_string(),
        left_lattice,
        right_lattice,
        distance: trace.distance,
        trace: Some(trace),
        trace_error: None,
    });

    assert!(dot.contains("digraph moine_romaji_lattice"));
    assert!(dot.contains("subgraph cluster_left"));
    assert!(dot.contains("subgraph cluster_right"));
    assert!(!dot.contains("source="));
    assert!(dot.contains("best_left=insatsu"));
    assert!(dot.contains("best_right=insatsu"));
    assert!(dot.contains("label=\"s\""));
    assert!(dot.contains("color=\"#9a5b38\""));
    assert!(dot.contains("penwidth=3.0"));
}

#[test]
fn renders_pinyin_lattice_dot_with_best_path() {
    let left_lattice = Lattice::from_paths(["weishiji"]);
    let right_lattice = Lattice::from_paths(["weishiji"]);
    let trace = distance_with_trace(&left_lattice, &right_lattice);
    let dot = pinyin_lattice_dot(&RomajiLatticeData {
        left_input: "weishiji".to_string(),
        right_input: "威士忌".to_string(),
        left_lattice,
        right_lattice,
        distance: trace.distance,
        trace: Some(trace),
        trace_error: None,
    });

    assert!(dot.contains("digraph moine_pinyin_lattice"));
    assert!(dot.contains("best_left=weishiji"));
    assert!(dot.contains("best_right=weishiji"));
    assert!(dot.contains("label=\"w\""));
    assert!(dot.contains("color=\"#9a5b38\""));
}

#[test]
fn romaji_lattice_graph_reports_missing_dot_command() {
    let temp = TempDir::new("moine-cli-test").unwrap();
    let err = write_romaji_lattice_graph_with_dot_command(
        &temp.path().join("lattice.svg"),
        "digraph g {}\n",
        RomajiLatticeOutputFormat::Svg,
        "__moine_missing_dot__",
    )
    .expect_err("missing dot command should be reported");

    let message = err.to_string();
    assert!(message.contains("required command \"__moine_missing_dot__\""));
    assert!(message.contains("install Graphviz"));
    assert!(message.contains("--output-format dot"));
}

#[test]
fn compare_reports_missing_method_for_direct_options() {
    let err = run_compare(CompareOptions {
        left: "moine".to_string(),
        right: "moinya".to_string(),
        overrides: None,
        lex_csv: None,
        sudachi_lex_csv: None,
        artifact_payload: None,
        artifact_metadata: None,
        payload_format: ArtifactPayloadFormat::Yaml,
        romaji_lattice: None,
        output_format: RomajiLatticeOutputFormat::Dot,
        index_options: Default::default(),
        sudachi_index_options: Default::default(),
        dictionary_options: Default::default(),
        dictionary_option_overrides: Default::default(),
    })
    .expect_err("direct options without a comparison method should be rejected");

    let message = err.to_string();
    assert!(message.contains("missing required argument"));
    assert!(message.contains("--artifact-metadata"));
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
fn compare_allows_overrides_with_one_dictionary_source() {
    let options = CompareOptions::parse(vec![
        "--left".to_string(),
        "いんさt".to_string(),
        "--right".to_string(),
        "印刷".to_string(),
        "--overrides".to_string(),
        "crates/moine-ja/tests/resources/overrides.yaml".to_string(),
        "--lex-csv".to_string(),
        "unidic-cwj-202512_full/lex.csv".to_string(),
    ])
    .unwrap();

    assert_eq!(
        options.overrides,
        Some("crates/moine-ja/tests/resources/overrides.yaml".to_string())
    );
    assert_eq!(
        options.lex_csv,
        Some("unidic-cwj-202512_full/lex.csv".to_string())
    );
}

#[test]
fn parses_compare_with_sudachi_lex_csv() {
    let options = CompareOptions::parse(vec![
        "--left".to_string(),
        "きめつのやいば".to_string(),
        "--right".to_string(),
        "鬼滅の刃".to_string(),
        "--sudachi-lex-csv".to_string(),
        "sudachi/full_lex.csv".to_string(),
        "--max-readings-per-surface".to_string(),
        "16".to_string(),
    ])
    .unwrap();

    assert_eq!(options.lex_csv, None);
    assert_eq!(
        options.sudachi_lex_csv,
        Some("sudachi/full_lex.csv".to_string())
    );
    assert_eq!(
        options.sudachi_index_options.max_readings_per_surface,
        Some(16)
    );
    assert!(options.sudachi_index_options.include_normalized_surfaces);
    assert!(!options.sudachi_index_options.exclude_unsupported_readings);
}

#[test]
fn parses_compare_with_sudachi_specific_index_options() {
    let options = CompareOptions::parse(vec![
        "--left".to_string(),
        "きめつのやいば".to_string(),
        "--right".to_string(),
        "鬼滅の刃".to_string(),
        "--sudachi-lex-csv".to_string(),
        "sudachi/full_lex.csv".to_string(),
        "--no-normalized-surfaces".to_string(),
        "--exclude-unsupported-readings".to_string(),
    ])
    .unwrap();

    assert!(!options.sudachi_index_options.include_normalized_surfaces);
    assert!(options.sudachi_index_options.exclude_unsupported_readings);
}

#[test]
fn compare_rejects_multiple_dictionary_sources() {
    let err = Cli::parse_from_args([
        "compare",
        "--left",
        "いんさt",
        "--right",
        "印刷",
        "--lex-csv",
        "unidic-cwj-202512_full/lex.csv",
        "--artifact-payload",
        "moine-unidic-cwj-202512.readings.yaml",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
}

#[test]
fn compare_rejects_unidic_and_sudachi_sources_together() {
    let err = Cli::parse_from_args([
        "compare",
        "--left",
        "きめつのやいば",
        "--right",
        "鬼滅の刃",
        "--lex-csv",
        "unidic-cwj-202512_full/lex.csv",
        "--sudachi-lex-csv",
        "sudachi/full_lex.csv",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::ArgumentConflict);
}

#[test]
fn compare_rejects_unidic_field_for_sudachi_source() {
    let err = CompareOptions::parse(vec![
        "--left".to_string(),
        "きめつのやいば".to_string(),
        "--right".to_string(),
        "鬼滅の刃".to_string(),
        "--sudachi-lex-csv".to_string(),
        "sudachi/full_lex.csv".to_string(),
        "--field".to_string(),
        "pron".to_string(),
    ])
    .unwrap_err();

    assert!(err.to_string().contains("--field"));
    assert!(err.to_string().contains("--sudachi-lex-csv"));
}

#[test]
fn compare_rejects_sudachi_options_for_unidic_source() {
    let err = CompareOptions::parse(vec![
        "--left".to_string(),
        "きめつのやいば".to_string(),
        "--right".to_string(),
        "鬼滅の刃".to_string(),
        "--lex-csv".to_string(),
        "unidic-cwj-202512_full/lex.csv".to_string(),
        "--no-normalized-surfaces".to_string(),
    ])
    .unwrap_err();

    assert!(err.to_string().contains("--no-normalized-surfaces"));
    assert!(err.to_string().contains("--lex-csv"));
}

#[test]
fn compare_rejects_csv_index_options_for_artifact_source() {
    let err = CompareOptions::parse(vec![
        "--left".to_string(),
        "きめつのやいば".to_string(),
        "--right".to_string(),
        "鬼滅の刃".to_string(),
        "--artifact-metadata".to_string(),
        "dist/moine-unidic-cwj-202512/metadata.yaml".to_string(),
        "--max-readings-per-surface".to_string(),
        "16".to_string(),
    ])
    .unwrap_err();

    assert!(err.to_string().contains("--max-readings-per-surface"));
    assert!(err.to_string().contains("--artifact-metadata"));
}

#[test]
fn parses_sudachi_csv_readings_options() {
    let options = SudachiCsvReadingsOptions::parse(vec![
        "--surface".to_string(),
        "鬼滅の刃".to_string(),
        "--lex-csv".to_string(),
        "sudachi/full_lex.csv".to_string(),
        "--exclude-unsupported-readings".to_string(),
    ])
    .unwrap();

    assert_eq!(options.surface, "鬼滅の刃");
    assert_eq!(options.lex_csv, "sudachi/full_lex.csv");
    assert!(options.index_options.exclude_unsupported_readings);
}

#[test]
fn parses_sudachi_artifact_bundle_options() {
    let options = SudachiArtifactBundleCliOptions::parse(vec![
        "--lex-csv".to_string(),
        "sudachi/full_lex.csv".to_string(),
        "--source-version".to_string(),
        "20260428".to_string(),
        "--output-dir".to_string(),
        "dist/moine-sudachi-full-20260428".to_string(),
        "--artifact-name".to_string(),
        "moine-sudachi-full-20260428".to_string(),
        "--license-file".to_string(),
        "SudachiDict/LICENSE-2.0.txt".to_string(),
        "--legal-file".to_string(),
        "SudachiDict/LEGAL".to_string(),
        "--max-readings-per-surface".to_string(),
        "16".to_string(),
        "--exclude-unsupported-readings".to_string(),
        "--max-readings-per-segment".to_string(),
        "8".to_string(),
        "--longest-only".to_string(),
    ])
    .unwrap();

    assert_eq!(options.lex_csv, "sudachi/full_lex.csv");
    assert_eq!(options.source_version, "20260428");
    assert_eq!(options.output_dir, "dist/moine-sudachi-full-20260428");
    assert_eq!(options.artifact_name, "moine-sudachi-full-20260428");
    assert_eq!(options.payload_format, ArtifactPayloadFormat::Indexed);
    assert_eq!(options.source_name, "SudachiDict");
    assert_eq!(
        options.license_file,
        "SudachiDict/LICENSE-2.0.txt".to_string()
    );
    assert_eq!(options.legal_file, "SudachiDict/LEGAL".to_string());
    assert_eq!(options.index_options.max_readings_per_surface, Some(16));
    assert!(options.index_options.exclude_unsupported_readings);
    assert_eq!(options.dictionary_options.max_readings_per_segment, Some(8));
    assert!(options.dictionary_options.longest_match_only);
}

#[test]
fn sudachi_artifact_bundle_requires_license_and_legal_files() {
    let err = Cli::parse_from_args([
        "sudachi-artifact-bundle",
        "--lex-csv",
        "sudachi/full_lex.csv",
        "--source-version",
        "20260428",
        "--output-dir",
        "dist/moine-sudachi-full-20260428",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
}

#[test]
fn sudachi_artifact_bundle_requires_legal_file() {
    let err = Cli::parse_from_args([
        "sudachi-artifact-bundle",
        "--lex-csv",
        "sudachi/full_lex.csv",
        "--source-version",
        "20260428",
        "--output-dir",
        "dist/moine-sudachi-full-20260428",
        "--license-file",
        "SudachiDict/LICENSE-2.0.txt",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
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
        options.index_options.reading_field,
        UnidicReadingField::Pron
    );
    assert_eq!(
        options.license_dir,
        Some("unidic-cwj-202512_full/license".to_string())
    );
    assert_eq!(options.index_options.max_readings_per_surface, Some(16));
    assert_eq!(options.dictionary_options.max_readings_per_segment, Some(8));
    assert!(options.dictionary_options.longest_match_only);
}

#[test]
fn unidic_artifact_generators_reject_over_budget_query_defaults() {
    let metadata_err = run_unidic_artifact_metadata(UnidicArtifactMetadataCliOptions {
        lex_csv: "/does/not/exist/lex.csv".to_string(),
        output: None,
        artifact_name: "moine-unidic-test".to_string(),
        payload_file_name: "readings.yaml".to_string(),
        payload_format: ArtifactPayloadFormat::Yaml,
        source_name: "UniDic-CWJ".to_string(),
        source_version: "test".to_string(),
        index_options: UnidicIndexOptions::default(),
        dictionary_options: over_budget_dictionary_options(),
    })
    .unwrap_err();

    assert!(metadata_err.to_string().contains("max_paths"));

    let bundle_err = run_unidic_artifact_bundle(UnidicArtifactBundleCliOptions {
        lex_csv: "/does/not/exist/lex.csv".to_string(),
        output_dir: "/does/not/matter".to_string(),
        artifact_name: "moine-unidic-test".to_string(),
        payload_format: ArtifactPayloadFormat::Indexed,
        source_name: "UniDic-CWJ".to_string(),
        source_version: "test".to_string(),
        license_dir: None,
        index_options: UnidicIndexOptions::default(),
        dictionary_options: over_budget_dictionary_options(),
    })
    .unwrap_err();

    assert!(bundle_err.to_string().contains("max_paths"));
}

#[test]
fn sudachi_artifact_bundle_rejects_over_budget_query_defaults() {
    let err = run_sudachi_artifact_bundle(SudachiArtifactBundleCliOptions {
        lex_csv: "/does/not/exist/sudachi.csv".to_string(),
        output_dir: "/does/not/matter".to_string(),
        artifact_name: "moine-sudachi-test".to_string(),
        payload_format: ArtifactPayloadFormat::Indexed,
        source_name: "SudachiDict".to_string(),
        source_version: "test".to_string(),
        license_file: "/does/not/exist/LICENSE-2.0.txt".to_string(),
        legal_file: "/does/not/exist/LEGAL".to_string(),
        index_options: SudachiIndexOptions::default(),
        dictionary_options: over_budget_dictionary_options(),
    })
    .unwrap_err();

    assert!(err.to_string().contains("max_paths"));
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
        options.index_options.reading_field,
        UnidicReadingField::Pron
    );
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

    assert!(err.to_string().contains("--output"));
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
fn parses_zh_artifact_release_checksums_options() {
    let options = ZhArtifactReleaseChecksumsCliOptions::parse(vec![
        "--asset".to_string(),
        "dist/moine-cedict-20260520.tar".to_string(),
        "--asset".to_string(),
        "dist/moine-cedict-20260520.tar.gz".to_string(),
        "--output".to_string(),
        "dist/ZH-SHA256SUMS".to_string(),
    ])
    .unwrap();

    assert_eq!(
        options.assets,
        vec![
            "dist/moine-cedict-20260520.tar".to_string(),
            "dist/moine-cedict-20260520.tar.gz".to_string(),
        ]
    );
    assert_eq!(options.output, Some("dist/ZH-SHA256SUMS".to_string()));
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

#[test]
fn runtime_measure_rejects_zero_iterations_before_loading_artifact() {
    let err = run_unidic_artifact_runtime_measure(UnidicArtifactRuntimeMeasureCliOptions {
        metadata: "/does/not/exist/metadata.yaml".to_string(),
        bundle_dir: None,
        pairs: vec![RuntimeMeasurePair {
            left: "いんさt".to_string(),
            right: "印刷".to_string(),
        }],
        warmups: 0,
        iterations: 0,
    })
    .expect_err("zero iterations should be rejected before artifact IO");

    let message = err.to_string();
    assert!(message.contains("invalid value \"0\" for argument --iterations"));
}

#[test]
fn runtime_measure_rejects_empty_pairs_before_loading_artifact() {
    let err = run_unidic_artifact_runtime_measure(UnidicArtifactRuntimeMeasureCliOptions {
        metadata: "/does/not/exist/metadata.yaml".to_string(),
        bundle_dir: None,
        pairs: Vec::new(),
        warmups: 0,
        iterations: 1,
    })
    .expect_err("empty pairs should be rejected before artifact IO");

    assert_eq!(err.to_string(), "missing required argument --pair");
}
