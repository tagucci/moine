# API Reference

This page documents the public Python API by hand. The API surface is small
enough that hand-written reference text is clearer than generated output for
the initial release.

## `moine.distance`

<p class="api-signature"><code>moine.distance(left, right, *, lang=None, dictionary=None, score_cutoff=None, max_readings_per_segment=None, max_span_chars=None, max_paths=None, longest_only=None)</code></p>

Returns the Levenshtein-style Lattice Path Edit Distance for one pair of
strings. When both `lang` and `dictionary` are omitted, the function falls back
to plain string edit distance.

`left`
: The first string.

`right`
: The second string.

`lang`
: Optional language code. Use `"ja"` for Japanese or `"zh"` for Chinese.

`dictionary`
: Optional loaded dictionary object. When omitted, mòine loads or reuses the
  default dictionary for `lang`.

`score_cutoff`
: Optional integer threshold. Distances greater than the cutoff return
  `score_cutoff + 1`.

`max_readings_per_segment`, `max_span_chars`, `max_paths`, `longest_only`
: Optional dictionary expansion controls. These options require `lang` or
  `dictionary`; plain string distance rejects them.

```python
>>> import moine
>>> moine.distance("weishiji", "威士忌", lang="zh")
0
```

## `moine.damerau_distance`

<p class="api-signature"><code>moine.damerau_distance(left, right, *, lang=None, dictionary=None, score_cutoff=None, max_readings_per_segment=None, max_span_chars=None, max_paths=None, longest_only=None)</code></p>

Returns the lattice-aware Damerau-Levenshtein distance for one pair of strings.
It can count adjacent transpositions as one edit on lattice paths.

```python
>>> import moine
>>> moine.damerau_distance("moine", "mione")
1
```

## `moine.normalized_distance`

<p class="api-signature"><code>moine.normalized_distance(left, right, *, lang=None, dictionary=None, score_cutoff=None, max_readings_per_segment=None, max_span_chars=None, max_paths=None, longest_only=None)</code></p>

Returns a normalized distance in `0.0..=1.0`.

```python
>>> import moine
>>> moine.normalized_distance("もいにゃ", "モイニャ", lang="ja")
0.0
```

## `moine.normalized_similarity`

<p class="api-signature"><code>moine.normalized_similarity(left, right, *, lang=None, dictionary=None, score_cutoff=None, max_readings_per_segment=None, max_span_chars=None, max_paths=None, longest_only=None)</code></p>

Returns a normalized similarity in `0.0..=1.0`, where larger is better.

```python
>>> import moine
>>> moine.normalized_similarity("もいにゃ", "モイニャ", lang="ja")
1.0
```

## `moine.ratio`

<p class="api-signature"><code>moine.ratio(left, right, *, lang=None, dictionary=None, score_cutoff=None, max_readings_per_segment=None, max_span_chars=None, max_paths=None, longest_only=None)</code></p>

Alias for `normalized_similarity`.

```python
>>> import moine
>>> moine.ratio("ピィート", "ピート", lang="ja")
0.7142857142857143
```

## `moine.partial_ratio`

<p class="api-signature"><code>moine.partial_ratio(query, text, *, lang=None, dictionary=None, score_cutoff=None, max_span_chars=None, max_reading_span_chars=None, max_readings_per_segment=None, max_paths=None, longest_only=None)</code></p>

Returns the best normalized similarity between `query` and a span in `text`.
The returned score is in `0.0..=1.0`, where larger is better. In partial
APIs, `max_span_chars` limits scanned spans in `text`; `max_reading_span_chars`
limits dictionary reading expansion. When `max_span_chars` is omitted,
dictionary-backed matching also accounts for the longest reading path of
`query`, so short written forms such as kanji or hanzi can still match longer
romanized spans.

```python
>>> import moine
>>> moine.partial_ratio("ウイスキー", "ういすきーをのんでいます", lang="ja")
1.0
```

## `moine.partial_distance`

<p class="api-signature"><code>moine.partial_distance(query, text, *, lang=None, dictionary=None, score_cutoff=None, max_span_chars=None, max_reading_span_chars=None, max_readings_per_segment=None, max_paths=None, longest_only=None)</code></p>

Returns the best distance between `query` and a span in `text`.
If dictionary-backed matching cannot score any span in `text`, this returns
`len(query)` without a cutoff or `score_cutoff + 1` with a cutoff.

