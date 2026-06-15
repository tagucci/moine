# moine-cedict-20260520-v0.1.1

Current public release of the `moine` CC-CEDICT no-tone pinyin artifact.

This release includes both gzip and zstd archive assets for the same indexed
payload. The gzip asset is the default downloader target; the zstd asset is
provided for users who prefer faster local extraction.

## Assets

- `moine-cedict-20260520.tar.gz` (1,979,620 bytes)
- `moine-cedict-20260520.tar.zst` (1,723,751 bytes)

## Build Inputs

- Source dictionary: CC-CEDICT
- Source version: `2026-05-20`
- Pinyin view: `no-tone`
- Artifact name: `moine-cedict-20260520`
- Payload format: `indexed-fst.surface-readings.v1`

## Build Options

```text
max_readings_per_surface: 16
max_readings_per_segment: 16
max_span_chars: 8
max_paths: 128
longest_match_only: true
entries: 197933
```

## Checksums

```text
846d8d2c3417f240bfa5acee46ba7dff8e37951ba514a60fb682aa7ff46aad6a  moine-cedict-20260520.tar.gz
5f5b548f5c71ead98fe077754acbbc066e47c21b8abb793fc23ce88b58166a28  moine-cedict-20260520.tar.zst
```

Payload checksums inside `metadata.yaml`:

```text
sha256-file-v1:      f6087a4f47af64f39f668c76a36c668ca4a47b5b9ddc4b00d65e12bc121e76e9
sha256-canonical-v1: 31829b86c7fb9c80029b0638b434184041c37c749568251d0f1f8b570f8d361e
```

## Reproduction

```bash
scripts/release-cedict.sh \
  --cedict cedict_1_0_ts_utf-8_mdbg.txt \
  --source-version 2026-05-20 \
  --artifact-name moine-cedict-20260520 \
  --payload-format indexed \
  --dist-dir dist \
  --compression gzip

target/release/moine zh-artifact-archive \
  --metadata dist/moine-cedict-20260520/metadata.yaml \
  --output dist/moine-cedict-20260520.tar.zst \
  --compression zstd
```

## Verification

```bash
shasum -a 256 moine-cedict-20260520.tar.gz \
  moine-cedict-20260520.tar.zst
tar -xzf moine-cedict-20260520.tar.gz
cargo run -q -p moine-cli -- zh-artifact-verify \
  --metadata moine-cedict-20260520/metadata.yaml
```

Python can use the baked-in `zh` downloader:

```bash
uv run python -m moine download zh
```

Python can also load the extracted bundle directory directly:

```python
import moine

moine.distance("weishiji", "威士忌", lang="zh")
moine.distance("布納哈奔", "布納哈本", lang="zh")
```

## License

The generated artifact is derived from CC-CEDICT and must carry CC BY-SA 4.0
attribution and source metadata separately from the `moine` code license. Raw
CC-CEDICT glosses are not included in the runtime payload.

## Notes

Code/package releases remain separate from generated dictionary artifacts.
