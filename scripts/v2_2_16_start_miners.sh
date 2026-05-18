#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_BIN="${PULSEDAGD_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${PULSEDAG_MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
STATE_DIR="${PULSEDAG_REHEARSAL_STATE_DIR:-$ROOT_DIR/.pulsedag-v2_2_16-rehearsal}"
LOG_DIR="${PULSEDAG_REHEARSAL_LOG_DIR:-$STATE_DIR/logs}"
DATA_ROOT="${PULSEDAG_REHEARSAL_DATA_ROOT:-$STATE_DIR/data}"
EVIDENCE_DIR="${PULSEDAG_REHEARSAL_EVIDENCE_DIR:-$STATE_DIR/evidence}"
CHAIN_ID="${PULSEDAG_REHEARSAL_CHAIN_ID:-pulsedag-rehearsal-v2-2-16}"
NETWORK_PROFILE="${PULSEDAG_REHEARSAL_NETWORK:-private}"
P2P_MODE="${PULSEDAG_REHEARSAL_P2P_MODE:-libp2p-real}"
NODE_COUNT="${PULSEDAG_REHEARSAL_NODE_COUNT:-3}"
CPU_MINERS="${PULSEDAG_REHEARSAL_CPU_MINERS:-2}"
GPU_MINERS="${PULSEDAG_REHEARSAL_GPU_MINERS:-0}"
MINER_TARGETS="${PULSEDAG_REHEARSAL_MINER_TARGETS:-a}"
ASSUME_NODES="${PULSEDAG_REHEARSAL_ASSUME_NODES:-0}"
STARTUP_WAIT_SECS="${PULSEDAG_REHEARSAL_STARTUP_WAIT_SECS:-45}"
MINER_START_GRACE_SECS="${PULSEDAG_REHEARSAL_MINER_START_GRACE_SECS:-3}"
MINER_THREADS="${PULSEDAG_MINER_THREADS:-2}"
MINER_MAX_TRIES="${PULSEDAG_MINER_MAX_TRIES:-500000}"
MINER_SLEEP_MS="${PULSEDAG_MINER_SLEEP_MS:-500}"
MINER_REFRESH_BEFORE_EXPIRY_MS="${PULSEDAG_MINER_REFRESH_BEFORE_EXPIRY_MS:-1000}"
CURL_TIMEOUT_SECS="${PULSEDAG_REHEARSAL_CURL_TIMEOUT_SECS:-5}"

NODE_NAMES=(a b c)

fatal() { echo "[error] $*" >&2; exit 1; }
info() { echo "[info] $*"; }
warn() { echo "[warn] $*" >&2; }

ensure_dirs() { mkdir -p "$STATE_DIR" "$LOG_DIR" "$DATA_ROOT" "$EVIDENCE_DIR"; }

node_rpc() {
  case "$1" in
    a) echo "${PULSEDAG_NODE_A_RPC:-127.0.0.1:18080}" ;;
    b) echo "${PULSEDAG_NODE_B_RPC:-127.0.0.1:18081}" ;;
    c) echo "${PULSEDAG_NODE_C_RPC:-127.0.0.1:18082}" ;;
    *) fatal "unknown node: $1" ;;
  esac
}

node_p2p() {
  case "$1" in
    a) echo "${PULSEDAG_NODE_A_P2P:-/ip4/127.0.0.1/tcp/18181}" ;;
    b) echo "${PULSEDAG_NODE_B_P2P:-/ip4/127.0.0.1/tcp/18182}" ;;
    c) echo "${PULSEDAG_NODE_C_P2P:-/ip4/127.0.0.1/tcp/18183}" ;;
    *) fatal "unknown node: $1" ;;
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

node_url() { echo "http://$(node_rpc "$1")"; }
node_pid_file() { echo "$STATE_DIR/node-$1.pid"; }
node_log_file() { echo "$LOG_DIR/node-$1.log"; }
node_data_dir() { echo "$DATA_ROOT/node-$1/rocksdb"; }
miner_pid_file() { echo "$STATE_DIR/miner-$1.pid"; }
miner_log_file() { echo "$LOG_DIR/miner-$1.log"; }
miner_meta_file() { echo "$STATE_DIR/miner-$1.env"; }

