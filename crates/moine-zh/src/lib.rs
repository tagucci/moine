//! Chinese pinyin and CC-CEDICT adapters for `moine`.
//!
//! The current adapter indexes simplified and traditional written Chinese forms
//! with Mandarin pinyin readings from CC-CEDICT. The default public artifact
//! view is no-tone pinyin; `tone3` is an explicit tone-aware artifact view.
//! Cantonese, Jyutping, and non-Mandarin readings are outside this crate's
//! current scope.
//!
//! Dictionary artifacts are external input. Prefer `try_*` lookup and expansion
//! APIs at trust boundaries so indexed-payload decode errors are reported as
//! [`ZhArtifactPayloadError`] instead of being collapsed into empty lookup
//! results for backward-compatible convenience APIs.
//!
//! ```
//! use moine_zh::{
//!     compare_with_zh_index, PinyinReadingOptions, ZhReadingIndex, ZhReadingIndexPayload,
//!     ZhReadingIndexPayloadEntry,
//! };
//!
//! let payload = ZhReadingIndexPayload {
//!     schema_version: 1,
//!     payload_type: "moine.zh.reading-index.surface-readings".to_string(),
//!     pinyin_view: "no-tone".to_string(),
//!     entries: vec![ZhReadingIndexPayloadEntry {
//!         surface: "威士忌".to_string(),
//!         readings: vec!["weishiji".to_string()],
//!     }],
//! };
//! let index = ZhReadingIndex::from_artifact_payload(payload).unwrap();
//!
//! assert_eq!(
//!     compare_with_zh_index("weishiji", "威士忌", &index, PinyinReadingOptions::default())
//!         .unwrap()
//!         .lattice,
//!     0,
//! );
//! ```
//!
#![deny(missing_docs)]

mod pinyin;

use std::borrow::Cow;
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::string::FromUtf8Error;
use std::sync::Arc;

use fst::{Map, MapBuilder, Streamer};
use memmap2::Mmap;
use moine_core::{
    levenshtein_str, normalized_similarity_str, try_damerau_distance, try_damerau_levenshtein_str,
    try_distance, DistanceError, Lattice,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use pinyin::{
    can_build_direct_pinyin_input, direct_pinyin_lattice, normalize_artifact_reading,
    normalize_direct_pinyin_input,
};
pub use pinyin::{normalize_pinyin, pinyin_lattice_from_reading_paths};

const ARTIFACT_PAYLOAD_SCHEMA_VERSION: u32 = 1;
const ARTIFACT_PAYLOAD_TYPE: &str = "moine.zh.reading-index.surface-readings";
const INDEXED_ARTIFACT_MAGIC: &[u8; 8] = b"MOINEZ01";
const INDEXED_ARTIFACT_VERSION: u32 = 1;
const INDEXED_ARTIFACT_HEADER_LEN: usize = 40;
const MAX_ARTIFACT_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARTIFACT_ENTRIES: usize = 2_000_000;
const MAX_ARTIFACT_READINGS_PER_ENTRY: usize = 256;
const MAX_ARTIFACT_STRING_BYTES: usize = 16 * 1024;
/// Current canonical checksum algorithm for normalized Chinese payload content.
pub const ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM: &str = "sha256-canonical-v1";
/// File digest algorithm used to verify payload bytes before loading.
pub const ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM: &str = "sha256-file-v1";

/// Pinyin representation used by a Chinese reading index.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PinyinView {
    /// Pinyin without tone marks or tone numbers.
    #[default]
    NoTone,
    /// Pinyin with tone numbers, such as `zhong1`.
    Tone3,
}

/// Options used while building a CC-CEDICT reading index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CedictIndexOptions {
    /// Pinyin representation to store.
    pub pinyin_view: PinyinView,
    /// Optional cap on readings stored for each surface form.
    pub max_readings_per_surface: Option<usize>,
}

/// Controls Chinese dictionary reading-path expansion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PinyinReadingOptions {
    /// Maximum surface span length considered for one dictionary segment.
    pub max_span_chars: usize,
    /// Maximum complete reading paths to keep.
    pub max_paths: usize,
    /// Prefer the longest dictionary span when multiple spans start together.
    pub longest_match_only: bool,
    /// Optional cap on readings used per dictionary segment.
    pub max_readings_per_segment: Option<usize>,
}

/// One Chinese surface segment and its selected pinyin reading.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinyinReadingSegment {
    /// Surface text covered by the segment.
    pub surface: String,
    /// Pinyin reading selected for the segment.
    pub reading: String,
    /// Whether the segment came from the dictionary or direct pinyin fallback.
    pub source: PinyinReadingSegmentSource,
}

/// Source of one Chinese pinyin reading-path segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PinyinReadingSegmentSource {
    /// Segment was backed by a dictionary entry.
    Dictionary,
    /// Segment was copied from direct pinyin/punctuation fallback.
    Direct,
}

/// One complete segmentation and joined pinyin reading for an input string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinyinReadingPath {
    /// Ordered dictionary/direct segments in the path.
    pub segments: Vec<PinyinReadingSegment>,
    /// Segment readings concatenated into one pinyin string.
    pub joined_reading: String,
}

/// Reading-path expansion result plus pruning statistics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PinyinReadingExpansion {
    /// Expanded pinyin paths.
    pub paths: Vec<PinyinReadingPath>,
    /// Statistics gathered during expansion.
    pub stats: PinyinReadingStats,
}

/// Counters describing Chinese reading-path expansion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PinyinReadingStats {
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
    /// Unique complete pinyin paths retained.
    pub unique_paths: usize,
    /// Duplicate joined readings removed.
    pub duplicate_joined_readings: usize,
    /// Number of times the `max_paths` cap was hit.
    pub max_paths_hit_count: usize,
}

/// Distances computed for one Chinese comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChineseDistance {
    /// Plain Levenshtein distance over the original strings.
    pub surface_levenshtein: usize,
    /// Plain Damerau-Levenshtein distance over the original strings.
    pub surface_damerau: usize,
    /// Lattice Path Edit Distance over pinyin reading lattices.
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

/// Public alias for the Chinese reading index type.
pub type ZhReadingIndex = CedictReadingIndex;

/// CC-CEDICT-derived surface-to-pinyin reading index.
#[derive(Clone, Debug)]
pub struct CedictReadingIndex {
    storage: ZhReadingStorage,
    pinyin_view: PinyinView,
}

#[derive(Clone, Debug)]
enum ZhReadingStorage {
    Eager(HashMap<String, Vec<String>>),
    Indexed(IndexedZhPayload),
}

impl Default for CedictReadingIndex {
    fn default() -> Self {
        Self {
            storage: ZhReadingStorage::Eager(HashMap::new()),
            pinyin_view: PinyinView::default(),
        }
    }
}

impl PartialEq for CedictReadingIndex {
    fn eq(&self, other: &Self) -> bool {
        self.pinyin_view == other.pinyin_view && self.artifact_payload() == other.artifact_payload()
    }
}

impl Eq for CedictReadingIndex {}

#[derive(Clone, Debug)]
struct IndexedZhPayload {
    mmap: Arc<Mmap>,
    map: Map<Vec<u8>>,
    readings_start: usize,
    entries: usize,
}

/// Header for indexed FST Chinese payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZhIndexedArtifactPayloadHeader {
    /// Indexed payload format version.
    pub version: u32,
    /// Pinyin representation stored in the payload.
    pub pinyin_view: PinyinView,
    /// Number of entries in the payload.
    pub entries: usize,
    /// Length of the embedded FST section in bytes.
    pub fst_len: usize,
    /// Length of the reading blob section in bytes.
    pub readings_len: usize,
}

/// Metadata stored in a Chinese dictionary bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Artifact type identifier.
    pub artifact_type: String,
    /// Human-readable artifact name.
    pub artifact_name: String,
    /// Tool or command that generated the artifact.
    pub generator: String,
    /// Payload file metadata.
    pub payload: ZhArtifactPayload,
    /// Source dictionary metadata.
    pub source: ZhArtifactSource,
    /// Build-time options and counts.
    pub build: ZhArtifactBuild,
    /// Default query options for this artifact.
    pub query_defaults: ZhArtifactQueryDefaults,
    /// License metadata and references.
    pub license: ZhArtifactLicense,
}

/// Payload metadata stored in a Chinese dictionary bundle.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactPayload {
    /// Relative payload path inside the bundle.
    pub path: String,
    /// Payload format identifier.
    pub format: String,
    /// Optional digest algorithm for the payload file bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_digest_algorithm: Option<String>,
    /// Optional digest of the payload file bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_digest: Option<String>,
    /// Canonical payload checksum algorithm.
    pub checksum_algorithm: String,
    /// Canonical payload checksum.
    pub checksum: String,
}

/// Source dictionary metadata for a Chinese artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactSource {
    /// Source dictionary name.
    pub name: String,
    /// Source dictionary version or release date.
    pub version: String,
    /// Path or label for the CC-CEDICT source file.
    pub cedict: String,
}

