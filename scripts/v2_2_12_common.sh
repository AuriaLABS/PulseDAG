#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_BIN="${PULSEDAGD_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${PULSEDAG_MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
STATE_DIR="${PULSEDAG_REHEARSAL_STATE_DIR:-$ROOT_DIR/.pulsedag-v2_2_12-rehearsal}"
LOG_DIR="${PULSEDAG_REHEARSAL_LOG_DIR:-$STATE_DIR/logs}"
DATA_ROOT="${PULSEDAG_REHEARSAL_DATA_ROOT:-$STATE_DIR/data}"
CHAIN_ID="${PULSEDAG_REHEARSAL_CHAIN_ID:-pulsedag-rehearsal-v2-2-12}"
NETWORK_PROFILE="${PULSEDAG_REHEARSAL_NETWORK:-private}"
P2P_MODE="${PULSEDAG_REHEARSAL_P2P_MODE:-libp2p-real}"
MINER_ADDRESS="${PULSEDAG_MINER_ADDRESS:-pulsedag-rehearsal-miner-a}"
MINER_THREADS="${PULSEDAG_MINER_THREADS:-2}"
MINER_MAX_TRIES="${PULSEDAG_MINER_MAX_TRIES:-500000}"
MINER_SLEEP_MS="${PULSEDAG_MINER_SLEEP_MS:-500}"
MINER_REFRESH_BEFORE_EXPIRY_MS="${PULSEDAG_MINER_REFRESH_BEFORE_EXPIRY_MS:-1000}"
CURL_TIMEOUT_SECS="${PULSEDAG_REHEARSAL_CURL_TIMEOUT_SECS:-5}"

# Multi-host operators should override the listen and dial addresses below:
# - PULSEDAG_NODE_A_RPC / PULSEDAG_NODE_B_RPC / PULSEDAG_NODE_C_RPC set each node's RPC bind address.
# - PULSEDAG_NODE_A_P2P / PULSEDAG_NODE_B_P2P / PULSEDAG_NODE_C_P2P set each node's libp2p listen multiaddr.
# - PULSEDAG_NODE_A_BOOTNODE must be the node-a multiaddr that B and C can dial from their hosts.
#   Include /p2p/<peer-id> for cross-process libp2p dials, or set PULSEDAG_NODE_A_PEER_ID to append it to the default.
# - PULSEDAG_MINER_NODE_URL must point the external miner at node-a's reachable RPC URL.
# State, data, log, chain-id, and binary paths also remain overrideable via PULSEDAG_REHEARSAL_* and PULSEDAG*_BIN.

node_rpc() {
  case "$1" in
    a) echo "${PULSEDAG_NODE_A_RPC:-127.0.0.1:18080}" ;;
    b) echo "${PULSEDAG_NODE_B_RPC:-127.0.0.1:18081}" ;;
    c) echo "${PULSEDAG_NODE_C_RPC:-127.0.0.1:18082}" ;;
    *) echo "unknown node: $1" >&2; return 1 ;;
  esac
}

node_p2p() {
  case "$1" in
    a) echo "${PULSEDAG_NODE_A_P2P:-/ip4/0.0.0.0/tcp/18181}" ;;
    b) echo "${PULSEDAG_NODE_B_P2P:-/ip4/0.0.0.0/tcp/18182}" ;;
    c) echo "${PULSEDAG_NODE_C_P2P:-/ip4/0.0.0.0/tcp/18183}" ;;
    *) echo "unknown node: $1" >&2; return 1 ;;
  esac
}

node_bootnode_a() {
  if [[ -n "${PULSEDAG_NODE_A_BOOTNODE:-}" ]]; then
    echo "$PULSEDAG_NODE_A_BOOTNODE"
  elif [[ -n "${PULSEDAG_NODE_A_PEER_ID:-}" ]]; then
    echo "/ip4/127.0.0.1/tcp/18181/p2p/$PULSEDAG_NODE_A_PEER_ID"
  else
    echo "/ip4/127.0.0.1/tcp/18181"
  fi
}
node_data_dir() { echo "${DATA_ROOT}/node-$1/rocksdb"; }
node_pid_file() { echo "${STATE_DIR}/node-$1.pid"; }
node_log_file() { echo "${LOG_DIR}/node-$1.log"; }
miner_pid_file() { echo "${STATE_DIR}/miner-a.pid"; }
miner_log_file() { echo "${LOG_DIR}/miner-a.log"; }
rpc_url() { echo "http://$(node_rpc "$1")"; }

ensure_dirs() { mkdir -p "$STATE_DIR" "$LOG_DIR" "$DATA_ROOT"; }

require_node_binary() {
  if [[ ! -x "$NODE_BIN" ]]; then
    echo "[error] Missing executable: $NODE_BIN" >&2
    echo "[hint] Run: cargo build --workspace --release" >&2
    exit 1
  fi
}

require_miner_binary() {
  if [[ ! -x "$MINER_BIN" ]]; then
    echo "[error] Missing executable: $MINER_BIN" >&2
    echo "[hint] Run: cargo build --workspace --release" >&2
    exit 1
  fi
}

