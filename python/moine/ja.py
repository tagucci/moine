"""Japanese helpers for moine."""

import os
from collections.abc import Iterable, Mapping
from dataclasses import dataclass
from os import PathLike
from typing import Literal

from ._moine import JapaneseDictionary

StrPath = str | PathLike[str]
Score = int | float
ChoiceKey = object
Choices = Iterable[str] | Mapping[ChoiceKey, str]
ExtractResult = tuple[str, Score, ChoiceKey]
Scorer = Literal[
    "distance",
    "damerau_distance",
    "normalized_distance",
    "normalized_similarity",
    "ratio",
]
ScorerKind = Literal["distance", "similarity"]


@dataclass(frozen=True)
class _ReadingOptions:
    max_readings_per_segment: int | None = None
    max_span_chars: int | None = None
    max_paths: int | None = None
    longest_only: bool | None = None


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


class _ProcessNamespace:
    def extract(
        self,
        query: str,
        choices: Choices,
        *,
        dictionary: JapaneseDictionary,
        scorer: Scorer = "distance",
        limit: int | None = 5,
        score_cutoff: Score | None = None,
        scorer_kwargs: Mapping[str, object] | None = None,
    ) -> list[ExtractResult]:
        reading_options = _reading_options(scorer_kwargs)
        scorer_kind = _scorer_kind(scorer)
        score_cutoff = _validate_score_cutoff(score_cutoff, scorer)
        limit = _validate_limit(limit)
        results = []
        for order, (choice, key) in enumerate(_iter_choices(choices)):
            score = _score_choice(
                query,
                choice,
                dictionary=dictionary,
                scorer=scorer,
                score_cutoff=score_cutoff,
                reading_options=reading_options,
            )
            if _passes_score_cutoff(score, score_cutoff, scorer_kind):
                results.append((choice, score, key, order))

        if scorer_kind == "distance":
            results.sort(key=lambda item: (item[1], item[3]))
        else:
            results.sort(key=lambda item: (-item[1], item[3]))

        trimmed = results if limit is None else results[:limit]
        return [(choice, score, key) for choice, score, key, _order in trimmed]

    def extract_one(
        self,
        query: str,
        choices: Choices,
        *,
        dictionary: JapaneseDictionary,
        scorer: Scorer = "distance",
        score_cutoff: Score | None = None,
        scorer_kwargs: Mapping[str, object] | None = None,
    ) -> ExtractResult | None:
        results = self.extract(
            query,
            choices,
            dictionary=dictionary,
            scorer=scorer,
            limit=1,
            score_cutoff=score_cutoff,
            scorer_kwargs=scorer_kwargs,
        )
        return results[0] if results else None


process = _ProcessNamespace()


def _iter_choices(choices: Choices) -> Iterable[tuple[str, ChoiceKey]]:
    if isinstance(choices, Mapping):
        for key, choice in choices.items():
            yield choice, key
    else:
        for index, choice in enumerate(choices):
            yield choice, index


def _scorer_kind(scorer: Scorer) -> ScorerKind:
    if scorer in {"distance", "damerau_distance"}:
        return "distance"
    if scorer == "normalized_distance":
        return "distance"
    if scorer in {"normalized_similarity", "ratio"}:
        return "similarity"
    raise ValueError(
        "scorer must be 'distance', 'damerau_distance', 'normalized_distance', "
        "'normalized_similarity', or 'ratio'"
    )


def _validate_score_cutoff(score_cutoff: Score | None, scorer: Scorer) -> Score | None:
    if score_cutoff is None:
        return None
    if scorer in {"distance", "damerau_distance"}:
        if isinstance(score_cutoff, bool) or not isinstance(score_cutoff, int):
            raise TypeError("score_cutoff must be an int for distance scorers")
        if score_cutoff < 0:
            raise ValueError("score_cutoff must be >= 0 for distance scorers")
        return score_cutoff
    if isinstance(score_cutoff, bool) or not isinstance(score_cutoff, int | float):
        raise TypeError("score_cutoff must be a float for normalized scorers")
    score_cutoff = float(score_cutoff)
    if not 0.0 <= score_cutoff <= 1.0:
        raise ValueError("score_cutoff must be between 0.0 and 1.0")
    return score_cutoff


def _validate_limit(limit: int | None) -> int | None:
    if limit is None:
        return None
    if isinstance(limit, bool) or not isinstance(limit, int):
        raise TypeError("limit must be an int or None")
    return max(limit, 0)