/// Build-time settings recorded in Chinese artifact metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactBuild {
    /// Pinyin representation stored in the artifact.
    pub pinyin_view: String,
    /// Maximum readings retained for each surface form, if capped.
    pub max_readings_per_surface: Option<usize>,
    /// Number of surface entries in the payload.
    pub entries: usize,
}

/// Default reading expansion options recorded in Chinese artifact metadata.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactQueryDefaults {
    /// Maximum surface span length considered for one dictionary segment.
    pub max_span_chars: usize,
    /// Maximum complete reading paths retained.
    pub max_paths: usize,
    /// Whether longest-match mode is enabled by default.
    pub longest_match_only: bool,
    /// Optional default cap on readings used per dictionary segment.
    pub max_readings_per_segment: Option<usize>,
}

/// License metadata for a Chinese dictionary artifact.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactLicense {
    /// Selected license expression for the distributed artifact.
    pub selected_license: String,
    /// License files or notices bundled with the artifact.
    pub references: Vec<ZhArtifactLicenseReference>,
}

/// One license reference stored in Chinese artifact metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhArtifactLicenseReference {
    /// Human-readable license reference label.
    pub label: String,
    /// Relative path to the bundled license or notice file.
    pub path: String,
}

/// Normalized Chinese reading-index payload.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhReadingIndexPayload {
    /// Payload schema version.
    pub schema_version: u32,
    /// Payload type identifier.
    pub payload_type: String,
    /// Pinyin representation used by all readings in the payload.
    pub pinyin_view: String,
    /// Surface-to-reading entries.
    pub entries: Vec<ZhReadingIndexPayloadEntry>,
}

/// One surface form and its normalized pinyin readings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ZhReadingIndexPayloadEntry {
    /// Simplified or traditional surface form.
    pub surface: String,
    /// Normalized pinyin readings for the surface form.
    pub readings: Vec<String>,
}

/// Inputs used to build Chinese artifact metadata from an index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZhArtifactMetadataOptions {
    /// Human-readable artifact name.
    pub artifact_name: String,
    /// Tool or command that generated the artifact.
    pub generator: String,
    /// Payload file name recorded in metadata.
    pub payload_file_name: String,
    /// Payload format recorded in metadata.
    pub payload_format: String,
    /// Source dictionary name.
    pub source_name: String,
    /// Source dictionary version or release date.
    pub source_version: String,
    /// Path or label for the CC-CEDICT source file.
    pub source_cedict: String,
    /// Index build options to record.
    pub index_options: CedictIndexOptions,
    /// Default query options to record.
    pub query_defaults: PinyinReadingOptions,
    /// License metadata to record.
    pub license: ZhArtifactLicense,
}

/// Errors returned while parsing CC-CEDICT source text.
#[derive(Debug)]
pub enum CedictError {
    /// Filesystem or reader access failed.
    Io(std::io::Error),
    /// A non-comment CC-CEDICT line could not be parsed.
    InvalidEntry {
        /// One-based input line number.
        line: usize,
        /// Parse failure detail.
        message: String,
    },
}

/// Errors returned while loading or validating Chinese artifact payloads.
#[derive(Debug)]
pub enum ZhArtifactPayloadError {
    /// Filesystem or reader access failed.
    Io(std::io::Error),
    /// YAML payload deserialization failed.
    Yaml(serde_yaml::Error),
    /// Indexed payload magic bytes do not match the Chinese artifact format.
    InvalidIndexedMagic {
        /// Magic bytes read from the payload.
        magic: [u8; 8],
    },
    /// Indexed payload version is not supported.
    UnsupportedIndexedVersion {
        /// Version read from the payload header.
        version: u32,
    },
    /// Indexed payload pinyin-view tag is not supported.
    UnsupportedIndexedPinyinView {
        /// Numeric pinyin-view tag from the payload header.
        value: u32,
    },
    /// An indexed payload section length cannot fit in memory on this target.
    IndexedSectionTooLarge {
        /// Section field name.
        field: &'static str,
        /// Section length from the payload.
        len: u64,
    },
    /// A configured artifact safety limit was exceeded.
    ArtifactLimitExceeded {
        /// Limited field name.
        field: &'static str,
        /// Observed length or count.
        len: u64,
        /// Maximum accepted length or count.
        max: u64,
    },
    /// Reserved indexed-payload header bytes were non-zero.
    NonZeroIndexedReserved {
        /// Reserved header value.
        value: u32,
    },
    /// The indexed payload ended before a required section was complete.
    TruncatedIndexed {
        /// Section or field being read.
        field: &'static str,
    },
    /// The embedded FST section is invalid.
    InvalidIndexedFst {
        /// FST validation failure detail.
        message: String,
    },
    /// Header entry count and FST entry count disagree.
    IndexedEntryCountMismatch {
        /// Entry count recorded in the header.
        header_entries: usize,
        /// Entry count observed in the FST.
        fst_entries: usize,
    },
    /// A reading block offset points outside the reading section.
    InvalidIndexedOffset {
        /// Invalid offset value.
        offset: u64,
    },
    /// Indexed payload bytes were not valid UTF-8.
    InvalidIndexedUtf8 {
        /// Field being decoded.
        field: &'static str,
        /// Underlying UTF-8 error.
        source: FromUtf8Error,
    },
    /// YAML payload schema version is not supported.
    UnsupportedSchemaVersion {
        /// Schema version read from the payload.
        version: u32,
    },
    /// YAML payload type does not identify a Chinese reading index.
    UnsupportedPayloadType {
        /// Payload type read from YAML.
        payload_type: String,
    },
    /// Payload pinyin view is not supported.
    UnsupportedPinyinView {
        /// Pinyin-view string read from the payload.
        pinyin_view: String,
    },
    /// A payload entry has an empty surface form.
    EmptySurface {
        /// Zero-based entry index.
        entry_index: usize,
    },
    /// The payload contains a duplicate surface form.
    DuplicateSurface {
        /// Duplicate surface form.
        surface: String,
    },
    /// A surface form has no readings.
    EmptyReadings {
        /// Surface form with no readings.
        surface: String,
    },
    /// A surface form has an empty reading.
    EmptyReading {
        /// Surface form containing the empty reading.
        surface: String,
        /// Zero-based reading index.
        reading_index: usize,
    },
    /// A surface form has a duplicate reading.
    DuplicateReading {
        /// Surface form containing the duplicate.
        surface: String,
        /// Duplicate reading.
        reading: String,
    },
    /// A reading was not normalized for the artifact pinyin view.
    ReadingNotNormalized {
        /// Surface form containing the invalid reading.
        surface: String,
        /// Reading as stored in the payload.
        reading: String,
        /// Expected normalized reading.
        normalized: String,
    },
}

/// Errors returned while building Chinese pinyin lattices.
#[derive(Debug, Eq, PartialEq)]
pub enum CnLatticeError {
    /// No pinyin readings were provided.
    EmptyReadings,
    /// Input cannot be interpreted as direct pinyin and has no dictionary path.
    UnsupportedDirectInput {
        /// Unsupported input surface.
        surface: String,
    },
    /// Artifact loading or indexed payload decoding failed.
    ArtifactPayload(String),
    /// Distance computation exceeded the configured matrix-size limit.
    Distance(DistanceError),
}

impl PinyinView {
    /// Returns the stable artifact string for this pinyin view.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoTone => "no-tone",
            Self::Tone3 => "tone3",
        }
    }
}

impl TryFrom<&str> for PinyinView {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "no-tone" | "notone" | "normal" => Ok(Self::NoTone),
            "tone3" => Ok(Self::Tone3),
            _ => Err(()),
        }
    }
}

impl Default for CedictIndexOptions {
    fn default() -> Self {
        Self {
            pinyin_view: PinyinView::NoTone,
            max_readings_per_surface: None,
        }
    }
}

impl Default for PinyinReadingOptions {
    fn default() -> Self {
        Self {
            max_span_chars: 8,
            max_paths: 1024,
            longest_match_only: false,
            max_readings_per_segment: None,
        }
    }
}

impl Default for ZhArtifactLicense {
    fn default() -> Self {
        Self {
            selected_license: "CC BY-SA 4.0".to_string(),
            references: vec![ZhArtifactLicenseReference {
                label: "CC-CEDICT".to_string(),
                path: "license/CC-CEDICT.md".to_string(),
            }],
        }
    }
}

impl fmt::Display for CedictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read CC-CEDICT: {err}"),
            Self::InvalidEntry { line, message } => {
                write!(f, "invalid CC-CEDICT entry at line {line}: {message}")
            }
        }
    }
}

impl Error for CedictError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::InvalidEntry { .. } => None,
        }
    }
}

