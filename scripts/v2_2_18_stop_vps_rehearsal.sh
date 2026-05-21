#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

NODE_PID_FILE="run/v2_2_18_vps_nodes.pid"
MINER_PID_FILE="run/v2_2_18_vps_miners.pid"

stop_pid_file() {
  local file="$1" kind="$2"
  [[ -f "$file" ]] || { echo "No ${kind} pid file: ${file}"; return 0; }

  while read -r pid rest; do
    [[ -n "${pid:-}" ]] || continue
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      echo "Sent SIGTERM to ${kind} pid=${pid} ${rest}"
    fi
  done < "$file"

  sleep 1

  while read -r pid rest; do
    [[ -n "${pid:-}" ]] || continue
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
      echo "Sent SIGKILL to ${kind} pid=${pid} ${rest}"
    fi
  done < "$file"
}

stop_pid_file "${MINER_PID_FILE}" "miner"
stop_pid_file "${NODE_PID_FILE}" "node"

echo "Stop sequence complete."
