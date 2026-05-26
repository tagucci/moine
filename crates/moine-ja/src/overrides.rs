use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use serde::Deserialize;

use moine_core::Lattice;

use crate::romaji::{romaji_lattice, romaji_lattice_from_readings, JaLatticeError};

const SUPPORTED_VERSION: u32 = 1;

/// In-memory surface-to-reading override dictionary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OverrideDictionary {
    readings_by_surface: HashMap<String, Vec<String>>,
}

/// Errors returned while loading an override dictionary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OverrideLoadError {
    /// The YAML document could not be parsed.
    Yaml(String),
    /// The override file version is not supported.
    UnsupportedVersion {
        /// Version read from the file.
        version: u32,
    },
    /// An override entry had an empty surface form.
    EmptySurface {
        /// Zero-based entry index.
        entry_index: usize,
    },
    /// An override entry had no readings.
    EmptyReadings {
        /// Surface form for the invalid entry.
        surface: String,
    },
    /// An override entry contained an empty reading.
    EmptyReading {
        /// Surface form for the invalid entry.
        surface: String,
        /// Zero-based reading index.
        reading_index: usize,
    },
    /// A surface form appeared more than once.
    DuplicateSurface {
        /// Duplicated surface form.
        surface: String,
    },
}

impl fmt::Display for OverrideLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Yaml(err) => write!(f, "invalid override YAML: {err}"),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported override version {version}")
            }
            Self::EmptySurface { entry_index } => {
                write!(f, "override entry {entry_index} has an empty surface")
            }
            Self::EmptyReadings { surface } => {
                write!(f, "override entry {surface:?} has no readings")
            }
            Self::EmptyReading {
                surface,
                reading_index,
            } => write!(
                f,
                "override entry {surface:?} has an empty reading at index {reading_index}"
            ),
            Self::DuplicateSurface { surface } => {
                write!(f, "override entry {surface:?} is duplicated")
            }
        }
    }
}

impl Error for OverrideLoadError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OverrideFile {
    version: u32,
    entries: Vec<OverrideEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct OverrideEntry {
    surface: String,
    readings: Vec<String>,
}

impl OverrideDictionary {
    /// Creates an empty override dictionary.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds an override dictionary from `(surface, readings)` entries.
    pub fn from_entries<I, S, R, Rs>(entries: I) -> Self
    where
        I: IntoIterator<Item = (S, Rs)>,
        S: Into<String>,
        R: Into<String>,
        Rs: IntoIterator<Item = R>,
    {
        let mut dict = Self::new();
        for (surface, readings) in entries {
            dict.insert_readings(surface, readings);
        }
        dict
    }

    /// Loads an override dictionary from a YAML string.
    pub fn from_yaml_str(input: &str) -> Result<Self, OverrideLoadError> {
        let file = serde_yaml::from_str::<OverrideFile>(input)
            .map_err(|err| OverrideLoadError::Yaml(err.to_string()))?;
        Self::from_override_file(file)
    }

    /// Inserts or replaces readings for a surface form.
    pub fn insert_readings<S, R, Rs>(&mut self, surface: S, readings: Rs)
    where
        S: Into<String>,
        R: Into<String>,
        Rs: IntoIterator<Item = R>,
    {
        self.readings_by_surface.insert(
            surface.into(),
            readings.into_iter().map(Into::into).collect(),
        );
    }

    /// Returns override readings for `surface`, if present.
    pub fn readings(&self, surface: &str) -> Option<&[String]> {
        self.readings_by_surface
            .get(surface)
            .map(std::vec::Vec::as_slice)
    }

    /// Builds a romaji lattice using an override when one exists.
    pub fn romaji_lattice(&self, input: &str) -> Result<Lattice, JaLatticeError> {
        if let Some(readings) = self.readings(input) {
            romaji_lattice_from_readings(readings)
        } else {
            romaji_lattice(input)
        }
    }