impl fmt::Display for ZhArtifactPayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "failed to read zh artifact payload: {err}"),
            Self::Yaml(err) => write!(f, "invalid zh artifact payload YAML: {err}"),
            Self::InvalidIndexedMagic { magic } => {
                write!(f, "invalid zh indexed artifact magic {magic:?}")
            }
            Self::UnsupportedIndexedVersion { version } => {
                write!(f, "unsupported zh indexed artifact version {version}")
            }
            Self::UnsupportedIndexedPinyinView { value } => {
                write!(f, "unsupported zh indexed artifact pinyin view {value}")
            }
            Self::IndexedSectionTooLarge { field, len } => {
                write!(f, "zh indexed artifact {field} length {len} exceeds usize::MAX")
            }
            Self::ArtifactLimitExceeded { field, len, max } => {
                write!(f, "zh artifact {field} length/count {len} exceeds limit {max}")
            }
            Self::NonZeroIndexedReserved { value } => {
                write!(f, "zh indexed artifact reserved header field is {value}")
            }
            Self::TruncatedIndexed { field } => {
                write!(f, "truncated zh indexed artifact while reading {field}")
            }
            Self::InvalidIndexedFst { message } => {
                write!(f, "invalid zh indexed artifact FST: {message}")
            }
            Self::IndexedEntryCountMismatch {
                header_entries,
                fst_entries,
            } => write!(
                f,
                "zh indexed artifact header entry count {header_entries} does not match FST entry count {fst_entries}"
            ),
            Self::InvalidIndexedOffset { offset } => {
                write!(f, "invalid zh indexed artifact readings offset {offset}")
            }
            Self::InvalidIndexedUtf8 { field, source } => {
                write!(f, "invalid UTF-8 in zh indexed artifact {field}: {source}")
            }
            Self::UnsupportedSchemaVersion { version } => {
                write!(f, "unsupported zh artifact payload schema version {version}")
            }
            Self::UnsupportedPayloadType { payload_type } => {
                write!(f, "unsupported zh artifact payload type {payload_type:?}")
            }
            Self::UnsupportedPinyinView { pinyin_view } => {
                write!(f, "unsupported zh artifact pinyin view {pinyin_view:?}")
            }
            Self::EmptySurface { entry_index } => {
                write!(f, "zh artifact payload entry {entry_index} has an empty surface")
            }
            Self::DuplicateSurface { surface } => {
                write!(f, "zh artifact payload has duplicate surface {surface:?}")
            }
            Self::EmptyReadings { surface } => {
                write!(f, "zh artifact payload surface {surface:?} has no readings")
            }
            Self::EmptyReading {
                surface,
                reading_index,
            } => write!(
                f,
                "zh artifact payload surface {surface:?} has an empty reading at index {reading_index}"
            ),
            Self::DuplicateReading { surface, reading } => write!(
                f,
                "zh artifact payload surface {surface:?} has duplicate reading {reading:?}"
            ),
            Self::ReadingNotNormalized {
                surface,
                reading,
                normalized,
            } => write!(
                f,
                "zh artifact payload surface {surface:?} has non-normalized reading {reading:?}; expected {normalized:?}"
            ),
        }
    }
}

impl Error for ZhArtifactPayloadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Yaml(err) => Some(err),
            Self::InvalidIndexedUtf8 { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CedictError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<std::io::Error> for ZhArtifactPayloadError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_yaml::Error> for ZhArtifactPayloadError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::Yaml(err)
    }
}

impl fmt::Display for CnLatticeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyReadings => write!(f, "at least one pinyin reading is required"),
            Self::UnsupportedDirectInput { surface } => {
                write!(f, "unsupported direct pinyin input {surface:?}")
            }
            Self::ArtifactPayload(err) => write!(f, "{err}"),
            Self::Distance(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CnLatticeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Distance(err) => Some(err),
            Self::EmptyReadings
            | Self::UnsupportedDirectInput { .. }
            | Self::ArtifactPayload(_) => None,
        }
    }
}

impl From<DistanceError> for CnLatticeError {
    fn from(value: DistanceError) -> Self {
        Self::Distance(value)
    }
}

impl CedictReadingIndex {
    /// Builds an index from a CC-CEDICT text file.
    pub fn from_cedict_path(path: impl AsRef<Path>) -> Result<Self, CedictError> {
        Self::from_cedict_path_with_options(path, CedictIndexOptions::default())
    }

    /// Builds an index from a CC-CEDICT text file with custom options.
    pub fn from_cedict_path_with_options(
        path: impl AsRef<Path>,
        options: CedictIndexOptions,
    ) -> Result<Self, CedictError> {
        let file = File::open(path)?;
        Self::from_cedict_reader_with_options(file, options)
    }

    /// Builds an index from a CC-CEDICT reader.
    pub fn from_cedict_reader(reader: impl Read) -> Result<Self, CedictError> {
        Self::from_cedict_reader_with_options(reader, CedictIndexOptions::default())
    }

    /// Builds an index from a CC-CEDICT reader with custom options.
    pub fn from_cedict_reader_with_options(
        reader: impl Read,
        options: CedictIndexOptions,
    ) -> Result<Self, CedictError> {
        let mut by_surface = HashMap::<String, BTreeSet<String>>::new();
        let reader = BufReader::new(reader);
        for (line_index, line) in reader.lines().enumerate() {
            let line_number = line_index + 1;
            let line = line?;
            let line = line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let entry = parse_cedict_entry(line, line_number)?;
            let reading = normalize_pinyin(entry.pinyin, options.pinyin_view);
            if reading.is_empty() {
                continue;
            }

            by_surface
                .entry(entry.traditional.to_string())
                .or_default()
                .insert(reading.clone());
            by_surface
                .entry(entry.simplified.to_string())
                .or_default()
                .insert(reading);
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

        Ok(Self {
            storage: ZhReadingStorage::Eager(readings_by_surface),
            pinyin_view: options.pinyin_view,
        })
    }

    /// Loads a YAML artifact payload from a file path.
    pub fn from_artifact_payload_path(
        path: impl AsRef<Path>,
    ) -> Result<Self, ZhArtifactPayloadError> {
        let path = path.as_ref();
        check_payload_file_size(path)?;
        let file = File::open(path)?;
        Self::from_artifact_payload_reader(file)
    }

    /// Loads a YAML artifact payload from a reader.
    pub fn from_artifact_payload_reader(reader: impl Read) -> Result<Self, ZhArtifactPayloadError> {
        let payload = serde_yaml::from_reader(reader)?;
        Self::from_artifact_payload(payload)
    }

    /// Builds an index from a deserialized artifact payload.
    pub fn from_artifact_payload(
        payload: ZhReadingIndexPayload,
    ) -> Result<Self, ZhArtifactPayloadError> {
        validate_artifact_payload_header(&payload)?;
        let pinyin_view = PinyinView::try_from(payload.pinyin_view.as_str()).map_err(|()| {
            ZhArtifactPayloadError::UnsupportedPinyinView {
                pinyin_view: payload.pinyin_view.clone(),
            }
        })?;
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
                return Err(ZhArtifactPayloadError::EmptySurface { entry_index });
            }
            if entry.readings.is_empty() {
                return Err(ZhArtifactPayloadError::EmptyReadings {
                    surface: entry.surface,
                });
            }

            let mut seen_readings = BTreeSet::new();
            for (reading_index, reading) in entry.readings.iter().enumerate() {
                check_limit("reading_bytes", reading.len(), MAX_ARTIFACT_STRING_BYTES)?;
                if reading.is_empty() {
                    return Err(ZhArtifactPayloadError::EmptyReading {
                        surface: entry.surface,
                        reading_index,
                    });
                }
                let normalized = normalize_artifact_reading(reading, pinyin_view);
                if normalized != *reading {
                    return Err(ZhArtifactPayloadError::ReadingNotNormalized {
                        surface: entry.surface,
                        reading: reading.clone(),
                        normalized,
                    });
                }
                if !seen_readings.insert(reading) {
                    return Err(ZhArtifactPayloadError::DuplicateReading {
                        surface: entry.surface,
                        reading: reading.clone(),
                    });
                }
            }

            if readings_by_surface
                .insert(entry.surface.clone(), entry.readings)
                .is_some()
            {
                return Err(ZhArtifactPayloadError::DuplicateSurface {
                    surface: entry.surface,
                });
            }
        }

