"""Japanese helpers for moine."""

import os
from collections.abc import Mapping
from os import PathLike

from ._moine import JapaneseDictionary
from ._process import Choices, ExtractResult, ProcessNamespace, Score, Scorer, ScorerFunctions

StrPath = str | PathLike[str]


Dictionary = JapaneseDictionary


def load_bundle(metadata_path: StrPath, bundle_dir: StrPath | None = None) -> JapaneseDictionary:
    """Load a UniDic artifact bundle from metadata.yaml or a bundle directory."""

    metadata_path = os.fspath(metadata_path)
    if bundle_dir is not None:
        bundle_dir = os.fspath(bundle_dir)
    return Dictionary.load_bundle(metadata_path, bundle_dir)


def distance(
    left: str,
    right: str,
    *,
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: int | None = None,
) -> int:
    """Compute Japanese lattice distance with an explicit dictionary."""

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
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: int | None = None,
) -> int:
    """Compute Japanese lattice-side Damerau distance with an explicit dictionary."""

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
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> bool:
    """Return whether Japanese lattice distance is within threshold."""

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
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> bool:
    """Return whether Japanese lattice-side Damerau distance is within threshold."""

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
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: float | None = None,
) -> float:
    """Compute Japanese normalized similarity with an explicit dictionary."""

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
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: float | None = None,
) -> float:
    """Compute Japanese normalized distance with an explicit dictionary."""

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
    dictionary: JapaneseDictionary,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
    score_cutoff: float | None = None,
) -> float:
    """Alias for Japanese normalized similarity."""

    return dictionary.ratio(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


_SCORERS = ScorerFunctions(
    distance=distance,
    damerau_distance=damerau_distance,
    normalized_distance=normalized_distance,
    normalized_similarity=normalized_similarity,
    ratio=ratio,
)
process = ProcessNamespace(_SCORERS)


def extract(
    query: str,
    choices: Choices,
    *,
    dictionary: JapaneseDictionary,
    scorer: Scorer = "distance",
    limit: int | None = 5,
    score_cutoff: Score | None = None,
    scorer_kwargs: Mapping[str, object] | None = None,
) -> list[ExtractResult]:
    """Return the best matching choices for a query using a Japanese scorer."""

    return process.extract(
        query,
        choices,
        dictionary=dictionary,
        scorer=scorer,
        limit=limit,
        score_cutoff=score_cutoff,
        scorer_kwargs=scorer_kwargs,
    )


def extract_one(
    query: str,
    choices: Choices,
    *,
    dictionary: JapaneseDictionary,
    scorer: Scorer = "distance",
    score_cutoff: Score | None = None,
    scorer_kwargs: Mapping[str, object] | None = None,
) -> ExtractResult | None:
    """Return the best matching choice for a query, or None when none match."""

    return process.extract_one(
        query,
        choices,
        dictionary=dictionary,
        scorer=scorer,
        score_cutoff=score_cutoff,
        scorer_kwargs=scorer_kwargs,
    )


__all__ = [
    "Dictionary",
    "JapaneseDictionary",
    "damerau_distance",
    "distance",
    "extract",
    "extract_one",
    "load_bundle",
    "normalized_distance",
    "normalized_similarity",
    "process",
    "ratio",
    "within_damerau_distance",
    "within_distance",
]
