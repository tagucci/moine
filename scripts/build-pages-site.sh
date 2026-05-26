#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SITE_DIR="$ROOT/site"
DEMO_DIR="$SITE_DIR/demo"
ARTIFACTS_DIR="${MOINE_PAGES_ARTIFACTS_DIR:-$ROOT/dist}"
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

install_demo_dictionary() {
  local lang="$1"
  local artifact_name="$2"
  local release_tag="$3"
  local payload_name="$4"
  local local_dir="$ARTIFACTS_DIR/$artifact_name"
  local target_dir="$DEMO_DIR/dictionaries/$lang"

  if [[ -f "$local_dir/metadata.yaml" && -f "$local_dir/$payload_name" ]]; then
    cp "$local_dir/metadata.yaml" "$target_dir/"
    cp "$local_dir/$payload_name" "$target_dir/"
    return
  fi

  if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to download $artifact_name for the browser demo." >&2
    exit 1
  fi

  local tmp_dir archive
  tmp_dir="$(mktemp -d)"
  archive="$tmp_dir/$artifact_name.tar.gz"
  curl -fsSL \
    "https://github.com/tagucci/moine/releases/download/$release_tag/$artifact_name.tar.gz" \
    -o "$archive"
  tar -xzf "$archive" -C "$tmp_dir" \
    "$artifact_name/metadata.yaml" \
    "$artifact_name/$payload_name"
  cp "$tmp_dir/$artifact_name/metadata.yaml" "$target_dir/"
  cp "$tmp_dir/$artifact_name/$payload_name" "$target_dir/"
  rm -rf "$tmp_dir"
}

install_demo_dictionary \
  "ja" \
  "moine-unidic-cwj-202512" \
  "unidic-cwj-202512-v0.1.0" \
  "moine-unidic-cwj-202512.readings.moineidx"
install_demo_dictionary \
  "zh" \
  "moine-cedict-20260520" \
  "moine-cedict-20260520-v0.1.0" \
  "moine-cedict-20260520.readings.moineidx"

cat <<EOF
Built Pages site:
  $SITE_DIR/

Serve locally with:
  uv run python -m http.server 8765 --bind 127.0.0.1 --directory "$SITE_DIR"
EOF
