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
- Python APIs include `distance`, `damerau_distance`, `ratio`, `partial_ratio`, and `cdist`.
- Rust users can use the published crate and detailed API documentation on docs.rs.

## Try It

[Open the browser demo](demo/){ .md-button .md-button--primary }
[Use the CLI](cli.md){ .md-button }
[Read the Python API reference](api.md){ .md-button }

## Name

The project name is inspired by Bunnahabhain Mòine, a Scotch whisky.
