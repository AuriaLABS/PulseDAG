#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/artifacts/private-testnet/v2_3_0/lag-injection-selected-segment/$RUN_ID}"
MIN_SELECTED_GAP="${MIN_SELECTED_GAP:-96}"
CI_MODE="${CI_MODE:-0}"
HARNESS_PATH="${V2_3_0_RUNTIME_HARNESS:-$ROOT_DIR/scripts/lib/v2_3_0_runtime_harness.sh}"
MANIFEST_JSON="$OUT_DIR/evidence_manifest.json"
TIMELINE_JSON="$OUT_DIR/transition_timeline.json"
FINAL_TABLE="$OUT_DIR/final_convergence_table.md"
GAP_TIMELINE="$OUT_DIR/gap_timeline.json"
COUNTER_SUMMARY="$OUT_DIR/selected_segment_counter_summary.json"
TOPOLOGY_SAMPLES="$OUT_DIR/topology_samples.json"
TARBALL="$OUT_DIR/evidence.tar.gz"
SHA_FILE="$OUT_DIR/evidence.tar.gz.sha256"
SHA256SUMS="$OUT_DIR/SHA256SUMS"
COMMAND_LOG="$OUT_DIR/command-log.txt"

if (( MIN_SELECTED_GAP < 64 )); then
  echo "MIN_SELECTED_GAP must be at least 64; got $MIN_SELECTED_GAP" >&2
  exit 2
fi

