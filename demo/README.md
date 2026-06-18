# moine browser demo

This directory contains a small static browser demo for comparing surface edit
distance with Lattice Path Edit Distance.

For Japanese and Chinese comparisons, the demo renders the same lattice DOT data
used by the CLI lattice graph output. The Pages build self-hosts Graphviz WASM
from [`@hpcc-js/wasm`](https://hpcc-systems.github.io/hpcc-js-wasm/) and uses it
in the browser to render DOT as SVG.

For GitHub Pages, the intended layout is:

```text
https://tagucci.github.io/moine/
  documentation landing page

https://tagucci.github.io/moine/demo/
  browser demo
```

The checked-in files in this directory are demo source files. The Zensical
documentation build writes the main site under `site`, and the build script
copies this demo into `site/demo`.

## Build The Pages Site

Install `wasm-bindgen-cli` with the same version used by the Rust dependency:

```bash
cargo install -f wasm-bindgen-cli --version 0.2.122
```

Then build the local Pages layout:

```bash
scripts/build-pages-site.sh
```

The script builds the Zensical documentation site, builds `crates/moine-wasm`,
generates `site/demo/pkg`, installs Graphviz WASM under `site/demo/vendor`, copies
its package license alongside the generated asset, and copies local indexed
dictionary artifacts from `dist/moine-unidic-cwj-202512` and
`dist/moine-cedict-20260520` into `site/demo/dictionaries` when they are present
and match the pinned release payload digests. If local artifacts are missing or
stale, the script downloads the published release archives and copies the same
files from them.

## Serve Locally

Serve the generated Pages directory:

```bash
python3 -m http.server 8765 --directory site
```

Then open:

- <http://127.0.0.1:8765/>
- <http://127.0.0.1:8765/demo/>

`site/demo`, `demo/pkg`, and `demo/dictionaries` are intentionally ignored.
Dictionary artifacts are published separately from the source repository.
