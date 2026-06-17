use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::romaji::can_build_romaji_reading;
use crate::unidic::{
    field, insert_surface_reading, is_symbol_pos, lex_csv_reader, normalize_ascii_width,
    UnidicCsvError, UnidicReadingIndex,
};

const SURFACE_COLUMN: usize = 0;
const NORMALIZED_COLUMN: usize = 4;
const POS1_COLUMN: usize = 5;
const READING_COLUMN: usize = 11;

/// Options used while building a Sudachi CSV reading index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SudachiIndexOptions {
    /// Optional cap on readings stored for each surface form.
    pub max_readings_per_surface: Option<usize>,
    /// Exclude ASCII-only dictionary surfaces.
    pub exclude_ascii_surfaces: bool,
    /// Exclude entries whose coarse part of speech is a symbol.
    pub exclude_symbol_pos: bool,
    /// Add Sudachi normalized-form aliases as lookup surfaces.
    pub include_normalized_surfaces: bool,
    /// Exclude readings that cannot be converted to romaji paths.
    pub exclude_unsupported_readings: bool,
}

impl Default for SudachiIndexOptions {
    fn default() -> Self {
        Self {
            max_readings_per_surface: None,
            exclude_ascii_surfaces: true,
            exclude_symbol_pos: true,
            include_normalized_surfaces: true,
            exclude_unsupported_readings: false,
        }
    }
}

impl UnidicReadingIndex {
    /// Builds an index from a Sudachi dictionary source CSV file.
    ///
    /// Sudachi full dictionaries are represented by concatenating
    /// `small_lex.csv`, `core_lex.csv`, and `notcore_lex.csv` from the same
    /// release.
    pub fn from_sudachi_lex_csv_path(path: impl AsRef<Path>) -> Result<Self, UnidicCsvError> {
        Self::from_sudachi_lex_csv_path_with_options(path, SudachiIndexOptions::default())
    }

    /// Builds an index from a Sudachi dictionary source CSV file with custom
    /// options.
    pub fn from_sudachi_lex_csv_path_with_options(
        path: impl AsRef<Path>,
        options: SudachiIndexOptions,
    ) -> Result<Self, UnidicCsvError> {
        let file = File::open(path)?;
        Self::from_sudachi_lex_csv_reader_with_options(file, options)
    }

    /// Builds an index from a reader containing Sudachi dictionary source CSV
    /// data.
    pub fn from_sudachi_lex_csv_reader(reader: impl Read) -> Result<Self, UnidicCsvError> {
        Self::from_sudachi_lex_csv_reader_with_options(reader, SudachiIndexOptions::default())
    }

    /// Builds an index from a Sudachi dictionary source CSV reader with custom
    /// options.
    pub fn from_sudachi_lex_csv_reader_with_options(
        reader: impl Read,
        options: SudachiIndexOptions,
    ) -> Result<Self, UnidicCsvError> {
        let mut readings = HashMap::<String, BTreeSet<String>>::new();
        let mut reading_support = HashMap::<String, bool>::new();
        for record in lex_csv_reader(reader).records() {
            let record = record?;
            let entry = SudachiLexEntry::parse(&record)?;

            if !entry.should_index(options, &mut reading_support) {
                continue;
            }

            entry.insert_readings(&mut readings, options);
        }

        Ok(Self::from_readings_by_surface(
            into_limited_reading_vectors(readings, options.max_readings_per_surface),
        ))
    }
}

struct SudachiLexEntry<'a> {
    surface: &'a str,
    normalized: &'a str,
    pos1: &'a str,
    reading: &'a str,
}

impl<'a> SudachiLexEntry<'a> {
    fn parse(record: &'a csv::StringRecord) -> Result<Self, UnidicCsvError> {
        Ok(Self {
            surface: field(record, SURFACE_COLUMN)?,
            normalized: field(record, NORMALIZED_COLUMN)?,
            pos1: field(record, POS1_COLUMN)?,
            reading: field(record, READING_COLUMN)?,
        })
    }

    fn should_index(
        &self,
        options: SudachiIndexOptions,
        reading_support: &mut HashMap<String, bool>,
    ) -> bool {
        if self.surface == "*"
            || self.reading == "*"
            || self.surface.is_empty()
            || self.reading.is_empty()
        {
            return false;
        }
        if options.exclude_ascii_surfaces && self.surface.is_ascii() {
            return false;
        }
        if options.exclude_symbol_pos && is_symbol_pos(self.pos1) {
            return false;
        }
        if options.exclude_unsupported_readings
            && !can_reading_build_romaji(self.reading, reading_support)
        {
            return false;
        }
        true
    }

    fn insert_readings(
        &self,
        readings: &mut HashMap<String, BTreeSet<String>>,
        options: SudachiIndexOptions,
    ) {
        insert_surface_reading(readings, self.surface, self.reading);

        if self.has_normalized_alias(options) {
            insert_surface_reading(readings, self.normalized, self.reading);
        }
        if let Some(surface) = normalize_ascii_width(self.surface) {
            insert_surface_reading(readings, &surface, self.reading);
        }
    }

    fn has_normalized_alias(&self, options: SudachiIndexOptions) -> bool {
        options.include_normalized_surfaces
            && self.normalized != "*"
            && !self.normalized.is_empty()
            && self.normalized != self.surface
            && (!options.exclude_ascii_surfaces || !self.normalized.is_ascii())
    }
}

fn can_reading_build_romaji(reading: &str, cache: &mut HashMap<String, bool>) -> bool {
    if let Some(value) = cache.get(reading) {
        return *value;
    }
    let value = can_build_romaji_reading(reading);
    cache.insert(reading.to_string(), value);
    value
}

