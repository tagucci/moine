# Installation

Install the Python package:

```bash
uv pip install moine
```

Install the Rust command-line tool:

```bash
cargo install moine
```

The packages do not bundle dictionary data. Download the language artifacts you
need explicitly:

```bash
# Default Japanese artifact: UniDic-CWJ
uv run python -m moine download ja

# Explicit Japanese sources
uv run python -m moine download ja-unidic
uv run python -m moine download ja-sudachi

# Chinese artifact: CC-CEDICT
uv run python -m moine download zh

# Same selectors are available from the Rust CLI.
moine download ja
moine download ja-unidic
moine download ja-sudachi
moine download zh
```

`ja` installs the default Japanese artifact, currently UniDic-CWJ. Use
`ja-unidic` or `ja-sudachi` when you want the dictionary source to be explicit.

Installed artifacts are stored in the local mòine cache:

```bash
uv run python -m moine list
uv run python -m moine where

moine list
moine where
```

The public artifact downloader verifies the release archive checksum, then
verifies bundle metadata and payload digests before installing an artifact.

## Existing Bundles

You can point mòine at an existing extracted bundle instead of using the cache:

```bash
export MOINE_JA_DICTIONARY=/path/to/moine-unidic-cwj-202512
export MOINE_ZH_DICTIONARY=/path/to/moine-cedict-20260520
```

For a shared directory that contains one or more bundle directories, use:

```bash
export MOINE_DICTIONARIES_PATH=/path/to/dictionaries
```

Use `MOINE_CACHE_DIR` when you want `download`, `list`, and `where` to use a
non-default cache location.
