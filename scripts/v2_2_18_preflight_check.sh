#!/usr/bin/env bash
set -euo pipefail
fail=0
check(){ local msg="$1"; shift; if "$@"; then echo "PASS: $msg"; else echo "FAIL: $msg"; fail=1; fi }
ver=$(cat VERSION 2>/dev/null || true)
cargo_ver=$(awk '/^version\s*=/{print $3; exit}' Cargo.toml | tr -d '"')
ref=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || git describe --all --always)
commit=$(git rev-parse HEAD)
echo "Git ref: $ref"
echo "Git commit: $commit"
check "VERSION == v2.2.18" test "$ver" = "v2.2.18"
check "Cargo workspace version == 2.2.18" test "$cargo_ver" = "2.2.18"
for f in docs/CLOSING_CHECKLIST_V2_2_18.md docs/RELEASE_NOTES_V2_2_18.md docs/V2_2_18_PRIVATE_TESTNET_RC_PLAN.md docs/V2_2_18_PREFLIGHT.md configs/private-testnet/v2_2_18/topology.local-3n-1m.json configs/private-testnet/v2_2_18/topology.rc-5n-4m.json; do
  check "exists: $f" test -f "$f"
done
topology_json_check="SKIP (jq not installed)"
if command -v jq >/dev/null 2>&1; then
  jq empty configs/private-testnet/v2_2_18/topology.local-3n-1m.json && jq empty configs/private-testnet/v2_2_18/topology.rc-5n-4m.json
  topology_json_check="PASS"
fi
echo "Topology JSON check: $topology_json_check"
if rg -n "(v2\.3\.0 is ready|ready for v2\.3\.0|v3\.0 is ready|ready for v3\.0)" README.md docs/VERSION_MATRIX.md docs/RELEASE_NOTES_V2_2_18.md >/dev/null; then
  echo "FAIL: readiness claim text detected"; fail=1
else
  echo "PASS: no readiness claims for v2.3.0/v3.0"
fi
echo "Next required (not run here): cargo fmt --check; cargo test --workspace; cargo build --workspace --release"
if [[ -n "${OUT_DIR:-}" ]]; then
  mkdir -p "$OUT_DIR"
  printf "# v2.2.18 preflight\n\n- ref: %s\n- commit: %s\n- version: %s\n- cargo: %s\n- topology_json_check: %s\n- result: %s\n" "$ref" "$commit" "$ver" "$cargo_ver" "$topology_json_check" "$([[ $fail -eq 0 ]] && echo PASS || echo FAIL)" > "$OUT_DIR/preflight-summary.md"
  printf "%s\n" "$ver" > "$OUT_DIR/version.txt"
  printf "%s\n" "$cargo_ver" > "$OUT_DIR/cargo-workspace-version.txt"
  printf "%s\n" "$ref" > "$OUT_DIR/git-ref.txt"
  printf "%s\n" "$commit" > "$OUT_DIR/git-commit.txt"
  printf "%s\n" "$topology_json_check" > "$OUT_DIR/topology-json-check.txt"
fi
[[ $fail -eq 0 ]] && echo "SUMMARY: PASS" || echo "SUMMARY: FAIL"
exit $fail