is_pid_running() {
  [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null
}

stop_pid_file() {
  local pid_file="$1" label="$2"
  if [[ ! -f "$pid_file" ]]; then
    return 0
  fi
  local pid
  pid="$(cat "$pid_file")"
  if is_pid_running "$pid"; then
    echo "[info] stopping $label (pid=$pid)"
    kill "$pid" 2>/dev/null || true
    for _ in {1..30}; do
      is_pid_running "$pid" || break
      sleep 1
    done
    if is_pid_running "$pid"; then
      echo "[warn] $label did not exit gracefully; sending SIGKILL"
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$pid_file"
}

stop_node() { stop_pid_file "$(node_pid_file "$1")" "node-$1"; }
stop_miner_a() { stop_pid_file "$(miner_pid_file)" "miner-a"; }
stop_all_v2_2_12() { stop_miner_a; stop_node c; stop_node b; stop_node a; }

clean_node_data() {
  local node="$1"
  echo "[info] cleaning node-$node data: $(dirname "$(node_data_dir "$node")")"
  rm -rf "$(dirname "$(node_data_dir "$node")")"
}

start_node() {
  local node="$1"; shift || true
  local clean="${PULSEDAG_CLEAN_DATA:-0}"
  local bootnodes=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --clean) clean=1; shift ;;
      --bootnode|--peer) bootnodes+=("$2"); shift 2 ;;
      *) echo "[error] unsupported launcher option: $1" >&2; return 2 ;;
    esac
  done

  require_node_binary
  ensure_dirs
  stop_node "$node"
  [[ "$clean" == "1" || "$clean" == "true" ]] && clean_node_data "$node"

  local rpc p2p data_dir pid_file log_file
  rpc="$(node_rpc "$node")"
  p2p="$(node_p2p "$node")"
  data_dir="$(node_data_dir "$node")"
  pid_file="$(node_pid_file "$node")"
  log_file="$(node_log_file "$node")"
  mkdir -p "$data_dir"

  local cmd=("$NODE_BIN" --network "$NETWORK_PROFILE" --rpc-listen "$rpc" --p2p-listen "$p2p")
  for bootnode in "${bootnodes[@]}"; do
    cmd+=(--bootnode "$bootnode")
  done

  echo "[info] starting node-$node"
  echo "       rpc=$rpc p2p=$p2p chain_id=$CHAIN_ID data=$data_dir"
  (
    export PULSEDAG_NETWORK_PROFILE="rehearsal-$node"
    export PULSEDAG_CHAIN_ID="$CHAIN_ID"
    export PULSEDAG_P2P_ENABLED="true"
    export PULSEDAG_P2P_MODE="$P2P_MODE"
    export PULSEDAG_P2P_MDNS="false"
    export PULSEDAG_ROCKSDB_PATH="$data_dir"
    exec "${cmd[@]}"
  ) >"$log_file" 2>&1 &
  echo "$!" > "$pid_file"
  echo "[ok] node-$node started (pid=$(cat "$pid_file"), log=$log_file)"
}

start_miner_a() {
  require_miner_binary
  ensure_dirs
  stop_miner_a
  local node_url="${PULSEDAG_MINER_NODE_URL:-$(rpc_url a)}"
  local log_file pid_file
  log_file="$(miner_log_file)"
  pid_file="$(miner_pid_file)"
  echo "[info] starting miner-a against $node_url"
  nohup env NO_PROXY="127.0.0.1,localhost,::1,${NO_PROXY:-}" no_proxy="127.0.0.1,localhost,::1,${no_proxy:-}" "$MINER_BIN" \
    --node "$node_url" \
    --miner-address "$MINER_ADDRESS" \
    --threads "$MINER_THREADS" \
    --max-tries "$MINER_MAX_TRIES" \
    --loop \
    --sleep-ms "$MINER_SLEEP_MS" \
    --refresh-before-expiry-ms "$MINER_REFRESH_BEFORE_EXPIRY_MS" \
    >"$log_file" 2>&1 &
  echo "$!" > "$pid_file"
  echo "[ok] miner-a started (pid=$(cat "$pid_file"), log=$log_file)"
}

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
    else:
        cur=cur.get(part)
    if cur is None:
        print("")
        sys.exit(0)
print(cur if not isinstance(cur,(dict,list)) else json.dumps(cur))' "$1"
}
node_height() { http_get "$(rpc_url "$1")/health" | json_field "data.height"; }
node_peer_count() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.connected_peers" | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'; }
node_peer_observation_count() { http_get "$(rpc_url "$1")/p2p/status" | python3 -c 'import json,sys
obj=json.load(sys.stdin).get("data") or {}
connected=len(obj.get("connected_peers") or [])
recovery_success=int(obj.get("peer_recovery_success_count") or 0)
recovering=int(((obj.get("peer_state_summary") or {}).get("recovering")) or 0)
print(max(connected, recovery_success, recovering))'; }
node_p2p_mode() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.mode"; }
node_peer_id() { http_get "$(rpc_url "$1")/p2p/status" | json_field "data.peer_id"; }
