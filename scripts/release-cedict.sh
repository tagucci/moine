#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
usage:
  scripts/release-cedict.sh [options]

options:
  --cedict PATH            CC-CEDICT dump path
  --source-version VALUE   CC-CEDICT source version
  --artifact-name VALUE    Release artifact name
  --dist-dir PATH          Output directory for release assets
  --license-file PATH      CC-CEDICT license/attribution file
  --pinyin-view VALUE      Pinyin view: no-tone or tone3
  --payload-format VALUE   Payload format: indexed or yaml
  --compression VALUE      Archive compression: xz, gzip, zstd, or none
  --checksum-manifest      Also write SHA256SUMS for the archive asset
  -h, --help               Show this help

environment overrides:
  CEDICT, SOURCE_VERSION, ARTIFACT_NAME, DIST_DIR, LICENSE_FILE
  PINYIN_VIEW, MAX_READINGS_PER_SURFACE, MAX_READINGS_PER_SEGMENT, MAX_PATHS
  PAYLOAD_FORMAT           Payload format: indexed or yaml
  RELEASE_COMPRESSION      Archive compression: xz, gzip, zstd, or none
  RELEASE_CHECKSUM_MANIFEST Set to 1 to write SHA256SUMS
  MOINE_BIN                Existing moine binary to use instead of building
USAGE
}

cedict="${CEDICT:-cedict_1_0_ts_utf-8_mdbg.txt}"
source_version="${SOURCE_VERSION:-2026-05-20}"
artifact_name="${ARTIFACT_NAME:-moine-cedict-20260520}"
dist_dir="${DIST_DIR:-dist}"
license_file="${LICENSE_FILE:-}"
pinyin_view="${PINYIN_VIEW:-no-tone}"
payload_format="${PAYLOAD_FORMAT:-indexed}"
max_readings_per_surface="${MAX_READINGS_PER_SURFACE:-16}"
max_readings_per_segment="${MAX_READINGS_PER_SEGMENT:-16}"
max_paths="${MAX_PATHS:-128}"
compression="${RELEASE_COMPRESSION:-gzip}"
checksum_manifest="${RELEASE_CHECKSUM_MANIFEST:-0}"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --cedict)
      cedict="${2:?missing value for --cedict}"
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
    --pinyin-view)
      pinyin_view="${2:?missing value for --pinyin-view}"
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

if [ ! -f "$cedict" ]; then
  echo "missing CC-CEDICT dump: $cedict" >&2
  echo "download the source dump first, or pass --cedict" >&2
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
  zh-artifact-bundle
  --cedict "$cedict"
  --source-version "$source_version"
  --artifact-name "$artifact_name"
  --payload-format "$payload_format"
  --pinyin-view "$pinyin_view"
  --max-readings-per-surface "$max_readings_per_surface"
  --max-readings-per-segment "$max_readings_per_segment"
  --max-paths "$max_paths"
  --longest-only
  --output-dir "$bundle_dir"
)
if [ -n "$license_file" ]; then
  bundle_args+=(--license-file "$license_file")
fi

"$moine_bin" "${bundle_args[@]}"

"$moine_bin" zh-artifact-verify \
  --metadata "$metadata"

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
  "$moine_bin" zh-artifact-archive \
    --metadata "$metadata" \
    --output "$tmp_tar" \
    --compression none
  xz -c "$tmp_tar" > "$archive"
  rm -f "$tmp_tar"
  trap - EXIT
else
  "$moine_bin" zh-artifact-archive \
    --metadata "$metadata" \
    --output "$archive" \
    --compression "$compression"
fi

if [ "$checksum_manifest" = "1" ] || [ "$checksum_manifest" = "true" ]; then
  "$moine_bin" zh-artifact-release-checksums \
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
