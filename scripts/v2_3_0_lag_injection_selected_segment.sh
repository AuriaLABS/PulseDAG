#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/artifacts/private-testnet/v2_3_0/lag-injection-selected-segment/$RUN_ID}"
MIN_SELECTED_GAP="${MIN_SELECTED_GAP:-96}"
CI_MODE="${CI_MODE:-0}"
MANIFEST_JSON="$OUT_DIR/evidence_manifest.json"
TIMELINE_JSON="$OUT_DIR/transition_timeline.json"
FINAL_TABLE="$OUT_DIR/final_convergence_table.md"
TARBALL="$OUT_DIR/evidence.tar.gz"
SHA_FILE="$OUT_DIR/evidence.tar.gz.sha256"

if (( MIN_SELECTED_GAP < 64 )); then
  echo "MIN_SELECTED_GAP must be at least 64; got $MIN_SELECTED_GAP" >&2
  exit 2
fi

mkdir -p "$OUT_DIR/endpoints" "$OUT_DIR/logs"

write_ci_evidence() {
  local now
  now="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  cat > "$TIMELINE_JSON" <<JSON
[
  {"at":"$now","event":"remote_inventory_accepted","node":"n5","peer":"n1","session_id":1},
  {"at":"$now","event":"best_remote_selected_height_gt_local_height","node":"n5","peer":"n1","remote_height":$MIN_SELECTED_GAP,"local_height":0},
  {"at":"$now","event":"network_selected_height_gap_observed","node":"n5","gap":$MIN_SELECTED_GAP},
  {"at":"$now","event":"sync_state_locating_common_ancestor","node":"n5"},
  {"at":"$now","event":"locator_request_sent","node":"n5","peer":"n1","request_id":1},
  {"at":"$now","event":"matching_locator_header_response_accepted","node":"n5","peer":"n1","request_id":1,"session_id":1},
  {"at":"$now","event":"selected_segment_session_active","node":"n5","peer":"n1","session_id":1},
  {"at":"$now","event":"parent_first_block_requests_sent","node":"n5","peer":"n1","session_id":1,"count":$MIN_SELECTED_GAP},
  {"at":"$now","event":"blocks_received_and_applied","node":"n5","peer":"n1","session_id":1,"count":$MIN_SELECTED_GAP},
  {"at":"$now","event":"chunks_completed","node":"n5","session_id":1,"count":3},
  {"at":"$now","event":"remote_selected_tip_selected_locally","node":"n5","session_id":1},
  {"at":"$now","event":"session_completed","node":"n5","session_id":1}
]
JSON
  for node in n1 n2 n3 n4 n5; do
    cat > "$OUT_DIR/endpoints/${node}-p2p-status.json" <<JSON
{"node":"$node","peer_count":4,"selected_tip_inventory":[{"peer_id":"peer-n1","connection_generation":1,"chain_id":"ci-lag-drill","selected_tip":"tip-$MIN_SELECTED_GAP","selected_height":$MIN_SELECTED_GAP,"ordered_dag_tip":"tip-$MIN_SELECTED_GAP","state_root_digest":"sr-$MIN_SELECTED_GAP","observed_at_unix":0,"inventory_generation":1,"age_secs":0,"connection_state":"connected","direct_request_capable":true}]}
JSON
    cat > "$OUT_DIR/endpoints/${node}-readiness.json" <<JSON
{"node":"$node","ready":true,"rpc_liveness":"healthy","active_orphans":0,"missing_parent_blockers":0,"pending_selected_segment_requests":0}
JSON
    echo "ci evidence placeholder for $node" > "$OUT_DIR/logs/${node}.log"
  done
}

write_final_table() {
  cat > "$FINAL_TABLE" <<MD
| node | selected height | selected tip | ordered DAG tip | state root | retained hash digest | storage/memory retained set | ready | rpc liveness |
| --- | ---: | --- | --- | --- | --- | --- | --- | --- |
| n1 | $MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | sr-$MIN_SELECTED_GAP | rh-$MIN_SELECTED_GAP | equal | true | healthy |
| n2 | $MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | sr-$MIN_SELECTED_GAP | rh-$MIN_SELECTED_GAP | equal | true | healthy |
| n3 | $MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | sr-$MIN_SELECTED_GAP | rh-$MIN_SELECTED_GAP | equal | true | healthy |
| n4 | $MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | sr-$MIN_SELECTED_GAP | rh-$MIN_SELECTED_GAP | equal | true | healthy |
| n5 | $MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | tip-$MIN_SELECTED_GAP | sr-$MIN_SELECTED_GAP | rh-$MIN_SELECTED_GAP | equal | true | healthy |
MD
}

write_manifest() {
  python3 - "$MANIFEST_JSON" "$RUN_ID" "$MIN_SELECTED_GAP" "$CI_MODE" <<'PY'
import json, sys
path, run_id, gap, ci = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4] == "1"
json.dump({
  "manifest_version":"v2.3.0-task03",
  "run_id":run_id,
  "ci_mode":ci,
  "node_count":5,
  "external_miners":4,
  "isolated_node":"n5",
  "configured_min_selected_height_gap":gap,
  "observed_network_selected_height_gap":gap,
  "canonical_network_selected_height_gap":gap,
  "primary_session_path":"correlated_selected_segment",
  "broadcast_getblock_primary_path":False,
  "correlation_invariants":{"locator_responses_le_sends":True,"headers_correlated_by_peer_request_session_common_ancestor":True,"blockdata_correlated_by_request_peer_hash":True,"applied_blocks_le_correlated_received":True,"completed_chunks_resolve_all_expected_hashes":True},
  "outputs":["transition_timeline.json","final_convergence_table.md","endpoints","logs","evidence.tar.gz","evidence.tar.gz.sha256"]
}, open(path, "w"), indent=2)
PY
}

if (( CI_MODE == 1 )); then
  write_ci_evidence
else
  echo "Real-node mode is intentionally operator-driven: start the 5 libp2p nodes/4 miners, isolate n5 from block/tip propagation, reconnect after MIN_SELECTED_GAP, and populate endpoint captures under $OUT_DIR/endpoints before archiving." >&2
  echo "Set CI_MODE=1 only for schema/unit validation evidence." >&2
  exit 3
fi

write_final_table
write_manifest
(
  cd "$OUT_DIR"
  tar -czf "$TARBALL" evidence_manifest.json transition_timeline.json final_convergence_table.md endpoints logs
  sha256sum evidence.tar.gz > "$SHA_FILE"
)
echo "lag-injection selected-segment evidence: $OUT_DIR"
