"""Dictionary artifact discovery and download helpers."""

import argparse
import hashlib
import os
import shutil
import tarfile
import tempfile
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Literal

Language = Literal["ja", "ja-unidic", "ja-sudachi", "zh"]


@dataclass(frozen=True)
class ArtifactSpec:
    lang: Language
    label: str
    artifact_name: str
    archive_name: str
    archive_url: str
    checksum_url: str | None


_RELEASE_BASE_URL = "https://github.com/tagucci/moine/releases/download"
_DOWNLOAD_TIMEOUT_SECONDS = 60
_MAX_DOWNLOAD_BYTES = 512 * 1024 * 1024
_MAX_CHECKSUM_MANIFEST_BYTES = 1024 * 1024
_MAX_ARCHIVE_ENTRIES = 1024
_MAX_ARCHIVE_TOTAL_EXTRACTED_BYTES = 1024 * 1024 * 1024
_MAX_ARCHIVE_ENTRY_BYTES = 512 * 1024 * 1024
_MAX_ARCHIVE_PATH_BYTES = 4096
_MAX_ARCHIVE_PATH_DEPTH = 16
ARTIFACT_SPECS: dict[Language, ArtifactSpec] = {
    "ja": ArtifactSpec(
        lang="ja",
        label="Japanese UniDic-CWJ default",
        artifact_name="moine-unidic-cwj-202512",
        archive_name="moine-unidic-cwj-202512.tar.gz",
        archive_url=(
            f"{_RELEASE_BASE_URL}/unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.gz"
        ),
        checksum_url=f"{_RELEASE_BASE_URL}/unidic-cwj-202512-v0.1.1/SHA256SUMS",
    ),
    "ja-unidic": ArtifactSpec(
        lang="ja-unidic",
        label="Japanese UniDic-CWJ",
        artifact_name="moine-unidic-cwj-202512",
        archive_name="moine-unidic-cwj-202512.tar.gz",
        archive_url=(
            f"{_RELEASE_BASE_URL}/unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.gz"
        ),
        checksum_url=f"{_RELEASE_BASE_URL}/unidic-cwj-202512-v0.1.1/SHA256SUMS",
    ),
    "ja-sudachi": ArtifactSpec(
        lang="ja-sudachi",
        label="Japanese SudachiDict-full",
        artifact_name="moine-sudachi-full-20260428",
        archive_name="moine-sudachi-full-20260428.tar.gz",
        archive_url=(
            f"{_RELEASE_BASE_URL}/"
            "moine-sudachi-full-20260428-v0.2.0/"
            "moine-sudachi-full-20260428.tar.gz"
        ),
        checksum_url=f"{_RELEASE_BASE_URL}/moine-sudachi-full-20260428-v0.2.0/SHA256SUMS",
    ),
    "zh": ArtifactSpec(
        lang="zh",
        label="Chinese CC-CEDICT no-tone",
        artifact_name="moine-cedict-20260520",
        archive_name="moine-cedict-20260520.tar.gz",
        archive_url=(
            f"{_RELEASE_BASE_URL}/moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.gz"
        ),
        checksum_url=f"{_RELEASE_BASE_URL}/moine-cedict-20260520-v0.1.1/SHA256SUMS",
    ),
}


def normalize_lang(lang: str) -> Language:
    if not isinstance(lang, str):
        raise TypeError("lang must be a str")
    if lang == "ja":
        return "ja"
    if lang in {"ja-unidic", "unidic"}:
        return "ja-unidic"
    if lang in {"ja-sudachi", "sudachi"}:
        return "ja-sudachi"
    if lang == "zh":
        return "zh"
    raise ValueError("lang must be 'ja', 'ja-unidic', 'ja-sudachi', or 'zh'")


def default_cache_dir() -> Path:
    override = os.environ.get("MOINE_CACHE_DIR")
    if override:
        return Path(override).expanduser()
    xdg_cache_home = os.environ.get("XDG_CACHE_HOME")
    if xdg_cache_home:
        return Path(xdg_cache_home).expanduser() / "moine" / "dictionaries"
    return Path.home() / ".cache" / "moine" / "dictionaries"


def default_search_roots() -> list[Path]:
    return [default_cache_dir()]


