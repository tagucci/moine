import hashlib
import tarfile
from pathlib import Path

import moine
import pytest
from moine._artifacts import ARTIFACT_SPECS, _extract_archive, cli_main


def push_len_prefixed(data, tag, value):
    encoded = value.encode("utf-8")
    data.extend(tag)
    data.extend(str(len(encoded)).encode("ascii"))
    data.append(0x0A)
    data.extend(encoded)
    data.append(0x0A)


def payload_checksum(entries):
    data = bytearray(b"moine.unidic.reading-index.surface-readings/v1\n")
    for surface, readings in entries:
        push_len_prefixed(data, b"S", surface)
        data.extend(f"R{len(readings)}\n".encode("ascii"))
        for reading in readings:
            push_len_prefixed(data, b"r", reading)
    return hashlib.sha256(data).hexdigest()


def write_ja_bundle(
    root: Path,
    *,
    artifact_name: str = "moine-unidic-cwj-202512-test",
    source_name: str = "UniDic-CWJ",
    reading_field: str = "pron",
    insatsu_reading: str = "インサツ",
) -> None:
    root.mkdir(parents=True)
    payload_path = root / "readings.yaml"
    payload_path.write_text(
        f"""\
schema_version: 1
payload_type: moine.unidic.reading-index.surface-readings
entries:
- surface: 印刷
  readings:
  - {insatsu_reading}
""",
        encoding="utf-8",
    )
    root.joinpath("metadata.yaml").write_text(
        f"""\
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: {artifact_name}
generator: pytest
payload:
  path: readings.yaml
  format: yaml.surface-readings.v1
  file_digest_algorithm: sha256-file-v1
  file_digest: {hashlib.sha256(payload_path.read_bytes()).hexdigest()}
  checksum_algorithm: sha256-canonical-v1
  checksum: {payload_checksum([("印刷", [insatsu_reading])])}
source:
  name: {source_name}
  version: test
  lex_csv: lex.csv
build:
  reading_field: {reading_field}
  max_readings_per_surface: 16
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


def write_archive(tmp_path: Path) -> Path:
    bundle_dir = tmp_path / "bundle" / "moine-unidic-cwj-202512-test"
    write_ja_bundle(bundle_dir)
    archive = tmp_path / "moine-unidic-cwj-202512-test.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(bundle_dir, arcname=bundle_dir.name)
    return archive


def write_sudachi_archive(tmp_path: Path) -> Path:
    bundle_dir = tmp_path / "sudachi-bundle" / "moine-sudachi-full-20260428-test"
    write_ja_bundle(
        bundle_dir,
        artifact_name="moine-sudachi-full-20260428-test",
        source_name="SudachiDict",
        reading_field="sudachi-reading",
    )
    archive = tmp_path / "moine-sudachi-full-20260428-test.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(bundle_dir, arcname=bundle_dir.name)
    return archive


def test_default_artifact_urls_and_japanese_aliases():
    assert ARTIFACT_SPECS["ja"].artifact_name == ARTIFACT_SPECS["ja-unidic"].artifact_name
    assert "unidic-cwj-202512-v0.1.1" in ARTIFACT_SPECS["ja"].archive_url
    assert "unidic-cwj-202512-v0.1.1" in ARTIFACT_SPECS["ja-unidic"].archive_url
    assert "moine-sudachi-full-20260428-v0.2.0" in ARTIFACT_SPECS["ja-sudachi"].archive_url
    assert "moine-cedict-20260520-v0.1.1" in ARTIFACT_SPECS["zh"].archive_url
    assert ARTIFACT_SPECS["ja"].checksum_url is not None
    assert ARTIFACT_SPECS["ja"].checksum_url.endswith("/unidic-cwj-202512-v0.1.1/SHA256SUMS")
    assert ARTIFACT_SPECS["ja-sudachi"].checksum_url is not None
    assert ARTIFACT_SPECS["ja-sudachi"].checksum_url.endswith(
        "/moine-sudachi-full-20260428-v0.2.0/SHA256SUMS"
    )
    assert ARTIFACT_SPECS["zh"].checksum_url is not None
    assert ARTIFACT_SPECS["zh"].checksum_url.endswith("/moine-cedict-20260520-v0.1.1/SHA256SUMS")


def test_download_list_where_and_default_cache_lookup(tmp_path, monkeypatch, capsys):
    cache_dir = tmp_path / "cache"
    archive = write_archive(tmp_path)

    assert (
        cli_main(
            [
                "download",
                "ja",
                "--url",
                str(archive),
                "--cache-dir",
                str(cache_dir),
            ]
        )
        == 0
    )
    installed = cache_dir / "moine-unidic-cwj-202512-test"
    assert installed.is_dir()
    assert capsys.readouterr().out.strip() == str(installed)

    assert cli_main(["list", "--cache-dir", str(cache_dir)]) == 0
    assert capsys.readouterr().out.strip() == str(installed)

    assert cli_main(["where", "ja", "--cache-dir", str(cache_dir)]) == 0
    assert capsys.readouterr().out.strip() == str(installed)

    assert cli_main(["where", "ja-unidic", "--cache-dir", str(cache_dir)]) == 0
    assert capsys.readouterr().out.strip() == str(installed)

    monkeypatch.setenv("MOINE_CACHE_DIR", str(cache_dir))
    moine.clear_default_dictionary(lang="ja")
    moine.clear_default_dictionary(lang="ja-unidic")
    try:
        assert moine.distance("いんさt", "印刷", lang="ja") == 1
        assert moine.distance("いんさt", "印刷", lang="ja-unidic") == 1
    finally:
        moine.clear_default_dictionary(lang="ja")
        moine.clear_default_dictionary(lang="ja-unidic")


def test_sudachi_download_where_and_cache_lookup(tmp_path, monkeypatch, capsys):
    cache_dir = tmp_path / "cache"
    archive = write_sudachi_archive(tmp_path)

    assert (
        cli_main(
            [
                "download",
                "ja-sudachi",
                "--url",
                str(archive),
                "--cache-dir",
                str(cache_dir),
            ]
        )
        == 0
    )
    installed = cache_dir / "moine-sudachi-full-20260428-test"
    assert installed.is_dir()
    assert capsys.readouterr().out.strip() == str(installed)

    assert cli_main(["where", "ja-sudachi", "--cache-dir", str(cache_dir)]) == 0
    assert capsys.readouterr().out.strip() == str(installed)

    monkeypatch.setenv("MOINE_CACHE_DIR", str(cache_dir))
    moine.clear_default_dictionary(lang="ja")
    moine.clear_default_dictionary(lang="ja-sudachi")
    try:
        with pytest.raises(FileNotFoundError, match="No default 'ja' dictionary artifact"):
            moine.load_dict(lang="ja")
        assert moine.distance("いんさt", "印刷", lang="ja-sudachi") == 1
    finally:
        moine.clear_default_dictionary(lang="ja")
        moine.clear_default_dictionary(lang="ja-sudachi")


def test_download_rejects_japanese_selector_mismatch(tmp_path):
    cache_dir = tmp_path / "cache"
    archive = write_archive(tmp_path)

    with pytest.raises(ValueError, match="requires a SudachiDict artifact"):
        cli_main(
            [
                "download",
                "ja-sudachi",
                "--url",
                str(archive),
                "--cache-dir",
                str(cache_dir),
            ]
        )


def test_japanese_env_path_must_match_requested_selector(tmp_path, monkeypatch):
    sudachi_bundle = tmp_path / "moine-sudachi-full-20260428-test"
    write_ja_bundle(
        sudachi_bundle,
        artifact_name="moine-sudachi-full-20260428-test",
        source_name="SudachiDict",
        reading_field="sudachi-reading",
    )

    monkeypatch.setenv("MOINE_JA_DICTIONARY", str(sudachi_bundle))
    moine.clear_default_dictionary(lang="ja")
    moine.clear_default_dictionary(lang="ja-sudachi")
    try:
        with pytest.raises(ValueError, match="requires a UniDic artifact"):
            moine.load_dict(lang="ja")
        assert moine.load_dict(lang="ja-sudachi").distance("いんさt", "印刷") == 1
    finally:
        moine.clear_default_dictionary(lang="ja")
        moine.clear_default_dictionary(lang="ja-sudachi")


def test_japanese_explicit_path_must_match_requested_selector(tmp_path):
    unidic_bundle = tmp_path / "moine-unidic-cwj-202512-test"
    write_ja_bundle(unidic_bundle)

    with pytest.raises(ValueError, match="requires a Sudachi artifact"):
        moine.load_dict(lang="ja-sudachi", path=unidic_bundle)


def test_japanese_default_cache_prefers_unidic_over_sudachi(tmp_path, monkeypatch):
    cache_dir = tmp_path / "cache"
    write_ja_bundle(cache_dir / "moine-sudachi-full-20260428-test", insatsu_reading="アウト")
    write_ja_bundle(cache_dir / "moine-unidic-cwj-202512-test")

    monkeypatch.setenv("MOINE_CACHE_DIR", str(cache_dir))
    moine.clear_default_dictionary(lang="ja")
    try:
        assert moine.distance("いんさt", "印刷", lang="ja") == 1
    finally:
        moine.clear_default_dictionary(lang="ja")


def test_japanese_loader_rejects_wrong_artifact_type(tmp_path):
    bundle = tmp_path / "moine-unidic-cwj-202512-test"
    write_ja_bundle(bundle)
    metadata = bundle / "metadata.yaml"
    metadata.write_text(
        metadata.read_text(encoding="utf-8").replace(
            "artifact_type: moine.unidic.reading-index",
            "artifact_type: moine.zh.reading-index",
        ),
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="unsupported Japanese artifact type"):
        moine.JapaneseDictionary.load_bundle(str(bundle))


def test_japanese_loader_rejects_missing_license_reference(tmp_path):
    bundle = tmp_path / "moine-unidic-cwj-202512-test"
    write_ja_bundle(bundle)
    metadata = bundle / "metadata.yaml"
    metadata.write_text(
        metadata.read_text(encoding="utf-8").replace(
            "  references: []",
            "  references:\n  - label: BSD\n    path: license/BSD\n",
        ),
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="missing license reference BSD"):
        moine.JapaneseDictionary.load_bundle(str(bundle))


def test_large_python_distance_returns_error_instead_of_panicking():
    text = "a" * 4000

    with pytest.raises(ValueError, match="distance matrix"):
        moine.distance(text, text)


def test_extract_archive_rejects_links(tmp_path):
    archive = tmp_path / "unsafe.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        root = tarfile.TarInfo("bundle")
        root.type = tarfile.DIRTYPE
        tar.addfile(root)
        link = tarfile.TarInfo("bundle/payload")
        link.type = tarfile.SYMTYPE
        link.linkname = "../outside"
        tar.addfile(link)

    with pytest.raises(RuntimeError, match="unsupported archive entry type"):
        _extract_archive(archive, tmp_path / "extract")


def test_default_artifact_specs_point_to_current_releases():
    assert (
        ARTIFACT_SPECS["ja"].archive_url == "https://github.com/tagucci/moine/releases/download/"
        "unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.gz"
    )
    assert (
        ARTIFACT_SPECS["ja-sudachi"].archive_url
        == "https://github.com/tagucci/moine/releases/download/"
        "moine-sudachi-full-20260428-v0.2.0/moine-sudachi-full-20260428.tar.gz"
    )
    assert (
        ARTIFACT_SPECS["zh"].archive_url == "https://github.com/tagucci/moine/releases/download/"
        "moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.gz"
    )
