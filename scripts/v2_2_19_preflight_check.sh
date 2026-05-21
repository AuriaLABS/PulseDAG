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

ver=$(cat VERSION 2>/dev/null || true)
cargo_ver=$(awk '/^version\s*=/{print $3; exit}' Cargo.toml | tr -d '"')
ref=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || git describe --all --always)
commit=$(git rev-parse HEAD)

echo "Git ref: $ref"
echo "Git commit: $commit"

check "VERSION == v2.2.19" test "$ver" = "v2.2.19"
check "Cargo workspace version == 2.2.19" test "$cargo_ver" = "2.2.19"

required=(
  "docs/VERSION_MATRIX.md"
  "docs/KNOWN_LIMITATIONS_V2_2_19.md"
  "configs/private-testnet/v2_2_19/topology.local-3n-1m.json"
)
for f in "${required[@]}"; do
  check "exists: $f" test -f "$f"
done

if rg -n "(v2\\.3\\.0 is ready|ready for v2\\.3\\.0|v2\\.3\\.0 readiness: yes|v2\\.3\\.0 ready)" README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md >/dev/null; then
  echo "FAIL: v2.3.0 readiness claim text detected"
  fail=1
else
  echo "PASS: no v2.3.0 readiness claim detected"
fi

if rg -n "(v3\\.0(\\.0)? is ready|ready for v3\\.0(\\.0)?|v3\\.0 readiness: yes|v3\\.0 ready)" README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md >/dev/null; then
  echo "FAIL: v3.0 readiness claim text detected"
  fail=1
else
  echo "PASS: no v3.0 readiness claim detected"
fi

if rg -n "(public testnet is live|public testnet now live|public testnet readiness: yes|we are ready to launch public testnet)" README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md >/dev/null; then
  echo "FAIL: public testnet launch claim detected"
  fail=1
else
  echo "PASS: no public testnet launch claim detected"
fi

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
