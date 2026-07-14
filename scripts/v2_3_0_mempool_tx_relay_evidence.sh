#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(git rev-parse --show-toplevel)"
# shellcheck source=lib/v2_3_0_runtime_harness.sh
source "$ROOT_DIR/scripts/lib/v2_3_0_runtime_harness.sh"

OUT_DIR="${OUT_DIR:?OUT_DIR must be an absolute output directory}"
[[ "$OUT_DIR" = /* ]] || { echo "OUT_DIR must be absolute" >&2; exit 2; }
NODE_COUNT=5
BASE_RPC_PORT="${BASE_RPC_PORT:-29100}"
BASE_P2P_PORT="${BASE_P2P_PORT:-29200}"
STARTUP_TIMEOUT="${STARTUP_TIMEOUT:-90}"
CONVERGENCE_TIMEOUT="${CONVERGENCE_TIMEOUT:-90}"
TX_COUNT="${TX_COUNT:-1}"
REHEARSAL_MEMPOOL_CAPACITY="${REHEARSAL_MEMPOOL_CAPACITY:-2}"
CHAIN_ID="v2_3_0_mempool_runtime_$(date +%s)_$$"
NODE_BIN="$ROOT_DIR/target/release/pulsedagd"
MINER_BIN="$ROOT_DIR/target/release/pulsedag-miner"
mkdir -p "$OUT_DIR" "$OUT_DIR/logs" "$OUT_DIR/endpoints" "$OUT_DIR/tx" "$OUT_DIR/sets" "$OUT_DIR/data"
COMMAND_LOG="$OUT_DIR/command.log"
FAILURES=()
NODE_PIDS=()
START_UTC="$(date -u +%FT%TZ)"
log(){ echo "[$(date -u +%FT%TZ)] $*" | tee -a "$COMMAND_LOG"; }
fail(){ FAILURES+=("$*"); log "FAIL: $*"; }
rpc_url(){ local i="$1"; echo "http://127.0.0.1:$((BASE_RPC_PORT+i))"; }
post_json(){ local url="$1" body="$2" out="$3"; curl -fsS --connect-timeout 2 --max-time 20 -H 'content-type: application/json' -d "$body" "$url" | tee "$out" >/dev/null; }
capture_node(){ local stage="$1" i url; for i in $(seq 1 "$NODE_COUNT"); do url="$(rpc_url "$i")"; for ep in mempool p2p/status p2p/topology sync/status checks runtime; do mkdir -p "$OUT_DIR/endpoints/$stage"; curl -fsS --connect-timeout 2 --max-time 10 "$url/$ep" > "$OUT_DIR/endpoints/$stage/n${i}-${ep//\//-}.json" || true; done; done; }
cleanup(){ local rc=$? i; set +e; for pid in "${NODE_PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done; for pid in "${NODE_PIDS[@]:-}"; do wait "$pid" 2>/dev/null || true; done; for i in $(seq 1 "$NODE_COUNT"); do pulsedag_wait_port_closed "$((BASE_RPC_PORT+i))" 20 || true; pulsedag_wait_port_closed "$((BASE_P2P_PORT+i))" 20 || true; done; if [[ ! -f "$OUT_DIR/evidence_manifest.json" ]]; then write_manifest FAIL; fi; pulsedag_write_checksums "$OUT_DIR" || true; exit "$rc"; }
trap cleanup EXIT
write_manifest(){
  local result="$1" end_utc final_digest="" per_nodes="[]" submitted="[]" confirmed="[]" relay=false dedupe=false capacity=false cleanup=false deterministic=false topology=false duplicate_evidence="{}" rejection="{}" confirmation="[]" topology_evidence="[]"
  end_utc="$(date -u +%FT%TZ)"
  [[ -f "$OUT_DIR/final_mempool_digest.txt" ]] && final_digest="$(cat "$OUT_DIR/final_mempool_digest.txt")"
  [[ -f "$OUT_DIR/per_node_final.json" ]] && per_nodes="$(cat "$OUT_DIR/per_node_final.json")"
  [[ -f "$OUT_DIR/submitted_txids.json" ]] && submitted="$(cat "$OUT_DIR/submitted_txids.json")"
  [[ -f "$OUT_DIR/confirmed_txids.json" ]] && confirmed="$(cat "$OUT_DIR/confirmed_txids.json")"
  [[ -f "$OUT_DIR/relay_converged.proof" ]] && relay=true
  [[ -f "$OUT_DIR/duplicate_suppression.proof" ]] && dedupe=true
  [[ -f "$OUT_DIR/capacity_rejection_taxonomy.proof" ]] && capacity=true
  [[ -f "$OUT_DIR/confirmation_cleanup.proof" ]] && cleanup=true
  [[ -f "$OUT_DIR/deterministic_final_mempool_sets.proof" ]] && deterministic=true
  [[ -f "$OUT_DIR/topology_stable.proof" ]] && topology=true
  [[ -f "$OUT_DIR/duplicate_evidence.json" ]] && duplicate_evidence="$(cat "$OUT_DIR/duplicate_evidence.json")"
  [[ -f "$OUT_DIR/rejection_evidence.json" ]] && rejection="$(cat "$OUT_DIR/rejection_evidence.json")"
  [[ -f "$OUT_DIR/confirmation_evidence.json" ]] && confirmation="$(cat "$OUT_DIR/confirmation_evidence.json")"
  [[ -f "$OUT_DIR/topology_evidence.json" ]] && topology_evidence="$(cat "$OUT_DIR/topology_evidence.json")"
  jq -n --arg result "$result" --arg commit "$(git rev-parse HEAD)" --arg start "$START_UTC" --arg end "$end_utc" --arg digest "$final_digest" \
    --argjson node_count "$NODE_COUNT" --argjson submitted "$submitted" --argjson confirmed "$confirmed" --argjson per_nodes "$per_nodes" \
    --argjson relay "$relay" --argjson dedupe "$dedupe" --argjson capacity "$capacity" --argjson cleanup "$cleanup" --argjson deterministic "$deterministic" --argjson topology "$topology" \
    --argjson duplicate_evidence "$duplicate_evidence" --argjson rejection "$rejection" --argjson confirmation "$confirmation" --argjson topology_evidence "$topology_evidence" \
    --argjson failures "$(printf '%s\n' "${FAILURES[@]:-}" | jq -R . | jq -s .)" \
    '{result:$result,evidence_kind:"runtime",candidate_commit:$commit,node_count:$node_count,relay_converged:$relay,duplicate_suppression:$dedupe,capacity_rejection_taxonomy:$capacity,confirmation_cleanup:$cleanup,deterministic_final_mempool_sets:$deterministic,submitted_txids:$submitted,confirmed_txids:$confirmed,final_mempool_digest:$digest,public_testnet_ready:false,per_node_final:$per_nodes,topology_status:{required_peers_per_node:4,stable:$topology,nodes:$topology_evidence},duplicate_evidence:$duplicate_evidence,rejection:$rejection,confirmation_evidence:$confirmation,timestamps:{start_utc:$start,end_utc:$end},failure_reasons:$failures}' > "$OUT_DIR/evidence_manifest.json"
}

log "building release binaries"
cargo build -p pulsedagd -p pulsedag-miner --release --locked 2>&1 | tee "$OUT_DIR/build.log"
[[ -x "$NODE_BIN" && -x "$MINER_BIN" ]] || { echo "release binaries missing" >&2; exit 3; }

start_node(){ local i="$1" boot="$2" data="$OUT_DIR/data/n$i"; mkdir -p "$data"; local args=("$NODE_BIN" --network private --rpc-listen "127.0.0.1:$((BASE_RPC_PORT+i))" --p2p-listen "/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+i))"); [[ -n "$boot" ]] && args+=(--bootnode "$boot"); log "start n$i ${args[*]}"; PULSEDAG_CHAIN_ID="$CHAIN_ID" PULSEDAG_ROCKSDB_PATH="$data/rocksdb" PULSEDAG_API_PROFILE=local_dev PULSEDAG_P2P_MODE=libp2p-real PULSEDAG_P2P_MDNS=false PULSEDAG_P2P_KADEMLIA=true PULSEDAG_MEMPOOL_MAX_TRANSACTIONS="$REHEARSAL_MEMPOOL_CAPACITY" RUST_LOG="pulsedagd=info,pulsedag_p2p=info" RUST_LOG_STYLE=never "${args[@]}" > "$OUT_DIR/logs/n$i.log" 2>&1 & NODE_PIDS+=("$!"); }
start_node 1 ""
pulsedag_wait_http_ok "$(rpc_url 1)/p2p/status" "$OUT_DIR/endpoints/n1-p2p-bootstrap.json" "$STARTUP_TIMEOUT" || { fail "n1 p2p status unavailable"; write_manifest FAIL; exit 1; }
PEER_ID="$(jq -r '.data.peer_id // .data.local_peer_id // empty' "$OUT_DIR/endpoints/n1-p2p-bootstrap.json")"
[[ -n "$PEER_ID" ]] || { fail "unable to extract n1 peer id"; write_manifest FAIL; exit 1; }
BOOT="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+1))/p2p/$PEER_ID"; echo "$BOOT" > "$OUT_DIR/bootnode.txt"
for i in 2 3 4 5; do start_node "$i" "$BOOT"; done
for i in $(seq 1 "$NODE_COUNT"); do pulsedag_wait_http_ok "$(rpc_url "$i")/status" "$OUT_DIR/endpoints/n${i}-status-ready.json" "$STARTUP_TIMEOUT" || fail "n$i rpc readiness failed"; done
topology_ok=0; start=$(date +%s); while (( $(date +%s) - start < CONVERGENCE_TIMEOUT )); do capture_node topology; topology_ok=1; topo="[]"; for i in $(seq 1 "$NODE_COUNT"); do peers=$(jq -r '[.data.connected_peers? // empty, .data.peer_count? // empty, .data.peers? // [] | length] | max // 0' "$OUT_DIR/endpoints/topology/n${i}-p2p-status.json" 2>/dev/null || echo 0); topo="$(jq --arg node "n$i" --argjson peers "$peers" '. + [{node:$node,connected_peers:$peers}]' <<<"$topo")"; (( peers >= NODE_COUNT - 1 )) || topology_ok=0; done; echo "$topo" > "$OUT_DIR/topology_evidence.json"; (( topology_ok == 1 )) && break; sleep 2; done
if (( topology_ok == 1 )); then touch "$OUT_DIR/topology_stable.proof"; else fail "topology did not stabilize with four peers per node"; fi
capture_node before_submit

post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding-wallet.json"
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding2-wallet.json"
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/funding3-wallet.json"
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient-wallet.json"
FROM="$(jq -r '.data.address' "$OUT_DIR/tx/funding-wallet.json")"; PRIV="$(jq -r '.data.private_key' "$OUT_DIR/tx/funding-wallet.json")"; TO="$(jq -r '.data.address' "$OUT_DIR/tx/recipient-wallet.json")"
FROM2="$(jq -r '.data.address' "$OUT_DIR/tx/funding2-wallet.json")"; PRIV2="$(jq -r '.data.private_key' "$OUT_DIR/tx/funding2-wallet.json")"
FROM3="$(jq -r '.data.address' "$OUT_DIR/tx/funding3-wallet.json")"; PRIV3="$(jq -r '.data.private_key' "$OUT_DIR/tx/funding3-wallet.json")"
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/funding-mine.json"
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM2\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/funding2-mine.json"
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM3\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/funding3-mine.json"
sleep 3
TRANSFER_BODY="{\"from\":\"$FROM\",\"to\":\"$TO\",\"amount\":1,\"fee\":1,\"private_key\":\"$PRIV\"}"
post_json "$(rpc_url 1)/wallet/transfer" "$TRANSFER_BODY" "$OUT_DIR/tx/submit-n1.json"
TXID="$(jq -r '.data.txid // empty' "$OUT_DIR/tx/submit-n1.json")"; [[ -n "$TXID" ]] || { fail "no submitted txid"; write_manifest FAIL; exit 1; }
jq -n --arg txid "$TXID" '[$txid]' > "$OUT_DIR/submitted_txids.json"

relay_ok=0; start=$(date +%s); while (( $(date +%s) - start < CONVERGENCE_TIMEOUT )); do capture_node after_relay; relay_ok=1; for i in $(seq 1 "$NODE_COUNT"); do jq -e --arg txid "$TXID" '(.data.txids // []) | index($txid)' "$OUT_DIR/endpoints/after_relay/n${i}-mempool.json" >/dev/null || relay_ok=0; done; (( relay_ok == 1 )) && break; sleep 2; done
if (( relay_ok == 1 )); then touch "$OUT_DIR/relay_converged.proof"; else fail "txid did not relay to all five mempools"; fi
DUP_BODY="$(jq -ce '{transaction:.data.transaction} | select(.transaction != null)' "$OUT_DIR/tx/submit-n1.json")" || { fail "wallet transfer did not return a duplicate-submittable transaction"; write_manifest FAIL; exit 1; }
post_json "$(rpc_url 2)/tx/submit" "$DUP_BODY" "$OUT_DIR/tx/duplicate-submit-n2.json" || true
capture_node after_duplicate
dupe_counts="[]"
for i in $(seq 1 "$NODE_COUNT"); do count=$(jq -r --arg txid "$TXID" '(.data.txids // []) | map(select(. == $txid)) | length' "$OUT_DIR/endpoints/after_duplicate/n${i}-mempool.json"); dupe_counts="$(jq --arg node "n$i" --argjson count "$count" '. + [{node:$node,count:$count}]' <<<"$dupe_counts")"; [[ "$count" = 1 ]] || fail "duplicate count on n$i was $count"; done
dupe_code="$(jq -r '.error.code // ""' "$OUT_DIR/tx/duplicate-submit-n2.json")"; dupe_msg="$(jq -r '.error.message // .data.txid // ""' "$OUT_DIR/tx/duplicate-submit-n2.json")"
if [[ "$dupe_code" == "TX_REJECTED" && "$dupe_msg" == *duplicate* ]]; then
  jq -n --arg via n2 --slurpfile response "$OUT_DIR/tx/duplicate-submit-n2.json" --argjson counts "$dupe_counts" '{resubmitted_via:$via,response:$response[0],per_node_duplicate_counts:$counts,bounded:true}' > "$OUT_DIR/duplicate_evidence.json"
  touch "$OUT_DIR/duplicate_suppression.proof"
else
  fail "duplicate submit response did not prove duplicate rejection: code=$dupe_code message=$dupe_msg"
fi
# Capacity rejection through the private rehearsal cap: create a second accepted tx, then a third rejected tx.
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient2-wallet.json"; TO2="$(jq -r '.data.address' "$OUT_DIR/tx/recipient2-wallet.json")"
post_json "$(rpc_url 1)/wallet/transfer" "{\"from\":\"$FROM2\",\"to\":\"$TO2\",\"amount\":1,\"fee\":1,\"private_key\":\"$PRIV2\"}" "$OUT_DIR/tx/capacity-fill.json" || true
post_json "$(rpc_url 1)/wallet/new" '{}' "$OUT_DIR/tx/recipient3-wallet.json"; TO3="$(jq -r '.data.address' "$OUT_DIR/tx/recipient3-wallet.json")"
post_json "$(rpc_url 1)/wallet/transfer" "{\"from\":\"$FROM3\",\"to\":\"$TO3\",\"amount\":1,\"fee\":1,\"private_key\":\"$PRIV3\"}" "$OUT_DIR/tx/capacity-reject.json" || true
cap_code="$(jq -r '.error.code // ""' "$OUT_DIR/tx/capacity-reject.json")"; cap_reason="$(jq -r '.error.message // ""' "$OUT_DIR/tx/capacity-reject.json")"
if [[ "$cap_code" == "TX_REJECTED" && ( "$cap_reason" == *"backpressure active"* || "$cap_reason" == *"capacity exceeded"* || "$cap_reason" == *"mempool pressure"* ) ]]; then
  jq -n --slurpfile response "$OUT_DIR/tx/capacity-reject.json" --arg code "$cap_code" --arg reason "$cap_reason" '{code:$code,reason:$reason,response:$response[0],bounded:true,private_rehearsal_capacity_override:true,taxonomy:"mempool_capacity"}' > "$OUT_DIR/rejection_evidence.json"
  touch "$OUT_DIR/capacity_rejection_taxonomy.proof"
else
  fail "capacity rejection did not return specific capacity taxonomy: code=$cap_code reason=$cap_reason"
fi
post_json "$(rpc_url 1)/mine" "{\"miner_address\":\"$FROM\",\"pow_max_tries\":1000000}" "$OUT_DIR/tx/confirm-mine.json"
sleep 5
capture_node after_confirmation
confirmations="[]"; common_hash=""; common_height=""; cleanup_ok=1
for i in $(seq 1 "$NODE_COUNT"); do
  curl -fsS --connect-timeout 2 --max-time 10 "$(rpc_url "$i")/txs/$TXID" > "$OUT_DIR/endpoints/after_confirmation/n${i}-tx-$TXID.json" || cleanup_ok=0
  jq -e --arg txid "$TXID" '((.data.txids // []) | index($txid)) == null' "$OUT_DIR/endpoints/after_confirmation/n${i}-mempool.json" >/dev/null || { fail "confirmed tx remained in n$i mempool"; cleanup_ok=0; }
  is_confirmed="$(jq -r '.data.is_confirmed // false' "$OUT_DIR/endpoints/after_confirmation/n${i}-tx-$TXID.json" 2>/dev/null)"; block_hash="$(jq -r '.data.block_hash // ""' "$OUT_DIR/endpoints/after_confirmation/n${i}-tx-$TXID.json" 2>/dev/null)"; block_height="$(jq -r '.data.block_height // ""' "$OUT_DIR/endpoints/after_confirmation/n${i}-tx-$TXID.json" 2>/dev/null)"
  [[ "$is_confirmed" == true && -n "$block_hash" && -n "$block_height" ]] || { fail "n$i did not report confirmed tx with block hash/height"; cleanup_ok=0; }
  [[ -z "$common_hash" || "$common_hash" == "$block_hash" ]] || { fail "n$i confirmation block hash differed"; cleanup_ok=0; }
  [[ -z "$common_height" || "$common_height" == "$block_height" ]] || { fail "n$i confirmation block height differed"; cleanup_ok=0; }
  common_hash="$block_hash"; common_height="$block_height"
  confirmations="$(jq --arg node "n$i" --arg txid "$TXID" --arg block_hash "$block_hash" --argjson block_height "${block_height:-0}" '. + [{node:$node,txid:$txid,block_hash:$block_hash,block_height:$block_height}]' <<<"$confirmations")"
done
jq -n --arg txid "$TXID" '[$txid]' > "$OUT_DIR/confirmed_txids.json"
echo "$confirmations" > "$OUT_DIR/confirmation_evidence.json"
(( cleanup_ok == 1 )) && touch "$OUT_DIR/confirmation_cleanup.proof"
capture_node final
for i in $(seq 1 "$NODE_COUNT"); do pulsedag_json_txids_sorted "$OUT_DIR/endpoints/final/n${i}-mempool.json" > "$OUT_DIR/sets/n${i}-final-txids.txt"; digest=$(pulsedag_sha256_file "$OUT_DIR/sets/n${i}-final-txids.txt"); jq -n --arg node "n$i" --arg digest "$digest" --slurpfile txids <(jq -R . "$OUT_DIR/sets/n${i}-final-txids.txt" | jq -s .) '{node:$node,digest:$digest,txids:$txids[0]}' > "$OUT_DIR/sets/n${i}.json"; done
jq -s . "$OUT_DIR"/sets/n*.json > "$OUT_DIR/per_node_final.json"
first="$(jq -r '.[0].digest' "$OUT_DIR/per_node_final.json")"; if jq -e --arg d "$first" 'all(.digest == $d)' "$OUT_DIR/per_node_final.json" >/dev/null; then touch "$OUT_DIR/deterministic_final_mempool_sets.proof"; else fail "final mempool sets differ"; fi
echo "$first" > "$OUT_DIR/final_mempool_digest.txt"
if ((${#FAILURES[@]})); then write_manifest FAIL; exit 1; fi
write_manifest PASS
log "runtime evidence PASS"