is_pid_running() { [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null; }

require_node_binary() {
  [[ -x "$NODE_BIN" ]] || fatal "missing pulsedagd executable at $NODE_BIN. Run: cargo build --workspace --release (or set PULSEDAGD_BIN)."
}

require_miner_binary() {
  [[ -x "$MINER_BIN" ]] || fatal "missing pulsedag-miner executable at $MINER_BIN. Run: cargo build --workspace --release (or set PULSEDAG_MINER_BIN)."
}

require_tools() {
  command -v curl >/dev/null || fatal "curl is required"
}

http_get() { curl --silent --show-error --fail --max-time "$CURL_TIMEOUT_SECS" "$1"; }

wait_for_endpoint() {
  local url="$1" label="$2" deadline=$((SECONDS + STARTUP_WAIT_SECS))
  until http_get "$url" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      fatal "timed out waiting for $label at $url"
    fi
    sleep 1
  done
}

stop_pid_file() {
  local pid_file="$1" label="$2" pid
  [[ -f "$pid_file" ]] || return 0
  pid="$(cat "$pid_file")"
  if is_pid_running "$pid"; then
    info "stopping $label (pid=$pid)"
    kill "$pid" 2>/dev/null || true
    for _ in {1..20}; do
      is_pid_running "$pid" || break
      sleep 1
    done
    if is_pid_running "$pid"; then
      warn "$label did not exit gracefully; sending SIGKILL"
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  rm -f "$pid_file"
}

start_node() {
  local node="$1" rpc p2p data_dir log_file pid_file cmd=()
  require_node_binary
  rpc="$(node_rpc "$node")"
  p2p="$(node_p2p "$node")"
  data_dir="$(node_data_dir "$node")"
  log_file="$(node_log_file "$node")"
  pid_file="$(node_pid_file "$node")"

  if http_get "$(node_url "$node")/status" >/dev/null 2>&1; then
    info "node-$node already answers on $(node_url "$node"); assuming it is local rehearsal node"
    return 0
  fi

  stop_pid_file "$pid_file" "node-$node"
  mkdir -p "$data_dir"
  cmd=("$NODE_BIN" --network "$NETWORK_PROFILE" --rpc-listen "$rpc" --p2p-listen "$p2p")
  if [[ "$node" != "a" ]]; then
    cmd+=(--bootnode "$(node_bootnode_a)")
  fi

  info "starting node-$node rpc=$rpc p2p=$p2p data=$data_dir"
  (
    export PULSEDAG_NETWORK_PROFILE="rehearsal-v2-2-16-node-$node"
    export PULSEDAG_CHAIN_ID="$CHAIN_ID"
    export PULSEDAG_P2P_ENABLED="true"
    export PULSEDAG_P2P_MODE="$P2P_MODE"
    export PULSEDAG_P2P_MDNS="false"
    export PULSEDAG_ROCKSDB_PATH="$data_dir"
    export RUST_LOG="${RUST_LOG:-info}"
    exec "${cmd[@]}"
  ) >"$log_file" 2>&1 &
  echo "$!" > "$pid_file"
  info "node-$node started (pid=$(cat "$pid_file"), log=$log_file)"
}

start_nodes() {
  local count="$NODE_COUNT" node
  [[ "$count" =~ ^[0-9]+$ ]] || fatal "PULSEDAG_REHEARSAL_NODE_COUNT must be numeric"
  (( count >= 1 && count <= 3 )) || fatal "PULSEDAG_REHEARSAL_NODE_COUNT must be between 1 and 3"
  if [[ "$ASSUME_NODES" == "1" || "$ASSUME_NODES" == "true" ]]; then
    info "assuming $count local nodes are already running"
  else
    for ((i=0; i<count; i++)); do
      node="${NODE_NAMES[$i]}"
      start_node "$node"
    done
  fi
  for ((i=0; i<count; i++)); do
    node="${NODE_NAMES[$i]}"
    wait_for_endpoint "$(node_url "$node")/status" "node-$node /status"
  done
}

choose_target() {
  local index="$1" IFS=',' targets=() raw target count
  read -r -a targets <<< "$MINER_TARGETS"
  count="${#targets[@]}"
  (( count > 0 )) || fatal "PULSEDAG_REHEARSAL_MINER_TARGETS cannot be empty"
  raw="${targets[$(((index - 1) % count))]}"
  target="$(echo "$raw" | tr '[:upper:]' '[:lower:]' | xargs)"
  case "$target" in
    a|b|c) echo "$target" ;;
    http://*|https://*) echo "$target" ;;
    *) fatal "unsupported miner target '$raw'; use a,b,c or an http(s) URL" ;;
  esac
}