```python
>>> import moine
>>> moine.partial_distance("ウイスキー", "ういすきーをのんでいます", lang="ja")
0
```

## `moine.partial_alignment`

<p class="api-signature"><code>moine.partial_alignment(query, text, *, lang=None, dictionary=None, metric="ratio", score_cutoff=None, max_span_chars=None, max_reading_span_chars=None, max_readings_per_segment=None, max_paths=None, longest_only=None)</code></p>

Returns a `PartialAlignment(score, src_start, src_end, dest_start, dest_end)`
for the best span, or `None` when no span can be scored or `score_cutoff`
filters every span. Offsets are Python character offsets. `metric` is
`"ratio"` by default; use `"distance"` to rank by distance instead.

```python
>>> import moine
>>> text = "ういすきーをのんでいます"
>>> alignment = moine.partial_alignment("ウイスキー", text, lang="ja")
>>> alignment
PartialAlignment(score=1.0, src_start=0, src_end=5, dest_start=0, dest_end=5)
>>> text[alignment.dest_start:alignment.dest_end]
'ういすきー'
```

## `moine.cdist`

<p class="api-signature"><code>moine.cdist(queries, choices, *, lang=None, dictionary=None, metric="distance", score_cutoff=None, max_readings_per_segment=None, max_span_chars=None, max_paths=None, longest_only=None)</code></p>

Returns a query-by-choice matrix of scores.

`queries`
: Iterable of query strings.

`choices`
: Iterable of candidate strings.

`lang`
: Optional language code. Use `"ja"` or `"zh"` for dictionary-backed scoring.
  Omit it for plain string scoring.

`dictionary`
: Optional loaded dictionary object. When supplied, `cdist` can run without
  `lang`.

`metric`
: One of `"distance"`, `"damerau_distance"`, `"normalized_distance"`,
  `"normalized_similarity"`, or `"ratio"`.

`score_cutoff`
: Optional threshold. Use an integer for distance metrics and a float for
  normalized metrics.

`max_readings_per_segment`, `max_span_chars`, `max_paths`, `longest_only`
: Optional dictionary expansion controls. These require `lang` or `dictionary`.

```python
>>> import moine
>>> moine.cdist(["abc", "axc"], ["abc", "acb"])
[[0, 2], [1, 2]]
>>> moine.cdist(["abc"], ["abc", "adc"], metric="ratio")
[[1.0, 0.6666666666666666]]
>>> moine.cdist(
...     ["weishiji", "布納哈奔"],
...     ["威士忌", "布納哈本"],
...     lang="zh",
... )
[[0, 8], [8, 0]]
```

!!! note

    `cdist` intentionally keeps the first public API small. It does not expose
    RapidFuzz-only knobs such as `processor`, `score_hint`, NumPy dtype options,
    or worker parallelism.

## Dictionary Loading

### `moine.load_dict`

<p class="api-signature"><code>moine.load_dict(*, lang, path=None)</code></p>

Loads a dictionary artifact for one language. If `path` is omitted, mòine
searches the configured cache, language-specific environment variables, and
`MOINE_DICTIONARIES_PATH`.

```python
>>> import moine
>>> dictionary = moine.load_dict(lang="ja")
```

### `moine.set_default_dictionary`

<p class="api-signature"><code>moine.set_default_dictionary(dictionary)</code></p>

Registers a loaded dictionary as the default dictionary for its language.

```python
>>> import moine
>>> dictionary = moine.load_dict(lang="ja")
>>> moine.set_default_dictionary(dictionary)
>>> moine.distance("もいにゃ", "モイニャ", lang="ja")
0
```

### `moine.clear_default_dictionary`

<p class="api-signature"><code>moine.clear_default_dictionary(*, lang)</code></p>

Clears the configured default dictionary for a language.

### `moine.get_default_dictionary`

<p class="api-signature"><code>moine.get_default_dictionary(*, lang)</code></p>

Returns the configured default dictionary for a language, or `None`.

## Language-Specific Modules

`moine.ja`
: Japanese helpers, the UniDic-backed `Dictionary` alias, and
  `process.extract(...)` / `extract_one(...)` candidate scoring helpers.

`moine.zh`
: Chinese helpers, the CC-CEDICT-backed `Dictionary` alias, and
  `process.extract(...)` / `extract_one(...)` candidate scoring helpers.

Rust users should use the crate documentation on
[docs.rs](https://docs.rs/moine).
