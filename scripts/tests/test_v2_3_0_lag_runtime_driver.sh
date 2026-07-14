#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

fake_harness="$TMP_DIR/fake_runtime_harness.sh"
cat > "$fake_harness" <<'FAKE'
v2_3_0_run_lag_injection_selected_segment_drill() {
  local out='' run='fake' gap='96'
  while (($#)); do
    case "$1" in
      --out-dir) out="$2"; shift 2;;
      --run-id) run="$2"; shift 2;;
      --min-selected-gap) gap="$2"; shift 2;;
      *) shift 2 || true;;
    esac
  done
  mkdir -p "$out/pids" "$out/endpoints" "$out/logs" "$out/miners"
  for n in 1 2 3 4 5; do echo "$$" > "$out/pids/n$n.pid"; done
  cat > "$out/evidence_manifest.json" <<JSON
{"result":"PASS","ci_mode":false,"evidence_kind":"runtime","closeout_eligible":true,"synthetic_schema_evidence":false,"node_count":5,"external_miners":4,"isolated_node":"n5","configured_min_gap":$gap,"configured_min_selected_height_gap":$gap,"observed_network_selected_height_gap":$gap,"remote_tip_inventory_received_total":1,"locator_requests_sent_total":1,"locator_responses_correlated_total":1,"selected_segment_block_requests_total":1,"selected_segment_blocks_applied_total":1,"selected_segment_chunks_completed_total":1,"primary_session_path":"correlated_selected_segment","broadcast_getblock_primary_path":false,"final_convergence":true,"storage_memory_consistent":true,"public_testnet_ready":false,"pending_selected_segment_requests":0,"final_orphan_count":0,"final_missing_parent_blockers":0,"final_state_by_node":[{"node":"n1","ready":true,"compatible_peers":4,"selected_tip":"tip-96","ordered_dag_tip":"tip-96","state_root":"synthetic-sr"},{"node":"n2","ready":true,"compatible_peers":4,"selected_tip":"tip-96","ordered_dag_tip":"tip-96","state_root":"synthetic-sr"},{"node":"n3","ready":true,"compatible_peers":4,"selected_tip":"tip-96","ordered_dag_tip":"tip-96","state_root":"synthetic-sr"},{"node":"n4","ready":true,"compatible_peers":4,"selected_tip":"tip-96","ordered_dag_tip":"tip-96","state_root":"synthetic-sr"},{"node":"n5","ready":true,"compatible_peers":4,"selected_tip":"tip-96","ordered_dag_tip":"tip-96","state_root":"synthetic-sr"}]}
JSON
}
FAKE

runtime_out="$TMP_DIR/runtime"
if V2_3_0_RUNTIME_HARNESS="$fake_harness" CI_MODE=0 OUT_DIR="$runtime_out" MIN_SELECTED_GAP=96 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >"$TMP_DIR/runtime.log" 2>&1; then
  echo "expected CI_MODE=0 to reject fabricated runtime evidence" >&2
  exit 1
fi
rg -q 'validator/shell pid|prefabricated|constant across all fields|not a live pulsedagd' "$TMP_DIR/runtime.log"

missing_bin_out="$TMP_DIR/missing-bin"
if PULSEDAGD_BIN="$TMP_DIR/not-pulsedagd" CI_MODE=0 OUT_DIR="$missing_bin_out" MIN_SELECTED_GAP=96 "$ROOT_DIR/scripts/v2_3_0_lag_injection_selected_segment.sh" >"$TMP_DIR/missing-bin.log" 2>&1; then
  echo "expected CI_MODE=0 to fail closed when release pulsedagd is unavailable" >&2
  exit 1
fi
rg -q 'missing release pulsedagd binary|refusing to fabricate runtime evidence' "$TMP_DIR/missing-bin.log"

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

bash -n "$ROOT_DIR/scripts/lib/v2_3_0_runtime_harness.sh"
echo "v2.3.0 lag runtime driver validation passed"
