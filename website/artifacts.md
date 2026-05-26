# Dictionary Artifacts

Dictionary data is distributed separately from the code package.

Japanese
: [UniDic-CWJ](https://clrd.ninjal.ac.jp/unidic/download.html)-derived indexed
  reading artifact.

Chinese
: [CC-CEDICT](https://cc-cedict.org/wiki/)-derived no-tone indexed pinyin
  artifact.

Artifact bundles include `metadata.yaml`, an indexed payload, checksum metadata,
and dictionary license/attribution files.

## Downloaded Artifacts

Most users should let the CLI install artifacts into the local cache:

```bash
uv run python -m moine download ja
uv run python -m moine download zh

moine download ja
moine download zh
```

Use `list` and `where` to inspect installed bundles:

```bash
uv run python -m moine list
uv run python -m moine where ja
uv run python -m moine where zh
```

The default public assets are compressed tar archives. The downloader safely
extracts the archive and verifies the unpacked bundle metadata and payload
digest before moving it into the cache.

## Manual Artifacts

You can also download and extract a release asset yourself, then load the bundle
by path:

```python
from moine.ja import Dictionary

dictionary = Dictionary.load_bundle("/path/to/moine-unidic-cwj-202512")
```

```python
from moine.zh import Dictionary

dictionary = Dictionary.load_bundle("/path/to/moine-cedict-20260520")
```

## Runtime Lookup

mòine searches for default dictionaries in this order:

1. Language-specific environment variables:
   `MOINE_JA_DICTIONARY` or `MOINE_ZH_DICTIONARY`.
2. Directories listed in `MOINE_DICTIONARIES_PATH`.
3. The local mòine cache used by `uv run python -m moine download` and
   `moine download`.

## License Boundary

Dictionary artifacts carry their own license and attribution metadata. Keep
dictionary licenses separate from the mòine source-code license when
redistributing artifacts.

The source package license for mòine is MIT OR Apache-2.0. That license does not
cover UniDic-derived or CC-CEDICT-derived dictionary data.

## Current Scope

- Japanese uses one UniDic-CWJ artifact.
- Chinese uses one CC-CEDICT no-tone artifact.
- Sudachi artifacts and additional benchmark datasets are intentionally outside
  the first OSS release scope.

## Maintainer Details

Artifact schemas, build recipes, release checks, and license-boundary notes live
in the repository maintainer docs:

- [dictionary_artifacts.md](https://github.com/tagucci/moine/blob/main/docs/dictionary_artifacts.md)
- [release_process.md](https://github.com/tagucci/moine/blob/main/docs/release_process.md)
