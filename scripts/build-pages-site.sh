#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SITE_DIR="$ROOT/site"
DEMO_DIR="$SITE_DIR/demo"
WASM_PATH="$ROOT/target/wasm32-unknown-unknown/release/moine_wasm.wasm"

if command -v uv >/dev/null 2>&1; then
  UV_BIN="uv"
elif [[ -x "$HOME/.local/bin/uv" ]]; then
  UV_BIN="$HOME/.local/bin/uv"
else
  echo "uv is required to build the documentation site." >&2
  exit 1
fi

"$UV_BIN" run --no-project --with 'zensical>=0.0.32' \
  zensical build --clean --config-file "$ROOT/zensical.toml"

mkdir -p "$DEMO_DIR" "$DEMO_DIR/pkg" "$DEMO_DIR/dictionaries/ja" "$DEMO_DIR/dictionaries/zh"

cp "$ROOT/demo/index.html" "$DEMO_DIR/index.html"
cp "$ROOT/demo/main.js" "$DEMO_DIR/main.js"
cp "$ROOT/demo/style.css" "$DEMO_DIR/style.css"

cargo build --release -p moine-wasm --target wasm32-unknown-unknown
wasm-bindgen "$WASM_PATH" --target web --out-dir "$DEMO_DIR/pkg"

if [[ -f "$ROOT/dist/moine-unidic-cwj-202512/metadata.yaml" ]]; then
  cp "$ROOT/dist/moine-unidic-cwj-202512/metadata.yaml" "$DEMO_DIR/dictionaries/ja/"
fi
if [[ -f "$ROOT/dist/moine-unidic-cwj-202512/moine-unidic-cwj-202512.readings.moineidx" ]]; then
  cp "$ROOT/dist/moine-unidic-cwj-202512/moine-unidic-cwj-202512.readings.moineidx" \
    "$DEMO_DIR/dictionaries/ja/"
fi
if [[ -f "$ROOT/dist/moine-cedict-20260520/metadata.yaml" ]]; then
  cp "$ROOT/dist/moine-cedict-20260520/metadata.yaml" "$DEMO_DIR/dictionaries/zh/"
fi
if [[ -f "$ROOT/dist/moine-cedict-20260520/moine-cedict-20260520.readings.moineidx" ]]; then
  cp "$ROOT/dist/moine-cedict-20260520/moine-cedict-20260520.readings.moineidx" \
    "$DEMO_DIR/dictionaries/zh/"
fi

cat <<EOF
Built Pages site:
  $SITE_DIR/

Serve locally with:
  uv run python -m http.server 8765 --bind 127.0.0.1 --directory "$SITE_DIR"
EOF
