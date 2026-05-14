#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="v2.2.15"
NODE_COUNT="${PULSEDAG_REHEARSAL_NODE_COUNT:-3}"
CHAIN_ID="${PULSEDAG_REHEARSAL_CHAIN_ID:-pulsedag-churn-rejoin-v2-2-15}"
RPC_BASE_PORT="${PULSEDAG_REHEARSAL_RPC_BASE_PORT:-19280}"
P2P_BASE_PORT="${PULSEDAG_REHEARSAL_P2P_BASE_PORT:-19380}"
STARTUP_WAIT_SECS="${PULSEDAG_REHEARSAL_STARTUP_WAIT_SECS:-60}"
PEER_WAIT_SECS="${PULSEDAG_REHEARSAL_PEER_WAIT_SECS:-90}"
SYNC_WAIT_SECS="${PULSEDAG_REHEARSAL_SYNC_WAIT_SECS:-180}"
CHURN_HOLD_SECS="${PULSEDAG_REHEARSAL_CHURN_HOLD_SECS:-15}"
CHURN_ADVANCE_BLOCKS="${PULSEDAG_REHEARSAL_CHURN_ADVANCE_BLOCKS:-0}"
CURL_TIMEOUT_SECS="${PULSEDAG_REHEARSAL_CURL_TIMEOUT_SECS:-5}"
KEEP_RUNNING="${PULSEDAG_REHEARSAL_KEEP_RUNNING:-0}"
P2P_MODE="${PULSEDAG_REHEARSAL_P2P_MODE:-libp2p-real}"
EVIDENCE_ROOT="${PULSEDAG_REHEARSAL_EVIDENCE_ROOT:-$ROOT_DIR/evidence/v2.2.15/p2p-churn-rejoin}"
RUN_ID="${PULSEDAG_REHEARSAL_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)-${NODE_COUNT}node-churn-rejoin}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RUNTIME_ROOT="${PULSEDAG_REHEARSAL_RUNTIME_ROOT:-$RUN_DIR/runtime}"
DATA_ROOT="$RUNTIME_ROOT/data"
LOG_DIR="$RUN_DIR/logs"
NODE_BIN="${PULSEDAGD_BIN:-}"
FAILURES=0
BOOTNODE_1=""
REJOIN_NODE="${PULSEDAG_REHEARSAL_REJOIN_NODE:-2}"

if [[ -z "$NODE_BIN" ]]; then
  if [[ -x "$ROOT_DIR/target/release/pulsedagd" ]]; then
    NODE_BIN="$ROOT_DIR/target/release/pulsedagd"
  elif [[ -x "$ROOT_DIR/target/debug/pulsedagd" ]]; then
    NODE_BIN="$ROOT_DIR/target/debug/pulsedagd"
  else
    NODE_BIN="$ROOT_DIR/target/release/pulsedagd"
  fi
fi