def cli_main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m moine")
    subcommands = parser.add_subparsers(dest="command", required=True)

    download = subcommands.add_parser("download", help="download a dictionary artifact")
    download.add_argument("lang", choices=sorted(ARTIFACT_SPECS))
    download.add_argument("--url", help="artifact archive URL or local path")
    download.add_argument("--checksum-url", help="SHA256SUMS URL or local path")
    download.add_argument("--sha256", help="expected archive SHA-256 hex digest")
    download.add_argument("--cache-dir", type=Path, help="dictionary cache directory")
    download.add_argument("--force", action="store_true", help="replace an existing artifact")

    list_parser = subcommands.add_parser("list", help="list installed dictionary artifacts")
    list_parser.add_argument("--cache-dir", type=Path, help="dictionary cache directory")

    where = subcommands.add_parser("where", help="print dictionary artifact locations")
    where.add_argument("lang", nargs="?", choices=sorted(ARTIFACT_SPECS))
    where.add_argument("--cache-dir", type=Path, help="dictionary cache directory")

    args = parser.parse_args(argv)
    if args.command == "download":
        return _download_command(args)
    if args.command == "list":
        return _list_command(args)
    if args.command == "where":
        return _where_command(args)
    raise AssertionError("unreachable command branch")


def _download_command(args: argparse.Namespace) -> int:
    lang = normalize_lang(args.lang)
    spec = ARTIFACT_SPECS[lang]
    cache_dir = _cache_dir_arg(args.cache_dir)
    archive_url = args.url or spec.archive_url
    checksum_url = args.checksum_url or (spec.checksum_url if args.url is None else None)
    archive_name = Path(urllib.parse.urlparse(archive_url).path).name or spec.archive_name

    with tempfile.TemporaryDirectory(prefix="moine-download-") as tmp:
        tmp_dir = Path(tmp)
        archive_path = tmp_dir / archive_name
        _copy_uri_to_path(archive_url, archive_path)
        expected_sha256 = args.sha256 or (
            _expected_sha256(checksum_url, archive_name) if checksum_url else None
        )
        if expected_sha256:
            actual_sha256 = _sha256_file(archive_path)
            if actual_sha256 != expected_sha256:
                raise RuntimeError(
                    f"checksum mismatch for {archive_name}: "
                    f"expected {expected_sha256}, got {actual_sha256}"
                )

        cache_dir.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".moine-install-", dir=cache_dir) as staging:
            extracted_root = _extract_archive(archive_path, Path(staging) / "extract")
            metadata = extracted_root / "metadata.yaml"
            if not metadata.is_file():
                raise RuntimeError(f"downloaded artifact has no metadata.yaml: {extracted_root}")
            _verify_extracted_bundle(lang, metadata)

            destination = cache_dir / extracted_root.name
            if destination.exists():
                if not args.force:
                    _verify_existing_installed_bundle(lang, destination)
                    print(f"{destination}")
                    return 0
                _replace_dir(extracted_root, destination)
            else:
                extracted_root.rename(destination)
            print(f"{destination}")
            return 0


def _list_command(args: argparse.Namespace) -> int:
    cache_dir = _cache_dir_arg(args.cache_dir)
    for metadata in _installed_metadata_paths([cache_dir]):
        print(metadata.parent)
    return 0


def _where_command(args: argparse.Namespace) -> int:
    cache_dir = _cache_dir_arg(args.cache_dir)
    if args.lang is None:
        print(cache_dir)
        return 0

    lang = normalize_lang(args.lang)
    spec = ARTIFACT_SPECS[lang]
    metadata = _find_metadata_by_prefix([cache_dir], spec.artifact_name)
    if metadata is None:
        print(cache_dir / spec.artifact_name)
    else:
        print(metadata.parent)
    return 0


def _cache_dir_arg(cache_dir: Path | None) -> Path:
    return cache_dir.expanduser() if cache_dir is not None else default_cache_dir()


def _verify_extracted_bundle(lang: Language, metadata: Path) -> None:
    if lang in {"ja", "ja-unidic", "ja-sudachi"}:
        from ._moine import JapaneseDictionary

        dictionary = JapaneseDictionary.load_bundle(os.fspath(metadata))
        _verify_japanese_download_identity(lang, dictionary)
        return
    if lang == "zh":
        from ._moine import ChineseDictionary

        ChineseDictionary.load_bundle(os.fspath(metadata))
        return
    raise AssertionError("unreachable language branch")


