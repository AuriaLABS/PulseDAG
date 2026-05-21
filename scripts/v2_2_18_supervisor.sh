#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOPOLOGY_MANIFEST="${TOPOLOGY_MANIFEST:-$ROOT_DIR/configs/private-testnet/v2_2_18/topology.local-3n-1m.json}"
NODE_BIN="${PULSEDAGD_BIN:-$ROOT_DIR/target/debug/pulsedagd}"
MINER_BIN="${PULSEDAG_MINER_BIN:-$ROOT_DIR/target/debug/pulsedag-miner}"
STATE_DIR="${SUPERVISOR_STATE_DIR:-$ROOT_DIR/.supervisor-v2_2_18}"
HEALTH_INTERVAL_SECS="${SUPERVISOR_HEALTH_INTERVAL_SECS:-10}"
HEALTH_TIMEOUT_SECS="${SUPERVISOR_HEALTH_TIMEOUT_SECS:-5}"
REHEARSAL_MODE="${REHEARSAL_MODE:-unspecified}"

RUN_ID=""
EVIDENCE_DIR=""
TIMELINE_FILE=""
NODE_PID_DIR=""
MINER_PID_DIR=""
NODE_LOG_DIR=""
MINER_LOG_DIR=""
SNAPSHOT_DIR=""

fatal(){ echo "[error] $*" >&2; exit 1; }
info(){ echo "[info] $*"; }

require_tools(){ command -v jq >/dev/null || fatal "jq is required"; command -v curl >/dev/null || fatal "curl is required"; }