    fn from_override_file(file: OverrideFile) -> Result<Self, OverrideLoadError> {
        if file.version != SUPPORTED_VERSION {
            return Err(OverrideLoadError::UnsupportedVersion {
                version: file.version,
            });
        }

        let mut dict = Self::new();
        for (entry_index, entry) in file.entries.into_iter().enumerate() {
            let surface = entry.surface.trim();
            if surface.is_empty() {
                return Err(OverrideLoadError::EmptySurface { entry_index });
            }
            if entry.readings.is_empty() {
                return Err(OverrideLoadError::EmptyReadings {
                    surface: surface.to_string(),
                });
            }
            if dict.readings_by_surface.contains_key(surface) {
                return Err(OverrideLoadError::DuplicateSurface {
                    surface: surface.to_string(),
                });
            }

            let mut readings = Vec::with_capacity(entry.readings.len());
            for (reading_index, reading) in entry.readings.into_iter().enumerate() {
                let reading = reading.trim();
                if reading.is_empty() {
                    return Err(OverrideLoadError::EmptyReading {
                        surface: surface.to_string(),
                        reading_index,
                    });
                }
                readings.push(reading.to_string());
            }

            dict.insert_readings(surface, readings);
        }

        Ok(dict)
    }
}

#[cfg(test)]
mod tests {
    use moine_core::{distance, distance_with_trace, Lattice};

    use super::*;

    fn symbols_to_string(symbols: &[moine_core::Symbol]) -> String {
        symbols
            .iter()
            .map(|&symbol| char::from_u32(symbol).expect("test symbol should be a char"))
            .collect()
    }

    #[test]
    fn override_reading_builds_romaji_lattice_for_kanji_surface() {
        let dict = OverrideDictionary::from_entries([("鬼滅の刃", ["キメツノヤイバ"])]);

        let left = romaji_lattice("きめつのやいば").expect("kana input should build");
        let right = dict
            .romaji_lattice("鬼滅の刃")
            .expect("override should build");

        assert_eq!(distance(&left, &right), 0);
    }

    #[test]
    fn multiple_override_readings_share_one_lookup() {
        let dict = OverrideDictionary::from_entries([("茶道具", ["チャドウグ", "チャドーグ"])]);

        let lattice = dict
            .romaji_lattice("茶道具")
            .expect("override should build");

        assert_eq!(distance(&lattice, &Lattice::from_paths(["chadougu"])), 0);
        assert_eq!(distance(&lattice, &Lattice::from_paths(["chado-gu"])), 0);
    }

    #[test]
    fn override_trace_exposes_best_path() {
        let dict = OverrideDictionary::from_entries([("印刷", ["インサツ"])]);

        let left = romaji_lattice("いんさt").expect("kana ascii input should build");
        let right = dict.romaji_lattice("印刷").expect("override should build");
        let trace = distance_with_trace(&left, &right);

        assert_eq!(trace.distance, 1);
        assert_eq!(symbols_to_string(&trace.left_symbols()), "insat");
        assert_eq!(symbols_to_string(&trace.right_symbols()), "insatu");
    }

    #[test]
    fn loads_versioned_yaml_fixture() {
        let dict =
            OverrideDictionary::from_yaml_str(include_str!("../tests/resources/overrides.yaml"))
                .expect("fixture should load");

        assert_eq!(
            dict.readings("茶道具"),
            Some(&["チャドウグ".to_string(), "チャドーグ".to_string()][..])
        );
    }

    #[test]
    fn rejects_unsupported_version() {
        let result = OverrideDictionary::from_yaml_str(
            "
version: 2
entries: []
",
        );

        assert!(matches!(
            result,
            Err(OverrideLoadError::UnsupportedVersion { version: 2 })
        ));
    }

    #[test]
    fn rejects_duplicate_surface() {
        let result = OverrideDictionary::from_yaml_str(
            "
version: 1
entries:
  - surface: 印刷
    readings: [インサツ]
  - surface: 印刷
    readings: [インサツ]
",
        );

        assert!(matches!(
            result,
            Err(OverrideLoadError::DuplicateSurface { surface }) if surface == "印刷"
        ));
    }

    #[test]
    fn rejects_empty_reading() {
        let result = OverrideDictionary::from_yaml_str(
            "
version: 1
entries:
  - surface: 印刷
    readings: [インサツ, '']
",
        );

        assert!(matches!(
            result,
            Err(OverrideLoadError::EmptyReading {
                surface,
                reading_index: 1,
            }) if surface == "印刷"
        ));
    }
}
