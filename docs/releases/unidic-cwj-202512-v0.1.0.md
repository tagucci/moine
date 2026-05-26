# UniDic-CWJ 2025.12 Reading Index v0.1.0

Initial public release of the `moine` UniDic-CWJ reading-index artifact.

This release includes both gzip and zstd archive assets for the same binary
payload. The zstd asset supports the Vibrato-style prebuilt artifact path while
the gzip asset remains the default downloader target.

## Assets

- `moine-unidic-cwj-202512.tar.gz`
- `moine-unidic-cwj-202512.tar.zst`
- `SHA256SUMS`

## Build Inputs

- Source dictionary: UniDic-CWJ
- Source version: `2025.12`
- Source CSV: `unidic-cwj-202512_full/lex.csv`
- Reading field: `lform`
- Artifact name: `moine-unidic-cwj-202512`
- Payload format: `binary.surface-readings.v1`

## Build Options

```text
max_readings_per_surface: 16
max_readings_per_segment: 16
max_paths: 128
longest_match_only: true
exclude_ascii_surfaces: true
exclude_symbol_pos: true
```

## Checksums

```text
25c3bad74b94795ecb10f05a5a9cc33a77834d62ab8520bba330dc1f92be66f5  moine-unidic-cwj-202512.tar.gz
fce90a42226ac94211c251dce0eb53cfab2e969c89e3e2e2ebd8c1dc3d5b31d1  moine-unidic-cwj-202512.tar.zst
```

Payload checksums inside `metadata.yaml`:

```text
sha256-file-v1:      39cd852ed2cf8113b3b961401bd948bc73c169d95eb013fb84de07c3258253d4
sha256-canonical-v1: df9f5337342531915c5b67877f43cac786b93707876077b01445d9ca6379cb64
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

target/release/moine unidic-artifact-release-checksums \
  --asset dist/moine-unidic-cwj-202512.tar.gz \
  --asset dist/moine-unidic-cwj-202512.tar.zst \
  --output dist/SHA256SUMS
```

## Verification

```bash
shasum -a 256 -c SHA256SUMS
tar -xzf moine-unidic-cwj-202512.tar.gz
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata moine-unidic-cwj-202512/metadata.yaml
cargo run -q -p moine-cli -- compare \
  --left "いんさt" \
  --right "印刷" \
  --artifact-metadata moine-unidic-cwj-202512/metadata.yaml
```

Python can load the extracted bundle directory directly:

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