load_manifest(){
  [[ -f "$TOPOLOGY_MANIFEST" ]] || fatal "manifest not found: $TOPOLOGY_MANIFEST"
  jq empty "$TOPOLOGY_MANIFEST" >/dev/null
  RUN_ID="$(jq -r '.run_id' "$TOPOLOGY_MANIFEST")"
  [[ -n "$RUN_ID" && "$RUN_ID" != "null" ]] || fatal "run_id missing"
  EVIDENCE_DIR="$(jq -r '.evidence_directory' "$TOPOLOGY_MANIFEST")"
  [[ -n "$EVIDENCE_DIR" && "$EVIDENCE_DIR" != "null" ]] || EVIDENCE_DIR="$STATE_DIR/evidence/$RUN_ID"
  [[ "$EVIDENCE_DIR" = /* ]] || EVIDENCE_DIR="$ROOT_DIR/${EVIDENCE_DIR#./}"
  TIMELINE_FILE="$EVIDENCE_DIR/timeline.md"
  SNAPSHOT_DIR="$EVIDENCE_DIR/process_snapshots"
  NODE_PID_DIR="$STATE_DIR/$RUN_ID/nodes"
  MINER_PID_DIR="$STATE_DIR/$RUN_ID/miners"
  NODE_LOG_DIR="$EVIDENCE_DIR/logs/nodes"
  MINER_LOG_DIR="$EVIDENCE_DIR/logs/miners"
}

ensure_layout(){
  mkdir -p "$EVIDENCE_DIR" "$SNAPSHOT_DIR" "$NODE_PID_DIR" "$MINER_PID_DIR" "$NODE_LOG_DIR" "$MINER_LOG_DIR"
  if [[ ! -f "$TIMELINE_FILE" ]]; then
    {
      echo "# Supervisor Timeline ($RUN_ID)"
      echo
      echo "- rehearsal_mode: ${REHEARSAL_MODE}"
    } > "$TIMELINE_FILE"
  fi
}

log_event(){
  local event="$1" subject="$2" details="${3:-}"
  printf -- "- %s | %s | %s | %s\n" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$event" "$subject" "$details" >> "$TIMELINE_FILE"
}

node_pid_file(){ echo "$NODE_PID_DIR/$1.pid"; }
miner_pid_file(){ echo "$MINER_PID_DIR/$1.pid"; }

is_running(){ [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null; }

capture_process_snapshot(){
  local tag="${1:-manual}" ts file
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  file="$SNAPSHOT_DIR/process-table-${tag}-${ts}.txt"
  {
    echo "snapshot_utc=$ts"
    echo "tag=$tag"
    echo "rehearsal_mode=$REHEARSAL_MODE"
    ps -eo pid,ppid,etime,stat,comm,args
  } > "$file"
  log_event "evidence_collected" "process_table" "snapshot=$file"
}

node_rpc_url(){ jq -r --arg id "$1" '.nodes[] | select(.id==$id) | "http://" + .rpc.bind + ":" + (.rpc.port|tostring)' "$TOPOLOGY_MANIFEST"; }

start_node(){
  local id="$1" rpc_url pid_file log_file data_dir p2p_bind rpc_bind bootnode
  pid_file="$(node_pid_file "$id")"; log_file="$NODE_LOG_DIR/$id.log"
  if [[ -f "$pid_file" ]] && is_running "$(cat "$pid_file")"; then info "node $id already running"; return 0; fi
  rpc_bind="$(jq -r --arg id "$id" '.nodes[]|select(.id==$id)|.rpc.bind+":"+(.rpc.port|tostring)' "$TOPOLOGY_MANIFEST")"
  p2p_bind="$(jq -r --arg id "$id" '.nodes[]|select(.id==$id)|.p2p.bind+":"+(.p2p.port|tostring)' "$TOPOLOGY_MANIFEST")"
  data_dir="$(jq -r --arg id "$id" '.nodes[]|select(.id==$id)|.data_directory' "$TOPOLOGY_MANIFEST")"
  [[ "$data_dir" = /* ]] || data_dir="$ROOT_DIR/${data_dir#./}"
  mkdir -p "$data_dir"
  bootnode="$(jq -r --arg id "$id" '.nodes[]|select(.id==$id)|.bootstrap_peers[0] // empty' "$TOPOLOGY_MANIFEST")"
  cmd=("$NODE_BIN" --rpc-bind "$rpc_bind" --p2p-bind "$p2p_bind" --data-dir "$data_dir")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  "${cmd[@]}" >"$log_file" 2>&1 &
  echo "$!" > "$pid_file"
  log_event "node_started" "$id" "pid=$(cat "$pid_file") rpc=$rpc_bind"
}

start_miner(){
  local id="$1" pid_file log_file target url
  pid_file="$(miner_pid_file "$id")"; log_file="$MINER_LOG_DIR/$id.log"
  if [[ -f "$pid_file" ]] && is_running "$(cat "$pid_file")"; then info "miner $id already running"; return 0; fi
  target="$(jq -r --arg id "$id" '.miners[]|select(.id==$id)|.target_node' "$TOPOLOGY_MANIFEST")"
  url="$(node_rpc_url "$target")"
  local run_id_safe="${RUN_ID//[^a-zA-Z0-9_-]/-}"
  local miner_address="${MINER_ADDRESS_OVERRIDE:-${run_id_safe}-${id}}"
  "$MINER_BIN" --node "$url" --miner-address "$miner_address" --backend cpu --loop >"$log_file" 2>&1 &
  echo "$!" > "$pid_file"
  log_event "miner_started" "$id" "pid=$(cat "$pid_file") target=$target miner_address=$miner_address"
}

stop_one(){
  local kind="$1" id="$2" pid_file event
  if [[ "$kind" == "node" ]]; then pid_file="$(node_pid_file "$id")"; event="node_stopped"; else pid_file="$(miner_pid_file "$id")"; event="miner_stopped"; fi
  [[ -f "$pid_file" ]] || return 0
  local pid; pid="$(cat "$pid_file")"
  if is_running "$pid"; then kill "$pid" 2>/dev/null || true; sleep 1; is_running "$pid" && kill -9 "$pid" 2>/dev/null || true; fi
  rm -f "$pid_file"
  log_event "$event" "$id" "pid=$pid"
}

restart_node(){ local id="$1"; stop_one node "$id"; start_node "$id"; log_event "node_restarted" "$id" "ok"; }
restart_miner(){ local id="$1"; stop_one miner "$id"; start_miner "$id"; log_event "miner_restarted" "$id" "ok"; }

start_all(){
  [[ -x "$NODE_BIN" ]] || fatal "missing node binary: $NODE_BIN"
  [[ -x "$MINER_BIN" ]] || fatal "missing miner binary: $MINER_BIN"
  jq -r '.nodes[].id' "$TOPOLOGY_MANIFEST" | while read -r id; do start_node "$id"; done
  jq -r '.miners[].id' "$TOPOLOGY_MANIFEST" | while read -r id; do start_miner "$id"; done
  capture_process_snapshot "start"
}

stop_all(){
  jq -r '.miners[].id' "$TOPOLOGY_MANIFEST" | while read -r id; do stop_one miner "$id"; done
  jq -r '.nodes[].id' "$TOPOLOGY_MANIFEST" | while read -r id; do stop_one node "$id"; done
  capture_process_snapshot "stop"
}

health_check_once(){
  local id url
  jq -r '.nodes[].id' "$TOPOLOGY_MANIFEST" | while read -r id; do
    url="$(node_rpc_url "$id")/status"
    if curl --silent --show-error --fail --max-time "$HEALTH_TIMEOUT_SECS" "$url" >/dev/null; then
      log_event "health_check_pass" "$id" "$url"
    else
      log_event "health_check_fail" "$id" "$url"
    fi
  done
}

monitor_health(){ while true; do health_check_once; sleep "$HEALTH_INTERVAL_SECS"; done; }

main(){
  require_tools
  load_manifest
  ensure_layout
  case "${1:-}" in
    start) start_all ;;
    stop) stop_all ;;
    health-once) health_check_once ;;
    monitor-health) monitor_health ;;
    restart-node) [[ -n "${2:-}" ]] || fatal "usage: restart-node <node-id>"; restart_node "$2" ;;
    restart-miner) [[ -n "${2:-}" ]] || fatal "usage: restart-miner <miner-id>"; restart_miner "$2" ;;
    snapshot) capture_process_snapshot "manual" ;;
    *) echo "usage: $0 {start|stop|health-once|monitor-health|restart-node <id>|restart-miner <id>|snapshot}"; exit 1 ;;
  esac
}

main "$@"
