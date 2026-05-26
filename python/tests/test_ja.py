import hashlib
from importlib import resources

import moine
import pytest
from moine.ja import (
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


def payload_checksum(entries, algorithm="sha256-canonical-v1"):
    data = bytearray(b"moine.unidic.reading-index.surface-readings/v1\n")
    for surface, readings in sorted(entries):
        push_len_prefixed(data, b"S", surface)
        data.extend(f"R{len(readings)}\n".encode("ascii"))
        for reading in readings:
            push_len_prefixed(data, b"r", reading)

    if algorithm == "sha256-canonical-v1":
        return hashlib.sha256(data).hexdigest()
    if algorithm == "fnv1a64-canonical-v1":
        value = 0xCBF29CE484222325
        for byte in data:
            value ^= byte
            value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
        return f"{value:016x}"
    raise ValueError(f"unsupported test checksum algorithm: {algorithm}")


def push_len_prefixed(data, tag, value):
    encoded = value.encode("utf-8")
    data.extend(tag)
    data.extend(str(len(encoded)).encode("ascii"))
    data.append(0x0A)
    data.extend(encoded)
    data.append(0x0A)


def write_test_bundle(
    tmp_path,
    checksum_algorithm="sha256-canonical-v1",
    include_file_digest=True,
    file_digest=None,
):
    entries = [
        ("印刷", ["インサツ"]),
        ("モイニャ", ["モイニャ"]),
        ("ブナハーブン", ["ブナハーブン"]),
    ]
    payload_path = tmp_path / "readings.yaml"
    payload_path.write_text(
        """\
schema_version: 1
payload_type: moine.unidic.reading-index.surface-readings
entries:
- surface: 印刷
  readings:
  - インサツ
- surface: モイニャ
  readings:
  - モイニャ
- surface: ブナハーブン
  readings:
  - ブナハーブン
""",
        encoding="utf-8",
    )
    checksum = payload_checksum(entries, checksum_algorithm)
    if file_digest is None:
        file_digest = hashlib.sha256(payload_path.read_bytes()).hexdigest()
    file_digest_lines = (
        f"""\
  file_digest_algorithm: sha256-file-v1
  file_digest: {file_digest}
"""
        if include_file_digest
        else ""
    )

    metadata_path = tmp_path / "metadata.yaml"
    metadata_path.write_text(
        f"""\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: pytest
payload:
  path: readings.yaml
  format: yaml.surface-readings.v1
{file_digest_lines}  checksum_algorithm: {checksum_algorithm}
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
  entries: 3
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references: []
""",
        encoding="utf-8",
    )
    return metadata_path


def test_low_level_distance_helpers():
    assert moine.distance("abc", "adc") == 1
    assert moine.distance("abc", "adc", score_cutoff=0) == 1
    assert moine.damerau_distance("abc", "acb") == 1
    assert moine.damerau_distance("abc", "acb", score_cutoff=0) == 1
    assert moine.normalized_similarity("abc", "adc") == pytest.approx(2 / 3)
    assert moine.normalized_similarity("abc", "adc", score_cutoff=0.8) == 0.0
    assert moine.normalized_distance("abc", "adc") == pytest.approx(1 / 3)
    assert moine.normalized_distance("abc", "adc", score_cutoff=0.2) == 1.0
    assert moine.ratio("abc", "adc") == pytest.approx(2 / 3)
    assert moine.ratio("abc", "adc", score_cutoff=0.5) == pytest.approx(2 / 3)
    assert moine.distance_paths(["insatu", "innsatu"], ["insat"]) == 1
    assert moine.distance_paths(["insatu", "innsatu"], ["insat"], score_cutoff=0) == 1
    assert moine.damerau_distance_paths(["abc", "axc"], ["acb"]) == 1
    assert moine.within_damerau_distance_paths(["abc", "axc"], ["acb"], 1)
    assert not moine.within_damerau_distance_paths(["abc", "axc"], ["acb"], 0)
    assert moine.normalized_similarity_paths(["abc", "abcd"], ["abxd"]) == 0.75
    assert moine.normalized_distance_paths(["abc", "abcd"], ["abxd"]) == 0.25
    assert moine.normalized_similarity_paths(["abc", "abcd"], ["abxd"], score_cutoff=0.8) == 0.0
    assert moine.ratio_paths(["abc", "abcd"], ["abxd"]) == 0.75
    assert moine.within_distance_paths(["insatu"], ["insat"], 1)
    assert not moine.within_distance_paths(["insatu"], ["insat"], 0)
    assert moine.cdist(["abc", "axc"], ["abc", "acb"]) == [[0, 2], [1, 2]]
    assert moine.cdist(["abc"], ["axc"], metric="damerau_distance") == [[1]]
    assert moine.cdist(["abc"], ["abc", "adc"], metric="ratio") == [[1.0, pytest.approx(2 / 3)]]
    assert moine.cdist(
        ["abc"],
        ["abc", "adc"],
        metric="normalized_distance",
        score_cutoff=0.2,
    ) == [[0.0, 1.0]]
    assert moine.cdist([], ["abc"]) == []
    assert moine.cdist(["abc"], []) == [[]]
    assert moine.partial_distance("abc", "xxabczz") == 0
    assert moine.partial_ratio("abc", "xxabczz") == 1.0
    assert moine.partial_alignment("abc", "xxabczz") == moine.PartialAlignment(
        score=1.0,
        src_start=0,
        src_end=3,
        dest_start=2,
        dest_end=5,
    )
    assert moine.partial_alignment(
        "abc",
        "xxabczz",
        metric="distance",
    ) == moine.PartialAlignment(
        score=0,
        src_start=0,
        src_end=3,
        dest_start=2,
        dest_end=5,
    )
    assert moine.partial_ratio("abc", "xxabczz", score_cutoff=1.0) == 1.0
    assert moine.partial_distance("abc", "xxxx", score_cutoff=1) == 2
    with pytest.raises(ValueError, match="score_cutoff"):
        moine.partial_ratio("abc", "xxabczz", score_cutoff=1.1)
    with pytest.raises(ValueError, match="metric"):
        moine.partial_alignment("abc", "abc", metric="normalized_similarity")
    with pytest.raises(ValueError, match="max_span_chars"):
        moine.partial_alignment("abc", "abc", max_span_chars=-1)
    with pytest.raises(ValueError, match="max_span_chars"):
        moine.partial_alignment("abc", "abc", max_span_chars=0)
    with pytest.raises(ValueError, match="score_cutoff"):
        moine.ratio("abc", "adc", score_cutoff=1.5)
    with pytest.raises(ValueError, match="score_cutoff"):
        moine.distance("abc", "adc", score_cutoff=-1)
    with pytest.raises(ValueError, match="score_cutoff"):
        moine.distance_paths(["abc"], ["adc"], score_cutoff=-1)
    with pytest.raises(ValueError, match="threshold"):
        moine.within_distance_paths(["abc"], ["adc"], -1)
    with pytest.raises(TypeError, match="threshold"):
        moine.within_damerau_distance_paths(["abc"], ["adc"], True)
    with pytest.raises(TypeError, match="requires lang or dictionary"):
        moine.cdist(["abc"], ["abc"], max_paths=1)


def test_package_includes_type_markers():
    package_files = resources.files("moine")

    assert (package_files / "py.typed").is_file()
    assert (package_files / "__init__.pyi").is_file()
    assert (package_files / "_moine.pyi").is_file()
    assert not (package_files / "ja.pyi").is_file()


def test_ja_bundle_helpers_use_metadata_defaults(tmp_path):
    metadata_path = write_test_bundle(tmp_path)

    dictionary = load_bundle(metadata_path)
    directory_dictionary = load_bundle(tmp_path)
    native_directory_dictionary = Dictionary.load_bundle(str(tmp_path))

    assert isinstance(dictionary, Dictionary)
    assert dictionary.distance("いんさt", "印刷") == 1
    assert directory_dictionary.distance("いんさt", "印刷") == 1
    assert native_directory_dictionary.distance("いんさt", "印刷") == 1
    assert dictionary.distance("いんさt", "印刷", score_cutoff=0) == 1
    assert dictionary.damerau_distance("モイネ", "モニエ") == 1
    assert dictionary.damerau_distance("モイネ", "モニエ", score_cutoff=0) == 1
    assert dictionary.within_distance("いんさt", "印刷", 1)
    assert not dictionary.within_distance("いんさt", "印刷", 0)
    assert dictionary.within_damerau_distance("モイネ", "モニエ", 1)
    assert not dictionary.within_damerau_distance("モイネ", "モニエ", 0)
    assert dictionary.normalized_similarity("いんさt", "印刷") == pytest.approx(6 / 7)
    assert dictionary.normalized_similarity("いんさt", "印刷", score_cutoff=0.9) == 0.0
    assert dictionary.normalized_distance("いんさt", "印刷") == pytest.approx(1 / 7)
    assert dictionary.normalized_distance("いんさt", "印刷", score_cutoff=0.1) == 1.0
    assert dictionary.ratio("いんさt", "印刷") == pytest.approx(6 / 7)
    assert distance("いんさt", "印刷", dictionary=dictionary) == 1
    assert distance("いんさt", "印刷", dictionary=dictionary, score_cutoff=0) == 1
    assert damerau_distance("モイネ", "モニエ", dictionary=dictionary) == 1
    assert damerau_distance("モイネ", "モニエ", dictionary=dictionary, score_cutoff=0) == 1
    assert normalized_similarity("いんさt", "印刷", dictionary=dictionary) == pytest.approx(6 / 7)
    assert normalized_distance("いんさt", "印刷", dictionary=dictionary) == pytest.approx(1 / 7)
    assert ratio("いんさt", "印刷", dictionary=dictionary) == pytest.approx(6 / 7)
    assert ratio("いんさt", "印刷", dictionary=dictionary, score_cutoff=0.9) == 0.0
    assert within_distance("いんさt", "印刷", 1, dictionary=dictionary)
    assert not within_distance("いんさt", "印刷", 0, dictionary=dictionary)
    assert dictionary.distance("もいにゃ", "モイニャ") == 0
    assert dictionary.distance("ぴーと", "ピート") == 0
    assert dictionary.distance("ピーと", "ピート") == 0
    assert dictionary.distance("ピィート", "ピート") == 2
    assert dictionary.ratio("ピィート", "ピート") == pytest.approx(5 / 7)


def test_top_level_ja_dictionary_loading_and_defaults(tmp_path, monkeypatch):
    metadata_path = write_test_bundle(tmp_path)

    dictionary = moine.load_dict(lang="ja", path=tmp_path)

    assert isinstance(dictionary, moine.JapaneseDictionary)
    assert moine.distance("いんさt", "印刷", lang="ja", dictionary=dictionary) == 1
    assert moine.damerau_distance("モイネ", "モニエ", lang="ja", dictionary=dictionary) == 1
    assert moine.normalized_similarity(
        "いんさt", "印刷", lang="ja", dictionary=dictionary
    ) == pytest.approx(6 / 7)

    with pytest.raises(FileNotFoundError, match="No default 'ja' dictionary artifact"):
        moine.distance("いんさt", "印刷", lang="ja")

    moine.set_default_dictionary(dictionary)
    try:
        assert moine.get_default_dictionary(lang="ja") is dictionary
        assert moine.distance("いんさt", "印刷", lang="ja") == 1
        assert moine.damerau_distance("モイネ", "モニエ", lang="ja") == 1
        assert moine.ratio("いんさt", "印刷", lang="ja") == pytest.approx(6 / 7)
    finally:
        moine.clear_default_dictionary(lang="ja")

    monkeypatch.setenv("MOINE_JA_DICTIONARY", str(metadata_path))
    assert moine.load_dict(lang="ja").distance("いんさt", "印刷") == 1
    assert moine.distance("いんさt", "印刷", lang="ja") == 1
    assert moine.get_default_dictionary(lang="ja") is not None
    moine.clear_default_dictionary(lang="ja")

    dictionary_root = tmp_path / "installed"
    dictionary_bundle = dictionary_root / "moine-unidic-test"
    dictionary_bundle.mkdir(parents=True)
    write_test_bundle(dictionary_bundle)

    monkeypatch.delenv("MOINE_JA_DICTIONARY")
    monkeypatch.setenv("MOINE_DICTIONARIES_PATH", str(dictionary_root))
    assert moine.load_dict(lang="ja").distance("いんさt", "印刷") == 1

    with pytest.raises(TypeError, match="max_paths requires lang or dictionary"):
        moine.distance("abc", "abc", max_paths=1)


def test_top_level_cdist_ja_uses_default_dictionary(tmp_path):
    dictionary = load_bundle(write_test_bundle(tmp_path))
    queries = ["いんさt", "abc"]
    choices = ["印刷", "abc"]

    moine.set_default_dictionary(dictionary)
    try:
        assert moine.cdist(queries, choices, dictionary=dictionary) == [
            [moine.distance(query, choice, dictionary=dictionary) for choice in choices]
            for query in queries
        ]
        assert moine.cdist(iter(queries), choices, lang="ja") == [
            [moine.distance(query, choice, lang="ja") for choice in choices] for query in queries
        ]
        assert moine.cdist(queries, choices, lang="ja", metric="damerau_distance") == [
            [moine.damerau_distance(query, choice, lang="ja") for choice in choices]
            for query in queries
        ]
        assert moine.cdist(queries, choices, lang="ja", metric="ratio") == [
            [moine.ratio(query, choice, lang="ja") for choice in choices] for query in queries
        ]
        assert moine.cdist(
            queries,
            choices,
            lang="ja",
            metric="ratio",
            score_cutoff=0.9,
        ) == [[0.0, 0.0], [0.0, 1.0]]
        assert moine.cdist([], choices, lang="ja") == []
        assert moine.cdist(queries, [], lang="ja") == [[], []]
        with pytest.raises(ValueError, match="metric must be"):
            moine.cdist(queries, choices, lang="ja", metric="unknown")
        with pytest.raises(TypeError, match="score_cutoff must be an int"):
            moine.cdist(queries, choices, lang="ja", score_cutoff=0.5)
        with pytest.raises(TypeError, match="ChineseDictionary"):
            moine.cdist(queries, choices, lang="zh", dictionary=dictionary)
    finally:
        moine.clear_default_dictionary(lang="ja")


def test_top_level_partial_ja_uses_dictionary(tmp_path):
    dictionary = load_bundle(write_test_bundle(tmp_path))

    assert (
        moine.partial_distance(
            "ウイスキー",
            "ういすきーをのんでいます",
            dictionary=dictionary,
        )
        == 0
    )
    assert moine.partial_alignment(
        "ウイスキー",
        "ういすきーをのんでいます",
        dictionary=dictionary,
    ) == moine.PartialAlignment(
        score=1.0,
        src_start=0,
        src_end=5,
        dest_start=0,
        dest_end=5,
    )
    assert moine.partial_alignment(
        "ウイスキー",
        "未知ういすきー。",
        dictionary=dictionary,
    ) == moine.PartialAlignment(
        score=1.0,
        src_start=0,
        src_end=5,
        dest_start=2,
        dest_end=7,
    )
    assert (
        moine.partial_alignment(
            "ウイスキー",
            "未知。",
            dictionary=dictionary,
        )
        is None
    )
    assert (
        moine.partial_distance(
            "ウイスキー",
            "未知。",
            dictionary=dictionary,
        )
        == 5
    )
    assert moine.partial_alignment(
        "ブナハーブン",
        "xxぶなはーぶんzz",
        dictionary=dictionary,
        metric="distance",
    ) == moine.PartialAlignment(
        score=0,
        src_start=0,
        src_end=6,
        dest_start=2,
        dest_end=8,
    )
    assert moine.partial_ratio("ブナハーブン", "xxbunahaabunzz", dictionary=dictionary) == 1.0
    assert moine.partial_alignment(
        "ブナハーブン",
        "xxbunahaabunzz",
        dictionary=dictionary,
    ) == moine.PartialAlignment(
        score=1.0,
        src_start=0,
        src_end=6,
        dest_start=2,
        dest_end=12,
    )
    assert moine.partial_alignment(
        "ブナハーブン",
        "xxぶなはーぶんzz",
        dictionary=dictionary,
        metric="distance",
        score_cutoff=0,
    ) == moine.PartialAlignment(
        score=0,
        src_start=0,
        src_end=6,
        dest_start=2,
        dest_end=8,
    )
    assert (
        moine.partial_distance(
            "ブナハーブン",
            "xxぶなはーぶ",
            dictionary=dictionary,
            score_cutoff=0,
        )
        == 1
    )


def test_ja_process_extract_distance_and_ratio(tmp_path):
    dictionary = load_bundle(write_test_bundle(tmp_path))

    distance_results = process.extract(
        "いんさt",
        ["abc", "印刷"],
        dictionary=dictionary,
        scorer="distance",
        score_cutoff=1,
    )
    assert distance_results == [("印刷", 1, 1)]
    assert (
        extract_one(
            "いんさt",
            ["abc"],
            dictionary=dictionary,
            scorer="distance",
            score_cutoff=1,
        )
        is None
    )

    ratio_results = extract(
        "いんさt",
        {"ascii": "abc", "kanji": "印刷"},
        dictionary=dictionary,
        scorer="ratio",
        score_cutoff=0.8,
    )
    assert ratio_results == [("印刷", pytest.approx(6 / 7), "kanji")]
    assert process.extract_one(
        "abc",
        ["axc", "ayc"],
        dictionary=dictionary,
        scorer="distance",
    ) == ("axc", 1, 0)
    assert process.extract_one(
        "abc",
        ["axc", "ayc"],
        dictionary=dictionary,
        scorer="normalized_distance",
        score_cutoff=0.5,
    ) == ("axc", pytest.approx(1 / 3), 0)
    assert process.extract(
        "abc",
        {"first": "axc", "second": "abc", "third": "ayc"},
        dictionary=dictionary,
        scorer="distance",
        limit=None,
    ) == [("abc", 0, "second"), ("axc", 1, "first"), ("ayc", 1, "third")]
    assert process.extract(
        "ぴーと",
        {"peat": "ピート", "mòine": "モイニャ"},
        dictionary=dictionary,
        scorer="distance",
        scorer_kwargs={"max_paths": 128},
        score_cutoff=0,
    ) == [("ピート", 0, "peat")]
    assert process.extract_one(
        "abc",
        ["acb"],
        dictionary=dictionary,
        scorer="damerau_distance",
        score_cutoff=1,
    ) == ("acb", 1, 0)
    with pytest.raises(TypeError, match="score_cutoff must be an int"):
        process.extract(
            "abc",
            ["acb"],
            dictionary=dictionary,
            scorer="damerau_distance",
            score_cutoff=0.5,
        )
    with pytest.raises(ValueError, match="score_cutoff"):
        process.extract(
            "abc",
            ["acb"],
            dictionary=dictionary,
            scorer="ratio",
            score_cutoff=1.5,
        )
    with pytest.raises(TypeError, match="unexpected scorer_kwargs key"):
        process.extract(
            "abc",
            ["abc"],
            dictionary=dictionary,
            scorer_kwargs={"score_hint": 1},
        )
    with pytest.raises(TypeError, match="must be an int or None"):
        process.extract(
            "abc",
            ["abc"],
            dictionary=dictionary,
            scorer_kwargs={"max_paths": 1.5},
        )
    with pytest.raises(ValueError, match="must be >= 0"):
        process.extract(
            "abc",
            ["abc"],
            dictionary=dictionary,
            scorer_kwargs={"max_paths": -1},
        )
    with pytest.raises(TypeError, match="limit must be an int or None"):
        process.extract("abc", ["abc"], dictionary=dictionary, limit=1.5)
    with pytest.raises(TypeError, match="limit must be an int or None"):
        process.extract("abc", ["abc"], dictionary=dictionary, limit=True)

    with pytest.raises(ValueError, match="scorer"):
        process.extract("abc", ["abc"], dictionary=dictionary, scorer="unknown")


def test_ja_bundle_accepts_legacy_fnv_checksum(tmp_path):
    metadata_path = write_test_bundle(
        tmp_path,
        "fnv1a64-canonical-v1",
        include_file_digest=False,
    )

    dictionary = load_bundle(metadata_path)

    assert dictionary.distance("いんさt", "印刷") == 1


def test_ja_bundle_rejects_file_digest_mismatch(tmp_path):
    metadata_path = write_test_bundle(
        tmp_path,
        file_digest="0" * 64,
    )

    with pytest.raises(ValueError, match="payload file digest mismatch"):
        load_bundle(metadata_path)


def test_ja_bundle_rejects_payload_path_escape(tmp_path):
    metadata_path = tmp_path / "metadata.yaml"
    metadata_path.write_text(
        """\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: test
generator: pytest
payload:
  path: ../readings.yaml
  format: yaml.surface-readings.v1
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
""",
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="stay inside the bundle"):
        load_bundle(metadata_path)
