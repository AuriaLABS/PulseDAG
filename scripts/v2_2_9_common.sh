#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="$ROOT_DIR/target/release/pulsedagd"
STATE_DIR="$ROOT_DIR/.pulsedag-rehearsal"
LOG_DIR="$STATE_DIR/logs"

require_binary() {
  if [[ ! -x "$BIN_PATH" ]]; then
    echo "[error] Missing executable: $BIN_PATH"
    echo "[hint] Build it first: cargo build --release"
    exit 1
  fi
}

ensure_dirs() {
  mkdir -p "$STATE_DIR" "$LOG_DIR"
}

node_rpc() {
  case "$1" in
    a) echo "127.0.0.1:18080" ;;
    b) echo "127.0.0.1:18081" ;;
    c) echo "127.0.0.1:18082" ;;
    *) return 1 ;;
  esac
}

node_p2p() {
  case "$1" in
    a) echo "0.0.0.0:18181" ;;
    b) echo "0.0.0.0:18182" ;;
    c) echo "0.0.0.0:18183" ;;
    *) return 1 ;;
  esac
}

node_pid_file() { echo "$STATE_DIR/node-$1.pid"; }
node_log_file() { echo "$LOG_DIR/node-$1.log"; }
node_data_dir() { echo "$ROOT_DIR/data/rehearsal-$1"; }

start_node() {
  local node="$1"
  shift

  require_binary
  ensure_dirs

  local rpc p2p data_dir pid_file log_file
  rpc="$(node_rpc "$node")"
  p2p="$(node_p2p "$node")"
  data_dir="$(node_data_dir "$node")"
  pid_file="$(node_pid_file "$node")"
  log_file="$(node_log_file "$node")"

  mkdir -p "$data_dir"

  if [[ -f "$pid_file" ]]; then
    local pid
    pid="$(cat "$pid_file")"
    if kill -0 "$pid" 2>/dev/null; then
      echo "[info] node-$node already running (pid=$pid)"
      return 0
    fi
    rm -f "$pid_file"
  fi

  echo "[info] starting node-$node"
  echo "       rpc=$rpc p2p=$p2p data=$data_dir"

  nohup "$BIN_PATH" \
    --network "rehearsal-$node" \
    --rpc "$rpc" \
    --p2p "$p2p" \
    --data-dir "$data_dir" \
    "$@" >"$log_file" 2>&1 &

  local pid=$!
  echo "$pid" > "$pid_file"
  echo "[ok] node-$node started (pid=$pid, log=$log_file)"
}
