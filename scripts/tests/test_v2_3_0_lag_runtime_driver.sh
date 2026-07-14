#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

runtime_out="$TMP_DIR/runtime"
if CI_MODE=0 OUT_DIR="$runtime_out" MIN_SELECTED_GAP=96 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >"$TMP_DIR/runtime.log" 2>&1; then
  echo "expected CI_MODE=0 to fail closed when scripts/lib/v2_3_0_runtime_harness.sh is unavailable" >&2
  exit 1
fi
if [[ -e "$runtime_out/evidence_manifest.json" ]] && jq -e '.closeout_eligible == true or .result == "PASS" or .evidence_kind == "runtime"' "$runtime_out/evidence_manifest.json" >/dev/null; then
  echo "runtime fallback must not create PASS/runtime/closeout evidence" >&2
  exit 1
fi
rg -q 'refusing to fabricate runtime evidence' "$TMP_DIR/runtime.log"

test_out="$TMP_DIR/synthetic"
CI_MODE=1 OUT_DIR="$test_out" MIN_SELECTED_GAP=96 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >/dev/null
jq -e '
  .ci_mode == true and
  .evidence_kind == "synthetic-schema" and
  .closeout_eligible == false and
  .synthetic_schema_evidence == true and
  .final_convergence == false and
  .storage_memory_consistent == false and
  .public_testnet_ready == false
' "$test_out/evidence_manifest.json" >/dev/null
test -s "$test_out/SHA256SUMS"

if CI_MODE=0 OUT_DIR="$TMP_DIR/bad-gap" MIN_SELECTED_GAP=63 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >/dev/null 2>&1; then
  echo "expected MIN_SELECTED_GAP < 64 to fail" >&2
  exit 1
fi

echo "v2.3.0 lag runtime driver validation passed"
