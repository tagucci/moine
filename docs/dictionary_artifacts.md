# Dictionary Artifact Notes

This note records the first persistent dictionary artifact boundary for moine.
The current payload can be represented as deterministic YAML, compact binary v1,
or indexed `surface -> readings` data. Verified bundles can also be packaged as
Vibrato-style compressed release assets. The metadata schema, payload name,
payload checksum, and bundle verification boundary are represented in code and
emitted by the CLI.

The initial OSS artifact set should stay intentionally small:

- Japanese: one UniDic-CWJ-derived indexed artifact and one
  SudachiDict-full-derived indexed artifact.
- Chinese: one CC-CEDICT-derived no-tone pinyin artifact.

Additional evaluation-specific artifacts are deferred until after the first
public release.

## UniDic Metadata

The current Japanese schema is `schema_version: 1` and describes a
`moine.unidic.reading-index` artifact. The type name is historical; Sudachi
artifacts reuse the same Japanese `surface -> readings` payload boundary and
record `source.name: SudachiDict` plus `build.reading_field: sudachi-reading`.

It records:

- artifact name and generator
- payload file path, format, file digest, checksum algorithm, and checksum
- source dictionary name, version, and `lex.csv` path
- build-time reading field and index filters
- entry count after filtering
- query defaults used by diagnostics
- license selection and license file references

`query_defaults` intentionally remains in distributed metadata. It is a
recommended runtime profile for this artifact, not part of the canonical
payload identity. Callers may override these values, but Python
`Dictionary.load_bundle(...)`, runtime measurement, and diagnostic examples use
them as the default bounded-expansion policy. This keeps "download this artifact
and compare strings reasonably" self-contained without encoding ranking or
MeCab/Viterbi costs into the dictionary.

Example command:

```bash
cargo run -q -p moine-cli -- unidic-artifact-metadata \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12 \
  --artifact-name moine-unidic-cwj-202512 \
  --max-readings-per-surface 16 \
  --max-readings-per-segment 16 \
  --max-paths 128 \
  --longest-only
```

`unidic-artifact-metadata` does not write a payload file, so it omits
`file_digest_algorithm` and `file_digest`. `unidic-artifact-bundle` fills those
fields after writing the payload. UniDic artifact builders use the `pron`
reading field by default. Observed bundle metadata for the local full CSV
package:

```yaml
schema_version: 1
artifact_type: moine.unidic.reading-index
artifact_name: moine-unidic-cwj-202512
generator: moine-cli
payload:
  path: moine-unidic-cwj-202512.readings.yaml
  format: yaml.surface-readings.v1
  file_digest_algorithm: sha256-file-v1
  file_digest: <64 lowercase hex digits>
  checksum_algorithm: sha256-canonical-v1
  checksum: <64 lowercase hex digits>
source:
  name: UniDic-CWJ
  version: '2025.12'
  lex_csv: unidic-cwj-202512_full/lex.csv
build:
  reading_field: pron
  max_readings_per_surface: 16
  exclude_ascii_surfaces: true
  exclude_symbol_pos: true
  entries: 743163
query_defaults:
  max_span_chars: 8
  max_paths: 128
  longest_match_only: true
  max_readings_per_segment: 16
license:
  selected_license: BSD-3-Clause
  references:
  - label: BSD
    path: license/BSD
  - label: COPYING
    path: license/COPYING
```

