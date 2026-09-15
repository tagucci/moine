# moine-sudachi-full-20260723-v0.2.0

Refresh the SudachiDict-full reading index from `20260428` to
[upstream `v20260723`](https://github.com/WorksApplications/SudachiDict/releases/tag/v20260723).
The artifact format and reading/expansion policies are unchanged. The `v0.2.0`
suffix continues the existing artifact format revision; the source date identifies
this new dictionary release.

## Publication

Published as [moine-sudachi-full-20260723-v0.2.0](https://github.com/tagucci/moine/releases/tag/moine-sudachi-full-20260723-v0.2.0).
Both archives match the published `SHA256SUMS`. Updated Rust and Python
downloaders installed the dictionary from the public URLs into separate empty
caches without URL overrides. Both returned LPED `0` for
`すまーとのうぎょう` compared with `スマート農業`.

## Assets

- `moine-sudachi-full-20260723.tar.gz` (29,277,745 bytes)
- `moine-sudachi-full-20260723.tar.zst` (23,772,647 bytes)

The gzip archive is the default downloader target. Both archives contain the
same indexed payload, metadata, and upstream `LICENSE-2.0.txt` and `LEGAL` from
`v20260723`. Dictionary licensing remains separate from the code license.

## Build inputs and options

Concatenate the official `small_lex.csv`, `core_lex.csv`, and `notcore_lex.csv`
for `20260723`. See the download recipe in `docs/development.md`.

```text
payload_format: indexed-fst.surface-readings.v1
reading_field: sudachi-reading
max_readings_per_surface: 16
max_readings_per_segment: 16
max_span_chars: 24
max_paths: 128
longest_match_only: true
exclude_ascii_surfaces: true
exclude_symbol_pos: true
include_normalized_surfaces: true
exclude_unsupported_readings: true
entries: 2310387
```

## Checksums

```text
6331f31a38186c5b898942c82e9725f5d5c0742b79d178dc507c346f341e68f1  moine-sudachi-full-20260723.tar.gz
076f8249c67dbaf1455dc8ecc960d9976053d7af1d0f2f1d88d743bc69533097  moine-sudachi-full-20260723.tar.zst
```

Payload checksums:

```text
sha256-file-v1:      50467635c2b0c8c1bb87f05536c140623060401203c788f88602c2cb536ab0ba
sha256-canonical-v1: 816b1656b79dff355d08de377bfb0645380ff7490b63cd2275f0fe03e6a6abf1
```

## Reproduction

```bash
scripts/release-sudachi-full.sh \
  --lex-csv /tmp/sudachi-raw-20260723/full_lex.csv \
  --source-version 20260723 \
  --license-file /tmp/sudachi-raw-20260723/LICENSE-2.0.txt \
  --legal-file /tmp/sudachi-raw-20260723/LEGAL \
  --dist-dir dist/sudachi-20260723

target/release/moine unidic-artifact-archive \
  --metadata dist/sudachi-20260723/moine-sudachi-full-20260723/metadata.yaml \
  --output dist/sudachi-20260723/moine-sudachi-full-20260723.tar.zst \
  --compression zstd

target/release/moine unidic-artifact-release-checksums \
  --asset dist/sudachi-20260723/moine-sudachi-full-20260723.tar.gz \
  --asset dist/sudachi-20260723/moine-sudachi-full-20260723.tar.zst \
  --output dist/sudachi-20260723/SHA256SUMS
```

## Local verification

The generated bundle passed file-digest and canonical-checksum verification.
Both Rust and Python downloaders installed the gzip archive into separate empty
caches using local archive and checksum-manifest overrides. Comparison of
`すまーとのうぎょう` with `スマート農業` returned LPED `0`.
Public-URL verification is recorded above.
