# UniDic-CWJ 2025.12 Reading Index v0.1.1

Current public release of the `moine` UniDic-CWJ reading-index artifact.

This release includes both gzip and zstd archive assets for the same indexed
payload. The gzip asset is the default downloader target; the zstd asset is
provided for users who prefer faster local extraction.

## Assets

- `moine-unidic-cwj-202512.tar.gz` (4,117,042 bytes)
- `moine-unidic-cwj-202512.tar.zst` (3,548,181 bytes)

## Build Inputs

- Source dictionary: UniDic-CWJ
- Source version: `2025.12`
- Source CSV: `unidic-cwj-202512_full/lex.csv`
- Reading field: `lform`
- Artifact name: `moine-unidic-cwj-202512`
- Payload format: `indexed-fst.surface-readings.v1`

## Build Options

```text
max_readings_per_surface: 16
max_readings_per_segment: 16
max_span_chars: 8
max_paths: 128
longest_match_only: true
exclude_ascii_surfaces: true
exclude_symbol_pos: true
entries: 751936
```

## Checksums

```text
74288e91eed7466b1824f6fed019b25b2fd4148453bbcd2f80eb23a79aa419de  moine-unidic-cwj-202512.tar.gz
6b62bdaf4e2742067b983c969442d3ad78fb2621285cd1d62834f5572a370f33  moine-unidic-cwj-202512.tar.zst
```

Payload checksums inside `metadata.yaml`:

```text
sha256-file-v1:      81a47ce845a53ad8c3c4c538f5cf4fbb66260579f1805b2148524a69459700e5
sha256-canonical-v1: 5b6747aa0fb0ec1860a56fc140876308b028b977429eb0397b8fa476665ec9c4
```

## Reproduction

```bash
scripts/release-unidic-cwj.sh \
  --lex-csv unidic-cwj-202512_full/lex.csv \
  --source-version 2025.12 \
  --compression gzip

target/release/moine unidic-artifact-archive \
  --metadata dist/moine-unidic-cwj-202512/metadata.yaml \
  --output dist/moine-unidic-cwj-202512.tar.zst \
  --compression zstd
```

## Verification

```bash
shasum -a 256 moine-unidic-cwj-202512.tar.gz \
  moine-unidic-cwj-202512.tar.zst
tar -xzf moine-unidic-cwj-202512.tar.gz
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata moine-unidic-cwj-202512/metadata.yaml
cargo run -q -p moine-cli -- compare \
  --left "いんさt" \
  --right "印刷" \
  --artifact-metadata moine-unidic-cwj-202512/metadata.yaml
```

Python can use the baked-in `ja` or `ja-unidic` downloader:

```bash
uv run python -m moine download ja
uv run python -m moine download ja-unidic
```

Python can also load the extracted bundle directory directly:

```python
from moine.ja import Dictionary

dictionary = Dictionary.load_bundle("moine-unidic-cwj-202512")
dictionary.distance("いんさt", "印刷")
```

## License

The archive includes UniDic license references under `license/BSD` and
`license/COPYING`. The dictionary data is UniDic-derived and separate from the
`moine` code license.

## Notes

Code/package releases remain separate from generated dictionary artifacts.
