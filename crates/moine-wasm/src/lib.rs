use std::io::Cursor;

use moine_core::{levenshtein_str, try_damerau_levenshtein_str, try_distance};
use moine_ja::{
    unidic_or_direct_lattice, DictionaryReadingOptions, JaLatticeError, UnidicArtifactMetadata,
    UnidicReadingIndex,
};
use moine_zh::{
    zh_or_direct_lattice, CedictReadingIndex, CnLatticeError, PinyinReadingOptions,
    ZhArtifactMetadata,
};
use sha2::{Digest, Sha256};
use wasm_bindgen::prelude::*;

const MAX_WASM_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[wasm_bindgen]
pub struct ComparisonResult {
    levenshtein_distance: usize,
    damerau_levenshtein_distance: usize,
    lattice_path_edit_distance: usize,
}

#[wasm_bindgen]
impl ComparisonResult {
    #[wasm_bindgen(getter, js_name = levenshteinDistance)]
    pub fn levenshtein_distance(&self) -> usize {
        self.levenshtein_distance
    }

    #[wasm_bindgen(getter, js_name = damerauLevenshteinDistance)]
    pub fn damerau_levenshtein_distance(&self) -> usize {
        self.damerau_levenshtein_distance
    }

    #[wasm_bindgen(getter, js_name = latticePathEditDistance)]
    pub fn lattice_path_edit_distance(&self) -> usize {
        self.lattice_path_edit_distance
    }
}

#[wasm_bindgen]
#[derive(Default)]
pub struct MoineDemo {
    japanese: Option<JapaneseDictionary>,
    chinese: Option<ChineseDictionary>,
}

#[wasm_bindgen]
impl MoineDemo {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(js_name = loadJapaneseDictionary)]
    pub fn load_japanese_dictionary(
        &mut self,
        metadata_yaml: &str,
        payload: &[u8],
    ) -> Result<(), JsValue> {
        self.japanese = Some(load_japanese_dictionary(metadata_yaml, payload)?);
        Ok(())
    }

    #[wasm_bindgen(js_name = loadChineseDictionary)]
    pub fn load_chinese_dictionary(
        &mut self,
        metadata_yaml: &str,
        payload: &[u8],
    ) -> Result<(), JsValue> {
        self.chinese = Some(load_chinese_dictionary(metadata_yaml, payload)?);
        Ok(())
    }

    pub fn compare(
        &self,
        lang: &str,
        left: &str,
        right: &str,
    ) -> Result<ComparisonResult, JsValue> {
        let lattice_path_edit_distance = match lang {
            "ja" => self
                .japanese
                .as_ref()
                .ok_or_else(|| JsValue::from_str("Japanese dictionary is not loaded"))?
                .distance(left, right)?,
            "zh" => self
                .chinese
                .as_ref()
                .ok_or_else(|| JsValue::from_str("Chinese dictionary is not loaded"))?
                .distance(left, right)?,
            _ => return Err(JsValue::from_str("lang must be 'ja' or 'zh'")),
        };

        Ok(ComparisonResult {
            levenshtein_distance: levenshtein_str(left, right),
            damerau_levenshtein_distance: try_damerau_levenshtein_str(left, right)
                .map_err(distance_error)?,
            lattice_path_edit_distance,
        })
    }
}

struct JapaneseDictionary {
    index: UnidicReadingIndex,
    options: DictionaryReadingOptions,
}

impl JapaneseDictionary {
    fn distance(&self, left: &str, right: &str) -> Result<usize, JsValue> {
        let left_lattice =
            unidic_or_direct_lattice(left, &self.index, self.options).map_err(japanese_error)?;
        let right_lattice =
            unidic_or_direct_lattice(right, &self.index, self.options).map_err(japanese_error)?;
        try_distance(&left_lattice, &right_lattice).map_err(distance_error)
    }
}

struct ChineseDictionary {
    index: CedictReadingIndex,
    options: PinyinReadingOptions,
}

