//! Public Rust API for `moine`.
//!
//! The crate keeps the language-independent edit-distance core available at the
//! root and exposes language adapters under explicit modules.
//!
//! The default feature includes the `moine` CLI binary for `cargo install
//! moine`. Library users who want only the Rust APIs can depend on the crate
//! with `default-features = false`.
//!
//! Dictionary data is distributed separately from the source crates. Rust users
//! load verified bundles explicitly through [`ja::load_bundle`] or
//! [`zh::load_bundle`].
//!
//! ```
//! use moine::{distance, Lattice};
//!
//! let left = Lattice::from_paths(["moine"]);
//! let right = Lattice::from_paths(["moinya"]);
//! assert_eq!(distance(&left, &right), 2);
//! ```
#![deny(missing_docs)]

pub use moine_core::*;

pub mod ja {
    //! Japanese kana, romaji, override, UniDic, and Sudachi adapters.

    use std::error::Error;
    use std::fmt;
    use std::path::{Path, PathBuf};

    pub use moine_ja::*;

    /// Errors returned when loading and verifying a Japanese dictionary bundle.
    #[derive(Debug)]
    pub enum BundleLoadError {
        /// Filesystem access failed.
        Io(std::io::Error),
        /// `metadata.yaml` could not be parsed.
        Yaml(serde_yaml::Error),
        /// The bundle metadata schema version is not supported.
        UnsupportedSchemaVersion {
            /// Version read from metadata.
            version: u32,
        },
        /// The metadata artifact type is not a Japanese reading index.
        UnsupportedArtifactType {
            /// Artifact type read from metadata.
            artifact_type: String,
        },
        /// The payload format is not supported by this crate.
        UnsupportedPayloadFormat {
            /// Payload format read from metadata.
            format: String,
        },
        /// The payload checksum algorithm is not supported.
        UnsupportedChecksumAlgorithm {
            /// Checksum algorithm read from metadata.
            algorithm: String,
        },
        /// The canonical payload checksum did not match metadata.
        ChecksumMismatch {
            /// Checksum recorded in metadata.
            expected: String,
            /// Checksum recomputed from the loaded payload.
            actual: String,
        },
        /// The payload entry count did not match metadata.
        EntryCountMismatch {
            /// Entry count recorded in metadata.
            expected: usize,
            /// Entry count read from the payload.
            actual: usize,
        },
        /// A license file referenced by metadata was missing.
        MissingLicense {
            /// License reference label.
            label: String,
            /// Expected license file path.
            path: PathBuf,
        },
        /// The payload file digest algorithm is not supported.
        UnsupportedFileDigestAlgorithm {
            /// File digest algorithm read from metadata.
            algorithm: String,
        },
        /// The payload file digest did not match metadata.
        FileDigestMismatch {
            /// Digest recorded in metadata.
            expected: String,
            /// Digest recomputed from the payload file.
            actual: String,
        },
        /// The file digest algorithm and digest fields were not both present.
        IncompleteFileDigestMetadata,
        /// Metadata pointed outside the bundle directory.
        UnsafeBundlePath {
            /// Unsafe relative path from metadata.
            path: String,
        },
        /// The payload itself failed validation.
        Payload(UnidicArtifactPayloadError),
        /// Other invalid bundle metadata.
        Invalid(String),
    }

