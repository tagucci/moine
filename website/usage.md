# Python Usage

Install the package and download at least one dictionary artifact before using
language-aware comparison:

```bash
uv pip install moine
# Default Japanese artifact: UniDic-CWJ
uv run python -m moine download ja

# Explicit Japanese sources
uv run python -m moine download ja-unidic
uv run python -m moine download ja-sudachi

# Chinese artifact: CC-CEDICT
uv run python -m moine download zh
```

Use the top-level API when you want mòine to load the default dictionary for a
language:

```python
>>> import moine

>>> moine.distance("moine", "モイニャ", lang="ja")
2

>>> moine.distance("もいにゃ", "モイニャ", lang="ja")
0

>>> moine.distance("じゅじゅつかいせん", "呪術廻戦", lang="ja-unidic")
5

>>> moine.distance("じゅじゅつかいせん", "呪術廻戦", lang="ja-sudachi")
0

>>> moine.partial_ratio("ウイスキー", "ういすきーをのんでいます", lang="ja")
1.0

>>> moine.distance("weishiji", "威士忌", lang="zh")
0

>>> moine.distance("布納哈奔", "布納哈本", lang="zh")
0
```

## Reusing A Dictionary

Load a dictionary explicitly when you want to control startup cost or artifact
location:

```python
>>> import moine

>>> dictionary = moine.load_dict(lang="ja")
>>> moine.set_default_dictionary(dictionary)

>>> moine.distance("もいにゃ", "モイニャ", lang="ja")
0

>>> dictionary.ratio("ピィート", "ピート")
0.7142857142857143

>>> moine.damerau_distance("moine", "mione", lang="ja")
1
```

## Pairwise Matrices

Use `cdist` to score a matrix of queries and choices:

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

`cdist` returns a plain `list[list[int | float]]`. It supports `distance`,
`damerau_distance`, `normalized_distance`, `normalized_similarity`, and
`ratio`.

## Partial Matching

Use `partial_ratio`, `partial_distance`, or `partial_alignment` when the query
may appear as a span inside longer text:

```python
>>> import moine

>>> moine.partial_distance("ウイスキー", "ういすきーをのんでいます", lang="ja")
0

>>> text = "ういすきーをのんでいます"
>>> alignment = moine.partial_alignment("ウイスキー", text, lang="ja")
>>> alignment
PartialAlignment(score=1.0, src_start=0, src_end=5, dest_start=0, dest_end=5)
>>> text[alignment.dest_start:alignment.dest_end]
'ういすきー'
```

## Candidate Reranking

mòine is usually the scorer after candidate generation, not the retrieval
engine itself. Generate a bounded candidate set with your existing search,
dictionary, or entity list, then rescore it in reading space:

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

For Chinese reranking, use the same `cdist(..., lang="zh")` pattern with the
CC-CEDICT-derived artifact.

## Cutoffs

`score_cutoff` follows the RapidFuzz-style contract:

- `distance` and `damerau_distance` return `score_cutoff + 1` when the distance
  exceeds the cutoff.
- `normalized_distance` returns `1.0` when the normalized distance exceeds the
  cutoff.
- `normalized_similarity` and `ratio` return `0.0` when the score is below the
  cutoff.

```python
>>> import moine

>>> moine.distance("ピィート", "ピート", lang="ja", score_cutoff=0)
1

>>> moine.ratio("ピィート", "ピート", lang="ja", score_cutoff=0.8)
0.0
```

## Language Helpers

The `moine.ja` and `moine.zh` modules expose explicit dictionary-backed helpers
for users who want to manage artifact paths directly.

```python
from moine.ja import Dictionary

dictionary = Dictionary.load_bundle("/path/to/moine-unidic-cwj-202512")
dictionary.distance("もいにゃ", "モイニャ")
dictionary.ratio("ピィート", "ピート")
```

```python
from moine.zh import Dictionary

dictionary = Dictionary.load_bundle("/path/to/moine-cedict-20260520")
dictionary.distance("weishiji", "威士忌")
dictionary.distance("布那哈本", "布納哈本")
dictionary.ratio("布呐哈本", "布納哈本")
```

## Candidate Extraction

`moine.ja.process.extract(...)`, `moine.zh.process.extract(...)`, and
`extract_one(...)` provide lightweight candidate scoring helpers over the
language-specific dictionary-backed scorers. Choices may be an iterable of
strings or a mapping from arbitrary keys to strings.

```python
from moine.ja import Dictionary, process

dictionary = Dictionary.load_bundle("/path/to/moine-unidic-cwj-202512")

matches = process.extract(
    "ぴーと",
    {"peat": "ピート", "mòine": "モイニャ"},
    dictionary=dictionary,
    scorer="distance",
    score_cutoff=0,
)
print(matches)
# [('ピート', 0, 'peat')]
```

```python
from moine.zh import Dictionary, process

dictionary = Dictionary.load_bundle("/path/to/moine-cedict-20260520")

matches = process.extract(
    "weishiji",
    {"bunnahabhain": "布納哈本", "whisky": "威士忌"},
    dictionary=dictionary,
    scorer="ratio",
    score_cutoff=1.0,
)
print(matches)
# [('威士忌', 1.0, 'whisky')]
```

Results are `(choice, score, index_or_key)` tuples and preserve input order for
ties.

## When Not To Use

Use RapidFuzz or another surface-string matcher when both inputs are already in
the same script and only ordinary typos matter. mòine is narrower: it is for
cases where kana, kanji, romanization, or Mandarin pinyin differ on the surface
but should still be close in reading space.
