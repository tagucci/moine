import argparse
import statistics
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType

Pair = tuple[str, str]
Scorer = Callable[[str, str], int]

CURATED_PAIRS: tuple[Pair, ...] = (
    ("moine", "モイニャ"),
    ("moine", "モーイン"),
    ("moine", "モアンヌ"),
    ("ブナハーブン", "ぶなはーぶん"),
    ("Bunnahabhain", "ブナハーブン"),
    ("蒸留所", "ジョウリュウショ"),
    ("きめつのやいば", "鬼滅の刃"),
    ("印刷", "いんさt"),
    ("マリトッツォ", "マトリッツォ"),
    ("呪術廻戦", "ジュジュツカイセン"),
)


@dataclass(frozen=True)
class BenchmarkResult:
    label: str
    pair_microseconds: list[float]
    total_seconds: float
    checksum: int

    @property
    def mean_microseconds(self) -> float:
        return statistics.mean(self.pair_microseconds)

    @property
    def stdev_microseconds(self) -> float:
        if len(self.pair_microseconds) == 1:
            return 0.0
        return statistics.stdev(self.pair_microseconds)

    @property
    def total_milliseconds(self) -> float:
        return self.total_seconds * 1_000


@dataclass(frozen=True)
class LoadResult:
    label: str
    timings: list[float]

    @property
    def mean(self) -> float:
        return statistics.mean(self.timings)

    @property
    def stdev(self) -> float:
        if len(self.timings) == 1:
            return 0.0
        return statistics.stdev(self.timings)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Benchmark moine distance scoring against RapidFuzz Levenshtein."
    )
    parser.add_argument(
        "--loops",
        type=positive_int,
        default=10_000,
        help="Number of loops per curated pair (default: 10000).",
    )
    parser.add_argument(
        "--dictionary",
        type=Path,
        help="Path to a moine dictionary bundle or metadata.yaml. Defaults to moine.load_dict search.",
    )
    parser.add_argument(
        "--dictionary-load-repeats",
        type=positive_int,
        default=10,
        help="Number of fresh dictionary loads to time before scoring (default: 10).",
    )
    parser.add_argument(
        "--lang",
        choices=("ja", "ja-unidic", "ja-sudachi"),
        default="ja",
        help="Japanese dictionary selector for moine.load_dict (default: ja).",
    )
    parser.add_argument(
        "--metric",
        choices=("distance", "damerau_distance", "combined_distance"),
        default="distance",
        help="moine dictionary metric to benchmark (default: distance).",
    )
    parser.add_argument(
        "--score-cutoff",
        type=nonnegative_int,
        help="Optional distance score_cutoff forwarded to RapidFuzz and moine distance scorers.",
    )
    parser.add_argument(
        "--include-surface",
        action="store_true",
        help="Also benchmark moine's plain string metric without a dictionary.",
    )
    parser.add_argument(
        "--require-dictionary",
        action="store_true",
        help="Exit with an error instead of printing n/a when no dictionary is available.",
    )
    parser.add_argument(
        "--max-readings-per-segment",
        type=positive_int,
        help="Forwarded to the moine dictionary scorer.",
    )
    parser.add_argument(
        "--max-span-chars",
        type=positive_int,
        help="Forwarded to the moine dictionary scorer.",
    )
    parser.add_argument(
        "--max-paths",
        type=positive_int,
        help="Forwarded to the moine dictionary scorer.",
    )
    parser.add_argument(
        "--longest-only",
        action="store_true",
        default=None,
        help="Forwarded to the moine dictionary scorer.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Write the Markdown report to this path instead of only printing it.",
    )
    return parser.parse_args()


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def nonnegative_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError("must be a non-negative integer")
    return parsed


def import_rapidfuzz_levenshtein() -> ModuleType:
    try:
        from rapidfuzz.distance import Levenshtein
    except ModuleNotFoundError as exc:
        raise SystemExit(
            "RapidFuzz is required for this benchmark. "
            "Run with `uv run --with rapidfuzz python scripts/benchmark_distances.py`."
        ) from exc
    return Levenshtein


def measure(
    label: str,
    scorer: Scorer,
    pairs: tuple[Pair, ...],
    *,
    loops: int,
) -> BenchmarkResult:
    pair_microseconds: list[float] = []
    total_seconds = 0.0
    checksum = 0

    for left, right in pairs:
        checksum += scorer(left, right)
        pair_checksum = 0
        start = time.perf_counter()
        for _loop in range(loops):
            pair_checksum += scorer(left, right)
        elapsed = time.perf_counter() - start
        pair_microseconds.append(elapsed / loops * 1_000_000)
        total_seconds += elapsed
        checksum ^= pair_checksum

    return BenchmarkResult(
        label=label,
        pair_microseconds=pair_microseconds,
        total_seconds=total_seconds,
        checksum=checksum,
    )


def load_dictionary(
    moine: ModuleType, args: argparse.Namespace
) -> tuple[object | None, LoadResult | None, str | None]:
    dictionary: object | None = None
    timings: list[float] = []
    for _repeat in range(args.dictionary_load_repeats):
        start = time.perf_counter()
        try:
            dictionary = moine.load_dict(lang=args.lang, path=args.dictionary)
        except (FileNotFoundError, ValueError) as exc:
            message = f"{type(exc).__name__}: {exc}"
            if args.require_dictionary:
                raise SystemExit(message) from exc
            return None, None, message
        timings.append(time.perf_counter() - start)
    return dictionary, LoadResult("moine dictionary load", timings), None


