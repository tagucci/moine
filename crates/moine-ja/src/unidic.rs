use std::borrow::Cow;
use std::collections::{btree_map::Entry, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::string::FromUtf8Error;
use std::sync::Arc;

use fst::{Map, MapBuilder, Streamer};
use memmap2::Mmap;
use moine_core::Lattice;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::romaji::{
    can_build_romaji_paths, romaji_paths_from_reading_segments,
    romaji_symbol_paths_from_reading_segments, JaLatticeError,
};

const SURFACE_COLUMN: usize = 0;
const POS1_COLUMN: usize = 4;
const LFORM_COLUMN: usize = 10;
const PRON_COLUMN: usize = 13;
const ARTIFACT_PAYLOAD_SCHEMA_VERSION: u32 = 1;
const ARTIFACT_PAYLOAD_TYPE: &str = "moine.unidic.reading-index.surface-readings";
const BINARY_ARTIFACT_MAGIC: &[u8; 8] = b"MOINEU01";
const BINARY_ARTIFACT_VERSION: u32 = 1;
const INDEXED_ARTIFACT_MAGIC: &[u8; 8] = b"MOINEI01";
const INDEXED_ARTIFACT_VERSION: u32 = 1;
const INDEXED_ARTIFACT_HEADER_LEN: usize = 40;
const MAX_ARTIFACT_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARTIFACT_ENTRIES: usize = 3_000_000;
const MAX_ARTIFACT_READINGS_PER_ENTRY: usize = 256;
const MAX_ARTIFACT_STRING_BYTES: usize = 16 * 1024;
/// Current canonical checksum algorithm for normalized UniDic payload content.
pub const ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM: &str = "sha256-canonical-v1";
/// Legacy canonical checksum algorithm accepted for older UniDic artifacts.
pub const LEGACY_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM: &str = "fnv1a64-canonical-v1";
/// File digest algorithm used to verify payload bytes before loading.
pub const ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM: &str = "sha256-file-v1";

/// UniDic-derived surface-to-reading index.
#[derive(Clone, Debug)]
pub struct UnidicReadingIndex {
    storage: UnidicReadingStorage,
}

#[derive(Clone, Debug)]
enum UnidicReadingStorage {
    Eager(HashMap<String, Vec<String>>),
    Indexed(IndexedUnidicPayload),
}

impl Default for UnidicReadingIndex {
    fn default() -> Self {
        Self {
            storage: UnidicReadingStorage::Eager(HashMap::new()),
        }
    }
}

impl PartialEq for UnidicReadingIndex {
    fn eq(&self, other: &Self) -> bool {
        self.artifact_payload() == other.artifact_payload()
    }
}

impl Eq for UnidicReadingIndex {}

#[derive(Clone, Debug)]
struct IndexedUnidicPayload {
    mmap: Arc<Mmap>,
    map: Map<Vec<u8>>,
    readings_start: usize,
    entries: usize,
}

/// Header for indexed FST UniDic payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnidicIndexedArtifactPayloadHeader {
    /// Indexed payload format version.
    pub version: u32,
    /// Number of entries in the payload.
    pub entries: usize,
    /// Length of the embedded FST section in bytes.
    pub fst_len: usize,
    /// Length of the reading blob section in bytes.
    pub readings_len: usize,
}

/// Header for legacy binary UniDic payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnidicBinaryArtifactPayloadHeader {
    /// Binary payload format version.
    pub version: u32,
    /// Number of entries in the payload.
    pub entries: usize,
}

/// Controls dictionary reading-path expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DictionaryReadingOptions {
    /// Maximum surface span length considered for one dictionary segment.
    pub max_span_chars: usize,
    /// Maximum complete reading paths to keep.
    pub max_paths: usize,
    /// Prefer the longest dictionary span when multiple spans start together.
    pub longest_match_only: bool,
    /// Optional cap on readings used per dictionary segment.
    pub max_readings_per_segment: Option<usize>,
}

/// One surface segment and its selected UniDic reading.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictionaryReadingSegment {
    /// Surface text covered by the segment.
    pub surface: String,
    /// Reading selected for the segment.
    pub reading: String,
}

/// One complete segmentation and joined reading for an input string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictionaryReadingPath {
    /// Ordered dictionary/direct segments in the path.
    pub segments: Vec<DictionaryReadingSegment>,
    /// Segment readings concatenated into one reading string.
    pub joined_reading: String,
}

/// Reading-path expansion result plus pruning statistics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DictionaryReadingExpansion {
    /// Expanded reading paths.
    pub paths: Vec<DictionaryReadingPath>,
    /// Statistics gathered during expansion.
    pub stats: DictionaryReadingStats,
}

/// Counters describing dictionary reading-path expansion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DictionaryReadingStats {
    /// Dictionary spans matched during expansion.
    pub matched_spans: usize,
    /// Direct fallback spans used when no dictionary span matched.
    pub direct_fallback_spans: usize,
    /// Candidate spans pruned by longest-match mode.
    pub longest_match_pruned_spans: usize,
    /// Raw readings seen before per-segment pruning.
    pub raw_segment_readings: usize,
    /// Readings retained after per-segment pruning.
    pub used_segment_readings: usize,
    /// Readings removed by per-segment pruning.
    pub pruned_segment_readings: usize,
    /// Candidate path combinations considered.
    pub candidate_combinations: usize,
    /// Unique complete reading paths retained.
    pub unique_paths: usize,
    /// Duplicate joined readings removed.
    pub duplicate_joined_readings: usize,
    /// Number of times the `max_paths` cap was hit.
    pub max_paths_hit_count: usize,
}

/// Builds a compact romaji lattice from dictionary reading paths.
pub fn romaji_lattice_from_reading_paths(
    paths: &[DictionaryReadingPath],
) -> Result<Lattice, JaLatticeError> {
    if paths.is_empty() {
        return Err(JaLatticeError::EmptyReadings);
    }

    let paths = romaji_symbol_paths_from_reading_segments(
        paths
            .iter()
            .map(|path| path.segments.iter().map(|segment| segment.reading.as_str())),
    )?;
    Ok(Lattice::from_symbol_paths_compact(paths))
}

/// Expands dictionary reading paths into explicit romaji strings.
pub fn romaji_paths_from_reading_paths(
    paths: &[DictionaryReadingPath],
) -> Result<Vec<String>, JaLatticeError> {
    if paths.is_empty() {
        return Err(JaLatticeError::EmptyReadings);
    }

    romaji_paths_from_reading_segments(
        paths
            .iter()
            .map(|path| path.segments.iter().map(|segment| segment.reading.as_str())),
    )
}

/// UniDic CSV field used as the source reading.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnidicReadingField {
    /// Lemma-form reading column.
    LForm,
    /// Pronunciation column.
    Pron,
}

/// Metadata stored in a UniDic dictionary bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Artifact type identifier.
    pub artifact_type: String,
    /// Human-readable artifact name.
    pub artifact_name: String,
    /// Tool or command that generated the artifact.
    pub generator: String,
    /// Payload file metadata.
    pub payload: UnidicArtifactPayload,
    /// Source dictionary metadata.
    pub source: UnidicArtifactSource,
    /// Build-time options and counts.
    pub build: UnidicArtifactBuild,
    /// Default query options for this artifact.
    pub query_defaults: UnidicArtifactQueryDefaults,
    /// License metadata and references.
    pub license: UnidicArtifactLicense,
}

/// Payload file metadata for a UniDic dictionary bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactPayload {
    /// Bundle-relative payload file path.
    pub path: String,
    /// Payload serialization format.
    pub format: String,
    /// Optional digest algorithm for the raw payload file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_digest_algorithm: Option<String>,
    /// Optional digest of the raw payload file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_digest: Option<String>,
    /// Canonical payload checksum algorithm.
    pub checksum_algorithm: String,
    /// Canonical payload checksum.
    pub checksum: String,
}

/// Source dictionary metadata for a UniDic artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactSource {
    /// Source dictionary name.
    pub name: String,
    /// Source dictionary version.
    pub version: String,
    /// Source `lex.csv` path used to build the artifact.
    pub lex_csv: String,
}

/// Build settings and counts recorded in UniDic artifact metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactBuild {
    /// UniDic reading field used for entries.
    pub reading_field: String,
    /// Optional cap applied to readings stored per surface.
    pub max_readings_per_surface: Option<usize>,
    /// Whether ASCII-only surfaces were excluded.
    pub exclude_ascii_surfaces: bool,
    /// Whether symbol part-of-speech entries were excluded.
    pub exclude_symbol_pos: bool,
    /// Whether source normalized-form aliases were added as lookup surfaces.
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_normalized_surfaces: bool,
    /// Whether readings unsupported by the romaji converter were excluded.
    #[serde(default, skip_serializing_if = "is_false")]
    pub exclude_unsupported_readings: bool,
    /// Number of entries in the generated payload.
    pub entries: usize,
}

/// Default reading-path query settings stored in an artifact.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactQueryDefaults {
    /// Maximum surface span length considered for one segment.
    pub max_span_chars: usize,
    /// Maximum complete reading paths to keep.
    pub max_paths: usize,
    /// Whether longest-match-only expansion should be used by default.
    pub longest_match_only: bool,
    /// Optional cap on readings used per segment.
    pub max_readings_per_segment: Option<usize>,
}

/// License metadata for a UniDic-derived artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactLicense {
    /// Selected license label for the artifact.
    pub selected_license: String,
    /// Bundle-relative license or notice files.
    pub references: Vec<UnidicArtifactLicenseReference>,
}

/// One license or notice file referenced by artifact metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicArtifactLicenseReference {
    /// Human-readable reference label.
    pub label: String,
    /// Bundle-relative file path.
    pub path: String,
}

/// Portable YAML representation of a UniDic reading index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicReadingIndexPayload {
    /// Payload schema version.
    pub schema_version: u32,
    /// Payload type identifier.
    pub payload_type: String,
    /// Surface entries and readings.
    pub entries: Vec<UnidicReadingIndexPayloadEntry>,
}

/// One surface entry in a UniDic reading-index payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UnidicReadingIndexPayloadEntry {
    /// Surface form.
    pub surface: String,
    /// Readings associated with the surface form.
    pub readings: Vec<String>,
}

/// Inputs used to generate artifact metadata for an index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnidicArtifactMetadataOptions {
    /// Human-readable artifact name.
    pub artifact_name: String,
    /// Tool or command that generated the artifact.
    pub generator: String,
    /// Bundle-relative payload file name.
    pub payload_file_name: String,
    /// Payload serialization format.
    pub payload_format: String,
    /// Source dictionary name.
    pub source_name: String,
    /// Source dictionary version.
    pub source_version: String,
    /// Source `lex.csv` path.
    pub source_lex_csv: String,
    /// Index build settings.
    pub index_options: UnidicIndexOptions,
    /// Default query settings.
    pub query_defaults: DictionaryReadingOptions,
    /// License metadata and references.
    pub license: UnidicArtifactLicense,
}

/// Options used while building a UniDic reading index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnidicIndexOptions {
    /// UniDic CSV field used as the source reading.
    pub reading_field: UnidicReadingField,
    /// Optional cap on readings stored for each surface form.
    pub max_readings_per_surface: Option<usize>,
    /// Exclude ASCII-only dictionary surfaces.
    pub exclude_ascii_surfaces: bool,
    /// Exclude entries whose coarse part of speech is a symbol.
    pub exclude_symbol_pos: bool,
}

impl UnidicReadingField {
    fn column(self) -> usize {
        match self {
            Self::LForm => LFORM_COLUMN,
            Self::Pron => PRON_COLUMN,
        }
    }

    /// Returns the stable artifact string for this reading field.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LForm => "lform",
            Self::Pron => "pron",
        }
    }
}

impl Default for UnidicIndexOptions {
    fn default() -> Self {
        Self {
            reading_field: UnidicReadingField::Pron,
            max_readings_per_surface: None,
            exclude_ascii_surfaces: true,
            exclude_symbol_pos: true,
        }
    }
}

impl Default for DictionaryReadingOptions {
    fn default() -> Self {
        Self {
            max_span_chars: 8,
            max_paths: 1024,
            longest_match_only: false,
            max_readings_per_segment: None,
        }
    }
}