    impl fmt::Display for BundleLoadError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Io(err) => write!(f, "{err}"),
                Self::Yaml(err) => write!(f, "{err}"),
                Self::UnsupportedSchemaVersion { version } => {
                    write!(
                        f,
                        "unsupported Japanese dictionary metadata schema version {version}"
                    )
                }
                Self::UnsupportedArtifactType { artifact_type } => {
                    write!(
                        f,
                        "unsupported Japanese dictionary artifact type {artifact_type:?}"
                    )
                }
                Self::UnsupportedPayloadFormat { format } => {
                    write!(
                        f,
                        "unsupported Japanese dictionary payload format {format:?}"
                    )
                }
                Self::UnsupportedChecksumAlgorithm { algorithm } => {
                    write!(
                        f,
                        "unsupported Japanese dictionary payload checksum algorithm {algorithm:?}"
                    )
                }
                Self::ChecksumMismatch { expected, actual } => write!(
                    f,
                    "payload checksum mismatch: metadata has {expected}, recomputed {actual}"
                ),
                Self::EntryCountMismatch { expected, actual } => write!(
                    f,
                    "entry count mismatch: metadata has {expected}, payload has {actual}"
                ),
                Self::MissingLicense { label, path } => {
                    write!(f, "missing license reference {label} at {}", path.display())
                }
                Self::UnsupportedFileDigestAlgorithm { algorithm } => write!(
                    f,
                    "unsupported Japanese dictionary payload file digest algorithm {algorithm:?}"
                ),
                Self::FileDigestMismatch { expected, actual } => write!(
                    f,
                    "payload file digest mismatch: metadata has {expected}, recomputed {actual}"
                ),
                Self::IncompleteFileDigestMetadata => write!(
                    f,
                    "Japanese dictionary payload file digest algorithm and digest must be provided together"
                ),
                Self::UnsafeBundlePath { path } => write!(
                    f,
                    "bundle path {path:?} must be relative and stay inside the bundle"
                ),
                Self::Payload(err) => write!(f, "Japanese dictionary payload error: {err}"),
                Self::Invalid(message) => write!(f, "{message}"),
            }
        }
    }

    impl Error for BundleLoadError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::Io(err) => Some(err),
                Self::Yaml(err) => Some(err),
                Self::UnsupportedSchemaVersion { .. }
                | Self::UnsupportedArtifactType { .. }
                | Self::UnsupportedPayloadFormat { .. }
                | Self::UnsupportedChecksumAlgorithm { .. }
                | Self::ChecksumMismatch { .. }
                | Self::EntryCountMismatch { .. }
                | Self::MissingLicense { .. }
                | Self::UnsupportedFileDigestAlgorithm { .. }
                | Self::FileDigestMismatch { .. }
                | Self::IncompleteFileDigestMetadata
                | Self::UnsafeBundlePath { .. } => None,
                Self::Payload(err) => Some(err),
                Self::Invalid(_) => None,
            }
        }
    }

    impl From<std::io::Error> for BundleLoadError {
        fn from(source: std::io::Error) -> Self {
            Self::Io(source)
        }
    }

    impl From<serde_yaml::Error> for BundleLoadError {
        fn from(source: serde_yaml::Error) -> Self {
            Self::Yaml(source)
        }
    }

    /// Verified Japanese dictionary bundle ready for scoring.
    #[derive(Debug)]
    pub struct LoadedBundle {
        /// Path to the loaded `metadata.yaml`.
        pub metadata_path: PathBuf,
        /// Directory containing the bundle metadata and payload.
        pub bundle_dir: PathBuf,
        /// Path to the reading-index payload file.
        pub payload_path: PathBuf,
        /// Parsed bundle metadata.
        pub metadata: UnidicArtifactMetadata,
        /// Loaded Japanese reading index.
        pub index: UnidicReadingIndex,
        /// Query options derived from bundle metadata defaults.
        pub options: DictionaryReadingOptions,
    }

    impl LoadedBundle {
        /// Computes Japanese LPED for `left` and `right` with this bundle.
        pub fn distance(&self, left: &str, right: &str) -> Result<usize, JaLatticeError> {
            let left_lattice = unidic_or_direct_lattice(left, &self.index, self.options)?;
            let right_lattice = unidic_or_direct_lattice(right, &self.index, self.options)?;
            moine_core::try_distance(&left_lattice, &right_lattice).map_err(JaLatticeError::from)
        }

        /// Computes Japanese lattice-aware Damerau-Levenshtein distance.
        pub fn damerau_distance(&self, left: &str, right: &str) -> Result<usize, JaLatticeError> {
            let left_lattice = unidic_or_direct_lattice(left, &self.index, self.options)?;
            let right_lattice = unidic_or_direct_lattice(right, &self.index, self.options)?;
            moine_core::try_damerau_distance(&left_lattice, &right_lattice)
                .map_err(JaLatticeError::from)
        }

        /// Computes normalized Japanese reading-space similarity.
        pub fn normalized_similarity(
            &self,
            left: &str,
            right: &str,
        ) -> Result<f64, JaLatticeError> {
            normalized_similarity_with_unidic_index(left, right, &self.index, self.options)
        }
    }

    /// Loads and verifies a Japanese dictionary bundle.
    ///
    /// `path` may point either to a bundle directory or directly to its
    /// `metadata.yaml`. The loader verifies metadata schema, payload type,
    /// bundle-relative paths, file digests, canonical payload checksums, entry
    /// counts, and referenced license files before returning.
    pub fn load_bundle(path: impl AsRef<Path>) -> Result<LoadedBundle, BundleLoadError> {
        let metadata_path = metadata_path_from_bundle_arg(path.as_ref());
        let metadata_yaml = std::fs::read_to_string(&metadata_path)?;
        let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(&metadata_yaml)?;
        if metadata.schema_version != 1 {
            return Err(BundleLoadError::UnsupportedSchemaVersion {
                version: metadata.schema_version,
            });
        }
        if metadata.artifact_type != "moine.unidic.reading-index" {
            return Err(BundleLoadError::UnsupportedArtifactType {
                artifact_type: metadata.artifact_type,
            });
        }
        let bundle_dir = metadata_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;
        verify_file_digest(&metadata, &payload_path)?;
        let index = load_payload_by_format(&payload_path, &metadata.payload.format)?;
        let checksum = index
            .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
            .ok_or_else(|| BundleLoadError::UnsupportedChecksumAlgorithm {
                algorithm: metadata.payload.checksum_algorithm.clone(),
            })?;
        if checksum != metadata.payload.checksum {
            return Err(BundleLoadError::ChecksumMismatch {
                expected: metadata.payload.checksum,
                actual: checksum,
            });
        }
        if index.len() != metadata.build.entries {
            return Err(BundleLoadError::EntryCountMismatch {
                expected: metadata.build.entries,
                actual: index.len(),
            });
        }
        for reference in &metadata.license.references {
            let path = checked_bundle_path(&bundle_dir, &reference.path)?;
            if !path.is_file() {
                return Err(BundleLoadError::MissingLicense {
                    label: reference.label.clone(),
                    path,
                });
            }
        }
        let options = DictionaryReadingOptions {
            max_span_chars: metadata.query_defaults.max_span_chars,
            max_paths: metadata.query_defaults.max_paths,
            longest_match_only: metadata.query_defaults.longest_match_only,
            max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
        };

        Ok(LoadedBundle {
            metadata_path,
            bundle_dir,
            payload_path,
            metadata,
            index,
            options,
        })
    }

    fn load_payload_by_format(
        path: &Path,
        payload_format: &str,
    ) -> Result<UnidicReadingIndex, BundleLoadError> {
        match payload_format {
            "yaml.surface-readings.v1" => UnidicReadingIndex::from_artifact_payload_path(path)
                .map_err(BundleLoadError::Payload),
            "binary.surface-readings.v1" => {
                UnidicReadingIndex::from_binary_artifact_payload_path(path)
                    .map_err(BundleLoadError::Payload)
            }
            "indexed-fst.surface-readings.v1" => {
                UnidicReadingIndex::from_indexed_artifact_payload_path(path)
                    .map_err(BundleLoadError::Payload)
            }
            _ => Err(BundleLoadError::UnsupportedPayloadFormat {
                format: payload_format.to_string(),
            }),
        }
    }

    fn verify_file_digest(
        metadata: &UnidicArtifactMetadata,
        payload_path: &Path,
    ) -> Result<(), BundleLoadError> {
        match (
            metadata.payload.file_digest_algorithm.as_deref(),
            metadata.payload.file_digest.as_deref(),
        ) {
            (None, None) => Ok(()),
            (Some(algorithm), Some(expected)) => {
                if algorithm != ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                    return Err(BundleLoadError::UnsupportedFileDigestAlgorithm {
                        algorithm: algorithm.to_string(),
                    });
                }
                let digest = artifact_file_digest_path(payload_path)?;
                if digest != expected {
                    return Err(BundleLoadError::FileDigestMismatch {
                        expected: expected.to_string(),
                        actual: digest,
                    });
                }
                Ok(())
            }
            _ => Err(BundleLoadError::IncompleteFileDigestMetadata),
        }
    }

    fn metadata_path_from_bundle_arg(path: &Path) -> PathBuf {
        if path.is_dir() {
            path.join("metadata.yaml")
        } else {
            path.to_path_buf()
        }
    }

    fn checked_bundle_path(
        bundle_dir: &Path,
        relative_path: &str,
    ) -> Result<PathBuf, BundleLoadError> {
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
            return Err(BundleLoadError::UnsafeBundlePath {
                path: relative_path.to_string(),
            });
        }
        Ok(bundle_dir.join(relative))
    }
}