fn into_limited_reading_vectors(
    readings: HashMap<String, BTreeSet<String>>,
    max_readings_per_surface: Option<usize>,
) -> HashMap<String, Vec<String>> {
    readings
        .into_iter()
        .map(|(surface, readings)| {
            let mut readings = readings.into_iter().collect::<Vec<_>>();
            if let Some(max_readings) = max_readings_per_surface {
                readings.truncate(max_readings);
            }
            (surface, readings)
        })
        .filter(|(_, readings)| !readings.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_sudachi_surface_to_readings_index() {
        let csv = "\
鬼滅の刃,4785,4785,15000,鬼滅の刃,名詞,固有名詞,一般,*,*,*,キメツノヤイバ,鬼滅の刃,*,A,*,*,*,*
単式蒸留器,5146,5774,20098,単式蒸留器,名詞,普通名詞,一般,*,*,*,タンシキジョウリュウキ,単式蒸留器,*,C,328549/654780/361310,328549/1510969,328549/1510969,*
malt whisky,5139,5139,5000,malt whisky,名詞,普通名詞,一般,*,*,*,モルトウイスキー,モルトウイスキー,*,A,*,*,*,020573
🥃,5968,5968,22000,🥃,補助記号,一般,*,*,*,*,ウィスキー,ウィスキー,*,A,*,*,*,*
シングルモルトウイスキー,5144,5144,5930,シングルモルトウイスキー,名詞,固有名詞,一般,*,*,*,シングルモルトウイスキー,シングル・モルト・ウイスキー,*,C,207600/257972/180439,207600/257972/180439,207600/257972/180439,*
";
        let index = UnidicReadingIndex::from_sudachi_lex_csv_reader(csv.as_bytes()).unwrap();

        assert_eq!(
            index.readings("鬼滅の刃").as_deref(),
            Some(&["キメツノヤイバ".to_string()][..])
        );
        assert_eq!(
            index.readings("単式蒸留器").as_deref(),
            Some(&["タンシキジョウリュウキ".to_string()][..])
        );
        assert_eq!(index.readings("malt whisky"), None);
        assert_eq!(index.readings("🥃"), None);
        assert_eq!(
            index.readings("シングルモルトウイスキー").as_deref(),
            Some(&["シングルモルトウイスキー".to_string()][..])
        );
    }

    #[test]
    fn sudachi_options_can_include_ascii_and_symbol_entries() {
        let csv = "\
scotch whisky,5139,5139,5000,Scotch whisky,名詞,普通名詞,一般,*,*,*,scotch whisky,Scotch whisky,*,A,*,*,*,019988
🥃,5968,5968,22000,🥃,補助記号,一般,*,*,*,*,ウィスキー,ウィスキー,*,A,*,*,*,*
";
        let index = UnidicReadingIndex::from_sudachi_lex_csv_reader_with_options(
            csv.as_bytes(),
            SudachiIndexOptions {
                exclude_ascii_surfaces: false,
                exclude_symbol_pos: false,
                ..SudachiIndexOptions::default()
            },
        )
        .unwrap();

        assert_eq!(
            index.readings("scotch whisky").as_deref(),
            Some(&["scotch whisky".to_string()][..])
        );
        assert_eq!(
            index.readings("Scotch whisky").as_deref(),
            Some(&["scotch whisky".to_string()][..])
        );
        assert_eq!(
            index.readings("🥃").as_deref(),
            Some(&["ウィスキー".to_string()][..])
        );
    }

    #[test]
    fn sudachi_options_can_exclude_unsupported_readings() {
        let csv = "ジョニー・ウォーカー,4788,4788,8922,ジョニー・ウォーカー,名詞,固有名詞,人名,一般,*,*,ジョニー・ウォーカー,ジョニー・ウォーカー,*,C,209649/267999/181003,209649/267999/181003,209649/267999/181003,*\n";
        let index = UnidicReadingIndex::from_sudachi_lex_csv_reader_with_options(
            csv.as_bytes(),
            SudachiIndexOptions {
                exclude_unsupported_readings: true,
                ..SudachiIndexOptions::default()
            },
        )
        .unwrap();

        assert_eq!(index.readings("ジョニー・ウォーカー"), None);
    }

    #[test]
    fn compare_skips_unsupported_sudachi_long_spans_so_hybrid_paths_can_work() {
        let csv = "\
Bunnahabhain蒸留所,4785,4785,15000,Bunnahabhain蒸留所,名詞,固有名詞,一般,*,*,*,ブナハーブン・ジョウリュウジョ,Bunnahabhain蒸留所,*,A,*,*,*,*
蒸留所,4785,4785,15000,蒸留所,名詞,普通名詞,一般,*,*,*,ジョウリュウジョ,蒸留所,*,A,*,*,*,*
";
        let index = UnidicReadingIndex::from_sudachi_lex_csv_reader(csv.as_bytes()).unwrap();

        assert_eq!(
            index.readings("Bunnahabhain蒸留所").as_deref(),
            Some(&["ブナハーブン・ジョウリュウジョ".to_string()][..])
        );
        assert_eq!(
            index.readings("蒸留所").as_deref(),
            Some(&["ジョウリュウジョ".to_string()][..])
        );

        let distances = crate::compare_with_unidic_index(
            "Bunnahabhainじょうりゅうじょ",
            "Bunnahabhain蒸留所",
            &index,
            crate::DictionaryReadingOptions::default(),
        )
        .expect("supported shorter Sudachi span should allow hybrid comparison");
        assert_eq!(distances.lattice, 0);
    }
}
