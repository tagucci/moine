#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/release-sudachi-full.sh [options]

options:
  --lex-csv PATH             Concatenated Sudachi raw full lex.csv path
  --source-version VALUE     SudachiDict source version
  --artifact-name VALUE      Release artifact name
  --dist-dir PATH            Output directory for release assets
  --license-file PATH        SudachiDict LICENSE-2.0.txt path
  --legal-file PATH          SudachiDict LEGAL notice path
  --payload-format VALUE     Payload format: indexed, binary, or yaml
  --compression VALUE        Archive compression: xz, gzip, zstd, or none
  --max-span-chars VALUE     Default maximum dictionary span length
  --include-unsupported-readings
                            Keep readings the romaji converter cannot use
  --checksum-manifest        Also write SHA256SUMS for the archive asset
  -h, --help                 Show this help

environment overrides:
  LEX_CSV, SOURCE_VERSION, ARTIFACT_NAME, DIST_DIR, LICENSE_FILE, LEGAL_FILE
  MAX_READINGS_PER_SURFACE, MAX_READINGS_PER_SEGMENT, MAX_SPAN_CHARS, MAX_PATHS
  RELEASE_PAYLOAD_FORMAT     Payload format: indexed, binary, or yaml
  RELEASE_COMPRESSION        Archive compression: xz, gzip, zstd, or none
  RELEASE_CHECKSUM_MANIFEST  Set to 0 to skip SHA256SUMS
  INCLUDE_UNSUPPORTED_READINGS Set to 1 to keep unsupported readings
  MOINE_BIN                  Existing moine binary to use instead of building
USAGE
}

lex_csv="${LEX_CSV:-/tmp/sudachi-raw-20260428/full_lex.csv}"
source_version="${SOURCE_VERSION:-20260428}"
artifact_name="${ARTIFACT_NAME:-moine-sudachi-full-20260428}"
dist_dir="${DIST_DIR:-dist}"
license_file="${LICENSE_FILE:-}"
legal_file="${LEGAL_FILE:-}"
max_readings_per_surface="${MAX_READINGS_PER_SURFACE:-16}"
max_readings_per_segment="${MAX_READINGS_PER_SEGMENT:-16}"
max_span_chars="${MAX_SPAN_CHARS:-24}"
max_paths="${MAX_PATHS:-128}"
compression="${RELEASE_COMPRESSION:-gzip}"
payload_format="${RELEASE_PAYLOAD_FORMAT:-indexed}"
checksum_manifest="${RELEASE_CHECKSUM_MANIFEST:-1}"
include_unsupported_readings="${INCLUDE_UNSUPPORTED_READINGS:-0}"

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
    --license-file)
      license_file="${2:?missing value for --license-file}"
      shift 2
      ;;
    --legal-file)
      legal_file="${2:?missing value for --legal-file}"
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
    --max-span-chars)
      max_span_chars="${2:?missing value for --max-span-chars}"
      shift 2
      ;;
    --include-unsupported-readings)
      include_unsupported_readings=1
      shift 1
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
  echo "missing Sudachi raw full lex.csv: $lex_csv" >&2
  echo "download raw small/core/notcore CSVs, concatenate them, or pass --lex-csv" >&2
  exit 1
fi
if [ -z "$license_file" ] || [ ! -f "$license_file" ]; then
  echo "missing SudachiDict LICENSE-2.0.txt: ${license_file:-<unset>}" >&2
  echo "pass --license-file or set LICENSE_FILE" >&2
  exit 1
fi
if [ -z "$legal_file" ] || [ ! -f "$legal_file" ]; then
  echo "missing SudachiDict LEGAL notice: ${legal_file:-<unset>}" >&2
  echo "pass --legal-file or set LEGAL_FILE" >&2
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
  sudachi-artifact-bundle
  --lex-csv "$lex_csv"
  --source-version "$source_version"
  --artifact-name "$artifact_name"
  --payload-format "$payload_format"
  --max-readings-per-surface "$max_readings_per_surface"
  --max-readings-per-segment "$max_readings_per_segment"
  --max-span-chars "$max_span_chars"
  --max-paths "$max_paths"
  --longest-only
  --license-file "$license_file"
  --legal-file "$legal_file"
  --output-dir "$bundle_dir"
)
if [ "$include_unsupported_readings" != "1" ] && [ "$include_unsupported_readings" != "true" ]; then
  bundle_args+=(--exclude-unsupported-readings)
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