pub mod zh {
    //! Chinese pinyin and CC-CEDICT adapters.

    use std::error::Error;
    use std::fmt;
    use std::path::{Path, PathBuf};

    pub use moine_zh::*;

    /// Errors returned when loading and verifying a Chinese dictionary bundle.
    #[derive(Debug)]
    pub enum BundleLoadError {
        /// Filesystem access failed.
        Io(std::io::Error),
        /// `metadata.yaml` could not be parsed.
        Yaml(serde_yaml::Error),
        /// The bundle metadata schema version is not supported.
        UnsupportedSchemaVersion {
            /// Version read from metadata.
            version: u32,
        },
        /// The metadata artifact type is not a Chinese reading index.
        UnsupportedArtifactType {
            /// Artifact type read from metadata.
            artifact_type: String,
        },
        /// The payload format is not supported by this crate.
        UnsupportedPayloadFormat {
            /// Payload format read from metadata.
            format: String,
        },
        /// The payload checksum algorithm is not supported.
        UnsupportedChecksumAlgorithm {
            /// Checksum algorithm read from metadata.
            algorithm: String,
        },
        /// The canonical payload checksum did not match metadata.
        ChecksumMismatch {
            /// Checksum recorded in metadata.
            expected: String,
            /// Checksum recomputed from the loaded payload.
            actual: String,
        },
        /// The payload entry count did not match metadata.
        EntryCountMismatch {
            /// Entry count recorded in metadata.
            expected: usize,
            /// Entry count read from the payload.
            actual: usize,
        },
        /// The payload pinyin view did not match metadata.
        PinyinViewMismatch {
            /// Pinyin view recorded in metadata.
            expected: String,
            /// Pinyin view read from the payload.
            actual: String,
        },
        /// A license file referenced by metadata was missing.
        MissingLicense {
            /// License reference label.
            label: String,
            /// Expected license file path.
            path: PathBuf,
        },
        /// The payload file digest algorithm is not supported.
        UnsupportedFileDigestAlgorithm {
            /// File digest algorithm read from metadata.
            algorithm: String,
        },
        /// The payload file digest did not match metadata.
        FileDigestMismatch {
            /// Digest recorded in metadata.
            expected: String,
            /// Digest recomputed from the payload file.
            actual: String,
        },
        /// The file digest algorithm and digest fields were not both present.
        IncompleteFileDigestMetadata,
        /// Metadata pointed outside the bundle directory.
        UnsafeBundlePath {
            /// Unsafe relative path from metadata.
            path: String,
        },
        /// The payload itself failed validation.
        Payload(ZhArtifactPayloadError),
        /// Other invalid bundle metadata.
        Invalid(String),
    }