impl Default for UnidicArtifactLicense {
    fn default() -> Self {
        Self {
            selected_license: "BSD-3-Clause".to_string(),
            references: vec![
                UnidicArtifactLicenseReference {
                    label: "BSD".to_string(),
                    path: "license/BSD".to_string(),
                },
                UnidicArtifactLicenseReference {
                    label: "COPYING".to_string(),
                    path: "license/COPYING".to_string(),
                },
            ],
        }
    }
}

/// Errors returned while reading UniDic CSV resources.
#[derive(Debug)]
pub enum UnidicCsvError {
    /// CSV parser error.
    Csv(csv::Error),
    /// Filesystem or reader error.
    Io(std::io::Error),
    /// A required CSV column was missing.
    MissingColumn {
        /// Zero-based record index.
        record_index: u64,
        /// Required column index.
        column: usize,
        /// Number of columns in the record.
        len: usize,
    },
}

/// Errors returned while reading or validating UniDic artifact payloads.
#[derive(Debug)]
pub enum UnidicArtifactPayloadError {
    /// Filesystem or reader error.
    Io(std::io::Error),
    /// YAML parser error.
    Yaml(serde_yaml::Error),
    /// Binary payload magic did not match the expected value.
    InvalidBinaryMagic {
        /// Magic bytes read from the payload.
        magic: [u8; 8],
    },
    /// Binary payload version is not supported.
    UnsupportedBinaryVersion {
        /// Version read from the payload.
        version: u32,
    },
    /// Reserved binary header field was non-zero.
    NonZeroBinaryReserved {
        /// Reserved value read from the payload.
        value: u32,
    },
    /// Binary payload ended before a field could be read.
    TruncatedBinary {
        /// Field being read.
        field: &'static str,
    },
    /// Binary payload contained invalid UTF-8.
    InvalidBinaryUtf8 {
        /// Field being decoded.
        field: &'static str,
        /// UTF-8 conversion error.
        source: FromUtf8Error,
    },
    /// Binary field length exceeded supported bounds.
    BinaryValueTooLarge {
        /// Field being read.
        field: &'static str,
        /// Field length.
        len: usize,
    },
    /// Binary payload entry count exceeded supported bounds.
    BinaryEntryCountTooLarge {
        /// Entry count read from the payload.
        entries: u64,
    },
    /// Artifact payload exceeded a configured safety limit.
    ArtifactLimitExceeded {
        /// Field whose length or count exceeded the limit.
        field: &'static str,
        /// Observed length or count.
        len: u64,
        /// Maximum allowed length or count.
        max: u64,
    },
    /// Indexed payload magic did not match the expected value.
    InvalidIndexedMagic {
        /// Magic bytes read from the payload.
        magic: [u8; 8],
    },
    /// Indexed payload version is not supported.
    UnsupportedIndexedVersion {
        /// Version read from the payload.
        version: u32,
    },
    /// Reserved indexed header field was non-zero.
    NonZeroIndexedReserved {
        /// Reserved value read from the payload.
        value: u32,
    },
    /// Indexed payload ended before a section could be read.
    TruncatedIndexed {
        /// Field or section being read.
        field: &'static str,
    },
    /// Indexed payload contained an invalid FST section.
    InvalidIndexedFst {
        /// FST error message.
        message: String,
    },
    /// Indexed payload section length exceeded supported bounds.
    IndexedSectionTooLarge {
        /// Section name.
        field: &'static str,
        /// Section length.
        len: u64,
    },
    /// Indexed payload referenced an invalid readings offset.
    InvalidIndexedOffset {
        /// Offset read from the FST value.
        offset: u64,
    },
    /// Indexed payload contained invalid UTF-8.
    InvalidIndexedUtf8 {
        /// Field being decoded.
        field: &'static str,
        /// UTF-8 conversion error.
        source: std::str::Utf8Error,
    },
    /// Indexed header entry count disagreed with the FST entry count.
    IndexedEntryCountMismatch {
        /// Entry count recorded in the header.
        header_entries: usize,
        /// Entry count decoded from the FST.
        fst_entries: usize,
    },
    /// YAML payload schema version is not supported.
    UnsupportedSchemaVersion {
        /// Version read from the payload.
        version: u32,
    },
    /// YAML payload type is not a UniDic reading index.
    UnsupportedPayloadType {
        /// Payload type read from the payload.
        payload_type: String,
    },
    /// Payload entry had an empty surface form.
    EmptySurface {
        /// Zero-based entry index.
        entry_index: usize,
    },
    /// Surface form appeared more than once.
    DuplicateSurface {
        /// Duplicated surface form.
        surface: String,
    },
    /// Payload entry had no readings.
    EmptyReadings {
        /// Surface form for the invalid entry.
        surface: String,
    },
    /// Payload entry contained an empty reading.
    EmptyReading {
        /// Surface form for the invalid entry.
        surface: String,
        /// Zero-based reading index.
        reading_index: usize,
    },
    /// Payload entry contained the same reading more than once.
    DuplicateReading {
        /// Surface form for the invalid entry.
        surface: String,
        /// Duplicated reading.
        reading: String,
    },
}

impl fmt::Display for UnidicCsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Csv(err) => write!(f, "invalid UniDic CSV: {err}"),
            Self::Io(err) => write!(f, "failed to read UniDic CSV: {err}"),
            Self::MissingColumn {
                record_index,
                column,
                len,
            } => write!(
                f,
                "UniDic CSV record {record_index} has no column {column}; record has {len} columns"
            ),
        }
    }
}

impl Error for UnidicCsvError {}

impl fmt::Display for UnidicArtifactPayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read UniDic artifact payload: {err}"),
            Self::Yaml(err) => write!(f, "invalid UniDic artifact payload YAML: {err}"),
            Self::InvalidBinaryMagic { magic } => {
                write!(f, "invalid UniDic binary artifact magic {magic:?}")
            }
            Self::UnsupportedBinaryVersion { version } => {
                write!(f, "unsupported UniDic binary artifact version {version}")
            }
            Self::NonZeroBinaryReserved { value } => {
                write!(f, "UniDic binary artifact reserved header field is {value}")
            }
            Self::TruncatedBinary { field } => {
                write!(f, "truncated UniDic binary artifact while reading {field}")
            }
            Self::InvalidBinaryUtf8 { field, source } => {
                write!(f, "invalid UTF-8 in UniDic binary artifact {field}: {source}")
            }
            Self::BinaryValueTooLarge { field, len } => write!(
                f,
                "UniDic binary artifact {field} length {len} exceeds u32::MAX"
            ),
            Self::BinaryEntryCountTooLarge { entries } => write!(
                f,
                "UniDic binary artifact entry count {entries} exceeds usize::MAX"
            ),
            Self::ArtifactLimitExceeded { field, len, max } => write!(
                f,
                "UniDic artifact {field} length/count {len} exceeds limit {max}"
            ),
            Self::InvalidIndexedMagic { magic } => {
                write!(f, "invalid UniDic indexed artifact magic {magic:?}")
            }
            Self::UnsupportedIndexedVersion { version } => {
                write!(f, "unsupported UniDic indexed artifact version {version}")
            }
            Self::NonZeroIndexedReserved { value } => {
                write!(f, "UniDic indexed artifact reserved header field is {value}")
            }
            Self::TruncatedIndexed { field } => {
                write!(f, "truncated UniDic indexed artifact while reading {field}")
            }
            Self::InvalidIndexedFst { message } => {
                write!(f, "invalid UniDic indexed artifact FST: {message}")
            }
            Self::IndexedSectionTooLarge { field, len } => write!(
                f,
                "UniDic indexed artifact {field} length {len} exceeds usize::MAX"
            ),
            Self::InvalidIndexedOffset { offset } => {
                write!(f, "invalid UniDic indexed artifact readings offset {offset}")
            }
            Self::InvalidIndexedUtf8 { field, source } => {
                write!(f, "invalid UTF-8 in UniDic indexed artifact {field}: {source}")
            }
            Self::IndexedEntryCountMismatch {
                header_entries,
                fst_entries,
            } => write!(
                f,
                "UniDic indexed artifact header entry count {header_entries} does not match FST entry count {fst_entries}"
            ),
            Self::UnsupportedSchemaVersion { version } => write!(
                f,
                "unsupported UniDic artifact payload schema version {version}"
            ),
            Self::UnsupportedPayloadType { payload_type } => {
                write!(f, "unsupported UniDic artifact payload type {payload_type:?}")
            }
            Self::EmptySurface { entry_index } => write!(
                f,
                "UniDic artifact payload entry {entry_index} has an empty surface"
            ),
            Self::DuplicateSurface { surface } => {
                write!(f, "UniDic artifact payload has duplicate surface {surface:?}")
            }
            Self::EmptyReadings { surface } => write!(
                f,
                "UniDic artifact payload surface {surface:?} has no readings"
            ),
            Self::EmptyReading {
                surface,
                reading_index,
            } => write!(
                f,
                "UniDic artifact payload surface {surface:?} has an empty reading at index {reading_index}"
            ),
            Self::DuplicateReading { surface, reading } => write!(
                f,
                "UniDic artifact payload surface {surface:?} has duplicate reading {reading:?}"
            ),
        }
    }
}

impl Error for UnidicArtifactPayloadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Yaml(err) => Some(err),
            Self::InvalidBinaryUtf8 { source, .. } => Some(source),
            Self::InvalidIndexedUtf8 { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<csv::Error> for UnidicCsvError {
    fn from(err: csv::Error) -> Self {
        Self::Csv(err)
    }
}

impl From<std::io::Error> for UnidicCsvError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<std::io::Error> for UnidicArtifactPayloadError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_yaml::Error> for UnidicArtifactPayloadError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::Yaml(err)
    }
}

impl UnidicReadingIndex {
    /// Builds an index from a UniDic `lex.csv` file.
    pub fn from_lex_csv_path(path: impl AsRef<Path>) -> Result<Self, UnidicCsvError> {
        Self::from_lex_csv_path_with_options(path, UnidicIndexOptions::default())
    }

    /// Builds an index from a UniDic `lex.csv` file using a specific reading field.
    pub fn from_lex_csv_path_with_field(
        path: impl AsRef<Path>,
        field: UnidicReadingField,
    ) -> Result<Self, UnidicCsvError> {
        Self::from_lex_csv_path_with_options(
            path,
            UnidicIndexOptions {
                reading_field: field,
                ..UnidicIndexOptions::default()
            },
        )
    }

    /// Builds an index from a UniDic `lex.csv` file with custom options.
    pub fn from_lex_csv_path_with_options(
        path: impl AsRef<Path>,
        options: UnidicIndexOptions,
    ) -> Result<Self, UnidicCsvError> {
        let file = File::open(path)?;
        Self::from_lex_csv_reader_with_options(file, options)
    }

    /// Builds an index from a reader containing UniDic `lex.csv` data.
    pub fn from_lex_csv_reader(reader: impl Read) -> Result<Self, UnidicCsvError> {
        Self::from_lex_csv_reader_with_options(reader, UnidicIndexOptions::default())
    }

    /// Builds an index from a UniDic `lex.csv` reader using a specific reading field.
    pub fn from_lex_csv_reader_with_field(
        reader: impl Read,
        reading_field: UnidicReadingField,
    ) -> Result<Self, UnidicCsvError> {
        Self::from_lex_csv_reader_with_options(
            reader,
            UnidicIndexOptions {
                reading_field,
                ..UnidicIndexOptions::default()
            },
        )
    }

