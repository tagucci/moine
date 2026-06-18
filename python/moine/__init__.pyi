from collections.abc import Iterable
from os import PathLike
from typing import Literal, NamedTuple, overload

from ._moine import ChineseDictionary as ChineseDictionary
from ._moine import JapaneseDictionary as JapaneseDictionary
from .ja import Dictionary as Dictionary

Language = Literal["ja", "ja-unidic", "ja-sudachi", "zh"]
Metric = Literal[
    "distance",
    "damerau_distance",
    "combined_distance",
    "normalized_distance",
    "normalized_similarity",
    "ratio",
]
PartialMetric = Literal["distance", "ratio"]
_Dictionary = JapaneseDictionary | ChineseDictionary
__version__: str

class PartialAlignment(NamedTuple):
    score: int | float
    src_start: int
    src_end: int
    dest_start: int
    dest_end: int

def load_dict(*, lang: Language, path: str | PathLike[str] | None = None) -> _Dictionary: ...
def distance_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], *, score_cutoff: int | None = None
) -> int: ...
def damerau_distance_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], *, score_cutoff: int | None = None
) -> int: ...
def normalized_distance_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], *, score_cutoff: float | None = None
) -> float: ...
def normalized_similarity_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], *, score_cutoff: float | None = None
) -> float: ...
def ratio_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], *, score_cutoff: float | None = None
) -> float: ...
def within_distance_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], threshold: int
) -> bool: ...
def within_damerau_distance_paths(
    left_paths: Iterable[str], right_paths: Iterable[str], threshold: int
) -> bool: ...
def set_default_dictionary(dictionary: _Dictionary) -> None: ...
def clear_default_dictionary(*, lang: Language) -> None: ...
def get_default_dictionary(*, lang: Language) -> _Dictionary | None: ...
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
) -> list[list[int]] | list[list[float]]: ...
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
) -> float: ...
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
) -> int: ...
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
) -> PartialAlignment | None: ...
@overload
def distance(left: str, right: str, *, score_cutoff: int | None = None) -> int: ...
@overload
def distance(
    left: str,
    right: str,
    *,
    lang: Language,
    dictionary: _Dictionary | None = None,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int: ...
@overload
def distance(
    left: str,
    right: str,
    *,
    dictionary: _Dictionary,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int: ...
@overload
def damerau_distance(left: str, right: str, *, score_cutoff: int | None = None) -> int: ...
@overload
def damerau_distance(
    left: str,
    right: str,
    *,
    lang: Language,
    dictionary: _Dictionary | None = None,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int: ...
@overload
def damerau_distance(
    left: str,
    right: str,
    *,
    dictionary: _Dictionary,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int: ...
@overload
def combined_distance(left: str, right: str, *, score_cutoff: int | None = None) -> int: ...
@overload
def combined_distance(
    left: str,
    right: str,
    *,
    lang: Language,
    dictionary: _Dictionary | None = None,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int: ...
@overload
def combined_distance(
    left: str,
    right: str,
    *,
    dictionary: _Dictionary,
    score_cutoff: int | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> int: ...
@overload
def normalized_distance(left: str, right: str, *, score_cutoff: float | None = None) -> float: ...
@overload
def normalized_distance(
    left: str,
    right: str,
    *,
    lang: Language,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float: ...
@overload
def normalized_distance(
    left: str,
    right: str,
    *,
    dictionary: _Dictionary,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float: ...
@overload
def normalized_similarity(left: str, right: str, *, score_cutoff: float | None = None) -> float: ...
@overload
def normalized_similarity(
    left: str,
    right: str,
    *,
    lang: Language,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float: ...
@overload
def normalized_similarity(
    left: str,
    right: str,
    *,
    dictionary: _Dictionary,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float: ...
@overload
def ratio(left: str, right: str, *, score_cutoff: float | None = None) -> float: ...
@overload
def ratio(
    left: str,
    right: str,
    *,
    lang: Language,
    dictionary: _Dictionary | None = None,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float: ...
@overload
def ratio(
    left: str,
    right: str,
    *,
    dictionary: _Dictionary,
    score_cutoff: float | None = None,
    max_readings_per_segment: int | None = None,
    max_span_chars: int | None = None,
    max_paths: int | None = None,
    longest_only: bool | None = None,
) -> float: ...