    impl fmt::Display for BundleLoadError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Io(err) => write!(f, "{err}"),
                Self::Yaml(err) => write!(f, "{err}"),
                Self::UnsupportedSchemaVersion { version } => {
                    write!(f, "unsupported zh metadata schema version {version}")
                }
                Self::UnsupportedArtifactType { artifact_type } => {
                    write!(f, "unsupported zh artifact type {artifact_type:?}")
                }
                Self::UnsupportedPayloadFormat { format } => {
                    write!(f, "unsupported zh payload format {format:?}")
                }
                Self::UnsupportedChecksumAlgorithm { algorithm } => {
                    write!(f, "unsupported zh payload checksum algorithm {algorithm:?}")
                }
                Self::ChecksumMismatch { expected, actual } => write!(
                    f,
                    "payload checksum mismatch: metadata has {expected}, recomputed {actual}"
                ),
                Self::EntryCountMismatch { expected, actual } => write!(
                    f,
                    "entry count mismatch: metadata has {expected}, payload has {actual}"
                ),
                Self::PinyinViewMismatch { expected, actual } => {
                    write!(
                        f,
                        "pinyin view mismatch: metadata has {expected}, payload has {actual}"
                    )
                }
                Self::MissingLicense { label, path } => {
                    write!(f, "missing license reference {label} at {}", path.display())
                }
                Self::UnsupportedFileDigestAlgorithm { algorithm } => {
                    write!(
                        f,
                        "unsupported zh payload file digest algorithm {algorithm:?}"
                    )
                }
                Self::FileDigestMismatch { expected, actual } => write!(
                    f,
                    "payload file digest mismatch: metadata has {expected}, recomputed {actual}"
                ),
                Self::IncompleteFileDigestMetadata => write!(
                    f,
                    "zh payload file digest algorithm and digest must be provided together"
                ),
                Self::UnsafeBundlePath { path } => write!(
                    f,
                    "bundle path {path:?} must be relative and stay inside the bundle"
                ),
                Self::Payload(err) => write!(f, "zh payload error: {err}"),
                Self::Invalid(message) => write!(f, "{message}"),
            }
        }
    }

    impl Error for BundleLoadError {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            match self {
                Self::Io(err) => Some(err),
                Self::Yaml(err) => Some(err),
                Self::UnsupportedSchemaVersion { .. }
                | Self::UnsupportedArtifactType { .. }
                | Self::UnsupportedPayloadFormat { .. }
                | Self::UnsupportedChecksumAlgorithm { .. }
                | Self::ChecksumMismatch { .. }
                | Self::EntryCountMismatch { .. }
                | Self::PinyinViewMismatch { .. }
                | Self::MissingLicense { .. }
                | Self::UnsupportedFileDigestAlgorithm { .. }
                | Self::FileDigestMismatch { .. }
                | Self::IncompleteFileDigestMetadata
                | Self::UnsafeBundlePath { .. } => None,
                Self::Payload(err) => Some(err),
                Self::Invalid(_) => None,
            }
        }
    }

    impl From<std::io::Error> for BundleLoadError {
        fn from(source: std::io::Error) -> Self {
            Self::Io(source)
        }
    }

    impl From<serde_yaml::Error> for BundleLoadError {
        fn from(source: serde_yaml::Error) -> Self {
            Self::Yaml(source)
        }
    }

    /// Verified Chinese dictionary bundle ready for scoring.
    #[derive(Debug)]
    pub struct LoadedBundle {
        /// Path to the loaded `metadata.yaml`.
        pub metadata_path: PathBuf,
        /// Directory containing the bundle metadata and payload.
        pub bundle_dir: PathBuf,
        /// Path to the reading-index payload file.
        pub payload_path: PathBuf,
        /// Parsed bundle metadata.
        pub metadata: ZhArtifactMetadata,
        /// Loaded CC-CEDICT reading index.
        pub index: ZhReadingIndex,
        /// Query options derived from bundle metadata defaults.
        pub options: PinyinReadingOptions,
    }

    impl LoadedBundle {
        /// Computes Chinese LPED for `left` and `right` with this bundle.
        pub fn distance(&self, left: &str, right: &str) -> Result<usize, CnLatticeError> {
            let left_lattice = zh_or_direct_lattice(left, &self.index, self.options)?;
            let right_lattice = zh_or_direct_lattice(right, &self.index, self.options)?;
            moine_core::try_distance(&left_lattice, &right_lattice).map_err(CnLatticeError::from)
        }

        /// Computes Chinese lattice-aware Damerau-Levenshtein distance.
        pub fn damerau_distance(&self, left: &str, right: &str) -> Result<usize, CnLatticeError> {
            let left_lattice = zh_or_direct_lattice(left, &self.index, self.options)?;
            let right_lattice = zh_or_direct_lattice(right, &self.index, self.options)?;
            moine_core::try_damerau_distance(&left_lattice, &right_lattice)
                .map_err(CnLatticeError::from)
        }

        /// Computes normalized Chinese pinyin-space similarity.
        pub fn normalized_similarity(
            &self,
            left: &str,
            right: &str,
        ) -> Result<f64, CnLatticeError> {
            normalized_similarity_with_zh_index(left, right, &self.index, self.options)
        }
    }

    /// Loads and verifies a Chinese dictionary bundle.
    ///
    /// `path` may point either to a bundle directory or directly to its
    /// `metadata.yaml`. The loader verifies metadata schema, payload type,
    /// bundle-relative paths, file digests, canonical payload checksums, entry
    /// counts, pinyin view, and referenced license files before returning.
    pub fn load_bundle(path: impl AsRef<Path>) -> Result<LoadedBundle, BundleLoadError> {
        let metadata_path = metadata_path_from_bundle_arg(path.as_ref());
        let metadata_yaml = std::fs::read_to_string(&metadata_path)?;
        let metadata = serde_yaml::from_str::<ZhArtifactMetadata>(&metadata_yaml)?;
        if metadata.schema_version != 1 {
            return Err(BundleLoadError::UnsupportedSchemaVersion {
                version: metadata.schema_version,
            });
        }
        if metadata.artifact_type != "moine.zh.reading-index" {
            return Err(BundleLoadError::UnsupportedArtifactType {
                artifact_type: metadata.artifact_type,
            });
        }
        let bundle_dir = metadata_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let payload_path = checked_bundle_path(&bundle_dir, &metadata.payload.path)?;
        verify_file_digest(&metadata, &payload_path)?;
        let index = load_payload_by_format(&payload_path, &metadata.payload.format)?;
        if index.len() != metadata.build.entries {
            return Err(BundleLoadError::EntryCountMismatch {
                expected: metadata.build.entries,
                actual: index.len(),
            });
        }
        if index.pinyin_view().as_str() != metadata.build.pinyin_view {
            return Err(BundleLoadError::PinyinViewMismatch {
                expected: metadata.build.pinyin_view,
                actual: index.pinyin_view().as_str().to_string(),
            });
        }
        let checksum = index
            .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
            .ok_or_else(|| BundleLoadError::UnsupportedChecksumAlgorithm {
                algorithm: metadata.payload.checksum_algorithm.clone(),
            })?;
        if checksum != metadata.payload.checksum {
            return Err(BundleLoadError::ChecksumMismatch {
                expected: metadata.payload.checksum,
                actual: checksum,
            });
        }
        for reference in &metadata.license.references {
            let path = checked_bundle_path(&bundle_dir, &reference.path)?;
            if !path.is_file() {
                return Err(BundleLoadError::MissingLicense {
                    label: reference.label.clone(),
                    path,
                });
            }
        }
        let options = PinyinReadingOptions {
            max_span_chars: metadata.query_defaults.max_span_chars,
            max_paths: metadata.query_defaults.max_paths,
            longest_match_only: metadata.query_defaults.longest_match_only,
            max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
        };

        Ok(LoadedBundle {
            metadata_path,
            bundle_dir,
            payload_path,
            metadata,
            index,
            options,
        })
    }

    fn load_payload_by_format(
        path: &Path,
        payload_format: &str,
    ) -> Result<ZhReadingIndex, BundleLoadError> {
        match payload_format {
            "yaml.surface-readings.v1" => {
                ZhReadingIndex::from_artifact_payload_path(path).map_err(BundleLoadError::Payload)
            }
            "indexed-fst.surface-readings.v1" => {
                ZhReadingIndex::from_indexed_artifact_payload_path(path)
                    .map_err(BundleLoadError::Payload)
            }
            _ => Err(BundleLoadError::UnsupportedPayloadFormat {
                format: payload_format.to_string(),
            }),
        }
    }

    fn verify_file_digest(
        metadata: &ZhArtifactMetadata,
        payload_path: &Path,
    ) -> Result<(), BundleLoadError> {
        match (
            metadata.payload.file_digest_algorithm.as_deref(),
            metadata.payload.file_digest.as_deref(),
        ) {
            (None, None) => Ok(()),
            (Some(algorithm), Some(expected)) => {
                if algorithm != ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                    return Err(BundleLoadError::UnsupportedFileDigestAlgorithm {
                        algorithm: algorithm.to_string(),
                    });
                }
                let digest = artifact_file_digest_path(payload_path)?;
                if digest != expected {
                    return Err(BundleLoadError::FileDigestMismatch {
                        expected: expected.to_string(),
                        actual: digest,
                    });
                }
                Ok(())
            }
            _ => Err(BundleLoadError::IncompleteFileDigestMetadata),
        }
    }

    fn metadata_path_from_bundle_arg(path: &Path) -> PathBuf {
        if path.is_dir() {
            path.join("metadata.yaml")
        } else {
            path.to_path_buf()
        }
    }

    fn checked_bundle_path(
        bundle_dir: &Path,
        relative_path: &str,
    ) -> Result<PathBuf, BundleLoadError> {
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
            return Err(BundleLoadError::UnsafeBundlePath {
                path: relative_path.to_string(),
            });
        }
        Ok(bundle_dir.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::{ja, zh};
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn ja_load_bundle_reports_unsafe_path_variant() {
        let temp = temp_dir("moine-rust-ja-bundle-error-test");
        let bundle = temp.join("moine-unidic-test");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(
            bundle.join("metadata.yaml"),
            r#"schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: moine-unidic-test
generator: test
payload:
  path: ../readings.yaml
  format: yaml.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: ignored
source:
  name: UniDic
  version: test
  lex_csv: lex.csv
build:
  reading_field: pron
  max_readings_per_surface:
  exclude_ascii_surfaces: false
  exclude_symbol_pos: false
  entries: 0
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment:
license:
  selected_license: BSD
  references: []
"#,
        )
        .unwrap();

        let err = ja::load_bundle(&bundle).unwrap_err();
        match err {
            ja::BundleLoadError::UnsafeBundlePath { path } => {
                assert_eq!(path, "../readings.yaml");
            }
            other => panic!("expected unsafe bundle path, got {other:?}"),
        }

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn ja_load_bundle_preserves_payload_error_variant() {
        let temp = temp_dir("moine-rust-ja-payload-error-test");
        let bundle = temp.join("moine-unidic-test");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(
            bundle.join("readings.yaml"),
            r#"schema_version: 2
payload_type: moine.unidic.reading-index.surface-readings
entries: []
"#,
        )
        .unwrap();
        fs::write(
            bundle.join("metadata.yaml"),
            r#"schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: moine-unidic-test
generator: test
payload:
  path: readings.yaml
  format: yaml.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: ignored
source:
  name: UniDic
  version: test
  lex_csv: lex.csv
build:
  reading_field: pron
  max_readings_per_surface:
  exclude_ascii_surfaces: false
  exclude_symbol_pos: false
  entries: 0
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment:
license:
  selected_license: BSD
  references: []
"#,
        )
        .unwrap();

        let err = ja::load_bundle(&bundle).unwrap_err();
        match err {
            ja::BundleLoadError::Payload(
                ja::UnidicArtifactPayloadError::UnsupportedSchemaVersion { version: 2 },
            ) => {}
            other => panic!("expected typed payload error, got {other:?}"),
        }

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn zh_load_bundle_reports_unsupported_payload_format_variant() {
        let temp = temp_dir("moine-rust-zh-bundle-error-test");
        let bundle = temp.join("moine-cedict-test");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(
            bundle.join("metadata.yaml"),
            r#"schema_version: 1
artifact_type: moine.zh.reading-index
artifact_name: moine-cedict-test
generator: test
payload:
  path: readings.dat
  format: unsupported.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: ignored
source:
  name: CC-CEDICT
  version: test
  cedict: cedict.txt
build:
  pinyin_view: no-tone
  max_readings_per_surface:
  entries: 0
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment:
license:
  selected_license: CC BY-SA 4.0
  references: []
"#,
        )
        .unwrap();

        let err = zh::load_bundle(&bundle).unwrap_err();
        match err {
            zh::BundleLoadError::UnsupportedPayloadFormat { format } => {
                assert_eq!(format, "unsupported.surface-readings.v1");
            }
            other => panic!("expected unsupported payload format, got {other:?}"),
        }

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn zh_load_bundle_loads_metadata_defaults() {
        let temp = temp_dir("moine-rust-zh-bundle-test");
        let bundle = temp.join("moine-cedict-test");
        fs::create_dir_all(bundle.join("license")).unwrap();
        fs::write(
            bundle.join("license/CC-CEDICT.md"),
            "CC-CEDICT test license\n",
        )
        .unwrap();

        let payload = zh::ZhReadingIndexPayload {
            schema_version: 1,
            payload_type: "moine.zh.reading-index.surface-readings".to_string(),
            pinyin_view: "no-tone".to_string(),
            entries: vec![
                zh::ZhReadingIndexPayloadEntry {
                    surface: "威士忌".to_string(),
                    readings: vec!["weishiji".to_string()],
                },
                zh::ZhReadingIndexPayloadEntry {
                    surface: "布納哈本".to_string(),
                    readings: vec!["bunahaben".to_string()],
                },
            ],
        };
        let index = zh::ZhReadingIndex::from_artifact_payload(payload).unwrap();
        fs::write(
            bundle.join("readings.yaml"),
            serde_yaml::to_string(&index.artifact_payload()).unwrap(),
        )
        .unwrap();
        let metadata = index.artifact_metadata(zh::ZhArtifactMetadataOptions {
            artifact_name: "moine-cedict-test".to_string(),
            generator: "test".to_string(),
            payload_file_name: "readings.yaml".to_string(),
            payload_format: "yaml.surface-readings.v1".to_string(),
            source_name: "CC-CEDICT".to_string(),
            source_version: "test".to_string(),
            source_cedict: "cedict.txt".to_string(),
            index_options: zh::CedictIndexOptions::default(),
            query_defaults: zh::PinyinReadingOptions {
                longest_match_only: true,
                ..zh::PinyinReadingOptions::default()
            },
            license: zh::ZhArtifactLicense {
                selected_license: "CC BY-SA 4.0".to_string(),
                references: vec![zh::ZhArtifactLicenseReference {
                    label: "CC-CEDICT".to_string(),
                    path: "license/CC-CEDICT.md".to_string(),
                }],
            },
        });
        fs::write(
            bundle.join("metadata.yaml"),
            serde_yaml::to_string(&metadata).unwrap(),
        )
        .unwrap();

        let loaded = zh::load_bundle(&bundle).unwrap();

        assert_eq!(loaded.distance("weishiji", "威士忌").unwrap(), 0);
        assert_eq!(loaded.distance("布納哈本", "布納哈本").unwrap(), 0);
        assert_eq!(loaded.damerau_distance("weishiji", "wieshiji").unwrap(), 1);

        fs::remove_dir_all(temp).unwrap();
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }
}
