#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/unidic/license" "$tmp/dist"
cp crates/moine-cli/tests/resources/unidic/whisky-lex.csv "$tmp/unidic/lex.csv"
cp crates/moine-cli/tests/resources/unidic/license/BSD "$tmp/unidic/license/BSD"
cp crates/moine-cli/tests/resources/unidic/license/COPYING "$tmp/unidic/license/COPYING"

cargo build -q -p moine-cli
moine_bin="${MOINE_BIN:-target/debug/moine}"

"$moine_bin" unidic-artifact-bundle \
  --lex-csv "$tmp/unidic/lex.csv" \
  --source-version test \
  --artifact-name moine-unidic-test \
  --payload-format binary \
  --output-dir "$tmp/dist/moine-unidic-test"
"$moine_bin" unidic-artifact-bundle \
  --lex-csv "$tmp/unidic/lex.csv" \
  --source-version test \
  --artifact-name moine-unidic-indexed-test \
  --payload-format indexed \
  --output-dir "$tmp/dist/moine-unidic-indexed-test"
"$moine_bin" unidic-artifact-verify \
  --metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  > "$tmp/verify-fast.txt"
"$moine_bin" unidic-artifact-verify \
  --metadata "$tmp/dist/moine-unidic-indexed-test/metadata.yaml" \
  > "$tmp/verify-indexed.txt"
"$moine_bin" unidic-artifact-verify \
  --metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  --canonical-checksum \
  > "$tmp/verify-canonical.txt"
grep -q 'entry_count_source: decoded_payload' "$tmp/verify-fast.txt"
grep -q 'payload_format: indexed-fst.surface-readings.v1' "$tmp/verify-indexed.txt"
grep -q 'checksum_algorithm: sha256-canonical-v1' "$tmp/verify-fast.txt"
grep -q 'entry_count_source: decoded_payload' "$tmp/verify-canonical.txt"
grep -q 'checksum_algorithm: sha256-canonical-v1' "$tmp/verify-canonical.txt"

"$moine_bin" unidic-artifact-runtime-measure \
  --metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  --pair ういすきー ウイスキー \
  --warmups 1 \
  --iterations 2 \
  > "$tmp/runtime-measure.txt"
grep -q 'file_digest_verified: true' "$tmp/runtime-measure.txt"
grep -q 'measured_comparisons: 2' "$tmp/runtime-measure.txt"

"$moine_bin" compare \
  --left ういすきー \
  --right ウイスキー \
  --artifact-metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  > "$tmp/compare-metadata.txt"
grep -q 'unidic_source:      artifact_metadata' "$tmp/compare-metadata.txt"
grep -q 'file_digest:        verified=true' "$tmp/compare-metadata.txt"
grep -q 'ja_dict_lattice: 0' "$tmp/compare-metadata.txt"

"$moine_bin" unidic-artifact-archive \
  --metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  --output "$tmp/dist/moine-unidic-test.tar"
"$moine_bin" unidic-artifact-archive \
  --metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  --output "$tmp/dist/moine-unidic-test.tar.gz" \
  --compression gzip
"$moine_bin" unidic-artifact-archive \
  --metadata "$tmp/dist/moine-unidic-test/metadata.yaml" \
  --output "$tmp/dist/moine-unidic-test.tar.zst" \
  --compression zstd
"$moine_bin" unidic-artifact-release-checksums \
  --asset "$tmp/dist/moine-unidic-test.tar" \
  --asset "$tmp/dist/moine-unidic-test.tar.gz" \
  --asset "$tmp/dist/moine-unidic-test.tar.zst" \
  --output "$tmp/dist/SHA256SUMS"
grep -q '  moine-unidic-test.tar$' "$tmp/dist/SHA256SUMS"
grep -q '  moine-unidic-test.tar.gz$' "$tmp/dist/SHA256SUMS"
grep -q '  moine-unidic-test.tar.zst$' "$tmp/dist/SHA256SUMS"

MOINE_BIN="$moine_bin" scripts/release-unidic-cwj.sh \
  --lex-csv "$tmp/unidic/lex.csv" \
  --source-version test \
  --artifact-name moine-unidic-recipe-test \
  --dist-dir "$tmp/recipe-dist" \
  > "$tmp/release-recipe.txt"
grep -q 'release asset:' "$tmp/release-recipe.txt"
test -f "$tmp/recipe-dist/moine-unidic-recipe-test.tar.gz"
test ! -f "$tmp/recipe-dist/SHA256SUMS"

tar -tf "$tmp/dist/moine-unidic-test.tar" | sort > "$tmp/archive.txt"
tar -tf "$tmp/dist/moine-unidic-test.tar.gz" | sort > "$tmp/archive-gzip.txt"
tar -tf "$tmp/recipe-dist/moine-unidic-recipe-test.tar.gz" | sort > "$tmp/archive-recipe-gzip.txt"
cat > "$tmp/expected.txt" <<'TXT'
moine-unidic-test/license/BSD
moine-unidic-test/license/COPYING
moine-unidic-test/metadata.yaml
moine-unidic-test/moine-unidic-test.readings.moinebin
TXT
cat > "$tmp/expected-recipe.txt" <<'TXT'
moine-unidic-recipe-test/license/BSD
moine-unidic-recipe-test/license/COPYING
moine-unidic-recipe-test/metadata.yaml
moine-unidic-recipe-test/moine-unidic-recipe-test.readings.moineidx
TXT
diff -u "$tmp/expected.txt" "$tmp/archive.txt"
diff -u "$tmp/expected.txt" "$tmp/archive-gzip.txt"
diff -u "$tmp/expected-recipe.txt" "$tmp/archive-recipe-gzip.txt"
