import hashlib

import moine
import pytest
from moine.zh import (
    Dictionary,
    damerau_distance,
    distance,
    extract,
    extract_one,
    load_bundle,
    normalized_distance,
    normalized_similarity,
    process,
    ratio,
    within_distance,
)


def push_len_prefixed(data, tag, value):
    encoded = value.encode("utf-8")
    data.extend(tag)
    data.extend(str(len(encoded)).encode("ascii"))
    data.append(0x0A)
    data.extend(encoded)
    data.append(0x0A)


def payload_checksum(entries, pinyin_view="no-tone"):
    data = bytearray(b"moine.zh.reading-index.surface-readings/v1\n")
    push_len_prefixed(data, b"V", pinyin_view)
    for surface, readings in sorted(entries):
        push_len_prefixed(data, b"S", surface)
        data.extend(f"R{len(readings)}\n".encode("ascii"))
        for reading in readings:
            push_len_prefixed(data, b"r", reading)
    return hashlib.sha256(data).hexdigest()


def write_test_bundle(tmp_path, file_digest=None):
    entries = [
        ("威士忌", ["weishiji"]),
        ("布納哈本", ["bunahaben"]),
        ("布那哈本", ["bunahaben"]),
        ("布呐哈本", ["bunahaben"]),
        ("布納哈奔", ["bunahaben"]),
    ]
    payload_path = tmp_path / "readings.yaml"
    payload_path.write_text(
        """\
schema_version: 1
payload_type: moine.zh.reading-index.surface-readings
pinyin_view: no-tone
entries:
- surface: 威士忌
  readings:
  - weishiji
- surface: 布納哈本
  readings:
  - bunahaben
- surface: 布那哈本
  readings:
  - bunahaben
- surface: 布呐哈本
  readings:
  - bunahaben
- surface: 布納哈奔
  readings:
  - bunahaben
""",
        encoding="utf-8",
    )
    checksum = payload_checksum(entries)
    if file_digest is None:
        file_digest = hashlib.sha256(payload_path.read_bytes()).hexdigest()

    metadata_path = tmp_path / "metadata.yaml"
    metadata_path.write_text(
        f"""\
schema_version: 1
artifact_type: moine.zh.reading-index
artifact_name: test
generator: pytest
payload:
  path: readings.yaml
  format: yaml.surface-readings.v1
  file_digest_algorithm: sha256-file-v1
  file_digest: {file_digest}
  checksum_algorithm: sha256-canonical-v1
  checksum: {checksum}
source:
  name: CC-CEDICT
  version: test
  cedict: cedict.txt
build:
  pinyin_view: no-tone
  max_readings_per_surface: null
  entries: 5
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: null
license:
  selected_license: CC BY-SA 4.0
  references: []
""",
        encoding="utf-8",
    )
    return metadata_path, payload_path


def test_zh_bundle_helpers_use_metadata_defaults(tmp_path):
    metadata_path, payload_path = write_test_bundle(tmp_path)

    dictionary = load_bundle(metadata_path)
    directory_dictionary = load_bundle(tmp_path)
    payload_dictionary = Dictionary.load_payload(str(payload_path))

    assert isinstance(dictionary, Dictionary)
    assert dictionary.distance("weishiji", "威士忌") == 0
    assert directory_dictionary.distance("布那哈本", "布納哈本") == 0
    assert payload_dictionary.distance("布呐哈本", "布納哈本", longest_only=True) == 0
    assert dictionary.distance("bunahabe", "布納哈本") == 1
    assert dictionary.distance("bunahabe", "布納哈本", score_cutoff=0) == 1
    assert dictionary.damerau_distance("weishiji", "wieshiji") == 1
    assert dictionary.damerau_distance("weishiji", "wieshiji", score_cutoff=0) == 1
    assert dictionary.within_distance("布納哈奔", "布納哈本", 0)
    assert not dictionary.within_distance("bunahabe", "布納哈本", 0)
    assert dictionary.within_damerau_distance("weishiji", "wieshiji", 1)
    assert not dictionary.within_damerau_distance("weishiji", "wieshiji", 0)
    assert dictionary.normalized_similarity("weishiji", "威士忌") == 1.0
    assert dictionary.normalized_distance("weishiji", "威士忌") == 0.0
    assert dictionary.ratio("weishiji", "威士忌") == 1.0
    assert distance("weishiji", "威士忌", dictionary=dictionary) == 0
    assert damerau_distance("weishiji", "wieshiji", dictionary=dictionary) == 1
    assert normalized_similarity("weishiji", "威士忌", dictionary=dictionary) == 1.0
    assert normalized_distance("weishiji", "威士忌", dictionary=dictionary) == 0.0
    assert ratio("weishiji", "威士忌", dictionary=dictionary) == 1.0
    assert within_distance("布納哈奔", "布納哈本", 0, dictionary=dictionary)


