# Release Process

This checklist records the current pre-1.0 release boundary for `moine`.

## Package Metadata

- Rust crates inherit workspace `license`, `repository`, `homepage`,
  `keywords`, `categories`, and `rust-version`.
- Published Rust crates set their own `documentation` URL and `readme` so each
  crates.io page points at the matching docs.rs crate. Support crates that are
  only used for Python extension builds or the browser demo are marked
  `publish = false`.
- The Python package includes project URLs, classifiers, and the `py.typed`
  marker.
- Dictionary artifacts remain separate release assets and carry their own
  metadata, payload checksums, file digests, and license references.

## Verification

Run before publishing a release candidate:

```bash
cargo fmt --check
cargo +1.86.0 check --workspace --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test -p moine --no-default-features
cargo build -p moine-wasm --target wasm32-unknown-unknown
cargo doc --workspace --no-deps
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
cargo rustdoc -p moine --lib -- -W missing_docs
cargo rustdoc -p moine-core --lib -- -W missing_docs

uv run --with '.[test]' python -m pytest python/tests
uv run --with '.[dev]' ruff check python
uv run --with '.[dev]' ruff format --check python
uv run --with '.[dev]' ty check
```

Run the local artifact smoke script:

```bash
scripts/ci-smoke-artifact-archive.sh
```

## Publish Dry Runs

Before the first crates.io publication, `cargo publish --dry-run` can only fully
verify crates whose versioned dependencies already exist in the crates.io index.
For a first release, use `cargo package --workspace --no-verify --list` to check
packaged files, then dry-run and publish in dependency order. After each publish,
wait for the crates.io index to contain the newly published crate before
dry-running the dependent crates:

```bash
cargo publish --dry-run -p moine-core
cargo publish -p moine-core

cargo publish --dry-run -p moine-ja
cargo publish -p moine-ja

cargo publish --dry-run -p moine-zh
cargo publish -p moine-zh

cargo publish --dry-run -p moine-cli
cargo publish -p moine-cli

cargo publish --dry-run -p moine
cargo publish -p moine
```

Build and inspect the Python wheel:

```bash
uv run --with 'maturin>=1.9,<2' maturin build --release --out dist
uv run --with 'twine>=6,<7' twine check dist/*
```

## Artifact Releases

Use the language-specific release scripts for dictionary bundles:

```bash
scripts/release-unidic-cwj.sh --help
scripts/release-cedict.sh --help
```

Each artifact release should include:

- at least one compressed archive asset, with `*.tar.gz` as the default
  downloader target
- an unpacked bundle containing `metadata.yaml`, indexed payload, and license
  or attribution files
- a short release note under `docs/releases/`

The first public UniDic-CWJ and CC-CEDICT artifact releases include both
`*.tar.gz` and `*.tar.zst` archives. Downloaders use the gzip assets by default;
the zstd assets are provided for users who prefer faster local extraction.

`SHA256SUMS` is optional. Generate it only for releases that need an external
checksum manifest in addition to the bundle metadata and payload digests.

The release assets are a hard public-package gate. Before publishing PyPI or
crates.io packages, and before pushing the package `vX.Y.Z` tag, create the
GitHub Releases referenced by the baked-in download specs and upload assets with
these exact names:

- `unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.gz`
- `unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.zst`
- `moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.gz`
- `moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.zst`

The release workflow checks these assets before publishing the Python
distribution on tag pushes. Keep the workflow asset list, Rust downloader specs,
Python downloader specs, and this document in sync whenever artifact names or
release tags change.

Then smoke-test both downloaders from an empty cache:

```bash
tmp="$(mktemp -d)"
MOINE_CACHE_DIR="$tmp/python-cache" uv run python -m moine download ja
MOINE_CACHE_DIR="$tmp/python-cache" uv run python -m moine download zh
MOINE_CACHE_DIR="$tmp/rust-cache" moine download ja
MOINE_CACHE_DIR="$tmp/rust-cache" moine download zh
```
