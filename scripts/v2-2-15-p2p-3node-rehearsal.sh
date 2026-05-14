#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="v2.2.15"
NODE_COUNT="${PULSEDAG_REHEARSAL_NODE_COUNT:-3}"
CHAIN_ID="${PULSEDAG_REHEARSAL_CHAIN_ID:-pulsedag-rehearsal-v2-2-15}"
RPC_BASE_PORT="${PULSEDAG_REHEARSAL_RPC_BASE_PORT:-19080}"
P2P_BASE_PORT="${PULSEDAG_REHEARSAL_P2P_BASE_PORT:-19180}"
STARTUP_WAIT_SECS="${PULSEDAG_REHEARSAL_STARTUP_WAIT_SECS:-60}"
PEER_WAIT_SECS="${PULSEDAG_REHEARSAL_PEER_WAIT_SECS:-90}"
DURATION_SECS="${PULSEDAG_REHEARSAL_DURATION_SECS:-60}"
CURL_TIMEOUT_SECS="${PULSEDAG_REHEARSAL_CURL_TIMEOUT_SECS:-5}"
KEEP_RUNNING="${PULSEDAG_REHEARSAL_KEEP_RUNNING:-0}"
P2P_MODE="${PULSEDAG_REHEARSAL_P2P_MODE:-libp2p-real}"
EVIDENCE_ROOT="${PULSEDAG_REHEARSAL_EVIDENCE_ROOT:-$ROOT_DIR/evidence/v2.2.15/p2p-rehearsal}"
RUN_ID="${PULSEDAG_REHEARSAL_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)-${NODE_COUNT}node}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RUNTIME_ROOT="${PULSEDAG_REHEARSAL_RUNTIME_ROOT:-$RUN_DIR/runtime}"
DATA_ROOT="$RUNTIME_ROOT/data"
LOG_DIR="$RUN_DIR/logs"
NODE_BIN="${PULSEDAGD_BIN:-}"
FAILURES=0

if [[ -z "$NODE_BIN" ]]; then
  if [[ -x "$ROOT_DIR/target/release/pulsedagd" ]]; then
    NODE_BIN="$ROOT_DIR/target/release/pulsedagd"
  elif [[ -x "$ROOT_DIR/target/debug/pulsedagd" ]]; then
    NODE_BIN="$ROOT_DIR/target/debug/pulsedagd"
  else
    NODE_BIN="$ROOT_DIR/target/release/pulsedagd"
  fi
fi

if (( NODE_COUNT < 2 )); then
  echo "[error] PULSEDAG_REHEARSAL_NODE_COUNT must be at least 2" >&2
  exit 2
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
pid_file() { echo "$RUNTIME_ROOT/$(node_name "$1").pid"; }
log_file() { echo "$LOG_DIR/$(node_name "$1").log"; }
node_evidence_dir() { echo "$RUN_DIR/$(node_name "$1")"; }

