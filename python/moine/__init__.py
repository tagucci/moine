"""Python entry points for moine."""

import os
from collections.abc import Iterable
from importlib.metadata import PackageNotFoundError
from importlib.metadata import version as _metadata_version
from pathlib import Path
from threading import RLock
from typing import Literal, NamedTuple

from . import _moine
from ._artifacts import default_search_roots
from ._moine import (
    ChineseDictionary,
    JapaneseDictionary,
)
from .ja import Dictionary

Language = Literal["ja", "zh"]
Metric = Literal[
    "distance",
    "damerau_distance",
    "normalized_distance",
    "normalized_similarity",
    "ratio",
]
PartialMetric = Literal["distance", "ratio"]
_Dictionary = JapaneseDictionary | ChineseDictionary

_DEFAULT_DICTIONARIES: dict[str, _Dictionary] = {}
_DEFAULT_DICTIONARIES_LOCK = RLock()
_LANGUAGE_ENV_VARS = {
    "ja": "MOINE_JA_DICTIONARY",
    "zh": "MOINE_ZH_DICTIONARY",
}
_LANGUAGE_DEFAULT_PREFIXES = {
    "ja": ("moine-unidic",),
    "zh": ("moine-cedict",),
}
try:
    __version__ = _metadata_version("moine")
except PackageNotFoundError:
    __version__ = _moine.__version__


class PartialAlignment(NamedTuple):
    """Best alignment of a query against a span in a longer text."""

    score: int | float
    src_start: int
    src_end: int
    dest_start: int
    dest_end: int


def load_dict(
    *,
    lang: Language,
    path: str | os.PathLike[str] | None = None,
) -> _Dictionary:
    """Load a language dictionary from an explicit or installed/default artifact."""

    lang = _normalize_lang(lang)
    artifact_path = Path(path) if path is not None else _find_default_dictionary_path(lang)
    if artifact_path is None:
        env_var = _LANGUAGE_ENV_VARS[lang]
        raise FileNotFoundError(
            f"No default {lang!r} dictionary artifact found. "
            f"Run `uv run python -m moine download {lang}` "
            f"(or `python -m moine download {lang}` from an active environment), "
            f"pass path=..., set {env_var}, "
            "or add a bundle to MOINE_DICTIONARIES_PATH."
        )
    if lang == "ja":
        return JapaneseDictionary.load_bundle(os.fspath(artifact_path))
    if lang == "zh":
        return ChineseDictionary.load_bundle(os.fspath(artifact_path))
    raise AssertionError("unreachable language branch")


def distance_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    *,
    score_cutoff: int | None = None,
) -> int:
    """Compute distance between two explicit path sets."""

    return _moine.distance_paths(
        list(left_paths),
        list(right_paths),
        score_cutoff=_distance_score_cutoff(score_cutoff),
    )


def damerau_distance_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    *,
    score_cutoff: int | None = None,
) -> int:
    """Compute Damerau distance between two explicit path sets."""

    return _moine.damerau_distance_paths(
        list(left_paths),
        list(right_paths),
        score_cutoff=_distance_score_cutoff(score_cutoff),
    )


def normalized_distance_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    *,
    score_cutoff: float | None = None,
) -> float:
    """Compute normalized distance between two explicit path sets."""

    return _moine.normalized_distance_paths(
        list(left_paths),
        list(right_paths),
        score_cutoff=_normalized_score_cutoff(score_cutoff),
    )


def normalized_similarity_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    *,
    score_cutoff: float | None = None,
) -> float:
    """Compute normalized similarity between two explicit path sets."""

    return _moine.normalized_similarity_paths(
        list(left_paths),
        list(right_paths),
        score_cutoff=_normalized_score_cutoff(score_cutoff),
    )


def ratio_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    *,
    score_cutoff: float | None = None,
) -> float:
    """Alias for normalized similarity between two explicit path sets."""

    return normalized_similarity_paths(left_paths, right_paths, score_cutoff=score_cutoff)