    /// Builds an index from a UniDic `lex.csv` reader with custom options.
    pub fn from_lex_csv_reader_with_options(
        reader: impl Read,
        options: UnidicIndexOptions,
    ) -> Result<Self, UnidicCsvError> {
        let mut by_surface = HashMap::<String, BTreeSet<String>>::new();
        for record in lex_csv_reader(reader).records() {
            let record = record?;
            let surface = field(&record, SURFACE_COLUMN)?;
            let reading = field(&record, options.reading_field.column())?;

            if surface == "*" || reading == "*" {
                continue;
            }
            if options.exclude_ascii_surfaces && surface.is_ascii() {
                continue;
            }
            if options.exclude_symbol_pos && is_symbol_pos(field(&record, POS1_COLUMN)?) {
                continue;
            }

            insert_surface_reading(&mut by_surface, surface, reading);
            if let Some(normalized_surface) = normalize_ascii_width(surface) {
                insert_surface_reading(&mut by_surface, &normalized_surface, reading);
            }
        }

        let readings_by_surface = by_surface
            .into_iter()
            .map(|(surface, readings)| {
                let mut readings = readings.into_iter().collect::<Vec<_>>();
                if let Some(max_readings) = options.max_readings_per_surface {
                    readings.truncate(max_readings);
                }
                (surface, readings)
            })
            .filter(|(_, readings)| !readings.is_empty())
            .collect();

        Ok(Self::from_readings_by_surface(readings_by_surface))
    }

    /// Loads a YAML artifact payload from a file path.
    pub fn from_artifact_payload_path(
        path: impl AsRef<Path>,
    ) -> Result<Self, UnidicArtifactPayloadError> {
        let path = path.as_ref();
        check_payload_file_size(path)?;
        let file = File::open(path)?;
        Self::from_artifact_payload_reader(file)
    }

    /// Loads a YAML artifact payload from a reader.
    pub fn from_artifact_payload_reader(
        reader: impl Read,
    ) -> Result<Self, UnidicArtifactPayloadError> {
        let payload = serde_yaml::from_reader(reader)?;
        Self::from_artifact_payload(payload)
    }

    /// Builds an index from a deserialized artifact payload.
    pub fn from_artifact_payload(
        payload: UnidicReadingIndexPayload,
    ) -> Result<Self, UnidicArtifactPayloadError> {
        validate_artifact_payload_header(&payload)?;
        check_limit("entry_count", payload.entries.len(), MAX_ARTIFACT_ENTRIES)?;

        let mut readings_by_surface = HashMap::new();
        for (entry_index, entry) in payload.entries.into_iter().enumerate() {
            check_limit(
                "surface_bytes",
                entry.surface.len(),
                MAX_ARTIFACT_STRING_BYTES,
            )?;
            check_limit(
                "reading_count",
                entry.readings.len(),
                MAX_ARTIFACT_READINGS_PER_ENTRY,
            )?;
            if entry.surface.is_empty() {
                return Err(UnidicArtifactPayloadError::EmptySurface { entry_index });
            }
            if entry.readings.is_empty() {
                return Err(UnidicArtifactPayloadError::EmptyReadings {
                    surface: entry.surface,
                });
            }

            let mut seen_readings = BTreeSet::new();
            for (reading_index, reading) in entry.readings.iter().enumerate() {
                check_limit("reading_bytes", reading.len(), MAX_ARTIFACT_STRING_BYTES)?;
                if reading.is_empty() {
                    return Err(UnidicArtifactPayloadError::EmptyReading {
                        surface: entry.surface,
                        reading_index,
                    });
                }
                if !seen_readings.insert(reading) {
                    return Err(UnidicArtifactPayloadError::DuplicateReading {
                        surface: entry.surface,
                        reading: reading.clone(),
                    });
                }
            }

            if readings_by_surface
                .insert(entry.surface.clone(), entry.readings)
                .is_some()
            {
                return Err(UnidicArtifactPayloadError::DuplicateSurface {
                    surface: entry.surface,
                });
            }
        }

        Ok(Self::from_readings_by_surface(readings_by_surface))
    }

    /// Loads a binary artifact payload from a file path.
    pub fn from_binary_artifact_payload_path(
        path: impl AsRef<Path>,
    ) -> Result<Self, UnidicArtifactPayloadError> {
        let path = path.as_ref();
        check_payload_file_size(path)?;
        let file = File::open(path)?;
        Self::from_binary_artifact_payload_reader(file)
    }

    /// Loads a binary artifact payload from a reader.
    pub fn from_binary_artifact_payload_reader(
        mut reader: impl Read,
    ) -> Result<Self, UnidicArtifactPayloadError> {
        let header = read_binary_artifact_payload_header(&mut reader)?;
        check_limit("entry_count", header.entries, MAX_ARTIFACT_ENTRIES)?;
        let mut entries = Vec::with_capacity(header.entries);
        for _ in 0..header.entries {
            let surface = read_binary_string(&mut reader, "surface")?;
            let reading_count = read_u32_le(&mut reader, "reading_count")?;
            let reading_count = usize::try_from(reading_count).expect("u32 fits usize");
            check_limit(
                "reading_count",
                reading_count,
                MAX_ARTIFACT_READINGS_PER_ENTRY,
            )?;
            let mut readings = Vec::with_capacity(reading_count);
            for _ in 0..reading_count {
                readings.push(read_binary_string(&mut reader, "reading")?);
            }
            entries.push(UnidicReadingIndexPayloadEntry { surface, readings });
        }

        Self::from_artifact_payload(UnidicReadingIndexPayload {
            schema_version: ARTIFACT_PAYLOAD_SCHEMA_VERSION,
            payload_type: ARTIFACT_PAYLOAD_TYPE.to_string(),
            entries,
        })
    }

    /// Loads an indexed FST artifact payload from a file path.
    pub fn from_indexed_artifact_payload_path(
        path: impl AsRef<Path>,
    ) -> Result<Self, UnidicArtifactPayloadError> {
        let path = path.as_ref();
        check_payload_file_size(path)?;
        let file = File::open(path)?;
        // SAFETY: the mmap is kept alive by IndexedUnidicPayload for as long as
        // any offsets or slices derived from it can be used.
        let mmap = unsafe { Mmap::map(&file)? };
        Self::from_indexed_mmap(mmap)
    }

    /// Loads an indexed artifact payload from bytes.
    ///
    /// This eagerly materializes the indexed payload and is intended for
    /// environments such as WebAssembly where mmap-backed loading is not
    /// available.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload is too large, malformed, truncated,
    /// has an invalid FST section, or fails canonical artifact validation.
    pub fn from_indexed_artifact_payload_bytes(
        bytes: &[u8],
    ) -> Result<Self, UnidicArtifactPayloadError> {
        if bytes.len() as u64 > MAX_ARTIFACT_PAYLOAD_BYTES {
            return Err(UnidicArtifactPayloadError::ArtifactLimitExceeded {
                field: "payload_bytes",
                len: bytes.len() as u64,
                max: MAX_ARTIFACT_PAYLOAD_BYTES,
            });
        }
        let header = read_indexed_artifact_payload_header_bytes(bytes)?;
        let fst_start = INDEXED_ARTIFACT_HEADER_LEN;
        let fst_end = fst_start.checked_add(header.fst_len).ok_or(
            UnidicArtifactPayloadError::TruncatedIndexed {
                field: "fst_section",
            },
        )?;
        let readings_end = fst_end.checked_add(header.readings_len).ok_or(
            UnidicArtifactPayloadError::TruncatedIndexed {
                field: "readings_section",
            },
        )?;
        if bytes.len() < readings_end {
            return Err(UnidicArtifactPayloadError::TruncatedIndexed {
                field: "indexed_payload",
            });
        }

        let map = Map::new(bytes[fst_start..fst_end].to_vec()).map_err(|err| {
            UnidicArtifactPayloadError::InvalidIndexedFst {
                message: err.to_string(),
            }
        })?;
        let fst_entries = map.len();
        if fst_entries != header.entries {
            return Err(UnidicArtifactPayloadError::IndexedEntryCountMismatch {
                header_entries: header.entries,
                fst_entries,
            });
        }

        let mut entries = Vec::with_capacity(header.entries);
        let mut stream = map.stream();
        while let Some((surface, offset)) = stream.next() {
            let surface = std::str::from_utf8(surface)
                .map_err(|source| UnidicArtifactPayloadError::InvalidIndexedUtf8 {
                    field: "surface",
                    source,
                })?
                .to_string();
            let readings = read_indexed_readings_at_bytes(bytes, fst_end, offset)?;
            entries.push(UnidicReadingIndexPayloadEntry { surface, readings });
        }

        Self::from_artifact_payload(UnidicReadingIndexPayload {
            schema_version: ARTIFACT_PAYLOAD_SCHEMA_VERSION,
            payload_type: ARTIFACT_PAYLOAD_TYPE.to_string(),
            entries,
        })
    }

    fn from_indexed_mmap(mmap: Mmap) -> Result<Self, UnidicArtifactPayloadError> {
        if mmap.len() as u64 > MAX_ARTIFACT_PAYLOAD_BYTES {
            return Err(UnidicArtifactPayloadError::ArtifactLimitExceeded {
                field: "payload_bytes",
                len: mmap.len() as u64,
                max: MAX_ARTIFACT_PAYLOAD_BYTES,
            });
        }
        let header = read_indexed_artifact_payload_header_bytes(&mmap)?;
        let fst_start = INDEXED_ARTIFACT_HEADER_LEN;
        let fst_end = fst_start.checked_add(header.fst_len).ok_or(
            UnidicArtifactPayloadError::TruncatedIndexed {
                field: "fst_section",
            },
        )?;
        let readings_end = fst_end.checked_add(header.readings_len).ok_or(
            UnidicArtifactPayloadError::TruncatedIndexed {
                field: "readings_section",
            },
        )?;
        if mmap.len() < readings_end {
            return Err(UnidicArtifactPayloadError::TruncatedIndexed {
                field: "indexed_payload",
            });
        }

        let map = Map::new(mmap[fst_start..fst_end].to_vec()).map_err(|err| {
            UnidicArtifactPayloadError::InvalidIndexedFst {
                message: err.to_string(),
            }
        })?;
        let fst_entries = map.len();
        if fst_entries != header.entries {
            return Err(UnidicArtifactPayloadError::IndexedEntryCountMismatch {
                header_entries: header.entries,
                fst_entries,
            });
        }

        let indexed = IndexedUnidicPayload {
            mmap: Arc::new(mmap),
            map,
            readings_start: fst_end,
            entries: header.entries,
        };
        indexed.validate()?;
        Ok(Self {
            storage: UnidicReadingStorage::Indexed(indexed),
        })
    }

    /// Reads only the header from a binary artifact payload file.
    pub fn binary_artifact_payload_header_path(
        path: impl AsRef<Path>,
    ) -> Result<UnidicBinaryArtifactPayloadHeader, UnidicArtifactPayloadError> {
        let file = File::open(path)?;
        Self::binary_artifact_payload_header_reader(file)
    }

    /// Reads only the header from a binary artifact payload reader.
    pub fn binary_artifact_payload_header_reader(
        mut reader: impl Read,
    ) -> Result<UnidicBinaryArtifactPayloadHeader, UnidicArtifactPayloadError> {
        read_binary_artifact_payload_header(&mut reader)
    }