mkdir -p "$OUT_DIR/endpoints" "$OUT_DIR/logs" "$OUT_DIR/miners" "$OUT_DIR/pids"
: > "$COMMAND_LOG"
log_cmd() { printf '[%s] %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*" >> "$COMMAND_LOG"; }

archive_evidence() {
  (
    cd "$OUT_DIR"
    tar -czf "$TARBALL" evidence_manifest.json transition_timeline.json gap_timeline.json topology_samples.json selected_segment_counter_summary.json final_convergence_table.md command-log.txt endpoints logs miners pids 2>/dev/null || true
    [[ -f evidence.tar.gz ]] && sha256sum evidence.tar.gz > "$SHA_FILE"
    find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > "$SHA256SUMS"
  )
}

write_synthetic_schema_evidence() {
  local now commit
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  commit="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
  cat > "$TIMELINE_JSON" <<JSON
[
  {"at":"$now","event":"synthetic_schema_only_not_runtime","node":"n5"},
  {"at":"$now","event":"remote_inventory_accepted","node":"n5","peer":"n1","session_id":1},
  {"at":"$now","event":"network_selected_height_gap_observed","node":"n5","gap":$MIN_SELECTED_GAP},
  {"at":"$now","event":"locator_request_sent","node":"n5","peer":"n1","request_id":1},
  {"at":"$now","event":"matching_locator_header_response_accepted","node":"n5","peer":"n1","request_id":1,"session_id":1},
  {"at":"$now","event":"session_completed","node":"n5","session_id":1}
]
JSON
  cat > "$GAP_TIMELINE" <<JSON
[
  {"at":"$now","phase":"schema_pre_isolation","isolated_node":"n5","network_selected_height":0,"n5_selected_height":0,"gap":0},
  {"at":"$now","phase":"schema_gap","isolated_node":"n5","network_selected_height":$MIN_SELECTED_GAP,"n5_selected_height":0,"gap":$MIN_SELECTED_GAP}
]
JSON
  cat > "$TOPOLOGY_SAMPLES" <<JSON
[{"at":"$now","phase":"schema_only","nodes":["n1","n2","n3","n4","n5"],"synthetic_schema_evidence":true}]
JSON
  cat > "$COUNTER_SUMMARY" <<JSON
{"mode":"synthetic-schema","remote_tip_inventory_received_total":1,"locator_requests_sent_total":1,"locator_responses_correlated_total":1,"selected_segment_block_requests_total":$MIN_SELECTED_GAP,"selected_segment_blocks_applied_total":$MIN_SELECTED_GAP,"selected_segment_chunks_completed_total":3,"broadcast_getblock_primary_path":false,"pending_selected_segment_requests":0,"synthetic_schema_evidence":true}
JSON
  for node in n1 n2 n3 n4 n5; do
    printf '{"node":"%s","synthetic_schema_evidence":true,"runtime_observed":false}\n' "$node" > "$OUT_DIR/endpoints/${node}-schema-only.json"
    printf 'synthetic schema-only placeholder for %s; not closeout eligible\n' "$node" > "$OUT_DIR/logs/${node}.log"
  done
  cat > "$FINAL_TABLE" <<MD
| node | selected height | selected tip | ordered DAG tip | state root | retained hash digest | storage/memory retained set | ready | rpc liveness |
| --- | ---: | --- | --- | --- | --- | --- | --- | --- |
| n1 | $MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-sr-$MIN_SELECTED_GAP | synthetic-rh-$MIN_SELECTED_GAP | synthetic | false | synthetic |
| n2 | $MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-sr-$MIN_SELECTED_GAP | synthetic-rh-$MIN_SELECTED_GAP | synthetic | false | synthetic |
| n3 | $MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-sr-$MIN_SELECTED_GAP | synthetic-rh-$MIN_SELECTED_GAP | synthetic | false | synthetic |
| n4 | $MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-tip-$MIN_SELECTED_GAP | synthetic-sr-$MIN_SELECTED_GAP | synthetic-rh-$MIN_SELECTED_GAP | synthetic | false | synthetic |
| n5 | 0 | synthetic-tip-0 | synthetic-tip-0 | synthetic-sr-0 | synthetic-rh-0 | synthetic | false | synthetic |
MD
  python3 - "$MANIFEST_JSON" "$RUN_ID" "$MIN_SELECTED_GAP" "$commit" <<'PY'
import json, sys
path, run_id, gap, commit = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
json.dump({
  "manifest_version":"v2.3.0-task03",
  "result":"PASS",
  "evidence_kind":"synthetic-schema",
  "candidate_commit":commit,
  "run_id":run_id,
  "ci_mode":True,
  "node_count":5,
  "external_miners":0,
  "isolated_node":"n5",
  "configured_min_gap":gap,
  "configured_min_selected_height_gap":gap,
  "observed_network_selected_height_gap":gap,
  "canonical_network_selected_height_gap":gap,
  "remote_tip_inventory_received_total":1,
  "locator_requests_sent_total":1,
  "locator_responses_correlated_total":1,
  "selected_segment_block_requests_total":gap,
  "selected_segment_blocks_applied_total":gap,
  "selected_segment_chunks_completed_total":3,
  "primary_session_path":"correlated_selected_segment",
  "final_convergence":False,
  "storage_memory_consistent":False,
  "public_testnet_ready":False,
  "closeout_eligible":False,
  "synthetic_schema_evidence":True,
  "broadcast_getblock_primary_path":False,
  "pending_selected_segment_requests":0,
  "final_orphan_count":0,
  "final_missing_parent_blockers":0,
  "failure_reasons":["schema-only evidence is synthetic and not closeout eligible"],
  "outputs":["transition_timeline.json","gap_timeline.json","topology_samples.json","selected_segment_counter_summary.json","final_convergence_table.md","endpoints","logs","command-log.txt","evidence.tar.gz","evidence.tar.gz.sha256","SHA256SUMS"]
}, open(path, "w"), indent=2)
PY
}

run_runtime_evidence() {
  log_cmd "runtime drill requested with min selected-height gap >= $MIN_SELECTED_GAP"
  if [[ ! -r "$HARNESS_PATH" ]]; then
    echo "FATAL: runtime-closeout requires $HARNESS_PATH; refusing to fabricate runtime evidence" >&2
    exit 78
  fi
  # shellcheck source=/dev/null
  source "$HARNESS_PATH"
  if ! declare -F v2_3_0_run_lag_injection_selected_segment_drill >/dev/null; then
    echo "FATAL: $HARNESS_PATH does not define v2_3_0_run_lag_injection_selected_segment_drill; refusing to fabricate runtime evidence" >&2
    exit 78
  fi
  v2_3_0_run_lag_injection_selected_segment_drill \
    --out-dir "$OUT_DIR" \
    --run-id "$RUN_ID" \
    --min-selected-gap "$MIN_SELECTED_GAP" \
    --isolated-node n5 \
    --node-count 5 \
    --miner-count 4
  [[ -s "$MANIFEST_JSON" ]] || { echo "FATAL: runtime harness did not write $MANIFEST_JSON" >&2; exit 1; }
  jq -e '
    .result == "PASS" and
    .ci_mode == false and
    .evidence_kind == "runtime" and
    .closeout_eligible == true and
    .synthetic_schema_evidence == false and
    .node_count == 5 and
    .external_miners == 4 and
    .isolated_node == "n5" and
    .observed_network_selected_height_gap >= (.configured_min_selected_height_gap // .configured_min_gap) and
    .canonical_network_selected_height_gap == .observed_network_selected_height_gap and
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
    .pending_selected_segment_requests == 0 and
    .final_orphan_count == 0 and
    .final_missing_parent_blockers == 0
  ' "$MANIFEST_JSON" >/dev/null || {
    echo "FATAL: runtime harness evidence failed closeout semantics; refusing closeout" >&2
    exit 1
  }
}

if (( CI_MODE == 1 )); then
  write_synthetic_schema_evidence
  archive_evidence
else
  run_runtime_evidence
  archive_evidence
fi

echo "lag-injection selected-segment evidence: $OUT_DIR"
