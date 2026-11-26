#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

expected_version="${PULSEDAG_REHEARSAL_VERSION:-v3.0.0}"
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

# Classify missing host tools separately from candidate failures.
env_fail=0
dependencies=(bash jq curl tar gzip tr awk grep)
if [[ "${1:-}" == "--docker-mode" || "${REHEARSAL_DOCKER_MODE:-0}" == "1" ]]; then
  dependencies+=(docker)
fi
for dep in "${dependencies[@]}"; do
  check "dependency available: $dep" command -v "$dep"
  if ! command -v "$dep" >/dev/null 2>&1; then
    echo "ENV_FAIL: missing dependency: $dep" >&2
    env_fail=1
  fi
done
if (( env_fail != 0 )); then
  if [[ -n "${OUT_DIR:-}" ]]; then
    mkdir -p "$OUT_DIR"
    printf '%s\n' '# v3.0.0 rehearsal preflight' '- failure_class: environment' '- result: FAIL' > "$OUT_DIR/preflight-summary.md"
  fi
  exit 2
fi

version="$(tr -d '\r' < VERSION)"
cargo_version="$(awk '/^[[:space:]]*version[[:space:]]*=/{gsub(/"/, "", $3); print $3; exit}' Cargo.toml | tr -d '\r')"

echo "Expected v3 rehearsal version: $expected_version"
echo "Git commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)"

check "requested identity is v3.0.0" test "$expected_version" = "v3.0.0"
check "VERSION == v3.0.0" test "$version" = "v3.0.0"
check "Cargo workspace version == 3.0.0" test "$cargo_version" = "3.0.0"
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
check "public testnet remains unapproved" grep -Fq '`public_testnet_ready=false`' docs/VERSION_MATRIX.md
check "public testnet clock remains stopped" grep -Fq '`thirty_day_public_testnet_clock_started=false`' docs/VERSION_MATRIX.md

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
