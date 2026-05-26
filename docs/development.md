# Development Notes

This page keeps development, diagnostic, artifact-generation, and publishing
details out of the public-facing README and Website.

## Workspace

```text
crates/moine       public Rust umbrella crate
crates/moine-core  language-independent lattice and edit-distance core
crates/moine-ja    Japanese kana, romaji, override, and UniDic adapters
crates/moine-zh    Chinese pinyin and CC-CEDICT adapters
crates/moine-cli   diagnostic CLI and report generation
```

## Development Checks

Run the Rust suite:

```bash
cargo fmt --check
cargo test
```

Run Python tests and package checks:

```bash
uv run --with '.[test]' python -m pytest python/tests
uv run --with '.[dev]' ruff check python
uv run --with '.[dev]' ruff format --check python
uv run --with '.[dev]' ty check
```

Build the GitHub Pages documentation site and browser demo:

```bash
scripts/build-pages-site.sh
uv run python -m http.server 8765 --bind 127.0.0.1 --directory site
```

Build and smoke-test a local wheel:

```bash
uv run --with maturin maturin build --out /private/tmp/moine-wheel
WHEEL=$(ls /private/tmp/moine-wheel/moine-*.whl)
uv run --with "$WHEEL" \
  python -c 'import moine; print(moine.distance("abc", "adc"))'
```

## Japanese Diagnostics

Compare two strings with the manual override fixture:

```bash
cargo run -q -p moine-cli -- compare \
  --left "いんさt" \
  --right "印刷" \
  --overrides crates/moine-ja/tests/resources/overrides.yaml
```

Inspect UniDic readings for a surface form:

```bash
cargo run -q -p moine-cli -- unidic-csv-readings \
  --surface "刃" \
  --lex-csv unidic-cwj-202512_full/lex.csv
```

Inspect structured reading paths and expansion statistics:

```bash
cargo run -q -p moine-cli -- unidic-csv-sequences \
  --text "鬼滅の刃" \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --longest-only \
  --max-readings-per-segment 16 \
  --max-paths 128
```

Generate the Japanese validation report:

```bash
cargo run -q -p moine-cli -- japanese-report \
  --overrides crates/moine-ja/tests/resources/overrides.yaml \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --longest-only \
  --max-paths 128 \
  --max-readings-per-surface 16 \
  --max-readings-per-segment 16 \
  --output reports/japanese_validation.md
```

The UniDic commands require a local full UniDic package containing `lex.csv`.
Dictionary packages are not committed to this repository.

## UniDic Artifact Recipes

Create a release-style indexed bundle:

```bash
cargo run -q -p moine-cli -- unidic-artifact-bundle \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12 \
  --artifact-name moine-unidic-cwj-202512 \
  --payload-format indexed \
  --max-readings-per-surface 16 \
  --max-readings-per-segment 16 \
  --max-paths 128 \
  --longest-only \
  --output-dir dist/moine-unidic-cwj-202512
```

The checked release recipe wraps bundle generation, verification, and
Vibrato-style archive creation:

```bash
scripts/release-unidic-cwj.sh \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12 \
  --artifact-name moine-unidic-cwj-202512 \
  --payload-format indexed
```

Verify a bundle:

```bash
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml
```

Add `--canonical-checksum` when you want to recompute the slower logical
payload checksum instead of relying on the payload file digest:

```bash
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --canonical-checksum
```

Measure bundle loading plus repeated artifact-backed comparison:

```bash
cargo run -q -p moine-cli --release -- unidic-artifact-runtime-measure \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --pair "いんさt" "印刷" \
  --pair "とうきょうと" "東京都" \
  --warmups 10 \
  --iterations 100
```

## Chinese Diagnostics

Inspect no-tone readings:

```bash
cargo run -q -p moine-cli -- cedict-readings \
  --surface "威士忌" \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt
```

Inspect tone-aware readings:

```bash
cargo run -q -p moine-cli -- cedict-readings \
  --surface "威士忌" \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --pinyin-view tone3
```