def dictionary_scorer(dictionary: object, args: argparse.Namespace) -> Scorer:
    scorer = getattr(dictionary, args.metric)
    scorer_kwargs = {
        "max_readings_per_segment": args.max_readings_per_segment,
        "max_span_chars": args.max_span_chars,
        "max_paths": args.max_paths,
        "longest_only": args.longest_only,
        "score_cutoff": args.score_cutoff,
    }

    def score(left: str, right: str) -> int:
        return scorer(left, right, **scorer_kwargs)

    return score


def surface_scorer(moine: ModuleType, metric: str, score_cutoff: int | None) -> Scorer:
    scorer = getattr(moine, metric)

    def score(left: str, right: str) -> int:
        return scorer(left, right, score_cutoff=score_cutoff)

    return score


def rapidfuzz_scorer(levenshtein: ModuleType, score_cutoff: int | None) -> Scorer:
    def score(left: str, right: str) -> int:
        return levenshtein.distance(left, right, score_cutoff=score_cutoff)

    return score


def build_markdown(
    *,
    args: argparse.Namespace,
    moine: ModuleType,
    results: list[BenchmarkResult],
    missing_labels: list[str],
    load_result: LoadResult | None,
    dictionary_error: str | None,
) -> str:
    baseline = results[0].mean_microseconds if results else None
    lines = [
        "# moine Distance Benchmark",
        "",
        f"- pairs: {len(CURATED_PAIRS)}",
        f"- loops / pair: {args.loops}",
        f"- calls: {len(CURATED_PAIRS) * args.loops}",
        "- mean and stddev: pair-level microseconds per call across the curated pairs",
        f"- python: `{sys.implementation.name} {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}`",
        f"- moine: `{getattr(moine, '__file__', 'unknown')}`",
    ]
    extension = getattr(getattr(moine, "_moine", None), "__file__", None)
    if extension is not None:
        lines.append(f"- moine extension: `{extension}`")
    if args.dictionary is not None:
        lines.append(f"- dictionary: `{args.dictionary}`")
    if args.score_cutoff is not None:
        lines.append(f"- score_cutoff: {args.score_cutoff}")
    if dictionary_error is not None:
        lines.append(f"- dictionary benchmark: n/a ({single_line(dictionary_error)})")

    lines.extend(
        [
            "",
            "| Method | mean (±std) | relative |",
            "|---|---:|---:|",
        ]
    )
    for result in results:
        lines.append(
            f"| {result.label} | {format_microseconds(result)} | "
            f"{format_relative(result, baseline)} |"
        )
    for label in missing_labels:
        lines.append(f"| {label} | n/a | n/a |")

    if load_result is not None:
        lines.extend(
            [
                "",
                "| Component | mean (±std) | repeats |",
                "|---|---:|---:|",
                f"| {load_result.label} | {format_load_result(load_result)} | "
                f"{len(load_result.timings)} |",
            ]
        )

    lines.extend(["", "| Method | total time |", "|---|---:|"])
    for result in results:
        lines.append(f"| {result.label} | {result.total_milliseconds:.2f} ms |")
    return "\n".join(lines) + "\n"


def format_ms(seconds: float) -> str:
    return f"{seconds * 1_000:.2f} ms"


def format_load_result(result: LoadResult) -> str:
    return f"{format_ms(result.mean)} ± {format_ms(result.stdev)}"


def format_microseconds(result: BenchmarkResult) -> str:
    return f"{result.mean_microseconds:.2f} ± {result.stdev_microseconds:.2f} us/call"


def format_relative(result: BenchmarkResult, baseline: float | None) -> str:
    if baseline is None:
        return "n/a"
    return f"{result.mean_microseconds / baseline:.2f}x"


def single_line(value: str) -> str:
    return " ".join(value.split())


def main() -> None:
    args = parse_args()

    levenshtein = import_rapidfuzz_levenshtein()
    import moine

    results = [
        measure(
            "RapidFuzz Levenshtein",
            rapidfuzz_scorer(levenshtein, args.score_cutoff),
            CURATED_PAIRS,
            loops=args.loops,
        )
    ]

    if args.include_surface:
        results.append(
            measure(
                f"moine surface {args.metric}",
                surface_scorer(moine, args.metric, args.score_cutoff),
                CURATED_PAIRS,
                loops=args.loops,
            )
        )

    dictionary, load_result, dictionary_error = load_dictionary(moine, args)
    missing_labels: list[str] = []
    if dictionary is not None:
        results.append(
            measure(
                f"moine {args.lang} {args.metric}",
                dictionary_scorer(dictionary, args),
                CURATED_PAIRS,
                loops=args.loops,
            )
        )
    else:
        missing_labels.append(f"moine {args.lang} {args.metric}")

    report = build_markdown(
        args=args,
        moine=moine,
        results=results,
        missing_labels=missing_labels,
        load_result=load_result,
        dictionary_error=dictionary_error,
    )
    if args.output is not None:
        args.output.write_text(report, encoding="utf-8")
    print(report, end="")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