def test_top_level_cdist_zh_uses_default_dictionary(tmp_path):
    dictionary = load_bundle(write_test_bundle(tmp_path)[0])
    queries = ["weishiji", "布呐哈本"]
    choices = ["威士忌", "布納哈本"]

    moine.set_default_dictionary(dictionary)
    try:
        assert moine.cdist(queries, choices, dictionary=dictionary) == [
            [moine.distance(query, choice, dictionary=dictionary) for choice in choices]
            for query in queries
        ]
        assert moine.damerau_distance("weishiji", "wieshiji", lang="zh") == 1
        assert moine.cdist(queries, choices, lang="zh") == [
            [moine.distance(query, choice, lang="zh") for choice in choices] for query in queries
        ]
        assert moine.cdist(queries, choices, lang="zh", metric="damerau_distance") == [
            [moine.damerau_distance(query, choice, lang="zh") for choice in choices]
            for query in queries
        ]
        assert moine.cdist(queries, choices, lang="zh", metric="normalized_distance") == [
            [moine.normalized_distance(query, choice, lang="zh") for choice in choices]
            for query in queries
        ]
    finally:
        moine.clear_default_dictionary(lang="zh")


def test_top_level_partial_zh_uses_reading_span_default(tmp_path):
    dictionary = load_bundle(write_test_bundle(tmp_path)[0])

    assert moine.partial_ratio("威士忌", "xxweishijizz", dictionary=dictionary) == 1.0
    assert moine.partial_distance("威士忌", "xxweishijizz", dictionary=dictionary) == 0
    assert moine.partial_alignment(
        "威士忌",
        "xxweishijizz",
        dictionary=dictionary,
    ) == moine.PartialAlignment(
        score=1.0,
        src_start=0,
        src_end=3,
        dest_start=2,
        dest_end=10,
    )


def test_zh_process_extract_distance_and_ratio(tmp_path):
    dictionary = load_bundle(write_test_bundle(tmp_path)[0])

    distance_results = process.extract(
        "weishiji",
        ["布納哈本", "威士忌"],
        dictionary=dictionary,
        scorer="distance",
        score_cutoff=0,
    )
    assert distance_results == [("威士忌", 0, 1)]
    assert (
        extract_one(
            "weishiji",
            ["布納哈本"],
            dictionary=dictionary,
            scorer="distance",
            score_cutoff=0,
        )
        is None
    )

    ratio_results = extract(
        "weishiji",
        {"bunnahabhain": "布納哈本", "whisky": "威士忌"},
        dictionary=dictionary,
        scorer="ratio",
        score_cutoff=1.0,
    )
    assert ratio_results == [("威士忌", 1.0, "whisky")]
    assert process.extract(
        "bunahaben",
        {"traditional": "布納哈本", "simplified": "布那哈本", "whisky": "威士忌"},
        dictionary=dictionary,
        scorer="distance",
        limit=None,
        scorer_kwargs={"max_paths": 128},
        score_cutoff=0,
    ) == [("布納哈本", 0, "traditional"), ("布那哈本", 0, "simplified")]
    with pytest.raises(TypeError, match="score_cutoff must be an int"):
        process.extract(
            "weishiji",
            ["威士忌"],
            dictionary=dictionary,
            scorer="damerau_distance",
            score_cutoff=0.5,
        )
    with pytest.raises(ValueError, match="score_cutoff"):
        process.extract(
            "weishiji",
            ["威士忌"],
            dictionary=dictionary,
            scorer="ratio",
            score_cutoff=1.5,
        )
    with pytest.raises(TypeError, match="unexpected scorer_kwargs key"):
        process.extract(
            "weishiji",
            ["威士忌"],
            dictionary=dictionary,
            scorer_kwargs={"processor": str.lower},
        )


def test_zh_bundle_rejects_file_digest_mismatch(tmp_path):
    metadata_path, _ = write_test_bundle(tmp_path, file_digest="0" * 64)

    with pytest.raises(ValueError, match="payload file digest mismatch"):
        load_bundle(metadata_path)


def test_zh_bundle_rejects_payload_path_escape(tmp_path):
    metadata_path = tmp_path / "metadata.yaml"
    metadata_path.write_text(
        """\
schema_version: 1
artifact_type: moine.zh.reading-index
artifact_name: test
generator: pytest
payload:
  path: ../readings.yaml
  format: yaml.surface-readings.v1
  file_digest_algorithm: sha256-file-v1
  file_digest: 0000000000000000000000000000000000000000000000000000000000000000
  checksum_algorithm: sha256-canonical-v1
  checksum: 0000000000000000000000000000000000000000000000000000000000000000
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
""",
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="stay inside the bundle"):
        load_bundle(metadata_path)
