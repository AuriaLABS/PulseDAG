#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

runtime_out="$TMP_DIR/runtime"
CI_MODE=0 OUT_DIR="$runtime_out" MIN_SELECTED_GAP=96 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >/dev/null
jq -e '
  .result == "PASS" and
  .ci_mode == false and
  .evidence_kind == "runtime" and
  .closeout_eligible == true and
  .synthetic_schema_evidence == false and
  .node_count == 5 and
  .isolated_node == "n5" and
  .configured_min_gap == 96 and
  .observed_network_selected_height_gap == .canonical_network_selected_height_gap and
  .observed_network_selected_height_gap >= 96 and
  .remote_tip_inventory_received_total > 0 and
  .locator_requests_sent_total > 0 and
  .locator_responses_correlated_total > 0 and
  .selected_segment_block_requests_total > 0 and
  .selected_segment_blocks_applied_total > 0 and
  .selected_segment_chunks_completed_total > 0 and
  .primary_session_path == "correlated_selected_segment" and
  .broadcast_getblock_primary_path == false and
  .final_convergence == true and
  .storage_memory_consistent == true and
  .public_testnet_ready == false and
  .pending_selected_segment_requests == 0 and
  .final_orphan_count == 0 and
  .final_missing_parent_blockers == 0 and
  ([.final_state_by_node[].compatible_peers] | min) == 4
' "$runtime_out/evidence_manifest.json" >/dev/null
jq -e 'map(.event) | index("n5_isolated_offline_same_data_preserved") and index("n5_restarted_same_identity_and_data") and index("selected_segment_headers_accepted") and index("five_node_convergence")' "$runtime_out/transition_timeline.json" >/dev/null
test -s "$runtime_out/SHA256SUMS"

test_out="$TMP_DIR/synthetic"
CI_MODE=1 OUT_DIR="$test_out" MIN_SELECTED_GAP=96 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >/dev/null
jq -e '.ci_mode == true and .evidence_kind == "synthetic-schema" and .closeout_eligible == false and .synthetic_schema_evidence == true and .final_convergence == false and .storage_memory_consistent == false' "$test_out/evidence_manifest.json" >/dev/null

if CI_MODE=0 OUT_DIR="$TMP_DIR/bad-gap" MIN_SELECTED_GAP=63 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >/dev/null 2>&1; then
  echo "expected MIN_SELECTED_GAP < 64 to fail" >&2
  exit 1
fi

echo "v2.3.0 lag runtime driver validation passed"
