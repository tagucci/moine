"""Shared language-specific candidate extraction helpers."""

from collections.abc import Callable, Iterable, Mapping
from dataclasses import dataclass
from typing import Literal

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
class ReadingOptions:
    max_readings_per_segment: int | None = None
    max_span_chars: int | None = None
    max_paths: int | None = None
    longest_only: bool | None = None


@dataclass(frozen=True)
class ScorerFunctions:
    distance: Callable[..., int]
    damerau_distance: Callable[..., int]
    normalized_distance: Callable[..., float]
    normalized_similarity: Callable[..., float]
    ratio: Callable[..., float]


class ProcessNamespace:
    def __init__(self, scorers: ScorerFunctions) -> None:
        self._scorers = scorers

    def extract(
        self,
        query: str,
        choices: Choices,
        *,
        dictionary: object,
        scorer: Scorer = "distance",
        limit: int | None = 5,
        score_cutoff: Score | None = None,
        scorer_kwargs: Mapping[str, object] | None = None,
    ) -> list[ExtractResult]:
        return extract(
            query,
            choices,
            dictionary=dictionary,
            scorers=self._scorers,
            scorer=scorer,
            limit=limit,
            score_cutoff=score_cutoff,
            scorer_kwargs=scorer_kwargs,
        )

    def extract_one(
        self,
        query: str,
        choices: Choices,
        *,
        dictionary: object,
        scorer: Scorer = "distance",
        score_cutoff: Score | None = None,
        scorer_kwargs: Mapping[str, object] | None = None,
    ) -> ExtractResult | None:
        return extract_one(
            query,
            choices,
            dictionary=dictionary,
            scorers=self._scorers,
            scorer=scorer,
            score_cutoff=score_cutoff,
            scorer_kwargs=scorer_kwargs,
        )


def extract(
    query: str,
    choices: Choices,
    *,
    dictionary: object,
    scorers: ScorerFunctions,
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
            scorers=scorers,
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
    query: str,
    choices: Choices,
    *,
    dictionary: object,
    scorers: ScorerFunctions,
    scorer: Scorer = "distance",
    score_cutoff: Score | None = None,
    scorer_kwargs: Mapping[str, object] | None = None,
) -> ExtractResult | None:
    results = extract(
        query,
        choices,
        dictionary=dictionary,
        scorers=scorers,
        scorer=scorer,
        limit=1,
        score_cutoff=score_cutoff,
        scorer_kwargs=scorer_kwargs,
    )
    return results[0] if results else None


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


def _reading_options(scorer_kwargs: Mapping[str, object] | None) -> ReadingOptions:
    if scorer_kwargs is None:
        return ReadingOptions()

    allowed = {
        "max_readings_per_segment",
        "max_span_chars",
        "max_paths",
        "longest_only",
    }
    for name in scorer_kwargs:
        if name not in allowed:
            raise TypeError(f"unexpected scorer_kwargs key {name!r}")

    return ReadingOptions(
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
    dictionary: object,
    scorers: ScorerFunctions,
    scorer: Scorer,
    score_cutoff: Score | None,
    reading_options: ReadingOptions,
) -> Score:
    kwargs = {
        "dictionary": dictionary,
        "max_readings_per_segment": reading_options.max_readings_per_segment,
        "max_span_chars": reading_options.max_span_chars,
        "max_paths": reading_options.max_paths,
        "longest_only": reading_options.longest_only,
    }
    if scorer == "distance":
        return scorers.distance(
            query,
            choice,
            **kwargs,
            score_cutoff=_distance_cutoff(score_cutoff),
        )
    if scorer == "damerau_distance":
        return scorers.damerau_distance(
            query,
            choice,
            **kwargs,
            score_cutoff=_distance_cutoff(score_cutoff),
        )
    if scorer == "normalized_similarity":
        return scorers.normalized_similarity(
            query,
            choice,
            **kwargs,
            score_cutoff=_normalized_cutoff(score_cutoff),
        )
    if scorer == "normalized_distance":
        return scorers.normalized_distance(
            query,
            choice,
            **kwargs,
            score_cutoff=_normalized_cutoff(score_cutoff),
        )
    if scorer == "ratio":
        return scorers.ratio(
            query,
            choice,
            **kwargs,
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
