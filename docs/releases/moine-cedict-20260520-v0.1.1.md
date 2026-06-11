# moine-cedict-20260520-v0.1.1

Release of the `moine` CC-CEDICT no-tone pinyin artifact for moine v0.1.1.

The payload content is unchanged from the v0.1.0 artifact release. This release
keeps the same source dictionary version, artifact name, indexed payload format,
and bounded expansion settings under a v0.1.1 artifact tag.

## Assets

- `moine-cedict-20260520.tar.gz`
- `moine-cedict-20260520.tar.zst`

## Source

- Dictionary: CC-CEDICT
- Source version: `2026-05-20`
- Pinyin view: `no-tone`
- Runtime payload: indexed FST/mmap-friendly normalized `surface -> pinyin readings`
- Artifact name: `moine-cedict-20260520`

## Build Options

```text
max_readings_per_surface: 16
max_readings_per_segment: 16
max_paths: 128
longest_match_only: true
```

## Checksums

```text
75f2822f212928f4f4f2ddfbc1736f6b44f3c64f8d3f94dcdfbd53859a60d19b  moine-cedict-20260520.tar.gz
8ade18cbf74241123c38a09bedf1d4369d3150751641dd6688a61bdbb4d70887  moine-cedict-20260520.tar.zst
```

Payload checksums inside `metadata.yaml`:

```text
sha256-file-v1:      f6087a4f47af64f39f668c76a36c668ca4a47b5b9ddc4b00d65e12bc121e76e9
sha256-canonical-v1: 31829b86c7fb9c80029b0638b434184041c37c749568251d0f1f8b570f8d361e
```

## Verification

```bash
tar -xzf moine-cedict-20260520.tar.gz
cargo run -q -p moine-cli -- zh-artifact-verify \
  --metadata moine-cedict-20260520/metadata.yaml
```

## License

The generated artifact is derived from CC-CEDICT and must carry CC BY-SA 4.0
attribution and source metadata separately from the `moine` code license. Raw
CC-CEDICT glosses are not included in the runtime payload.
