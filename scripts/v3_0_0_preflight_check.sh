#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

expected_version="${PULSEDAG_REHEARSAL_VERSION:-v3.0.0}"
expected_cargo_version="${expected_version#v}"
expected_version_slug="${PULSEDAG_REHEARSAL_VERSION_SLUG:-v3_0_0}"
fail=0
checks=0
passes=0

check() {
  local message="$1"; shift
  checks=$((checks + 1))
  if "$@"; then
    passes=$((passes + 1))
    echo "PASS: $message"
  else
    echo "FAIL: $message" >&2
    fail=1
  fi
}

for dep in bash jq curl tar gzip; do
  check "dependency available: $dep" command -v "$dep"
done

version="$(tr -d '\r' < VERSION)"
cargo_version="$(awk '/^[[:space:]]*version[[:space:]]*=/{gsub(/"/, "", $3); print $3; exit}' Cargo.toml | tr -d '\r')"

echo "Expected v3 rehearsal version: $expected_version"
echo "Git commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)"

check "VERSION == $expected_version" test "$version" = "$expected_version"
check "Cargo workspace version == $expected_cargo_version" test "$cargo_version" = "$expected_cargo_version"
check "v3 version slug == v3_0_0" test "$expected_version_slug" = "v3_0_0"

required_files=(
  docs/VERSION_MATRIX.md
  docs/ROADMAP_V3_0_0.md
  docs/ROADMAP_V3_0_LONG_LIVED_CORE.md
  scripts/v3_0_0_preflight_check.sh
  scripts/v3_compact_relay_5n4m_rehearsal.sh
  scripts/v3_compact_relay_multinode_evidence.py
)
for file in "${required_files[@]}"; do
  check "exists: $file" test -f "$file"
done

check "README identifies v3.0.0" grep -Fq '# PulseDAG v3.0.0' README.md
check "version matrix identifies active v3 candidate" grep -Fq 'Active candidate construction' docs/VERSION_MATRIX.md
check "contracts remain disabled" grep -Fq '`contracts_enabled=false`' docs/VERSION_MATRIX.md

checks=$((checks + 1))
if grep -Eiq 'public testnet (is|now) live|mainnet (is|now) live|v3\.0(\.0)? (is )?ready|ready for v3\.0(\.0)?' README.md docs/VERSION_MATRIX.md; then
  echo "FAIL: unsupported v3 launch/readiness claim detected" >&2
  fail=1
else
  passes=$((passes + 1))
  echo "PASS: no unsupported v3 launch/readiness claim"
fi

checks=$((checks + 1))
if grep -Eiq 'current milestone.*v2\.|active candidate.*v2\.|Repository version: `v2\.' README.md docs/README.md docs/VERSION_MATRIX.md; then
  echo "FAIL: stale v2 active-release identity detected" >&2
  fail=1
else
  passes=$((passes + 1))
  echo "PASS: no stale v2 active-release identity"
fi

if [[ -n "${OUT_DIR:-}" ]]; then
  mkdir -p "$OUT_DIR"
  result="PASS"; [[ "$fail" -eq 0 ]] || result="FAIL"
  cat > "$OUT_DIR/preflight-summary.md" <<EOF
# v3.0.0 rehearsal preflight

- expected_version: $expected_version
- expected_version_slug: $expected_version_slug
- cargo: $cargo_version
- explicit_checks: $checks
- explicit_passes: $passes
- result: $result
EOF
fi

result="PASS"; [[ "$fail" -eq 0 ]] || result="FAIL"
echo "SUMMARY: $result ($passes/$checks explicit checks passed)"
exit "$fail"
