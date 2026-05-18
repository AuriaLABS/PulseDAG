#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${PULSEDAG_REHEARSAL_STATE_DIR:-$ROOT_DIR/.pulsedag-v2_2_16-rehearsal}"
EVIDENCE_ROOT="${PULSEDAG_REHEARSAL_EVIDENCE_DIR:-$STATE_DIR/evidence}"
RUN_ID="${PULSEDAG_REHEARSAL_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
NODE_COUNT="${PULSEDAG_REHEARSAL_NODE_COUNT:-3}"
DURATION_SECS="${PULSEDAG_REHEARSAL_DURATION_SECS:-30}"
POLL_INTERVAL_SECS="${PULSEDAG_REHEARSAL_POLL_INTERVAL_SECS:-5}"
CURL_TIMEOUT_SECS="${PULSEDAG_REHEARSAL_CURL_TIMEOUT_SECS:-5}"
KEEP_RUNNING="${PULSEDAG_REHEARSAL_KEEP_RUNNING:-0}"
ASSUME_NODES="${PULSEDAG_REHEARSAL_ASSUME_NODES:-0}"

NODE_NAMES=(a b c)
ENDPOINTS=(status mining/workers/stats pow/metrics metrics sync/status)

info() { echo "[info] $*"; }
warn() { echo "[warn] $*" >&2; }
fatal() { echo "[error] $*" >&2; exit 1; }

node_rpc() {
  case "$1" in
    a) echo "${PULSEDAG_NODE_A_RPC:-127.0.0.1:18080}" ;;
    b) echo "${PULSEDAG_NODE_B_RPC:-127.0.0.1:18081}" ;;
    c) echo "${PULSEDAG_NODE_C_RPC:-127.0.0.1:18082}" ;;
    *) fatal "unknown node: $1" ;;
  esac
}
node_url() { echo "http://$(node_rpc "$1")"; }
http_get() { curl --silent --show-error --fail --max-time "$CURL_TIMEOUT_SECS" "$1"; }

pretty_json_file() {
  local src="$1" dst="$2"
  if command -v python3 >/dev/null; then
    python3 -m json.tool "$src" > "$dst" 2>/dev/null || cp "$src" "$dst"
  else
    cp "$src" "$dst"
  fi
}

collect_endpoint() {
  local node="$1" endpoint="$2" phase="$3" dir file url raw err
  dir="$RUN_DIR/node-$node/$phase"
  mkdir -p "$dir"
  file="${endpoint//\//-}.json"
  url="$(node_url "$node")/$endpoint"
  raw="$dir/$file.raw"
  err="$dir/$file.err"
  if http_get "$url" > "$raw" 2> "$err"; then
    pretty_json_file "$raw" "$dir/$file"
    rm -f "$raw" "$err"
  else
    mv "$raw" "$dir/$file.failed" 2>/dev/null || true
  fi
}

collect_phase() {
  local phase="$1" node endpoint count
  count="$NODE_COUNT"
  for ((i=0; i<count; i++)); do
    node="${NODE_NAMES[$i]}"
    for endpoint in "${ENDPOINTS[@]}"; do
      collect_endpoint "$node" "$endpoint" "$phase"
    done
  done
}

copy_logs() {
  local dest="$RUN_DIR/logs"
  mkdir -p "$dest"
  cp -a "$STATE_DIR"/*.env "$dest"/ 2>/dev/null || true
  cp -a "$STATE_DIR"/logs/*.log "$dest"/ 2>/dev/null || true
}

write_config() {
  mkdir -p "$RUN_DIR"
  cat > "$RUN_DIR/rehearsal-config.json" <<JSON
{
  "scenario": "v2.2.16 multi-miner local rehearsal",
  "run_id": "$RUN_ID",
  "node_count": $NODE_COUNT,
  "cpu_miners": ${PULSEDAG_REHEARSAL_CPU_MINERS:-2},
  "gpu_miners": ${PULSEDAG_REHEARSAL_GPU_MINERS:-0},
  "miner_targets": "${PULSEDAG_REHEARSAL_MINER_TARGETS:-a}",
  "duration_secs": $DURATION_SECS,
  "poll_interval_secs": $POLL_INTERVAL_SECS,
  "assume_nodes": "$ASSUME_NODES",
  "rpc_exposure": "loopback only by default"
}
JSON
}

write_summary() {
  local summary="$RUN_DIR/summary.txt" miners_running=0 pid_file pid
  shopt -s nullglob
  for pid_file in "$STATE_DIR"/miner-*.pid; do
    pid="$(cat "$pid_file")"
    if kill -0 "$pid" 2>/dev/null; then
      miners_running=$((miners_running + 1))
    fi
  done
  {
    echo "PulseDAG v2.2.16 multi-miner local rehearsal"
    echo "run_id=$RUN_ID"
    echo "evidence_dir=$RUN_DIR"
    echo "node_count=$NODE_COUNT"
    echo "miners_running_at_summary=$miners_running"
    echo "polled_endpoints=${ENDPOINTS[*]}"
    echo "notes=no pool logic, no central coordinator, no payouts/accounting; RPC binds remain loopback by default"
  } > "$summary"
}

cleanup() {
  copy_logs || true
  if [[ "$KEEP_RUNNING" == "1" || "$KEEP_RUNNING" == "true" ]]; then
    info "leaving miners/nodes running because PULSEDAG_REHEARSAL_KEEP_RUNNING=$KEEP_RUNNING"
    info "stop miners later with: bash scripts/v2_2_16_stop_miners.sh"
  else
    if [[ "$ASSUME_NODES" == "1" || "$ASSUME_NODES" == "true" ]]; then
      PULSEDAG_STOP_REHEARSAL_NODES=0 bash "$ROOT_DIR/scripts/v2_2_16_stop_miners.sh" || true
    else
      PULSEDAG_STOP_REHEARSAL_NODES=1 bash "$ROOT_DIR/scripts/v2_2_16_stop_miners.sh" || true
    fi
  fi
}
trap cleanup EXIT

main() {
  command -v curl >/dev/null || fatal "curl is required"
  [[ "$NODE_COUNT" =~ ^[0-9]+$ ]] || fatal "PULSEDAG_REHEARSAL_NODE_COUNT must be numeric"
  (( NODE_COUNT >= 1 && NODE_COUNT <= 3 )) || fatal "PULSEDAG_REHEARSAL_NODE_COUNT must be between 1 and 3"
  mkdir -p "$RUN_DIR"
  write_config
  info "starting v2.2.16 multi-miner rehearsal"
  PULSEDAG_REHEARSAL_EVIDENCE_DIR="$EVIDENCE_ROOT" bash "$ROOT_DIR/scripts/v2_2_16_start_miners.sh"

  collect_phase "initial"
  local deadline=$((SECONDS + DURATION_SECS)) sample=0
  while (( SECONDS < deadline )); do
    sample=$((sample + 1))
    collect_phase "poll-$sample"
    sleep "$POLL_INTERVAL_SECS"
  done
  collect_phase "final"
  write_summary
  copy_logs
  info "evidence written to $RUN_DIR"
}

main "$@"