Sudachi full artifacts are built from the concatenated raw
`small_lex.csv + core_lex.csv + notcore_lex.csv` source files for a single
SudachiDict release. This mirrors the upstream
[SudachiDict build-from-sources guidance](https://github.com/WorksApplications/SudachiDict#build-from-sources):
"Core dictionary requires small and core files, Full requires all three files."

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

For release assets, use the checked wrapper so bundle generation,
verification, archive creation, and optional checksum-manifest output stay
consistent with the UniDic and CC-CEDICT release paths:

```bash
scripts/release-sudachi-full.sh \
  --lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --source-version 20260428 \
  --license-file /path/to/SudachiDict/LICENSE-2.0.txt \
  --legal-file /path/to/SudachiDict/LEGAL
```

The Sudachi CSV builder reads `surface` column 0, `normalized_form` column 4,
coarse POS column 5, and `reading_form` column 11. By default it excludes
ASCII-only surfaces and symbol POS entries, adds normalized-form aliases, and
keeps raw Sudachi readings. Compare-time lattice construction skips dictionary
paths whose readings cannot be converted to romaji; release artifacts can also
omit those readings up front with `--exclude-unsupported-readings`.

Sudachi release metadata uses `max_span_chars: 24` as the default query window.
With the 20260428 full artifact filters, this leaves a small tail of lookup
keys longer than 24 characters outside the default expansion path. They include
long legal names, long title/artist strings, and unusually long phrase-like
entries. Users who need those exact entries can pass a larger `max_span_chars`
value when comparing or loading through higher-level APIs.

## Payload

The initial payload schema is `schema_version: 1` with payload type
`moine.unidic.reading-index.surface-readings`.

It contains one sorted entry per surface form:

```yaml
schema_version: 1
payload_type: moine.unidic.reading-index.surface-readings
entries:
- surface: 刃
  readings:
  - ジン
  - ハ
  - バ
  - ヤイバ
```

Example command:

```bash
cargo run -q -p moine-cli -- unidic-artifact-payload \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --max-readings-per-surface 16 \
  --output moine-unidic-cwj-202512.readings.yaml
```

Payloads can be loaded back into `UnidicReadingIndex` with validation:

```rust
let index = UnidicReadingIndex::from_artifact_payload_path(
    "moine-unidic-cwj-202512.readings.yaml",
)?;
```

The loader rejects unsupported schema versions, unsupported payload types,
empty surfaces, empty reading lists, empty readings, duplicate surfaces, and
duplicate readings for the same surface.

The CLI can inspect a generated payload and recompute its canonical checksum:

```bash
cargo run -q -p moine-cli -- unidic-artifact-inspect \
  --payload moine-unidic-cwj-202512.readings.yaml
```

The metadata checksum is computed over a deterministic length-prefixed
canonical form of the payload entries, not over the YAML renderer's exact text.
The current algorithm name is `sha256-canonical-v1`. Older internal MVP
metadata that uses `fnv1a64-canonical-v1` is still accepted by bundle
verification and Python loading, but new artifacts should use SHA-256.

## Binary Payload V1

The binary payload preserves the same logical schema as the YAML payload:

```text
surface -> readings
```

The byte layout is fixed and little-endian:

```text
magic           8 bytes   "MOINEU01"
version         u32-le    currently 1
reserved        u32-le    currently 0; non-zero is rejected
entry_count     u64-le

repeated entry_count times:
  surface_len   u32-le
  surface       UTF-8 bytes
  reading_count u32-le

  repeated reading_count times:
    reading_len u32-le
    reading     UTF-8 bytes
```

Load-time validation rejects:

- bad magic
- unsupported binary version
- non-zero reserved header field
- truncated fields
- invalid UTF-8
- unsupported logical schema after decode
- empty surfaces or readings
- duplicate surfaces
- duplicate readings for a surface

The implementation deliberately uses `to_le_bytes` and `from_le_bytes`, never
native-endian struct dumps. A golden byte-layout test locks the exact binary
representation so x86/Linux CI does not need a big-endian runner to catch an
accidental native-endian rewrite.

Example commands:

```bash
cargo run -q -p moine-cli -- unidic-artifact-binary-payload \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --max-readings-per-surface 16 \
  --output moine-unidic-cwj-202512.readings.moinebin

cargo run -q -p moine-cli -- unidic-artifact-binary-inspect \
  --payload moine-unidic-cwj-202512.readings.moinebin \
  --timing
```

## Indexed FST Payload V1

The indexed UniDic payload is an experimental v1 format for reducing runtime
dictionary load cost without changing the logical `surface -> readings`
payload identity. It stores a FST surface index plus offset-addressed reading
blocks in one mmap-backed file:

```text
magic           8 bytes   "MOINEI01"
version         u32-le    currently 1
reserved        u32-le    currently 0; non-zero is rejected
entry_count     u64-le
fst_len         u64-le
readings_len    u64-le
fst bytes       surface -> readings offset
readings bytes  repeated variable-length reading blocks
```

The metadata payload format string is
`indexed-fst.surface-readings.v1`, and the default file extension is
`.readings.moineidx`.

Example command:

```bash
cargo run -q -p moine-cli -- unidic-artifact-bundle \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12 \
  --artifact-name moine-unidic-cwj-202512 \
  --payload-format indexed \
  --output-dir dist/moine-unidic-cwj-202512
```

The current loader maps the file, copies the FST bytes into the `fst` runtime
map, and lazily decodes reading blocks by offset during lookup. It still
validates the indexed payload on load and still supports the same canonical
logical payload checksum. In the local full-UniDic measurement, this reduced
dictionary load time from about `3.95s` for the eager binary
payload to about `1.91s` for the indexed payload. Per-call comparison was
slightly slower because readings are decoded lazily at lookup time instead of
being pre-materialized in a `HashMap`.

## Chinese Pinyin Payload

The zh artifact boundary uses the same high-level idea as the UniDic indexed
artifact. Raw CC-CEDICT remains the source format; the published moine payload
is a normalized reading index:

```yaml
schema_version: 1
payload_type: moine.zh.reading-index.surface-readings
pinyin_view: no-tone
entries:
- surface: 威士忌
  readings:
  - weishiji
```

The payload is view-specific. Generate the default no-tone indexed payload for
IME-style comparison, or generate a separate `tone3` payload for tone-aware
diagnostics. YAML payloads remain useful for inspection, but the release
candidate format is `indexed-fst.surface-readings.v1` with `.moineidx` files.

The local no-tone bundle generated from the 2026-05-20 CC-CEDICT dump is small
enough to distribute through a Python data package:

```text
raw CC-CEDICT dump:      9,859,684 bytes
generated YAML payload:  9,014,460 bytes
gzip release archive:    1,536,746 bytes
zip-compressed bundle:   1,650,623 bytes
entries:                 197,933
```

That makes `moine[zh]` a reasonable future convenience extra, as long as the
data wheel remains separate from the base `moine` package and carries the
CC BY-SA attribution, source metadata, and checksum metadata.

Example commands:

```bash
cargo run -q -p moine-cli -- zh-artifact-payload \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --payload-format yaml \
  --output moine-cedict-20260520.readings.yaml

cargo run -q -p moine-cli -- zh-artifact-bundle \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520 \
  --payload-format indexed \
  --output-dir dist/moine-cedict-20260520 \
  --longest-only \
  --max-paths 128

cargo run -q -p moine-cli -- zh-artifact-verify \
  --metadata dist/moine-cedict-20260520/metadata.yaml

cargo run -q -p moine-cli -- zh-artifact-archive \
  --metadata dist/moine-cedict-20260520/metadata.yaml \
  --output dist/moine-cedict-20260520.tar \
  --compression none
```

The release script wraps this lower-level tar writer and compresses the result
as `*.tar.gz` by default.

```bash
scripts/release-cedict.sh \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520
```

Use `--checksum-manifest` only when the release should also include
`SHA256SUMS`.

```bash
scripts/release-cedict.sh \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520 \
  --checksum-manifest
```

`zh-artifact-verify` checks metadata type/version, payload file digest,
canonical payload checksum, entry count, pinyin view, and license references.
The loader rejects unsupported schema versions, unsupported payload types,
unsupported pinyin views, empty surfaces, empty reading lists, empty readings,
duplicate surfaces, duplicate readings, and readings that are not normalized
for casing, whitespace, or `u:` / `ü`. It does not try to infer whether digits
inside a joined no-tone reading came from numeric tokens or tone numbers; that
no-tone conversion is the generator's responsibility.

## File Layout

The intended first release bundle layout is:

```text
moine-unidic-cwj-202512/
  metadata.yaml
  moine-unidic-cwj-202512.readings.moineidx
  license/
    BSD
    COPYING
```

The default release payload is indexed. YAML remains useful for inspection and
binary remains available as an eager fallback, but public downloads and the
browser demo should use the indexed `.moineidx` payload:

```text
moine-unidic-cwj-202512/
  metadata.yaml
  moine-unidic-cwj-202512.readings.moineidx
  license/
    BSD
    COPYING
```

`metadata.yaml` should use paths relative to this bundle root. The code license
for moine remains separate from the UniDic-derived payload license.

Example bundle command:

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

By default, the command finds license files next to `lex.csv` under
`license/BSD` and `license/COPYING`. Use `--license-dir <DIR>` when the source
package has a different layout.

Verify the generated bundle before publishing:

```bash
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml
```

For bundles generated by `unidic-artifact-bundle`, verification checks
`payload.file_digest` against the raw payload file bytes when present, decodes
the payload, verifies the entry count, and always recomputes
`payload.checksum` against the canonical logical `surface -> readings` payload.
The `--canonical-checksum` flag is retained for compatibility with older
commands, but canonical checksum verification is now the default behavior:

```bash
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --canonical-checksum
```

Create a deterministic release tar archive after verification:

```bash
cargo run -q -p moine-cli -- unidic-artifact-archive \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --output dist/moine-unidic-cwj-202512.tar
```

Use `--compression gzip` to write a deterministic gzip-compressed tar archive:

```bash
cargo run -q -p moine-cli -- unidic-artifact-archive \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --output dist/moine-unidic-cwj-202512.tar.gz \
  --compression gzip
```

Use `--compression zstd` to write a high-compression zstd tar archive:

```bash
cargo run -q -p moine-cli -- unidic-artifact-archive \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --output dist/moine-unidic-cwj-202512.tar.zst \
  --compression zstd
```

Generate a release checksum manifest for archive assets only when a release
needs an external manifest:

```bash
cargo run -q -p moine-cli -- unidic-artifact-release-checksums \
  --asset dist/moine-unidic-cwj-202512.tar.gz \
  --output dist/SHA256SUMS
```

The archive command writes only `metadata.yaml`, the metadata payload path, and
the metadata license references under a root directory named after
`artifact_name` unless `--root-name` is provided.

## Release Channel Policy

`moine` should follow the split used by daachorse and Vibrato:

- ordinary code/package releases stay lightweight and use normal `vX.Y.Z`
  version tags
- generated dictionary artifacts are published separately as GitHub Release
  Assets
- each artifact release note records the source dictionary version, build
  options, reproduction command, verification command, and license boundary
- Rust users load artifacts explicitly by path; dictionary data is not bundled
  into the `moine` crate
- Python users install the base `moine` package without dictionary data, while
  optional extras may depend on separate data wheels or downloader helpers

Dictionary tags use an artifact-specific form such as
`unidic-cwj-202512-v0.1.1`. That keeps generated dictionary releases separate
from `moine` library tags.

Users should download, extract, and pass the artifact explicitly:

```bash
mkdir -p dist/downloads
gh release download unidic-cwj-202512-v0.1.1 \
  --repo tagucci/moine \
  --pattern moine-unidic-cwj-202512.tar.gz \
  --dir dist/downloads

tar -xzf dist/downloads/moine-unidic-cwj-202512.tar.gz -C dist/downloads
```

The extracted bundle is then loaded by path through CLI or Python APIs. The
current policy does not silently download full dictionary artifacts during
package installation, because automatic fetching would also need cache paths,
corporate-network behavior, license display, and mandatory hash verification
semantics.

Python packaging may still offer fugashi-style extras. The model is the same
split as `fugashi[unidic-lite]` for small installable data and
`fugashi[unidic]` plus an explicit `python -m unidic download` step for the full
UniDic package. The first public downloader surface should stay small:

```bash
# Default Japanese artifact: UniDic-CWJ
uv run python -m moine download ja

# Explicit Japanese sources
uv run python -m moine download ja-unidic
uv run python -m moine download ja-sudachi

# Chinese artifact: CC-CEDICT
uv run python -m moine download zh
uv run python -m moine list
uv run python -m moine where
```

`download` is responsible for archive extraction plus mandatory metadata and
payload digest verification before a bundle is written into the local cache. It
also verifies an archive SHA-256 when `--sha256` or `--checksum-url` is supplied,
but a separate `SHA256SUMS` asset is no longer part of the default public
release shape. A separate `verify` command is not part of the initial
user-facing surface; if needed later, it should target an explicit artifact path
for manually downloaded, mirrored, or copied bundles rather than a vague
language name such as `verify ja`.

The implemented downloader uses `MOINE_CACHE_DIR` as the install root override
and otherwise writes to `~/.cache/moine/dictionaries`. `load_dict(lang=...)`
searches explicit language environment variables, `MOINE_DICTIONARIES_PATH`, and
then the default cache. `download` accepts `--url`, `--checksum-url`, and
`--sha256` for mirrored or local release testing, while the normal public path
uses the release URL baked into the package and verifies the unpacked bundle.

```text
moine[zh]
  -> may install a small CC-CEDICT-derived data wheel

moine[ja]
  -> installs helpers for Japanese artifacts; full dictionary downloads remain
     explicit, for example through uv run python -m moine download ja,
     uv run python -m moine download ja-unidic, or uv run python -m moine
     download ja-sudachi

moine[ja-lite]
  -> possible only if a small Japanese artifact is compact enough for PyPI

moine[all]
  -> aggregate convenience extra, without silent full-dictionary downloads
```

```bash
cargo run -q -p moine-cli -- compare \
  --left "いんさt" \
  --right "印刷" \
  --artifact-metadata dist/downloads/moine-unidic-cwj-202512/metadata.yaml
```

`compare --artifact-metadata` verifies the payload file digest, loads the
payload path and format from metadata, and uses `query_defaults` unless the
caller supplies query options such as `--max-paths` or
`--max-readings-per-segment`.

## Release Recipe

The checked UniDic recipe script builds the release CLI, creates the indexed
bundle, runs fast and canonical verification, and writes a compressed release
asset:

```bash
scripts/release-unidic-cwj.sh \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12
```

Default outputs:

```text
dist/moine-unidic-cwj-202512/
dist/moine-unidic-cwj-202512.tar.gz
```

The checked SudachiDict-full recipe uses the same Japanese payload and archive
boundary while preserving both required SudachiDict notice files. Its input CSV
should be the all-three-file Full source build described by the upstream
[SudachiDict build-from-sources notes](https://github.com/WorksApplications/SudachiDict#build-from-sources):

```bash
scripts/release-sudachi-full.sh \
  --lex-csv /tmp/sudachi-raw-20260428/full_lex.csv \
  --source-version 20260428 \
  --license-file /path/to/SudachiDict/LICENSE-2.0.txt \
  --legal-file /path/to/SudachiDict/LEGAL \
  --compression gzip

target/release/moine unidic-artifact-archive \
  --metadata dist/moine-sudachi-full-20260428/metadata.yaml \
  --output dist/moine-sudachi-full-20260428.tar.zst \
  --compression zstd
```

Release outputs:

```text
dist/moine-sudachi-full-20260428/
dist/moine-sudachi-full-20260428.tar.gz
dist/moine-sudachi-full-20260428.tar.zst
```

The checked CC-CEDICT recipe mirrors the same archive boundary for the first
Chinese no-tone artifact:

```bash
scripts/release-cedict.sh \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520 \
  --compression gzip

target/release/moine zh-artifact-archive \
  --metadata dist/moine-cedict-20260520/metadata.yaml \
  --output dist/moine-cedict-20260520.tar.zst \
  --compression zstd
```

Release outputs:

```text
dist/moine-cedict-20260520/
dist/moine-cedict-20260520.tar.gz
dist/moine-cedict-20260520.tar.zst
```

The UniDic script accepts `--artifact-name`, `--dist-dir`, `--license-dir`,
`--payload-format`, and `--compression`, plus environment overrides for
`MAX_READINGS_PER_SURFACE`, `MAX_READINGS_PER_SEGMENT`, `MAX_PATHS`,
`RELEASE_PAYLOAD_FORMAT`, `RELEASE_COMPRESSION`, `RELEASE_CHECKSUM_MANIFEST`,
and `MOINE_BIN`. Use `--checksum-manifest` when a release should also emit
`dist/SHA256SUMS`. The script intentionally does not create a GitHub Release or
sign assets; publishing and key management are kept outside the local build
recipe.

The Sudachi script accepts `--artifact-name`, `--dist-dir`, `--license-file`,
`--legal-file`, `--payload-format`, `--compression`, `--max-span-chars`,
`--include-unsupported-readings`, and `--checksum-manifest`, plus matching
limit and `MOINE_BIN` environment overrides. Its default payload format is
indexed, its default `max_span_chars` is 24, and its default release build
excludes readings unsupported by the current romaji converter.

The CC-CEDICT script accepts `--artifact-name`, `--dist-dir`, `--license-file`,
`--pinyin-view`, `--payload-format`, `--compression`, and
`--checksum-manifest`, plus matching limit and `MOINE_BIN` environment
overrides. Its default payload format is indexed.

Release integrity has three layers:

- `payload.file_digest` verifies the raw payload file inside an unpacked bundle.
- `payload.checksum` verifies the canonical logical `surface -> readings`
  payload whenever metadata includes a checksum.
- optional `SHA256SUMS` verifies the published release asset, such as
  `moine-unidic-cwj-202512.tar.gz`, before unpacking.

Detached signing should be applied to `SHA256SUMS` or to the final archive
asset outside `metadata.yaml`. The metadata schema should not embed signatures
until there is a concrete key-management and rotation policy.

Observed with the local `unidic-cwj-202512_full/lex.csv` package on
2026-05-20:

```text
entries:              743163
binary payload:       28,546,877 bytes
gzip release archive: 4,379,095 bytes
metadata:             931 bytes
bundle generation:    20.70 s real
gzip archive:         9.09 s real
binary inspect:       6.15 s real
```

The gzip size is already small enough for an initial GitHub Release Asset. The
eager binary payload remains the fallback format, while the indexed FST payload
is now the preferred release-candidate format for reducing startup cost.

`unidic-artifact-binary-inspect --timing` breaks down the inspect path:

```text
read file:       4.227 ms
decode binary:   1135.411 ms
checksum:        2700.270 ms
process total:   4.05 s real
```

The file read is negligible, binary decode is about one second, and the
canonical checksum dominates. This motivated the current split between fast
release-file integrity checks and opt-in canonical logical-payload checks.

After adding `sha256-file-v1` and binary-header entry-count verification,
bundle verification can skip canonical checksum recomputation and full binary
payload decoding for normal integrity checks:

```text
fast verify:      1.08 s real
canonical verify: 6.66 s real
```

The fast output reports `entry_count_source: binary_header` and
`canonical_checksum: skipped`. Forced canonical verification still decodes the
payload, reports `entry_count_source: decoded_payload`, and checks the
`sha256-canonical-v1` logical checksum.

`unidic-artifact-runtime-measure` measures the runtime path that users exercise
after downloading a bundle: metadata parsing, payload file digest verification,
payload decoding, and repeated artifact-backed comparisons using the metadata
`query_defaults`.

```bash
cargo run -q -p moine-cli --release -- unidic-artifact-runtime-measure \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --pair "いんさt" "印刷" \
  --pair "きめつのやいば" "鬼滅の刃" \
  --pair "とうきょうと" "東京都" \
  --pair "愛知家コロナ" "愛知県コロナ" \
  --pair "マトリッツォ" "マリトッツォ" \
  --warmups 10 \
  --iterations 100
```

Observed with the release build and the local full UniDic binary bundle:

```text
entries:                 743163
file digest verification: 84.800-108.236 ms
payload decode:          1815.061-1874.802 ms
load total:              1900.059-1962.710 ms
pair count:              5
measured comparisons:    500
compare average:         0.148-0.151 ms
process total:           2.10-2.56 s real
```

Python `Dictionary.load_bundle(...)` measured against the same bundle loaded in
1916.976 ms and then averaged 0.149 ms per `distance(...)` call over the same
five pairs. Later full-UniDic indexed FST measurement reduced load time from
about 3.95 s to about 1.91 s on the same local validation workload, with a
small per-call slowdown from lazy reading-block decode. This makes the indexed
payload worth carrying into the first public artifact while keeping the eager
binary payload as a simpler fallback.

Generated payloads can also be used directly by the diagnostic `compare` CLI:

```bash
cargo run -q -p moine-cli -- compare \
  --left "いんさt" \
  --right "印刷" \
  --artifact-payload dist/moine-unidic-cwj-202512/moine-unidic-cwj-202512.readings.moinebin \
  --payload-format binary \
  --max-readings-per-segment 16 \
  --max-paths 128 \
  --longest-only
```

Python can load the whole bundle from its directory or from `metadata.yaml`.
The metadata payload path, payload format, and query defaults are applied
automatically. Loading also verifies the payload file digest when present, then
checks the entry count:

```python
from moine.ja import Dictionary

dictionary = Dictionary.load_bundle("dist/downloads/moine-unidic-cwj-202512")
dictionary.distance("いんさt", "印刷")
```

Verification currently checks:

- metadata can be parsed
- payload path exists relative to the bundle root
- payload path stays inside the bundle root and uses `/` separators
- payload file digest matches metadata when present
- canonical payload checksum matches metadata when no file digest is present,
  or when `--canonical-checksum` is requested
- payload entry count matches metadata
- each license reference exists relative to the bundle root

## Still Open

- concrete signing tool/key policy for release assets
- whether the indexed payload needs a small hot lookup cache to recover the
  eager binary payload's slightly faster per-call comparison time
