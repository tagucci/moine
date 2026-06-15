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

WHEEL_DIR=$(mktemp -d)
uv run --no-project --with 'maturin>=1.9,<2' maturin build --out "$WHEEL_DIR"
WHEEL=$(ls "$WHEEL_DIR"/moine-*.whl)
uv run --no-project --with 'pytest>=8,<9' --with "$WHEEL" python -m pytest python/tests
uv run --no-project --with 'ruff>=0.14,<0.15' ruff check python
uv run --no-project --with 'ruff>=0.14,<0.15' ruff format --check python
uv run --no-project --with 'ty>=0.0.38,<0.1' ty check
```

The Python checks intentionally use `--no-project`. That keeps `uv` from
installing `moine` itself into the temporary tool environment. The pytest step
first builds a fresh local wheel and installs that wheel explicitly so cached
`moine` wheels with the same version cannot shadow the checkout.

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
scripts/release-sudachi-full.sh --help
scripts/release-cedict.sh --help
```

Each artifact release should include:

- at least one compressed archive asset, with `*.tar.gz` as the default
  downloader target
- an unpacked bundle containing `metadata.yaml`, indexed payload, and license
  or attribution files
- a short release note under `docs/releases/`

The public UniDic-CWJ, SudachiDict-full, and CC-CEDICT artifact releases include
both `*.tar.gz` and `*.tar.zst` archives. Downloaders use the gzip assets by
default; the zstd assets are provided for users who prefer faster local
extraction.

`SHA256SUMS` is optional. Generate it only for releases that need an external
checksum manifest in addition to the bundle metadata and payload digests.

The release assets are a hard public-package gate. Before publishing PyPI or
crates.io packages, and before pushing the package `vX.Y.Z` tag, create the
GitHub Releases referenced by the baked-in download specs and upload assets with
these exact names:

- `unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.gz`
- `unidic-cwj-202512-v0.1.1/moine-unidic-cwj-202512.tar.zst`
- `moine-sudachi-full-20260428-v0.2.0/moine-sudachi-full-20260428.tar.gz`
- `moine-sudachi-full-20260428-v0.2.0/moine-sudachi-full-20260428.tar.zst`
- `moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.gz`
- `moine-cedict-20260520-v0.1.1/moine-cedict-20260520.tar.zst`

These dictionary release tags are artifact-specific and do not need to match
the package version when the generated payloads are unchanged. For the package
`v0.2.0` release, the current public downloader targets are UniDic-CWJ
`v0.1.1`, SudachiDict-full `v0.2.0`, and CC-CEDICT `v0.1.1`.

The release workflow checks these assets before publishing the Python
distribution on tag pushes. Keep the workflow asset list, Rust downloader specs,
Python downloader specs, and this document in sync whenever artifact names or
release tags change.

Then smoke-test both downloaders from an empty cache:

```bash
tmp="$(mktemp -d)"
MOINE_CACHE_DIR="$tmp/python-cache" uv run python -m moine download ja
MOINE_CACHE_DIR="$tmp/python-cache" uv run python -m moine download ja-unidic
MOINE_CACHE_DIR="$tmp/python-cache" uv run python -m moine download ja-sudachi
MOINE_CACHE_DIR="$tmp/python-cache" uv run python -m moine download zh
MOINE_CACHE_DIR="$tmp/rust-cache" moine download ja
MOINE_CACHE_DIR="$tmp/rust-cache" moine download ja-unidic
MOINE_CACHE_DIR="$tmp/rust-cache" moine download ja-sudachi
MOINE_CACHE_DIR="$tmp/rust-cache" moine download zh
```