def _reading_options(scorer_kwargs: Mapping[str, object] | None) -> _ReadingOptions:
    if scorer_kwargs is None:
        return _ReadingOptions()

    allowed = {
        "max_readings_per_segment",
        "max_span_chars",
        "max_paths",
        "longest_only",
    }
    for name in scorer_kwargs:
        if name not in allowed:
            raise TypeError(f"unexpected scorer_kwargs key {name!r}")

    return _ReadingOptions(
        max_readings_per_segment=_optional_int(
            scorer_kwargs.get("max_readings_per_segment"),
            "max_readings_per_segment",
        ),
        max_span_chars=_optional_int(scorer_kwargs.get("max_span_chars"), "max_span_chars"),
        max_paths=_optional_int(scorer_kwargs.get("max_paths"), "max_paths"),
        longest_only=_optional_bool(scorer_kwargs.get("longest_only"), "longest_only"),
    )


def _optional_int(value: object, name: str) -> int | None:
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"scorer_kwargs[{name!r}] must be an int or None")
    if value < 0:
        raise ValueError(f"scorer_kwargs[{name!r}] must be >= 0")
    return value


def _optional_bool(value: object, name: str) -> bool | None:
    if value is None:
        return None
    if not isinstance(value, bool):
        raise TypeError(f"scorer_kwargs[{name!r}] must be a bool or None")
    return value


def _distance_cutoff(score_cutoff: Score | None) -> int | None:
    if score_cutoff is None:
        return None
    if isinstance(score_cutoff, bool) or not isinstance(score_cutoff, int):
        raise AssertionError("distance score_cutoff was not validated")
    return score_cutoff


def _normalized_cutoff(score_cutoff: Score | None) -> float | None:
    if score_cutoff is None:
        return None
    if isinstance(score_cutoff, bool) or not isinstance(score_cutoff, int | float):
        raise AssertionError("normalized score_cutoff was not validated")
    return float(score_cutoff)


def _score_choice(
    query: str,
    choice: str,
    *,
    dictionary: JapaneseDictionary,
    scorer: Scorer,
    score_cutoff: Score | None,
    reading_options: _ReadingOptions,
) -> Score:
    if scorer == "distance":
        return distance(
            query,
            choice,
            dictionary=dictionary,
            max_readings_per_segment=reading_options.max_readings_per_segment,
            max_span_chars=reading_options.max_span_chars,
            max_paths=reading_options.max_paths,
            longest_only=reading_options.longest_only,
            score_cutoff=_distance_cutoff(score_cutoff),
        )
    if scorer == "damerau_distance":
        return damerau_distance(
            query,
            choice,
            dictionary=dictionary,
            max_readings_per_segment=reading_options.max_readings_per_segment,
            max_span_chars=reading_options.max_span_chars,
            max_paths=reading_options.max_paths,
            longest_only=reading_options.longest_only,
            score_cutoff=_distance_cutoff(score_cutoff),
        )
    if scorer == "normalized_similarity":
        return normalized_similarity(
            query,
            choice,
            dictionary=dictionary,
            max_readings_per_segment=reading_options.max_readings_per_segment,
            max_span_chars=reading_options.max_span_chars,
            max_paths=reading_options.max_paths,
            longest_only=reading_options.longest_only,
            score_cutoff=_normalized_cutoff(score_cutoff),
        )
    if scorer == "normalized_distance":
        return normalized_distance(
            query,
            choice,
            dictionary=dictionary,
            max_readings_per_segment=reading_options.max_readings_per_segment,
            max_span_chars=reading_options.max_span_chars,
            max_paths=reading_options.max_paths,
            longest_only=reading_options.longest_only,
            score_cutoff=_normalized_cutoff(score_cutoff),
        )
    if scorer == "ratio":
        return ratio(
            query,
            choice,
            dictionary=dictionary,
            max_readings_per_segment=reading_options.max_readings_per_segment,
            max_span_chars=reading_options.max_span_chars,
            max_paths=reading_options.max_paths,
            longest_only=reading_options.longest_only,
            score_cutoff=_normalized_cutoff(score_cutoff),
        )
    raise ValueError(
        "scorer must be 'distance', 'damerau_distance', 'normalized_distance', "
        "'normalized_similarity', or 'ratio'"
    )


def _passes_score_cutoff(score: Score, score_cutoff: Score | None, scorer_kind: ScorerKind) -> bool:
    if score_cutoff is None:
        return True
    if scorer_kind == "distance":
        return score <= score_cutoff
    return score >= score_cutoff


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
