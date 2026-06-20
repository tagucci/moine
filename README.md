# mòine

[![CI](https://github.com/tagucci/moine/actions/workflows/ci.yml/badge.svg)](https://github.com/tagucci/moine/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/moine.svg)](https://pypi.org/project/moine/)
[![crates.io](https://badgen.net/crates/v/moine)](https://crates.io/crates/moine)
[![docs.rs](https://img.shields.io/docsrs/moine.svg)](https://docs.rs/moine)

`mòine` is a Python and Rust library for romanization-aware string comparison.

It implements [Lattice Path Edit Distance (Kaji, 2023)](https://aclanthology.org/2023.emnlp-industry.24/),
a distance metric that compares strings through possible reading paths rather
than only through visible surface characters.

This is useful when romanized input and written Japanese or Chinese look far
apart as strings, but stay close in reading space.

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

## Name

The project name comes from `Moine`, a peated malt from Bunnahabhain and one of
the developer's favorite Scotch whiskies. In Japanese, the name has several
plausible katakana renderings, such as `モイニャ`, `モーイン`, and `モアンヌ`, which
makes it a fitting name for a project about readings, spelling variation, and
ambiguity in input sequences.

## Features

- Japanese comparison with
  [UniDic-CWJ](https://clrd.ninjal.ac.jp/unidic/download.html)-derived reading
  artifacts and separate
  [SudachiDict](https://github.com/WorksApplications/SudachiDict)-derived
  artifacts.
- Chinese comparison with
  [CC-CEDICT](https://cc-cedict.org/wiki/)-derived no-tone pinyin artifacts.
- Plain string Levenshtein-compatible distance helpers.
- Lattice-aware Damerau-Levenshtein distance for adjacent transpositions.
- Combined `min(surface Damerau-Levenshtein, LPED)` scorer for paper-style
  reranking.
- Normalized similarity / `ratio` helpers in `0.0..=1.0`.
- [RapidFuzz](https://github.com/rapidfuzz/RapidFuzz)-inspired APIs such as
  `cdist` and partial matching helpers.

## When To Use

mòine is best used after another system has produced candidates: lexical
retrieval, n-gram search, BM25, embeddings, a product catalog, or an entity
list. Use mòine to rescore those candidates in reading space.

| Good fit | Poor fit |
| --- | --- |
| Romanized, kana, kanji, or pinyin input mixed together | Same-script typo matching only |
| Query correction, search suggest, and candidate reranking | Replacing a full search engine |
| Japanese and Mandarin pinyin Chinese entity matching | Cantonese/Jyutping or arbitrary languages |
| Pipelines that can download dictionary artifacts explicitly | Install-only workflows with no data step |
| Hundreds or thousands of candidates after retrieval | Brute-force scoring over a whole corpus |

## Installation

Install the Python package:

```bash
pip install moine
uv pip install moine
```

Install the Rust command-line tool:

```bash
cargo install moine
```

The packages do not bundle dictionary data. Download the language artifacts you
need explicitly:

```bash
# Default Japanese artifact: UniDic-CWJ
uv run python -m moine download ja

# Explicit Japanese sources
uv run python -m moine download ja-unidic
uv run python -m moine download ja-sudachi

# Chinese artifact: CC-CEDICT
uv run python -m moine download zh

# Same selectors are available from the Rust CLI.
moine download ja
moine download ja-unidic
moine download ja-sudachi
moine download zh
```

`ja` is the short default selector for the current Japanese artifact, which is
UniDic-CWJ. Use `ja-unidic` or `ja-sudachi` when the dictionary source should be
explicit.

## Quick Start

Use the top-level Python API when you want mòine to load the default dictionary
for a language:

```python
import moine

print(moine.distance("もいにゃ", "モイニャ", lang="ja"))  # 0
print(moine.ratio("ピィート", "ピート", lang="ja"))  # 0.7142857142857143
print(moine.partial_ratio("ウイスキー", "ういすきーをのんでいます", lang="ja"))  # 1.0
print(moine.distance("weishiji", "威士忌", lang="zh"))  # 0
```

Load a dictionary explicitly when you want to control startup cost or artifact
location:

```python
import moine

dictionary = moine.load_dict(lang="ja")
moine.set_default_dictionary(dictionary)

print(moine.distance("もいにゃ", "モイニャ", lang="ja"))  # 0
```

Use `cdist` for query-by-choice matrices:

```python
import moine

scores = moine.cdist(
    ["もいにゃ", "ぴーと", "ピィート"],
    ["モイニャ", "ピート", "ピーと", "ピィート"],
    lang="ja",
    metric="damerau_distance",
    score_cutoff=1,
)
```

For search or entity matching, generate candidates with your existing system
and use mòine as a reading-aware reranker:

```python
import moine

query = "moine"
candidates = ["モイニャ", "モーイン", "モアンヌ", "ストイーシャ"]

scores = moine.cdist(
    [query],
    candidates,
    lang="ja",
    metric="distance",
)[0]

ranked = sorted(zip(candidates, scores), key=lambda item: item[1])
print(ranked)
# [('モイニャ', 2), ('モーイン', 2), ('モアンヌ', 3), ('ストイーシャ', 7)]
```

Score interpretation is intentionally simple: `distance=0` means the best
reading paths are identical, `combined_distance` is the minimum of surface
Damerau-Levenshtein and LPED, distance metrics are smaller-is-better, `ratio`
and `normalized_similarity` are in `0.0..=1.0` and larger-is-better, and
`score_cutoff` filters in the RapidFuzz style.

## Command Line

Most users only need the public runtime commands:

```bash
moine download ja
moine download zh
moine list
moine where
moine compare --left "もいにゃ" --right "モイニャ" \
  --artifact-metadata /path/to/moine-unidic-cwj-202512/metadata.yaml
moine chinese-compare --left weishiji --right 威士忌 \
  --artifact-metadata /path/to/moine-cedict-20260520/metadata.yaml
```

Use `moine download ja-unidic` for explicit UniDic-CWJ and
`moine download ja-sudachi` for SudachiDict-full.

The artifact bundle, verification, archive, and raw-dictionary inspection
commands are maintainer-facing tools for producing and checking release assets.
They are documented in [docs/development.md](docs/development.md) and
[docs/release_process.md](docs/release_process.md).

The public comparison commands can also emit lattice graphs:
`moine compare --romaji-lattice <PATH> --output-format <dot|svg|png>` for
Japanese romaji lattices, and
`moine chinese-compare --pinyin-lattice <PATH> --output-format <dot|svg|png>`
for Chinese pinyin lattices. Writing SVG or PNG graphs requires the Graphviz
`dot` command to be available in `PATH`; DOT output does not require that runtime
dependency. See [CLI usage](https://tagucci.github.io/moine/cli/) for rendered
examples.

## Documentation

- [Project documentation](https://tagucci.github.io/moine/)
- [Installation](https://tagucci.github.io/moine/installation/)
- [Python usage](https://tagucci.github.io/moine/usage/)
- [CLI usage](https://tagucci.github.io/moine/cli/)
- [API reference](https://tagucci.github.io/moine/api/)
- [Rust usage](https://tagucci.github.io/moine/rust/)
- [Dictionary artifacts](https://tagucci.github.io/moine/artifacts/)
- [Browser demo](https://tagucci.github.io/moine/demo/)
- [Rust docs](https://docs.rs/moine)

Developer and maintainer notes live under [docs/](docs/), starting with
[docs/development.md](docs/development.md) and
[docs/release_process.md](docs/release_process.md).
See [CONTRIBUTING.md](CONTRIBUTING.md) before opening pull requests.

## How It Differs From RapidFuzz

RapidFuzz is the better fit when both inputs should be compared directly as
surface strings and you need a broad set of highly optimized fuzzy-matching
scorers. mòine focuses on a narrower problem: comparing strings through
possible reading paths before edit distance is computed.

### Benchmark

Recorded on 2026-06-20. The first table reports scoring time only; dictionary
loading is shown separately below.

> [!IMPORTANT]
> RapidFuzz measures surface Levenshtein distance, so it is expected to be much
> faster. Treat this as a reference for mòine's dictionary-backed reading edit
> distance, not a same-task speed comparison.

```bash
uv run python -m moine download ja
uv run --python python3.14 --with rapidfuzz \
  python scripts/benchmark_distances.py \
  --loops 10000
```

This is a quick local benchmark command. The release-wheel command used for
the recorded table lives in [development notes](docs/development.md).

| Method | mean (±std) | relative |
|---|---:|---:|
| RapidFuzz Levenshtein | 0.15 ± 0.01 us/call | 1.00x |
| mòine ja distance | 58.38 ± 109.10 us/call | 390x |

Fresh dictionary loads from the standard installed artifacts, measured over 100
loads:

| Dictionary | mean (±std) |
|---|---:|
| UniDic-CWJ | 475.23 ms ± 22.41 ms |
| SudachiDict-full | 1959.09 ms ± 43.83 ms |
| CC-CEDICT | 168.82 ms ± 12.47 ms |

## Limitations

- mòine does not reproduce the original paper's private search-query-log
  evaluation.
- Dictionary-backed comparison requires separately distributed dictionary
  artifacts.
- UniDic matching intentionally does not use MeCab/Viterbi costs.
- Chinese support is Mandarin pinyin only; it does not model Cantonese/Jyutping
  or non-Mandarin readings.
- `processor`, `score_hint`, NumPy dtype options, and worker parallelism are not
  part of the initial `cdist` API.

## Reference

> [!CAUTION]
> This project is not the official implementation by the paper author.

```bibtex
@inproceedings{kaji-2023-lattice,
    title = "Lattice Path Edit Distance: A {R}omanization-aware Edit Distance for Extracting Misspelling-Correction Pairs from {J}apanese Search Query Logs",
    author = "Kaji, Nobuhiro",
    editor = "Wang, Mingxuan  and
      Zitouni, Imed",
    booktitle = "Proceedings of the 2023 Conference on Empirical Methods in Natural Language Processing: Industry Track",
    month = dec,
    year = "2023",
    address = "Singapore",
    publisher = "Association for Computational Linguistics",
    url = "https://aclanthology.org/2023.emnlp-industry.24/",
    doi = "10.18653/v1/2023.emnlp-industry.24",
    pages = "233--242",
}
```

## License

mòine source code is licensed under either MIT or Apache-2.0. See
[LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

Dictionary data is separate. UniDic-derived, SudachiDict-derived, and
CC-CEDICT-derived artifacts carry their own license and attribution metadata,
and should keep dictionary license information separate from the mòine
source-code license. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
