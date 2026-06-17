from collections.abc import Sequence

__version__: str

_DistanceAlignmentTuple = tuple[int, int, int, int, int]
_RatioAlignmentTuple = tuple[float, int, int, int, int]

class JapaneseDictionary:
    artifact_name: str | None
    source_name: str | None
    reading_field: str | None

    @staticmethod
    def load_payload(path: str, payload_format: str = "yaml") -> "JapaneseDictionary": ...
    @staticmethod
    def load_bundle(metadata_path: str, bundle_dir: str | None = None) -> "JapaneseDictionary": ...
    def distance(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> int: ...
    def damerau_distance(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> int: ...
    def within_distance(
        self,
        left: str,
        right: str,
        threshold: int,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
    ) -> bool: ...
    def within_damerau_distance(
        self,
        left: str,
        right: str,
        threshold: int,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
    ) -> bool: ...
    def normalized_distance(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> float: ...
    def normalized_similarity(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> float: ...
    def ratio(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> float: ...
    def _partial_distance_alignment(
        self,
        query: str,
        text: str,
        max_span_chars: int,
        *,
        max_readings_per_segment: int | None = None,
        reading_max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> _DistanceAlignmentTuple | None: ...
    def _partial_ratio_alignment(
        self,
        query: str,
        text: str,
        max_span_chars: int,
        *,
        max_readings_per_segment: int | None = None,
        reading_max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> _RatioAlignmentTuple | None: ...
    def _cdist_distance(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> list[list[int]]: ...
    def _cdist_damerau_distance(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> list[list[int]]: ...
    def _cdist_normalized_distance(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> list[list[float]]: ...
    def _cdist_normalized_similarity(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> list[list[float]]: ...

class ChineseDictionary:
    artifact_name: str | None
    source_name: str | None
    pinyin_view: str | None

    @staticmethod
    def load_payload(path: str, payload_format: str = "yaml") -> "ChineseDictionary": ...
    @staticmethod
    def load_bundle(metadata_path: str, bundle_dir: str | None = None) -> "ChineseDictionary": ...
    def distance(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> int: ...
    def damerau_distance(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> int: ...
    def within_distance(
        self,
        left: str,
        right: str,
        threshold: int,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
    ) -> bool: ...
    def within_damerau_distance(
        self,
        left: str,
        right: str,
        threshold: int,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
    ) -> bool: ...
    def normalized_distance(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> float: ...
    def normalized_similarity(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> float: ...
    def ratio(
        self,
        left: str,
        right: str,
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> float: ...
    def _partial_distance_alignment(
        self,
        query: str,
        text: str,
        max_span_chars: int,
        *,
        max_readings_per_segment: int | None = None,
        reading_max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> _DistanceAlignmentTuple | None: ...
    def _partial_ratio_alignment(
        self,
        query: str,
        text: str,
        max_span_chars: int,
        *,
        max_readings_per_segment: int | None = None,
        reading_max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> _RatioAlignmentTuple | None: ...
    def _cdist_distance(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> list[list[int]]: ...
    def _cdist_damerau_distance(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: int | None = None,
    ) -> list[list[int]]: ...
    def _cdist_normalized_distance(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> list[list[float]]: ...
    def _cdist_normalized_similarity(
        self,
        queries: Sequence[str],
        choices: Sequence[str],
        *,
        max_readings_per_segment: int | None = None,
        max_span_chars: int | None = None,
        max_paths: int | None = None,
        longest_only: bool | None = None,
        score_cutoff: float | None = None,
    ) -> list[list[float]]: ...

def distance(left: str, right: str, *, score_cutoff: int | None = None) -> int: ...
def damerau_distance(left: str, right: str, *, score_cutoff: int | None = None) -> int: ...
def normalized_distance(left: str, right: str, *, score_cutoff: float | None = None) -> float: ...
def normalized_similarity(left: str, right: str, *, score_cutoff: float | None = None) -> float: ...
def ratio(left: str, right: str, *, score_cutoff: float | None = None) -> float: ...
def _partial_distance_alignment(
    query: str,
    text: str,
    max_span_chars: int,
    *,
    score_cutoff: int | None = None,
) -> _DistanceAlignmentTuple | None: ...
def _partial_ratio_alignment(
    query: str,
    text: str,
    max_span_chars: int,
    *,
    score_cutoff: float | None = None,
) -> _RatioAlignmentTuple | None: ...
def _cdist_distance(
    queries: Sequence[str],
    choices: Sequence[str],
    *,
    score_cutoff: int | None = None,
) -> list[list[int]]: ...
def _cdist_damerau_distance(
    queries: Sequence[str],
    choices: Sequence[str],
    *,
    score_cutoff: int | None = None,
) -> list[list[int]]: ...
def _cdist_normalized_distance(
    queries: Sequence[str],
    choices: Sequence[str],
    *,
    score_cutoff: float | None = None,
) -> list[list[float]]: ...
def _cdist_normalized_similarity(
    queries: Sequence[str],
    choices: Sequence[str],
    *,
    score_cutoff: float | None = None,
) -> list[list[float]]: ...
def distance_paths(
    left_paths: Sequence[str],
    right_paths: Sequence[str],
    *,
    score_cutoff: int | None = None,
) -> int: ...
def damerau_distance_paths(
    left_paths: Sequence[str],
    right_paths: Sequence[str],
    *,
    score_cutoff: int | None = None,
) -> int: ...
def normalized_distance_paths(
    left_paths: Sequence[str],
    right_paths: Sequence[str],
    *,
    score_cutoff: float | None = None,
) -> float: ...
def normalized_similarity_paths(
    left_paths: Sequence[str],
    right_paths: Sequence[str],
    *,
    score_cutoff: float | None = None,
) -> float: ...
def ratio_paths(
    left_paths: Sequence[str],
    right_paths: Sequence[str],
    *,
    score_cutoff: float | None = None,
) -> float: ...
def within_distance_paths(
    left_paths: Sequence[str], right_paths: Sequence[str], threshold: int
) -> bool: ...
def within_damerau_distance_paths(
    left_paths: Sequence[str], right_paths: Sequence[str], threshold: int
) -> bool: ...
