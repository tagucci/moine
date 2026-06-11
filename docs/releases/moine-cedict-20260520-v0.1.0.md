# moine-cedict-20260520-v0.1.0

Planned first public release of the `moine` CC-CEDICT no-tone pinyin artifact.

This release is retained as a compatibility artifact for older package builds.
Current downloader defaults use `moine-cedict-20260520-v0.1.1`.

Assets:

- `moine-cedict-20260520.tar.gz`
- `moine-cedict-20260520.tar.zst`

Source:

- Dictionary: CC-CEDICT
- Source version: 2026-05-20
- Pinyin view: `no-tone`
- Runtime payload: indexed FST/mmap-friendly normalized `surface -> pinyin readings`

The release is generated with:

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

The script builds the bundle, verifies metadata and payload checksums, creates
the gzip archive asset, and leaves external checksum manifests optional. The
second command reuses the verified bundle to create the zstd archive asset.

Optional maintainer verification:

```bash
cargo run -q -p moine-cli -- zh-artifact-verify \
  --metadata moine-cedict-20260520/metadata.yaml
```

Python usage after download/extraction:

```python
import moine

moine.distance("weishiji", "威士忌", lang="zh")
moine.distance("布納哈奔", "布納哈本", lang="zh")
```

License and attribution:

The generated artifact is derived from CC-CEDICT and must carry CC BY-SA 4.0
attribution and source metadata separately from the `moine` code license. Raw
CC-CEDICT glosses are not included in the runtime payload.