        Ok(Self {
            storage: ZhReadingStorage::Eager(readings_by_surface),
            pinyin_view,
        })
    }

    /// Loads an indexed artifact payload from a file path using mmap-backed
    /// storage.
    ///
    /// The file is validated before the index is returned, but readings remain
    /// lazy and are decoded from the indexed payload during lookup.
    pub fn from_indexed_artifact_payload_path(
        path: impl AsRef<Path>,
    ) -> Result<Self, ZhArtifactPayloadError> {
        let path = path.as_ref();
        check_payload_file_size(path)?;
        let file = File::open(path)?;
        // SAFETY: the mmap is kept alive by IndexedZhPayload for as long as
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
    ) -> Result<Self, ZhArtifactPayloadError> {
        if bytes.len() as u64 > MAX_ARTIFACT_PAYLOAD_BYTES {
            return Err(ZhArtifactPayloadError::ArtifactLimitExceeded {
                field: "payload_bytes",
                len: bytes.len() as u64,
                max: MAX_ARTIFACT_PAYLOAD_BYTES,
            });
        }
        let header = read_indexed_artifact_payload_header_bytes(bytes)?;
        let fst_start = INDEXED_ARTIFACT_HEADER_LEN;
        let fst_end = fst_start.checked_add(header.fst_len).ok_or(
            ZhArtifactPayloadError::TruncatedIndexed {
                field: "fst_section",
            },
        )?;
        let readings_end = fst_end.checked_add(header.readings_len).ok_or(
            ZhArtifactPayloadError::TruncatedIndexed {
                field: "readings_section",
            },
        )?;
        if bytes.len() < readings_end {
            return Err(ZhArtifactPayloadError::TruncatedIndexed {
                field: "indexed_payload",
            });
        }

        let map = Map::new(bytes[fst_start..fst_end].to_vec()).map_err(|err| {
            ZhArtifactPayloadError::InvalidIndexedFst {
                message: err.to_string(),
            }
        })?;
        let fst_entries = map.len();
        if fst_entries != header.entries {
            return Err(ZhArtifactPayloadError::IndexedEntryCountMismatch {
                header_entries: header.entries,
                fst_entries,
            });
        }

        let mut entries = Vec::with_capacity(header.entries);
        let mut stream = map.stream();
        while let Some((surface, offset)) = stream.next() {
            let surface = String::from_utf8(surface.to_vec()).map_err(|source| {
                ZhArtifactPayloadError::InvalidIndexedUtf8 {
                    field: "surface",
                    source,
                }
            })?;
            let readings = read_indexed_readings_at_bytes(bytes, fst_end, offset)?;
            entries.push(ZhReadingIndexPayloadEntry { surface, readings });
        }

        Self::from_artifact_payload(ZhReadingIndexPayload {
            schema_version: ARTIFACT_PAYLOAD_SCHEMA_VERSION,
            payload_type: ARTIFACT_PAYLOAD_TYPE.to_string(),
            pinyin_view: header.pinyin_view.as_str().to_string(),
            entries,
        })
    }

    fn from_indexed_mmap(mmap: Mmap) -> Result<Self, ZhArtifactPayloadError> {
        if mmap.len() as u64 > MAX_ARTIFACT_PAYLOAD_BYTES {
            return Err(ZhArtifactPayloadError::ArtifactLimitExceeded {
                field: "payload_bytes",
                len: mmap.len() as u64,
                max: MAX_ARTIFACT_PAYLOAD_BYTES,
            });
        }
        let header = read_indexed_artifact_payload_header_bytes(&mmap)?;
        let fst_start = INDEXED_ARTIFACT_HEADER_LEN;
        let fst_end = fst_start.checked_add(header.fst_len).ok_or(
            ZhArtifactPayloadError::TruncatedIndexed {
                field: "fst_section",
            },
        )?;
        let readings_end = fst_end.checked_add(header.readings_len).ok_or(
            ZhArtifactPayloadError::TruncatedIndexed {
                field: "readings_section",
            },
        )?;
        if mmap.len() < readings_end {
            return Err(ZhArtifactPayloadError::TruncatedIndexed {
                field: "indexed_payload",
            });
        }

        let map = Map::new(mmap[fst_start..fst_end].to_vec()).map_err(|err| {
            ZhArtifactPayloadError::InvalidIndexedFst {
                message: err.to_string(),
            }
        })?;
        let fst_entries = map.len();
        if fst_entries != header.entries {
            return Err(ZhArtifactPayloadError::IndexedEntryCountMismatch {
                header_entries: header.entries,
                fst_entries,
            });
        }
        let indexed = IndexedZhPayload {
            mmap: Arc::new(mmap),
            map,
            readings_start: fst_end,
            entries: header.entries,
        };
        indexed.validate(header.pinyin_view)?;
        Ok(Self {
            storage: ZhReadingStorage::Indexed(indexed),
            pinyin_view: header.pinyin_view,
        })
    }

    /// Returns the pinyin representation stored by this index.
    pub fn pinyin_view(&self) -> PinyinView {
        self.pinyin_view
    }

    /// Returns pinyin readings for `surface`, if present.
    ///
    /// For indexed artifacts, decode errors are treated the same as a missing
    /// surface for backward compatibility. Use [`Self::try_readings`] at trust
    /// boundaries when artifact corruption must be reported distinctly.
    pub fn readings(&self, surface: &str) -> Option<Cow<'_, [String]>> {
        self.try_readings(surface).ok().flatten()
    }

    /// Returns pinyin readings for `surface` and preserves indexed artifact
    /// decode errors.
    pub fn try_readings(
        &self,
        surface: &str,
    ) -> Result<Option<Cow<'_, [String]>>, ZhArtifactPayloadError> {
        match &self.storage {
            ZhReadingStorage::Eager(readings_by_surface) => Ok(readings_by_surface
                .get(surface)
                .map(|readings| Cow::Borrowed(readings.as_slice()))),
            ZhReadingStorage::Indexed(indexed) => indexed
                .readings(surface)
                .map(|readings| readings.map(Cow::Owned)),
        }
    }

    /// Returns the number of indexed Chinese surface forms.
    pub fn len(&self) -> usize {
        match &self.storage {
            ZhReadingStorage::Eager(readings_by_surface) => readings_by_surface.len(),
            ZhReadingStorage::Indexed(indexed) => indexed.entries,
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
    pub fn artifact_metadata(&self, options: ZhArtifactMetadataOptions) -> ZhArtifactMetadata {
        ZhArtifactMetadata {
            schema_version: 1,
            artifact_type: "moine.zh.reading-index".to_string(),
            artifact_name: options.artifact_name,
            generator: options.generator,
            payload: ZhArtifactPayload {
                path: options.payload_file_name,
                format: options.payload_format,
                file_digest_algorithm: None,
                file_digest: None,
                checksum_algorithm: ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM.to_string(),
                checksum: self.artifact_payload_checksum(),
            },
            source: ZhArtifactSource {
                name: options.source_name,
                version: options.source_version,
                cedict: options.source_cedict,
            },
            build: ZhArtifactBuild {
                pinyin_view: options.index_options.pinyin_view.as_str().to_string(),
                max_readings_per_surface: options.index_options.max_readings_per_surface,
                entries: self.len(),
            },
            query_defaults: ZhArtifactQueryDefaults {
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
    pub fn artifact_payload(&self) -> ZhReadingIndexPayload {
        let entries = match &self.storage {
            ZhReadingStorage::Eager(readings_by_surface) => {
                let mut entries = readings_by_surface
                    .iter()
                    .map(|(surface, readings)| ZhReadingIndexPayloadEntry {
                        surface: surface.clone(),
                        readings: readings.clone(),
                    })
                    .collect::<Vec<_>>();
                entries.sort_by(|left, right| left.surface.cmp(&right.surface));
                entries
            }
            ZhReadingStorage::Indexed(indexed) => indexed
                .entries()
                .expect("validated indexed artifact should decode"),
        };

        ZhReadingIndexPayload {
            schema_version: ARTIFACT_PAYLOAD_SCHEMA_VERSION,
            payload_type: ARTIFACT_PAYLOAD_TYPE.to_string(),
            pinyin_view: self.pinyin_view.as_str().to_string(),
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
    /// Unknown algorithms return `None`.
    pub fn artifact_payload_checksum_for_algorithm(&self, algorithm: &str) -> Option<String> {
        let payload = self.artifact_payload();
        let bytes = canonical_payload_bytes(&payload);
        match algorithm {
            ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM => Some(sha256_hex(&bytes)),
            _ => None,
        }
    }

    /// Writes the indexed FST-backed artifact payload format.
    ///
    /// The payload stores a finite-state transducer from surface form to an
    /// offset in a compact reading blob and can be loaded with
    /// [`Self::from_indexed_artifact_payload_path`].
    pub fn write_indexed_artifact_payload(
        &self,
        mut writer: impl Write,
    ) -> Result<(), ZhArtifactPayloadError> {
        let payload = self.artifact_payload();
        let mut fst_bytes = Vec::new();
        let mut readings_bytes = Vec::new();
        {
            let mut builder = MapBuilder::new(&mut fst_bytes).map_err(|err| {
                ZhArtifactPayloadError::InvalidIndexedFst {
                    message: err.to_string(),
                }
            })?;
            for entry in &payload.entries {
                let offset = readings_bytes.len() as u64;
                builder.insert(&entry.surface, offset).map_err(|err| {
                    ZhArtifactPayloadError::InvalidIndexedFst {
                        message: err.to_string(),
                    }
                })?;
                write_indexed_reading_block(&mut readings_bytes, &entry.readings)?;
            }
            builder
                .finish()
                .map_err(|err| ZhArtifactPayloadError::InvalidIndexedFst {
                    message: err.to_string(),
                })?;
        }

        writer.write_all(INDEXED_ARTIFACT_MAGIC)?;
        writer.write_all(&INDEXED_ARTIFACT_VERSION.to_le_bytes())?;
        writer.write_all(&pinyin_view_header_value(self.pinyin_view).to_le_bytes())?;
        writer.write_all(&(payload.entries.len() as u64).to_le_bytes())?;
        writer.write_all(&(fst_bytes.len() as u64).to_le_bytes())?;
        writer.write_all(&(readings_bytes.len() as u64).to_le_bytes())?;
        writer.write_all(&fst_bytes)?;
        writer.write_all(&readings_bytes)?;
        Ok(())
    }

    /// Expands `text` into joined pinyin reading strings.
    ///
    /// This is a compatibility helper over [`Self::reading_paths`]. It drops
    /// segment boundaries and treats indexed artifact decode errors as an empty
    /// expansion.
    pub fn reading_sequences(&self, text: &str, options: PinyinReadingOptions) -> Vec<String> {
        self.reading_paths(text, options)
            .into_iter()
            .map(|path| path.joined_reading)
            .collect()
    }

    /// Expands `text` into dictionary-only pinyin reading paths.
    ///
    /// Every returned path contains surface/reading segment boundaries plus the
    /// joined pinyin reading. Use [`Self::try_reading_paths_with_stats`] when
    /// indexed artifact corruption must be reported.
    pub fn reading_paths(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> Vec<PinyinReadingPath> {
        self.reading_paths_with_stats(text, options).paths
    }

    /// Expands dictionary pinyin paths and treats artifact decode errors as an
    /// empty expansion for backward compatibility.
    ///
    /// Use [`Self::try_reading_paths_with_stats`] when loading indexed
    /// artifacts from outside the process trust boundary.
    pub fn reading_paths_with_stats(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> PinyinReadingExpansion {
        self.try_reading_paths_with_stats(text, options)
            .unwrap_or_default()
    }

    /// Expands dictionary pinyin paths and preserves indexed artifact decode
    /// errors.
    pub fn try_reading_paths_with_stats(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> Result<PinyinReadingExpansion, ZhArtifactPayloadError> {
        self.reading_paths_with_stats_inner(text, options, false)
    }

    /// Expands `text` into pinyin reading paths with direct fallback segments.
    ///
    /// Dictionary matches are preferred, but direct pinyin and punctuation
    /// spans can pass through directly so mixed dictionary/direct input can
    /// still form a full path.
    pub fn hybrid_reading_paths(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> Vec<PinyinReadingPath> {
        self.hybrid_reading_paths_with_stats(text, options).paths
    }

    /// Expands hybrid dictionary/direct pinyin paths and treats artifact decode
    /// errors as an empty expansion for backward compatibility.
    ///
    /// Use [`Self::try_hybrid_reading_paths_with_stats`] when loading indexed
    /// artifacts from outside the process trust boundary.
    pub fn hybrid_reading_paths_with_stats(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> PinyinReadingExpansion {
        self.try_hybrid_reading_paths_with_stats(text, options)
            .unwrap_or_default()
    }

    /// Expands hybrid dictionary/direct pinyin paths and preserves indexed
    /// artifact decode errors.
    pub fn try_hybrid_reading_paths_with_stats(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> Result<PinyinReadingExpansion, ZhArtifactPayloadError> {
        self.reading_paths_with_stats_inner(text, options, true)
    }

    /// Builds a pinyin lattice from dictionary-only readings of `text`.
    ///
    /// Returns `Ok(None)` when the dictionary cannot cover the entire input.
    /// Indexed artifact decode errors are reported as
    /// [`CnLatticeError::ArtifactPayload`].
    pub fn pinyin_lattice(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> Result<Option<Lattice>, CnLatticeError> {
        let paths = self
            .try_reading_paths_with_stats(text, options)
            .map_err(|err| CnLatticeError::ArtifactPayload(err.to_string()))?
            .paths;
        if paths.is_empty() {
            return Ok(None);
        }
        pinyin_lattice_from_reading_paths(&paths).map(Some)
    }

    /// Builds a pinyin lattice with dictionary readings and direct fallback.
    ///
    /// This is the preferred lattice builder for mixed Chinese text where
    /// direct pinyin or punctuation spans may appear beside
    /// CC-CEDICT-backed surfaces.
    pub fn hybrid_pinyin_lattice(
        &self,
        text: &str,
        options: PinyinReadingOptions,
    ) -> Result<Option<Lattice>, CnLatticeError> {
        let paths = self
            .try_hybrid_reading_paths_with_stats(text, options)
            .map_err(|err| CnLatticeError::ArtifactPayload(err.to_string()))?
            .paths;
        if paths.is_empty() {
            return Ok(None);
        }
        pinyin_lattice_from_reading_paths(&paths).map(Some)
    }

    fn reading_paths_with_stats_inner(
        &self,
        text: &str,
        options: PinyinReadingOptions,
        allow_direct_fallback: bool,
    ) -> Result<PinyinReadingExpansion, ZhArtifactPayloadError> {
        if text.is_empty() || options.max_span_chars == 0 || options.max_paths == 0 {
            return Ok(PinyinReadingExpansion::default());
        }

        let mut stats = PinyinReadingStats::default();
        let boundaries = char_boundaries(text);
        let char_len = boundaries.len() - 1;
        let mut suffix_paths = vec![Vec::<PinyinReadingPath>::new(); char_len + 1];
        suffix_paths[char_len].push(PinyinReadingPath {
            segments: Vec::new(),
            joined_reading: String::new(),
        });

        for start in (0..char_len).rev() {
            let mut paths_by_reading = BTreeMap::new();
            let end_limit = char_len.min(start + options.max_span_chars);
            let mut matching_ends = Vec::new();

            for end in start + 1..=end_limit {
                let surface = &text[boundaries[start]..boundaries[end]];
                if !suffix_paths[end].is_empty() {
                    if let Some(surface_readings) = self.try_readings(surface)? {
                        matching_ends.push((end, surface_readings));
                    }
                }
            }
            stats.matched_spans += matching_ends.len();

            if options.longest_match_only {
                let pruned_spans = matching_ends.len().saturating_sub(1);
                if let Some(longest_match) = matching_ends.pop() {
                    stats.longest_match_pruned_spans += pruned_spans;
                    matching_ends.clear();
                    matching_ends.push(longest_match);
                }
            }

            for (end, surface_readings) in matching_ends {
                let surface = &text[boundaries[start]..boundaries[end]];

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
                        segments.push(PinyinReadingSegment {
                            surface: surface.to_string(),
                            reading: surface_reading.to_string(),
                            source: PinyinReadingSegmentSource::Dictionary,
                        });
                        segments.extend(suffix.segments.iter().cloned());

                        match paths_by_reading.entry(reading.clone()) {
                            Entry::Vacant(entry) => {
                                entry.insert(PinyinReadingPath {
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
                        let reading = normalize_direct_pinyin_input(surface);
                        for suffix in &suffix_paths[end] {
                            stats.candidate_combinations += 1;
                            let mut joined =
                                String::with_capacity(reading.len() + suffix.joined_reading.len());
                            joined.push_str(&reading);
                            joined.push_str(&suffix.joined_reading);

                            let mut segments = Vec::with_capacity(suffix.segments.len() + 1);
                            segments.push(PinyinReadingSegment {
                                surface: surface.to_string(),
                                reading: reading.clone(),
                                source: PinyinReadingSegmentSource::Direct,
                            });
                            segments.extend(suffix.segments.iter().cloned());

                            match paths_by_reading.entry(joined.clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(PinyinReadingPath {
                                        segments,
                                        joined_reading: joined,
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

        Ok(PinyinReadingExpansion {
            paths: suffix_paths.remove(0),
            stats,
        })
    }
}

/// Compares two strings using direct pinyin handling and a CC-CEDICT index.
pub fn compare_with_cedict_index(
    left: &str,
    right: &str,
    index: &CedictReadingIndex,
    options: PinyinReadingOptions,
) -> Result<ChineseDistance, CnLatticeError> {
    compare_with_zh_index(left, right, index, options)
}

/// Compares two strings using direct pinyin handling and a Chinese index.
pub fn compare_with_zh_index(
    left: &str,
    right: &str,
    index: &ZhReadingIndex,
    options: PinyinReadingOptions,
) -> Result<ChineseDistance, CnLatticeError> {
    let left_lattice = cedict_or_direct_lattice(left, index, options)?;
    let right_lattice = cedict_or_direct_lattice(right, index, options)?;
    compare_lattices(left, right, &left_lattice, &right_lattice)
}

/// Computes the best normalized similarity across Chinese pinyin readings.
pub fn normalized_similarity_with_zh_index(
    left: &str,
    right: &str,
    index: &ZhReadingIndex,
    options: PinyinReadingOptions,
) -> Result<f64, CnLatticeError> {
    let left_paths = zh_or_direct_pinyin_paths(left, index, options)?;
    let right_paths = zh_or_direct_pinyin_paths(right, index, options)?;
    Ok(max_normalized_similarity(&left_paths, &right_paths))
}

/// Builds a pinyin lattice from direct input, CC-CEDICT readings, or both.
pub fn cedict_or_direct_lattice(
    input: &str,
    index: &CedictReadingIndex,
    options: PinyinReadingOptions,
) -> Result<Lattice, CnLatticeError> {
    zh_or_direct_lattice(input, index, options)
}

/// Builds a pinyin lattice from direct input, dictionary readings, or both.
pub fn zh_or_direct_lattice(
    input: &str,
    index: &ZhReadingIndex,
    options: PinyinReadingOptions,
) -> Result<Lattice, CnLatticeError> {
    if let Some(lattice) = direct_pinyin_lattice(input) {
        return Ok(lattice);
    }

    if let Some(lattice) = index.pinyin_lattice(input, options)? {
        return Ok(lattice);
    }

    if let Some(lattice) = index.hybrid_pinyin_lattice(input, options)? {
        return Ok(lattice);
    }

    direct_pinyin_lattice(input).ok_or_else(|| CnLatticeError::UnsupportedDirectInput {
        surface: input.to_string(),
    })
}

/// Returns pinyin paths from direct input, dictionary readings, or both.
pub fn zh_or_direct_pinyin_paths(
    input: &str,
    index: &ZhReadingIndex,
    options: PinyinReadingOptions,
) -> Result<Vec<String>, CnLatticeError> {
    if can_build_direct_pinyin_input(input) {
        return Ok(vec![normalize_direct_pinyin_input(input)]);
    }

    let paths = index
        .try_reading_paths_with_stats(input, options)
        .map_err(|err| CnLatticeError::ArtifactPayload(err.to_string()))?
        .paths;
    if !paths.is_empty() {
        return Ok(paths.into_iter().map(|path| path.joined_reading).collect());
    }

    let paths = index
        .try_hybrid_reading_paths_with_stats(input, options)
        .map_err(|err| CnLatticeError::ArtifactPayload(err.to_string()))?
        .paths;
    if !paths.is_empty() {
        return Ok(paths.into_iter().map(|path| path.joined_reading).collect());
    }

    Err(CnLatticeError::UnsupportedDirectInput {
        surface: input.to_string(),
    })
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
) -> Result<ChineseDistance, CnLatticeError> {
    let lattice = try_distance(left_lattice, right_lattice)?;
    let lattice_damerau = try_damerau_distance(left_lattice, right_lattice)?;
    let surface_levenshtein = levenshtein_str(left, right);
    let surface_damerau = try_damerau_levenshtein_str(left, right)?;

    Ok(ChineseDistance {
        surface_levenshtein,
        surface_damerau,
        lattice,
        lattice_damerau,
        combined: surface_damerau.min(lattice),
    })
}

fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn limited_surface_readings(readings: &[String], options: PinyinReadingOptions) -> &[String] {
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
        if !can_build_direct_pinyin_input(surface) {
            break;
        }
        end += 1;
    }

    (end > start).then_some(end)
}

fn pinyin_view_header_value(view: PinyinView) -> u32 {
    match view {
        PinyinView::NoTone => 0,
        PinyinView::Tone3 => 1,
    }
}

fn pinyin_view_from_header_value(value: u32) -> Result<PinyinView, ZhArtifactPayloadError> {
    match value {
        0 => Ok(PinyinView::NoTone),
        1 => Ok(PinyinView::Tone3),
        _ => Err(ZhArtifactPayloadError::UnsupportedIndexedPinyinView { value }),
    }
}

fn write_binary_string(
    writer: &mut impl Write,
    field: &'static str,
    value: &str,
) -> Result<(), ZhArtifactPayloadError> {
    write_u32_len(writer, field, value.len())?;
    writer.write_all(value.as_bytes())?;
    Ok(())
}

fn write_u32_len(
    writer: &mut impl Write,
    field: &'static str,
    len: usize,
) -> Result<(), ZhArtifactPayloadError> {
    let len = u32::try_from(len).map_err(|_| ZhArtifactPayloadError::IndexedSectionTooLarge {
        field,
        len: len as u64,
    })?;
    writer.write_all(&len.to_le_bytes())?;
    Ok(())
}

fn read_indexed_artifact_payload_header_bytes(
    bytes: &[u8],
) -> Result<ZhIndexedArtifactPayloadHeader, ZhArtifactPayloadError> {
    if bytes.len() < INDEXED_ARTIFACT_HEADER_LEN {
        return Err(ZhArtifactPayloadError::TruncatedIndexed { field: "header" });
    }
    let mut magic = [0_u8; 8];
    magic.copy_from_slice(&bytes[..8]);
    if &magic != INDEXED_ARTIFACT_MAGIC {
        return Err(ZhArtifactPayloadError::InvalidIndexedMagic { magic });
    }

    let version = read_u32_le_bytes(bytes, 8, "version")?;
    if version != INDEXED_ARTIFACT_VERSION {
        return Err(ZhArtifactPayloadError::UnsupportedIndexedVersion { version });
    }
    let pinyin_view = pinyin_view_from_header_value(read_u32_le_bytes(bytes, 12, "pinyin_view")?)?;
    let entry_count = read_u64_le_bytes(bytes, 16, "entry_count")?;
    let fst_len = read_u64_le_bytes(bytes, 24, "fst_len")?;
    let readings_len = read_u64_le_bytes(bytes, 32, "readings_len")?;
    let entries = checked_indexed_usize("entry_count", entry_count)?;
    check_limit("entry_count", entries, MAX_ARTIFACT_ENTRIES)?;
    Ok(ZhIndexedArtifactPayloadHeader {
        version,
        pinyin_view,
        entries,
        fst_len: checked_indexed_usize("fst_len", fst_len)?,
        readings_len: checked_indexed_usize("readings_len", readings_len)?,
    })
}

fn read_u32_le_bytes(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<u32, ZhArtifactPayloadError> {
    let end = offset
        .checked_add(4)
        .ok_or(ZhArtifactPayloadError::TruncatedIndexed { field })?;
    let chunk = bytes
        .get(offset..end)
        .ok_or(ZhArtifactPayloadError::TruncatedIndexed { field })?;
    Ok(u32::from_le_bytes(
        chunk.try_into().expect("slice length is 4"),
    ))
}

fn read_u64_le_bytes(
    bytes: &[u8],
    offset: usize,
    field: &'static str,
) -> Result<u64, ZhArtifactPayloadError> {
    let end = offset
        .checked_add(8)
        .ok_or(ZhArtifactPayloadError::TruncatedIndexed { field })?;
    let chunk = bytes
        .get(offset..end)
        .ok_or(ZhArtifactPayloadError::TruncatedIndexed { field })?;
    Ok(u64::from_le_bytes(
        chunk.try_into().expect("slice length is 8"),
    ))
}

fn checked_indexed_usize(field: &'static str, len: u64) -> Result<usize, ZhArtifactPayloadError> {
    usize::try_from(len).map_err(|_| ZhArtifactPayloadError::IndexedSectionTooLarge { field, len })
}

fn check_payload_file_size(path: &Path) -> Result<(), ZhArtifactPayloadError> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_ARTIFACT_PAYLOAD_BYTES {
        return Err(ZhArtifactPayloadError::ArtifactLimitExceeded {
            field: "payload_bytes",
            len,
            max: MAX_ARTIFACT_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

fn check_limit(field: &'static str, len: usize, max: usize) -> Result<(), ZhArtifactPayloadError> {
    if len > max {
        return Err(ZhArtifactPayloadError::ArtifactLimitExceeded {
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
) -> Result<(), ZhArtifactPayloadError> {
    write_u32_len(writer, "reading_count", readings.len())?;
    for reading in readings {
        write_binary_string(writer, "reading", reading)?;
    }
    Ok(())
}

impl IndexedZhPayload {
    fn validate(&self, pinyin_view: PinyinView) -> Result<(), ZhArtifactPayloadError> {
        let mut stream = self.map.stream();
        while let Some((surface, offset)) = stream.next() {
            let surface = String::from_utf8(surface.to_vec()).map_err(|source| {
                ZhArtifactPayloadError::InvalidIndexedUtf8 {
                    field: "surface",
                    source,
                }
            })?;
            if surface.is_empty() {
                return Err(ZhArtifactPayloadError::EmptySurface { entry_index: 0 });
            }
            let readings = self.readings_at(offset)?;
            if readings.is_empty() {
                return Err(ZhArtifactPayloadError::EmptyReadings { surface });
            }
            let mut seen = BTreeSet::new();
            for (reading_index, reading) in readings.iter().enumerate() {
                if reading.is_empty() {
                    return Err(ZhArtifactPayloadError::EmptyReading {
                        surface: surface.clone(),
                        reading_index,
                    });
                }
                let normalized = normalize_artifact_reading(reading, pinyin_view);
                if normalized != *reading {
                    return Err(ZhArtifactPayloadError::ReadingNotNormalized {
                        surface: surface.clone(),
                        reading: reading.clone(),
                        normalized,
                    });
                }
                if !seen.insert(reading) {
                    return Err(ZhArtifactPayloadError::DuplicateReading {
                        surface: surface.clone(),
                        reading: reading.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn readings(&self, surface: &str) -> Result<Option<Vec<String>>, ZhArtifactPayloadError> {
        self.map
            .get(surface)
            .map(|offset| self.readings_at(offset))
            .transpose()
    }

    fn entries(&self) -> Result<Vec<ZhReadingIndexPayloadEntry>, ZhArtifactPayloadError> {
        let mut entries = Vec::with_capacity(self.entries);
        let mut stream = self.map.stream();
        while let Some((surface, offset)) = stream.next() {
            let surface = String::from_utf8(surface.to_vec()).map_err(|source| {
                ZhArtifactPayloadError::InvalidIndexedUtf8 {
                    field: "surface",
                    source,
                }
            })?;
            let readings = self.readings_at(offset)?;
            entries.push(ZhReadingIndexPayloadEntry { surface, readings });
        }
        Ok(entries)
    }

    fn readings_at(&self, offset: u64) -> Result<Vec<String>, ZhArtifactPayloadError> {
        read_indexed_readings_at_bytes(&self.mmap, self.readings_start, offset)
    }
}

fn read_indexed_readings_at_bytes(
    bytes: &[u8],
    readings_start: usize,
    offset: u64,
) -> Result<Vec<String>, ZhArtifactPayloadError> {
    let offset = usize::try_from(offset)
        .map_err(|_| ZhArtifactPayloadError::InvalidIndexedOffset { offset })?;
    let start =
        readings_start
            .checked_add(offset)
            .ok_or(ZhArtifactPayloadError::InvalidIndexedOffset {
                offset: offset as u64,
            })?;
    if start >= bytes.len() {
        return Err(ZhArtifactPayloadError::InvalidIndexedOffset {
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
            .ok_or(ZhArtifactPayloadError::TruncatedIndexed { field: "reading" })?;
        let reading_bytes = bytes
            .get(cursor..end)
            .ok_or(ZhArtifactPayloadError::TruncatedIndexed { field: "reading" })?;
        let reading = String::from_utf8(reading_bytes.to_vec()).map_err(|source| {
            ZhArtifactPayloadError::InvalidIndexedUtf8 {
                field: "reading",
                source,
            }
        })?;
        readings.push(reading);
        cursor = end;
    }
    Ok(readings)
}

/// Computes the SHA-256 file digest string for a Chinese artifact payload file.
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
    payload: &ZhReadingIndexPayload,
) -> Result<(), ZhArtifactPayloadError> {
    if payload.schema_version != ARTIFACT_PAYLOAD_SCHEMA_VERSION {
        return Err(ZhArtifactPayloadError::UnsupportedSchemaVersion {
            version: payload.schema_version,
        });
    }
    if payload.payload_type != ARTIFACT_PAYLOAD_TYPE {
        return Err(ZhArtifactPayloadError::UnsupportedPayloadType {
            payload_type: payload.payload_type.clone(),
        });
    }
    Ok(())
}

fn canonical_payload_bytes(payload: &ZhReadingIndexPayload) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"moine.zh.reading-index.surface-readings/v1\n");
    push_len_prefixed(&mut bytes, b"V", &payload.pinyin_view);
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

struct CedictEntry<'a> {
    traditional: &'a str,
    simplified: &'a str,
    pinyin: &'a str,
}

fn parse_cedict_entry(line: &str, line_number: usize) -> Result<CedictEntry<'_>, CedictError> {
    let (traditional, rest) = take_token(line)
        .ok_or_else(|| invalid_entry(line_number, "missing traditional surface"))?;
    let (simplified, rest) = take_token(rest.trim_start())
        .ok_or_else(|| invalid_entry(line_number, "missing simplified surface"))?;
    let rest = rest.trim_start();

    let (pinyin, rest) = if let Some(after_open) = rest.strip_prefix("[[") {
        let Some(end) = after_open.find("]]") else {
            return Err(invalid_entry(line_number, "missing closing ]] for pinyin"));
        };
        (&after_open[..end], &after_open[end + 2..])
    } else if let Some(after_open) = rest.strip_prefix('[') {
        let Some(end) = after_open.find(']') else {
            return Err(invalid_entry(line_number, "missing closing ] for pinyin"));
        };
        (&after_open[..end], &after_open[end + 1..])
    } else {
        return Err(invalid_entry(line_number, "missing pinyin bracket"));
    };

    if pinyin.is_empty() {
        return Err(invalid_entry(line_number, "empty pinyin field"));
    }
    if !rest.trim_start().starts_with('/') {
        return Err(invalid_entry(line_number, "missing definition slash"));
    }

    Ok(CedictEntry {
        traditional,
        simplified,
        pinyin,
    })
}

fn invalid_entry(line: usize, message: impl Into<String>) -> CedictError {
    CedictError::InvalidEntry {
        line,
        message: message.into(),
    }
}

fn take_token(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() {
            return Some((&input[..index], &input[index..]));
        }
    }
    Some((input, ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_pinyin_views() {
        assert_eq!(
            normalize_pinyin("Wei1 shi4 ji4", PinyinView::NoTone),
            "weishiji"
        );
        assert_eq!(
            normalize_pinyin("Wei1 shi4 ji4", PinyinView::Tone3),
            "wei1shi4ji4"
        );
        assert_eq!(normalize_pinyin("nu:3 er2", PinyinView::NoTone), "nver");
        assert_eq!(normalize_pinyin("nu:3 er2", PinyinView::Tone3), "nv3er2");
        assert_eq!(normalize_pinyin("hua1 r5", PinyinView::NoTone), "huar");
        assert_eq!(normalize_pinyin("11 Qu1", PinyinView::NoTone), "11qu");
        assert_eq!(normalize_pinyin("Shuang1 11", PinyinView::NoTone), "shuang");
        assert_eq!(
            normalize_pinyin("D N A jian4 ding4", PinyinView::NoTone),
            "dnajianding"
        );
    }

    #[test]
    fn builds_no_tone_index_from_cedict() {
        let cedict = "\
# CC-CEDICT
威士忌 威士忌 [Wei1 shi4 ji4] /whisky/
布納哈本 布纳哈本 [Bu4 na4 ha1 ben3] /Bunnahabhain/
女兒 女儿 [nu:3 er2] /daughter/
";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();

        assert_eq!(index.pinyin_view(), PinyinView::NoTone);
        assert_eq!(
            index.readings("威士忌").as_deref(),
            Some(&["weishiji".to_string()][..])
        );
        assert_eq!(
            index.readings("布纳哈本").as_deref(),
            Some(&["bunahaben".to_string()][..])
        );
        assert_eq!(
            index.readings("女儿").as_deref(),
            Some(&["nver".to_string()][..])
        );
    }

    #[test]
    fn builds_tone3_index_when_requested() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let index = CedictReadingIndex::from_cedict_reader_with_options(
            cedict.as_bytes(),
            CedictIndexOptions {
                pinyin_view: PinyinView::Tone3,
                ..CedictIndexOptions::default()
            },
        )
        .unwrap();

        assert_eq!(index.pinyin_view(), PinyinView::Tone3);
        assert_eq!(
            index.readings("威士忌").as_deref(),
            Some(&["wei1shi4ji4".to_string()][..])
        );
    }

    #[test]
    fn deduplicates_after_normalization() {
        let cedict = "\
樂 乐 [Le4] /surname Le/
樂 乐 [le4] /happy/
樂 乐 [Yue4] /surname Yue/
";
        let no_tone = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let tone3 = CedictReadingIndex::from_cedict_reader_with_options(
            cedict.as_bytes(),
            CedictIndexOptions {
                pinyin_view: PinyinView::Tone3,
                ..CedictIndexOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            no_tone.readings("乐").as_deref(),
            Some(&["le".to_string(), "yue".to_string()][..])
        );
        assert_eq!(
            tone3.readings("乐").as_deref(),
            Some(&["le4".to_string(), "yue4".to_string()][..])
        );
    }

    #[test]
    fn rejects_malformed_entries() {
        let err = CedictReadingIndex::from_cedict_reader(
            "威士忌 威士忌 Wei1 shi4 ji4 /whisky/\n".as_bytes(),
        )
        .unwrap_err();

        assert!(matches!(err, CedictError::InvalidEntry { line: 1, .. }));
    }

    #[test]
    fn computes_dictionary_paths_and_stats() {
        let cedict = "\
威 威 [wei1] /power/
士忌 士忌 [shi4 ji4] /whisky transcription tail/
威士忌 威士忌 [Wei1 shi4 ji4] /whisky/
";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let expansion = index.reading_paths_with_stats(
            "威士忌",
            PinyinReadingOptions {
                longest_match_only: true,
                ..PinyinReadingOptions::default()
            },
        );

        assert_eq!(expansion.paths.len(), 1);
        assert_eq!(expansion.paths[0].joined_reading, "weishiji");
        assert_eq!(
            expansion.paths[0].segments,
            vec![PinyinReadingSegment {
                surface: "威士忌".to_string(),
                reading: "weishiji".to_string(),
                source: PinyinReadingSegmentSource::Dictionary,
            }]
        );
        assert_eq!(expansion.stats.longest_match_pruned_spans, 1);
    }

    #[test]
    fn hybrid_paths_allow_ascii_prefix_and_dictionary_tail() {
        let cedict = "忌 忌 [ji4] /whisky transcription character/\n";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let paths = index.hybrid_reading_paths("weishi忌", PinyinReadingOptions::default());

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].joined_reading, "weishiji");
    }

    #[test]
    fn hybrid_paths_allow_dictionary_text_with_chinese_punctuation() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let paths = index.hybrid_reading_paths("威士忌。", PinyinReadingOptions::default());

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].joined_reading, "weishiji。");
    }

    #[test]
    fn compare_matches_pinyin_input_to_chinese_surface() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let distances = compare_with_cedict_index(
            "weishiji",
            "威士忌",
            &index,
            PinyinReadingOptions::default(),
        )
        .unwrap();

        assert_eq!(distances.lattice, 0);
        assert_eq!(distances.lattice_damerau, 0);
        assert!(distances.surface_damerau > distances.lattice);
    }

    #[test]
    fn compare_handles_chinese_punctuation_between_pinyin_and_dictionary_spans() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let distances = compare_with_cedict_index(
            "weishiji，威士忌。",
            "威士忌，weishiji。",
            &index,
            PinyinReadingOptions::default(),
        )
        .unwrap();

        assert_eq!(distances.lattice, 0);
    }

    #[test]
    fn direct_pinyin_normalizes_unicode_whitespace() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        for whitespace in [' ', '\u{00a0}', '\u{2003}', '\u{2009}', '\u{3000}'] {
            let input = format!("weishi{whitespace}ji");
            let distances = compare_with_cedict_index(
                &input,
                "威士忌",
                &index,
                PinyinReadingOptions::default(),
            )
            .unwrap();

            assert_eq!(distances.lattice, 0);
        }
    }

    #[test]
    fn lattice_damerau_counts_adjacent_pinyin_transposition() {
        let distances = compare_with_cedict_index(
            "weishiji",
            "wieshiji",
            &CedictReadingIndex::default(),
            PinyinReadingOptions::default(),
        )
        .unwrap();

        assert_eq!(distances.lattice, 2);
        assert_eq!(distances.lattice_damerau, 1);
    }

    #[test]
    fn normalized_similarity_matches_pinyin_input_to_chinese_surface() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let similarity = normalized_similarity_with_zh_index(
            "weishiji",
            "威士忌",
            &index,
            PinyinReadingOptions::default(),
        )
        .unwrap();

        assert_eq!(similarity, 1.0);
    }

    #[test]
    fn emits_and_loads_artifact_payload() {
        let cedict = "\
威士忌 威士忌 [Wei1 shi4 ji4] /whisky/
布納哈本 布纳哈本 [Bu4 na4 ha1 ben3] /Bunnahabhain/
";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let payload = index.artifact_payload();
        let loaded = ZhReadingIndex::from_artifact_payload(payload).unwrap();

        assert_eq!(loaded.pinyin_view(), PinyinView::NoTone);
        assert_eq!(
            loaded.readings("威士忌").as_deref(),
            Some(&["weishiji".to_string()][..])
        );
        assert_eq!(
            loaded.readings("布纳哈本").as_deref(),
            Some(&["bunahaben".to_string()][..])
        );
        assert_eq!(
            loaded.artifact_payload_checksum(),
            index.artifact_payload_checksum()
        );
    }

    #[test]
    fn indexed_artifact_payload_round_trips_and_supports_lookup() {
        let cedict = "\
威士忌 威士忌 [Wei1 shi4 ji4] /whisky/
布納哈本 布纳哈本 [Bu4 na4 ha1 ben3] /Bunnahabhain/
";
        let index = CedictReadingIndex::from_cedict_reader(cedict.as_bytes()).unwrap();
        let mut bytes = Vec::new();
        index.write_indexed_artifact_payload(&mut bytes).unwrap();
        let path = std::env::temp_dir().join(format!(
            "moine-zh-indexed-test-{}-{}.moineidx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &bytes).unwrap();
        let loaded = ZhReadingIndex::from_indexed_artifact_payload_path(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let loaded_from_bytes = ZhReadingIndex::from_indexed_artifact_payload_bytes(&bytes)
            .expect("indexed payload bytes should load");

        assert_eq!(loaded.pinyin_view(), PinyinView::NoTone);
        assert_eq!(
            loaded.readings("威士忌").as_deref(),
            Some(&["weishiji".to_string()][..])
        );
        assert_eq!(
            loaded_from_bytes.artifact_payload(),
            index.artifact_payload()
        );
        assert_eq!(
            loaded.readings("布纳哈本").as_deref(),
            Some(&["bunahaben".to_string()][..])
        );
        assert_eq!(
            loaded.artifact_payload_checksum(),
            index.artifact_payload_checksum()
        );
    }

    #[test]
    fn artifact_metadata_records_build_and_license() {
        let cedict = "威士忌 威士忌 [Wei1 shi4 ji4] /whisky/\n";
        let options = CedictIndexOptions {
            pinyin_view: PinyinView::Tone3,
            max_readings_per_surface: Some(4),
        };
        let index = CedictReadingIndex::from_cedict_reader_with_options(cedict.as_bytes(), options)
            .unwrap();
        let metadata = index.artifact_metadata(ZhArtifactMetadataOptions {
            artifact_name: "moine-cedict-test".to_string(),
            generator: "test".to_string(),
            payload_file_name: "payload.yaml".to_string(),
            payload_format: "yaml.surface-readings.v1".to_string(),
            source_name: "CC-CEDICT".to_string(),
            source_version: "2026-05-20".to_string(),
            source_cedict: "cedict.txt".to_string(),
            index_options: options,
            query_defaults: PinyinReadingOptions {
                longest_match_only: true,
                ..PinyinReadingOptions::default()
            },
            license: ZhArtifactLicense::default(),
        });

        assert_eq!(metadata.artifact_type, "moine.zh.reading-index");
        assert_eq!(metadata.build.pinyin_view, "tone3");
        assert_eq!(metadata.build.max_readings_per_surface, Some(4));
        assert!(metadata.query_defaults.longest_match_only);
        assert_eq!(metadata.license.selected_license, "CC BY-SA 4.0");
    }

    #[test]
    fn rejects_duplicate_artifact_surface() {
        let payload = ZhReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.zh.reading-index.surface-readings".to_string(),
            pinyin_view: "no-tone".to_string(),
            entries: vec![
                ZhReadingIndexPayloadEntry {
                    surface: "威士忌".to_string(),
                    readings: vec!["weishiji".to_string()],
                },
                ZhReadingIndexPayloadEntry {
                    surface: "威士忌".to_string(),
                    readings: vec!["weishiji".to_string()],
                },
            ],
        };
        let err = ZhReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            ZhArtifactPayloadError::DuplicateSurface { .. }
        ));
    }

    #[test]
    fn rejects_artifact_payload_excessive_reading_count() {
        let payload = ZhReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.zh.reading-index.surface-readings".to_string(),
            pinyin_view: "no-tone".to_string(),
            entries: vec![ZhReadingIndexPayloadEntry {
                surface: "威士忌".to_string(),
                readings: vec!["weishiji".to_string(); MAX_ARTIFACT_READINGS_PER_ENTRY + 1],
            }],
        };
        let err = ZhReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            ZhArtifactPayloadError::ArtifactLimitExceeded {
                field: "reading_count",
                ..
            }
        ));
    }

    #[test]
    fn rejects_non_normalized_artifact_reading() {
        let payload = ZhReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.zh.reading-index.surface-readings".to_string(),
            pinyin_view: "no-tone".to_string(),
            entries: vec![ZhReadingIndexPayloadEntry {
                surface: "威士忌".to_string(),
                readings: vec!["Wei1shi4ji4".to_string()],
            }],
        };
        let err = ZhReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            ZhArtifactPayloadError::ReadingNotNormalized { .. }
        ));
    }

    #[test]
    fn no_tone_artifact_rejects_tone_digits_after_letters() {
        let payload = ZhReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.zh.reading-index.surface-readings".to_string(),
            pinyin_view: "no-tone".to_string(),
            entries: vec![ZhReadingIndexPayloadEntry {
                surface: "威士忌".to_string(),
                readings: vec!["wei1shi4ji4".to_string()],
            }],
        };
        let err = ZhReadingIndex::from_artifact_payload(payload).unwrap_err();

        assert!(matches!(
            err,
            ZhArtifactPayloadError::ReadingNotNormalized { .. }
        ));
    }

    #[test]
    fn artifact_validation_keeps_numeric_tokens_in_no_tone_view() {
        let payload = ZhReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.zh.reading-index.surface-readings".to_string(),
            pinyin_view: "no-tone".to_string(),
            entries: vec![ZhReadingIndexPayloadEntry {
                surface: "11区".to_string(),
                readings: vec!["11qu".to_string()],
            }],
        };
        let index = ZhReadingIndex::from_artifact_payload(payload).unwrap();

        assert_eq!(
            index.readings("11区").as_deref(),
            Some(&["11qu".to_string()][..])
        );
    }

    #[test]
    fn tone3_view_preserves_tone_digits() {
        let cedict = "重 重 [chong2] /again/\n重 重 [zhong4] /heavy/\n";
        let index = CedictReadingIndex::from_cedict_reader_with_options(
            cedict.as_bytes(),
            CedictIndexOptions {
                pinyin_view: PinyinView::Tone3,
                ..CedictIndexOptions::default()
            },
        )
        .unwrap();
        let distances =
            compare_with_cedict_index("zhong4", "重", &index, PinyinReadingOptions::default())
                .unwrap();

        assert_eq!(distances.lattice, 0);
    }

    #[test]
    fn unknown_han_without_dictionary_path_is_rejected() {
        let index = CedictReadingIndex::default();
        let err =
            cedict_or_direct_lattice("印", &index, PinyinReadingOptions::default()).unwrap_err();

        assert!(matches!(err, CnLatticeError::UnsupportedDirectInput { .. }));
    }
}