Compare ASCII pinyin input with Chinese surface text:

```bash
cargo run -q -p moine-cli -- chinese-compare \
  --left "weishiji" \
  --right "威士忌" \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --longest-only \
  --max-paths 128
```

## CC-CEDICT Artifact Recipes

Create a release-style indexed bundle:

```bash
cargo run -q -p moine-cli -- zh-artifact-bundle \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520 \
  --payload-format indexed \
  --output-dir dist/moine-cedict-20260520 \
  --longest-only \
  --max-paths 128
```

The checked release recipe mirrors the UniDic recipe:

```bash
scripts/release-cedict.sh \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520 \
  --payload-format indexed
```

Verify and use the generated bundle without reading raw CC-CEDICT:

```bash
cargo run -q -p moine-cli -- zh-artifact-verify \
  --metadata dist/moine-cedict-20260520/metadata.yaml

cargo run -q -p moine-cli -- chinese-compare \
  --left "布那哈本" \
  --right "布納哈本" \
  --artifact-metadata dist/moine-cedict-20260520/metadata.yaml
```

## Python Scoring Details

`score_cutoff` follows the RapidFuzz-style contract:

- `distance` and `damerau_distance` return `score_cutoff + 1` when the distance
  exceeds the cutoff;
- `normalized_distance` returns `1.0` when the normalized distance exceeds the
  cutoff;
- `normalized_similarity` and `ratio` return `0.0` when the score is below the
  cutoff.

`moine.cdist(...)` supports plain string scoring and dictionary-backed
`distance`, `damerau_distance`, `normalized_distance`, `normalized_similarity`,
and `ratio`. It returns a plain `list[list[int | float]]`. NumPy/dtype
handling, arbitrary `processor` callbacks, `score_hint`, and `workers` are
intentionally outside the initial API.

`moine.partial_ratio(...)`, `partial_distance(...)`, and
`partial_alignment(...)` search bounded text spans for query-in-text matching.
`partial_alignment` defaults to `metric="ratio"` and reports Python character
offsets.

`moine.ja.process.extract(...)` and `extract_one(...)` are thin candidate
scoring helpers over the dictionary-backed Japanese scorers. Choices may be an
iterable of strings or a mapping from arbitrary keys to strings. Results are
`(choice, score, index_or_key)` tuples and preserve input order for ties.

## Rust API Sketch

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dictionary = moine::zh::load_bundle("dist/moine-cedict-20260520")?;

    assert_eq!(dictionary.distance("weishiji", "威士忌")?, 0);

    Ok(())
}
```

Lower-level lattice APIs remain available at the root:

```rust
use moine::{damerau_distance, distance, Lattice};

let left = Lattice::from_paths(["moine"]);
let right = Lattice::from_paths(["moinya"]);

assert_eq!(distance(&left, &right), 2);
assert_eq!(damerau_distance(&left, &Lattice::from_paths(["mione"])), 1);
```

The lower-level crates remain available for narrower dependencies:
`moine-core`, `moine-ja`, and `moine-zh`.

## Rust Publishing

The public installation target is the `moine` package:

```bash
cargo install moine
moine --help
moine download ja
moine download zh
```

The `moine` package also exposes the Rust library surface. `moine-cli` remains
an implementation/support package so `cargo install moine` can install the
`moine` binary without asking users to know about the internal CLI crate name.

Publish the Rust crates in dependency order:

```bash
cargo publish -p moine-core
cargo publish -p moine-ja
cargo publish -p moine-zh
cargo publish -p moine-cli
cargo publish -p moine
```

Before publishing, run the package checks in the same order. `cargo package`
performs the local verification; downstream crates can only fully verify after
their internal dependencies have already been published to the registry.

```bash
cargo package -p moine-core
cargo package -p moine-ja
cargo package -p moine-zh
cargo package -p moine-cli
cargo package -p moine
```

For the final `moine` package, `cargo package -p moine --list` is a useful
local file-inclusion check before the support crates are present on crates.io.
