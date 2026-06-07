#![allow(clippy::too_many_arguments)]

use moine_core::{
    damerau_distance as lattice_damerau_distance, distance as lattice_distance,
    normalized_similarity_str, within_damerau_distance as lattice_within_damerau_distance,
    within_distance as lattice_within_distance, Lattice,
};
use moine_ja::{
    artifact_file_digest_path, normalized_similarity_with_unidic_index, unidic_or_direct_lattice,
    unidic_or_direct_romaji_paths, DictionaryReadingOptions, JaLatticeError,
    UnidicArtifactMetadata, UnidicReadingIndex, ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM,
};
use moine_zh::{
    artifact_file_digest_path as zh_artifact_file_digest_path, normalized_similarity_with_zh_index,
    zh_or_direct_lattice, zh_or_direct_pinyin_paths, CnLatticeError, PinyinReadingOptions,
    ZhArtifactMetadata, ZhReadingIndex,
    ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM as ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

const YAML_PAYLOAD_FORMAT: &str = "yaml.surface-readings.v1";
const BINARY_PAYLOAD_FORMAT: &str = "binary.surface-readings.v1";
const INDEXED_PAYLOAD_FORMAT: &str = "indexed-fst.surface-readings.v1";

type PartialDistanceAlignmentTuple = (usize, usize, usize, usize, usize);
type PartialRatioAlignmentTuple = (f64, usize, usize, usize, usize);
type PyPartialDistanceAlignment = PyResult<Option<PartialDistanceAlignmentTuple>>;
type PyPartialRatioAlignment = PyResult<Option<PartialRatioAlignmentTuple>>;

#[pyfunction(signature = (left, right, *, score_cutoff = None))]
fn distance(py: Python<'_>, left: &str, right: &str, score_cutoff: Option<usize>) -> usize {
    let left = left.to_owned();
    let right = right.to_owned();
    py.detach(move || raw_distance_pair_with_cutoff(&left, &right, score_cutoff))
}

fn raw_distance_pair_with_cutoff(left: &str, right: &str, score_cutoff: Option<usize>) -> usize {
    let left_lattice = Lattice::from_paths([left]);
    let right_lattice = Lattice::from_paths([right]);
    distance_with_cutoff(&left_lattice, &right_lattice, score_cutoff)
}

#[pyfunction(signature = (left, right, *, score_cutoff = None))]
fn damerau_distance(py: Python<'_>, left: &str, right: &str, score_cutoff: Option<usize>) -> usize {
    let left = left.to_owned();
    let right = right.to_owned();
    py.detach(move || raw_damerau_distance_pair_with_cutoff(&left, &right, score_cutoff))
}

fn raw_damerau_distance_pair_with_cutoff(
    left: &str,
    right: &str,
    score_cutoff: Option<usize>,
) -> usize {
    let left_lattice = Lattice::from_paths([left]);
    let right_lattice = Lattice::from_paths([right]);
    damerau_distance_with_cutoff(&left_lattice, &right_lattice, score_cutoff)
}

#[pyfunction(signature = (left, right, *, score_cutoff = None))]
fn normalized_similarity(
    py: Python<'_>,
    left: &str,
    right: &str,
    score_cutoff: Option<f64>,
) -> PyResult<f64> {
    let left = left.to_owned();
    let right = right.to_owned();
    py.detach(move || {
        apply_similarity_cutoff(raw_normalized_similarity_pair(&left, &right), score_cutoff)
    })
}

#[pyfunction(signature = (left, right, *, score_cutoff = None))]
fn normalized_distance(
    py: Python<'_>,
    left: &str,
    right: &str,
    score_cutoff: Option<f64>,
) -> PyResult<f64> {
    let left = left.to_owned();
    let right = right.to_owned();
    py.detach(move || {
        apply_normalized_distance_cutoff(raw_normalized_distance_pair(&left, &right), score_cutoff)
    })
}

#[pyfunction(signature = (left, right, *, score_cutoff = None))]
fn ratio(py: Python<'_>, left: &str, right: &str, score_cutoff: Option<f64>) -> PyResult<f64> {
    normalized_similarity(py, left, right, score_cutoff)
}

#[pyfunction(signature = (query, text, max_span_chars, *, score_cutoff = None))]
fn _partial_distance_alignment(
    py: Python<'_>,
    query: &str,
    text: &str,
    max_span_chars: usize,
    score_cutoff: Option<usize>,
) -> PyPartialDistanceAlignment {
    let query = query.to_owned();
    let text = text.to_owned();
    py.detach(move || {
        let query_lattice = Lattice::from_paths([query.as_str()]);
        partial_distance_alignment(
            &query,
            &text,
            max_span_chars,
            score_cutoff,
            |span, cutoff| {
                let span_lattice = Lattice::from_paths([span]);
                Ok(Some(distance_with_cutoff(
                    &query_lattice,
                    &span_lattice,
                    cutoff,
                )))
            },
        )
        .map(|alignment| alignment.map(PartialDistanceAlignment::into_tuple))
    })
}

#[pyfunction(signature = (query, text, max_span_chars, *, score_cutoff = None))]
fn _partial_ratio_alignment(
    py: Python<'_>,
    query: &str,
    text: &str,
    max_span_chars: usize,
    score_cutoff: Option<f64>,
) -> PyPartialRatioAlignment {
    let query = query.to_owned();
    let text = text.to_owned();
    py.detach(move || {
        partial_ratio_alignment(
            &query,
            &text,
            max_span_chars,
            score_cutoff,
            |span, cutoff| {
                apply_similarity_cutoff(raw_normalized_similarity_pair(&query, span), cutoff)
                    .map(Some)
            },
        )
        .map(|alignment| alignment.map(PartialRatioAlignment::into_tuple))
    })
}

#[pyfunction(signature = (queries, choices, *, score_cutoff = None))]
fn _cdist_distance(
    py: Python<'_>,
    queries: Vec<String>,
    choices: Vec<String>,
    score_cutoff: Option<usize>,
) -> Vec<Vec<usize>> {
    py.detach(move || {
        let query_lattices = string_lattices(&queries);
        let choice_lattices = string_lattices(&choices);
        cdist_distance_matrix(&query_lattices, &choice_lattices, score_cutoff)
    })
}

#[pyfunction(signature = (queries, choices, *, score_cutoff = None))]
fn _cdist_damerau_distance(
    py: Python<'_>,
    queries: Vec<String>,
    choices: Vec<String>,
    score_cutoff: Option<usize>,
) -> Vec<Vec<usize>> {
    py.detach(move || {
        let query_lattices = string_lattices(&queries);
        let choice_lattices = string_lattices(&choices);
        cdist_damerau_distance_matrix(&query_lattices, &choice_lattices, score_cutoff)
    })
}

#[pyfunction(signature = (queries, choices, *, score_cutoff = None))]
fn _cdist_normalized_similarity(
    py: Python<'_>,
    queries: Vec<String>,
    choices: Vec<String>,
    score_cutoff: Option<f64>,
) -> PyResult<Vec<Vec<f64>>> {
    py.detach(move || {
        queries
            .iter()
            .map(|query| {
                choices
                    .iter()
                    .map(|choice| {
                        apply_similarity_cutoff(
                            raw_normalized_similarity_pair(query, choice),
                            score_cutoff,
                        )
                    })
                    .collect()
            })
            .collect()
    })
}

#[pyfunction(signature = (queries, choices, *, score_cutoff = None))]
fn _cdist_normalized_distance(
    py: Python<'_>,
    queries: Vec<String>,
    choices: Vec<String>,
    score_cutoff: Option<f64>,
) -> PyResult<Vec<Vec<f64>>> {
    py.detach(move || {
        queries
            .iter()
            .map(|query| {
                choices
                    .iter()
                    .map(|choice| {
                        apply_normalized_distance_cutoff(
                            raw_normalized_distance_pair(query, choice),
                            score_cutoff,
                        )
                    })
                    .collect()
            })
            .collect()
    })
}

#[pyfunction(signature = (left_paths, right_paths, *, score_cutoff = None))]
fn distance_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    score_cutoff: Option<usize>,
) -> PyResult<usize> {
    py.detach(move || {
        let left_lattice = lattice_from_paths(left_paths, "left_paths")?;
        let right_lattice = lattice_from_paths(right_paths, "right_paths")?;
        Ok(distance_with_cutoff(
            &left_lattice,
            &right_lattice,
            score_cutoff,
        ))
    })
}

