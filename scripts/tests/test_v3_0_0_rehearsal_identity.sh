#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

bash -n   scripts/v3_0_0_preflight_check.sh   scripts/v3_compact_relay_5n4m_rehearsal.sh   scripts/v2_2_20_private_5n_4m_rehearsal.sh

test "$(tr -d '\r\n' < VERSION)" = "v3.0.0"
test "$(awk '/^version = / {gsub(/"/, "", $3); print $3; exit}' Cargo.toml)" = "3.0.0"

grep -Fq 'PULSEDAG_REHEARSAL_VERSION="v3.0.0"' scripts/v3_compact_relay_5n4m_rehearsal.sh
grep -Fq 'PULSEDAG_REHEARSAL_VERSION_SLUG="v3_0_0"' scripts/v3_compact_relay_5n4m_rehearsal.sh
grep -Fq 'PULSEDAG_REHEARSAL_PREFLIGHT_SCRIPT="$ROOT_DIR/scripts/v3_0_0_preflight_check.sh"'   scripts/v3_compact_relay_5n4m_rehearsal.sh
grep -Fq 'PREFLIGHT_SCRIPT="${PULSEDAG_REHEARSAL_PREFLIGHT_SCRIPT:-$ROOT_DIR/scripts/v2_2_20_preflight_check.sh}"'   scripts/v2_2_20_private_5n_4m_rehearsal.sh

if grep -Eq 'PULSEDAG_REHEARSAL_VERSION="v2\.|PULSEDAG_REHEARSAL_VERSION_SLUG="v2_'   scripts/v3_compact_relay_5n4m_rehearsal.sh; then
  echo "v3 rehearsal wrapper still declares a v2 release identity" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
OUT_DIR="$tmp_dir/preflight" bash scripts/v3_0_0_preflight_check.sh
test -s "$tmp_dir/preflight/preflight-summary.md"
grep -Fq -- '- expected_version: v3.0.0' "$tmp_dir/preflight/preflight-summary.md"
grep -Fq -- '- result: PASS' "$tmp_dir/preflight/preflight-summary.md"

echo "PASS: v3.0.0 rehearsal identity regression"
