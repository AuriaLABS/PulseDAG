#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${PULSEDAG_REHEARSAL_STATE_DIR:-$ROOT_DIR/.pulsedag-v2_2_16-rehearsal}"
STOP_NODES="${PULSEDAG_STOP_REHEARSAL_NODES:-0}"

info() { echo "[info] $*"; }
warn() { echo "[warn] $*" >&2; }
is_pid_running() { [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null; }

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

main() {
  shopt -s nullglob
  for pid_file in "$STATE_DIR"/miner-*.pid; do
    stop_pid_file "$pid_file" "$(basename "$pid_file" .pid)"
  done

  if [[ "$STOP_NODES" == "1" || "$STOP_NODES" == "true" ]]; then
    for node in c b a; do
      stop_pid_file "$STATE_DIR/node-$node.pid" "node-$node"
    done
  else
    info "leaving nodes running; set PULSEDAG_STOP_REHEARSAL_NODES=1 to stop nodes started by the rehearsal"
  fi
}

main "$@"
