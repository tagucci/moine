"""Chinese pinyin helpers for moine."""

import os
from os import PathLike

from ._moine import ChineseDictionary

StrPath = str | PathLike[str]

Dictionary = ChineseDictionary


def load_bundle(metadata_path: StrPath, bundle_dir: StrPath | None = None) -> ChineseDictionary:
    """Load a zh CC-CEDICT-derived artifact bundle from metadata.yaml or a bundle directory."""

    metadata_path = os.fspath(metadata_path)
    if bundle_dir is not None:
        bundle_dir = os.fspath(bundle_dir)
    return Dictionary.load_bundle(metadata_path, bundle_dir)


def distance(
    left: str,
    right: str,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: int | None = None,
) -> int:
    """Compute Chinese pinyin lattice distance with an explicit dictionary."""

    return dictionary.distance(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


def damerau_distance(
    left: str,
    right: str,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: int | None = None,
) -> int:
    """Compute Chinese pinyin lattice-side Damerau distance with an explicit dictionary."""

    return dictionary.damerau_distance(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


def within_distance(
    left: str,
    right: str,
    threshold: int,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> bool:
    """Return whether Chinese pinyin lattice distance is within threshold."""

    return dictionary.within_distance(
        left,
        right,
        threshold,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
    )


def within_damerau_distance(
    left: str,
    right: str,
    threshold: int,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> bool:
    """Return whether Chinese pinyin lattice-side Damerau distance is within threshold."""

    return dictionary.within_damerau_distance(
        left,
        right,
        threshold,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
    )


def normalized_similarity(
    left: str,
    right: str,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: float | None = None,
) -> float:
    """Compute Chinese pinyin normalized similarity with an explicit dictionary."""

    return dictionary.normalized_similarity(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


def normalized_distance(
    left: str,
    right: str,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: float | None = None,
) -> float:
    """Compute Chinese pinyin normalized distance with an explicit dictionary."""

    return dictionary.normalized_distance(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


def ratio(
    left: str,
    right: str,
    *,
    dictionary: ChineseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: float | None = None,
) -> float:
    """Alias for Chinese pinyin normalized similarity."""

    return dictionary.ratio(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


__all__ = [
    "ChineseDictionary",
    "Dictionary",
    "damerau_distance",
    "distance",
    "load_bundle",
    "normalized_distance",
    "normalized_similarity",
    "ratio",
    "within_damerau_distance",
    "within_distance",
]
