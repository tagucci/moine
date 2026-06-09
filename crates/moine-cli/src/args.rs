use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use clap::{ArgGroup, Args, Parser, Subcommand};
use moine_ja::{DictionaryReadingOptions, UnidicIndexOptions, UnidicReadingField};
use moine_zh::{CedictIndexOptions, PinyinReadingOptions, PinyinView};

pub(crate) const YAML_PAYLOAD_FORMAT: &str = "yaml.surface-readings.v1";
pub(crate) const BINARY_PAYLOAD_FORMAT: &str = "binary.surface-readings.v1";
pub(crate) const INDEXED_PAYLOAD_FORMAT: &str = "indexed-fst.surface-readings.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtifactLanguage {
    Japanese,
    Chinese,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DownloadArtifactSpec {
    pub(crate) language: ArtifactLanguage,
    pub(crate) artifact_name: &'static str,
    pub(crate) archive_name: &'static str,
    pub(crate) archive_url: &'static str,
    pub(crate) checksum_url: Option<&'static str>,
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

pub(crate) struct CompareOptions {
    pub(crate) left: String,
    pub(crate) right: String,
    pub(crate) overrides: Option<String>,
    pub(crate) lex_csv: Option<String>,
    pub(crate) artifact_payload: Option<String>,
    pub(crate) artifact_metadata: Option<String>,
    pub(crate) payload_format: ArtifactPayloadFormat,
    pub(crate) index_options: UnidicIndexOptions,
    pub(crate) dictionary_options: DictionaryReadingOptions,
    pub(crate) dictionary_option_overrides: DictionaryReadingOptionOverrides,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DownloadCliOptions {
    pub(crate) spec: DownloadArtifactSpec,
    pub(crate) url: Option<String>,
    pub(crate) checksum_url: Option<String>,
    pub(crate) sha256: Option<String>,
    pub(crate) cache_dir: Option<String>,
    pub(crate) force: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CacheCliOptions {
    pub(crate) cache_dir: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WhereCliOptions {
    pub(crate) language: Option<ArtifactLanguage>,
    pub(crate) cache_dir: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CedictReadingsOptions {
    pub(crate) surface: String,
    pub(crate) cedict: String,
    pub(crate) index_options: CedictIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CedictSequencesOptions {
    pub(crate) text: String,
    pub(crate) cedict: String,
    pub(crate) index_options: CedictIndexOptions,
    pub(crate) reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChineseCompareOptions {
    pub(crate) left: String,
    pub(crate) right: String,
    pub(crate) source: ZhIndexSource,
    pub(crate) index_options: CedictIndexOptions,
    pub(crate) reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ZhIndexSource {
    Cedict(String),
    ArtifactPayload {
        path: String,
        payload_format: ArtifactPayloadFormat,
    },
    ArtifactMetadata(String),
}

impl ZhIndexSource {
    pub(crate) fn label(&self) -> (&'static str, &str) {
        match self {
            Self::Cedict(path) => ("cedict", path),
            Self::ArtifactPayload { path, .. } => ("artifact_payload", path),
            Self::ArtifactMetadata(path) => ("artifact_metadata", path),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactMetadataCliOptions {
    pub(crate) cedict: String,
    pub(crate) output: Option<String>,
    pub(crate) artifact_name: String,
    pub(crate) payload_file_name: String,
    pub(crate) payload_format: ArtifactPayloadFormat,
    pub(crate) source_name: String,
    pub(crate) source_version: String,
    pub(crate) index_options: CedictIndexOptions,
    pub(crate) reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactBundleCliOptions {
    pub(crate) cedict: String,
    pub(crate) output_dir: String,
    pub(crate) artifact_name: String,
    pub(crate) payload_format: ArtifactPayloadFormat,
    pub(crate) source_name: String,
    pub(crate) source_version: String,
    pub(crate) license_file: Option<String>,
    pub(crate) index_options: CedictIndexOptions,
    pub(crate) reading_options: PinyinReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactPayloadCliOptions {
    pub(crate) cedict: String,
    pub(crate) output: Option<String>,
    pub(crate) payload_format: ArtifactPayloadFormat,
    pub(crate) index_options: CedictIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactArchiveCliOptions {
    pub(crate) metadata: String,
    pub(crate) output: String,
    pub(crate) bundle_dir: Option<String>,
    pub(crate) root_name: Option<String>,
    pub(crate) compression: ArchiveCompression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactInspectCliOptions {
    pub(crate) payload: String,
    pub(crate) payload_format: ArtifactPayloadFormat,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactVerifyCliOptions {
    pub(crate) metadata: String,
    pub(crate) bundle_dir: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactMetadataCliOptions {
    pub(crate) lex_csv: String,
    pub(crate) output: Option<String>,
    pub(crate) artifact_name: String,
    pub(crate) payload_file_name: String,
    pub(crate) payload_format: ArtifactPayloadFormat,
    pub(crate) source_name: String,
    pub(crate) source_version: String,
    pub(crate) index_options: UnidicIndexOptions,
    pub(crate) dictionary_options: DictionaryReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactBundleCliOptions {
    pub(crate) lex_csv: String,
    pub(crate) output_dir: String,
    pub(crate) artifact_name: String,
    pub(crate) payload_format: ArtifactPayloadFormat,
    pub(crate) source_name: String,
    pub(crate) source_version: String,
    pub(crate) license_dir: Option<String>,
    pub(crate) index_options: UnidicIndexOptions,
    pub(crate) dictionary_options: DictionaryReadingOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactArchiveCliOptions {
    pub(crate) metadata: String,
    pub(crate) output: String,
    pub(crate) bundle_dir: Option<String>,
    pub(crate) root_name: Option<String>,
    pub(crate) compression: ArchiveCompression,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArtifactPayloadFormat {
    Yaml,
    Binary,
    Indexed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveCompression {
    None,
    Gzip,
    Zstd,
}

impl ArchiveCompression {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactBinaryPayloadCliOptions {
    pub(crate) lex_csv: String,
    pub(crate) output: String,
    pub(crate) index_options: UnidicIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactBinaryInspectCliOptions {
    pub(crate) payload: String,
    pub(crate) timing: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactPayloadCliOptions {
    pub(crate) lex_csv: String,
    pub(crate) output: Option<String>,
    pub(crate) index_options: UnidicIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactInspectCliOptions {
    pub(crate) payload: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactVerifyCliOptions {
    pub(crate) metadata: String,
    pub(crate) bundle_dir: Option<String>,
    pub(crate) canonical_checksum: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactReleaseChecksumsCliOptions {
    pub(crate) assets: Vec<String>,
    pub(crate) output: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ZhArtifactReleaseChecksumsCliOptions {
    pub(crate) assets: Vec<String>,
    pub(crate) output: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicArtifactRuntimeMeasureCliOptions {
    pub(crate) metadata: String,
    pub(crate) bundle_dir: Option<String>,
    pub(crate) pairs: Vec<RuntimeMeasurePair>,
    pub(crate) warmups: usize,
    pub(crate) iterations: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeMeasurePair {
    pub(crate) left: String,
    pub(crate) right: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DictionaryReadingOptionOverrides {
    pub(crate) max_span_chars: Option<usize>,
    pub(crate) max_paths: Option<usize>,
    pub(crate) longest_match_only: bool,
    pub(crate) max_readings_per_segment: Option<usize>,
}

impl DictionaryReadingOptionOverrides {
    pub(crate) fn apply_to(
        self,
        mut options: DictionaryReadingOptions,
    ) -> DictionaryReadingOptions {
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
pub(crate) struct UnidicReadingsOptions {
    pub(crate) text: String,
    pub(crate) dic_dir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicCsvReadingsOptions {
    pub(crate) surface: String,
    pub(crate) lex_csv: String,
    pub(crate) index_options: UnidicIndexOptions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnidicCsvSequencesOptions {
    pub(crate) text: String,
    pub(crate) lex_csv: String,
    pub(crate) index_options: UnidicIndexOptions,
    pub(crate) dictionary_options: DictionaryReadingOptions,
}

#[derive(Debug, Parser)]
#[command(
    name = "moine",
    disable_help_subcommand = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

impl Cli {
    pub(crate) fn from_env() -> Self {
        Self::parse()
    }

    pub(crate) fn parse_from_args<I, S>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::try_parse_from(
            std::iter::once("moine".to_string()).chain(args.into_iter().map(Into::into)),
        )
    }

    pub(crate) fn into_action(self) -> Result<CliAction, CliError> {
        self.command.into_action()
    }
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Show CC-CEDICT readings for a Chinese surface.
    CedictReadings(CedictReadingsArgs),
    /// Expand Chinese text into dictionary pinyin paths.
    CedictSequences(CedictSequencesArgs),
    /// Compare Chinese text with pinyin-aware dictionary paths.
    ChineseCompare(ChineseCompareArgs),
    /// Compare Japanese text with reading-aware paths.
    Compare(CompareArgs),
    /// Download a published dictionary artifact into the local cache.
    Download(DownloadArgs),
    /// List installed dictionary artifacts in the local cache.
    List(CacheArgs),
    /// Show the cache location or expected artifact path.
    Where(WhereArgs),
    /// Create a release archive for a Chinese dictionary artifact.
    ZhArtifactArchive(ZhArtifactArchiveArgs),
    /// Build a Chinese dictionary artifact bundle from CC-CEDICT.
    ZhArtifactBundle(ZhArtifactBundleArgs),
    /// Inspect a Chinese dictionary artifact payload.
    ZhArtifactInspect(ZhArtifactInspectArgs),
    /// Generate Chinese dictionary artifact metadata.
    ZhArtifactMetadata(ZhArtifactMetadataArgs),
    /// Generate a Chinese dictionary artifact payload.
    ZhArtifactPayload(ZhArtifactPayloadArgs),
    /// Generate SHA-256 checksums for Chinese release assets.
    ZhArtifactReleaseChecksums(ReleaseChecksumsArgs),
    /// Verify a Chinese dictionary artifact bundle.
    ZhArtifactVerify(ArtifactVerifyArgs),
    /// Create a release archive for a UniDic dictionary artifact.
    UnidicArtifactArchive(UnidicArtifactArchiveArgs),
    /// Inspect a binary UniDic artifact payload.
    UnidicArtifactBinaryInspect(UnidicArtifactBinaryInspectArgs),
    /// Generate a binary UniDic artifact payload.
    UnidicArtifactBinaryPayload(UnidicArtifactBinaryPayloadArgs),
    /// Build a UniDic dictionary artifact bundle.
    UnidicArtifactBundle(UnidicArtifactBundleArgs),
    /// Inspect a YAML UniDic artifact payload.
    UnidicArtifactInspect(UnidicArtifactInspectArgs),
    /// Generate UniDic dictionary artifact metadata.
    UnidicArtifactMetadata(UnidicArtifactMetadataArgs),
    /// Generate a YAML UniDic artifact payload.
    UnidicArtifactPayload(UnidicArtifactPayloadArgs),
    /// Generate SHA-256 checksums for UniDic release assets.
    UnidicArtifactReleaseChecksums(ReleaseChecksumsArgs),
    /// Measure runtime loading and comparison for a UniDic artifact.
    UnidicArtifactRuntimeMeasure(UnidicArtifactRuntimeMeasureArgs),
    /// Verify a UniDic dictionary artifact bundle.
    UnidicArtifactVerify(UnidicArtifactVerifyArgs),
    /// Show UniDic CSV readings for a surface.
    UnidicCsvReadings(UnidicCsvReadingsArgs),
    /// Expand Japanese text into UniDic reading paths.
    UnidicCsvSequences(UnidicCsvSequencesArgs),
    /// Get MeCab readings from a compiled UniDic dictionary.
    UnidicReadings(UnidicReadingsArgs),
}

impl CliCommand {
    fn into_action(self) -> Result<CliAction, CliError> {
        Ok(match self {
            Self::CedictReadings(args) => CliAction::CedictReadings(args.into_options()),
            Self::CedictSequences(args) => CliAction::CedictSequences(args.into_options()),
            Self::ChineseCompare(args) => CliAction::ChineseCompare(args.into_options()),
            Self::Compare(args) => CliAction::Compare(args.into_options()),
            Self::Download(args) => CliAction::Download(args.into_options()),
            Self::List(args) => CliAction::List(args.into_options()),
            Self::Where(args) => CliAction::Where(args.into_options()),
            Self::ZhArtifactArchive(args) => CliAction::ZhArtifactArchive(args.into_options()),
            Self::ZhArtifactBundle(args) => CliAction::ZhArtifactBundle(args.into_options()),
            Self::ZhArtifactInspect(args) => CliAction::ZhArtifactInspect(args.into_options()),
            Self::ZhArtifactMetadata(args) => CliAction::ZhArtifactMetadata(args.into_options()),
            Self::ZhArtifactPayload(args) => CliAction::ZhArtifactPayload(args.into_options()),
            Self::ZhArtifactReleaseChecksums(args) => {
                CliAction::ZhArtifactReleaseChecksums(args.into_zh_options())
            }
            Self::ZhArtifactVerify(args) => CliAction::ZhArtifactVerify(args.into_zh_options()),
            Self::UnidicArtifactArchive(args) => {
                CliAction::UnidicArtifactArchive(args.into_options())
            }
            Self::UnidicArtifactBinaryInspect(args) => {
                CliAction::UnidicArtifactBinaryInspect(args.into_options())
            }
            Self::UnidicArtifactBinaryPayload(args) => {
                CliAction::UnidicArtifactBinaryPayload(args.into_options())
            }
            Self::UnidicArtifactBundle(args) => {
                CliAction::UnidicArtifactBundle(args.into_options())
            }
            Self::UnidicArtifactInspect(args) => {
                CliAction::UnidicArtifactInspect(args.into_options())
            }
            Self::UnidicArtifactMetadata(args) => {
                CliAction::UnidicArtifactMetadata(args.into_options())
            }
            Self::UnidicArtifactPayload(args) => {
                CliAction::UnidicArtifactPayload(args.into_options())
            }
            Self::UnidicArtifactReleaseChecksums(args) => {
                CliAction::UnidicArtifactReleaseChecksums(args.into_unidic_options())
            }
            Self::UnidicArtifactRuntimeMeasure(args) => {
                CliAction::UnidicArtifactRuntimeMeasure(args.into_options()?)
            }
            Self::UnidicArtifactVerify(args) => {
                CliAction::UnidicArtifactVerify(args.into_options())
            }
            Self::UnidicCsvReadings(args) => CliAction::UnidicCsvReadings(args.into_options()),
            Self::UnidicCsvSequences(args) => CliAction::UnidicCsvSequences(args.into_options()),
            Self::UnidicReadings(args) => CliAction::UnidicReadings(args.into_options()),
        })
    }
}

pub(crate) enum CliAction {
    CedictReadings(CedictReadingsOptions),
    CedictSequences(CedictSequencesOptions),
    ChineseCompare(ChineseCompareOptions),
    Compare(CompareOptions),
    Download(DownloadCliOptions),
    List(CacheCliOptions),
    Where(WhereCliOptions),
    ZhArtifactArchive(ZhArtifactArchiveCliOptions),
    ZhArtifactBundle(ZhArtifactBundleCliOptions),
    ZhArtifactInspect(ZhArtifactInspectCliOptions),
    ZhArtifactMetadata(ZhArtifactMetadataCliOptions),
    ZhArtifactPayload(ZhArtifactPayloadCliOptions),
    ZhArtifactReleaseChecksums(ZhArtifactReleaseChecksumsCliOptions),
    ZhArtifactVerify(ZhArtifactVerifyCliOptions),
    UnidicArtifactArchive(UnidicArtifactArchiveCliOptions),
    UnidicArtifactBinaryInspect(UnidicArtifactBinaryInspectCliOptions),
    UnidicArtifactBinaryPayload(UnidicArtifactBinaryPayloadCliOptions),
    UnidicArtifactBundle(UnidicArtifactBundleCliOptions),
    UnidicArtifactInspect(UnidicArtifactInspectCliOptions),
    UnidicArtifactMetadata(UnidicArtifactMetadataCliOptions),
    UnidicArtifactPayload(UnidicArtifactPayloadCliOptions),
    UnidicArtifactReleaseChecksums(UnidicArtifactReleaseChecksumsCliOptions),
    UnidicArtifactRuntimeMeasure(UnidicArtifactRuntimeMeasureCliOptions),
    UnidicArtifactVerify(UnidicArtifactVerifyCliOptions),
    UnidicCsvReadings(UnidicCsvReadingsOptions),
    UnidicCsvSequences(UnidicCsvSequencesOptions),
    UnidicReadings(UnidicReadingsOptions),
}

#[derive(Clone, Debug, Args)]
struct DownloadArgs {
    #[arg(value_parser = parse_artifact_language_clap, value_name = "ja|zh")]
    language: ArtifactLanguage,
    #[arg(long)]
    url: Option<String>,
    #[arg(long = "checksum-url")]
    checksum_url: Option<String>,
    #[arg(long)]
    sha256: Option<String>,
    #[arg(long = "cache-dir")]
    cache_dir: Option<String>,
    #[arg(long)]
    force: bool,
}

impl DownloadArgs {
    fn into_options(self) -> DownloadCliOptions {
        DownloadCliOptions {
            spec: *download_spec_for_language(self.language),
            url: self.url,
            checksum_url: self.checksum_url,
            sha256: self.sha256,
            cache_dir: self.cache_dir,
            force: self.force,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct CacheArgs {
    #[arg(long = "cache-dir")]
    cache_dir: Option<String>,
}

impl CacheArgs {
    fn into_options(self) -> CacheCliOptions {
        CacheCliOptions {
            cache_dir: self.cache_dir,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct WhereArgs {
    #[arg(value_parser = parse_artifact_language_clap, value_name = "ja|zh")]
    language: Option<ArtifactLanguage>,
    #[arg(long = "cache-dir")]
    cache_dir: Option<String>,
}

impl WhereArgs {
    fn into_options(self) -> WhereCliOptions {
        WhereCliOptions {
            language: self.language,
            cache_dir: self.cache_dir,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct PinyinIndexArgs {
    #[arg(long = "pinyin-view", value_parser = parse_pinyin_view_clap, value_name = "no-tone|tone3", default_value = "no-tone")]
    pinyin_view: PinyinView,
    #[arg(long = "max-readings-per-surface")]
    max_readings_per_surface: Option<usize>,
}

impl PinyinIndexArgs {
    fn into_options(self) -> CedictIndexOptions {
        CedictIndexOptions {
            pinyin_view: self.pinyin_view,
            max_readings_per_surface: self.max_readings_per_surface,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct PinyinReadingArgs {
    #[arg(long = "max-readings-per-segment")]
    max_readings_per_segment: Option<usize>,
    #[arg(long = "max-span-chars")]
    max_span_chars: Option<usize>,
    #[arg(long = "max-paths")]
    max_paths: Option<usize>,
    #[arg(long = "longest-only")]
    longest_only: bool,
}

impl PinyinReadingArgs {
    fn into_options(self) -> PinyinReadingOptions {
        let mut options = PinyinReadingOptions::default();
        if let Some(max_readings_per_segment) = self.max_readings_per_segment {
            options.max_readings_per_segment = Some(max_readings_per_segment);
        }
        if let Some(max_span_chars) = self.max_span_chars {
            options.max_span_chars = max_span_chars;
        }
        if let Some(max_paths) = self.max_paths {
            options.max_paths = max_paths;
        }
        if self.longest_only {
            options.longest_match_only = true;
        }
        options
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicIndexArgs {
    #[arg(long = "field", value_parser = parse_unidic_reading_field_clap, value_name = "lform|pron", default_value = "pron")]
    field: UnidicReadingField,
    #[arg(long = "max-readings-per-surface")]
    max_readings_per_surface: Option<usize>,
    #[arg(long = "include-ascii-surfaces")]
    include_ascii_surfaces: bool,
    #[arg(long = "include-symbol-pos")]
    include_symbol_pos: bool,
}

impl UnidicIndexArgs {
    fn into_options(self) -> UnidicIndexOptions {
        let mut options = UnidicIndexOptions {
            reading_field: self.field,
            max_readings_per_surface: self.max_readings_per_surface,
            ..UnidicIndexOptions::default()
        };
        if self.include_ascii_surfaces {
            options.exclude_ascii_surfaces = false;
        }
        if self.include_symbol_pos {
            options.exclude_symbol_pos = false;
        }
        options
    }
}

#[derive(Clone, Copy, Debug, Args)]
struct UnidicReadingArgs {
    #[arg(long = "max-readings-per-segment")]
    max_readings_per_segment: Option<usize>,
    #[arg(long = "max-span-chars")]
    max_span_chars: Option<usize>,
    #[arg(long = "max-paths")]
    max_paths: Option<usize>,
    #[arg(long = "longest-only")]
    longest_only: bool,
}

impl UnidicReadingArgs {
    fn into_options(self) -> DictionaryReadingOptions {
        let mut options = DictionaryReadingOptions::default();
        if let Some(max_readings_per_segment) = self.max_readings_per_segment {
            options.max_readings_per_segment = Some(max_readings_per_segment);
        }
        if let Some(max_span_chars) = self.max_span_chars {
            options.max_span_chars = max_span_chars;
        }
        if let Some(max_paths) = self.max_paths {
            options.max_paths = max_paths;
        }
        if self.longest_only {
            options.longest_match_only = true;
        }
        options
    }

    fn into_overrides(self) -> DictionaryReadingOptionOverrides {
        DictionaryReadingOptionOverrides {
            max_span_chars: self.max_span_chars,
            max_paths: self.max_paths,
            longest_match_only: self.longest_only,
            max_readings_per_segment: self.max_readings_per_segment,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct CedictReadingsArgs {
    #[arg(long)]
    surface: String,
    #[arg(long)]
    cedict: String,
    #[command(flatten)]
    index: PinyinIndexArgs,
}

impl CedictReadingsArgs {
    fn into_options(self) -> CedictReadingsOptions {
        CedictReadingsOptions {
            surface: self.surface,
            cedict: self.cedict,
            index_options: self.index.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct CedictSequencesArgs {
    #[arg(long)]
    text: String,
    #[arg(long)]
    cedict: String,
    #[command(flatten)]
    index: PinyinIndexArgs,
    #[command(flatten)]
    reading: PinyinReadingArgs,
}

impl CedictSequencesArgs {
    fn into_options(self) -> CedictSequencesOptions {
        CedictSequencesOptions {
            text: self.text,
            cedict: self.cedict,
            index_options: self.index.into_options(),
            reading_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
#[command(group(
    ArgGroup::new("zh_source")
        .args(["cedict", "artifact_payload", "artifact_metadata"])
        .required(true),
))]
struct ChineseCompareArgs {
    #[arg(long)]
    left: String,
    #[arg(long)]
    right: String,
    #[arg(long)]
    cedict: Option<String>,
    #[arg(long = "artifact-payload")]
    artifact_payload: Option<String>,
    #[arg(long = "payload-format", value_parser = parse_zh_artifact_payload_format_clap, value_name = "yaml|indexed", default_value = "yaml")]
    payload_format: ArtifactPayloadFormat,
    #[arg(long = "artifact-metadata")]
    artifact_metadata: Option<String>,
    #[command(flatten)]
    index: PinyinIndexArgs,
    #[command(flatten)]
    reading: PinyinReadingArgs,
}

impl ChineseCompareArgs {
    fn into_options(self) -> ChineseCompareOptions {
        let source = if let Some(path) = self.cedict {
            ZhIndexSource::Cedict(path)
        } else if let Some(path) = self.artifact_payload {
            ZhIndexSource::ArtifactPayload {
                path,
                payload_format: self.payload_format,
            }
        } else if let Some(path) = self.artifact_metadata {
            ZhIndexSource::ArtifactMetadata(path)
        } else {
            unreachable!("clap requires exactly one Chinese dictionary source")
        };
        ChineseCompareOptions {
            left: self.left,
            right: self.right,
            source,
            index_options: self.index.into_options(),
            reading_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
#[command(
    group(
        ArgGroup::new("comparison_method")
            .args(["overrides", "lex_csv", "artifact_payload", "artifact_metadata"])
            .required(true)
            .multiple(true),
    ),
    group(ArgGroup::new("dictionary_source").args([
        "lex_csv",
        "artifact_payload",
        "artifact_metadata",
    ])),
)]
struct CompareArgs {
    #[arg(long)]
    left: String,
    #[arg(long)]
    right: String,
    #[arg(long)]
    overrides: Option<String>,
    #[arg(long = "lex-csv")]
    lex_csv: Option<String>,
    #[arg(long = "artifact-payload")]
    artifact_payload: Option<String>,
    #[arg(long = "artifact-metadata")]
    artifact_metadata: Option<String>,
    #[arg(long = "payload-format", value_parser = parse_artifact_payload_format_clap, value_name = "yaml|binary|indexed", default_value = "yaml")]
    payload_format: ArtifactPayloadFormat,
    #[command(flatten)]
    index: UnidicIndexArgs,
    #[command(flatten)]
    reading: UnidicReadingArgs,
}

impl CompareArgs {
    fn into_options(self) -> CompareOptions {
        CompareOptions {
            left: self.left,
            right: self.right,
            overrides: self.overrides,
            lex_csv: self.lex_csv,
            artifact_payload: self.artifact_payload,
            artifact_metadata: self.artifact_metadata,
            payload_format: self.payload_format,
            index_options: self.index.into_options(),
            dictionary_options: self.reading.into_options(),
            dictionary_option_overrides: self.reading.into_overrides(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ZhArtifactMetadataArgs {
    #[arg(long)]
    cedict: String,
    #[arg(long)]
    output: Option<String>,
    #[arg(long = "artifact-name", default_value = "moine-cedict-reading-index")]
    artifact_name: String,
    #[arg(long = "payload-file-name")]
    payload_file_name: Option<String>,
    #[arg(long = "payload-format", value_parser = parse_zh_artifact_payload_format_clap, value_name = "yaml|indexed", default_value = "indexed")]
    payload_format: ArtifactPayloadFormat,
    #[arg(long = "source-name", default_value = "CC-CEDICT")]
    source_name: String,
    #[arg(long = "source-version")]
    source_version: String,
    #[command(flatten)]
    index: PinyinIndexArgs,
    #[command(flatten)]
    reading: PinyinReadingArgs,
}

impl ZhArtifactMetadataArgs {
    fn into_options(self) -> ZhArtifactMetadataCliOptions {
        let payload_file_name = self.payload_file_name.unwrap_or_else(|| {
            default_zh_payload_file_name(&self.artifact_name, self.payload_format)
        });
        ZhArtifactMetadataCliOptions {
            cedict: self.cedict,
            output: self.output,
            artifact_name: self.artifact_name,
            payload_file_name,
            payload_format: self.payload_format,
            source_name: self.source_name,
            source_version: self.source_version,
            index_options: self.index.into_options(),
            reading_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ZhArtifactBundleArgs {
    #[arg(long)]
    cedict: String,
    #[arg(long = "output-dir")]
    output_dir: String,
    #[arg(long = "artifact-name", default_value = "moine-cedict-reading-index")]
    artifact_name: String,
    #[arg(long = "payload-format", value_parser = parse_zh_artifact_payload_format_clap, value_name = "yaml|indexed", default_value = "indexed")]
    payload_format: ArtifactPayloadFormat,
    #[arg(long = "source-name", default_value = "CC-CEDICT")]
    source_name: String,
    #[arg(long = "source-version")]
    source_version: String,
    #[arg(long = "license-file")]
    license_file: Option<String>,
    #[command(flatten)]
    index: PinyinIndexArgs,
    #[command(flatten)]
    reading: PinyinReadingArgs,
}

impl ZhArtifactBundleArgs {
    fn into_options(self) -> ZhArtifactBundleCliOptions {
        ZhArtifactBundleCliOptions {
            cedict: self.cedict,
            output_dir: self.output_dir,
            artifact_name: self.artifact_name,
            payload_format: self.payload_format,
            source_name: self.source_name,
            source_version: self.source_version,
            license_file: self.license_file,
            index_options: self.index.into_options(),
            reading_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ZhArtifactPayloadArgs {
    #[arg(long)]
    cedict: String,
    #[arg(long)]
    output: Option<String>,
    #[arg(long = "payload-format", value_parser = parse_zh_artifact_payload_format_clap, value_name = "yaml|indexed", default_value = "yaml")]
    payload_format: ArtifactPayloadFormat,
    #[command(flatten)]
    index: PinyinIndexArgs,
}

impl ZhArtifactPayloadArgs {
    fn into_options(self) -> ZhArtifactPayloadCliOptions {
        ZhArtifactPayloadCliOptions {
            cedict: self.cedict,
            output: self.output,
            payload_format: self.payload_format,
            index_options: self.index.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ZhArtifactArchiveArgs {
    #[arg(long)]
    metadata: String,
    #[arg(long)]
    output: String,
    #[arg(long = "bundle-dir")]
    bundle_dir: Option<String>,
    #[arg(long = "root-name")]
    root_name: Option<String>,
    #[arg(long, value_parser = parse_archive_compression_clap, value_name = "none|gzip|zstd", default_value = "none")]
    compression: ArchiveCompression,
}

impl ZhArtifactArchiveArgs {
    fn into_options(self) -> ZhArtifactArchiveCliOptions {
        ZhArtifactArchiveCliOptions {
            metadata: self.metadata,
            output: self.output,
            bundle_dir: self.bundle_dir,
            root_name: self.root_name,
            compression: self.compression,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ZhArtifactInspectArgs {
    #[arg(long)]
    payload: String,
    #[arg(long = "payload-format", value_parser = parse_zh_artifact_payload_format_clap, value_name = "yaml|indexed", default_value = "yaml")]
    payload_format: ArtifactPayloadFormat,
}

impl ZhArtifactInspectArgs {
    fn into_options(self) -> ZhArtifactInspectCliOptions {
        ZhArtifactInspectCliOptions {
            payload: self.payload,
            payload_format: self.payload_format,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ArtifactVerifyArgs {
    #[arg(long)]
    metadata: String,
    #[arg(long = "bundle-dir")]
    bundle_dir: Option<String>,
}

impl ArtifactVerifyArgs {
    fn into_zh_options(self) -> ZhArtifactVerifyCliOptions {
        ZhArtifactVerifyCliOptions {
            metadata: self.metadata,
            bundle_dir: self.bundle_dir,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicCsvSequencesArgs {
    #[arg(long)]
    text: String,
    #[arg(long = "lex-csv")]
    lex_csv: String,
    #[command(flatten)]
    index: UnidicIndexArgs,
    #[command(flatten)]
    reading: UnidicReadingArgs,
}

impl UnidicCsvSequencesArgs {
    fn into_options(self) -> UnidicCsvSequencesOptions {
        UnidicCsvSequencesOptions {
            text: self.text,
            lex_csv: self.lex_csv,
            index_options: self.index.into_options(),
            dictionary_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicCsvReadingsArgs {
    #[arg(long)]
    surface: String,
    #[arg(long = "lex-csv")]
    lex_csv: String,
    #[command(flatten)]
    index: UnidicIndexArgs,
}

impl UnidicCsvReadingsArgs {
    fn into_options(self) -> UnidicCsvReadingsOptions {
        UnidicCsvReadingsOptions {
            surface: self.surface,
            lex_csv: self.lex_csv,
            index_options: self.index.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactMetadataArgs {
    #[arg(long = "lex-csv")]
    lex_csv: String,
    #[arg(long)]
    output: Option<String>,
    #[arg(long = "artifact-name", default_value = "moine-unidic-reading-index")]
    artifact_name: String,
    #[arg(long = "payload-file-name")]
    payload_file_name: Option<String>,
    #[arg(long = "payload-format", value_parser = parse_artifact_payload_format_clap, value_name = "yaml|binary|indexed", default_value = "yaml")]
    payload_format: ArtifactPayloadFormat,
    #[arg(long = "source-name", default_value = "UniDic-CWJ")]
    source_name: String,
    #[arg(long = "source-version")]
    source_version: String,
    #[command(flatten)]
    index: UnidicIndexArgs,
    #[command(flatten)]
    reading: UnidicReadingArgs,
}

impl UnidicArtifactMetadataArgs {
    fn into_options(self) -> UnidicArtifactMetadataCliOptions {
        let payload_file_name = self.payload_file_name.unwrap_or_else(|| {
            default_unidic_payload_file_name(&self.artifact_name, self.payload_format)
        });
        UnidicArtifactMetadataCliOptions {
            lex_csv: self.lex_csv,
            output: self.output,
            artifact_name: self.artifact_name,
            payload_file_name,
            payload_format: self.payload_format,
            source_name: self.source_name,
            source_version: self.source_version,
            index_options: self.index.into_options(),
            dictionary_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactBundleArgs {
    #[arg(long = "lex-csv")]
    lex_csv: String,
    #[arg(long = "output-dir")]
    output_dir: String,
    #[arg(long = "artifact-name", default_value = "moine-unidic-reading-index")]
    artifact_name: String,
    #[arg(long = "payload-format", value_parser = parse_artifact_payload_format_clap, value_name = "yaml|binary|indexed", default_value = "yaml")]
    payload_format: ArtifactPayloadFormat,
    #[arg(long = "source-name", default_value = "UniDic-CWJ")]
    source_name: String,
    #[arg(long = "source-version")]
    source_version: String,
    #[arg(long = "license-dir")]
    license_dir: Option<String>,
    #[command(flatten)]
    index: UnidicIndexArgs,
    #[command(flatten)]
    reading: UnidicReadingArgs,
}

impl UnidicArtifactBundleArgs {
    fn into_options(self) -> UnidicArtifactBundleCliOptions {
        UnidicArtifactBundleCliOptions {
            lex_csv: self.lex_csv,
            output_dir: self.output_dir,
            artifact_name: self.artifact_name,
            payload_format: self.payload_format,
            source_name: self.source_name,
            source_version: self.source_version,
            license_dir: self.license_dir,
            index_options: self.index.into_options(),
            dictionary_options: self.reading.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactArchiveArgs {
    #[arg(long)]
    metadata: String,
    #[arg(long)]
    output: String,
    #[arg(long = "bundle-dir")]
    bundle_dir: Option<String>,
    #[arg(long = "root-name")]
    root_name: Option<String>,
    #[arg(long, value_parser = parse_archive_compression_clap, value_name = "none|gzip|zstd", default_value = "none")]
    compression: ArchiveCompression,
}

impl UnidicArtifactArchiveArgs {
    fn into_options(self) -> UnidicArtifactArchiveCliOptions {
        UnidicArtifactArchiveCliOptions {
            metadata: self.metadata,
            output: self.output,
            bundle_dir: self.bundle_dir,
            root_name: self.root_name,
            compression: self.compression,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactBinaryPayloadArgs {
    #[arg(long = "lex-csv")]
    lex_csv: String,
    #[arg(long)]
    output: String,
    #[command(flatten)]
    index: UnidicIndexArgs,
}

impl UnidicArtifactBinaryPayloadArgs {
    fn into_options(self) -> UnidicArtifactBinaryPayloadCliOptions {
        UnidicArtifactBinaryPayloadCliOptions {
            lex_csv: self.lex_csv,
            output: self.output,
            index_options: self.index.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactBinaryInspectArgs {
    #[arg(long)]
    payload: String,
    #[arg(long)]
    timing: bool,
}

impl UnidicArtifactBinaryInspectArgs {
    fn into_options(self) -> UnidicArtifactBinaryInspectCliOptions {
        UnidicArtifactBinaryInspectCliOptions {
            payload: self.payload,
            timing: self.timing,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactPayloadArgs {
    #[arg(long = "lex-csv")]
    lex_csv: String,
    #[arg(long)]
    output: Option<String>,
    #[command(flatten)]
    index: UnidicIndexArgs,
}

impl UnidicArtifactPayloadArgs {
    fn into_options(self) -> UnidicArtifactPayloadCliOptions {
        UnidicArtifactPayloadCliOptions {
            lex_csv: self.lex_csv,
            output: self.output,
            index_options: self.index.into_options(),
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactInspectArgs {
    #[arg(long)]
    payload: String,
}

impl UnidicArtifactInspectArgs {
    fn into_options(self) -> UnidicArtifactInspectCliOptions {
        UnidicArtifactInspectCliOptions {
            payload: self.payload,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactVerifyArgs {
    #[arg(long)]
    metadata: String,
    #[arg(long = "bundle-dir")]
    bundle_dir: Option<String>,
    #[arg(long = "canonical-checksum")]
    canonical_checksum: bool,
}

impl UnidicArtifactVerifyArgs {
    fn into_options(self) -> UnidicArtifactVerifyCliOptions {
        UnidicArtifactVerifyCliOptions {
            metadata: self.metadata,
            bundle_dir: self.bundle_dir,
            canonical_checksum: self.canonical_checksum,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct ReleaseChecksumsArgs {
    #[arg(long = "asset", required = true, value_name = "PATH_TO_RELEASE_ASSET")]
    assets: Vec<String>,
    #[arg(long)]
    output: Option<String>,
}

impl ReleaseChecksumsArgs {
    fn into_unidic_options(self) -> UnidicArtifactReleaseChecksumsCliOptions {
        UnidicArtifactReleaseChecksumsCliOptions {
            assets: self.assets,
            output: self.output,
        }
    }

    fn into_zh_options(self) -> ZhArtifactReleaseChecksumsCliOptions {
        ZhArtifactReleaseChecksumsCliOptions {
            assets: self.assets,
            output: self.output,
        }
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicArtifactRuntimeMeasureArgs {
    #[arg(long)]
    metadata: String,
    #[arg(long = "bundle-dir")]
    bundle_dir: Option<String>,
    #[arg(long)]
    left: Option<String>,
    #[arg(long)]
    right: Option<String>,
    #[arg(long = "pair", num_args = 2)]
    pair: Vec<String>,
    #[arg(long, default_value_t = 5)]
    warmups: usize,
    #[arg(long, default_value_t = 100)]
    iterations: usize,
}

impl UnidicArtifactRuntimeMeasureArgs {
    fn into_options(self) -> Result<UnidicArtifactRuntimeMeasureCliOptions, CliError> {
        if self.iterations == 0 {
            return Err(CliError::InvalidArgumentValue {
                name: "--iterations",
                value: "0".to_string(),
                expected: "a positive integer",
            });
        }
        let mut pairs = self
            .pair
            .chunks_exact(2)
            .map(|chunk| RuntimeMeasurePair {
                left: chunk[0].clone(),
                right: chunk[1].clone(),
            })
            .collect::<Vec<_>>();
        match (self.left, self.right) {
            (Some(left), Some(right)) => pairs.push(RuntimeMeasurePair { left, right }),
            (None, None) => {}
            (None, Some(_)) => return Err(CliError::MissingArgument("--left")),
            (Some(_), None) => return Err(CliError::MissingArgument("--right")),
        }
        if pairs.is_empty() {
            return Err(CliError::MissingArgument("--pair"));
        }
        Ok(UnidicArtifactRuntimeMeasureCliOptions {
            metadata: self.metadata,
            bundle_dir: self.bundle_dir,
            pairs,
            warmups: self.warmups,
            iterations: self.iterations,
        })
    }
}

#[derive(Clone, Debug, Args)]
struct UnidicReadingsArgs {
    #[arg(long)]
    text: String,
    #[arg(long = "dic-dir")]
    dic_dir: String,
}

impl UnidicReadingsArgs {
    fn into_options(self) -> UnidicReadingsOptions {
        UnidicReadingsOptions {
            text: self.text,
            dic_dir: self.dic_dir,
        }
    }
}

macro_rules! impl_parse_with_command {
    ($options:ty, $command:literal, $variant:ident) => {
        impl $options {
            #[cfg(test)]
            #[allow(dead_code)]
            pub(crate) fn parse(args: Vec<String>) -> Result<Self, Box<dyn Error>> {
                match Cli::parse_from_args(std::iter::once($command.to_string()).chain(args))?
                    .into_action()?
                {
                    CliAction::$variant(options) => Ok(options),
                    _ => unreachable!("subcommand parser returned the wrong action"),
                }
            }
        }
    };
}

impl_parse_with_command!(DownloadCliOptions, "download", Download);
impl_parse_with_command!(CacheCliOptions, "list", List);
impl_parse_with_command!(WhereCliOptions, "where", Where);
impl_parse_with_command!(CedictReadingsOptions, "cedict-readings", CedictReadings);
impl_parse_with_command!(CedictSequencesOptions, "cedict-sequences", CedictSequences);
impl_parse_with_command!(ChineseCompareOptions, "chinese-compare", ChineseCompare);
impl_parse_with_command!(CompareOptions, "compare", Compare);
impl_parse_with_command!(
    ZhArtifactMetadataCliOptions,
    "zh-artifact-metadata",
    ZhArtifactMetadata
);
impl_parse_with_command!(
    ZhArtifactBundleCliOptions,
    "zh-artifact-bundle",
    ZhArtifactBundle
);
impl_parse_with_command!(
    ZhArtifactPayloadCliOptions,
    "zh-artifact-payload",
    ZhArtifactPayload
);
impl_parse_with_command!(
    ZhArtifactArchiveCliOptions,
    "zh-artifact-archive",
    ZhArtifactArchive
);
impl_parse_with_command!(
    ZhArtifactInspectCliOptions,
    "zh-artifact-inspect",
    ZhArtifactInspect
);
impl_parse_with_command!(
    ZhArtifactVerifyCliOptions,
    "zh-artifact-verify",
    ZhArtifactVerify
);
impl_parse_with_command!(
    UnidicCsvSequencesOptions,
    "unidic-csv-sequences",
    UnidicCsvSequences
);
impl_parse_with_command!(
    UnidicCsvReadingsOptions,
    "unidic-csv-readings",
    UnidicCsvReadings
);
impl_parse_with_command!(
    UnidicArtifactMetadataCliOptions,
    "unidic-artifact-metadata",
    UnidicArtifactMetadata
);
impl_parse_with_command!(
    UnidicArtifactBundleCliOptions,
    "unidic-artifact-bundle",
    UnidicArtifactBundle
);
impl_parse_with_command!(
    UnidicArtifactArchiveCliOptions,
    "unidic-artifact-archive",
    UnidicArtifactArchive
);
impl_parse_with_command!(
    UnidicArtifactBinaryPayloadCliOptions,
    "unidic-artifact-binary-payload",
    UnidicArtifactBinaryPayload
);
impl_parse_with_command!(
    UnidicArtifactBinaryInspectCliOptions,
    "unidic-artifact-binary-inspect",
    UnidicArtifactBinaryInspect
);
impl_parse_with_command!(
    UnidicArtifactPayloadCliOptions,
    "unidic-artifact-payload",
    UnidicArtifactPayload
);
impl_parse_with_command!(
    UnidicArtifactInspectCliOptions,
    "unidic-artifact-inspect",
    UnidicArtifactInspect
);
impl_parse_with_command!(
    UnidicArtifactVerifyCliOptions,
    "unidic-artifact-verify",
    UnidicArtifactVerify
);
impl_parse_with_command!(
    UnidicArtifactReleaseChecksumsCliOptions,
    "unidic-artifact-release-checksums",
    UnidicArtifactReleaseChecksums
);
impl_parse_with_command!(
    ZhArtifactReleaseChecksumsCliOptions,
    "zh-artifact-release-checksums",
    ZhArtifactReleaseChecksums
);
impl_parse_with_command!(
    UnidicArtifactRuntimeMeasureCliOptions,
    "unidic-artifact-runtime-measure",
    UnidicArtifactRuntimeMeasure
);
impl_parse_with_command!(UnidicReadingsOptions, "unidic-readings", UnidicReadings);

impl ArtifactPayloadFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Yaml => YAML_PAYLOAD_FORMAT,
            Self::Binary => BINARY_PAYLOAD_FORMAT,
            Self::Indexed => INDEXED_PAYLOAD_FORMAT,
        }
    }
}

pub(crate) fn parse_artifact_payload_format(
    value: &str,
) -> Result<ArtifactPayloadFormat, CliError> {
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

pub(crate) fn parse_zh_artifact_payload_format(
    value: &str,
) -> Result<ArtifactPayloadFormat, CliError> {
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

pub(crate) fn parse_archive_compression(value: &str) -> Result<ArchiveCompression, CliError> {
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

pub(crate) fn default_unidic_payload_file_name(
    artifact_name: &str,
    payload_format: ArtifactPayloadFormat,
) -> String {
    match payload_format {
        ArtifactPayloadFormat::Yaml => format!("{artifact_name}.readings.yaml"),
        ArtifactPayloadFormat::Binary => format!("{artifact_name}.readings.moinebin"),
        ArtifactPayloadFormat::Indexed => format!("{artifact_name}.readings.moineidx"),
    }
}

pub(crate) fn default_zh_payload_file_name(
    artifact_name: &str,
    payload_format: ArtifactPayloadFormat,
) -> String {
    match payload_format {
        ArtifactPayloadFormat::Yaml => format!("{artifact_name}.readings.yaml"),
        ArtifactPayloadFormat::Binary => format!("{artifact_name}.readings.moinebin"),
        ArtifactPayloadFormat::Indexed => format!("{artifact_name}.readings.moineidx"),
    }
}

pub(crate) fn default_unidic_license_dir(lex_csv: &str) -> PathBuf {
    Path::new(lex_csv)
        .parent()
        .map(|parent| parent.join("license"))
        .unwrap_or_else(|| PathBuf::from("license"))
}

pub(crate) fn parse_artifact_language(value: &str) -> Result<ArtifactLanguage, CliError> {
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

pub(crate) fn download_spec_for_language(
    language: ArtifactLanguage,
) -> &'static DownloadArtifactSpec {
    DOWNLOAD_ARTIFACT_SPECS
        .iter()
        .find(|spec| spec.language == language)
        .expect("download spec should exist for language")
}

pub(crate) fn parse_pinyin_view(value: &str) -> Result<PinyinView, CliError> {
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

pub(crate) fn parse_unidic_reading_field(value: &str) -> Result<UnidicReadingField, CliError> {
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

pub(crate) fn unidic_reading_field_name(field: UnidicReadingField) -> &'static str {
    field.as_str()
}

pub(crate) fn max_readings_per_surface_label(max_readings: Option<usize>) -> String {
    max_readings
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unbounded".to_string())
}

pub(crate) fn max_readings_per_segment_label(max_readings: Option<usize>) -> String {
    max_readings
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unbounded".to_string())
}

fn parse_artifact_language_clap(value: &str) -> Result<ArtifactLanguage, String> {
    parse_artifact_language(value).map_err(|err| err.to_string())
}

fn parse_pinyin_view_clap(value: &str) -> Result<PinyinView, String> {
    parse_pinyin_view(value).map_err(|err| err.to_string())
}

fn parse_unidic_reading_field_clap(value: &str) -> Result<UnidicReadingField, String> {
    parse_unidic_reading_field(value).map_err(|err| err.to_string())
}

fn parse_artifact_payload_format_clap(value: &str) -> Result<ArtifactPayloadFormat, String> {
    parse_artifact_payload_format(value).map_err(|err| err.to_string())
}

fn parse_zh_artifact_payload_format_clap(value: &str) -> Result<ArtifactPayloadFormat, String> {
    parse_zh_artifact_payload_format(value).map_err(|err| err.to_string())
}

fn parse_archive_compression_clap(value: &str) -> Result<ArchiveCompression, String> {
    parse_archive_compression(value).map_err(|err| err.to_string())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CliError {
    MissingArgument(&'static str),
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
    ArtifactVerificationFailed(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingArgument(arg) => write!(f, "missing required argument {arg}"),
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
            Self::ArtifactVerificationFailed(message) => {
                write!(f, "artifact verification failed: {message}")
            }
        }
    }
}

impl Error for CliError {}
