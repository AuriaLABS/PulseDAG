#!/usr/bin/env bash
set -euo pipefail

fail=0
checks=0
passes=0

check(){
  local msg="$1"; shift
  checks=$((checks+1))
  if "$@"; then
    passes=$((passes+1))
    echo "PASS: $msg"
  else
    echo "FAIL: $msg"
    fail=1
  fi
}

search_text(){
  local pattern="$1"; shift
  if command -v rg >/dev/null 2>&1; then
    rg -n "$pattern" "$@" >/dev/null
  else
    grep -En "$pattern" "$@" >/dev/null
  fi
}

ver=$(cat VERSION 2>/dev/null || true)
cargo_ver=$(awk '/^version\s*=/{print $3; exit}' Cargo.toml | tr -d '"')
ref="unknown"
commit="unknown"
if command -v git >/dev/null 2>&1; then
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    ref=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || git describe --all --always 2>/dev/null || echo "unknown")
    commit=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
  fi
fi

echo "Git ref: $ref"
echo "Git commit: $commit"

check "VERSION == v2.2.19" test "$ver" = "v2.2.19"
check "Cargo workspace version == 2.2.19" test "$cargo_ver" = "2.2.19"

required=(
  "docs/VERSION_MATRIX.md"
  "docs/KNOWN_LIMITATIONS_V2_2_19.md"
  "configs/private-testnet/v2_2_19/topology.local-3n-1m.json"
  "configs/private-testnet/v2_2_19/topology.rc-5n-4m.json"
  "scripts/v2_2_19_private_5n_4m_rehearsal.sh"
)
for f in "${required[@]}"; do
  check "exists: $f" test -f "$f"
done

check_no_claim(){
  local pattern="$1"; shift
  if search_text "$pattern" "$@"; then
    return 1
  fi
  return 0
}

check "no v2.3.0 readiness claim detected" check_no_claim "(v2\.3\.0 is ready|ready for v2\.3\.0|v2\.3\.0 readiness: yes|v2\.3\.0 ready)" README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md

check "no v3.0 readiness claim detected" check_no_claim "(v3\.0(\.0)? is ready|ready for v3\.0(\.0)?|v3\.0 readiness: yes|v3\.0 ready)" README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md

check "no public testnet launch claim detected" check_no_claim "(public testnet is live|public testnet now live|public testnet readiness: yes|we are ready to launch public testnet)" README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md

summary_result=$([[ $fail -eq 0 ]] && echo PASS || echo FAIL)
echo "SUMMARY: ${summary_result} (${passes}/${checks} explicit checks passed)"

if [[ -n "${OUT_DIR:-}" ]]; then
  mkdir -p "$OUT_DIR"
  cat > "$OUT_DIR/preflight-summary.md" <<SUM
# v2.2.19 preflight

- ref: $ref
- commit: $commit
- version: $ver
- cargo: $cargo_ver
- explicit_checks: $checks
- explicit_passes: $passes
- result: $summary_result
SUM
  printf "%s\n" "$ver" > "$OUT_DIR/version.txt"
  printf "%s\n" "$cargo_ver" > "$OUT_DIR/cargo-workspace-version.txt"
  printf "%s\n" "$ref" > "$OUT_DIR/git-ref.txt"
  printf "%s\n" "$commit" > "$OUT_DIR/git-commit.txt"
fi

exit $fail