section() { echo; echo "========== $* =========="; }
pass() { echo "PASS: $*"; }
fail_msg() { echo "FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }
fatal() { echo "FAIL: $*" >&2; exit 1; }
node_name() { printf 'node-%02d' "$1"; }
rpc_port() { echo $((RPC_BASE_PORT + $1 - 1)); }
p2p_port() { echo $((P2P_BASE_PORT + $1 - 1)); }
rpc_addr() { echo "127.0.0.1:$(rpc_port "$1")"; }
rpc_url() { echo "http://$(rpc_addr "$1")"; }
p2p_listen() { echo "/ip4/0.0.0.0/tcp/$(p2p_port "$1")"; }
p2p_dial() { echo "/ip4/127.0.0.1/tcp/$(p2p_port "$1")"; }
node_dir() { echo "$DATA_ROOT/$(node_name "$1")"; }
node_db() { echo "$(node_dir "$1")/rocksdb"; }
node_marker() { echo "$(node_dir "$1")/db-preserved.marker"; }
pid_file() { echo "$RUNTIME_ROOT/$(node_name "$1").pid"; }
log_file() { echo "$LOG_DIR/$(node_name "$1").log"; }
node_evidence_dir() { echo "$RUN_DIR/$(node_name "$1")"; }

http_get() { curl -fsS -m "$CURL_TIMEOUT_SECS" "$1"; }
http_post_json() { curl -fsS -m "$CURL_TIMEOUT_SECS" -H 'content-type: application/json' -d "$2" "$1"; }
json_field() {
  python3 -c 'import json,sys
obj=json.load(sys.stdin)
cur=obj
for part in sys.argv[1].split("."):
    if not part:
        continue
    if isinstance(cur, list):
        cur=cur[int(part)]
    elif isinstance(cur, dict):
        cur=cur.get(part)
    else:
        cur=None
    if cur is None:
        print("")
        sys.exit(0)
print(cur if not isinstance(cur,(dict,list)) else json.dumps(cur, sort_keys=True))' "$1"
}
pretty_json_file() { python3 -m json.tool "$1" > "$2.tmp" && mv "$2.tmp" "$2"; }

node_height() { http_get "$(rpc_url "$1")/status" | json_field "data.best_height"; }
node_chain_id() { http_get "$(rpc_url "$1")/status" | json_field "data.chain_id"; }
node_selected_tip() { http_get "$(rpc_url "$1")/status" | json_field "data.selected_tip"; }
node_persisted_blocks() { http_get "$(rpc_url "$1")/status" | json_field "data.persisted_block_count"; }
node_p2p_mode() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.mode"; }
node_real_network() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.connected_peers_are_real_network"; }
node_peer_count() { http_get "$(rpc_url "$1")/p2p/peers" | json_field "data.count"; }
node_peer_id() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.peer_id"; }
node_peer_ids() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.connected_peers"; }
node_peer_observation_count() { http_get "$(rpc_url "$1")/p2p/status" | python3 -c 'import json,sys
obj=json.load(sys.stdin).get("data") or {}
connected=len(obj.get("connected_peers") or [])
recovery_success=int(obj.get("peer_recovery_success_count") or 0)
recovering=int(((obj.get("peer_state_summary") or {}).get("recovering")) or 0)
print(max(connected, recovery_success, recovering))'; }

require_tools() {
  command -v curl >/dev/null || fatal "curl is required"
  command -v python3 >/dev/null || fatal "python3 is required for JSON evidence parsing"
  [[ -x "$NODE_BIN" ]] || fatal "missing pulsedagd binary at $NODE_BIN. Run: cargo build --workspace --release (or set PULSEDAGD_BIN)."
  (( NODE_COUNT >= 3 )) || fatal "PULSEDAG_REHEARSAL_NODE_COUNT must be at least 3 for churn/rejoin evidence"
  (( REJOIN_NODE > 1 && REJOIN_NODE <= NODE_COUNT )) || fatal "PULSEDAG_REHEARSAL_REJOIN_NODE must be a non-bootstrap node in this topology"
  [[ "$CHURN_ADVANCE_BLOCKS" =~ ^[0-9]+$ ]] || fatal "PULSEDAG_REHEARSAL_CHURN_ADVANCE_BLOCKS must be numeric"
  [[ "$P2P_MODE" == "libp2p-real" ]] || fatal "churn/rejoin evidence requires libp2p-real mode; got $P2P_MODE"
}

prepare_dirs() {
  mkdir -p "$RUN_DIR" "$RUNTIME_ROOT" "$DATA_ROOT" "$LOG_DIR"
  cat > "$RUN_DIR/rehearsal-config.json" <<JSON
{
  "version": "$VERSION",
  "scenario": "p2p-churn-rejoin",
  "node_count": $NODE_COUNT,
  "rejoin_node": $REJOIN_NODE,
  "churn_advance_blocks": $CHURN_ADVANCE_BLOCKS,
  "chain_id": "$CHAIN_ID",
  "p2p_mode": "$P2P_MODE",
  "rpc_base_port": $RPC_BASE_PORT,
  "p2p_base_port": $P2P_BASE_PORT,
  "node_binary": "$NODE_BIN",
  "run_id": "$RUN_ID"
}
JSON
}

is_pid_running() { [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null; }
stop_node() {
  local node="$1" file pid
  file="$(pid_file "$node")"
  [[ -f "$file" ]] || return 0
  pid="$(cat "$file")"
  if is_pid_running "$pid"; then
    echo "[info] stopping $(node_name "$node") (pid=$pid)"
    kill "$pid" 2>/dev/null || true
    for _ in {1..20}; do
      is_pid_running "$pid" || break
      sleep 1
    done
    if is_pid_running "$pid"; then
      echo "[warn] $(node_name "$node") did not stop gracefully; sending SIGKILL"
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$file"
}
stop_all() { for ((node=NODE_COUNT; node>=1; node--)); do stop_node "$node"; done; }
cleanup() {
  if [[ "$KEEP_RUNNING" == "1" || "$KEEP_RUNNING" == "true" ]]; then
    echo "[info] leaving nodes running because PULSEDAG_REHEARSAL_KEEP_RUNNING=$KEEP_RUNNING"
  else
    stop_all
  fi
}
trap cleanup EXIT

start_node() {
  local node="$1" name log rpc p2p db bootnode
  name="$(node_name "$node")"
  log="$(log_file "$node")"
  rpc="$(rpc_addr "$node")"
  p2p="$(p2p_listen "$node")"
  db="$(node_db "$node")"
  mkdir -p "$db" "$(node_evidence_dir "$node")"
  [[ -f "$(node_marker "$node")" ]] || printf 'created=%s node=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$name" > "$(node_marker "$node")"
  cmd=("$NODE_BIN" --network private --rpc-listen "$rpc" --p2p-listen "$p2p")
  if (( node > 1 )); then
    bootnode="$BOOTNODE_1"
    cmd+=(--bootnode "$bootnode")
  fi
  echo "[info] starting $name rpc=$rpc p2p=$p2p db=$db"
  (
    export PULSEDAG_NETWORK_PROFILE="rehearsal-v2-2-15-$name"
    export PULSEDAG_CHAIN_ID="$CHAIN_ID"
    export PULSEDAG_P2P_ENABLED="true"
    export PULSEDAG_P2P_MODE="$P2P_MODE"
    export PULSEDAG_P2P_MDNS="false"
    export PULSEDAG_P2P_KADEMLIA="true"
    export PULSEDAG_P2P_CONNECTION_SLOT_BUDGET="32"
    export PULSEDAG_ROCKSDB_PATH="$db"
    export RUST_LOG="${RUST_LOG:-info}"
    exec "${cmd[@]}"
  ) >"$log" 2>&1 &
  echo "$!" > "$(pid_file "$node")"
}

wait_for_endpoint() {
  local node="$1" endpoint="$2" deadline=$((SECONDS + STARTUP_WAIT_SECS)) url
  url="$(rpc_url "$node")$endpoint"
  until http_get "$url" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      tail -100 "$(log_file "$node")" >&2 || true
      fatal "timed out waiting for $(node_name "$node") $endpoint at $url"
    fi
    sleep 2
  done
}
wait_for_peer_count() {
  local node="$1" min_count="$2" deadline=$((SECONDS + PEER_WAIT_SECS)) count
  until count="$(node_peer_observation_count "$node" 2>/dev/null)" && [[ "$count" =~ ^[0-9]+$ ]] && (( count >= min_count )); do
    if (( SECONDS >= deadline )); then
      http_get "$(rpc_url "$node")/p2p/status" > "$(node_evidence_dir "$node")/p2p-status-peer-timeout.json" 2>/dev/null || true
      fail_msg "$(node_name "$node") peer observation stayed below $min_count (last=${count:-unknown})"
      return 1
    fi
    sleep 2
  done
  pass "$(node_name "$node") observed peer connectivity signal $count"
}
wait_for_height_at_least() {
  local node="$1" expected="$2" deadline=$((SECONDS + SYNC_WAIT_SECS)) height
  until height="$(node_height "$node" 2>/dev/null)" && [[ "$height" =~ ^[0-9]+$ ]] && (( height >= expected )); do
    if (( SECONDS >= deadline )); then
      fail_msg "$(node_name "$node") height did not catch up to $expected (last=${height:-unknown})"
      return 1
    fi
    sleep 3
  done
  pass "$(node_name "$node") reached height $height (target >= $expected)"
}
wait_for_tip_match() {
  local node="$1" expected_tip="$2" deadline=$((SECONDS + SYNC_WAIT_SECS)) tip
  until tip="$(node_selected_tip "$node" 2>/dev/null)" && [[ -n "$tip" && "$tip" == "$expected_tip" ]]; do
    if (( SECONDS >= deadline )); then
      fail_msg "$(node_name "$node") selected tip did not match expected tip $expected_tip (last=${tip:-unknown})"
      return 1
    fi
    sleep 3
  done
  pass "$(node_name "$node") selected tip matches current network tip"
}

mine_block() {
  local node="$1"
  local label="$2"
  local out="$RUN_DIR/mine-$label.json"
  http_post_json "$(rpc_url "$node")/mine" '{"miner_address":"p2p-rehearsal-external-rpc-client","pow_max_tries":1000000}' > "$out.raw"
  pretty_json_file "$out.raw" "$out"
  rm -f "$out.raw"
  local height hash
  height="$(json_field data.height < "$out")"
  hash="$(json_field data.block_hash < "$out")"
  [[ -n "$height" && -n "$hash" ]] || fatal "mining response did not include height/hash; see $out"
  echo "[info] mined $label height=$height hash=$hash via external RPC client on $(node_name "$node")"
}

collect_node_evidence() {
  local node="$1" phase="$2" dir endpoint file url
  dir="$(node_evidence_dir "$node")/$phase"
  mkdir -p "$dir"
  for endpoint in health status p2p/status p2p/peers p2p/propagation p2p/topics p2p/topology sync/status sync/missing tips dag orphans admin/dag/consistency; do
    file="${endpoint//\//-}.json"
    url="$(rpc_url "$node")/$endpoint"
    if http_get "$url" > "$dir/$file.raw" 2> "$dir/$file.err"; then
      pretty_json_file "$dir/$file.raw" "$dir/$file"
      rm -f "$dir/$file.raw" "$dir/$file.err"
    else
      mv "$dir/$file.raw" "$dir/$file.failed" 2>/dev/null || true
    fi
  done
  cp "$(log_file "$node")" "$dir/$(node_name "$node").log" 2>/dev/null || true
}
collect_all_evidence() {
  local phase="$1"
  section "Collecting $phase evidence"
  for ((node=1; node<=NODE_COUNT; node++)); do
    if [[ -f "$(pid_file "$node")" ]]; then
      collect_node_evidence "$node" "$phase"
    else
      mkdir -p "$(node_evidence_dir "$node")/$phase"
      echo "offline" > "$(node_evidence_dir "$node")/$phase/offline.txt"
      cp "$(log_file "$node")" "$(node_evidence_dir "$node")/$phase/$(node_name "$node").log" 2>/dev/null || true
    fi
  done
}
write_summary() {
  local summary="$RUN_DIR/summary.txt" first_height first_tip ok=1 mode real chain peers peer_ids height tip persisted marker
  first_height="$(node_height 1 2>/dev/null || echo unknown)"
  first_tip="$(node_selected_tip 1 2>/dev/null || echo unknown)"
  {
    echo "PulseDAG $VERSION P2P churn/rejoin evidence summary"
    echo "run_id=$RUN_ID"
    echo "node_count=$NODE_COUNT"
    echo "rejoin_node=$(node_name "$REJOIN_NODE")"
    echo "chain_id=$CHAIN_ID"
    echo "evidence_dir=$RUN_DIR"
    echo
    printf '%-8s %-22s %-8s %-8s %-10s %-8s %-10s %-44s %s\n' node rpc height peers persisted real_net db_marker selected_tip connected_peer_ids
  } > "$summary"
  for ((node=1; node<=NODE_COUNT; node++)); do
    height="$(node_height "$node" 2>/dev/null || echo unknown)"
    peers="$(node_peer_observation_count "$node" 2>/dev/null || echo unknown)"
    mode="$(node_p2p_mode "$node" 2>/dev/null || echo unknown)"
    real="$(node_real_network "$node" 2>/dev/null || echo unknown)"
    chain="$(node_chain_id "$node" 2>/dev/null || echo unknown)"
    tip="$(node_selected_tip "$node" 2>/dev/null || echo unknown)"
    persisted="$(node_persisted_blocks "$node" 2>/dev/null || echo unknown)"
    marker="missing"; [[ -f "$(node_marker "$node")" ]] && marker="present"
    peer_ids="$(node_peer_ids "$node" 2>/dev/null || echo unknown)"
    printf '%-8s %-22s %-8s %-8s %-10s %-8s %-10s %-44s %s\n' "$(node_name "$node")" "$(rpc_url "$node")" "$height" "$peers" "$persisted" "$real" "$marker" "$tip" "$peer_ids" >> "$summary"
    [[ "$chain" == "$CHAIN_ID" ]] || { fail_msg "$(node_name "$node") chain_id mismatch: $chain"; ok=0; }
    [[ "$mode" == "libp2p-real" ]] || { fail_msg "$(node_name "$node") is not using libp2p-real (got $mode)"; ok=0; }
    [[ "$real" == "True" || "$real" == "true" ]] || { fail_msg "$(node_name "$node") does not report real network peer semantics"; ok=0; }
    [[ "$height" == "$first_height" ]] || { fail_msg "$(node_name "$node") height $height does not match node-01 $first_height"; ok=0; }
    [[ "$tip" == "$first_tip" ]] || { fail_msg "$(node_name "$node") selected_tip $tip does not match node-01 $first_tip"; ok=0; }
    [[ "$marker" == "present" ]] || { fail_msg "$(node_name "$node") DB preservation marker missing"; ok=0; }
  done
  cat "$summary"
  (( ok == 1 ))
}

section "Preflight"
require_tools
prepare_dirs
pass "preflight complete; evidence will be written to $RUN_DIR"

section "Start nodes"
start_node 1
wait_for_endpoint 1 /health
wait_for_endpoint 1 /p2p/status
peer_id="$(node_peer_id 1)"
[[ -n "$peer_id" ]] || fatal "node-01 did not expose a libp2p peer_id"
BOOTNODE_1="$(p2p_dial 1)/p2p/$peer_id"
echo "[info] node-01 bootnode=$BOOTNODE_1"
for ((node=2; node<=NODE_COUNT; node++)); do
  start_node "$node"
  wait_for_endpoint "$node" /health
  wait_for_endpoint "$node" /p2p/status
done
pass "all $NODE_COUNT node RPC endpoints started"

section "Initial convergence"
for ((node=2; node<=NODE_COUNT; node++)); do wait_for_peer_count "$node" 1 || true; done
if (( CHURN_ADVANCE_BLOCKS > 0 )); then
  mine_block 1 initial-advance
fi
initial_height="$(node_height 1)"
for ((node=2; node<=NODE_COUNT; node++)); do wait_for_height_at_least "$node" "$initial_height" || true; done
collect_all_evidence initial

section "Stop non-bootstrap node and keep peers running"
pre_stop_persisted="$(node_persisted_blocks "$REJOIN_NODE" 2>/dev/null || echo unknown)"
stop_node "$REJOIN_NODE"
echo "[info] $(node_name "$REJOIN_NODE") stopped with preserved data dir $(node_dir "$REJOIN_NODE") (persisted_blocks_before_stop=$pre_stop_persisted)"
sleep "$CHURN_HOLD_SECS"
for ((i=1; i<=CHURN_ADVANCE_BLOCKS; i++)); do
  mine_block 1 "while-rejoin-node-offline-$i"
  sleep 2
done
current_height="$(node_height 1)"
current_tip="$(node_selected_tip 1)"
for ((node=2; node<=NODE_COUNT; node++)); do
  [[ "$node" -eq "$REJOIN_NODE" ]] && continue
  wait_for_peer_count "$node" 1 || true
  wait_for_height_at_least "$node" "$current_height" || true
done
collect_all_evidence stopped-node

section "Restart stopped node without deleting DB"
[[ -f "$(node_marker "$REJOIN_NODE")" ]] || fatal "DB preservation marker disappeared before restart"
start_node "$REJOIN_NODE"
wait_for_endpoint "$REJOIN_NODE" /health
wait_for_endpoint "$REJOIN_NODE" /p2p/status
wait_for_peer_count "$REJOIN_NODE" 1 || true
wait_for_height_at_least "$REJOIN_NODE" "$current_height" || true
wait_for_tip_match "$REJOIN_NODE" "$current_tip" || true
collect_all_evidence rejoined

section "Convergence checks"
if write_summary; then
  pass "rejoined node recovered peers, height, tip, chain id, and preserved DB marker"
else
  fail_msg "one or more churn/rejoin convergence checks failed"
fi

section "Result"
if [[ "$FAILURES" -eq 0 ]]; then
  echo "PASS: $VERSION P2P churn/rejoin evidence"
  echo "Evidence: $RUN_DIR"
  exit 0
fi

echo "FAIL: $VERSION P2P churn/rejoin evidence ($FAILURES failing section(s))" >&2
echo "Evidence: $RUN_DIR" >&2
exit 1
