# Matcher API Contract

Status: proposed; audited against `v0.2.3` (`3cc3ec8`) on 2026-09-16.

This document fixes the public boundary for reusable candidate matching before
the implementation is optimized. It is a design contract, not documentation
for APIs available in `0.2.3`. Examples below describe the proposed API and
are not runnable against the current release.

## Goals

- load and configure a dictionary once;
- prepare a candidate collection once and reuse it for many queries;
- move candidate scoring, filtering, ordering, and truncation into one Rust
  batch call;
- keep Python convenient while making Rust allocation reuse explicit;
- leave room for an opt-in, typed explanation of a single comparison;
- preserve the current scalar functions, `cdist`, and `process.extract`
  behavior during migration.

## Current Implementation And Constraints

The work starts from existing optimizations, not a new distance engine:

- `python/moine/_process.py` scores candidates through individual Python
  calls. `extract` retains and sorts every passing candidate before slicing;
  `extract_one` scans all candidates with the original cutoff.
- `crates/moine-python/src/lib.rs` already prepares inputs once per `cdist`
  call, releases the GIL, and reuses `DistanceWorkspace` or
  `StringDistanceWorkspace`. Preparation is not reusable across calls.
- Japanese integer LPED can build compact lattices directly from romaji
  fragments. Japanese and Chinese normalized scoring still compare explicit
  reading-path pairs. Japanese partial matching also expands paths and scores
  each bounded surface span separately.
- `moine-core` already provides fallible distances, cutoffs, reusable scratch
  buffers, and Levenshtein trace reconstruction. Extend these boundaries rather
  than introduce another implementation in the Python binding.

The first optimization target is repeated queries against a fixed candidate
collection. Lower latency and bounded result memory matter independently:
returning five matches must not require retaining every passing result or
constructing a dense query-by-choice matrix.

## Score Semantics Before Optimization

The existing `distance` is Levenshtein-style LPED, and `combined_distance` is
`min(surface OSA, Levenshtein-style LPED)`. It does not use lattice Damerau
distance as its second term. The plain-string `damerau_distance` implements
optimal string alignment (OSA), not unrestricted Damerau-Levenshtein.

For normalized reading scores, let `P` and `Q` be the existing adapter's
accepted path sets. Preserve:

```text
normalized_similarity = max over p in P, q in Q of
                        (1 - levenshtein(p, q) / max(len(p), len(q)))
normalized_distance   = 1 - normalized_similarity
ratio                 = normalized_similarity
```

Lengths count Unicode scalar values; two empty paths have similarity one.
The path pair with minimum raw distance need not maximize normalized
similarity. Dividing the minimum LPED by a lattice-wide maximum length changes
the contract and is not an optimization of this API.