http_get() { curl -fsS -m "$CURL_TIMEOUT_SECS" "$1"; }
json_field() {
  python3 -c 'import json,sys
obj=json.load(sys.stdin)
cur=obj
for part in sys.argv[1].split("."):
    if part == "":
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
pretty_json_file() {
  local src="$1" dst="$2"
  python3 -m json.tool "$src" > "$dst.tmp" && mv "$dst.tmp" "$dst"
}

node_height() { http_get "$(rpc_url "$1")/status" | json_field "data.best_height"; }
node_chain_id() { http_get "$(rpc_url "$1")/status" | json_field "data.chain_id"; }
node_selected_tip() { http_get "$(rpc_url "$1")/status" | json_field "data.selected_tip"; }
node_p2p_mode() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.mode"; }
node_real_network() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.connected_peers_are_real_network"; }
node_peer_count() { http_get "$(rpc_url "$1")/p2p/peers" | json_field "data.count"; }
node_peer_observation_count() { http_get "$(rpc_url "$1")/p2p/status" | python3 -c 'import json,sys
obj=json.load(sys.stdin).get("data") or {}
connected=len(obj.get("connected_peers") or [])
recovery_success=int(obj.get("peer_recovery_success_count") or 0)
recovering=int(((obj.get("peer_state_summary") or {}).get("recovering")) or 0)
print(max(connected, recovery_success, recovering))'; }
node_peer_id() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.peer_id"; }

require_tools() {
  command -v curl >/dev/null || fatal "curl is required"
  command -v python3 >/dev/null || fatal "python3 is required for JSON evidence parsing"
  [[ -x "$NODE_BIN" ]] || fatal "missing pulsedagd binary at $NODE_BIN. Run: cargo build --workspace --release (or set PULSEDAGD_BIN)."
}

prepare_dirs() {
  mkdir -p "$RUN_DIR" "$RUNTIME_ROOT" "$DATA_ROOT" "$LOG_DIR"
  printf 'PulseDAG %s P2P rehearsal runtime root\n' "$VERSION" > "$RUNTIME_ROOT/.pulsedag-v2-2-15-rehearsal-root"
  cat > "$RUN_DIR/rehearsal-config.json" <<JSON
{
  "version": "$VERSION",
  "node_count": $NODE_COUNT,
  "chain_id": "$CHAIN_ID",
  "p2p_mode": "$P2P_MODE",
  "rpc_base_port": $RPC_BASE_PORT,
  "p2p_base_port": $P2P_BASE_PORT,
  "startup_wait_secs": $STARTUP_WAIT_SECS,
  "peer_wait_secs": $PEER_WAIT_SECS,
  "duration_secs": $DURATION_SECS,
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
stop_all() {
  for ((node=NODE_COUNT; node>=1; node--)); do
    stop_node "$node"
  done
}
cleanup() {
  if [[ "$KEEP_RUNNING" == "1" || "$KEEP_RUNNING" == "true" ]]; then
    echo "[info] leaving nodes running because PULSEDAG_REHEARSAL_KEEP_RUNNING=$KEEP_RUNNING"
    echo "[info] stop later with: bash scripts/v2-2-15-p2p-clean-rehearsal-data.sh"
  else
    stop_all
  fi
}
trap cleanup EXIT

start_node() {
  local node="$1" log rpc p2p db name cmd bootnode
  name="$(node_name "$node")"
  log="$(log_file "$node")"
  rpc="$(rpc_addr "$node")"
  p2p="$(p2p_listen "$node")"
  db="$(node_db "$node")"
  mkdir -p "$db" "$(node_evidence_dir "$node")"
  cmd=("$NODE_BIN" --network private --rpc-listen "$rpc" --p2p-listen "$p2p")
  if (( node > 1 )); then
    bootnode="$BOOTNODE_1"
    cmd+=(--bootnode "$bootnode")
  fi
  echo "[info] starting $name rpc=$rpc p2p=$p2p data=$db"
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
      tail -80 "$(log_file "$node")" >&2 || true
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

collect_node_evidence() {
  local node="$1" phase="$2" dir url endpoint file
  dir="$(node_evidence_dir "$node")/$phase"
  mkdir -p "$dir"
  for endpoint in health status p2p/status p2p/peers tips dag admin/dag/consistency; do
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
    collect_node_evidence "$node" "$phase"
  done
}

write_summary() {
  local summary="$RUN_DIR/summary.txt" first_height first_tip ok=1 mode real chain peers height tip
  first_height="$(node_height 1 2>/dev/null || echo unknown)"
  first_tip="$(node_selected_tip 1 2>/dev/null || echo unknown)"
  {
    echo "PulseDAG $VERSION P2P rehearsal summary"
    echo "run_id=$RUN_ID"
    echo "node_count=$NODE_COUNT"
    echo "chain_id=$CHAIN_ID"
    echo "evidence_dir=$RUN_DIR"
    echo
    printf '%-8s %-22s %-8s %-8s %-12s %-8s %-44s %s\n' node rpc height peers p2p_mode real_net selected_tip log
  } > "$summary"
  for ((node=1; node<=NODE_COUNT; node++)); do
    height="$(node_height "$node" 2>/dev/null || echo unknown)"
    peers="$(node_peer_observation_count "$node" 2>/dev/null || echo unknown)"
    mode="$(node_p2p_mode "$node" 2>/dev/null || echo unknown)"
    real="$(node_real_network "$node" 2>/dev/null || echo unknown)"
    chain="$(node_chain_id "$node" 2>/dev/null || echo unknown)"
    tip="$(node_selected_tip "$node" 2>/dev/null || echo unknown)"
    printf '%-8s %-22s %-8s %-8s %-12s %-8s %-44s %s\n' "$(node_name "$node")" "$(rpc_url "$node")" "$height" "$peers" "$mode" "$real" "$tip" "$(log_file "$node")" >> "$summary"
    [[ "$chain" == "$CHAIN_ID" ]] || { fail_msg "$(node_name "$node") chain_id mismatch: $chain"; ok=0; }
    [[ "$mode" == "libp2p-real" ]] || { fail_msg "$(node_name "$node") is not using libp2p-real (got $mode)"; ok=0; }
    [[ "$real" == "True" || "$real" == "true" ]] || { fail_msg "$(node_name "$node") does not report real network peer semantics"; ok=0; }
    [[ "$height" == "$first_height" ]] || { fail_msg "$(node_name "$node") height $height does not match node-01 $first_height"; ok=0; }
    [[ "$tip" == "$first_tip" ]] || { fail_msg "$(node_name "$node") selected_tip $tip does not match node-01 $first_tip"; ok=0; }
  done
  cat "$summary"
  (( ok == 1 ))
}

section "Preflight"
require_tools
if [[ "$P2P_MODE" != "libp2p-real" ]]; then
  fatal "main rehearsal must use real libp2p mode; PULSEDAG_REHEARSAL_P2P_MODE=$P2P_MODE"
fi
prepare_dirs
pass "preflight complete; evidence will be written to $RUN_DIR"

section "Start nodes"
BOOTNODE_1=""
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

section "Verify real P2P mode"
for ((node=1; node<=NODE_COUNT; node++)); do
  mode="$(node_p2p_mode "$node")"
  [[ "$mode" == "libp2p-real" ]] || fatal "$(node_name "$node") expected libp2p-real, got $mode"
  pass "$(node_name "$node") reports p2p mode $mode"
done

section "Wait for peer connectivity"
for ((node=2; node<=NODE_COUNT; node++)); do
  wait_for_peer_count "$node" 1 || true
done
echo "[info] node-01 is the local bootnode; downstream nodes provide dial evidence."
collect_all_evidence initial

section "Sustained rehearsal window"
echo "[info] holding $NODE_COUNT-node local P2P rehearsal for ${DURATION_SECS}s"
sleep "$DURATION_SECS"
collect_all_evidence final

section "Convergence checks"
if write_summary; then
  pass "all nodes converged on chain_id, height, selected_tip, and real libp2p mode"
else
  fail_msg "one or more convergence checks failed"
fi

section "Result"
if [[ "$FAILURES" -eq 0 ]]; then
  echo "PASS: $VERSION ${NODE_COUNT}-node P2P rehearsal"
  echo "Evidence: $RUN_DIR"
  exit 0
fi

echo "FAIL: $VERSION ${NODE_COUNT}-node P2P rehearsal ($FAILURES failing section(s))" >&2
echo "Evidence: $RUN_DIR" >&2
exit 1
