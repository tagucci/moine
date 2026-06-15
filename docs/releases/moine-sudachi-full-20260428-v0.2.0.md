# moine-sudachi-full-20260428-v0.2.0

Initial public release of the `moine` SudachiDict-full reading-index artifact.

This release includes both gzip and zstd archive assets for the same indexed
payload. The gzip asset is the default downloader target; the zstd asset is
provided for users who prefer faster local extraction.

## Assets

- `moine-sudachi-full-20260428.tar.gz` (29,263,578 bytes)
- `moine-sudachi-full-20260428.tar.zst` (23,761,074 bytes)

## Build Inputs

- Source dictionary: SudachiDict
- Source version: `20260428`
- Source CSV: `small_lex.csv + core_lex.csv + notcore_lex.csv`
- Reading field: `sudachi-reading`
- Artifact name: `moine-sudachi-full-20260428`
- Payload format: `indexed-fst.surface-readings.v1`

SudachiDict's upstream
[build-from-sources notes](https://github.com/WorksApplications/SudachiDict#build-from-sources)
state: "Core dictionary requires small and core files, Full requires all three
files."

## Build Options

```text
max_readings_per_surface: 16
max_readings_per_segment: 16
max_span_chars: 24
max_paths: 128
longest_match_only: true
exclude_ascii_surfaces: true
exclude_symbol_pos: true
include_normalized_surfaces: true
exclude_unsupported_readings: true
entries: 2309794
```

The default query window does not consider exact dictionary keys longer than 24
characters. In the 20260428 full build, the ignored tail is mostly long legal
names, long title/artist strings, and unusually long phrase-like entries. Users
who need those entries can override `max_span_chars` at comparison time.

## Checksums

```text
5047e9c81c96f16622cc1cbe5b0d27692285af5f233d437ddd62a1020deff172  moine-sudachi-full-20260428.tar.gz
a6da9563ceaae392d09cfb9c90078499a3a88487be537aa5ab6315e9da45c74a  moine-sudachi-full-20260428.tar.zst
```

Payload checksums inside `metadata.yaml`:

```text
sha256-file-v1:      953537ab875984a7606bc7d084f12108a85d46e7172544d26a04ac04faf38f81
sha256-canonical-v1: cab3e543cd90fcad4b021a717a3d4acaf27b07ef4fe539993adce3f2ddcb0655
```

## Reproduction

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

## Verification

```bash
shasum -a 256 moine-sudachi-full-20260428.tar.gz \
  moine-sudachi-full-20260428.tar.zst
tar -xzf moine-sudachi-full-20260428.tar.gz
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata moine-sudachi-full-20260428/metadata.yaml
cargo run -q -p moine-cli -- compare \
  --left "きめつのやいば" \
  --right "鬼滅の刃" \
  --artifact-metadata moine-sudachi-full-20260428/metadata.yaml
```

Python can use the baked-in `ja-sudachi` downloader after the release asset is
published:

```bash
uv run python -m moine download ja-sudachi
```

Python can also load the extracted bundle directory directly:

```python
from moine.ja import Dictionary

dictionary = Dictionary.load_bundle("moine-sudachi-full-20260428")
dictionary.distance("きめつのやいば", "鬼滅の刃")
```

## License

The archive includes SudachiDict license references under
`license/LICENSE-2.0.txt` and `license/LEGAL`. The dictionary data is
SudachiDict-derived and separate from the `moine` code license.

## Notes

Code/package releases remain separate from generated dictionary artifacts.
