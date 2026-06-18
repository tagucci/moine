#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SITE_DIR="$ROOT/site"
DEMO_DIR="$SITE_DIR/demo"
ARTIFACTS_DIR="${MOINE_PAGES_ARTIFACTS_DIR:-$ROOT/dist}"
WASM_PATH="$ROOT/target/wasm32-unknown-unknown/release/moine_wasm.wasm"
GRAPHVIZ_WASM_VERSION="2.34.2"
GRAPHVIZ_WASM_TARBALL="$ROOT/target/demo-vendor/hpcc-js-wasm-$GRAPHVIZ_WASM_VERSION.tgz"
GRAPHVIZ_WASM_TARBALL_SHA256="1380f114183c402b56f52ffff5c032c299f03a6474bc6dc2997af9650718b61d"
GRAPHVIZ_WASM_URL="https://registry.npmjs.org/@hpcc-js/wasm/-/wasm-$GRAPHVIZ_WASM_VERSION.tgz"

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

mkdir -p "$DEMO_DIR" "$DEMO_DIR/pkg" "$DEMO_DIR/vendor" "$DEMO_DIR/dictionaries/ja" "$DEMO_DIR/dictionaries/zh"

cp "$ROOT/demo/index.html" "$DEMO_DIR/index.html"
cp "$ROOT/demo/main.js" "$DEMO_DIR/main.js"
cp "$ROOT/demo/style.css" "$DEMO_DIR/style.css"

cargo build --release -p moine-wasm --target wasm32-unknown-unknown
wasm-bindgen "$WASM_PATH" --target web --out-dir "$DEMO_DIR/pkg"

verify_sha256() {
  local expected="$1"
  local path="$2"
  echo "$expected  $path" | shasum -a 256 -c - >/dev/null 2>&1
}

install_graphviz_wasm() {
  mkdir -p "$(dirname "$GRAPHVIZ_WASM_TARBALL")"
  if [[ ! -f "$GRAPHVIZ_WASM_TARBALL" ]] || \
    ! verify_sha256 "$GRAPHVIZ_WASM_TARBALL_SHA256" "$GRAPHVIZ_WASM_TARBALL"; then
    if ! command -v curl >/dev/null 2>&1; then
      echo "curl is required to download Graphviz WASM for the browser demo." >&2
      exit 1
    fi
    curl -fsSL "$GRAPHVIZ_WASM_URL" -o "$GRAPHVIZ_WASM_TARBALL"
  fi
  verify_sha256 "$GRAPHVIZ_WASM_TARBALL_SHA256" "$GRAPHVIZ_WASM_TARBALL"

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  tar -xzf "$GRAPHVIZ_WASM_TARBALL" -C "$tmp_dir" \
    package/dist/graphviz.js \
    package/LICENSE
  cp "$tmp_dir/package/dist/graphviz.js" "$DEMO_DIR/vendor/graphviz.js"
  cp "$tmp_dir/package/LICENSE" "$DEMO_DIR/vendor/hpcc-js-wasm-LICENSE"
  rm -rf "$tmp_dir"
}

install_demo_dictionary() {
  local lang="$1"
  local artifact_name="$2"
  local release_tag="$3"
  local payload_name="$4"
  local expected_payload_digest="$5"
  local local_dir="$ARTIFACTS_DIR/$artifact_name"
  local target_dir="$DEMO_DIR/dictionaries/$lang"

  if [[ -f "$local_dir/metadata.yaml" && -f "$local_dir/$payload_name" ]]; then
    if grep -Fq "file_digest: $expected_payload_digest" "$local_dir/metadata.yaml"; then
      cp "$local_dir/metadata.yaml" "$target_dir/"
      cp "$local_dir/$payload_name" "$target_dir/"
      return
    fi
    echo "Skipping local $local_dir because it does not match $release_tag." >&2
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
  curl -fsSL \
    "https://github.com/tagucci/moine/releases/download/$release_tag/SHA256SUMS" \
    -o "$tmp_dir/SHA256SUMS"
  awk -v asset="$artifact_name.tar.gz" '
    $2 == asset || $2 ~ "/" asset "$" {
      print $1 "  " asset
      found = 1
    }
    END { exit found ? 0 : 1 }
  ' "$tmp_dir/SHA256SUMS" | (cd "$tmp_dir" && shasum -a 256 -c -)
  tar -xzf "$archive" -C "$tmp_dir" \
    "$artifact_name/metadata.yaml" \
    "$artifact_name/$payload_name"
  cp "$tmp_dir/$artifact_name/metadata.yaml" "$target_dir/"
  cp "$tmp_dir/$artifact_name/$payload_name" "$target_dir/"
  rm -rf "$tmp_dir"
}

install_graphviz_wasm

install_demo_dictionary \
  "ja" \
  "moine-unidic-cwj-202512" \
  "unidic-cwj-202512-v0.1.1" \
  "moine-unidic-cwj-202512.readings.moineidx" \
  "62b6fb5bc24e1a46be65e86e30c5a3ec23a4016f49e50bc3251698aa10525dec"
install_demo_dictionary \
  "zh" \
  "moine-cedict-20260520" \
  "moine-cedict-20260520-v0.1.1" \
  "moine-cedict-20260520.readings.moineidx" \
  "f6087a4f47af64f39f668c76a36c668ca4a47b5b9ddc4b00d65e12bc121e76e9"

cat <<EOF
Built Pages site:
  $SITE_DIR/

Serve locally with:
  uv run python -m http.server 8765 --bind 127.0.0.1 --directory "$SITE_DIR"
EOF