def within_distance_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    threshold: int,
) -> bool:
    """Return whether explicit path-set distance is within threshold."""

    return _moine.within_distance_paths(
        list(left_paths),
        list(right_paths),
        _distance_threshold(threshold),
    )


def within_damerau_distance_paths(
    left_paths: Iterable[str],
    right_paths: Iterable[str],
    threshold: int,
) -> bool:
    """Return whether explicit path-set Damerau distance is within threshold."""

    return _moine.within_damerau_distance_paths(
        list(left_paths),
        list(right_paths),
        _distance_threshold(threshold),
    )


def set_default_dictionary(dictionary: _Dictionary) -> None:
    """Register the default dictionary for its language."""

    lang = _dictionary_lang(dictionary)
    with _DEFAULT_DICTIONARIES_LOCK:
        _DEFAULT_DICTIONARIES[lang] = dictionary


def clear_default_dictionary(*, lang: Language) -> None:
    """Clear the configured default dictionary for a language."""

    lang = _normalize_lang(lang)
    with _DEFAULT_DICTIONARIES_LOCK:
        _DEFAULT_DICTIONARIES.pop(lang, None)


def get_default_dictionary(*, lang: Language) -> _Dictionary | None:
    """Return the configured default dictionary for a language, if any."""

    lang = _normalize_lang(lang)
    with _DEFAULT_DICTIONARIES_LOCK:
        return _DEFAULT_DICTIONARIES.get(lang)