#[pyfunction(signature = (left_paths, right_paths, *, score_cutoff = None))]
fn damerau_distance_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    score_cutoff: Option<usize>,
) -> PyResult<usize> {
    py.detach(move || {
        let left_lattice = lattice_from_paths(left_paths, "left_paths")?;
        let right_lattice = lattice_from_paths(right_paths, "right_paths")?;
        Ok(damerau_distance_with_cutoff(
            &left_lattice,
            &right_lattice,
            score_cutoff,
        ))
    })
}

#[pyfunction(signature = (left_paths, right_paths, *, score_cutoff = None))]
fn normalized_similarity_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    score_cutoff: Option<f64>,
) -> PyResult<f64> {
    py.detach(move || {
        validate_paths(&left_paths, "left_paths")?;
        validate_paths(&right_paths, "right_paths")?;
        apply_similarity_cutoff(
            max_normalized_similarity(&left_paths, &right_paths),
            score_cutoff,
        )
    })
}

#[pyfunction(signature = (left_paths, right_paths, *, score_cutoff = None))]
fn normalized_distance_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    score_cutoff: Option<f64>,
) -> PyResult<f64> {
    py.detach(move || {
        validate_paths(&left_paths, "left_paths")?;
        validate_paths(&right_paths, "right_paths")?;
        apply_normalized_distance_cutoff(
            1.0 - max_normalized_similarity(&left_paths, &right_paths),
            score_cutoff,
        )
    })
}

#[pyfunction(signature = (left_paths, right_paths, *, score_cutoff = None))]
fn ratio_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    score_cutoff: Option<f64>,
) -> PyResult<f64> {
    normalized_similarity_paths(py, left_paths, right_paths, score_cutoff)
}

#[pyfunction]
fn within_distance_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    threshold: usize,
) -> PyResult<bool> {
    py.detach(move || {
        let left_lattice = lattice_from_paths(left_paths, "left_paths")?;
        let right_lattice = lattice_from_paths(right_paths, "right_paths")?;
        Ok(lattice_within_distance(
            &left_lattice,
            &right_lattice,
            threshold,
        ))
    })
}

#[pyfunction]
fn within_damerau_distance_paths(
    py: Python<'_>,
    left_paths: Vec<String>,
    right_paths: Vec<String>,
    threshold: usize,
) -> PyResult<bool> {
    py.detach(move || {
        let left_lattice = lattice_from_paths(left_paths, "left_paths")?;
        let right_lattice = lattice_from_paths(right_paths, "right_paths")?;
        Ok(lattice_within_damerau_distance(
            &left_lattice,
            &right_lattice,
            threshold,
        ))
    })
}

#[pyclass(name = "JapaneseDictionary", frozen)]
struct PyJapaneseDictionary {
    index: UnidicReadingIndex,
    default_options: DictionaryReadingOptions,
}

#[pymethods]
impl PyJapaneseDictionary {
    #[staticmethod]
    #[pyo3(signature = (path, payload_format = "yaml"))]
    fn load_payload(path: &str, payload_format: &str) -> PyResult<Self> {
        let index = load_unidic_payload(Path::new(path), payload_format)?;
        Ok(Self {
            index,
            default_options: DictionaryReadingOptions::default(),
        })
    }

