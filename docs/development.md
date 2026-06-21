# Development Notes

This page keeps development, diagnostic, artifact-generation, and publishing
details out of the public-facing README and Website.

## Workspace

```text
crates/moine         public Rust umbrella crate and cargo-installable binary
crates/moine-core    language-independent lattice and edit-distance core
crates/moine-ja      Japanese kana, romaji, override, UniDic, and Sudachi adapters
crates/moine-zh      Chinese pinyin and CC-CEDICT adapters
crates/moine-cli     CLI implementation, diagnostics, downloads, and artifacts
crates/moine-python  PyO3 extension module backing the Python package
crates/moine-wasm    wasm-bindgen bindings for the browser demo
```

## Development Checks

Run the Rust suite:

```bash
cargo fmt --check
cargo test
```

Run Python tests and package checks:

```bash
WHEEL_DIR=$(mktemp -d)
uv run --no-project --with 'maturin>=1.9,<2' maturin build --out "$WHEEL_DIR"
WHEEL=$(ls "$WHEEL_DIR"/moine-*.whl)
uv run --no-project --with "$WHEEL" \
  python -I -c 'import moine; print(moine.__version__); print(moine.__file__)'
uv run --no-project --with pip --with "$WHEEL" python -m pip check
uv run --no-project --with 'pytest>=8,<9' --with "$WHEEL" python -m pytest python/tests
uv run --no-project --with 'ruff>=0.14,<0.15' ruff check python
uv run --no-project --with 'ruff>=0.14,<0.15' ruff format --check python
uv run --no-project --with 'ty>=0.0.38,<0.1' ty check
```

Use the `--no-project` form for local checks so `uv` installs only the tool
being run. Build a fresh local wheel first, smoke-test that exact wheel with
isolated import and `pip check`, then install it into the temporary pytest
environment so cached `moine` wheels with the same version cannot shadow the
checkout.

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
  python -I -c 'import moine; print(moine.__version__); print(moine.__file__); print(moine.distance("abc", "adc"))'
uv run --with pip --with "$WHEEL" python -m pip check
```

Run the compact Python speed benchmark and print a Markdown table:

```bash
PYTHON=${PYTHON:-python3.14}
WHEEL_DIR=$(mktemp -d)
uv run --python "$PYTHON" --no-project --with 'maturin>=1.9,<2' \
  maturin build --release --out "$WHEEL_DIR"
WHEEL=$(ls "$WHEEL_DIR"/moine-*.whl)
uv run --python "$PYTHON" --no-project --with rapidfuzz --with "$WHEEL" \
  python scripts/benchmark_distances.py \
  --dictionary dist/moine-unidic-cwj-202512/metadata.yaml \
  --loops 10000
```

Use Python 3.14 for comparable local speed numbers. The benchmark keeps
dictionary loading outside the timed loop, reports that load time separately,
and compares RapidFuzz surface Levenshtein with the configured moine Japanese
dictionary metric. The `mean ± std` timing is computed from pair-level
microseconds per call across the curated 10-pair smoke corpus. Dictionary
loading is timed separately as 10 fresh loads by default; pass
`--dictionary-load-repeats` to adjust that count. Use `--score-cutoff` to
measure the cutoff scoring path separately.

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

Render the romaji lattice used by comparison as SVG:

```bash
cargo run -q -p moine-cli -- compare \
  --left "蒸溜所" \
  --right "蒸留所" \
  --artifact-metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --romaji-lattice lattice.svg \
  --output-format svg
```

`--output-format svg` and `--output-format png` call the Graphviz `dot`
command. Use `--output-format dot` when you want DOT text without depending on
a local Graphviz installation.

The UniDic commands require a local full UniDic package containing `lex.csv`.
Dictionary packages are not committed to this repository.

## Sudachi Diagnostics

SudachiDict raw CSV files are published separately from the GitHub release
dictionary zips. The upstream
[SudachiDict build-from-sources notes](https://github.com/WorksApplications/SudachiDict#build-from-sources)
state: "Core dictionary requires small and core files, Full requires all three
files." Download the three raw lexicon files from the same release and
concatenate them to build a full-equivalent CSV:

```bash
mkdir -p /tmp/sudachi-raw-20260428

curl -L -o /tmp/sudachi-raw-20260428/small_lex.zip \
  http://sudachi.s3-website-ap-northeast-1.amazonaws.com/sudachidict-raw/20260428/small_lex.zip
curl -L -o /tmp/sudachi-raw-20260428/core_lex.zip \
  http://sudachi.s3-website-ap-northeast-1.amazonaws.com/sudachidict-raw/20260428/core_lex.zip
curl -L -o /tmp/sudachi-raw-20260428/notcore_lex.zip \
  http://sudachi.s3-website-ap-northeast-1.amazonaws.com/sudachidict-raw/20260428/notcore_lex.zip

unzip -p /tmp/sudachi-raw-20260428/small_lex.zip small_lex.csv \
  > /tmp/sudachi-raw-20260428/full_lex.csv
unzip -p /tmp/sudachi-raw-20260428/core_lex.zip core_lex.csv \
  >> /tmp/sudachi-raw-20260428/full_lex.csv
unzip -p /tmp/sudachi-raw-20260428/notcore_lex.zip notcore_lex.csv \
  >> /tmp/sudachi-raw-20260428/full_lex.csv