target_url() {
  case "$1" in
    a|b|c) node_url "$1" ;;
    http://*|https://*) echo "$1" ;;
    *) fatal "unsupported target '$1'" ;;
  esac
}

start_miner() {
  local id="$1" backend="$2" target="$3" url log_file pid_file address worker_id threads
  require_miner_binary
  url="$(target_url "$target")"
  log_file="$(miner_log_file "$id")"
  pid_file="$(miner_pid_file "$id")"
  address="${PULSEDAG_MINER_ADDRESS_PREFIX:-pulsedag-v2-2-16-miner}-$id"
  worker_id="v2-2-16-$id"
  threads="$MINER_THREADS"
  [[ "$backend" == "gpu" ]] && threads="${PULSEDAG_GPU_MINER_THREADS:-1}"

  stop_pid_file "$pid_file" "miner-$id"
  cat > "$(miner_meta_file "$id")" <<META
id=$id
backend=$backend
target=$target
url=$url
log=$log_file
META
  info "starting miner-$id backend=$backend node=$url log=$log_file"
  (
    export RUST_LOG="${RUST_LOG:-info}"
    exec "$MINER_BIN" \
      --node "$url" \
      --miner-address "$address" \
      --backend "$backend" \
      --threads "$threads" \
      --max-tries "$MINER_MAX_TRIES" \
      --sleep-ms "$MINER_SLEEP_MS" \
      --refresh-before-expiry-ms "$MINER_REFRESH_BEFORE_EXPIRY_MS" \
      --worker-id "$worker_id" \
      --loop \
      --heartbeat
  ) >"$log_file" 2>&1 &
  echo "$!" > "$pid_file"
  sleep "$MINER_START_GRACE_SECS"
  if ! is_pid_running "$(cat "$pid_file")"; then
    warn "miner-$id exited during startup; see $log_file"
    return 1
  fi
  info "miner-$id started (pid=$(cat "$pid_file"))"
}

start_miners() {
  local started=0 failed=0 target
  [[ "$CPU_MINERS" =~ ^[0-9]+$ ]] || fatal "PULSEDAG_REHEARSAL_CPU_MINERS must be numeric"
  [[ "$GPU_MINERS" =~ ^[0-9]+$ ]] || fatal "PULSEDAG_REHEARSAL_GPU_MINERS must be numeric"
  for ((i=1; i<=CPU_MINERS; i++)); do
    target="$(choose_target "$i")"
    if start_miner "cpu-$i" cpu "$target"; then
      started=$((started + 1))
    else
      failed=$((failed + 1))
    fi
  done
  for ((i=1; i<=GPU_MINERS; i++)); do
    target="$(choose_target "$((CPU_MINERS + i))")"
    if start_miner "gpu-$i" gpu "$target"; then
      started=$((started + 1))
    else
      failed=$((failed + 1))
    fi
  done
  (( started > 0 )) || fatal "no miners started successfully"
  if (( failed > 0 )); then
    warn "$failed optional miner(s) failed to stay running; continuing with $started miner(s)"
  fi
}

write_manifest() {
  cat > "$STATE_DIR/manifest.env" <<MANIFEST
STATE_DIR=$STATE_DIR
LOG_DIR=$LOG_DIR
DATA_ROOT=$DATA_ROOT
EVIDENCE_DIR=$EVIDENCE_DIR
CHAIN_ID=$CHAIN_ID
NODE_COUNT=$NODE_COUNT
CPU_MINERS=$CPU_MINERS
GPU_MINERS=$GPU_MINERS
MINER_TARGETS=$MINER_TARGETS
ASSUME_NODES=$ASSUME_NODES
MANIFEST
}

main() {
  ensure_dirs
  require_tools
  start_nodes
  start_miners
  write_manifest
  info "v2.2.16 multi-miner rehearsal processes are running"
  info "logs: $LOG_DIR"
  info "state: $STATE_DIR"
}

main "$@"