def _verify_existing_installed_bundle(lang: Language, destination: Path) -> None:
    metadata = destination / "metadata.yaml"
    if not metadata.is_file():
        raise RuntimeError(f"installed artifact has no metadata.yaml: {destination}")
    _verify_extracted_bundle(lang, metadata)


def _replace_dir(source: Path, destination: Path) -> None:
    backup = _replacement_backup_path(destination)
    destination.rename(backup)
    try:
        source.rename(destination)
    except Exception:
        if destination.exists():
            _remove_path(destination)
        backup.rename(destination)
        raise
    _remove_path(backup)


def _remove_path(path: Path) -> None:
    if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
    else:
        path.unlink()


def _replacement_backup_path(destination: Path) -> Path:
    parent = destination.parent
    for index in range(100):
        candidate = parent / f".{destination.name}.replacing-{os.getpid()}-{index}"
        if not candidate.exists():
            return candidate
    raise RuntimeError(f"could not choose replacement path for {destination}")


def _verify_japanese_download_identity(lang: Language, dictionary) -> None:
    artifact_name = dictionary.artifact_name
    source_name = dictionary.source_name
    reading_field = dictionary.reading_field

    if lang in {"ja", "ja-unidic"}:
        if (
            (artifact_name is not None and artifact_name.startswith("moine-sudachi"))
            or source_name != "UniDic-CWJ"
            or reading_field == "sudachi-reading"
        ):
            raise ValueError(
                f"download {lang} requires a UniDic-CWJ artifact; "
                f"got {artifact_name!r} from {source_name!r}"
            )
        return

    if lang == "ja-sudachi" and (
        (artifact_name is not None and artifact_name.startswith("moine-unidic"))
        or source_name != "SudachiDict"
        or reading_field != "sudachi-reading"
    ):
        raise ValueError(
            "download ja-sudachi requires a SudachiDict artifact; "
            f"got {artifact_name!r} from {source_name!r}"
        )


def _installed_metadata_paths(roots: list[Path]) -> list[Path]:
    metadata_paths: list[Path] = []
    for root in roots:
        if not root.is_dir():
            continue
        if (root / "metadata.yaml").is_file():
            metadata_paths.append(root / "metadata.yaml")
        for child in sorted(root.iterdir()):
            metadata = child / "metadata.yaml"
            if child.is_dir() and metadata.is_file():
                metadata_paths.append(metadata)
    return sorted(set(metadata_paths))


def _find_metadata_by_prefix(roots: list[Path], prefix: str) -> Path | None:
    for metadata in _installed_metadata_paths(roots):
        if metadata.parent.name.startswith(prefix):
            return metadata
    return None


def _copy_uri_to_path(uri: str, output: Path) -> None:
    parsed = urllib.parse.urlparse(uri)
    if parsed.scheme in {"http", "https"}:
        with urllib.request.urlopen(uri, timeout=_DOWNLOAD_TIMEOUT_SECONDS) as response:
            with output.open("wb") as file:
                _copy_limited(response, file, _MAX_DOWNLOAD_BYTES)
        return
    if parsed.scheme == "file":
        _copy_file_limited(Path(urllib.request.url2pathname(parsed.path)), output)
        return
    _copy_file_limited(Path(uri), output)


def _read_uri_text(uri: str) -> str:
    parsed = urllib.parse.urlparse(uri)
    if parsed.scheme in {"http", "https"}:
        with urllib.request.urlopen(uri, timeout=_DOWNLOAD_TIMEOUT_SECONDS) as response:
            data = response.read(_MAX_CHECKSUM_MANIFEST_BYTES + 1)
            if len(data) > _MAX_CHECKSUM_MANIFEST_BYTES:
                raise RuntimeError(
                    f"checksum manifest exceeded {_MAX_CHECKSUM_MANIFEST_BYTES} bytes"
                )
            return data.decode("utf-8")
    if parsed.scheme == "file":
        return _read_file_text_limited(
            Path(urllib.request.url2pathname(parsed.path)),
            _MAX_CHECKSUM_MANIFEST_BYTES,
        )
    return _read_file_text_limited(Path(uri), _MAX_CHECKSUM_MANIFEST_BYTES)