impl ChineseDictionary {
    fn distance(&self, left: &str, right: &str) -> Result<usize, JsValue> {
        let left_lattice =
            zh_or_direct_lattice(left, &self.index, self.options).map_err(chinese_error)?;
        let right_lattice =
            zh_or_direct_lattice(right, &self.index, self.options).map_err(chinese_error)?;
        try_distance(&left_lattice, &right_lattice).map_err(distance_error)
    }
}

fn load_japanese_dictionary(
    metadata_yaml: &str,
    payload: &[u8],
) -> Result<JapaneseDictionary, JsValue> {
    verify_payload_size(payload, "Japanese")?;
    let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(metadata_yaml)
        .map_err(|err| JsValue::from_str(&format!("invalid Japanese metadata: {err}")))?;
    if metadata.schema_version != 1 {
        return Err(JsValue::from_str(
            "unsupported Japanese metadata schema version",
        ));
    }
    if metadata.artifact_type != "moine.unidic.reading-index" {
        return Err(JsValue::from_str("unsupported Japanese artifact type"));
    }
    verify_file_digest(
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
        payload,
        "Japanese",
    )?;

    let index = match metadata.payload.format.as_str() {
        "yaml.surface-readings.v1" => {
            UnidicReadingIndex::from_artifact_payload_reader(Cursor::new(payload))
        }
        "binary.surface-readings.v1" => {
            UnidicReadingIndex::from_binary_artifact_payload_reader(Cursor::new(payload))
        }
        "indexed-fst.surface-readings.v1" => {
            UnidicReadingIndex::from_indexed_artifact_payload_bytes(payload)
        }
        unsupported => {
            return Err(JsValue::from_str(&format!(
                "unsupported Japanese payload format {unsupported:?}"
            )));
        }
    }
    .map_err(|err| JsValue::from_str(&format!("invalid Japanese payload: {err}")))?;
    verify_japanese_payload_checksum(&metadata, &index)?;
    if index.len() != metadata.build.entries {
        return Err(JsValue::from_str("Japanese payload entry count mismatch"));
    }

    Ok(JapaneseDictionary {
        index,
        options: DictionaryReadingOptions {
            max_span_chars: metadata.query_defaults.max_span_chars,
            max_paths: metadata.query_defaults.max_paths,
            longest_match_only: metadata.query_defaults.longest_match_only,
            max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
        },
    })
}

fn load_chinese_dictionary(
    metadata_yaml: &str,
    payload: &[u8],
) -> Result<ChineseDictionary, JsValue> {
    verify_payload_size(payload, "Chinese")?;
    let metadata = serde_yaml::from_str::<ZhArtifactMetadata>(metadata_yaml)
        .map_err(|err| JsValue::from_str(&format!("invalid Chinese metadata: {err}")))?;
    if metadata.schema_version != 1 {
        return Err(JsValue::from_str(
            "unsupported Chinese metadata schema version",
        ));
    }
    if metadata.artifact_type != "moine.zh.reading-index" {
        return Err(JsValue::from_str("unsupported Chinese artifact type"));
    }
    verify_file_digest(
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
        payload,
        "Chinese",
    )?;

    let index = match metadata.payload.format.as_str() {
        "yaml.surface-readings.v1" => {
            CedictReadingIndex::from_artifact_payload_reader(Cursor::new(payload))
        }
        "indexed-fst.surface-readings.v1" => {
            CedictReadingIndex::from_indexed_artifact_payload_bytes(payload)
        }
        unsupported => {
            return Err(JsValue::from_str(&format!(
                "unsupported Chinese payload format {unsupported:?}"
            )));
        }
    }
    .map_err(|err| JsValue::from_str(&format!("invalid Chinese payload: {err}")))?;
    verify_chinese_payload_checksum(&metadata, &index)?;
    if index.len() != metadata.build.entries {
        return Err(JsValue::from_str("Chinese payload entry count mismatch"));
    }
    if index.pinyin_view().as_str() != metadata.build.pinyin_view {
        return Err(JsValue::from_str("Chinese payload pinyin view mismatch"));
    }

    Ok(ChineseDictionary {
        index,
        options: PinyinReadingOptions {
            max_span_chars: metadata.query_defaults.max_span_chars,
            max_paths: metadata.query_defaults.max_paths,
            longest_match_only: metadata.query_defaults.longest_match_only,
            max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
        },
    })
}