    #[staticmethod]
    #[pyo3(signature = (metadata_path, bundle_dir = None))]
    fn load_bundle(metadata_path: &str, bundle_dir: Option<&str>) -> PyResult<Self> {
        let metadata_input_path = PathBuf::from(metadata_path);
        let metadata_path = if metadata_input_path.is_dir() {
            metadata_input_path.join("metadata.yaml")
        } else {
            metadata_input_path
        };
        let metadata_yaml = fs::read_to_string(&metadata_path)
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        let metadata = serde_yaml::from_str::<UnidicArtifactMetadata>(&metadata_yaml)
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
            metadata_path
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf()
        });
        let payload_path = resolve_bundle_path(&bundle_dir, &metadata.payload.path)?;
        verify_metadata_file_digest(&metadata, &payload_path)?;
        let index = load_unidic_payload(&payload_path, &metadata.payload.format)?;
        verify_metadata_payload(&metadata, &index)?;
        let default_options = DictionaryReadingOptions {
            max_span_chars: metadata.query_defaults.max_span_chars,
            max_paths: metadata.query_defaults.max_paths,
            longest_match_only: metadata.query_defaults.longest_match_only,
            max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
        };

        Ok(Self {
            index,
            default_options,
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<usize> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = unidic_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = unidic_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(distance_with_cutoff(
                &left_lattice,
                &right_lattice,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn damerau_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<usize> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = unidic_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = unidic_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(damerau_distance_with_cutoff(
                &left_lattice,
                &right_lattice,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (left, right, threshold, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None))]
    fn within_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        threshold: usize,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
    ) -> PyResult<bool> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = unidic_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = unidic_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(lattice_within_distance(
                &left_lattice,
                &right_lattice,
                threshold,
            ))
        })
    }

    #[pyo3(signature = (left, right, threshold, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None))]
    fn within_damerau_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        threshold: usize,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
    ) -> PyResult<bool> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = unidic_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = unidic_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(lattice_within_damerau_distance(
                &left_lattice,
                &right_lattice,
                threshold,
            ))
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn normalized_similarity(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<f64> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let score =
                normalized_similarity_with_unidic_index(&left, &right, &self.index, options)
                    .map_err(|err| PyValueError::new_err(err.to_string()))?;
            apply_similarity_cutoff(score, score_cutoff)
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn normalized_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<f64> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let score =
                normalized_similarity_with_unidic_index(&left, &right, &self.index, options)
                    .map_err(|err| PyValueError::new_err(err.to_string()))?;
            apply_normalized_distance_cutoff(1.0 - score, score_cutoff)
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn ratio(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<f64> {
        self.normalized_similarity(
            py,
            left,
            right,
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
            score_cutoff,
        )
    }

    #[pyo3(signature = (query, text, max_span_chars, *, max_readings_per_segment = None, reading_max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _partial_distance_alignment(
        &self,
        py: Python<'_>,
        query: &str,
        text: &str,
        max_span_chars: usize,
        max_readings_per_segment: Option<usize>,
        reading_max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyPartialDistanceAlignment {
        let query = query.to_owned();
        let text = text.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            reading_max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = unidic_or_direct_romaji_paths(&query, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let span_limit = effective_partial_span_limit(&query, max_span_chars, &query_paths);
            let query_lattice = Lattice::from_paths(query_paths.iter().map(String::as_str));
            partial_distance_alignment(&query, &text, span_limit, score_cutoff, |span, cutoff| {
                let Some(span_lattice) =
                    optional_ja_lattice(unidic_or_direct_lattice(span, &self.index, options))?
                else {
                    return Ok(None);
                };
                Ok(Some(distance_with_cutoff(
                    &query_lattice,
                    &span_lattice,
                    cutoff,
                )))
            })
            .map(|alignment| alignment.map(PartialDistanceAlignment::into_tuple))
        })
    }

    #[pyo3(signature = (query, text, max_span_chars, *, max_readings_per_segment = None, reading_max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _partial_ratio_alignment(
        &self,
        py: Python<'_>,
        query: &str,
        text: &str,
        max_span_chars: usize,
        max_readings_per_segment: Option<usize>,
        reading_max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyPartialRatioAlignment {
        let query = query.to_owned();
        let text = text.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            reading_max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = unidic_or_direct_romaji_paths(&query, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let span_limit = effective_partial_span_limit(&query, max_span_chars, &query_paths);
            partial_ratio_alignment(&query, &text, span_limit, score_cutoff, |span, cutoff| {
                let Some(span_paths) =
                    optional_ja_paths(unidic_or_direct_romaji_paths(span, &self.index, options))?
                else {
                    return Ok(None);
                };
                apply_similarity_cutoff(
                    max_normalized_similarity(&query_paths, &span_paths),
                    cutoff,
                )
                .map(Some)
            })
            .map(|alignment| alignment.map(PartialRatioAlignment::into_tuple))
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_distance(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<Vec<Vec<usize>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_lattices = ja_lattices(&queries, &self.index, options)?;
            let choice_lattices = ja_lattices(&choices, &self.index, options)?;
            Ok(cdist_distance_matrix(
                &query_lattices,
                &choice_lattices,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_damerau_distance(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<Vec<Vec<usize>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_lattices = ja_lattices(&queries, &self.index, options)?;
            let choice_lattices = ja_lattices(&choices, &self.index, options)?;
            Ok(cdist_damerau_distance_matrix(
                &query_lattices,
                &choice_lattices,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_normalized_similarity(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<Vec<Vec<f64>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = ja_path_sets(&queries, &self.index, options)?;
            let choice_paths = ja_path_sets(&choices, &self.index, options)?;
            cdist_similarity_matrix(&query_paths, &choice_paths, score_cutoff)
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_normalized_distance(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<Vec<Vec<f64>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = ja_path_sets(&queries, &self.index, options)?;
            let choice_paths = ja_path_sets(&choices, &self.index, options)?;
            cdist_normalized_distance_matrix(&query_paths, &choice_paths, score_cutoff)
        })
    }
}

impl PyJapaneseDictionary {
    fn dictionary_options(
        &self,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
    ) -> DictionaryReadingOptions {
        let mut options = self.default_options;
        if let Some(max_readings_per_segment) = max_readings_per_segment {
            options.max_readings_per_segment = Some(max_readings_per_segment);
        }
        if let Some(max_span_chars) = max_span_chars {
            options.max_span_chars = max_span_chars;
        }
        if let Some(max_paths) = max_paths {
            options.max_paths = max_paths;
        }
        if let Some(longest_only) = longest_only {
            options.longest_match_only = longest_only;
        }
        options
    }
}

#[pyclass(name = "ChineseDictionary", frozen)]
struct PyChineseDictionary {
    index: ZhReadingIndex,
    default_options: PinyinReadingOptions,
}

#[pymethods]
impl PyChineseDictionary {
    #[staticmethod]
    #[pyo3(signature = (path, payload_format = "yaml"))]
    fn load_payload(path: &str, payload_format: &str) -> PyResult<Self> {
        let index = load_zh_payload(Path::new(path), payload_format)?;
        Ok(Self {
            index,
            default_options: PinyinReadingOptions::default(),
        })
    }

    #[staticmethod]
    #[pyo3(signature = (metadata_path, bundle_dir = None))]
    fn load_bundle(metadata_path: &str, bundle_dir: Option<&str>) -> PyResult<Self> {
        let metadata_input_path = PathBuf::from(metadata_path);
        let metadata_path = if metadata_input_path.is_dir() {
            metadata_input_path.join("metadata.yaml")
        } else {
            metadata_input_path
        };
        let metadata_yaml = fs::read_to_string(&metadata_path)
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        let metadata = serde_yaml::from_str::<ZhArtifactMetadata>(&metadata_yaml)
            .map_err(|err| PyValueError::new_err(err.to_string()))?;
        verify_zh_metadata_header(&metadata)?;
        let bundle_dir = bundle_dir.map(PathBuf::from).unwrap_or_else(|| {
            metadata_path
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf()
        });
        let payload_path = resolve_bundle_path(&bundle_dir, &metadata.payload.path)?;
        verify_zh_metadata_file_digest(&metadata, &payload_path)?;
        let index = load_zh_payload(&payload_path, &metadata.payload.format)?;
        verify_zh_metadata_payload(&metadata, &index)?;
        let default_options = PinyinReadingOptions {
            max_span_chars: metadata.query_defaults.max_span_chars,
            max_paths: metadata.query_defaults.max_paths,
            longest_match_only: metadata.query_defaults.longest_match_only,
            max_readings_per_segment: metadata.query_defaults.max_readings_per_segment,
        };

        Ok(Self {
            index,
            default_options,
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<usize> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = zh_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = zh_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(distance_with_cutoff(
                &left_lattice,
                &right_lattice,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn damerau_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<usize> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = zh_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = zh_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(damerau_distance_with_cutoff(
                &left_lattice,
                &right_lattice,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (left, right, threshold, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None))]
    fn within_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        threshold: usize,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
    ) -> PyResult<bool> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = zh_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = zh_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(lattice_within_distance(
                &left_lattice,
                &right_lattice,
                threshold,
            ))
        })
    }

    #[pyo3(signature = (left, right, threshold, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None))]
    fn within_damerau_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        threshold: usize,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
    ) -> PyResult<bool> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let left_lattice = zh_or_direct_lattice(&left, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let right_lattice = zh_or_direct_lattice(&right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            Ok(lattice_within_damerau_distance(
                &left_lattice,
                &right_lattice,
                threshold,
            ))
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn normalized_similarity(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<f64> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let score = normalized_similarity_with_zh_index(&left, &right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            apply_similarity_cutoff(score, score_cutoff)
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn normalized_distance(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<f64> {
        let left = left.to_owned();
        let right = right.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let score = normalized_similarity_with_zh_index(&left, &right, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            apply_normalized_distance_cutoff(1.0 - score, score_cutoff)
        })
    }

    #[pyo3(signature = (left, right, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn ratio(
        &self,
        py: Python<'_>,
        left: &str,
        right: &str,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<f64> {
        self.normalized_similarity(
            py,
            left,
            right,
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
            score_cutoff,
        )
    }

    #[pyo3(signature = (query, text, max_span_chars, *, max_readings_per_segment = None, reading_max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _partial_distance_alignment(
        &self,
        py: Python<'_>,
        query: &str,
        text: &str,
        max_span_chars: usize,
        max_readings_per_segment: Option<usize>,
        reading_max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyPartialDistanceAlignment {
        let query = query.to_owned();
        let text = text.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            reading_max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = zh_or_direct_pinyin_paths(&query, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let span_limit = effective_partial_span_limit(&query, max_span_chars, &query_paths);
            let query_lattice = Lattice::from_paths(query_paths.iter().map(String::as_str));
            partial_distance_alignment(&query, &text, span_limit, score_cutoff, |span, cutoff| {
                let Some(span_lattice) =
                    optional_zh_lattice(zh_or_direct_lattice(span, &self.index, options))?
                else {
                    return Ok(None);
                };
                Ok(Some(distance_with_cutoff(
                    &query_lattice,
                    &span_lattice,
                    cutoff,
                )))
            })
            .map(|alignment| alignment.map(PartialDistanceAlignment::into_tuple))
        })
    }

    #[pyo3(signature = (query, text, max_span_chars, *, max_readings_per_segment = None, reading_max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _partial_ratio_alignment(
        &self,
        py: Python<'_>,
        query: &str,
        text: &str,
        max_span_chars: usize,
        max_readings_per_segment: Option<usize>,
        reading_max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyPartialRatioAlignment {
        let query = query.to_owned();
        let text = text.to_owned();
        let options = self.dictionary_options(
            max_readings_per_segment,
            reading_max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = zh_or_direct_pinyin_paths(&query, &self.index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            let span_limit = effective_partial_span_limit(&query, max_span_chars, &query_paths);
            partial_ratio_alignment(&query, &text, span_limit, score_cutoff, |span, cutoff| {
                let Some(span_paths) =
                    optional_zh_paths(zh_or_direct_pinyin_paths(span, &self.index, options))?
                else {
                    return Ok(None);
                };
                apply_similarity_cutoff(
                    max_normalized_similarity(&query_paths, &span_paths),
                    cutoff,
                )
                .map(Some)
            })
            .map(|alignment| alignment.map(PartialRatioAlignment::into_tuple))
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_distance(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<Vec<Vec<usize>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_lattices = zh_lattices(&queries, &self.index, options)?;
            let choice_lattices = zh_lattices(&choices, &self.index, options)?;
            Ok(cdist_distance_matrix(
                &query_lattices,
                &choice_lattices,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_damerau_distance(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<usize>,
    ) -> PyResult<Vec<Vec<usize>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_lattices = zh_lattices(&queries, &self.index, options)?;
            let choice_lattices = zh_lattices(&choices, &self.index, options)?;
            Ok(cdist_damerau_distance_matrix(
                &query_lattices,
                &choice_lattices,
                score_cutoff,
            ))
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_normalized_similarity(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<Vec<Vec<f64>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = zh_path_sets(&queries, &self.index, options)?;
            let choice_paths = zh_path_sets(&choices, &self.index, options)?;
            cdist_similarity_matrix(&query_paths, &choice_paths, score_cutoff)
        })
    }

    #[pyo3(signature = (queries, choices, *, max_readings_per_segment = None, max_span_chars = None, max_paths = None, longest_only = None, score_cutoff = None))]
    fn _cdist_normalized_distance(
        &self,
        py: Python<'_>,
        queries: Vec<String>,
        choices: Vec<String>,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
        score_cutoff: Option<f64>,
    ) -> PyResult<Vec<Vec<f64>>> {
        let options = self.dictionary_options(
            max_readings_per_segment,
            max_span_chars,
            max_paths,
            longest_only,
        );
        py.detach(move || {
            let query_paths = zh_path_sets(&queries, &self.index, options)?;
            let choice_paths = zh_path_sets(&choices, &self.index, options)?;
            cdist_normalized_distance_matrix(&query_paths, &choice_paths, score_cutoff)
        })
    }
}

impl PyChineseDictionary {
    fn dictionary_options(
        &self,
        max_readings_per_segment: Option<usize>,
        max_span_chars: Option<usize>,
        max_paths: Option<usize>,
        longest_only: Option<bool>,
    ) -> PinyinReadingOptions {
        let mut options = self.default_options;
        if let Some(max_readings_per_segment) = max_readings_per_segment {
            options.max_readings_per_segment = Some(max_readings_per_segment);
        }
        if let Some(max_span_chars) = max_span_chars {
            options.max_span_chars = max_span_chars;
        }
        if let Some(max_paths) = max_paths {
            options.max_paths = max_paths;
        }
        if let Some(longest_only) = longest_only {
            options.longest_match_only = longest_only;
        }
        options
    }
}

fn ja_lattices(
    inputs: &[String],
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> PyResult<Vec<Lattice>> {
    inputs
        .iter()
        .map(|input| {
            unidic_or_direct_lattice(input, index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        })
        .collect()
}

fn ja_path_sets(
    inputs: &[String],
    index: &UnidicReadingIndex,
    options: DictionaryReadingOptions,
) -> PyResult<Vec<Vec<String>>> {
    inputs
        .iter()
        .map(|input| {
            unidic_or_direct_romaji_paths(input, index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        })
        .collect()
}

fn zh_lattices(
    inputs: &[String],
    index: &ZhReadingIndex,
    options: PinyinReadingOptions,
) -> PyResult<Vec<Lattice>> {
    inputs
        .iter()
        .map(|input| {
            zh_or_direct_lattice(input, index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        })
        .collect()
}

fn zh_path_sets(
    inputs: &[String],
    index: &ZhReadingIndex,
    options: PinyinReadingOptions,
) -> PyResult<Vec<Vec<String>>> {
    inputs
        .iter()
        .map(|input| {
            zh_or_direct_pinyin_paths(input, index, options)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        })
        .collect()
}

fn verify_metadata_file_digest(
    metadata: &UnidicArtifactMetadata,
    payload_path: &Path,
) -> PyResult<()> {
    match (
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
    ) {
        (None, None) => Ok(()),
        (Some(algorithm), Some(expected)) => {
            if algorithm != ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                return Err(PyValueError::new_err(format!(
                    "unsupported payload file digest algorithm {algorithm:?}"
                )));
            }
            let digest = artifact_file_digest_path(payload_path)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            if digest != expected {
                return Err(PyValueError::new_err(format!(
                    "payload file digest mismatch: metadata has {expected}, recomputed {digest}"
                )));
            }
            Ok(())
        }
        _ => Err(PyValueError::new_err(
            "payload file digest algorithm and digest must be provided together",
        )),
    }
}

fn verify_metadata_payload(
    metadata: &UnidicArtifactMetadata,
    index: &UnidicReadingIndex,
) -> PyResult<()> {
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "unsupported checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum != metadata.payload.checksum {
        return Err(PyValueError::new_err(format!(
            "payload checksum mismatch: metadata has {}, recomputed {}",
            metadata.payload.checksum, checksum
        )));
    }

    verify_metadata_entry_count(metadata, index)
}

fn verify_metadata_entry_count(
    metadata: &UnidicArtifactMetadata,
    index: &UnidicReadingIndex,
) -> PyResult<()> {
    if index.len() != metadata.build.entries {
        return Err(PyValueError::new_err(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries,
            index.len()
        )));
    }

    Ok(())
}

fn verify_zh_metadata_header(metadata: &ZhArtifactMetadata) -> PyResult<()> {
    if metadata.schema_version != 1 {
        return Err(PyValueError::new_err(format!(
            "unsupported zh metadata schema version {}",
            metadata.schema_version
        )));
    }
    if metadata.artifact_type != "moine.zh.reading-index" {
        return Err(PyValueError::new_err(format!(
            "unsupported zh artifact type {:?}",
            metadata.artifact_type
        )));
    }
    Ok(())
}

fn verify_zh_metadata_file_digest(
    metadata: &ZhArtifactMetadata,
    payload_path: &Path,
) -> PyResult<()> {
    match (
        metadata.payload.file_digest_algorithm.as_deref(),
        metadata.payload.file_digest.as_deref(),
    ) {
        (None, None) => Ok(()),
        (Some(algorithm), Some(expected)) => {
            if algorithm != ZH_ARTIFACT_PAYLOAD_FILE_DIGEST_ALGORITHM {
                return Err(PyValueError::new_err(format!(
                    "unsupported zh payload file digest algorithm {algorithm:?}"
                )));
            }
            let digest = zh_artifact_file_digest_path(payload_path)
                .map_err(|err| PyValueError::new_err(err.to_string()))?;
            if digest != expected {
                return Err(PyValueError::new_err(format!(
                    "payload file digest mismatch: metadata has {expected}, recomputed {digest}"
                )));
            }
            Ok(())
        }
        _ => Err(PyValueError::new_err(
            "zh payload file digest algorithm and digest must be provided together",
        )),
    }
}

fn verify_zh_metadata_payload(
    metadata: &ZhArtifactMetadata,
    index: &ZhReadingIndex,
) -> PyResult<()> {
    if index.len() != metadata.build.entries {
        return Err(PyValueError::new_err(format!(
            "entry count mismatch: metadata has {}, payload has {}",
            metadata.build.entries,
            index.len()
        )));
    }
    if index.pinyin_view().as_str() != metadata.build.pinyin_view {
        return Err(PyValueError::new_err(format!(
            "pinyin view mismatch: metadata has {}, payload has {}",
            metadata.build.pinyin_view,
            index.pinyin_view().as_str()
        )));
    }
    let checksum = index
        .artifact_payload_checksum_for_algorithm(&metadata.payload.checksum_algorithm)
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "unsupported checksum algorithm {:?}",
                metadata.payload.checksum_algorithm
            ))
        })?;
    if checksum != metadata.payload.checksum {
        return Err(PyValueError::new_err(format!(
            "payload checksum mismatch: metadata has {}, recomputed {}",
            metadata.payload.checksum, checksum
        )));
    }
    Ok(())
}

fn resolve_bundle_path(bundle_dir: &Path, relative_path: &str) -> PyResult<PathBuf> {
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
        return Err(PyValueError::new_err(format!(
            "bundle path {relative_path:?} must be relative and stay inside the bundle"
        )));
    }
    Ok(bundle_dir.join(relative))
}

fn load_unidic_payload(path: &Path, payload_format: &str) -> PyResult<UnidicReadingIndex> {
    match payload_format {
        "yaml" | YAML_PAYLOAD_FORMAT => UnidicReadingIndex::from_artifact_payload_path(path)
            .map_err(|err| PyValueError::new_err(err.to_string())),
        "binary" | BINARY_PAYLOAD_FORMAT => {
            UnidicReadingIndex::from_binary_artifact_payload_path(path)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        }
        "indexed" | "fst" | INDEXED_PAYLOAD_FORMAT => {
            UnidicReadingIndex::from_indexed_artifact_payload_path(path)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        }
        _ => Err(PyValueError::new_err(format!(
            "unsupported payload_format {payload_format:?}; expected 'yaml', 'binary', or 'indexed'"
        ))),
    }
}

fn load_zh_payload(path: &Path, payload_format: &str) -> PyResult<ZhReadingIndex> {
    match payload_format {
        "yaml" | YAML_PAYLOAD_FORMAT => ZhReadingIndex::from_artifact_payload_path(path)
            .map_err(|err| PyValueError::new_err(err.to_string())),
        "indexed" | "fst" | INDEXED_PAYLOAD_FORMAT => {
            ZhReadingIndex::from_indexed_artifact_payload_path(path)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        }
        _ => Err(PyValueError::new_err(format!(
            "unsupported payload_format {payload_format:?}; expected 'yaml' or 'indexed'"
        ))),
    }
}

fn lattice_from_paths(paths: Vec<String>, argument_name: &'static str) -> PyResult<Lattice> {
    validate_paths(&paths, argument_name)?;
    if paths.iter().all(String::is_empty) {
        return Ok(Lattice::from_paths([""]));
    }
    Ok(Lattice::from_paths(paths))
}

fn validate_paths(paths: &[String], argument_name: &'static str) -> PyResult<()> {
    if paths.is_empty() {
        return Err(PyValueError::new_err(format!(
            "{argument_name} must contain at least one path"
        )));
    }
    let has_empty = paths.iter().any(String::is_empty);
    let has_non_empty = paths.iter().any(|path| !path.is_empty());
    if has_empty && has_non_empty {
        return Err(PyValueError::new_err(format!(
            "{argument_name} cannot mix empty and non-empty paths"
        )));
    }
    Ok(())
}

fn max_normalized_similarity(left_paths: &[String], right_paths: &[String]) -> f64 {
    left_paths
        .iter()
        .flat_map(|left| {
            right_paths
                .iter()
                .map(move |right| raw_normalized_similarity_pair(left, right))
        })
        .fold(0.0, f64::max)
}

fn effective_partial_span_limit(
    query: &str,
    max_span_chars: usize,
    query_paths: &[String],
) -> usize {
    if max_span_chars != 0 {
        return max_span_chars;
    }
    let query_len = query.chars().count();
    let default_limit = default_partial_span_limit(query_len);
    let path_limit = query_paths
        .iter()
        .map(|path| path.chars().count())
        .max()
        .unwrap_or(query_len);
    default_limit.max(path_limit)
}

fn default_partial_span_limit(query_len: usize) -> usize {
    if query_len == 0 {
        0
    } else {
        (query_len * 2).max(query_len + 4)
    }
}

fn optional_ja_lattice(result: Result<Lattice, JaLatticeError>) -> PyResult<Option<Lattice>> {
    match result {
        Ok(lattice) => Ok(Some(lattice)),
        Err(JaLatticeError::UnsupportedChar { .. }) => Ok(None),
        Err(err) => Err(PyValueError::new_err(err.to_string())),
    }
}

fn optional_ja_paths(result: Result<Vec<String>, JaLatticeError>) -> PyResult<Option<Vec<String>>> {
    match result {
        Ok(paths) => Ok(Some(paths)),
        Err(JaLatticeError::UnsupportedChar { .. }) => Ok(None),
        Err(err) => Err(PyValueError::new_err(err.to_string())),
    }
}

fn optional_zh_lattice(result: Result<Lattice, CnLatticeError>) -> PyResult<Option<Lattice>> {
    match result {
        Ok(lattice) => Ok(Some(lattice)),
        Err(CnLatticeError::UnsupportedDirectInput { .. }) => Ok(None),
        Err(err) => Err(PyValueError::new_err(err.to_string())),
    }
}

fn optional_zh_paths(result: Result<Vec<String>, CnLatticeError>) -> PyResult<Option<Vec<String>>> {
    match result {
        Ok(paths) => Ok(Some(paths)),
        Err(CnLatticeError::UnsupportedDirectInput { .. }) => Ok(None),
        Err(err) => Err(PyValueError::new_err(err.to_string())),
    }
}

#[derive(Clone, Copy)]
struct PartialDistanceAlignment {
    score: usize,
    src_start: usize,
    src_end: usize,
    dest_start: usize,
    dest_end: usize,
}

impl PartialDistanceAlignment {
    fn into_tuple(self) -> PartialDistanceAlignmentTuple {
        (
            self.score,
            self.src_start,
            self.src_end,
            self.dest_start,
            self.dest_end,
        )
    }
}

#[derive(Clone, Copy)]
struct PartialRatioAlignment {
    score: f64,
    src_start: usize,
    src_end: usize,
    dest_start: usize,
    dest_end: usize,
}

impl PartialRatioAlignment {
    fn into_tuple(self) -> PartialRatioAlignmentTuple {
        (
            self.score,
            self.src_start,
            self.src_end,
            self.dest_start,
            self.dest_end,
        )
    }
}

#[derive(Clone, Copy)]
struct TextSpan<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

fn partial_distance_alignment<F>(
    query: &str,
    text: &str,
    max_span_chars: usize,
    score_cutoff: Option<usize>,
    mut scorer: F,
) -> PyResult<Option<PartialDistanceAlignment>>
where
    F: FnMut(&str, Option<usize>) -> PyResult<Option<usize>>,
{
    let query_len = query.chars().count();
    let mut best = None;
    for span in text_spans(text, max_span_chars, query.is_empty()) {
        let Some(score) = scorer(span.text, score_cutoff)? else {
            continue;
        };
        if matches!(score_cutoff, Some(cutoff) if score > cutoff) {
            continue;
        }
        let candidate = PartialDistanceAlignment {
            score,
            src_start: 0,
            src_end: query_len,
            dest_start: span.start,
            dest_end: span.end,
        };
        if match best {
            Some(current) => distance_alignment_key(candidate) < distance_alignment_key(current),
            None => true,
        } {
            best = Some(candidate);
        }
    }
    Ok(best)
}

fn partial_ratio_alignment<F>(
    query: &str,
    text: &str,
    max_span_chars: usize,
    score_cutoff: Option<f64>,
    mut scorer: F,
) -> PyResult<Option<PartialRatioAlignment>>
where
    F: FnMut(&str, Option<f64>) -> PyResult<Option<f64>>,
{
    let query_len = query.chars().count();
    let mut best = None;
    for span in text_spans(text, max_span_chars, query.is_empty()) {
        let Some(score) = scorer(span.text, score_cutoff)? else {
            continue;
        };
        if matches!(score_cutoff, Some(cutoff) if score < cutoff) {
            continue;
        }
        let candidate = PartialRatioAlignment {
            score,
            src_start: 0,
            src_end: query_len,
            dest_start: span.start,
            dest_end: span.end,
        };
        if match best {
            Some(current) => ratio_alignment_is_better(candidate, current),
            None => true,
        } {
            best = Some(candidate);
        }
    }
    Ok(best)
}

struct TextSpanIter<'a> {
    text: &'a str,
    boundaries: Vec<usize>,
    char_len: usize,
    max_span_chars: usize,
    emit_empty: bool,
    yielded_empty: bool,
    start: usize,
    end: usize,
}

impl<'a> TextSpanIter<'a> {
    fn new(text: &'a str, max_span_chars: usize, include_empty: bool) -> Self {
        let boundaries = char_boundaries(text);
        let char_len = boundaries.len() - 1;
        Self {
            text,
            boundaries,
            char_len,
            max_span_chars,
            emit_empty: char_len == 0 || include_empty || max_span_chars == 0,
            yielded_empty: false,
            start: 0,
            end: 1,
        }
    }
}

impl<'a> Iterator for TextSpanIter<'a> {
    type Item = TextSpan<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.emit_empty {
            if self.yielded_empty {
                return None;
            }
            self.yielded_empty = true;
            return Some(TextSpan {
                start: 0,
                end: 0,
                text: "",
            });
        }

        if self.start >= self.char_len {
            return None;
        }

        let mut end_limit = self.char_len.min(self.start + self.max_span_chars);
        while self.end > end_limit {
            self.start += 1;
            if self.start >= self.char_len {
                return None;
            }
            self.end = self.start + 1;
            end_limit = self.char_len.min(self.start + self.max_span_chars);
        }

        let span = TextSpan {
            start: self.start,
            end: self.end,
            text: &self.text[self.boundaries[self.start]..self.boundaries[self.end]],
        };
        self.end += 1;
        Some(span)
    }
}

fn text_spans(text: &str, max_span_chars: usize, include_empty: bool) -> TextSpanIter<'_> {
    TextSpanIter::new(text, max_span_chars, include_empty)
}

fn string_lattices(strings: &[String]) -> Vec<Lattice> {
    strings
        .iter()
        .map(|value| Lattice::from_paths([value.as_str()]))
        .collect()
}

fn char_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(text.chars().count() + 1);
    boundaries.push(0);
    boundaries.extend(text.char_indices().skip(1).map(|(index, _)| index));
    if !text.is_empty() {
        boundaries.push(text.len());
    }
    boundaries
}

fn distance_alignment_key(alignment: PartialDistanceAlignment) -> (usize, usize, usize) {
    (
        alignment.score,
        alignment.dest_end - alignment.dest_start,
        alignment.dest_start,
    )
}

fn ratio_alignment_is_better(
    candidate: PartialRatioAlignment,
    current: PartialRatioAlignment,
) -> bool {
    candidate.score > current.score
        || (candidate.score == current.score
            && (
                candidate.dest_end - candidate.dest_start,
                candidate.dest_start,
            ) < (current.dest_end - current.dest_start, current.dest_start))
}

fn cdist_distance_matrix(
    query_lattices: &[Lattice],
    choice_lattices: &[Lattice],
    score_cutoff: Option<usize>,
) -> Vec<Vec<usize>> {
    query_lattices
        .iter()
        .map(|query| {
            choice_lattices
                .iter()
                .map(|choice| distance_with_cutoff(query, choice, score_cutoff))
                .collect()
        })
        .collect()
}

fn cdist_damerau_distance_matrix(
    query_lattices: &[Lattice],
    choice_lattices: &[Lattice],
    score_cutoff: Option<usize>,
) -> Vec<Vec<usize>> {
    query_lattices
        .iter()
        .map(|query| {
            choice_lattices
                .iter()
                .map(|choice| damerau_distance_with_cutoff(query, choice, score_cutoff))
                .collect()
        })
        .collect()
}

fn cdist_similarity_matrix(
    query_paths: &[Vec<String>],
    choice_paths: &[Vec<String>],
    score_cutoff: Option<f64>,
) -> PyResult<Vec<Vec<f64>>> {
    query_paths
        .iter()
        .map(|query| {
            choice_paths
                .iter()
                .map(|choice| {
                    apply_similarity_cutoff(max_normalized_similarity(query, choice), score_cutoff)
                })
                .collect()
        })
        .collect()
}

fn cdist_normalized_distance_matrix(
    query_paths: &[Vec<String>],
    choice_paths: &[Vec<String>],
    score_cutoff: Option<f64>,
) -> PyResult<Vec<Vec<f64>>> {
    query_paths
        .iter()
        .map(|query| {
            choice_paths
                .iter()
                .map(|choice| {
                    apply_normalized_distance_cutoff(
                        1.0 - max_normalized_similarity(query, choice),
                        score_cutoff,
                    )
                })
                .collect()
        })
        .collect()
}

fn raw_normalized_similarity_pair(left: &str, right: &str) -> f64 {
    normalized_similarity_str(left, right)
}

fn raw_normalized_distance_pair(left: &str, right: &str) -> f64 {
    1.0 - raw_normalized_similarity_pair(left, right)
}

fn distance_with_cutoff(
    left_lattice: &Lattice,
    right_lattice: &Lattice,
    score_cutoff: Option<usize>,
) -> usize {
    match score_cutoff {
        Some(cutoff) if !lattice_within_distance(left_lattice, right_lattice, cutoff) => cutoff + 1,
        _ => lattice_distance(left_lattice, right_lattice),
    }
}

fn damerau_distance_with_cutoff(
    left_lattice: &Lattice,
    right_lattice: &Lattice,
    score_cutoff: Option<usize>,
) -> usize {
    match score_cutoff {
        Some(cutoff) if !lattice_within_damerau_distance(left_lattice, right_lattice, cutoff) => {
            cutoff + 1
        }
        _ => lattice_damerau_distance(left_lattice, right_lattice),
    }
}

fn apply_similarity_cutoff(score: f64, score_cutoff: Option<f64>) -> PyResult<f64> {
    match score_cutoff {
        Some(cutoff) => {
            if !(0.0..=1.0).contains(&cutoff) {
                return Err(PyValueError::new_err(
                    "score_cutoff must be between 0.0 and 1.0",
                ));
            }
            if score < cutoff {
                Ok(0.0)
            } else {
                Ok(score)
            }
        }
        None => Ok(score),
    }
}

fn apply_normalized_distance_cutoff(distance: f64, score_cutoff: Option<f64>) -> PyResult<f64> {
    match score_cutoff {
        Some(cutoff) => {
            if !(0.0..=1.0).contains(&cutoff) {
                return Err(PyValueError::new_err(
                    "score_cutoff must be between 0.0 and 1.0",
                ));
            }
            if distance > cutoff {
                Ok(1.0)
            } else {
                Ok(distance)
            }
        }
        None => Ok(distance),
    }
}

#[pymodule]
fn _moine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(distance, m)?)?;
    m.add_function(wrap_pyfunction!(damerau_distance, m)?)?;
    m.add_function(wrap_pyfunction!(normalized_distance, m)?)?;
    m.add_function(wrap_pyfunction!(normalized_similarity, m)?)?;
    m.add_function(wrap_pyfunction!(ratio, m)?)?;
    m.add_function(wrap_pyfunction!(_partial_distance_alignment, m)?)?;
    m.add_function(wrap_pyfunction!(_partial_ratio_alignment, m)?)?;
    m.add_function(wrap_pyfunction!(_cdist_distance, m)?)?;
    m.add_function(wrap_pyfunction!(_cdist_damerau_distance, m)?)?;
    m.add_function(wrap_pyfunction!(_cdist_normalized_distance, m)?)?;
    m.add_function(wrap_pyfunction!(_cdist_normalized_similarity, m)?)?;
    m.add_function(wrap_pyfunction!(distance_paths, m)?)?;
    m.add_function(wrap_pyfunction!(damerau_distance_paths, m)?)?;
    m.add_function(wrap_pyfunction!(normalized_distance_paths, m)?)?;
    m.add_function(wrap_pyfunction!(normalized_similarity_paths, m)?)?;
    m.add_function(wrap_pyfunction!(ratio_paths, m)?)?;
    m.add_function(wrap_pyfunction!(within_distance_paths, m)?)?;
    m.add_function(wrap_pyfunction!(within_damerau_distance_paths, m)?)?;
    m.add_class::<PyJapaneseDictionary>()?;
    m.add_class::<PyChineseDictionary>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_distance_matches_linear_lattice_distance() {
        Python::initialize();
        Python::attach(|py| {
            assert_eq!(distance(py, "abc", "adc", None), 1);
            assert_eq!(distance(py, "abc", "adc", Some(0)), 1);
            assert_eq!(damerau_distance(py, "abc", "acb", None), 1);
            assert_eq!(damerau_distance(py, "abc", "acb", Some(0)), 1);
            assert_close(
                normalized_similarity(py, "abc", "adc", None).unwrap(),
                2.0 / 3.0,
            );
            assert_close(
                normalized_distance(py, "abc", "adc", None).unwrap(),
                1.0 / 3.0,
            );
            assert_eq!(
                normalized_distance(py, "abc", "adc", Some(0.2)).unwrap(),
                1.0
            );
            assert_eq!(
                normalized_similarity(py, "abc", "adc", Some(0.8)).unwrap(),
                0.0
            );
            assert_close(ratio(py, "abc", "adc", Some(0.5)).unwrap(), 2.0 / 3.0);
            assert!(ratio(py, "abc", "adc", Some(1.5)).is_err());
        });
    }

    #[test]
    fn python_distance_paths_takes_minimum_path_distance() {
        Python::initialize();
        Python::attach(|py| {
            let distance = distance_paths(
                py,
                vec!["insatu".to_string(), "innsatu".to_string()],
                vec!["insat".to_string()],
                None,
            )
            .unwrap();

            assert_eq!(distance, 1);
            assert_eq!(
                damerau_distance_paths(
                    py,
                    vec!["abc".to_string(), "axc".to_string()],
                    vec!["acb".to_string()],
                    None,
                )
                .unwrap(),
                1
            );
            assert!(within_damerau_distance_paths(
                py,
                vec!["abc".to_string(), "axc".to_string()],
                vec!["acb".to_string()],
                1,
            )
            .unwrap());
            assert_eq!(
                distance_paths(
                    py,
                    vec!["insatu".to_string(), "innsatu".to_string()],
                    vec!["insat".to_string()],
                    Some(0),
                )
                .unwrap(),
                1
            );
            assert_eq!(
                normalized_distance_paths(
                    py,
                    vec!["abc".to_string(), "abcd".to_string()],
                    vec!["abxd".to_string()],
                    None,
                )
                .unwrap(),
                0.25
            );
            assert_eq!(
                normalized_similarity_paths(
                    py,
                    vec!["abc".to_string(), "abcd".to_string()],
                    vec!["abxd".to_string()],
                    None,
                )
                .unwrap(),
                0.75
            );
            assert_eq!(
                normalized_similarity_paths(
                    py,
                    vec!["abc".to_string(), "abcd".to_string()],
                    vec!["abxd".to_string()],
                    Some(0.8),
                )
                .unwrap(),
                0.0
            );
            assert_eq!(
                ratio_paths(
                    py,
                    vec!["abc".to_string(), "abcd".to_string()],
                    vec!["abxd".to_string()],
                    Some(0.7),
                )
                .unwrap(),
                0.75
            );
            assert!(within_distance_paths(
                py,
                vec!["insatu".to_string(), "innsatu".to_string()],
                vec!["insat".to_string()],
                1
            )
            .unwrap());
        });
    }

    #[test]
    fn python_distance_paths_rejects_empty_path_list() {
        Python::initialize();
        Python::attach(|py| {
            let err = distance_paths(py, Vec::new(), vec!["a".to_string()], None).unwrap_err();

            assert!(err.to_string().contains("left_paths"));
            let empty_distance = distance_paths(
                py,
                vec!["".to_string(), "".to_string()],
                vec!["".to_string()],
                None,
            )
            .unwrap();
            assert_eq!(empty_distance, 0);
            assert_eq!(
                normalized_similarity_paths(py, vec!["".to_string()], vec!["".to_string()], None)
                    .unwrap(),
                1.0
            );
        });
    }

    #[test]
    fn python_japanese_dictionary_loads_binary_payload() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let path =
            std::env::temp_dir().join(format!("moine-python-test-{}.moinebin", std::process::id()));
        {
            let file = std::fs::File::create(&path).unwrap();
            index.write_artifact_binary_payload(file).unwrap();
        }

        let dictionary =
            PyJapaneseDictionary::load_payload(path.to_str().unwrap(), "binary").unwrap();
        Python::initialize();
        Python::attach(|py| {
            let distance = dictionary
                .distance(
                    py,
                    "いんさt",
                    "印刷",
                    Some(16),
                    None,
                    Some(128),
                    Some(true),
                    None,
                )
                .unwrap();
            let cutoff_distance = dictionary
                .distance(
                    py,
                    "いんさt",
                    "印刷",
                    Some(16),
                    None,
                    Some(128),
                    Some(true),
                    Some(0),
                )
                .unwrap();

            assert_eq!(distance, 1);
            assert_eq!(cutoff_distance, 1);
            assert!(dictionary
                .within_distance(
                    py,
                    "いんさt",
                    "印刷",
                    1,
                    Some(16),
                    None,
                    Some(128),
                    Some(true)
                )
                .unwrap());
        });

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn python_japanese_dictionary_loads_indexed_payload() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let path =
            std::env::temp_dir().join(format!("moine-python-test-{}.moineidx", std::process::id()));
        {
            let file = std::fs::File::create(&path).unwrap();
            index.write_indexed_artifact_payload(file).unwrap();
        }

        let dictionary =
            PyJapaneseDictionary::load_payload(path.to_str().unwrap(), "indexed").unwrap();
        Python::initialize();
        Python::attach(|py| {
            let distance = dictionary
                .distance(
                    py,
                    "いんさt",
                    "印刷",
                    Some(16),
                    None,
                    Some(128),
                    Some(true),
                    None,
                )
                .unwrap();

            assert_eq!(distance, 1);
        });

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn python_japanese_dictionary_loads_bundle_defaults() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let bundle_dir =
            std::env::temp_dir().join(format!("moine-python-bundle-test-{}", std::process::id()));
        std::fs::create_dir_all(&bundle_dir).unwrap();
        let payload_path = bundle_dir.join("readings.moinebin");
        let metadata_path = bundle_dir.join("metadata.yaml");
        {
            let file = std::fs::File::create(&payload_path).unwrap();
            index.write_artifact_binary_payload(file).unwrap();
        }
        let file_digest = artifact_file_digest_path(&payload_path).unwrap();
        let checksum = index.artifact_payload_checksum();
        let metadata_yaml = format!(
            "\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: test
payload:
  path: readings.moinebin
  format: binary.surface-readings.v1
  file_digest_algorithm: sha256-file-v1
  file_digest: {file_digest}
  checksum_algorithm: sha256-canonical-v1
  checksum: {checksum}
source:
  name: UniDic-CWJ
  version: test
  lex_csv: test.csv
build:
  reading_field: lform
  max_readings_per_surface: null
  exclude_ascii_surfaces: true
  exclude_symbol_pos: true
  entries: 1
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references: []
"
        );
        std::fs::write(&metadata_path, metadata_yaml).unwrap();

        let dictionary = PyJapaneseDictionary::load_bundle(metadata_path.to_str().unwrap(), None)
            .expect("should load bundle");
        let directory_dictionary =
            PyJapaneseDictionary::load_bundle(bundle_dir.to_str().unwrap(), None)
                .expect("should load bundle directory");
        Python::initialize();
        Python::attach(|py| {
            let distance = dictionary
                .distance(py, "いんさt", "印刷", None, None, None, None, None)
                .unwrap();
            let directory_distance = directory_dictionary
                .distance(py, "いんさt", "印刷", None, None, None, None, None)
                .unwrap();
            let cutoff_similarity = dictionary
                .normalized_similarity(py, "いんさt", "印刷", None, None, None, None, Some(0.9))
                .unwrap();
            let normalized_distance = dictionary
                .normalized_distance(py, "いんさt", "印刷", None, None, None, None, None)
                .unwrap();
            let cutoff_normalized_distance = dictionary
                .normalized_distance(py, "いんさt", "印刷", None, None, None, None, Some(0.1))
                .unwrap();

            assert_eq!(distance, 1);
            assert_eq!(directory_distance, 1);
            assert_eq!(cutoff_similarity, 0.0);
            assert_close(normalized_distance, 1.0 / 7.0);
            assert_eq!(cutoff_normalized_distance, 1.0);
            assert!(dictionary
                .within_distance(py, "いんさt", "印刷", 1, None, None, None, None)
                .unwrap());
        });

        std::fs::remove_dir_all(bundle_dir).unwrap();
    }

    #[test]
    fn python_japanese_dictionary_rejects_bundle_file_digest_mismatch() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let bundle_dir = std::env::temp_dir().join(format!(
            "moine-python-bad-file-digest-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&bundle_dir).unwrap();
        let payload_path = bundle_dir.join("readings.moinebin");
        let metadata_path = bundle_dir.join("metadata.yaml");
        {
            let file = std::fs::File::create(&payload_path).unwrap();
            index.write_artifact_binary_payload(file).unwrap();
        }
        let checksum = index.artifact_payload_checksum();
        let metadata_yaml = format!(
            "\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: test
payload:
  path: readings.moinebin
  format: binary.surface-readings.v1
  file_digest_algorithm: sha256-file-v1
  file_digest: 0000000000000000000000000000000000000000000000000000000000000000
  checksum_algorithm: sha256-canonical-v1
  checksum: {checksum}
source:
  name: UniDic-CWJ
  version: test
  lex_csv: test.csv
build:
  reading_field: lform
  max_readings_per_surface: null
  exclude_ascii_surfaces: true
  exclude_symbol_pos: true
  entries: 1
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references: []
"
        );
        std::fs::write(&metadata_path, metadata_yaml).unwrap();

        let err = match PyJapaneseDictionary::load_bundle(metadata_path.to_str().unwrap(), None) {
            Ok(_) => panic!("should reject file digest mismatch"),
            Err(err) => err,
        };

        std::fs::remove_dir_all(bundle_dir).unwrap();
        assert!(err.to_string().contains("payload file digest mismatch"));
    }

    #[test]
    fn python_japanese_dictionary_rejects_bundle_checksum_mismatch() {
        let csv = "\
印刷,1,2,3,名詞,普通名詞,サ変可能,*,*,*,インサツ,印刷,印刷,インサツ,印刷,インサツ,漢
";
        let index = UnidicReadingIndex::from_lex_csv_reader(csv.as_bytes()).unwrap();
        let bundle_dir = std::env::temp_dir().join(format!(
            "moine-python-bad-bundle-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&bundle_dir).unwrap();
        let payload_path = bundle_dir.join("readings.moinebin");
        let metadata_path = bundle_dir.join("metadata.yaml");
        {
            let file = std::fs::File::create(&payload_path).unwrap();
            index.write_artifact_binary_payload(file).unwrap();
        }
        let metadata_yaml = "\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: test
payload:
  path: readings.moinebin
  format: binary.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: 0000000000000000000000000000000000000000000000000000000000000000
source:
  name: UniDic-CWJ
  version: test
  lex_csv: test.csv
build:
  reading_field: lform
  max_readings_per_surface: null
  exclude_ascii_surfaces: true
  exclude_symbol_pos: true
  entries: 1
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references: []
";
        std::fs::write(&metadata_path, metadata_yaml).unwrap();

        let err = match PyJapaneseDictionary::load_bundle(metadata_path.to_str().unwrap(), None) {
            Ok(_) => panic!("should reject checksum mismatch"),
            Err(err) => err,
        };

        std::fs::remove_dir_all(bundle_dir).unwrap();
        assert!(err.to_string().contains("payload checksum mismatch"));
    }

    #[test]
    fn python_japanese_dictionary_rejects_bundle_path_escape() {
        let bundle_dir = std::env::temp_dir().join(format!(
            "moine-python-path-escape-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&bundle_dir).unwrap();
        let metadata_path = bundle_dir.join("metadata.yaml");
        let metadata_yaml = "\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: test
payload:
  path: ../readings.moinebin
  format: binary.surface-readings.v1
  checksum_algorithm: sha256-canonical-v1
  checksum: 0000000000000000000000000000000000000000000000000000000000000000
source:
  name: UniDic-CWJ
  version: test
  lex_csv: test.csv
build:
  reading_field: lform
  max_readings_per_surface: null
  exclude_ascii_surfaces: true
  exclude_symbol_pos: true
  entries: 1
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references: []
";
        std::fs::write(&metadata_path, metadata_yaml).unwrap();

        let err = match PyJapaneseDictionary::load_bundle(metadata_path.to_str().unwrap(), None) {
            Ok(_) => panic!("should reject bundle path escape"),
            Err(err) => err,
        };

        std::fs::remove_dir_all(bundle_dir).unwrap();
        assert!(err.to_string().contains("stay inside the bundle"));
    }

    #[test]
    fn python_bundle_path_rejects_backslash_separators() {
        Python::initialize();
        let err = resolve_bundle_path(Path::new("bundle"), r"license\BSD").unwrap_err();

        assert!(err.to_string().contains("stay inside the bundle"));
    }

    fn assert_close(left: f64, right: f64) {
        assert!((left - right).abs() < f64::EPSILON);
    }
}