    pub(crate) fn from_readings_by_surface(
        readings_by_surface: HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            storage: UnidicReadingStorage::Eager(readings_by_surface),
        }
    }

    /// Returns readings for `surface`, if present.
    ///
    /// For indexed artifacts, decode errors are treated the same as a missing
    /// surface for backward compatibility. Use [`Self::try_readings`] at trust
    /// boundaries when artifact corruption must be reported distinctly.
    pub fn readings(&self, surface: &str) -> Option<Cow<'_, [String]>> {
        self.try_readings(surface).ok().flatten()
    }

    /// Returns readings for `surface` and preserves indexed artifact decode
    /// errors.
    pub fn try_readings(
        &self,
        surface: &str,
    ) -> Result<Option<Cow<'_, [String]>>, UnidicArtifactPayloadError> {
        if let Some(readings) = self.try_readings_exact(surface)? {
            return Ok(Some(readings));
        }

        let Some(normalized_surface) = normalize_ascii_width(surface) else {
            return Ok(None);
        };
        if normalized_surface == surface {
            return Ok(None);
        }

        self.try_readings_exact(&normalized_surface)
    }

    fn try_readings_exact(
        &self,
        surface: &str,
    ) -> Result<Option<Cow<'_, [String]>>, UnidicArtifactPayloadError> {
        match &self.storage {
            UnidicReadingStorage::Eager(readings_by_surface) => Ok(readings_by_surface
                .get(surface)
                .map(|readings| Cow::Borrowed(readings.as_slice()))),
            UnidicReadingStorage::Indexed(indexed) => indexed
                .readings(surface)
                .map(|readings| readings.map(Cow::Owned)),
        }
    }

    /// Returns the number of indexed surface forms.
    pub fn len(&self) -> usize {
        match &self.storage {
            UnidicReadingStorage::Eager(readings_by_surface) => readings_by_surface.len(),
            UnidicReadingStorage::Indexed(indexed) => indexed.entries,
        }
    }

    /// Returns `true` when the index contains no surface forms.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Builds bundle metadata for the current index and caller-provided
    /// provenance.
    ///
    /// The returned metadata includes a canonical payload checksum computed
    /// from the normalized payload view.
    pub fn artifact_metadata(
        &self,
        options: UnidicArtifactMetadataOptions,
    ) -> UnidicArtifactMetadata {
        let build = UnidicArtifactBuild {
            reading_field: options.index_options.reading_field.as_str().to_string(),
            max_readings_per_surface: options.index_options.max_readings_per_surface,
            exclude_ascii_surfaces: options.index_options.exclude_ascii_surfaces,
            exclude_symbol_pos: options.index_options.exclude_symbol_pos,
            include_normalized_surfaces: false,
            exclude_unsupported_readings: false,
            entries: self.len(),
        };
        self.artifact_metadata_with_build(options, build)
    }

    /// Builds bundle metadata with caller-provided build provenance.
    ///
    /// This is used by Japanese dictionary sources that reuse the same reading
    /// payload format but do not share UniDic's CSV field layout.
    pub fn artifact_metadata_with_build(
        &self,
        options: UnidicArtifactMetadataOptions,
        mut build: UnidicArtifactBuild,
    ) -> UnidicArtifactMetadata {
        build.entries = self.len();
        UnidicArtifactMetadata {
            schema_version: 1,
            artifact_type: "moine.unidic.reading-index".to_string(),
            artifact_name: options.artifact_name,
            generator: options.generator,
            payload: UnidicArtifactPayload {
                path: options.payload_file_name,
                format: options.payload_format,
                file_digest_algorithm: None,
                file_digest: None,
                checksum_algorithm: ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM.to_string(),
                checksum: self.artifact_payload_checksum(),
            },
            source: UnidicArtifactSource {
                name: options.source_name,
                version: options.source_version,
                lex_csv: options.source_lex_csv,
            },
            build,
            query_defaults: UnidicArtifactQueryDefaults {
                max_span_chars: options.query_defaults.max_span_chars,
                max_paths: options.query_defaults.max_paths,
                longest_match_only: options.query_defaults.longest_match_only,
                max_readings_per_segment: options.query_defaults.max_readings_per_segment,
            },
            license: options.license,
        }
    }

    /// Returns the normalized YAML-compatible payload view for this index.
    ///
    /// Entries are sorted by surface form so serialization and checksums are
    /// deterministic regardless of the index storage backend.
    pub fn artifact_payload(&self) -> UnidicReadingIndexPayload {
        let entries = match &self.storage {
            UnidicReadingStorage::Eager(readings_by_surface) => {
                let mut entries = readings_by_surface
                    .iter()
                    .map(|(surface, readings)| UnidicReadingIndexPayloadEntry {
                        surface: surface.clone(),
                        readings: readings.clone(),
                    })
                    .collect::<Vec<_>>();
                entries.sort_by(|left, right| left.surface.cmp(&right.surface));
                entries
            }
            UnidicReadingStorage::Indexed(indexed) => indexed
                .entries()
                .expect("validated indexed artifact should decode"),
        };

        UnidicReadingIndexPayload {
            schema_version: ARTIFACT_PAYLOAD_SCHEMA_VERSION,
            payload_type: ARTIFACT_PAYLOAD_TYPE.to_string(),
            entries,
        }
    }

    /// Returns the canonical checksum for the normalized payload.
    pub fn artifact_payload_checksum(&self) -> String {
        self.artifact_payload_checksum_for_algorithm(ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM)
            .expect("default artifact checksum algorithm should be supported")
    }

    /// Returns a canonical payload checksum for `algorithm`.
    ///
    /// Supported values are [`ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM`] and
    /// [`LEGACY_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM`]. Unknown algorithms return
    /// `None`.
    pub fn artifact_payload_checksum_for_algorithm(&self, algorithm: &str) -> Option<String> {
        let payload = self.artifact_payload();
        let bytes = canonical_payload_bytes(&payload);
        match algorithm {
            ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM => Some(sha256_hex(&bytes)),
            LEGACY_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM => Some(format!("{:016x}", fnv1a64(&bytes))),
            _ => None,
        }
    }

    /// Writes the legacy binary artifact payload format.
    ///
    /// Prefer [`Self::write_indexed_artifact_payload`] for newly generated
    /// bundles; this format is kept for compatibility with older artifacts.
    pub fn write_artifact_binary_payload(
        &self,
        mut writer: impl Write,
    ) -> Result<(), UnidicArtifactPayloadError> {
        let payload = self.artifact_payload();
        writer.write_all(BINARY_ARTIFACT_MAGIC)?;
        writer.write_all(&BINARY_ARTIFACT_VERSION.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&(payload.entries.len() as u64).to_le_bytes())?;

        for entry in &payload.entries {
            write_binary_string(&mut writer, "surface", &entry.surface)?;
            write_u32_len(&mut writer, "reading_count", entry.readings.len())?;
            for reading in &entry.readings {
                write_binary_string(&mut writer, "reading", reading)?;
            }
        }

        Ok(())
    }

    /// Writes the indexed FST-backed artifact payload format.
    ///
    /// The payload stores a finite-state transducer from surface form to an
    /// offset in a compact reading blob and can be loaded with
    /// [`Self::from_indexed_artifact_payload_path`].
    pub fn write_indexed_artifact_payload(
        &self,
        mut writer: impl Write,
    ) -> Result<(), UnidicArtifactPayloadError> {
        let payload = self.artifact_payload();
        let mut fst_bytes = Vec::new();
        let mut readings_bytes = Vec::new();
        {
            let mut builder = MapBuilder::new(&mut fst_bytes).map_err(|err| {
                UnidicArtifactPayloadError::InvalidIndexedFst {
                    message: err.to_string(),
                }
            })?;
            for entry in &payload.entries {
                let offset = readings_bytes.len() as u64;
                builder.insert(&entry.surface, offset).map_err(|err| {
                    UnidicArtifactPayloadError::InvalidIndexedFst {
                        message: err.to_string(),
                    }
                })?;
                write_indexed_reading_block(&mut readings_bytes, &entry.readings)?;
            }
            builder
                .finish()
                .map_err(|err| UnidicArtifactPayloadError::InvalidIndexedFst {
                    message: err.to_string(),
                })?;
        }

        writer.write_all(INDEXED_ARTIFACT_MAGIC)?;
        writer.write_all(&INDEXED_ARTIFACT_VERSION.to_le_bytes())?;
        writer.write_all(&0_u32.to_le_bytes())?;
        writer.write_all(&(payload.entries.len() as u64).to_le_bytes())?;
        writer.write_all(&(fst_bytes.len() as u64).to_le_bytes())?;
        writer.write_all(&(readings_bytes.len() as u64).to_le_bytes())?;
        writer.write_all(&fst_bytes)?;
        writer.write_all(&readings_bytes)?;
        Ok(())
    }

    /// Expands `text` into joined kana reading strings.
    ///
    /// This is a compatibility helper over [`Self::reading_paths`]. It drops
    /// segment boundaries and treats indexed artifact decode errors as an empty
    /// expansion.
    pub fn reading_sequences(&self, text: &str, options: DictionaryReadingOptions) -> Vec<String> {
        self.reading_sequences_with_stats_inner(text, options, false)
            .unwrap_or_default()
            .paths
    }

    /// Expands `text` into dictionary-only reading paths.
    ///
    /// Every returned path contains surface/reading segment boundaries plus the
    /// joined kana reading. Use [`Self::try_reading_paths_with_stats`] when
    /// indexed artifact corruption must be reported.
    pub fn reading_paths(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> Vec<DictionaryReadingPath> {
        self.reading_paths_with_stats(text, options).paths
    }

    /// Expands dictionary reading paths and treats artifact decode errors as an
    /// empty expansion for backward compatibility.
    ///
    /// Use [`Self::try_reading_paths_with_stats`] when loading indexed
    /// artifacts from outside the process trust boundary.
    pub fn reading_paths_with_stats(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> DictionaryReadingExpansion {
        self.try_reading_paths_with_stats(text, options)
            .unwrap_or_default()
    }

    /// Expands dictionary reading paths and preserves indexed artifact decode
    /// errors.
    pub fn try_reading_paths_with_stats(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> Result<DictionaryReadingExpansion, UnidicArtifactPayloadError> {
        self.reading_paths_with_stats_inner(text, options, false)
    }

    /// Expands `text` into reading paths with direct fallback segments.
    ///
    /// Dictionary matches are preferred, but kana and ASCII spans can pass
    /// through directly so mixed dictionary/direct input can still form a full
    /// path.
    pub fn hybrid_reading_paths(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> Vec<DictionaryReadingPath> {
        self.hybrid_reading_paths_with_stats(text, options).paths
    }

    /// Expands hybrid dictionary/direct reading paths and treats artifact
    /// decode errors as an empty expansion for backward compatibility.
    ///
    /// Use [`Self::try_hybrid_reading_paths_with_stats`] when loading indexed
    /// artifacts from outside the process trust boundary.
    pub fn hybrid_reading_paths_with_stats(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> DictionaryReadingExpansion {
        self.try_hybrid_reading_paths_with_stats(text, options)
            .unwrap_or_default()
    }

    /// Expands hybrid dictionary/direct reading paths and preserves indexed
    /// artifact decode errors.
    pub fn try_hybrid_reading_paths_with_stats(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> Result<DictionaryReadingExpansion, UnidicArtifactPayloadError> {
        self.reading_paths_with_stats_inner(text, options, true)
    }

    fn reading_paths_with_stats_inner(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
        allow_direct_fallback: bool,
    ) -> Result<DictionaryReadingExpansion, UnidicArtifactPayloadError> {
        if text.is_empty() || options.max_span_chars == 0 || options.max_paths == 0 {
            return Ok(DictionaryReadingExpansion::default());
        }

        let mut stats = DictionaryReadingStats::default();
        let boundaries = char_boundaries(text);
        let char_len = boundaries.len() - 1;
        let mut suffix_paths = vec![Vec::<DictionaryReadingPath>::new(); char_len + 1];
        suffix_paths[char_len].push(DictionaryReadingPath {
            segments: Vec::new(),
            joined_reading: String::new(),
        });

        for start in (0..char_len).rev() {
            let mut paths_by_reading = std::collections::BTreeMap::new();
            let end_limit = char_len.min(start + options.max_span_chars);
            let mut matching_ends = Vec::new();

            for end in start + 1..=end_limit {
                let surface = &text[boundaries[start]..boundaries[end]];
                if self.try_readings(surface)?.is_some() && !suffix_paths[end].is_empty() {
                    matching_ends.push(end);
                }
            }
            stats.matched_spans += matching_ends.len();

            if options.longest_match_only && !allow_direct_fallback {
                if let Some(end) = matching_ends.last().copied() {
                    stats.longest_match_pruned_spans += matching_ends.len().saturating_sub(1);
                    matching_ends.clear();
                    matching_ends.push(end);
                }
            }

            for end in matching_ends {
                let surface = &text[boundaries[start]..boundaries[end]];
                let Some(surface_readings) = self.try_readings(surface)? else {
                    continue;
                };

                stats.raw_segment_readings += surface_readings.len();
                let raw_surface_reading_count = surface_readings.len();
                let surface_readings = limited_surface_readings(surface_readings.as_ref(), options);
                stats.used_segment_readings += surface_readings.len();
                stats.pruned_segment_readings += raw_surface_reading_count - surface_readings.len();
                for surface_reading in surface_readings {
                    for suffix in &suffix_paths[end] {
                        stats.candidate_combinations += 1;
                        let mut reading = String::with_capacity(
                            surface_reading.len() + suffix.joined_reading.len(),
                        );
                        reading.push_str(surface_reading);
                        reading.push_str(&suffix.joined_reading);

                        let mut segments = Vec::with_capacity(suffix.segments.len() + 1);
                        segments.push(DictionaryReadingSegment {
                            surface: surface.to_string(),
                            reading: surface_reading.to_string(),
                        });
                        segments.extend(suffix.segments.iter().cloned());

                        match paths_by_reading.entry(reading.clone()) {
                            Entry::Vacant(entry) => {
                                entry.insert(DictionaryReadingPath {
                                    segments,
                                    joined_reading: reading,
                                });
                                stats.unique_paths += 1;
                            }
                            Entry::Occupied(_) => {
                                stats.duplicate_joined_readings += 1;
                            }
                        }

                        if paths_by_reading.len() >= options.max_paths {
                            stats.max_paths_hit_count += 1;
                            break;
                        }
                    }

                    if paths_by_reading.len() >= options.max_paths {
                        break;
                    }
                }

                if paths_by_reading.len() >= options.max_paths {
                    break;
                }
            }

            if allow_direct_fallback && paths_by_reading.len() < options.max_paths {
                if let Some(end) = direct_fallback_end(text, &boundaries, start, char_len) {
                    if !suffix_paths[end].is_empty() {
                        stats.direct_fallback_spans += 1;
                        let surface = &text[boundaries[start]..boundaries[end]];
                        for suffix in &suffix_paths[end] {
                            stats.candidate_combinations += 1;
                            let mut reading =
                                String::with_capacity(surface.len() + suffix.joined_reading.len());
                            reading.push_str(surface);
                            reading.push_str(&suffix.joined_reading);

                            let mut segments = Vec::with_capacity(suffix.segments.len() + 1);
                            segments.push(DictionaryReadingSegment {
                                surface: surface.to_string(),
                                reading: surface.to_string(),
                            });
                            segments.extend(suffix.segments.iter().cloned());

                            match paths_by_reading.entry(reading.clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(DictionaryReadingPath {
                                        segments,
                                        joined_reading: reading,
                                    });
                                    stats.unique_paths += 1;
                                }
                                Entry::Occupied(_) => {
                                    stats.duplicate_joined_readings += 1;
                                }
                            }

                            if paths_by_reading.len() >= options.max_paths {
                                stats.max_paths_hit_count += 1;
                                break;
                            }
                        }
                    }
                }
            }

            suffix_paths[start] = paths_by_reading.into_values().collect();
        }

        Ok(DictionaryReadingExpansion {
            paths: suffix_paths.remove(0),
            stats,
        })
    }

    fn reading_sequences_with_stats_inner(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
        allow_direct_fallback: bool,
    ) -> Result<DictionaryReadingSequenceExpansion, UnidicArtifactPayloadError> {
        if text.is_empty() || options.max_span_chars == 0 || options.max_paths == 0 {
            return Ok(DictionaryReadingSequenceExpansion::default());
        }

        let mut stats = DictionaryReadingStats::default();
        let boundaries = char_boundaries(text);
        let char_len = boundaries.len() - 1;
        let mut suffix_paths = vec![Vec::<String>::new(); char_len + 1];
        suffix_paths[char_len].push(String::new());

        for start in (0..char_len).rev() {
            let mut paths_by_reading = BTreeSet::new();
            let end_limit = char_len.min(start + options.max_span_chars);
            let mut matching_ends = Vec::new();

            for end in start + 1..=end_limit {
                let surface = &text[boundaries[start]..boundaries[end]];
                if self.try_readings(surface)?.is_some() && !suffix_paths[end].is_empty() {
                    matching_ends.push(end);
                }
            }
            stats.matched_spans += matching_ends.len();

            if options.longest_match_only && !allow_direct_fallback {
                if let Some(end) = matching_ends.last().copied() {
                    stats.longest_match_pruned_spans += matching_ends.len().saturating_sub(1);
                    matching_ends.clear();
                    matching_ends.push(end);
                }
            }

            for end in matching_ends {
                let surface = &text[boundaries[start]..boundaries[end]];
                let Some(surface_readings) = self.try_readings(surface)? else {
                    continue;
                };

                stats.raw_segment_readings += surface_readings.len();
                let raw_surface_reading_count = surface_readings.len();
                let surface_readings = limited_surface_readings(surface_readings.as_ref(), options);
                stats.used_segment_readings += surface_readings.len();
                stats.pruned_segment_readings += raw_surface_reading_count - surface_readings.len();
                for surface_reading in surface_readings {
                    for suffix in &suffix_paths[end] {
                        stats.candidate_combinations += 1;
                        let mut reading =
                            String::with_capacity(surface_reading.len() + suffix.len());
                        reading.push_str(surface_reading);
                        reading.push_str(suffix);

                        if paths_by_reading.insert(reading) {
                            stats.unique_paths += 1;
                        } else {
                            stats.duplicate_joined_readings += 1;
                        }

                        if paths_by_reading.len() >= options.max_paths {
                            stats.max_paths_hit_count += 1;
                            break;
                        }
                    }

                    if paths_by_reading.len() >= options.max_paths {
                        break;
                    }
                }

                if paths_by_reading.len() >= options.max_paths {
                    break;
                }
            }

            if allow_direct_fallback && paths_by_reading.len() < options.max_paths {
                if let Some(end) = direct_fallback_end(text, &boundaries, start, char_len) {
                    if !suffix_paths[end].is_empty() {
                        stats.direct_fallback_spans += 1;
                        let surface = &text[boundaries[start]..boundaries[end]];
                        for suffix in &suffix_paths[end] {
                            stats.candidate_combinations += 1;
                            let mut reading = String::with_capacity(surface.len() + suffix.len());
                            reading.push_str(surface);
                            reading.push_str(suffix);

                            if paths_by_reading.insert(reading) {
                                stats.unique_paths += 1;
                            } else {
                                stats.duplicate_joined_readings += 1;
                            }

                            if paths_by_reading.len() >= options.max_paths {
                                stats.max_paths_hit_count += 1;
                                break;
                            }
                        }
                    }
                }
            }

            suffix_paths[start] = paths_by_reading.into_iter().collect();
        }

        Ok(DictionaryReadingSequenceExpansion {
            paths: suffix_paths.remove(0),
            stats,
        })
    }

    /// Builds a romaji lattice from dictionary-only readings of `text`.
    ///
    /// Returns `Ok(None)` when the dictionary cannot cover the entire input.
    /// Indexed artifact decode errors are reported as
    /// [`JaLatticeError::ArtifactPayload`].
    pub fn romaji_lattice(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> Result<Option<Lattice>, JaLatticeError> {
        let readings = self
            .reading_sequences_with_stats_inner(text, options, false)
            .map_err(|err| JaLatticeError::ArtifactPayload(err.to_string()))?;
        if readings.paths.is_empty() {
            return Ok(None);
        }

        crate::romaji::romaji_lattice_from_readings(readings.paths).map(Some)
    }

    /// Builds a romaji lattice with dictionary readings and direct fallback.
    ///
    /// This is the preferred lattice builder for mixed Japanese text where
    /// kana or ASCII spans may appear beside dictionary-backed surfaces.
    pub fn hybrid_romaji_lattice(
        &self,
        text: &str,
        options: DictionaryReadingOptions,
    ) -> Result<Option<Lattice>, JaLatticeError> {
        let readings = self
            .reading_sequences_with_stats_inner(text, options, true)
            .map_err(|err| JaLatticeError::ArtifactPayload(err.to_string()))?;
        if readings.paths.is_empty() {
            return Ok(None);
        }

        crate::romaji::romaji_lattice_from_readings(readings.paths).map(Some)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DictionaryReadingSequenceExpansion {
    paths: Vec<String>,
    stats: DictionaryReadingStats,
}

fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

pub(crate) fn insert_surface_reading(
    by_surface: &mut HashMap<String, BTreeSet<String>>,
    surface: &str,
    reading: &str,
) {
    by_surface
        .entry(surface.to_string())
        .or_default()
        .insert(reading.to_string());
}

pub(crate) fn normalize_ascii_width(input: &str) -> Option<String> {
    let mut normalized = String::with_capacity(input.len());
    let mut changed = false;

    for ch in input.chars() {
        let normalized_ch = normalize_ascii_width_char(ch);
        changed |= normalized_ch != ch;
        normalized.push(normalized_ch);
    }

    changed.then_some(normalized)
}

fn normalize_ascii_width_char(ch: char) -> char {
    match ch {
        '\u{3000}' => ' ',
        '\u{ff01}'..='\u{ff5e}' => {
            char::from_u32(ch as u32 - 0xfee0).expect("fullwidth ASCII maps to ASCII")
        }
        _ => ch,
    }
}

pub(crate) fn lex_csv_reader(reader: impl Read) -> csv::Reader<impl Read> {
    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(reader)
}

pub(crate) fn field(record: &csv::StringRecord, column: usize) -> Result<&str, UnidicCsvError> {
    record
        .get(column)
        .ok_or_else(|| UnidicCsvError::MissingColumn {
            record_index: record
                .position()
                .map(|position| position.record())
                .unwrap_or(0),
            column,
            len: record.len(),
        })
}

pub(crate) fn is_symbol_pos(pos1: &str) -> bool {
    pos1.contains("記号")
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn limited_surface_readings(readings: &[String], options: DictionaryReadingOptions) -> &[String] {
    if let Some(max_readings) = options.max_readings_per_segment {
        &readings[..readings.len().min(max_readings)]
    } else {
        readings
    }
}

fn direct_fallback_end(
    text: &str,
    boundaries: &[usize],
    start: usize,
    char_len: usize,
) -> Option<usize> {
    let mut end = start;
    while end < char_len {
        let surface = &text[boundaries[start]..boundaries[end + 1]];
        if !can_build_romaji_paths(surface) {
            break;
        }
        end += 1;
    }

    (end > start).then_some(end)
}

fn write_binary_string(
    writer: &mut impl Write,
    field: &'static str,
    value: &str,
) -> Result<(), UnidicArtifactPayloadError> {
    write_u32_len(writer, field, value.len())?;
    writer.write_all(value.as_bytes())?;
    Ok(())
}

fn write_u32_len(
    writer: &mut impl Write,
    field: &'static str,
    len: usize,
) -> Result<(), UnidicArtifactPayloadError> {
    let len = u32::try_from(len)
        .map_err(|_| UnidicArtifactPayloadError::BinaryValueTooLarge { field, len })?;
    writer.write_all(&len.to_le_bytes())?;
    Ok(())
}

fn read_binary_string(
    reader: &mut impl Read,
    field: &'static str,
) -> Result<String, UnidicArtifactPayloadError> {
    let len = read_u32_le(reader, field)? as usize;
    check_limit(field, len, MAX_ARTIFACT_STRING_BYTES)?;
    let mut bytes = vec![0_u8; len];
    read_exact_binary(reader, &mut bytes, field)?;
    String::from_utf8(bytes)
        .map_err(|source| UnidicArtifactPayloadError::InvalidBinaryUtf8 { field, source })
}

fn read_u32_le(
    reader: &mut impl Read,
    field: &'static str,
) -> Result<u32, UnidicArtifactPayloadError> {
    let mut bytes = [0_u8; 4];
    read_exact_binary(reader, &mut bytes, field)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64_le(
    reader: &mut impl Read,
    field: &'static str,
) -> Result<u64, UnidicArtifactPayloadError> {
    let mut bytes = [0_u8; 8];
    read_exact_binary(reader, &mut bytes, field)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_exact_binary(
    reader: &mut impl Read,
    bytes: &mut [u8],
    field: &'static str,
) -> Result<(), UnidicArtifactPayloadError> {
    match reader.read_exact(bytes) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
            Err(UnidicArtifactPayloadError::TruncatedBinary { field })
        }
        Err(err) => Err(UnidicArtifactPayloadError::Io(err)),
    }
}

fn read_binary_artifact_payload_header(
    reader: &mut impl Read,
) -> Result<UnidicBinaryArtifactPayloadHeader, UnidicArtifactPayloadError> {
    let mut magic = [0_u8; 8];
    read_exact_binary(reader, &mut magic, "magic")?;
    if &magic != BINARY_ARTIFACT_MAGIC {
        return Err(UnidicArtifactPayloadError::InvalidBinaryMagic { magic });
    }

    let version = read_u32_le(reader, "version")?;
    if version != BINARY_ARTIFACT_VERSION {
        return Err(UnidicArtifactPayloadError::UnsupportedBinaryVersion { version });
    }

    let reserved = read_u32_le(reader, "reserved")?;
    if reserved != 0 {
        return Err(UnidicArtifactPayloadError::NonZeroBinaryReserved { value: reserved });
    }

    let entry_count = read_u64_le(reader, "entry_count")?;
    let entries = usize::try_from(entry_count).map_err(|_| {
        UnidicArtifactPayloadError::BinaryEntryCountTooLarge {
            entries: entry_count,
        }
    })?;
    check_limit("entry_count", entries, MAX_ARTIFACT_ENTRIES)?;

    Ok(UnidicBinaryArtifactPayloadHeader { version, entries })
}

fn read_indexed_artifact_payload_header_bytes(
    bytes: &[u8],
) -> Result<UnidicIndexedArtifactPayloadHeader, UnidicArtifactPayloadError> {
    if bytes.len() < INDEXED_ARTIFACT_HEADER_LEN {
        return Err(UnidicArtifactPayloadError::TruncatedIndexed { field: "header" });
    }
    let mut magic = [0_u8; 8];
    magic.copy_from_slice(&bytes[..8]);
    if &magic != INDEXED_ARTIFACT_MAGIC {
        return Err(UnidicArtifactPayloadError::InvalidIndexedMagic { magic });
    }

    let version = read_u32_le_bytes(bytes, 8, "version")?;
    if version != INDEXED_ARTIFACT_VERSION {
        return Err(UnidicArtifactPayloadError::UnsupportedIndexedVersion { version });
    }
    let reserved = read_u32_le_bytes(bytes, 12, "reserved")?;
    if reserved != 0 {
        return Err(UnidicArtifactPayloadError::NonZeroIndexedReserved { value: reserved });
    }
    let entry_count = read_u64_le_bytes(bytes, 16, "entry_count")?;
    let fst_len = read_u64_le_bytes(bytes, 24, "fst_len")?;
    let readings_len = read_u64_le_bytes(bytes, 32, "readings_len")?;
    let entries = checked_indexed_usize("entry_count", entry_count)?;
    check_limit("entry_count", entries, MAX_ARTIFACT_ENTRIES)?;
    Ok(UnidicIndexedArtifactPayloadHeader {
        version,
        entries,
        fst_len: checked_indexed_usize("fst_len", fst_len)?,
        readings_len: checked_indexed_usize("readings_len", readings_len)?,
    })
}

fn read_u32_le_bytes(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<u32, UnidicArtifactPayloadError> {
    let end = offset
        .checked_add(4)
        .ok_or(UnidicArtifactPayloadError::TruncatedIndexed { field })?;
    let chunk = bytes
        .get(offset..end)
        .ok_or(UnidicArtifactPayloadError::TruncatedIndexed { field })?;
    Ok(u32::from_le_bytes(
        chunk.try_into().expect("slice length is 4"),
    ))
}

fn read_u64_le_bytes(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<u64, UnidicArtifactPayloadError> {
    let end = offset
        .checked_add(8)
        .ok_or(UnidicArtifactPayloadError::TruncatedIndexed { field })?;
    let chunk = bytes
        .get(offset..end)
        .ok_or(UnidicArtifactPayloadError::TruncatedIndexed { field })?;
    Ok(u64::from_le_bytes(
        chunk.try_into().expect("slice length is 8"),
    ))
}

fn checked_indexed_usize(
    field: &'static str,
    len: u64,
) -> Result<usize, UnidicArtifactPayloadError> {
    usize::try_from(len)
        .map_err(|_| UnidicArtifactPayloadError::IndexedSectionTooLarge { field, len })
}

fn check_payload_file_size(path: &Path) -> Result<(), UnidicArtifactPayloadError> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_ARTIFACT_PAYLOAD_BYTES {
        return Err(UnidicArtifactPayloadError::ArtifactLimitExceeded {
            field: "payload_bytes",
            len,
            max: MAX_ARTIFACT_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

fn check_limit(
    field: &'static str,
    len: usize,
    max: usize,
) -> Result<(), UnidicArtifactPayloadError> {
    if len > max {
        return Err(UnidicArtifactPayloadError::ArtifactLimitExceeded {
            field,
            len: len as u64,
            max: max as u64,
        });
    }
    Ok(())
}

fn write_indexed_reading_block(
    writer: &mut Vec<u8>,
    readings: &[String],
) -> Result<(), UnidicArtifactPayloadError> {
    write_u32_len(writer, "reading_count", readings.len())?;
    for reading in readings {
        write_binary_string(writer, "reading", reading)?;
    }
    Ok(())
}

impl IndexedUnidicPayload {
    fn validate(&self) -> Result<(), UnidicArtifactPayloadError> {
        let mut stream = self.map.stream();
        while let Some((surface, offset)) = stream.next() {
            let surface = std::str::from_utf8(surface).map_err(|source| {
                UnidicArtifactPayloadError::InvalidIndexedUtf8 {
                    field: "surface",
                    source,
                }
            })?;
            if surface.is_empty() {
                return Err(UnidicArtifactPayloadError::EmptySurface { entry_index: 0 });
            }
            let readings = self.readings_at(offset)?;
            if readings.is_empty() {
                return Err(UnidicArtifactPayloadError::EmptyReadings {
                    surface: surface.to_string(),
                });
            }
            let mut seen = BTreeSet::new();
            for (reading_index, reading) in readings.iter().enumerate() {
                if reading.is_empty() {
                    return Err(UnidicArtifactPayloadError::EmptyReading {
                        surface: surface.to_string(),
                        reading_index,
                    });
                }
                if !seen.insert(reading) {
                    return Err(UnidicArtifactPayloadError::DuplicateReading {
                        surface: surface.to_string(),
                        reading: reading.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn readings(&self, surface: &str) -> Result<Option<Vec<String>>, UnidicArtifactPayloadError> {
        self.map
            .get(surface)
            .map(|offset| self.readings_at(offset))
            .transpose()
    }

    fn entries(&self) -> Result<Vec<UnidicReadingIndexPayloadEntry>, UnidicArtifactPayloadError> {
        let mut entries = Vec::with_capacity(self.entries);
        let mut stream = self.map.stream();
        while let Some((surface, offset)) = stream.next() {
            let surface = std::str::from_utf8(surface)
                .map_err(|source| UnidicArtifactPayloadError::InvalidIndexedUtf8 {
                    field: "surface",
                    source,
                })?
                .to_string();
            let readings = self.readings_at(offset)?;
            entries.push(UnidicReadingIndexPayloadEntry { surface, readings });
        }
        Ok(entries)
    }

    fn readings_at(&self, offset: u64) -> Result<Vec<String>, UnidicArtifactPayloadError> {
        read_indexed_readings_at_bytes(&self.mmap, self.readings_start, offset)
    }
}

fn read_indexed_readings_at_bytes(
    bytes: &[u8],
    readings_start: usize,
    offset: u64,
) -> Result<Vec<String>, UnidicArtifactPayloadError> {
    let offset = usize::try_from(offset)
        .map_err(|_| UnidicArtifactPayloadError::InvalidIndexedOffset { offset })?;
    let start = readings_start.checked_add(offset).ok_or(
        UnidicArtifactPayloadError::InvalidIndexedOffset {
            offset: offset as u64,
        },
    )?;
    if start >= bytes.len() {
        return Err(UnidicArtifactPayloadError::InvalidIndexedOffset {
            offset: offset as u64,
        });
    }
    let mut cursor = start;
    let reading_count = read_u32_le_bytes(bytes, cursor, "reading_count")? as usize;
    check_limit(
        "reading_count",
        reading_count,
        MAX_ARTIFACT_READINGS_PER_ENTRY,
    )?;
    cursor += 4;
    let mut readings = Vec::with_capacity(reading_count);
    for _ in 0..reading_count {
        let len = read_u32_le_bytes(bytes, cursor, "reading_len")? as usize;
        check_limit("reading_bytes", len, MAX_ARTIFACT_STRING_BYTES)?;
        cursor += 4;
        let end = cursor
            .checked_add(len)
            .ok_or(UnidicArtifactPayloadError::TruncatedIndexed { field: "reading" })?;
        let reading_bytes = bytes
            .get(cursor..end)
            .ok_or(UnidicArtifactPayloadError::TruncatedIndexed { field: "reading" })?;
        let reading = std::str::from_utf8(reading_bytes)
            .map_err(|source| UnidicArtifactPayloadError::InvalidIndexedUtf8 {
                field: "reading",
                source,
            })?
            .to_string();
        readings.push(reading);
        cursor = end;
    }
    Ok(readings)
}

/// Computes the SHA-256 file digest string for a UniDic artifact payload file.
pub fn artifact_file_digest_path(path: impl AsRef<Path>) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    artifact_file_digest_reader(file)
}

/// Computes the SHA-256 file digest string from a reader.
pub fn artifact_file_digest_reader(mut reader: impl Read) -> Result<String, std::io::Error> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(sha256_digest_hex(hasher.finalize()))
}

fn validate_artifact_payload_header(
    payload: &UnidicReadingIndexPayload,
) -> Result<(), UnidicArtifactPayloadError> {
    if payload.schema_version != ARTIFACT_PAYLOAD_SCHEMA_VERSION {
        return Err(UnidicArtifactPayloadError::UnsupportedSchemaVersion {
            version: payload.schema_version,
        });
    }
    if payload.payload_type != ARTIFACT_PAYLOAD_TYPE {
        return Err(UnidicArtifactPayloadError::UnsupportedPayloadType {
            payload_type: payload.payload_type.clone(),
        });
    }
    Ok(())
}

fn canonical_payload_bytes(payload: &UnidicReadingIndexPayload) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"moine.unidic.reading-index.surface-readings/v1\n");
    for entry in &payload.entries {
        push_len_prefixed(&mut bytes, b"S", &entry.surface);
        bytes.extend_from_slice(format!("R{}\n", entry.readings.len()).as_bytes());
        for reading in &entry.readings {
            push_len_prefixed(&mut bytes, b"r", reading);
        }
    }
    bytes
}

fn push_len_prefixed(bytes: &mut Vec<u8>, tag: &[u8], value: &str) {
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(value.len().to_string().as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(b'\n');
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn sha256_hex(bytes: &[u8]) -> String {
    sha256_digest_hex(Sha256::digest(bytes))
}

fn sha256_digest_hex(digest: impl IntoIterator<Item = u8>) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_surface_to_readings_index() {
        let csv = "\
印刷,18331,19434,9138,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢,*,*,*,*,*,*,体,インサツ,インサツ,インサツ,インサツ,0,C2,*,752349454934528,2737
刃,18521,20041,11551,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和,ハ濁,基本形,*,*,*,*,体,ハ,ハ,ハ,ハ,1,C3,*,8060803244761600,29325
刃,18419,19578,12664,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和,*,*,*,*,*,*,体,ヤイバ,ヤイバ,ヤイバ,ヤイバ,\"1,0\",C1,*,18677687522566656,67949
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();

        assert_eq!(
            index.readings("印刷").as_deref(),
            Some(&["インサツ".to_string()][..])
        );
        assert_eq!(
            index.readings("刃").as_deref(),
            Some(&["ハ".to_string(), "ヤイバ".to_string()][..])
        );
    }

    #[test]
    fn skips_star_readings() {
        let csv = "記号,1,2,3,補助記号,一般,*,*,*,*,*,記号,記号,*,記号,*,記号\n";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();

        assert!(index.is_empty());
    }

    #[test]
    fn excludes_ascii_and_symbol_surfaces_by_default() {
        let csv = "\
a,1,2,3,記号,文字,*,*,*,*,エー,a,a,エー,a,エー,外
!,1,2,3,補助記号,一般,*,*,*,*,!,!,!,!,!,!,記号
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();

        assert_eq!(index.readings("a"), None);
        assert_eq!(index.readings("!"), None);
        assert_eq!(
            index.readings("印刷").as_deref(),
            Some(&["インサツ".to_string()][..])
        );
    }

    #[test]
    fn can_keep_ascii_surfaces_when_requested() {
        let csv = "a,1,2,3,名詞,普通名詞,一般,*,*,*,エー,a,a,エー,a,エー,外\n";
        let index = UnidicReadingIndex::from_lex_csv_reader_with_options(
            csv.as_bytes(),
            UnidicIndexOptions {
                exclude_ascii_surfaces: false,
                ..UnidicIndexOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            index.readings("a").as_deref(),
            Some(&["エー".to_string()][..])
        );
    }

    #[test]
    fn limits_readings_per_surface_when_requested() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ジン,刃,刃,ジン,刃,ジン,漢
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader_with_options(
            csv.as_bytes(),
            UnidicIndexOptions {
                max_readings_per_surface: Some(2),
                ..UnidicIndexOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            index.readings("刃").as_deref(),
            Some(&["ジン".to_string(), "ハ".to_string()][..])
        );
    }

    #[test]
    fn can_limit_readings_per_segment_at_query_time() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ジン,刃,刃,ジン,刃,ジン,漢
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let readings = index.reading_sequences(
            "刃",
            DictionaryReadingOptions {
                max_readings_per_segment: Some(2),
                ..DictionaryReadingOptions::default()
            },
        );

        assert_eq!(readings, vec!["ジン".to_string(), "ハ".to_string()]);
    }

    #[test]
    fn builds_artifact_metadata_from_index_and_options() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ジン,刃,刃,ジン,刃,ジン,漢
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
";
        let index_options = UnidicIndexOptions {
            reading_field: UnidicReadingField::Pron,
            max_readings_per_surface: Some(1),
            exclude_ascii_surfaces: true,
            exclude_symbol_pos: true,
        };
        let index =
            UnidicReadingIndex::from_lex_csv_reader_with_options(csv.as_bytes(), index_options)
                .unwrap();

        let metadata = index.artifact_metadata(UnidicArtifactMetadataOptions {
            artifact_name: "moine-unidic-cwj-202512".to_string(),
            generator: "moine-cli".to_string(),
            payload_file_name: "moine-unidic-cwj-202512.readings.yaml".to_string(),
            payload_format: "yaml.surface-readings.v1".to_string(),
            source_name: "UniDic-CWJ".to_string(),
            source_version: "2025.12".to_string(),
            source_lex_csv: "unidic-cwj-202512_full/lex.csv".to_string(),
            index_options,
            query_defaults: DictionaryReadingOptions {
                longest_match_only: true,
                max_readings_per_segment: Some(16),
                ..DictionaryReadingOptions::default()
            },
            license: UnidicArtifactLicense::default(),
        });

        assert_eq!(metadata.schema_version, 1);
        assert_eq!(metadata.artifact_type, "moine.unidic.reading-index");
        assert_eq!(
            metadata.payload.path,
            "moine-unidic-cwj-202512.readings.yaml"
        );
        assert_eq!(metadata.payload.format, "yaml.surface-readings.v1");
        assert_eq!(
            metadata.payload.checksum_algorithm,
            ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM
        );
        assert_eq!(metadata.payload.checksum.len(), 64);
        assert_eq!(metadata.source.version, "2025.12");
        assert_eq!(metadata.build.reading_field, "pron");
        assert_eq!(metadata.build.entries, 1);
        assert_eq!(metadata.build.max_readings_per_surface, Some(1));
        assert!(metadata.query_defaults.longest_match_only);
        assert_eq!(metadata.query_defaults.max_readings_per_segment, Some(16));
        assert_eq!(metadata.license.selected_license, "BSD-3-Clause");
    }

    #[test]
    fn builds_deterministic_payload_entries() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let payload = index.artifact_payload();

        assert_eq!(payload.schema_version, 1);
        assert_eq!(
            payload.payload_type,
            "moine.unidic.reading-index.surface-readings"
        );
        assert_eq!(
            payload.entries,
            vec![
                UnidicReadingIndexPayloadEntry {
                    surface: "刃".to_string(),
                    readings: vec!["ハ".to_string(), "ヤイバ".to_string()],
                },
                UnidicReadingIndexPayloadEntry {
                    surface: "印刷".to_string(),
                    readings: vec!["インサツ".to_string()],
                },
            ]
        );
    }

    #[test]
    fn payload_checksum_changes_with_payload_content() {
        let first = UnidicReadingIndex::from_lex_csv_reader(
            "刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和\n".as_bytes(),
        )
        .unwrap();
        let second = UnidicReadingIndex::from_lex_csv_reader(
            "刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和\n".as_bytes(),
        )
        .unwrap();

        assert_eq!(first.artifact_payload_checksum().len(), 64);
        assert_eq!(
            first.artifact_payload_checksum(),
            first
                .artifact_payload_checksum_for_algorithm(ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM)
                .unwrap()
        );
        assert_eq!(
            first
                .artifact_payload_checksum_for_algorithm(LEGACY_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM)
                .unwrap()
                .len(),
            16
        );
        assert_ne!(
            first.artifact_payload_checksum(),
            second.artifact_payload_checksum()
        );
    }

    #[test]
    fn loads_artifact_payload_back_into_index() {
        let payload = UnidicReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.unidic.reading-index.surface-readings".to_string(),
            entries: vec![UnidicReadingIndexPayloadEntry {
                surface: "印刷".to_string(),
                readings: vec!["インサツ".to_string()],
            }],
        };

        let index = UnidicReadingIndex::from_artifact_payload(payload).unwrap();

        assert_eq!(index.len(), 1);
        assert_eq!(
            index.readings("印刷").as_deref(),
            Some(&["インサツ".to_string()][..])
        );
    }

    #[test]
    fn loads_artifact_payload_reader() {
        let yaml = "\
schema_version: 1
payload_type: moine.unidic.reading-index.surface-readings
entries:
- surface: 刃
  readings:
  - ハ
  - ヤイバ
";

        let index = UnidicReadingIndex::from_artifact_payload_reader(yaml.as_bytes()).unwrap();

        assert_eq!(
            index.readings("刃").as_deref(),
            Some(&["ハ".to_string(), "ヤイバ".to_string()][..])
        );
    }

    #[test]
    fn binary_artifact_payload_round_trips_to_equivalent_index() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let mut bytes = Vec::new();

        index.write_artifact_binary_payload(&mut bytes).unwrap();
        let loaded = UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice())
            .expect("binary payload should load");
        let header = UnidicReadingIndex::binary_artifact_payload_header_reader(bytes.as_slice())
            .expect("binary payload header should load");

        assert_eq!(
            header,
            UnidicBinaryArtifactPayloadHeader {
                version: 1,
                entries: 2,
            }
        );
        assert_eq!(loaded.artifact_payload(), index.artifact_payload());
        assert_eq!(
            loaded.artifact_payload_checksum(),
            index.artifact_payload_checksum()
        );
    }

    #[test]
    fn indexed_artifact_payload_round_trips_and_supports_lookup() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        index.write_indexed_artifact_payload(&mut bytes).unwrap();

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "moine-indexed-test-{}-{}.moineidx",
            std::process::id(),
            unique
        ));
        std::fs::write(&path, &bytes).unwrap();
        let loaded = UnidicReadingIndex::from_indexed_artifact_payload_path(&path)
            .expect("indexed payload should load");
        let _ = std::fs::remove_file(&path);
        let loaded_from_bytes = UnidicReadingIndex::from_indexed_artifact_payload_bytes(&bytes)
            .expect("indexed payload bytes should load");

        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.readings("刃").as_deref(),
            Some(&["ハ".to_string(), "ヤイバ".to_string()][..])
        );
        assert_eq!(
            loaded_from_bytes.artifact_payload(),
            index.artifact_payload()
        );
        assert_eq!(loaded.artifact_payload(), index.artifact_payload());
        assert_eq!(
            loaded.artifact_payload_checksum(),
            index.artifact_payload_checksum()
        );
        assert_eq!(
            loaded.reading_sequences("印刷", DictionaryReadingOptions::default()),
            vec!["インサツ".to_string()]
        );
    }

    #[test]
    fn binary_artifact_payload_uses_stable_little_endian_layout() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let mut bytes = Vec::new();

        index.write_artifact_binary_payload(&mut bytes).unwrap();

        #[rustfmt::skip]
        let expected = vec![
            b'M', b'O', b'I', b'N', b'E', b'U', b'0', b'1',
            1, 0, 0, 0,
            0, 0, 0, 0,
            2, 0, 0, 0, 0, 0, 0, 0,
            3, 0, 0, 0, 0xe5, 0x88, 0x83,
            2, 0, 0, 0,
            3, 0, 0, 0, 0xe3, 0x83, 0x8f,
            9, 0, 0, 0, 0xe3, 0x83, 0xa4, 0xe3, 0x82, 0xa4, 0xe3, 0x83, 0x90,
            6, 0, 0, 0, 0xe5, 0x8d, 0xb0, 0xe5, 0x88, 0xb7,
            1, 0, 0, 0,
            12, 0, 0, 0, 0xe3, 0x82, 0xa4, 0xe3, 0x83, 0xb3, 0xe3, 0x82, 0xb5, 0xe3, 0x83, 0x84,
        ];
        assert_eq!(bytes, expected);
    }

    #[test]
    fn rejects_binary_artifact_bad_magic() {
        let bytes = *b"NOTMOINE";
        let err =
            UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice()).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::InvalidBinaryMagic { .. }
        ));
    }

    #[test]
    fn rejects_binary_artifact_unsupported_version() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MOINEU01");
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());

        let err =
            UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice()).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::UnsupportedBinaryVersion { version: 2 }
        ));
    }

    #[test]
    fn rejects_binary_artifact_truncated_string() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MOINEU01");
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        bytes.extend_from_slice("刃".as_bytes());

        let err =
            UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice()).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::TruncatedBinary { field: "surface" }
        ));
    }

    #[test]
    fn rejects_binary_artifact_invalid_utf8() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MOINEU01");
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u64.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.push(0xff);
        bytes.extend_from_slice(&0_u32.to_le_bytes());

        let err =
            UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice()).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::InvalidBinaryUtf8 {
                field: "surface",
                ..
            }
        ));
    }

    #[test]
    fn rejects_binary_artifact_excessive_entry_count() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MOINEU01");
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&((MAX_ARTIFACT_ENTRIES as u64) + 1).to_le_bytes());

        let err =
            UnidicReadingIndex::from_binary_artifact_payload_reader(bytes.as_slice()).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::ArtifactLimitExceeded {
                field: "entry_count",
                ..
            }
        ));
    }

    #[test]
    fn rejects_artifact_payload_duplicate_surfaces() {
        let payload = UnidicReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.unidic.reading-index.surface-readings".to_string(),
            entries: vec![
                UnidicReadingIndexPayloadEntry {
                    surface: "刃".to_string(),
                    readings: vec!["ハ".to_string()],
                },
                UnidicReadingIndexPayloadEntry {
                    surface: "刃".to_string(),
                    readings: vec!["ヤイバ".to_string()],
                },
            ],
        };

        let err = UnidicReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::DuplicateSurface { surface } if surface == "刃"
        ));
    }

    #[test]
    fn rejects_artifact_payload_duplicate_readings() {
        let payload = UnidicReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.unidic.reading-index.surface-readings".to_string(),
            entries: vec![UnidicReadingIndexPayloadEntry {
                surface: "刃".to_string(),
                readings: vec!["ハ".to_string(), "ハ".to_string()],
            }],
        };

        let err = UnidicReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::DuplicateReading { surface, reading }
                if surface == "刃" && reading == "ハ"
        ));
    }

    #[test]
    fn rejects_artifact_payload_excessive_reading_count() {
        let payload = UnidicReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.unidic.reading-index.surface-readings".to_string(),
            entries: vec![UnidicReadingIndexPayloadEntry {
                surface: "刃".to_string(),
                readings: vec!["ハ".to_string(); MAX_ARTIFACT_READINGS_PER_ENTRY + 1],
            }],
        };

        let err = UnidicReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::ArtifactLimitExceeded {
                field: "reading_count",
                ..
            }
        ));
    }

    #[test]
    fn rejects_artifact_payload_schema_mismatch() {
        let payload = UnidicReadingIndexPayload {
            schema_version: 2,
            payload_type: "moine.unidic.reading-index.surface-readings".to_string(),
            entries: Vec::new(),
        };

        let err = UnidicReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            UnidicArtifactPayloadError::UnsupportedSchemaVersion { version: 2 }
        ));
    }

    #[test]
    fn reports_reading_expansion_stats() {
        let csv = "\
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ジン,刃,刃,ジン,刃,ジン,漢
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let expansion = index.reading_paths_with_stats(
            "刃",
            DictionaryReadingOptions {
                max_readings_per_segment: Some(2),
                ..DictionaryReadingOptions::default()
            },
        );

        assert_eq!(expansion.paths.len(), 2);
        assert_eq!(
            expansion.stats,
            DictionaryReadingStats {
                matched_spans: 1,
                direct_fallback_spans: 0,
                longest_match_pruned_spans: 0,
                raw_segment_readings: 3,
                used_segment_readings: 2,
                pruned_segment_readings: 1,
                candidate_combinations: 2,
                unique_paths: 2,
                duplicate_joined_readings: 0,
                max_paths_hit_count: 0,
            }
        );
    }

    #[test]
    fn reports_longest_match_and_path_limit_stats() {
        let csv = "\
茶,1,2,3,名詞,普通名詞,一般,*,*,*,チャ,茶,茶,チャ,茶,チャ,和
道,1,2,3,名詞,普通名詞,一般,*,*,*,ミチ,道,道,ミチ,道,ミチ,和
道具,1,2,3,名詞,普通名詞,一般,*,*,*,ドウグ,道具,道具,ドーグ,道具,ドーグ,和
具,1,2,3,名詞,普通名詞,一般,*,*,*,グ,具,具,グ,具,グ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let expansion = index.reading_paths_with_stats(
            "茶道具",
            DictionaryReadingOptions {
                longest_match_only: true,
                max_paths: 1,
                ..DictionaryReadingOptions::default()
            },
        );

        assert_eq!(expansion.paths.len(), 1);
        assert!(expansion.stats.longest_match_pruned_spans > 0);
        assert!(expansion.stats.max_paths_hit_count > 0);
    }

    #[test]
    fn hybrid_reading_paths_use_direct_fallback_for_kana_ascii_spans() {
        let csv = "\
印,1,2,3,名詞,普通名詞,一般,*,*,*,イン,印,印,イン,印,イン,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let expansion =
            index.hybrid_reading_paths_with_stats("印さt", DictionaryReadingOptions::default());

        assert_eq!(
            expansion.paths,
            vec![DictionaryReadingPath {
                joined_reading: "インさt".to_string(),
                segments: vec![
                    DictionaryReadingSegment {
                        surface: "印".to_string(),
                        reading: "イン".to_string(),
                    },
                    DictionaryReadingSegment {
                        surface: "さt".to_string(),
                        reading: "さt".to_string(),
                    },
                ],
            }]
        );
        assert_eq!(expansion.stats.direct_fallback_spans, 2);
    }

    #[test]
    fn hybrid_reading_paths_keep_shorter_dictionary_spans_for_direct_tail() {
        let csv = "\
印,1,2,3,名詞,普通名詞,一般,*,*,*,イン,印,印,イン,印,イン,漢
印さ,1,2,3,動詞,一般,*,*,*,*,シルス,印す,印す,シルス,印す,シルス,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let expansion = index.hybrid_reading_paths_with_stats(
            "印さt",
            DictionaryReadingOptions {
                longest_match_only: true,
                ..DictionaryReadingOptions::default()
            },
        );

        assert!(expansion
            .paths
            .iter()
            .any(|path| path.joined_reading == "インさt"));
        assert_eq!(expansion.stats.longest_match_pruned_spans, 0);
    }

    #[test]
    fn hybrid_reading_paths_still_reject_uncovered_kanji() {
        let index = UnidicReadingIndex::default();
        let expansion =
            index.hybrid_reading_paths_with_stats("未知z", DictionaryReadingOptions::default());

        assert!(expansion.paths.is_empty());
        assert_eq!(expansion.stats.direct_fallback_spans, 1);
    }

    #[test]
    fn can_use_pron_instead_of_lform() {
        let csv = "\
刃,18521,20041,11551,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和,ハ濁,基本形,*,*,*,*,体,ハ,ハ,ハ,ハ,1,C3,*,8060803244761600,29325
刃,18521,20055,14836,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,バ,刃,バ,和,ハ濁,濁音形,*,*,*,*,体,バ,バ,バ,ハ,1,C3,*,8060803244769792,29325
";
        let index = UnidicReadingIndex::from_lex_csv_reader_with_field(
            csv.as_bytes(),
            UnidicReadingField::Pron,
        )
        .unwrap();

        assert_eq!(
            index.readings("刃").as_deref(),
            Some(&["ハ".to_string(), "バ".to_string()][..])
        );
    }

    #[test]
    fn fullwidth_ascii_surfaces_are_indexed_under_halfwidth_aliases() {
        let csv = "\
ＷＨＩＳＫＹ,1,2,3,名詞,普通名詞,一般,*,*,*,ウイスキー,ＷＨＩＳＫＹ,ＷＨＩＳＫＹ,ウイスキー,ＷＨＩＳＫＹ,ウイスキー,外
ＷＨＩＳＫＥＹ,1,2,3,名詞,普通名詞,一般,*,*,*,ウイスキー,ＷＨＩＳＫＥＹ,ＷＨＩＳＫＥＹ,ウイスキー,ＷＨＩＳＫＥＹ,ウイスキー,外
ＭＡＬＴ,1,2,3,名詞,普通名詞,一般,*,*,*,モルト,ＭＡＬＴ,ＭＡＬＴ,モルト,ＭＡＬＴ,モルト,外
abc,1,2,3,名詞,固有名詞,一般,*,*,*,エービーシー,abc,abc,エービーシー,abc,エービーシー,外
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();

        assert_eq!(
            index.readings("ＷＨＩＳＫＹ").as_deref(),
            Some(&["ウイスキー".to_string()][..])
        );
        assert_eq!(
            index.readings("WHISKY").as_deref(),
            Some(&["ウイスキー".to_string()][..])
        );
        assert_eq!(
            index.readings("WHISKEY").as_deref(),
            Some(&["ウイスキー".to_string()][..])
        );
        assert_eq!(
            index.readings("ＷＨＩＳＫＥＹ").as_deref(),
            Some(&["ウイスキー".to_string()][..])
        );
        assert_eq!(
            index.readings("MALT").as_deref(),
            Some(&["モルト".to_string()][..])
        );
        assert_eq!(index.readings("abc"), None);
    }

    #[test]
    fn builds_reading_sequences_from_dictionary_segments() {
        let csv = "\
鬼滅,1,2,3,名詞,普通名詞,一般,*,*,*,キメツ,鬼滅,鬼滅,キメツ,鬼滅,キメツ,固
の,1,2,3,助詞,格助詞,*,*,*,*,ノ,の,の,ノ,の,ノ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ハ,刃,刃,ハ,刃,ハ,和
刃,1,2,3,名詞,普通名詞,一般,*,*,*,ヤイバ,刃,刃,ヤイバ,刃,ヤイバ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let readings = index.reading_sequences("鬼滅の刃", DictionaryReadingOptions::default());

        assert_eq!(
            readings,
            vec!["キメツノハ".to_string(), "キメツノヤイバ".to_string()]
        );
    }

    #[test]
    fn reading_paths_keep_segmentation_and_segment_readings() {
        let csv = "\
茶,1,2,3,名詞,普通名詞,一般,*,*,*,チャ,茶,茶,チャ,茶,チャ,和
道具,1,2,3,名詞,普通名詞,一般,*,*,*,ドウグ,道具,道具,ドーグ,道具,ドーグ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let paths = index.reading_paths(
            "茶道具",
            DictionaryReadingOptions {
                longest_match_only: true,
                ..DictionaryReadingOptions::default()
            },
        );

        assert_eq!(
            paths,
            vec![DictionaryReadingPath {
                joined_reading: "チャドーグ".to_string(),
                segments: vec![
                    DictionaryReadingSegment {
                        surface: "茶".to_string(),
                        reading: "チャ".to_string(),
                    },
                    DictionaryReadingSegment {
                        surface: "道具".to_string(),
                        reading: "ドーグ".to_string(),
                    },
                ],
            }]
        );
    }

    #[test]
    fn builds_romaji_lattice_from_dictionary_segments() {
        let csv = "\
茶,1,2,3,名詞,普通名詞,一般,*,*,*,チャ,茶,茶,チャ,茶,チャ,和
道具,1,2,3,名詞,普通名詞,一般,*,*,*,ドウグ,道具,道具,ドーグ,道具,ドーグ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let lattice = index
            .romaji_lattice("茶道具", DictionaryReadingOptions::default())
            .unwrap()
            .unwrap();

        assert_eq!(
            moine_core::distance(&lattice, &Lattice::from_paths(["chadougu"])),
            0
        );
    }

    #[test]
    fn builds_romaji_lattice_directly_from_reading_paths() {
        let paths = vec![
            DictionaryReadingPath {
                joined_reading: "チャドウグ".to_string(),
                segments: vec![
                    DictionaryReadingSegment {
                        surface: "茶".to_string(),
                        reading: "チャ".to_string(),
                    },
                    DictionaryReadingSegment {
                        surface: "道具".to_string(),
                        reading: "ドウグ".to_string(),
                    },
                ],
            },
            DictionaryReadingPath {
                joined_reading: "チャドーグ".to_string(),
                segments: vec![
                    DictionaryReadingSegment {
                        surface: "茶".to_string(),
                        reading: "チャ".to_string(),
                    },
                    DictionaryReadingSegment {
                        surface: "道具".to_string(),
                        reading: "ドーグ".to_string(),
                    },
                ],
            },
        ];
        let lattice = romaji_lattice_from_reading_paths(&paths).unwrap();

        assert_eq!(
            moine_core::distance(&lattice, &Lattice::from_paths(["chadougu"])),
            0
        );
        assert_eq!(
            moine_core::distance(&lattice, &Lattice::from_paths(["chadoogu"])),
            0
        );
    }

    #[test]
    fn structured_reading_paths_keep_cross_segment_context() {
        let paths = vec![DictionaryReadingPath {
            joined_reading: "マッチャ".to_string(),
            segments: vec![
                DictionaryReadingSegment {
                    surface: "抹".to_string(),
                    reading: "マッ".to_string(),
                },
                DictionaryReadingSegment {
                    surface: "茶".to_string(),
                    reading: "チャ".to_string(),
                },
            ],
        }];
        let lattice = romaji_lattice_from_reading_paths(&paths).unwrap();

        assert_eq!(
            moine_core::distance(&lattice, &Lattice::from_paths(["maccha"])),
            0
        );
        assert_eq!(
            moine_core::distance(&lattice, &Lattice::from_paths(["mattya"])),
            0
        );
    }

    #[test]
    fn can_restrict_reading_sequences_to_longest_matches() {
        let csv = "\
茶,1,2,3,名詞,普通名詞,一般,*,*,*,チャ,茶,茶,チャ,茶,チャ,和
道,1,2,3,名詞,普通名詞,一般,*,*,*,ミチ,道,道,ミチ,道,ミチ,和
道具,1,2,3,名詞,普通名詞,一般,*,*,*,ドウグ,道具,道具,ドーグ,道具,ドーグ,和
具,1,2,3,名詞,普通名詞,一般,*,*,*,グ,具,具,グ,具,グ,和
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let readings = index.reading_sequences(
            "茶道具",
            DictionaryReadingOptions {
                longest_match_only: true,
                ..DictionaryReadingOptions::default()
            },
        );

        assert_eq!(readings, vec!["チャドーグ".to_string()]);
    }
}
