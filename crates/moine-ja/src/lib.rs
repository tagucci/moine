//! Japanese kana, romaji, override, and UniDic adapters for `moine`.
//!
//! This crate converts Japanese surface text into romaji lattices through
//! direct kana/ASCII handling, manual override dictionaries, or UniDic-derived
//! reading artifacts. The language-independent edit-distance algorithms remain
//! in `moine-core`.
//!
//! Dictionary artifacts are external input. Prefer `try_*` lookup and expansion
//! APIs at trust boundaries so indexed-payload decode errors are reported as
//! [`UnidicArtifactPayloadError`] instead of being collapsed into empty lookup
//! results for backward-compatible convenience APIs.
//!
//! ```
//! use moine_ja::romaji_lattice;
//! use moine_core::{distance, Lattice};
//!
//! let left = romaji_lattice("もいにゃ").unwrap();
//! let right = Lattice::from_paths(["moinya"]);
//!
//! assert_eq!(distance(&left, &right), 0);
//! ```
//!
#![deny(missing_docs)]

mod distance;
mod kana;
mod overrides;
mod romaji;
mod unidic;

pub use distance::{
    compare_with_overrides, compare_with_unidic_index, normalized_similarity_with_unidic_index,
    unidic_or_direct_lattice, unidic_or_direct_romaji_paths, JapaneseDistance,
};
pub use kana::{is_kana, normalize_kana, normalize_kana_char};
pub use overrides::{OverrideDictionary, OverrideLoadError};
pub use romaji::{romaji_lattice, romaji_paths, JaLatticeError, RomajiVariantTable};
pub use unidic::{
    artifact_file_digest_path, artifact_file_digest_reader, romaji_lattice_from_reading_paths,
    romaji_paths_from_reading_paths, DictionaryReadingExpansion, DictionaryReadingOptions,
    DictionaryReadingPath, DictionaryReadingSegment, DictionaryReadingStats, UnidicArtifactBuild,
    UnidicArtifactLicense, UnidicArtifactLicenseReference, UnidicArtifactMetadata,
    UnidicArtifactMetadataOptions, UnidicArtifactPayload, UnidicArtifactPayloadError,
    UnidicArtifactQueryDefaults, UnidicArtifactSource, UnidicBinaryArtifactPayloadHeader,
    UnidicCsvError, UnidicIndexOptions, UnidicReadingField, UnidicReadingIndex,
    UnidicReadingIndexPayload, UnidicReadingIndexPayloadEntry, ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM,
    ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM, LEGACY_ARTIFACT_PAYLOAD_CHECKSUM_ALGORITHM,
};
