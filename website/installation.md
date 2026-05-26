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
uv run python -m moine download ja
uv run python -m moine download zh

moine download ja
moine download zh
```

Installed artifacts are stored in the local mòine cache:

```bash
uv run python -m moine list
uv run python -m moine where

moine list
moine where
```

The downloader verifies an archive checksum when one is configured, and always
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