fn verify_payload_size(payload: &[u8], label: &str) -> Result<(), JsValue> {
    if payload.len() > MAX_WASM_PAYLOAD_BYTES {
        return Err(JsValue::from_str(&format!(
            "{label} payload exceeded {MAX_WASM_PAYLOAD_BYTES} bytes"
        )));
    }
    Ok(())
}

fn verify_japanese_payload_checksum(
    metadata: &UnidicArtifactMetadata,
    index: &UnidicReadingIndex,
) -> Result<(), JsValue> {
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            JsValue::from_str(&format!(
                "unsupported Japanese payload checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum == metadata.payload.checksum {
        Ok(())
    } else {
        Err(JsValue::from_str("Japanese payload checksum mismatch"))
    }
}

fn verify_chinese_payload_checksum(
    metadata: &ZhArtifactMetadata,
    index: &CedictReadingIndex,
) -> Result<(), JsValue> {
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            JsValue::from_str(&format!(
                "unsupported Chinese payload checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum == metadata.payload.checksum {
        Ok(())
    } else {
        Err(JsValue::from_str("Chinese payload checksum mismatch"))
    }
}

fn verify_file_digest(
    algorithm: Option<&str>,
    expected: Option<&str>,
    payload: &[u8],
    label: &str,
) -> Result<(), JsValue> {
    match (algorithm, expected) {
        (None, None) => Ok(()),
        (Some("sha256-file-v1"), Some(expected)) => {
            let digest = Sha256::digest(payload);
            let actual = format!("{digest:x}");
            if actual == expected {
                Ok(())
            } else {
                Err(JsValue::from_str(&format!(
                    "{label} payload file digest mismatch"
                )))
            }
        }
        (Some(unsupported), Some(_)) => Err(JsValue::from_str(&format!(
            "unsupported {label} payload file digest algorithm {unsupported:?}"
        ))),
        _ => Err(JsValue::from_str(&format!(
            "{label} payload file digest algorithm and digest must be provided together"
        ))),
    }
}

fn japanese_error(err: JaLatticeError) -> JsValue {
    JsValue::from_str(&format!("Japanese LPED failed: {err}"))
}

fn chinese_error(err: CnLatticeError) -> JsValue {
    JsValue::from_str(&format!("Chinese LPED failed: {err}"))
}

fn distance_error(err: moine_core::DistanceError) -> JsValue {
    JsValue::from_str(&format!("LPED failed: {err}"))
}

#[wasm_bindgen(js_name = levenshteinDistance)]
pub fn levenshtein_distance(left: &str, right: &str) -> usize {
    levenshtein_str(left, right)
}

#[wasm_bindgen(js_name = damerauLevenshteinDistance)]
pub fn damerau_levenshtein_distance(left: &str, right: &str) -> Result<usize, JsValue> {
    try_damerau_levenshtein_str(left, right).map_err(distance_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JA_METADATA: &str = r#"
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: test
payload:
  path: readings.yaml
  format: yaml.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: ignored
source:
  name: UniDic-CWJ
  version: test
  lex_csv: test.csv
build:
  reading_field: pron
  max_readings_per_surface: null
  exclude_ascii_surfaces: true
  exclude_symbol_pos: true
  entries: 2
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references: []
"#;

    const JA_PAYLOAD: &str = r#"
schema_version: 1
payload_type: moine.unidic.reading-index.surface-readings
entries:
- surface: モイニャ
  readings:
  - モイニャ
- surface: です
  readings:
  - デス
"#;

    const ZH_METADATA: &str = r#"
schema_version: 1
artifact_type: moine.zh.reading-index
artifact_name: test
generator: test
payload:
  path: readings.yaml
  format: yaml.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: ignored
source:
  name: CC-CEDICT
  version: test
  cedict: cedict.txt
build:
  pinyin_view: no-tone
  max_readings_per_surface: null
  entries: 1
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: null
license:
  selected_license: CC BY-SA 4.0
  references: []
"#;

    const ZH_PAYLOAD: &str = r#"
schema_version: 1
payload_type: moine.zh.reading-index.surface-readings
pinyin_view: no-tone
entries:
- surface: 威士忌
  readings:
  - weishiji
"#;

    #[test]
    fn compares_japanese_with_loaded_dictionary() {
        let mut demo = MoineDemo::new();
        let metadata = JA_METADATA.replace(
            "checksum: ignored",
            &format!(
                "checksum: {}",
                UnidicReadingIndex::from_artifact_payload_reader(Cursor::new(
                    JA_PAYLOAD.as_bytes()
                ))
                .unwrap()
                .artifact_payload_checksum()
            ),
        );
        demo.load_japanese_dictionary(&metadata, JA_PAYLOAD.as_bytes())
            .unwrap();

        let result = demo.compare("ja", "moine", "モイニャ").unwrap();
        assert_eq!(result.levenshtein_distance(), 5);
        assert_eq!(result.lattice_path_edit_distance(), 2);

        let kana_result = demo.compare("ja", "もいにゃ", "モイニャ").unwrap();
        assert_eq!(kana_result.lattice_path_edit_distance(), 0);

        let punctuated_result = demo
            .compare("ja", "モイニャです。", "もいにゃです。")
            .unwrap();
        assert_eq!(punctuated_result.lattice_path_edit_distance(), 0);
    }

    #[test]
    fn compares_chinese_with_loaded_dictionary() {
        let mut demo = MoineDemo::new();
        let metadata = ZH_METADATA.replace(
            "checksum: ignored",
            &format!(
                "checksum: {}",
                CedictReadingIndex::from_artifact_payload_reader(Cursor::new(
                    ZH_PAYLOAD.as_bytes()
                ))
                .unwrap()
                .artifact_payload_checksum()
            ),
        );
        demo.load_chinese_dictionary(&metadata, ZH_PAYLOAD.as_bytes())
            .unwrap();

        let result = demo.compare("zh", "weishiji", "威士忌").unwrap();
        assert_eq!(result.levenshtein_distance(), 8);
        assert_eq!(result.lattice_path_edit_distance(), 0);

        let punctuated_result = demo
            .compare("zh", "weishiji，威士忌。", "威士忌，weishiji。")
            .unwrap();
        assert_eq!(punctuated_result.lattice_path_edit_distance(), 0);
    }

    #[test]
    fn loads_indexed_japanese_dictionary() {
        let source =
            UnidicReadingIndex::from_artifact_payload_reader(Cursor::new(JA_PAYLOAD.as_bytes()))
                .unwrap();
        let mut payload = Vec::new();
        source.write_indexed_artifact_payload(&mut payload).unwrap();
        let metadata = JA_METADATA
            .replace(
                "format: yaml.surface-readings.v1",
                "format: indexed-fst.surface-readings.v1",
            )
            .replace(
                "checksum: ignored",
                &format!("checksum: {}", source.artifact_payload_checksum()),
            );

        let mut demo = MoineDemo::new();
        demo.load_japanese_dictionary(&metadata, &payload).unwrap();
        let result = demo.compare("ja", "もいにゃ", "モイニャ").unwrap();
        assert_eq!(result.lattice_path_edit_distance(), 0);
    }

    #[test]
    fn loads_indexed_chinese_dictionary() {
        let source =
            CedictReadingIndex::from_artifact_payload_reader(Cursor::new(ZH_PAYLOAD.as_bytes()))
                .unwrap();
        let mut payload = Vec::new();
        source.write_indexed_artifact_payload(&mut payload).unwrap();
        let metadata = ZH_METADATA
            .replace(
                "format: yaml.surface-readings.v1",
                "format: indexed-fst.surface-readings.v1",
            )
            .replace(
                "checksum: ignored",
                &format!("checksum: {}", source.artifact_payload_checksum()),
            );

        let mut demo = MoineDemo::new();
        demo.load_chinese_dictionary(&metadata, &payload).unwrap();
        let result = demo.compare("zh", "weishiji", "威士忌").unwrap();
        assert_eq!(result.lattice_path_edit_distance(), 0);
    }
}
