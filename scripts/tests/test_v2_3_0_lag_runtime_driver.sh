#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"
DRIVER="scripts/v2_3_0_lag_injection_selected_segment.sh"
HARNESS="scripts/lib/v2_3_0_runtime_harness.sh"
PATCHER="scripts/lib/patch_v2_3_0_lag_runtime_harness.py"
NODE_MAIN="apps/pulsedagd/src/main.rs"
METRICS="crates/pulsedag-rpc/src/handlers/metrics.rs"

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

bash -n "$DRIVER"
bash -n "$HARNESS"
python3 -m py_compile "$PATCHER"
python3 "$PATCHER" "$HARNESS" "$tmp/patched-harness.sh"
bash -n "$tmp/patched-harness.sh"
source "$tmp/patched-harness.sh"
declare -F v2_3_0_run_lag_injection_selected_segment_drill >/dev/null

if grep -Fq 'local idx="$1" boot="$2" data="$out_dir/data/n$idx"' "$tmp/patched-harness.sh"; then
  echo "patched harness must not expand idx while declaring it" >&2
  exit 1
fi
if grep -Fq 'details="${3:-{}}"' "$tmp/patched-harness.sh"; then
  echo "patched harness must not append a stray JSON brace" >&2
  exit 1
fi
if grep -Fq "trap '_v230_lag_unexpected_exit \$?' EXIT" "$tmp/patched-harness.sh"; then
  echo "patched harness must not retain a function-local EXIT trap" >&2
  exit 1
fi
if grep -Fq "index(\$0, token) {print \$4}" "$tmp/patched-harness.sh"; then
  echo "socket isolation must not treat ss field 4 as the local endpoint" >&2
  exit 1
fi
grep -Fq 'local data="$out_dir/data/n$idx"' "$tmp/patched-harness.sh"
grep -Fq '[[ -n "$details" ]] || details='"'"'{}'"'"'' "$tmp/patched-harness.sh"
grep -Fq "trap '_v230_lag_unexpected_exit \$?' ERR" "$tmp/patched-harness.sh"
grep -Fq 'index($0, token) {print $(NF-2) "\t" $(NF-1)}' "$tmp/patched-harness.sh"
grep -Fq '"${ss_cmd[@]}" -K state established src "$local_endpoint" dst "$peer_endpoint"' "$tmp/patched-harness.sh"
grep -Fq '[[ -z "$remaining" ]] && return 0' "$tmp/patched-harness.sh"

# The normalizer is idempotent so an externally supplied fixed harness remains usable.
python3 "$PATCHER" "$tmp/patched-harness.sh" "$tmp/patched-harness-second.sh"
cmp "$tmp/patched-harness.sh" "$tmp/patched-harness-second.sh"

grep -Fq '.canonical_network_selected_height_gap == .observed_network_selected_height_gap' "$DRIVER"
grep -Fq '.remote_tip_inventory_accepted_total > 0' "$DRIVER"
grep -Fq '.peer_addressed_getblock_sent_total >= .selected_segment_block_requests_total' "$DRIVER"
grep -Fq 'kill -STOP "$n5_pid"' "$HARNESS"
grep -Fq 'kill -CONT "$n5_pid"' "$HARNESS"
grep -Fq 'queued_gossip_discarded' "$HARNESS"
grep -Fq 'canonical_gap_sample > canonical_gap_max' "$HARNESS"
grep -Fq 'harness_gap_sample > harness_gap_max' "$HARNESS"
grep -Fq 'observed_gap="$canonical_gap_max"' "$HARNESS"
grep -Fq 'V2_3_0_GAP_BUILD_MARGIN_BLOCKS:-16' "$HARNESS"
grep -Fq 'built_gap" -ge "$target_gap' "$HARNESS"
grep -Fq 'selected_segment_header_requests_total' "$HARNESS"
grep -Fq 'selected_segment_headers_received_total' "$HARNESS"
grep -Fq 'selected_segment_block_requests_total' "$HARNESS"
grep -Fq 'selected_segment_blocks_applied_total' "$HARNESS"
grep -Fq 'selected_segment_chunks_completed_total' "$HARNESS"
grep -Fq 'peer_addressed_getblock_sent_total' "$HARNESS"
grep -Fq 'peer_addressed_getblock_delta" -lt "$block_requests_delta' "$HARNESS"
grep -Fq 'remote_tip_inventory_accepted_total' "$HARNESS"
grep -Fq 'closeout_eligible:true' "$HARNESS"
grep -Fq 'public_testnet_ready:false' "$HARNESS"
grep -Fq '_v230_lag_package_failure' "$HARNESS"
python3 scripts/tests/test_v2_3_0_selected_segment_source_semantics.py "$NODE_MAIN" "$METRICS"

if grep -Fq 'canonical_gap_max=$(( canonical_gap_max > observed_gap' "$HARNESS"; then
  echo "canonical gap must come from runtime observations, not be forced to the harness gap" >&2
  exit 1
fi
if grep -Fq '$r.node_operational_ready // $r.private_conservative_ready' "$HARNESS"; then
  echo "readiness booleans must use logical OR rather than null coalescing" >&2
  exit 1
fi

cat > "$tmp/manifest.json" <<'JSON'
{
  "result":"PASS",
  "evidence_kind":"runtime",
  "ci_mode":false,
  "node_count":5,
  "external_miners":4,
  "isolated_node":"n5",
  "configured_min_selected_height_gap":96,
  "observed_network_selected_height_gap":96,
  "canonical_network_selected_height_gap":96,
  "remote_tip_inventory_received_total":4,
  "remote_tip_inventory_accepted_total":4,
  "locator_requests_sent_total":1,
  "locator_responses_correlated_total":1,
  "selected_segment_block_requests_total":96,
  "selected_segment_blocks_applied_total":96,
  "selected_segment_chunks_completed_total":3,
  "peer_addressed_getblock_sent_total":96,
  "primary_session_path":"correlated_selected_segment",
  "broadcast_getblock_primary_path":false,
  "final_convergence":true,
  "storage_memory_consistent":true,
  "public_testnet_ready":false,
  "closeout_eligible":true,
  "synthetic_schema_evidence":false,
  "pending_selected_segment_requests":0,
  "final_orphan_count":0,
  "final_missing_parent_blockers":0
}
JSON
jq -e '
  .result == "PASS" and
  .evidence_kind == "runtime" and
  .ci_mode == false and
  .node_count == 5 and
  .external_miners == 4 and
  .isolated_node == "n5" and
  .observed_network_selected_height_gap == .canonical_network_selected_height_gap and
  .observed_network_selected_height_gap >= .configured_min_selected_height_gap and
  .remote_tip_inventory_received_total > 0 and
  .remote_tip_inventory_accepted_total > 0 and
  .locator_requests_sent_total > 0 and
  .locator_responses_correlated_total > 0 and
  .selected_segment_block_requests_total > 0 and
  .selected_segment_blocks_applied_total > 0 and
  .selected_segment_chunks_completed_total > 0 and
  .peer_addressed_getblock_sent_total >= .selected_segment_block_requests_total and
  .primary_session_path == "correlated_selected_segment" and
  .broadcast_getblock_primary_path == false and
  .final_convergence == true and
  .storage_memory_consistent == true and
  .public_testnet_ready == false and
  .closeout_eligible == true and
  .synthetic_schema_evidence == false and
  .pending_selected_segment_requests == 0 and
  .final_orphan_count == 0 and
  .final_missing_parent_blockers == 0
' "$tmp/manifest.json" >/dev/null