def _copy_file_limited(source: Path, output: Path) -> None:
    if source.stat().st_size > _MAX_DOWNLOAD_BYTES:
        raise RuntimeError(f"download exceeded {_MAX_DOWNLOAD_BYTES} bytes")
    with source.open("rb") as input_file, output.open("wb") as output_file:
        _copy_limited(input_file, output_file, _MAX_DOWNLOAD_BYTES)


def _read_file_text_limited(path: Path, max_bytes: int) -> str:
    if path.stat().st_size > max_bytes:
        raise RuntimeError(f"checksum manifest exceeded {max_bytes} bytes")
    with path.open("rb") as file:
        data = file.read(max_bytes + 1)
    if len(data) > max_bytes:
        raise RuntimeError(f"checksum manifest exceeded {max_bytes} bytes")
    return data.decode("utf-8")


def _copy_limited(source, output, max_bytes: int) -> None:
    copied = 0
    while True:
        chunk = source.read(min(1024 * 1024, max_bytes + 1 - copied))
        if not chunk:
            return
        copied += len(chunk)
        if copied > max_bytes:
            raise RuntimeError(f"download exceeded {max_bytes} bytes")
        output.write(chunk)


def _expected_sha256(checksum_url: str, archive_name: str) -> str:
    for line in _read_uri_text(checksum_url).splitlines():
        parts = line.strip().split()
        if len(parts) != 2:
            continue
        digest, label = parts
        if label == archive_name or Path(label).name == archive_name:
            return digest.lower()
    raise RuntimeError(f"{archive_name} not found in checksum manifest: {checksum_url}")


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _extract_archive(archive: Path, output_dir: Path) -> Path:
    output_dir.mkdir(parents=True, exist_ok=True)
    destination_root = output_dir.resolve()
    root_name: str | None = None
    entry_count = 0
    total_extracted_bytes = 0
    with tarfile.open(archive, "r:*") as tar:
        for member in tar:
            entry_count += 1
            if entry_count > _MAX_ARCHIVE_ENTRIES:
                raise RuntimeError(
                    f"archive entry count {entry_count} exceeded {_MAX_ARCHIVE_ENTRIES}"
                )
            if not member.isdir() and not member.isfile():
                raise RuntimeError(f"unsupported archive entry type: {member.name}")
            parts = _safe_archive_member_parts(member.name)
            current_root = parts[0]
            if root_name is None:
                root_name = current_root
            elif root_name != current_root:
                raise RuntimeError("artifact archive must contain exactly one top-level directory")
            target = output_dir.joinpath(*parts)
            resolved_target = target.resolve()
            if (
                destination_root not in resolved_target.parents
                and resolved_target != destination_root
            ):
                raise RuntimeError(f"unsafe archive path: {member.name}")
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            if member.size < 0:
                raise RuntimeError(f"archive entry {member.name} has a negative size")
            if member.size > _MAX_ARCHIVE_ENTRY_BYTES:
                raise RuntimeError(
                    f"archive entry {member.name} size {member.size} exceeded "
                    f"{_MAX_ARCHIVE_ENTRY_BYTES}"
                )
            total_extracted_bytes += member.size
            if total_extracted_bytes > _MAX_ARCHIVE_TOTAL_EXTRACTED_BYTES:
                raise RuntimeError(
                    f"archive extracted size {total_extracted_bytes} exceeded "
                    f"{_MAX_ARCHIVE_TOTAL_EXTRACTED_BYTES}"
                )
            target.parent.mkdir(parents=True, exist_ok=True)
            source = tar.extractfile(member)
            if source is None:
                raise RuntimeError(f"missing archive file data: {member.name}")
            with source, target.open("wb") as output:
                shutil.copyfileobj(source, output)
    if root_name is None:
        raise RuntimeError("artifact archive is empty")
    return output_dir / root_name


def _safe_archive_member_parts(name: str) -> tuple[str, ...]:
    if not name or "\\" in name or len(name.encode("utf-8")) > _MAX_ARCHIVE_PATH_BYTES:
        raise RuntimeError(f"unsafe archive path: {name}")
    path = PurePosixPath(name)
    parts = path.parts
    if (
        path.is_absolute()
        or not parts
        or len(parts) > _MAX_ARCHIVE_PATH_DEPTH
        or any(part in {"", ".", ".."} for part in parts)
    ):
        raise RuntimeError(f"unsafe archive path: {name}")
    return parts
