#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"
DRIVER="scripts/v2_3_0_mempool_tx_relay_evidence.sh"
HARNESS="scripts/lib/v2_3_0_runtime_harness.sh"

bash -n "$DRIVER"
bash -n "$HARNESS"

grep -Fq '((.data.connected_peers? // []) | length)' "$DRIVER"
grep -Fq '[[ "$peers" =~ ^[0-9]+$ ]] || peers=0' "$DRIVER"
grep -Fq 'capture_node before_duplicate' "$DRIVER"
grep -Fq '.data.publish_attempts // 0' "$DRIVER"
grep -Fq 'publish_attempts_unchanged:true' "$DRIVER"
grep -Fq 'retransmission_observed:false' "$DRIVER"
if grep -Fq '.data.connected_peers? // empty' "$DRIVER"; then
  echo "connected_peers array must not be compared directly as a number" >&2
  exit 1
fi

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/mempool.json" <<'JSON'
{"data":{"txids":["b","a","a"]}}
JSON
source "$HARNESS"
[[ "$(pulsedag_json_txids_sorted "$tmp/mempool.json" | paste -sd ',')" == "a,b" ]]

cat > "$tmp/p2p.json" <<'JSON'
{"data":{"connected_peers":["a","b","c","d"],"peer_count":4,"peers":[1,2,3,4]}}
JSON
peers="$(jq -r '[((.data.connected_peers? // []) | length),(.data.peer_count? // 0),((.data.peers? // []) | length)] | max // 0' "$tmp/p2p.json")"
[[ "$peers" == 4 ]]

cat > "$tmp/duplicate-evidence.json" <<'JSON'
{"per_node_duplicate_counts":[{"node":"n1","count":1},{"node":"n2","count":1},{"node":"n3","count":1},{"node":"n4","count":1},{"node":"n5","count":1}],"p2p_counter_deltas":[{"node":"n1","publish_attempts_before":7,"publish_attempts_after":7},{"node":"n2","publish_attempts_before":8,"publish_attempts_after":8},{"node":"n3","publish_attempts_before":9,"publish_attempts_after":9},{"node":"n4","publish_attempts_before":10,"publish_attempts_after":10},{"node":"n5","publish_attempts_before":11,"publish_attempts_after":11}],"publish_attempts_unchanged":true,"retransmission_observed":false,"bounded":true}
JSON
jq -e '(.per_node_duplicate_counts | length) == 5 and all(.per_node_duplicate_counts[]; .count == 1) and (.p2p_counter_deltas | length) == 5 and all(.p2p_counter_deltas[]; .publish_attempts_after == .publish_attempts_before) and .publish_attempts_unchanged == true and .retransmission_observed == false and .bounded == true' "$tmp/duplicate-evidence.json" >/dev/null

cat > "$tmp/manifest.json" <<'JSON'
{"result":"PASS","evidence_kind":"runtime","node_count":5,"relay_converged":true,"duplicate_suppression":true,"capacity_rejection_taxonomy":true,"confirmation_cleanup":true,"deterministic_final_mempool_sets":true,"public_testnet_ready":false}
JSON
jq -e '.result == "PASS" and .evidence_kind == "runtime" and .node_count == 5 and .relay_converged and .duplicate_suppression and .capacity_rejection_taxonomy and .confirmation_cleanup and .deterministic_final_mempool_sets and (.public_testnet_ready == false)' "$tmp/manifest.json" >/dev/null