```

Inspect Sudachi readings for a surface form:

```bash
cargo run -q -p moine-cli -- sudachi-csv-readings \
  --surface "鬼滅の刃" \
  --lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --max-readings-per-surface 16
```

Compare with Sudachi raw CSV directly:

```bash
cargo run -q -p moine-cli -- compare \
  --left "きめつのやいば" \
  --right "鬼滅の刃" \
  --sudachi-lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --max-readings-per-surface 16 \
  --max-readings-per-segment 16 \
  --max-paths 128 \
  --longest-only
```

Render the same kind of comparison as a romaji lattice graph:

```bash
cargo run -q -p moine-cli -- compare \
  --left "呪術廻戦" \
  --right "ジュジュツカイセン" \
  --sudachi-lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --max-readings-per-surface 16 \
  --max-readings-per-segment 16 \
  --max-paths 128 \
  --longest-only \
  --romaji-lattice jujutsu-sudachi.svg \
  --output-format svg
```

The public website shows the installed-artifact version of this example in
[CLI usage](https://tagucci.github.io/moine/cli/).

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

Use `--compression zstd` when preparing the `.tar.zst` release asset directly
from the raw UniDic CSV:

```bash
scripts/release-unidic-cwj.sh \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12 \
  --compression zstd
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

## Sudachi Artifact Recipes

Create a release-style indexed bundle from the concatenated full CSV. This
follows the upstream
[SudachiDict source-build requirement](https://github.com/WorksApplications/SudachiDict#build-from-sources)
for Full dictionaries, which requires all three raw source files:

```bash
cargo run -q -p moine-cli -- sudachi-artifact-bundle \
  --lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --source-version 20260428 \
  --artifact-name moine-sudachi-full-20260428 \
  --payload-format indexed \
  --max-readings-per-surface 16 \
  --exclude-unsupported-readings \
  --max-readings-per-segment 16 \
  --max-span-chars 24 \
  --max-paths 128 \
  --longest-only \
  --license-file /path/to/SudachiDict/LICENSE-2.0.txt \
  --legal-file /path/to/SudachiDict/LEGAL \
  --output-dir dist/moine-sudachi-full-20260428
```

`--license-file` and `--legal-file` are required so the generated bundle
metadata always references both the SudachiDict Apache-2.0 license text and the
upstream legal notice. The release default `--max-span-chars 24` covers nearly
all release-shaped Sudachi lookup keys while keeping segmentation bounded; exact
keys longer than 24 characters require a caller override such as
`max_span_chars=32` or higher. The existing artifact verification and archive
commands operate on the generated metadata:

```bash
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata dist/moine-sudachi-full-20260428/metadata.yaml
```

For release assets, prefer the checked wrapper:

```bash
scripts/release-sudachi-full.sh \
  --lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --source-version 20260428 \
  --license-file /path/to/SudachiDict/LICENSE-2.0.txt \
  --legal-file /path/to/SudachiDict/LEGAL
```

Use `--compression zstd` when preparing the `.tar.zst` release asset directly
from the raw Sudachi CSV:

```bash
scripts/release-sudachi-full.sh \
  --lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --source-version 20260428 \
  --license-file /path/to/SudachiDict/LICENSE-2.0.txt \
  --legal-file /path/to/SudachiDict/LEGAL \
  --compression zstd
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

Render the pinyin lattice used by Chinese comparison:

```bash
cargo run -q -p moine-cli -- chinese-compare \
  --left "tiaoheweishiji" \
  --right "调和威士忌" \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --max-paths 128 \
  --pinyin-lattice tiaoheweishiji-pinyin.svg \
  --output-format svg
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

Use `--compression zstd` when preparing the `.tar.zst` release asset directly
from the raw CC-CEDICT dump:

```bash
scripts/release-cedict.sh \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --compression zstd
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

- `distance`, `damerau_distance`, and `combined_distance` return
  `score_cutoff + 1` when the distance exceeds the cutoff;
- `normalized_distance` returns `1.0` when the normalized distance exceeds the
  cutoff;
- `normalized_similarity` and `ratio` return `0.0` when the score is below the
  cutoff.

`moine.cdist(...)` supports plain string scoring and dictionary-backed
`distance`, `damerau_distance`, `combined_distance`, `normalized_distance`,
`normalized_similarity`, and `ratio`. It returns a plain
`list[list[int | float]]`. NumPy/dtype handling, arbitrary `processor`
callbacks, `score_hint`, and `workers` are
intentionally outside the initial API.

`moine.partial_ratio(...)`, `partial_distance(...)`, and
`partial_alignment(...)` search bounded text spans for query-in-text matching.
`partial_alignment` defaults to `metric="ratio"` and reports Python character
offsets.

`moine.ja.process.extract(...)`, `moine.zh.process.extract(...)`, and
`extract_one(...)` are thin candidate scoring helpers over the
language-specific dictionary-backed scorers. Choices may be an iterable of
strings or a mapping from arbitrary keys to strings. Results are
`(choice, score, index_or_key)` tuples and preserve input order for ties.

## Rust API Sketch

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dictionary = moine::zh::load_bundle("dist/moine-cedict-20260520")?;

    assert_eq!(dictionary.distance("weishiji", "威士忌")?, 0);
    assert_eq!(dictionary.combined_distance("weishiji", "wieshiji")?, 1);

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

`ja` installs the default Japanese artifact, currently UniDic-CWJ. Use
`ja-unidic` for an explicit UniDic-CWJ selector and `ja-sudachi` for
SudachiDict-full.

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