def distance(
    left: str,
    right: str,
    *,
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int:
    """Compute string or language-aware lattice distance."""

    if lang is None and dictionary is None:
        _reject_dictionary_options(
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
        )
        return _moine.distance(left, right, score_cutoff=_distance_score_cutoff(score_cutoff))

    dictionary = _resolve_dictionary(lang=lang, dictionary=dictionary)
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
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int:
    """Compute string or language-aware lattice-side Damerau distance."""

    if lang is None and dictionary is None:
        _reject_dictionary_options(
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
        )
        return _moine.damerau_distance(
            left,
            right,
            score_cutoff=_distance_score_cutoff(score_cutoff),
        )

    dictionary = _resolve_dictionary(lang=lang, dictionary=dictionary)
    return dictionary.damerau_distance(
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
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float:
    """Compute normalized string or language-aware lattice distance."""

    if lang is None and dictionary is None:
        _reject_dictionary_options(
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
        )
        return _moine.normalized_distance(
            left,
            right,
            score_cutoff=_normalized_score_cutoff(score_cutoff),
        )

    dictionary = _resolve_dictionary(lang=lang, dictionary=dictionary)
    return dictionary.normalized_distance(
        left,
        right,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=score_cutoff,
    )


def normalized_similarity(
    left: str,
    right: str,
    *,
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float:
    """Compute normalized string or language-aware lattice similarity."""

    if lang is None and dictionary is None:
        _reject_dictionary_options(
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
        )
        return _moine.normalized_similarity(
            left,
            right,
            score_cutoff=_normalized_score_cutoff(score_cutoff),
        )

    dictionary = _resolve_dictionary(lang=lang, dictionary=dictionary)
    return dictionary.normalized_similarity(
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
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float:
    """Alias for normalized similarity."""

    return normalized_similarity(
        left,
        right,
        lang=lang,
        dictionary=dictionary,
        score_cutoff=score_cutoff,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
    )


def partial_ratio(
    query: str,
    text: str,
    *,
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_span_chars: int | None = None,
    max_reading_span_chars: int | None = None,
    max_readings_per_segment: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float:
    """Return the best normalized similarity between query and a text span."""

    alignment = partial_alignment(
        query,
        text,
        lang=lang,
        dictionary=dictionary,
        metric="ratio",
        score_cutoff=score_cutoff,
        max_span_chars=max_span_chars,
        max_reading_span_chars=max_reading_span_chars,
        max_readings_per_segment=max_readings_per_segment,
        max_paths=max_paths,
        longest_only=longest_only,
    )
    if alignment is None:
        return 0.0
    return float(alignment.score)


def partial_distance(
    query: str,
    text: str,
    *,
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    score_cutoff: int | None = None,
    max_span_chars: int | None = None,
    max_reading_span_chars: int | None = None,
    max_readings_per_segment: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int:
    """Return the best distance between query and a span in text."""

    alignment = partial_alignment(
        query,
        text,
        lang=lang,
        dictionary=dictionary,
        metric="distance",
        score_cutoff=score_cutoff,
        max_span_chars=max_span_chars,
        max_reading_span_chars=max_reading_span_chars,
        max_readings_per_segment=max_readings_per_segment,
        max_paths=max_paths,
        longest_only=longest_only,
    )
    if alignment is None:
        if score_cutoff is None:
            return len(query)
        return score_cutoff + 1
    return int(alignment.score)


def partial_alignment(
    query: str,
    text: str,
    *,
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    metric: PartialMetric = "ratio",
    score_cutoff: int | float | None = None,
    max_span_chars: int | None = None,
    max_reading_span_chars: int | None = None,
    max_readings_per_segment: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> PartialAlignment | None:
    """Return the best alignment between query and a span in text."""

    metric = _normalize_partial_metric(metric)

    if lang is None and dictionary is None:
        span_limit = _partial_span_limit(query, max_span_chars)
        _reject_dictionary_options(
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_reading_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
        )
        if metric == "distance":
            cutoff = _distance_score_cutoff(score_cutoff)
            return _partial_alignment_from_tuple(
                _moine._partial_distance_alignment(
                    query,
                    text,
                    span_limit,
                    score_cutoff=cutoff,
                )
            )
        cutoff = _normalized_score_cutoff(score_cutoff)
        return _partial_alignment_from_tuple(
            _moine._partial_ratio_alignment(
                query,
                text,
                span_limit,
                score_cutoff=cutoff,
            )
        )

    resolved_dictionary = _resolve_dictionary(lang=lang, dictionary=dictionary)
    span_limit = 0 if max_span_chars is None else _partial_span_limit(query, max_span_chars)
    if metric == "distance":
        cutoff = _distance_score_cutoff(score_cutoff)
        return _partial_alignment_from_tuple(
            resolved_dictionary._partial_distance_alignment(
                query,
                text,
                span_limit,
                max_readings_per_segment=max_readings_per_segment,
                reading_max_span_chars=max_reading_span_chars,
                max_paths=max_paths,
                longest_only=longest_only,
                score_cutoff=cutoff,
            )
        )
    cutoff = _normalized_score_cutoff(score_cutoff)
    return _partial_alignment_from_tuple(
        resolved_dictionary._partial_ratio_alignment(
            query,
            text,
            span_limit,
            max_readings_per_segment=max_readings_per_segment,
            reading_max_span_chars=max_reading_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
            score_cutoff=cutoff,
        )
    )


def cdist(
    queries: Iterable[str],
    choices: Iterable[str],
    *,
    lang: Language | None = None,
    dictionary: _Dictionary | None = None,
    metric: Metric = "distance",
    score_cutoff: int | float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> list[list[int]] | list[list[float]]:
    """Compute a cross-distance or cross-similarity matrix for a language."""

    metric = _normalize_metric(metric)
    query_list = list(queries)
    choice_list = list(choices)

    if lang is None and dictionary is None:
        _reject_dictionary_options(
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
        )
        if metric == "distance":
            return _moine._cdist_distance(
                query_list,
                choice_list,
                score_cutoff=_distance_score_cutoff(score_cutoff),
            )
        if metric == "damerau_distance":
            return _moine._cdist_damerau_distance(
                query_list,
                choice_list,
                score_cutoff=_distance_score_cutoff(score_cutoff),
            )
        if metric == "normalized_distance":
            return _moine._cdist_normalized_distance(
                query_list,
                choice_list,
                score_cutoff=_normalized_score_cutoff(score_cutoff),
            )
        return _moine._cdist_normalized_similarity(
            query_list,
            choice_list,
            score_cutoff=_normalized_score_cutoff(score_cutoff),
        )

    dictionary = _resolve_dictionary(lang=lang, dictionary=dictionary)
    if metric == "distance":
        return dictionary._cdist_distance(
            query_list,
            choice_list,
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
            score_cutoff=_distance_score_cutoff(score_cutoff),
        )
    if metric == "damerau_distance":
        return dictionary._cdist_damerau_distance(
            query_list,
            choice_list,
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
            score_cutoff=_distance_score_cutoff(score_cutoff),
        )
    if metric == "normalized_distance":
        return dictionary._cdist_normalized_distance(
            query_list,
            choice_list,
            max_readings_per_segment=max_readings_per_segment,
            max_span_chars=max_span_chars,
            max_paths=max_paths,
            longest_only=longest_only,
            score_cutoff=_normalized_score_cutoff(score_cutoff),
        )

    return dictionary._cdist_normalized_similarity(
        query_list,
        choice_list,
        max_readings_per_segment=max_readings_per_segment,
        max_span_chars=max_span_chars,
        max_paths=max_paths,
        longest_only=longest_only,
        score_cutoff=_normalized_score_cutoff(score_cutoff),
    )


def _resolve_dictionary(
    *,
    lang: Language | None,
    dictionary: _Dictionary | None,
) -> _Dictionary:
    if dictionary is not None:
        if lang is not None:
            _validate_dictionary(_normalize_lang(lang), dictionary)
        return dictionary

    if lang is None:
        raise TypeError("lang is required when dictionary is not provided")

    lang = _normalize_lang(lang)
    with _DEFAULT_DICTIONARIES_LOCK:
        default_dictionary = _DEFAULT_DICTIONARIES.get(lang)
        if default_dictionary is None:
            default_dictionary = load_dict(lang=lang)
            _DEFAULT_DICTIONARIES[lang] = default_dictionary
        return default_dictionary


def _normalize_lang(lang: str) -> Language:
    if lang == "ja":
        return "ja"
    if lang == "zh":
        return "zh"
    raise ValueError("lang must be 'ja' or 'zh'")


def _normalize_metric(metric: str) -> Metric:
    if metric == "distance":
        return "distance"
    if metric == "damerau_distance":
        return "damerau_distance"
    if metric == "normalized_distance":
        return "normalized_distance"
    if metric == "normalized_similarity":
        return "normalized_similarity"
    if metric == "ratio":
        return "ratio"
    raise ValueError(
        "metric must be 'distance', 'damerau_distance', "
        "'normalized_distance', 'normalized_similarity', or 'ratio'"
    )


def _normalize_partial_metric(metric: str) -> PartialMetric:
    if metric == "distance":
        return "distance"
    if metric == "ratio":
        return "ratio"
    raise ValueError("metric must be 'distance' or 'ratio'")


def _partial_span_limit(query: str, max_span_chars: int | None) -> int:
    if max_span_chars is not None:
        if isinstance(max_span_chars, bool) or not isinstance(max_span_chars, int):
            raise TypeError("max_span_chars must be an int or None")
        if max_span_chars <= 0:
            raise ValueError("max_span_chars must be > 0")
        return max_span_chars

    query_chars = len(query)
    if query_chars == 0:
        return 0
    return max(query_chars * 2, query_chars + 4)


def _partial_alignment_from_tuple(
    alignment: tuple[int | float, int, int, int, int] | None,
) -> PartialAlignment | None:
    if alignment is None:
        return None
    return PartialAlignment(*alignment)


def _reject_dictionary_options(
    *,
    max_readings_per_segment: int | None,
    max_span_chars: int | None,
    max_paths: int | None,
    longest_only: bool | None,
) -> None:
    options = {
        "max_readings_per_segment": max_readings_per_segment,
        "max_span_chars": max_span_chars,
        "max_paths": max_paths,
        "longest_only": longest_only,
    }
    for name, value in options.items():
        if value is not None:
            raise TypeError(f"{name} requires lang or dictionary")


def _distance_score_cutoff(score_cutoff: int | float | None) -> int | None:
    if score_cutoff is None:
        return None
    if isinstance(score_cutoff, bool) or not isinstance(score_cutoff, int):
        raise TypeError("score_cutoff must be an int for metric='distance'")
    if score_cutoff < 0:
        raise ValueError("score_cutoff must be >= 0 for metric='distance'")
    return score_cutoff


def _distance_threshold(threshold: int) -> int:
    if isinstance(threshold, bool) or not isinstance(threshold, int):
        raise TypeError("threshold must be an int")
    if threshold < 0:
        raise ValueError("threshold must be >= 0")
    return threshold


def _normalized_score_cutoff(score_cutoff: int | float | None) -> float | None:
    if score_cutoff is None:
        return None
    if isinstance(score_cutoff, bool) or not isinstance(score_cutoff, int | float):
        raise TypeError("score_cutoff must be a float for normalized metrics")
    cutoff = float(score_cutoff)
    if not 0.0 <= cutoff <= 1.0:
        raise ValueError("score_cutoff must be between 0.0 and 1.0")
    return cutoff


def _validate_dictionary(lang: Language, dictionary: _Dictionary) -> None:
    if lang == "ja" and not isinstance(dictionary, JapaneseDictionary):
        raise TypeError("dictionary must be JapaneseDictionary for lang='ja'")
    if lang == "zh" and not isinstance(dictionary, ChineseDictionary):
        raise TypeError("dictionary must be ChineseDictionary for lang='zh'")


def _dictionary_lang(dictionary: _Dictionary) -> Language:
    if isinstance(dictionary, JapaneseDictionary):
        return "ja"
    if isinstance(dictionary, ChineseDictionary):
        return "zh"
    raise TypeError("dictionary must be JapaneseDictionary or ChineseDictionary")


def _find_default_dictionary_path(lang: Language) -> Path | None:
    env_path = os.environ.get(_LANGUAGE_ENV_VARS[lang])
    if env_path:
        return Path(env_path)

    search_path = os.environ.get("MOINE_DICTIONARIES_PATH")
    raw_roots = search_path.split(os.pathsep) if search_path else []
    roots = [Path(raw_root).expanduser() for raw_root in raw_roots if raw_root]
    roots.extend(default_search_roots())

    for root in roots:
        metadata = _metadata_path(root)
        if metadata is not None:
            return metadata
        match = _find_child_bundle(root, lang)
        if match is not None:
            return match
    return None


def _metadata_path(path: Path) -> Path | None:
    if path.is_file() and path.name == "metadata.yaml":
        return path
    metadata = path / "metadata.yaml"
    if metadata.is_file():
        return metadata
    return None


def _find_child_bundle(root: Path, lang: Language) -> Path | None:
    if not root.is_dir():
        return None
    prefixes = _LANGUAGE_DEFAULT_PREFIXES[lang]
    for child in sorted(root.iterdir()):
        if not child.is_dir() or not child.name.startswith(prefixes):
            continue
        metadata = _metadata_path(child)
        if metadata is not None:
            return metadata
    return None


__all__ = [
    "ChineseDictionary",
    "Metric",
    "PartialAlignment",
    "PartialMetric",
    "__version__",
    "cdist",
    "clear_default_dictionary",
    "Dictionary",
    "JapaneseDictionary",
    "damerau_distance",
    "damerau_distance_paths",
    "distance",
    "distance_paths",
    "get_default_dictionary",
    "load_dict",
    "normalized_distance",
    "normalized_distance_paths",
    "normalized_similarity",
    "normalized_similarity_paths",
    "partial_alignment",
    "partial_distance",
    "partial_ratio",
    "ratio",
    "ratio_paths",
    "set_default_dictionary",
    "within_damerau_distance_paths",
    "within_distance_paths",
]
