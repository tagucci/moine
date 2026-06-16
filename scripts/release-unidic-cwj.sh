#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/release-unidic-cwj.sh [options]

options:
  --lex-csv PATH           UniDic full lex.csv path
  --source-version VALUE   UniDic source version
  --artifact-name VALUE    Release artifact name
  --dist-dir PATH          Output directory for release assets
  --license-dir PATH       Directory containing BSD and COPYING
  --payload-format VALUE   Payload format: indexed, binary, or yaml
  --compression VALUE      Archive compression: xz, gzip, zstd, or none
  --checksum-manifest      Also write SHA256SUMS for the archive asset
  -h, --help               Show this help

environment overrides:
  LEX_CSV, SOURCE_VERSION, ARTIFACT_NAME, DIST_DIR, LICENSE_DIR
  MAX_READINGS_PER_SURFACE, MAX_READINGS_PER_SEGMENT, MAX_PATHS
  RELEASE_PAYLOAD_FORMAT   Payload format: indexed, binary, or yaml
  RELEASE_COMPRESSION      Archive compression: xz, gzip, zstd, or none
  RELEASE_CHECKSUM_MANIFEST Set to 0 to skip SHA256SUMS
  MOINE_BIN                Existing moine binary to use instead of building
USAGE
}

lex_csv="${LEX_CSV:-unidic-cwj-202512_full/lex.csv}"
source_version="${SOURCE_VERSION:-2025.12}"
artifact_name="${ARTIFACT_NAME:-moine-unidic-cwj-202512}"
dist_dir="${DIST_DIR:-dist}"
license_dir="${LICENSE_DIR:-}"
max_readings_per_surface="${MAX_READINGS_PER_SURFACE:-16}"
max_readings_per_segment="${MAX_READINGS_PER_SEGMENT:-16}"
max_paths="${MAX_PATHS:-128}"
compression="${RELEASE_COMPRESSION:-gzip}"
payload_format="${RELEASE_PAYLOAD_FORMAT:-indexed}"
checksum_manifest="${RELEASE_CHECKSUM_MANIFEST:-1}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --lex-csv)
      lex_csv="${2:?missing value for --lex-csv}"
      shift 2
      ;;
    --source-version)
      source_version="${2:?missing value for --source-version}"
      shift 2
      ;;
    --artifact-name)
      artifact_name="${2:?missing value for --artifact-name}"
      shift 2
      ;;
    --dist-dir)
      dist_dir="${2:?missing value for --dist-dir}"
      shift 2
      ;;
    --license-dir)
      license_dir="${2:?missing value for --license-dir}"
      shift 2
      ;;
    --payload-format)
      payload_format="${2:?missing value for --payload-format}"
      shift 2
      ;;
    --compression)
      compression="${2:?missing value for --compression}"
      shift 2
      ;;
    --checksum-manifest)
      checksum_manifest=1
      shift 1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [ ! -f "$lex_csv" ]; then
  echo "missing UniDic lex.csv: $lex_csv" >&2
  echo "download/extract the full UniDic package first, or pass --lex-csv" >&2
  exit 1
fi

if [ -n "${MOINE_BIN:-}" ]; then
  moine_bin="$MOINE_BIN"
else
  cargo build -q -p moine-cli --release
  moine_bin="target/release/moine"
fi

case "$compression" in
  xz)
    compression="xz"
    archive_suffix="tar.xz"
    ;;
  gzip|gz)
    compression="gzip"
    archive_suffix="tar.gz"
    ;;
  zstd|zst)
    compression="zstd"
    archive_suffix="tar.zst"
    ;;
  none|tar)
    compression="none"
    archive_suffix="tar"
    ;;
  *)
    echo "unsupported compression: $compression" >&2
    echo "expected xz, gzip, zstd, or none" >&2
    exit 2
    ;;
esac

bundle_dir="$dist_dir/$artifact_name"
metadata="$bundle_dir/metadata.yaml"
archive="$dist_dir/$artifact_name.$archive_suffix"
checksums="$dist_dir/SHA256SUMS"

mkdir -p "$dist_dir"

bundle_args=(
  unidic-artifact-bundle
  --lex-csv "$lex_csv" \
  --source-version "$source_version" \
    --artifact-name "$artifact_name" \
  --payload-format "$payload_format" \
  --max-readings-per-surface "$max_readings_per_surface" \
  --max-readings-per-segment "$max_readings_per_segment" \
  --max-paths "$max_paths" \
  --longest-only \
  --output-dir "$bundle_dir"
)
if [ -n "$license_dir" ]; then
  bundle_args+=(--license-dir "$license_dir")
fi

"$moine_bin" "${bundle_args[@]}"

"$moine_bin" unidic-artifact-verify \
  --metadata "$metadata"

"$moine_bin" unidic-artifact-verify \
  --metadata "$metadata" \
  --canonical-checksum

if [ "$compression" = "xz" ]; then
  command -v xz >/dev/null 2>&1 || {
    echo "xz is required for --compression xz" >&2
    exit 1
  }
  tmp_tar="$(mktemp "$dist_dir/$artifact_name.XXXXXX.tar")"
  cleanup_tmp_tar() {
    rm -f "$tmp_tar"
  }
  trap cleanup_tmp_tar EXIT
  "$moine_bin" unidic-artifact-archive \
    --metadata "$metadata" \
    --output "$tmp_tar" \
    --compression none
  xz -c "$tmp_tar" > "$archive"
  rm -f "$tmp_tar"
  trap - EXIT
else
  "$moine_bin" unidic-artifact-archive \
    --metadata "$metadata" \
    --output "$archive" \
    --compression "$compression"
fi

if [ "$checksum_manifest" = "1" ] || [ "$checksum_manifest" = "true" ]; then
  "$moine_bin" unidic-artifact-release-checksums \
    --asset "$archive" \
    --output "$checksums"
fi

cat <<EOF
release bundle: $bundle_dir
release asset:  $archive
EOF

if [ "$checksum_manifest" = "1" ] || [ "$checksum_manifest" = "true" ]; then
  echo "checksums:      $checksums"
fi
