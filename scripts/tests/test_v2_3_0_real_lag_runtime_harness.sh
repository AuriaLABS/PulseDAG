#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel)"
HARNESS="$ROOT_DIR/scripts/lib/v2_3_0_runtime_harness.sh"
WRAPPER="$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh"

bash -n "$HARNESS"
bash -n "$WRAPPER"

# shellcheck source=/dev/null
source "$HARNESS"
declare -F v2_3_0_run_lag_injection_selected_segment_drill >/dev/null

# The diagnostic path must remain synthetic and ineligible for closeout.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
CI_MODE=1 OUT_DIR="$tmp/schema" MIN_SELECTED_GAP=64 "$WRAPPER" >/dev/null
jq -e '
  .result == "PASS" and
  .ci_mode == true and
  .evidence_kind == "synthetic-schema" and
  .closeout_eligible == false and
  .synthetic_schema_evidence == true and
  .public_testnet_ready == false
' "$tmp/schema/evidence_manifest.json" >/dev/null

# The runtime function must reject underspecified or weakened drills before launch.
set +e
v2_3_0_run_lag_injection_selected_segment_drill \
  --out-dir "$tmp/invalid" \
  --run-id invalid \
  --min-selected-gap 63 \
  --isolated-node n5 \
  --node-count 5 \
  --miner-count 4 >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 64 ]]

# Guard against replacing operational isolation/correlation with placeholders.
grep -q 'iptables -I OUTPUT -m owner --uid-owner' "$HARNESS"
grep -q -- '--consensus-mode ghostdag_dev' "$HARNESS"
grep -q 'n5-health-during-isolation.json' "$HARNESS"
grep -q 'selected_segment_block_requests_total' "$HARNESS"
grep -q 'peer_addressed_getblock_response_total' "$HARNESS"
grep -q 'storage_memory_consistent' "$HARNESS"
! grep -q 'synthetic_schema_evidence.*True' "$HARNESS"

echo "v2.3.0 real lag runtime harness contract tests passed"
