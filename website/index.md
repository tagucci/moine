# mòine

`mòine` is a Python and Rust library for romanization-aware string comparison.
It implements [Lattice Path Edit Distance (Kaji, 2023)](https://aclanthology.org/2023.emnlp-industry.24/),
a distance metric that compares strings through possible reading paths rather
than only through visible surface characters.

```python
>>> import moine
>>> moine.distance("moine", "モイニャ", lang="ja")
2
>>> moine.distance("もいにゃ", "モイニャ", lang="ja")
0
>>> moine.distance("weishiji", "威士忌", lang="zh")
0
>>> moine.distance("布納哈奔", "布納哈本", lang="zh")
0
```

## What It Is For

mòine is useful for matching noisy Japanese or Chinese search/input strings,
especially when surface forms differ but reading paths stay close.

- Japanese comparison uses [UniDic-CWJ](https://clrd.ninjal.ac.jp/unidic/download.html)-derived reading artifacts by default, with separate SudachiDict-derived artifacts when users choose the `ja-sudachi` selector.
- Chinese comparison uses [CC-CEDICT](https://cc-cedict.org/wiki/)-derived no-tone pinyin artifacts.
- Python APIs include `distance`, `combined_distance`, `ratio`, `partial_ratio`, and `cdist`.
- Rust users can use the published crate and detailed API documentation on docs.rs.

## Try It

[Open the browser demo](demo/){ .md-button .md-button--primary }
[Use the CLI](cli.md){ .md-button }
[Read the Python API reference](api.md){ .md-button }

## Benchmark

The scoring table was recorded on 2026-06-28. It reports scoring time only;
dictionary loading is shown separately below.

!!! important

    RapidFuzz measures surface Levenshtein distance, so it is expected to be
    much faster. Treat this as a reference for mòine's dictionary-backed
    reading edit distance, not a same-task speed comparison.

```bash
uv run python -m moine download ja
uv run --python python3.14 --with rapidfuzz \
  python scripts/benchmark_distances.py \
  --loops 10000
```

This is a quick local benchmark command. The release-wheel command used for
the recorded table lives in the
[development notes](https://github.com/tagucci/moine/blob/main/docs/development.md).

| Method | mean (±std) | relative |
|---|---:|---:|
| RapidFuzz Levenshtein | 0.15 ± 0.01 us/call | 1.00x |
| mòine ja distance | 26.08 ± 33.82 us/call | 177x |

Fresh dictionary loads from the standard installed artifacts, recorded on
2026-06-28 and measured over 100 loads:

| Dictionary | mean (±std) |
|---|---:|
| UniDic-CWJ | 476.82 ms ± 14.09 ms |
| SudachiDict-full | 2002.83 ms ± 42.17 ms |
| CC-CEDICT | 176.68 ms ± 30.45 ms |

## Name

The project name is inspired by Bunnahabhain Mòine, a Scotch whisky.