`ratio` uses a `0..1` scale and Levenshtein normalization. RapidFuzz's
[`fuzz.ratio`](https://rapidfuzz.github.io/RapidFuzz/Usage/fuzz.html#ratio)
uses Indel normalization on a `0..100` scale. Multiplying moine's ratio by 100
does not make the scores equivalent. Compare plain-string moine normalization
with RapidFuzz's
[`Levenshtein.normalized_similarity`](https://rapidfuzz.github.io/RapidFuzz/Usage/distance/Levenshtein.html#normalized-similarity).

Freeze empty-input, cutoff, tie, Unicode, unsupported-input, and expansion-limit
behavior in parity tests before moving scoring code. Partial matching remains
directional query-in-text matching over bounded surface spans; it is not
interchangeable with RapidFuzz's shorter-input alignment semantics.

## Reference Shape

The contract follows two deliberately small APIs:

- [`daachorse`](https://github.com/daac-tools/daachorse) builds an immutable
  search object once, keeps advanced construction behind a builder, and
  returns a small typed `Match` value;
- [`vibrato`](https://github.com/daac-tools/vibrato) separates the immutable
  `Tokenizer` from a reusable mutable `Worker`, while its Python binding hides
  the worker from ordinary callers.

The corresponding mòine model is:

```text
loaded dictionary -> Matcher -> PreparedChoices -> repeated extract calls
                                  |
                                  +-> Rust Worker for reusable scratch space
```

Like those libraries, this API keeps adjacent concerns out of the matching
object. `Matcher` does not download artifacts, open archives, resolve default
cache locations, or validate bundle metadata. Those operations continue to
belong to the dictionary and artifact loaders.

## Public Types

| Rust | Python | Responsibility |
| --- | --- | --- |
| `moine::ja::Matcher`, `moine::zh::Matcher` | `moine.Matcher` | Immutable dictionary and reading-expansion configuration. |
| `moine::ja::PreparedChoices`, `moine::zh::PreparedChoices` | `moine.PreparedChoices` | Immutable candidate strings and their prepared language representation. |
| `moine::ja::Worker<'_>`, `moine::zh::Worker<'_>` | hidden | Mutable, reusable Rust scratch space for query preparation, distances, and ranking. |
| `moine::Match<T>` | `moine.Match` | One ranked result: candidate index/key, candidate string, and score. |
| `moine::ExtractOptions<T>` | keyword arguments | Typed cutoff and result-limit controls. |
| `moine::ComparisonResult` | `moine.ComparisonResult` | Opt-in evidence for one LPED comparison. It is never returned by bulk APIs. |

The Rust Japanese and Chinese modules expose the same method names but retain
their language-specific dictionary and error types. The Python `Matcher`
dispatches from the concrete `JapaneseDictionary` or `ChineseDictionary`
passed to its constructor; it does not accept a separate `lang` argument.

## Construction

Dictionary loading remains explicit. Required state is passed to the
constructor and optional reading controls are immutable after construction.

```python
import moine

dictionary = moine.load_dict(lang="ja")
matcher = moine.Matcher(
    dictionary,
    max_paths=128,
    longest_only=True,
)
prepared = matcher.prepare(["印刷", "印字", "印刷所"])
```

The Rust shape is language-specific and uses consuming configuration methods,
following the `vibrato::Tokenizer` style:

```rust,no_run
let dictionary = moine::ja::load_bundle("dist/moine-unidic-cwj-202512")?;
let matcher = moine::ja::Matcher::new(dictionary)
    .max_paths(128)?
    .longest_only(true);
let prepared = matcher.prepare(["印刷", "印字", "印刷所"])?;
```

The exact internal ownership mechanism is private. Constructing a `Matcher`
must not copy the dictionary payload, and cloning a `Matcher` or
`PreparedChoices` must use shared immutable storage.

`Matcher` accepts the same reading controls already used by dictionary-backed
scoring:

- `max_readings_per_segment`;
- `max_span_chars`;
- `max_paths`;
- `longest_only`.

Bundle defaults apply when an option is omitted. Invalid values fail during
construction or preparation. Query-dependent expansion and pair-dependent
matrix limits can still fail during scoring and must remain structured errors.

## Prepared Choices

`Matcher.prepare` materializes the input iterable and performs dictionary
lookup and reading-lattice construction once per candidate. Preparation
preserves:

- candidate order;
- duplicate strings;
- empty strings;
- Python mapping keys, when the Python input is a mapping.

The resulting object is immutable and reusable for any number of queries. It
exposes `len`, `is_empty`, and indexed access to the original candidate text.
Python also exposes the original mapping key. Mutation of the source list or
mapping after `prepare` has no effect.

A `PreparedChoices` value is bound to the dictionary and reading options of the
`Matcher` that created it. The public API does not allow combining it with a
different matcher configuration.

The implementation may prepare metric-specific data lazily, but a repeated
call with the same metric must not repeat dictionary lookup or reading
expansion for every candidate.

Prepare integer-distance lattices eagerly. Retain enough adapter-owned input
to derive the existing normalized path representation on first use; cache that
representation in shared storage. Use fallible, synchronized initialization so
concurrent calls do not race to publish partial data. A normalized-expansion
failure must not invalidate integer scoring. A successful `prepare` therefore
does not promise that every metric or every query will fit its resource limits.

Keep the existing direct-input, exact-dictionary, and hybrid fallback policies
for each metric. Reuse a common intermediate representation only after tests
show that it preserves the accepted paths; do not infer normalized paths by
enumerating a possibly different integer-distance lattice.

The context retained by `PreparedChoices` owns its dictionary and options.
Clones keep it alive after the original matcher is dropped. Input deduplication
may share internal representations but must preserve every original index,
duplicate, mapping key, and stable tie order in results. Python mapping keys
stay outside the GIL-free scoring loop; Rust returns numeric indices.

## Extraction

Python keeps the current scorer names and performs the complete candidate loop
inside one native call:

```python
hits = prepared.extract(
    "いんさt",
    scorer="distance",
    limit=5,
    score_cutoff=3,
)

for hit in hits:
    print(hit.choice, hit.score, hit.key)
```

The Python defaults remain `scorer="distance"`, `limit=5`, and no
`score_cutoff`. Rust selects the metric through its method name;
`ExtractOptions<T>::default()` likewise uses a limit of five and no cutoff.

`extract_one` is equivalent to `extract(..., limit=1)` but may avoid sorting
or retaining non-winning candidates.

Rust uses metric-specific methods so the score remains statically typed:

```rust,no_run
let mut worker = prepared.new_worker();
let hits: Vec<moine::Match<usize>> = worker.extract_distance(
    "いんさt",
    moine::ExtractOptions::default()
        .limit(5)
        .score_cutoff(3),
)?;
```

The first implementation supports the same scorer set as the current Python
helpers:

- `distance`, `damerau_distance`, and `combined_distance`, returning integer
  scores;
- `normalized_distance`, `normalized_similarity`, and `ratio`, returning
  floating-point scores.

The public Rust method names mirror those scorers, for example
`extract_combined_distance` and `extract_normalized_similarity`. This avoids a
public `int | float`-style score enum.

Ranking and filtering preserve the existing contract:

- distance metrics sort ascending;
- similarity metrics sort descending;
- equal scores retain candidate input order;
- `score_cutoff` keeps only passing scores;
- `limit=None` returns every passing candidate in Python;
- `limit=0` returns an empty list;
- an empty candidate collection returns an empty list;
- `extract_one` returns `None` when no candidate passes the cutoff.

`Match<T>` is a small value with private fields and read-only accessors for
`index` and `score`. Rust callers recover the original string through
`PreparedChoices::choice(index)`. Python `Match` additionally exposes
`choice` and `key` so it can replace the current result tuple without losing
mapping behavior.

The Python batch call releases the GIL while Rust prepares the query and scores
candidates. `Matcher` and `PreparedChoices` are safe to share between threads.
Rust `Worker` is mutable and intended for one thread at a time; callers create
one worker per thread.

### Native Execution And Result Memory

One extraction call prepares the query once, then scores candidates using one
worker. The worker reuses the existing distance buffers and ranking storage.
Python hides workers and uses exclusive per-call scratch state initially;
cross-call worker pooling is optional and must not serialize all readers on
one global mutex.

- For a finite positive `limit=k`, retain at most `k` ranked results in a
  bounded heap (or an equivalent selection structure), then order the winners.
  Ranking work is `O(N log k)` rather than a full `O(N log N)` sort.
- For `extract_one`, retain one winner. For `limit=None`, collect and sort all
  passing results; document the resulting `O(N)` result memory.
- Tighten the scorer cutoff using the current worst retained score. Preserve
  boundary ties and original indices. Cutoff sentinels are not exact scores and
  must never become ranked matches.
- Once arguments have been validated, `limit=0` and empty prepared collections
  return without preparing the query or allocating a distance matrix.
- An optimal score may permit early termination only when all skipped work is
  known not to change observable error behavior. A pair-dependent resource
  failure remains possible even after candidate preparation.

Do not silently change legacy helper behavior while optimizing the object API.
The existing helpers consume their input iterable and may encounter errors in
later candidates even when `limit=0` or an earlier score is optimal. Preserve
that behavior during delegation, or document and test a deliberate change
separately. Likewise, a failed candidate cannot silently disappear from a
ranking; errors identify its index and the relevant preparation/scoring stage.
Retain a legacy execution path where eager preparation would change observable
iterator consumption or error ordering; native delegation is conditional on
compatibility, not a requirement to rewrite every wrapper at once.

Normalization cutoffs should eventually reach the path-pair kernel rather
than only filter a completed score. Derive each pair's integer threshold from
its own lengths, verify floating-point boundary equivalence, and retain an
unoptimized reference implementation for tests.

## Scalar Scoring And `cdist`

The existing scalar APIs remain the shortest path for one-off comparisons:

```python
moine.distance(left, right, lang="ja")
dictionary.distance(left, right)
```

`Matcher` exposes the same scalar scorer methods with reading controls fixed by
construction. Their results and cutoff rules remain identical to the existing
dictionary methods.

Rust should expose typed cutoff and normalized-distance conveniences through
the matcher/worker API, rather than require callers to rebuild lattices solely
to access core cutoff methods. No public scorer trait or generic language
adapter framework is required for the first release.

`cdist` remains the dense query-by-choice matrix API. It does not return
`Match` or `ComparisonResult` objects, and `PreparedChoices` does not replace
it. This keeps bulk numeric scoring separate from ranked retrieval.

After prepared extraction is stable, add `cpdist` for corresponding pairs of
equal-length inputs. This avoids building an `N x N` matrix merely to read its
diagonal. Specify empty inputs, length mismatch, cutoff sentinels, and integer
versus floating output before implementation. Keep the current `cdist` list
return type; contiguous arrays, blocked output, `out=`, and `workers=` are
separate optional additions with explicit memory and dependency decisions.

## Comparison Evidence

Detailed evidence is explicit because it is more expensive than a scalar
score. The public operation is named `reading_alignment`, rather than exposing
the implementation term `trace`:

```python
result = matcher.reading_alignment("いんさt", "印刷")
print(result.distance)
print(result.left_reading, result.right_reading)
for step in result.steps:
    print(step.operation, step.left_range, step.right_range)
```

`ComparisonResult` contains only the evidence for the selected minimum LPED
path:

- integer `distance`;
- selected `left_reading` and `right_reading`;
- typed edit steps with character offsets into those selected readings;
- preparation diagnostics describing observed reading-expansion pruning and
  the effective expansion options.

Reuse `try_distance_with_trace` on the same accepted lattice as scalar LPED.
Report half-open Unicode-scalar ranges into the selected reading strings, not
surface-text ranges. Surface attribution requires adapter provenance and is a
separate extension: compact lattices can merge equivalent readings from
different source segments.

Diagnostics must distinguish observed pruning from limits merely being
configured. Existing expansion counters do not establish that no dictionary
match was excluded by `max_span_chars`, nor recover readings removed when an
artifact was built. Do not claim globally exhaustive readings. Preserve
dictionary identity, effective options, and existing dictionary/direct segment
provenance internally; do not invent token-level source identity after paths
have been merged. Resolve deterministic equal-cost trace selection before
publishing it as a stable explanation contract.

The initial explanation API covers Levenshtein-style LPED. It does not pretend
that the current Damerau or normalized scorers have the same trace semantics.
Those metrics can gain separate evidence only after their semantics are
specified.

`ComparisonResult` is returned only by `reading_alignment`. `distance`,
`cdist`, `extract`, and `extract_one` continue to return compact numeric
results. This avoids allocating explanations for reranking workloads.

## Errors And Determinism

- malformed external artifacts fail in the artifact loader before a matcher is
  constructed;
- invalid options, invalid cutoffs, and language/type mismatches return
  structured Rust errors and corresponding Python exceptions;
- external input must not trigger Rust panics;
- the same dictionary, options, candidates, query, and metric produce the same
  order and scores;
- reading-expansion caps remain deterministic controls, not probability or
  source-cost ranking.

Do not use metric-tree pruning for LPED without a valid lower-bound proof.
Minimum path-pair distance is not generally a metric over path sets: with
`A={a}`, `B={a,b}`, and `C={b}`, distances are `d(A,B)=0`, `d(B,C)=0`, and
`d(A,C)=1`. An approximate candidate prefilter must be explicit and evaluated
for missed matches rather than presented as exact extraction.

## Implementation Boundaries

- Keep language conversion and reading provenance in `moine-ja` and
  `moine-zh`; expose public matcher conveniences through `moine::ja` and
  `moine::zh`. Keep `moine-core` language-independent.
- Move reusable numeric scoring and batch kernels out of the monolithic PyO3
  module into appropriate Rust modules before sharing them with the public
  Rust API. Python should validate, convert, release the GIL, and map results.
- Share only demonstrated duplication: normalized path-pair scoring, cutoff
  application, stable ranking, and buffer ownership. Avoid a language trait or
  macro framework that obscures Japanese and Chinese fallback differences.
- Consolidate repeated artifact-verification policy in a separate change with
  loader parity tests. Matcher construction consumes loaded dictionaries and
  does not add a dependency from language crates to `moine-cli`.
- A bounded lookup cache is a measured follow-up, not an unbounded global
  cache. Prefer prepared candidate storage first; measure indexed decode and
  query preparation separately before adding cache synchronization.

## Compatibility And Migration

This is an additive pre-1.0 migration:

1. introduce `Matcher`, `PreparedChoices`, `Worker`, and `Match<T>` without
   removing existing functions;
2. implement native batch extraction and make
   `moine.ja.process.extract`/`moine.zh.process.extract` delegate to it;
3. add `reading_alignment` and `ComparisonResult` separately;
4. consider deprecations only after the new API has shipped and real examples
   show that it is simpler.

Existing `process.extract` continues to return `(choice, score, index_or_key)`
tuples. The new object API returns `Match` values. This allows migration without
silently changing equality, unpacking, or type-checking behavior.

Implement in reviewable stages:

1. Lock scalar/batch/extraction contracts and add a reproducible benchmark
   separating load, candidate preparation, query preparation, scoring, ranking,
   and Python conversion.
2. Extract shared Rust kernels without changing results; implement immutable
   matcher context, prepared integer lattices, and worker scratch reuse.
3. Add native bounded top-k extraction, then lazy normalized representations
   and all six existing scorer names. Preserve legacy helper tuples and errors.
4. Add `cpdist` and bounded/chunked `extract_iter` independently. An iterator
   over prepared candidates still retains the prepared collection; genuinely
   streaming candidate input needs a separate memory contract.
5. Add Levenshtein `reading_alignment` with deterministic traces and honest
   pruning diagnostics.
6. Experiment separately with exact lattice-native normalization and partial
   search. Path lengths are part of normalized optimization state. Surface
   substring boundaries can change dictionary segmentation in partial search,
   so slicing a full-text reading lattice is not automatically equivalent.

Do not make lattice-native normalized DP, arbitrary Python callbacks, public
parallel scheduling, or explanation allocation prerequisites for prepared
integer extraction. The first published matcher supports the six existing
extraction scorers, using the existing normalized algorithm where necessary.

## Acceptance Criteria

- Japanese and Chinese scalar, prepared, batch, and legacy extraction agree
  on exact scores, order, cutoffs, duplicate choices, mapping keys, empty
  inputs, Unicode, and effective dictionary defaults. Common validation rules
  remain aligned; document the new object's eager preparation errors separately
  from legacy iteration and error timing.
- Tests cover query-dependent matrix limits, normalized expansion failures,
  and continued integer use after such a failure; no panics cross the boundary.
- Preparing `N` candidates and issuing `Q` integer queries performs candidate
  lookup/expansion once per prepared entry, not `N x Q` times. Query preparation
  occurs once per extraction call. Verify with counters as well as timings.
- Finite top-k ranking retains `O(k)` result state apart from prepared inputs
  and distance scratch. Report peak memory, preparation cost, and repeated-query
  latency for small and large candidate collections.
- Benchmarks record commit, release build, Python/native versions, artifact
  checksum, options, input lengths, ambiguity, OOV behavior, repetitions, and
  warm/cold conditions. Include real distinct candidates as well as duplicates.
- Keep performance measurements separate from retrieval-quality evaluation.
  Compare matching algorithms with equal semantics; surface RapidFuzz timings
  are a separate workload from dictionary-backed LPED.

## Non-goals

The first version does not include:

- automatic artifact download or cache resolution in `Matcher`;
- archive decompression in `Matcher`;
- mutable matcher options after candidates are prepared;
- custom Python scorer callables or processors;
- a `workers=` scheduler in the public API;
- probability, dictionary-cost, or frequency-based ranking;
- per-pair explanations in `cdist` or extraction results.

RapidFuzz-inspired follow-ups should be justified by use: `processor` needs an
explicit normalization and preparation policy; custom scorers need score
direction/range and GIL rules; token scorers need a Japanese/Chinese tokenization
contract. `score_hint` is useful only once there are alternative kernels to
select. Weighted edit costs and unrestricted Damerau require separate semantics
and quality evaluation, not just additional keyword arguments.
