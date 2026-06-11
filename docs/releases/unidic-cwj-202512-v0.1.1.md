# UniDic-CWJ 2025.12 Reading Index v0.1.1

Release of the `moine` UniDic-CWJ reading-index artifact for moine v0.1.1.

This release rebuilds the artifact with the `pron` reading field. It keeps the
same source dictionary version, artifact name, indexed payload format, and
bounded expansion settings as the v0.1.0 artifact release.

## Assets

- `moine-unidic-cwj-202512.tar.gz`
- `moine-unidic-cwj-202512.tar.zst`

## Source

- Dictionary: UniDic-CWJ
- Source version: `2025.12`
- Reading field: `pron`
- Runtime payload: indexed FST/mmap-friendly normalized `surface -> readings`
- Artifact name: `moine-unidic-cwj-202512`

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
d37570e7b5a6cfa871f83f202765682ea492efd49f8ff21e98aa0794feb8ada5  moine-unidic-cwj-202512.tar.gz
dbbe7bdae52d6b9e7a68f321976ce19a935fcffdbbdb4182287abd475d53fdd  moine-unidic-cwj-202512.tar.zst
```

Payload checksums inside `metadata.yaml`:

```text
sha256-file-v1:      62b6fb5bc24e1a46be65e86e30c5a3ec23a4016f49e50bc3251698aa10525dec
sha256-canonical-v1: 21788ed133d29acffab7047264575ebd06a4549b712105e4b64e2600a29abcab
```

## Verification

```bash
tar -xzf moine-unidic-cwj-202512.tar.gz
cargo run -q -p moine-cli -- unidic-artifact-verify \
  --metadata moine-unidic-cwj-202512/metadata.yaml
```

## License

The generated artifact is derived from UniDic-CWJ and must carry the UniDic
license references separately from the `moine` code license. The archive
includes UniDic license references under `license/BSD` and `license/COPYING`.
