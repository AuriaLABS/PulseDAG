#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-1800}
P2P_CONNECT_WAIT_SECS=${P2P_CONNECT_WAIT_SECS:-120}
CURL_CONNECT_TIMEOUT_SECS=${CURL_CONNECT_TIMEOUT_SECS:-3}
CURL_MAX_TIME_SECS=${CURL_MAX_TIME_SECS:-10}
QUIESCENCE_WAIT_SECS=${QUIESCENCE_WAIT_SECS:-90}
GLOBAL_DEADLINE_SECS=${GLOBAL_DEADLINE_SECS:-21600}
MAX_GLOBAL_DEADLINE_SECS=21600
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
START_TS=$(date +%s)
START_UTC=$(date -u +%FT%TZ)
if (( GLOBAL_DEADLINE_SECS > MAX_GLOBAL_DEADLINE_SECS )); then
  GLOBAL_DEADLINE_SECS=$MAX_GLOBAL_DEADLINE_SECS
fi
GLOBAL_DEADLINE_TS=$((START_TS + GLOBAL_DEADLINE_SECS))
if (( DURATION_SECS >= GLOBAL_DEADLINE_SECS )); then
  DURATION_SECS=$((GLOBAL_DEADLINE_SECS > 600 ? GLOBAL_DEADLINE_SECS - 600 : GLOBAL_DEADLINE_SECS / 2))
fi
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
NODE_COUNT=5
MINER_COUNT=${MINER_COUNT:-4}
NETWORK_PROFILE="private"
CHAIN_ID_EXPECTED="pulsedag-private"
BASE_RPC_PORT=${BASE_RPC_PORT:-28544}
BASE_P2P_PORT=${BASE_P2P_PORT:-32302}

case "$MINER_COUNT" in
  1) STAGE_NAME=${STAGE_NAME:-"5N/1M baseline"}; DEFAULT_OUT="private_5n_1m_rehearsal" ;;
  2) STAGE_NAME=${STAGE_NAME:-"5N/2M intermediate"}; DEFAULT_OUT="private_5n_2m_rehearsal" ;;
  4) STAGE_NAME=${STAGE_NAME:-"5N/4M stress"}; DEFAULT_OUT="private_5n_4m_rehearsal" ;;
  *) echo "FATAL: MINER_COUNT must be 1, 2, or 4 for staged v2.2.19 convergence gates" >&2; exit 2 ;;
esac

OUT_DIR_BASE="${OUT_DIR:-$ROOT_DIR/artifacts/v2_2_19/$DEFAULT_OUT}"
RUN_DIR="$OUT_DIR_BASE/$RUN_ID"
OUT_DIR_ROOT="$OUT_DIR_BASE"
OUT_DIR="$RUN_DIR"

mkdir -p "$OUT_DIR_ROOT" "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs" "$OUT_DIR/miners" "$OUT_DIR/nodes" "$OUT_DIR/samples" "$OUT_DIR/summaries"
printf "%s\n" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt"
printf "%s\n" "$OUT_DIR" > "$OUT_DIR/current-run-dir.txt"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

declare -a NODE_PIDS=()
declare -a MINER_PIDS=()
declare -A NODE_READY NODE_HEALTHY NODE_ADVANCED NODE_TIP NODE_HEIGHT NODE_P2P_OK NODE_PEERS NODE_P2P_INBOUND NODE_P2P_OUTBOUND NODE_CHAIN_ID
declare -A NODE_ORPHAN_COUNT NODE_PENDING_MISSING_PARENTS NODE_INV_HASHES_REQUESTED NODE_PEER_RECOVERY_SUCCESS_COUNT NODE_MISSING_PARENTS_COUNT
declare -A NODE_READINESS_SCHEMA_OK NODE_SYNC_STATE NODE_SYNC_STAGE
declare -A PRE_NODE_HEIGHT PRE_NODE_TIP PRE_NODE_ORPHANS PRE_NODE_MISSING_PARENTS PRE_NODE_SYNC_STATE PRE_NODE_PEERS
FAIL_REASONS=()
FAIL_CLASSES=()
WARNINGS=()
RESULT="PENDING"
EXIT_CODE=0
WAIVE_ACCEPTED_BLOCK_GATE=${WAIVE_ACCEPTED_BLOCK_GATE:-0}
WAIVE_ACCEPTED_BLOCK_REASON=${WAIVE_ACCEPTED_BLOCK_REASON:-""}
ACCEPTED_BLOCKS=0
REJECTED_BLOCKS=0
TEMPLATES_OK=0
MINERS_STOPPED_FOR_QUIESCENCE=0
GATE_5N_1M_BASELINE=FAIL
GATE_5N_2M_INTERMEDIATE=FAIL
GATE_5N_4M_STRESS=OBSERVE
IN_CLEANUP=0
CLEANUP_STARTED=0
QUIESCENCE_COMPLETED=0
PRE_WORST_LAG=0
PRE_DISTINCT_TIPS=0
PRE_CONVERGED=0
POST_WORST_LAG=0
POST_DISTINCT_TIPS=0
POST_CONVERGED=0
LAG_IMPROVED=0
declare -A miner_submit miner_accept miner_reject miner_template
for i in 1 2 3 4; do miner_submit[$i]=0; miner_accept[$i]=0; miner_reject[$i]=0; miner_template[$i]=0; done
REPO_COMMIT="$(git -C "$ROOT_DIR" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
NODE_VERSION="$({ "$NODE_BIN" --version 2>/dev/null || true; } | head -n1)"
NODE_VERSION=${NODE_VERSION:-unknown}
for i in $(seq 1 "$MINER_COUNT"); do miner_submit[$i]=0; miner_accept[$i]=0; miner_reject[$i]=0; miner_template[$i]=0; done

text_has_match(){
  local pattern="$1" file="$2"
  [[ -f "$file" ]] || return 1
  if command -v rg >/dev/null 2>&1; then rg -qi -- "$pattern" "$file"; else grep -Eqi -- "$pattern" "$file"; fi
}

count_matches_file(){
  local pattern="$1" file="$2"
  [[ -f "$file" ]] || { echo 0; return 0; }
  if command -v rg >/dev/null 2>&1; then rg -ci -- "$pattern" "$file" 2>/dev/null || echo 0; else grep -Eic -- "$pattern" "$file" 2>/dev/null || echo 0; fi
}

count_matches_in_logs(){
  local pattern="$1" total=0 c i
  for i in $(seq 1 "$MINER_COUNT"); do
    c=$(count_matches_file "$pattern" "$OUT_DIR/logs/miner-${i}.log")
    total=$((total + c))
  done
  echo "$total"
}

miner_log_has_accepted(){
  local file="$1"
  [[ -f "$file" ]] || return 1
  awk '{ line=tolower($0); if (line ~ /submit_result accepted=false|reject|rejected/) next; if (line ~ /submit_result accepted=true|submit_accepted|accepted/) found=1 } END { exit(found ? 0 : 1) }' "$file" 2>/dev/null
}

count_accepted_file(){
  local file="$1"
  [[ -f "$file" ]] || { echo 0; return 0; }
  awk '{ line=tolower($0); if (line ~ /submit_result accepted=false|reject|rejected/) next; if (line ~ /submit_result accepted=true|submit_accepted|accepted/) count++ } END { print count + 0 }' "$file" 2>/dev/null || echo 0
}

count_rejected_file(){
  local file="$1"
  [[ -f "$file" ]] || { echo 0; return 0; }
  awk '{ line=tolower($0); if (line ~ /submit_result accepted=false|reject|rejected/) count++ } END { print count + 0 }' "$file" 2>/dev/null || echo 0
}

collect_miner_metrics(){
  local i log accepted_total=0 rejected_total=0 c
  for i in $(seq 1 "$MINER_COUNT"); do
    log="$OUT_DIR/logs/miner-${i}.log"
    text_has_match "template_received|template" "$log" && miner_template[$i]=1 || true
    text_has_match "submit_result|submit_accepted|submit" "$log" && miner_submit[$i]=1 || true
    miner_log_has_accepted "$log" && miner_accept[$i]=1 || true
    text_has_match "submit_result accepted=false|[Rr]eject|[Rr]ejected" "$log" && miner_reject[$i]=1 || true
    c=$(count_accepted_file "$log")
    accepted_total=$((accepted_total + c))
    c=$(count_rejected_file "$log")
    rejected_total=$((rejected_total + c))
  done
  ACCEPTED_BLOCKS=$accepted_total
  REJECTED_BLOCKS=$rejected_total
  TEMPLATES_OK=0
  for i in $(seq 1 "$MINER_COUNT"); do
    if (( ${miner_template[$i]:-0} == 1 )); then
      TEMPLATES_OK=1
      break
    fi
  done
  return 0
}

record_warn(){ local msg="$1"; echo "WARN: $msg"; WARNINGS+=("$msg"); }
record_fail(){
  local class="$1" msg="$2"
  echo "FAIL[$class]: $msg"
  FAIL_CLASSES+=("$class")
  FAIL_REASONS+=("$class: $msg")
}

assert_global_deadline(){
  (( ${IN_CLEANUP:-0} == 1 )) && return 0
  if (( $(date +%s) >= GLOBAL_DEADLINE_TS )); then
    echo "FATAL: global deadline exceeded after ${GLOBAL_DEADLINE_SECS}s"
    record_fail "GLOBAL_DEADLINE_TIMEOUT" "global deadline exceeded after ${GLOBAL_DEADLINE_SECS}s"
    exit 124
  fi
}

sleep_with_deadline(){
  local requested now remaining
  requested="$1"
  if (( ${IN_CLEANUP:-0} == 1 )); then
    sleep "$requested"
    return 0
  fi
  while (( requested > 0 )); do
    assert_global_deadline
    now=$(date +%s)
    remaining=$((GLOBAL_DEADLINE_TS - now))
    (( remaining > 0 )) || { echo "FATAL: global deadline exhausted before sleep"; record_fail "GLOBAL_DEADLINE_TIMEOUT" "global deadline exhausted before sleep"; exit 124; }
    if (( requested < remaining )); then
      sleep "$requested"
      return 0
    fi
    sleep "$remaining"
    requested=$((requested - remaining))
  done
}

start_global_deadline_watchdog(){
  (
    sleep "$GLOBAL_DEADLINE_SECS"
    echo "FATAL: private rehearsal global deadline ${GLOBAL_DEADLINE_SECS}s reached; terminating script" >&2
    {
      echo "timestamp_utc=$(date -u +%FT%TZ)"
      echo "deadline_seconds=$GLOBAL_DEADLINE_SECS"
    } > "$OUT_DIR/global-watchdog-timeout.txt" 2>/dev/null || true
    kill -TERM $$ 2>/dev/null || true
  ) &
  DEADLINE_WATCHDOG_PID=$!
}

stop_global_deadline_watchdog(){
  if [[ -n "${DEADLINE_WATCHDOG_PID:-}" ]]; then
    kill "$DEADLINE_WATCHDOG_PID" 2>/dev/null || true
    wait "$DEADLINE_WATCHDOG_PID" 2>/dev/null || true
    DEADLINE_WATCHDOG_PID=""
  fi
}

write_curl_failure_stub(){
  local out="$1" url="$2" label="$3" rc="$4" error="$5"
  mkdir -p "$(dirname "$out")" 2>/dev/null || true
  if command -v jq >/dev/null 2>&1; then
    jq -n \
      --arg url "$url" \
      --arg label "$label" \
      --arg error "$error" \
      --argjson exit_code "$rc" \
      '{ok:false,error:$error,label:$label,url:$url,exit_code:$exit_code}' \
      > "$out" 2>/dev/null || true
  fi
  if [[ ! -s "$out" ]]; then
    printf '{"ok":false,"error":"%s","label":"%s","url":"%s","exit_code":%s}\n' \
      "$error" "$label" "$url" "$rc" > "$out" 2>/dev/null || true
  fi
}

safe_curl_json(){
  local url out label required rc now remaining max_time
  url="$1"; out="$2"; label="${3:-$url}"; required="${4:-0}"
  now=$(date +%s)
  if (( ${IN_CLEANUP:-0} != 1 )); then
    assert_global_deadline
    remaining=$((GLOBAL_DEADLINE_TS - now))
    (( remaining > 0 )) || { echo "FATAL: global deadline exhausted before curl: $label"; record_fail "GLOBAL_DEADLINE_TIMEOUT" "global deadline exhausted before curl: $label"; exit 124; }
  else
    remaining=$((GLOBAL_DEADLINE_TS - now))
    if (( remaining <= 0 )); then
      write_curl_failure_stub "$out" "$url" "$label" 124 "global deadline exhausted; skipped curl during cleanup"
      record_warn "skipped endpoint capture during cleanup after deadline: $label"
      return 1
    fi
  fi
  max_time=$CURL_MAX_TIME_SECS
  (( max_time > remaining )) && max_time=$remaining
  (( max_time < 1 )) && max_time=1
  rc=0
  curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT_SECS" --max-time "$max_time" "$url" -o "$out" || rc=$?
  if (( rc == 0 )); then
    return 0
  fi
  write_curl_failure_stub "$out" "$url" "$label" "$rc" "curl failed"
  capture_rpc_failure_diagnostics "$label" "$url" "$rc" || true
  if (( required == 1 )); then record_fail "RPC_UNAVAILABLE" "required endpoint failed: $label ($url)"; else record_warn "optional endpoint failed: $label"; fi
  return 1
}
safe_curl_required(){ safe_curl_json "$1" "$2" "${3:-$1}" 1; }
safe_curl_optional(){ safe_curl_json "$1" "$2" "${3:-$1}" 0; }
json_get_or_default(){ local expr="$1" file="$2" def="$3"; jq -r "$expr // $def" "$file" 2>/dev/null || echo "$def"; }

extract_chain_id(){
  local status_file="$1" release_file="$2" p2p_file="$3"
  jq -r '.data.chain_id // .chain_id // empty' "$status_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' && return 0
  jq -r '.data.chain_id // .chain_id // .data.network_id // .network_id // empty' "$release_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' && return 0
  jq -r '.data.chain_id // .chain_id // empty' "$p2p_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' && return 0
  return 1
}

node_pid_for_label(){
  local label="$1" node
  node="$(printf '%s' "$label" | sed -n 's/.*\(n[0-9][0-9]*\):.*/\1/p' | head -n1)"
  [[ -n "$node" && -f "$OUT_DIR/process-pids.txt" ]] || return 1
  awk -v node="node-${node}" '$2 == node {print $1; exit}' "$OUT_DIR/process-pids.txt"
}

capture_rpc_failure_diagnostics(){
  local label="$1" url="$2" rc="$3" pid port node diag class alive listening
  node="$(printf '%s' "$label" | sed -n 's/.*\(n[0-9][0-9]*\):.*/\1/p' | head -n1)"
  port="$(printf '%s' "$url" | sed -n 's#.*127\.0\.0\.1:\([0-9][0-9]*\).*#\1#p' | head -n1)"
  if [[ -z "$node" && -n "$port" ]]; then
    local idx=$((port - BASE_RPC_PORT))
    if (( idx >= 1 && idx <= NODE_COUNT )); then node="n${idx}"; fi
  fi
  [[ -n "$node" ]] || node="unknown"
  pid="$(node_pid_for_label "$node" 2>/dev/null || true)"
  diag="$OUT_DIR/endpoints/${node}-rpc-failure-diagnostics.jsonl"
  alive=0; listening=0; class="RPC_TIMEOUT_UNCLASSIFIED"
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then alive=1; fi
  if [[ -n "$port" ]] && port_in_use "$port"; then listening=1; fi
  if (( alive == 0 )); then
    class="RPC_PROCESS_EXITED"
  elif (( listening == 0 )); then
    class="RPC_LISTENER_DOWN"
  elif [[ "$rc" == "28" ]]; then
    class="RPC_ALIVE_LISTENER_TIMEOUT"
  else
    class="RPC_CURL_FAILURE_WITH_ALIVE_LISTENER"
  fi
  {
    jq -n \
      --arg ts "$(date -u +%FT%TZ)" \
      --arg label "$label" \
      --arg url "$url" \
      --arg node "$node" \
      --arg port "${port:-unknown}" \
      --arg pid "${pid:-unknown}" \
      --arg class "$class" \
      --argjson exit_code "$rc" \
      --argjson process_alive "$alive" \
      --argjson listener_present "$listening" \
      '{timestamp_utc:$ts,label:$label,url:$url,node:$node,port:$port,pid:$pid,exit_code:$exit_code,class:$class,process_alive:$process_alive,listener_present:$listener_present}'
  } >> "$diag" 2>/dev/null || true
  if [[ -n "$pid" ]]; then ps -p "$pid" -o pid,ppid,stat,etime,pcpu,pmem,comm > "$OUT_DIR/endpoints/${node}-rpc-failure-ps.txt" 2>/dev/null || true; fi
  if [[ -n "$port" ]] && command -v ss >/dev/null 2>&1; then ss -ltnp "( sport = :$port )" > "$OUT_DIR/endpoints/${node}-rpc-failure-listener.txt" 2>/dev/null || true; fi
  if [[ "$node" != "unknown" ]]; then tail -n 200 "$OUT_DIR/logs/${node}.log" > "$OUT_DIR/logs/${node}-rpc-failure-tail.log" 2>/dev/null || true; fi
  echo "RPC_DIAGNOSTIC[$class]: label=$label node=$node pid=${pid:-unknown} alive=$alive listener=$listening curl_exit=$rc"
}

port_in_use(){
  local p="$1"
  if command -v ss >/dev/null 2>&1; then ss -ltn "( sport = :$p )" | grep -Eq ":$p\b"; return $?; fi
  if command -v lsof >/dev/null 2>&1; then lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; return $?; fi
  if command -v netstat >/dev/null 2>&1; then netstat -ltn 2>/dev/null | grep -Eq "[:.]$p[[:space:]]"; return $?; fi
  (exec 3<>"/dev/tcp/127.0.0.1/$p") >/dev/null 2>&1 && { exec 3<&- 3>&-; return 0; }
  return 1
}

ensure_ports_free(){
  local -a ports=() p
  for i in $(seq 1 "$NODE_COUNT"); do ports+=("$((BASE_RPC_PORT+i))" "$((BASE_P2P_PORT+i))"); done
  for p in "${ports[@]}"; do
    if port_in_use "$p"; then
      echo "FATAL: port $p is already in use"
      command -v ss >/dev/null 2>&1 && ss -ltnp "( sport = :$p )" || true
      record_fail "HARNESS_TIMEOUT" "port $p is already in use before rehearsal"
      exit 1
    fi
  done
}

stop_pids(){
  local p
  (( $# == 0 )) && return 0
  for p in "$@"; do [[ -n "$p" ]] && kill "$p" 2>/dev/null || true; done
  sleep_with_deadline 1
  for p in "$@"; do [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null || true; done
}
capture_p2p_gate_failure(){
  local i rpc ep f
  for i in $(seq 1 "$NODE_COUNT"); do
    rpc=$((BASE_RPC_PORT+i))
    for ep in health status readiness p2p/status sync/status sync/missing orphans; do
      safe_curl_optional "http://127.0.0.1:${rpc}/${ep}" "$OUT_DIR/endpoints/n${i}-${ep//\//_}.json" "n${i}:/${ep}" || true
    done
  done
  {
    echo "# p2p gate failure diagnostics"
    echo "- bootnode: $(cat "$OUT_DIR/bootnode.txt" 2>/dev/null || echo unknown)"
    for i in $(seq 1 "$NODE_COUNT"); do
      f="$OUT_DIR/endpoints/n${i}-p2p_status-p2p-gate.json"
      echo "- n${i}.peer_count: $(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$f" 2>/dev/null || echo 0)"
      echo "- n${i}.connected_peers: $(jq -c '.data.connected_peers // .data.connected_peer_ids // []' "$f" 2>/dev/null || echo '[]')"
    done
  } > "$OUT_DIR/p2p-gate-failure.md"
}

capture_log_tails(){
  local i
  for i in $(seq 1 "$NODE_COUNT"); do tail -n 120 "$OUT_DIR/logs/n${i}.log" > "$OUT_DIR/logs/n${i}-tail.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do tail -n 120 "$OUT_DIR/logs/miner-${i}.log" > "$OUT_DIR/logs/miner-${i}-tail.log" 2>/dev/null || true; done
}

collect_final_state(){
  local phase
  phase="${1:-final}"
  for i in $(seq 1 "$NODE_COUNT"); do
    rpc=$((BASE_RPC_PORT+i))
    safe_curl_optional "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${i}-status-final.json" "n${i}:/status final" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/release" "$OUT_DIR/endpoints/n${i}-release-final.json" "n${i}:/release final" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/readiness" "$OUT_DIR/endpoints/n${i}-readiness-final.json" "n${i}:/readiness final" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "n${i}:/p2p/status ${phase}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/sync/status" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "n${i}:/sync/status ${phase}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/sync/missing" "$OUT_DIR/endpoints/n${i}-sync-missing-final.json" "n${i}:/sync/missing ${phase}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/orphans" "$OUT_DIR/endpoints/n${i}-orphans-final.json" "n${i}:/orphans ${phase}" || true
    NODE_HEIGHT[$i]="$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo 0)"
    NODE_TIP[$i]="$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo '')"
    NODE_READY[$i]="$(jq -r '.data.ready_for_release // .ready_for_release // 0' "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_HEALTHY[$i]="$(jq -r '.ok // .data.ok // 0' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_PEERS[$i]="$(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // (.data.peers|length) // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_INBOUND[$i]="$(jq -r '.data.inbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_OUTBOUND[$i]="$(jq -r '.data.outbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_CHAIN_ID[$i]="$(extract_chain_id "$OUT_DIR/endpoints/n${i}-status-final.json" "$OUT_DIR/endpoints/n${i}-release-final.json" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" || true)"
    NODE_ORPHAN_COUNT[$i]="$(jq -r '.data.orphan_count // .orphan_count // (.data.orphans|length) // (.orphans|length) // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "$OUT_DIR/endpoints/n${i}-orphans-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_PENDING_MISSING_PARENTS[$i]="$(jq -r '.data.pending_missing_parents // .pending_missing_parents // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo 0)"
    NODE_INV_HASHES_REQUESTED[$i]="$(jq -r '.data.inv_hashes_requested // .inv_hashes_requested // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_PEER_RECOVERY_SUCCESS_COUNT[$i]="$(jq -r '.data.peer_recovery_success_count // .peer_recovery_success_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_MISSING_PARENTS_COUNT[$i]="$(jq -r '[.. | objects | .missing_parents? // empty | if type == "array" then length else 1 end] | add // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "$OUT_DIR/endpoints/n${i}-sync-missing-final.json" "$OUT_DIR/endpoints/n${i}-orphans-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_ORPHANS[$i]="${NODE_ORPHAN_COUNT[$i]:-0}"
    NODE_MISSING_PARENTS[$i]="${NODE_PENDING_MISSING_PARENTS[$i]:-0}"
    NODE_SYNC_STATE[$i]="$(jq -r '.data.sync_state // .sync_state // .data.state // "unknown"' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo unknown)"
    NODE_SYNC_STAGE[$i]="$(jq -r '.data.catchup_stage // .catchup_stage // .data.stage // "unknown"' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo unknown)"
    NODE_P2P_OK[$i]=$(( NODE_PEERS[$i] > 0 ? 1 : 0 ))
    readiness_has_ready=$(jq -e '(.data.ready_for_release? // .ready_for_release?) | type == "boolean"' "$OUT_DIR/endpoints/n${i}-readiness-final.json" >/dev/null 2>&1 && echo 1 || echo 0)
    readiness_has_public=$(jq -e '(.data.public_testnet_ready? // .public_testnet_ready?) == false' "$OUT_DIR/endpoints/n${i}-readiness-final.json" >/dev/null 2>&1 && echo 1 || echo 0)
    NODE_READINESS_SCHEMA_OK[$i]=$(( readiness_has_ready == 1 && readiness_has_public == 1 ? 1 : 0 ))
  done
  collect_miner_metrics
}

snapshot_current_as_pre(){
  local i
  for i in $(seq 1 "$NODE_COUNT"); do
    PRE_NODE_HEIGHT[$i]="${NODE_HEIGHT[$i]:-0}"
    PRE_NODE_TIP[$i]="${NODE_TIP[$i]:-}"
    PRE_NODE_ORPHANS[$i]="${NODE_ORPHANS[$i]:-0}"
    PRE_NODE_MISSING_PARENTS[$i]="${NODE_MISSING_PARENTS[$i]:-0}"
    PRE_NODE_SYNC_STATE[$i]="${NODE_SYNC_STATE[$i]:-unknown}"
    PRE_NODE_PEERS[$i]="${NODE_PEERS[$i]:-0}"
  done
}

compute_metrics_from_current(){
  local prefix="$1" max=0 min=999999999 h i tips distinct worst converged
  for i in $(seq 1 "$NODE_COUNT"); do
    h=${NODE_HEIGHT[$i]:-0}
    (( h > max )) && max=$h
    (( h < min )) && min=$h
  done
  worst=$((max - min))
  distinct=$(for i in $(seq 1 "$NODE_COUNT"); do printf '%s\n' "${NODE_TIP[$i]:-}"; done | awk 'NF' | sort -u | wc -l | tr -d ' ')
  converged=0
  (( distinct == 1 && worst == 0 )) && converged=1
  if [[ "$prefix" == "PRE" ]]; then
    PRE_WORST_LAG=$worst; PRE_DISTINCT_TIPS=$distinct; PRE_CONVERGED=$converged
  else
    POST_WORST_LAG=$worst; POST_DISTINCT_TIPS=$distinct; POST_CONVERGED=$converged
  fi
}

write_quiescence_metrics(){
  local pre_orphans=0 post_orphans=0 pre_missing=0 post_missing=0 i
  for i in $(seq 1 "$NODE_COUNT"); do
    pre_orphans=$((pre_orphans + ${PRE_NODE_ORPHANS[$i]:-0}))
    post_orphans=$((post_orphans + ${NODE_ORPHANS[$i]:-0}))
    pre_missing=$((pre_missing + ${PRE_NODE_MISSING_PARENTS[$i]:-0}))
    post_missing=$((post_missing + ${NODE_MISSING_PARENTS[$i]:-0}))
  done
  (( POST_WORST_LAG < PRE_WORST_LAG )) && LAG_IMPROVED=1 || LAG_IMPROVED=0
  jq -n \
    --arg stage "$STAGE_NAME" \
    --argjson quiescence_secs "$QUIESCENCE_WAIT_SECS" \
    --argjson pre_converged "$PRE_CONVERGED" \
    --argjson post_converged "$POST_CONVERGED" \
    --argjson pre_worst_lag "$PRE_WORST_LAG" \
    --argjson post_worst_lag "$POST_WORST_LAG" \
    --argjson pre_distinct_tips "$PRE_DISTINCT_TIPS" \
    --argjson post_distinct_tips "$POST_DISTINCT_TIPS" \
    --argjson lag_improved "$LAG_IMPROVED" \
    --argjson pre_orphan_count "$pre_orphans" \
    --argjson post_orphan_count "$post_orphans" \
    --argjson pre_missing_parent_count "$pre_missing" \
    --argjson post_missing_parent_count "$post_missing" \
    '{stage:$stage,quiescence_secs:$quiescence_secs,pre:{converged:($pre_converged==1),worst_lag_from_max_height:$pre_worst_lag,distinct_tips:$pre_distinct_tips,total_orphan_count:$pre_orphan_count,total_missing_parent_count:$pre_missing_parent_count},post:{converged:($post_converged==1),worst_lag_from_max_height:$post_worst_lag,distinct_tips:$post_distinct_tips,total_orphan_count:$post_orphan_count,total_missing_parent_count:$post_missing_parent_count},lag_improved_during_quiescence:($lag_improved==1)}' \
    > "$OUT_DIR/quiescence-metrics.json"
}

write_evidence_summary(){
  local end_ts now_utc duration i unique_classes
  end_ts=$(date +%s); now_utc=$(date -u +%FT%TZ); duration=$((end_ts - START_TS))
  unique_classes=$(printf '%s\n' "${FAIL_CLASSES[@]:-}" | awk 'NF' | sort -u | paste -sd, -)
  {
    echo "# v2.2.19 $STAGE_NAME Rehearsal Evidence"
    echo "- chain id expected: \`$CHAIN_ID_EXPECTED\`"
    echo "- network profile: \`$NETWORK_PROFILE\`"
    echo "- start utc: $START_UTC"
    echo "- end utc: $now_utc"
    echo "- runtime duration (s): $duration"
    echo "- global deadline (s): $GLOBAL_DEADLINE_SECS"
    echo "- curl connect timeout (s): $CURL_CONNECT_TIMEOUT_SECS"
    echo "- curl max time (s): $CURL_MAX_TIME_SECS"
    echo "- quiescence wait (s): $QUIESCENCE_WAIT_SECS"
    echo "- miners stopped for quiescence: $MINERS_STOPPED_FOR_QUIESCENCE"
    echo
    echo "## Status/readiness per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: healthy=${NODE_HEALTHY[$i]:-0} ready=${NODE_READY[$i]:-0} readiness_schema_ok=${NODE_READINESS_SCHEMA_OK[$i]:-0}"; done
    echo
    echo "## P2P status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: peers=${NODE_PEERS[$i]:-0} inbound=${NODE_P2P_INBOUND[$i]:-0} outbound=${NODE_P2P_OUTBOUND[$i]:-0} ok=${NODE_P2P_OK[$i]:-0}"; done
    echo
    echo "## Sync/orphan status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: sync_state=${NODE_SYNC_STATE[$i]:-unknown} catchup_stage=${NODE_SYNC_STAGE[$i]:-unknown} orphan_count=${NODE_ORPHANS[$i]:-0} pending_missing_parents=${NODE_MISSING_PARENTS[$i]:-0} missing_parent_entries=${NODE_MISSING_PARENTS_COUNT[$i]:-0} inv_hashes_requested=${NODE_INV_HASHES_REQUESTED[$i]:-0} peer_recovery_success_count=${NODE_PEER_RECOVERY_SUCCESS_COUNT[$i]:-0}"; done
    echo
    echo "## Chain identity per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: chain_id=${NODE_CHAIN_ID[$i]:-unknown}"; done
    echo
    echo "## Final height/tip per node after quiescence"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: height=${NODE_HEIGHT[$i]:-0} tip=${NODE_TIP[$i]:-}"; done
    echo
    echo "## Miner summaries"
    for i in $(seq 1 "$MINER_COUNT"); do echo "- miner-${i}: templates=${miner_template[$i]:-0} submits=${miner_submit[$i]:-0} accepted=${miner_accept[$i]:-0} rejected=${miner_reject[$i]:-0}"; done
    echo
    echo "## Convergence and quiescence"
    echo "- convergence before quiescence: $([[ $PRE_CONVERGED == 1 ]] && echo PASS || echo FAIL)"
    echo "- convergence after quiescence: $([[ $POST_CONVERGED == 1 ]] && echo PASS || echo FAIL)"
    echo "- worst lag before quiescence from max height: $PRE_WORST_LAG"
    echo "- worst lag after quiescence from max height: $POST_WORST_LAG"
    echo "- distinct final tips after quiescence: $POST_DISTINCT_TIPS"
    echo "- lag improved during quiescence: $([[ $LAG_IMPROVED == 1 ]] && echo true || echo false)"
    echo
    echo "## Block acceptance/rejection counters"
    echo "- accepted blocks: $ACCEPTED_BLOCKS"
    echo "- rejected blocks: $REJECTED_BLOCKS"
    echo
    echo "## Sync recovery counters after quiescence"
    echo "| node | orphan_count | pending_missing_parents | missing_parents_entries | inv_hashes_requested | peer_recovery_success_count |"
    echo "|---|---:|---:|---:|---:|---:|"
    for i in $(seq 1 "$NODE_COUNT"); do
      echo "| n${i} | ${NODE_ORPHAN_COUNT[$i]:-0} | ${NODE_PENDING_MISSING_PARENTS[$i]:-0} | ${NODE_MISSING_PARENTS_COUNT[$i]:-0} | ${NODE_INV_HASHES_REQUESTED[$i]:-0} | ${NODE_PEER_RECOVERY_SUCCESS_COUNT[$i]:-0} |"
    done
    echo
    echo "## Separated recovery gates"
    echo "| gate | status | readiness mandatory |"
    echo "|---|---|---|"
    echo "| 5N/1M baseline | $GATE_5N_1M_BASELINE | yes |"
    echo "| 5N/2M intermediate | $GATE_5N_2M_INTERMEDIATE | yes |"
    echo "| 5N/4M stress | $GATE_5N_4M_STRESS | no, evidence only for v2.2.19 |"
    echo
    echo "## Required gates"
    echo "| gate | status |"
    echo "|---|---|"
    echo "| 5 nodes launched | $( (( ${#NODE_PIDS[@]}==5 )) && echo PASS || echo FAIL ) |"
    echo "| ${MINER_COUNT} miners launched | $( (( ${#MINER_PIDS[@]}>=MINER_COUNT || MINERS_STOPPED_FOR_QUIESCENCE==1 )) && echo PASS || echo FAIL ) |"
    echo "| bootnode peer id extracted | $([[ -n "${NODE_1_ID:-}" ]] && echo PASS || echo FAIL) |"
    echo "| all nodes healthy/status | $( for i in $(seq 1 "$NODE_COUNT"); do [[ "${NODE_HEALTHY[$i]:-0}" == "1" ]] || exit 1; done; echo PASS ) |"
    echo "| all nodes readiness (baseline/intermediate only) | $( for i in $(seq 1 "$NODE_COUNT"); do [[ "${NODE_READY[$i]:-0}" == "1" ]] || exit 1; done; echo PASS ) |"
    echo "| peer count network non-zero | $( (( $(for i in $(seq 1 "$NODE_COUNT"); do echo ${NODE_PEERS[$i]:-0}; done | awk '{s+=$1} END{print s+0}') > 0 )) && echo PASS || echo FAIL ) |"
    echo "| heights above genesis | $( for i in $(seq 1 "$NODE_COUNT"); do (( ${NODE_HEIGHT[$i]:-0} > 0 )) || exit 1; done; echo PASS ) |"
    echo "| baseline miner receives templates | $( (( ${miner_template[1]:-0} == 1 )) && echo PASS || echo FAIL ) |"
    echo "| baseline miner submits work | $( (( ${miner_submit[1]:-0} == 1 )) && echo PASS || echo FAIL ) |"
    if (( MINER_COUNT >= 2 )); then
      echo "| intermediate miners receive templates | $( for i in 1 2; do (( ${miner_template[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
      echo "| intermediate miners submit work | $( for i in 1 2; do (( ${miner_submit[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    else
      echo "| intermediate miners receive templates | NOT_RUN |"
      echo "| intermediate miners submit work | NOT_RUN |"
    fi
    echo "| stress miners receive templates (evidence only) | $( for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_template[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    echo "| stress miners submit work (evidence only) | $( for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_submit[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    echo "| accepted blocks >0 (or waived) | $( (( ACCEPTED_BLOCKS>0 || WAIVE_ACCEPTED_BLOCK_GATE==1 )) && echo PASS || echo FAIL ) |"
    echo "| convergence after quiescence | $( (( POST_CONVERGED==1 )) && echo PASS || echo FAIL ) |"
    echo "| missing parent backlog clear | $(for i in $(seq 1 "$NODE_COUNT"); do (( ${NODE_MISSING_PARENTS[$i]:-0} == 0 )) || { echo FAIL; break; }; [[ $i == $NODE_COUNT ]] && echo PASS; done) |"
    echo
    echo "## Build/runtime metadata"
    echo "- commit: $REPO_COMMIT"
    echo "- version: $NODE_VERSION"
    echo
    echo "## Result"
    echo "- result: $RESULT"
    echo "- exit_code: $EXIT_CODE"
    echo "- node_count: $NODE_COUNT"
    echo "- miner_count: $MINER_COUNT"
    echo "- failure_classification: ${unique_classes:-none}"
    echo "- warnings:"
    if (( ${#WARNINGS[@]} > 0 )); then for w in "${WARNINGS[@]}"; do echo "  - $w"; done; else echo "  - none"; fi
    echo "- failure reasons:"
    if (( ${#FAIL_REASONS[@]} > 0 )); then for r in "${FAIL_REASONS[@]}"; do echo "  - $r"; done; else echo "  - none"; fi
  } > "$OUT_DIR/evidence-summary.md"
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  printf "%s\n" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt"
}

write_p2p_convergence_json(){
  jq -n \
    --arg stage "$STAGE_NAME" \
    --arg chain_id "$CHAIN_ID_EXPECTED" \
    --arg version "$NODE_VERSION" \
    --arg commit "$REPO_COMMIT" \
    --arg tip "${NODE_TIP[1]:-}" \
    --argjson miner_count "$MINER_COUNT" \
    --argjson accepted_blocks "${ACCEPTED_BLOCKS:-0}" \
    --argjson rejected_blocks "${REJECTED_BLOCKS:-0}" \
    --arg gate_5n_1m "$GATE_5N_1M_BASELINE" \
    --arg gate_5n_2m "$GATE_5N_2M_INTERMEDIATE" \
    --arg gate_5n_4m "$GATE_5N_4M_STRESS" \
    --argjson nodes "$(for i in $(seq 1 "$NODE_COUNT"); do
      jq -n --arg node "n$i" --arg chain_id "${NODE_CHAIN_ID[$i]:-}" --argjson height "${NODE_HEIGHT[$i]:-0}" --arg tip "${NODE_TIP[$i]:-}" --argjson peer_count "${NODE_PEERS[$i]:-0}" --argjson orphan_count "${NODE_ORPHAN_COUNT[$i]:-0}" --argjson pending_missing_parents "${NODE_PENDING_MISSING_PARENTS[$i]:-0}" --argjson missing_parents_entries "${NODE_MISSING_PARENTS_COUNT[$i]:-0}" --argjson inv_hashes_requested "${NODE_INV_HASHES_REQUESTED[$i]:-0}" --argjson peer_recovery_success_count "${NODE_PEER_RECOVERY_SUCCESS_COUNT[$i]:-0}" '{node:$node,chain_id:$chain_id,height:$height,tip:$tip,peer_count:$peer_count,orphan_count:$orphan_count,pending_missing_parents:$pending_missing_parents,missing_parents_entries:$missing_parents_entries,inv_hashes_requested:$inv_hashes_requested,peer_recovery_success_count:$peer_recovery_success_count}'
    done | jq -s '.')" \
    '{chain_id:$chain_id,version:$version,commit:$commit,tip:$tip,accepted_blocks:$accepted_blocks,rejected_blocks:$rejected_blocks,gates:{baseline_5n_1m:$gate_5n_1m,intermediate_5n_2m:$gate_5n_2m,stress_5n_4m:$gate_5n_4m},nodes:$nodes}' \
    > "$OUT_DIR/p2p_convergence.json"
}

write_restart_rejoin_log(){
  {
    echo "restart_rejoin_status=NOT_EXECUTED"
    echo "note=this staged convergence rehearsal validates steady-state convergence and quiescence; restart/rejoin drill not invoked by this script"
    echo "timestamp_utc=$(date -u +%FT%TZ)"
  } > "$OUT_DIR/restart_rejoin.log"
}

write_metadata(){
  {
    echo "stage_name=$STAGE_NAME"
    echo "git_ref=$(git -C "$ROOT_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
    echo "git_commit=$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "version=$(cat "$ROOT_DIR/VERSION" 2>/dev/null || echo unknown)"
    echo "cargo_workspace_version=$(cargo metadata --format-version 1 --no-deps 2>/dev/null | jq -r '.packages[0].version // "unknown"' || echo unknown)"
    echo "uname=$(uname -a 2>/dev/null || echo unknown)"
    echo "rustc_version=$(rustc --version 2>/dev/null || echo unavailable)"
    echo "cargo_version=$(cargo --version 2>/dev/null || echo unavailable)"
    echo "start_utc=$START_UTC"
    echo "end_utc=$(date -u +%FT%TZ)"
    echo "duration_seconds=$(( $(date +%s) - START_TS ))"
    echo "exit_code=$EXIT_CODE"
    echo "global_deadline_seconds=$GLOBAL_DEADLINE_SECS"
    echo "curl_connect_timeout_seconds=$CURL_CONNECT_TIMEOUT_SECS"
    echo "curl_max_time_seconds=$CURL_MAX_TIME_SECS"
    echo "quiescence_wait_seconds=$QUIESCENCE_WAIT_SECS"
  } > "$OUT_DIR/summaries/package-metadata.txt"
}

write_checksum_file(){
  local file="$1" out="$2" digest
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" > "$out"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" > "$out"
  elif command -v openssl >/dev/null 2>&1; then
    digest=$(openssl dgst -sha256 -r "$file" | awk '{print $1}')
    printf '%s  %s\n' "$digest" "$file" > "$out"
  else
    record_warn "no sha256sum/shasum/openssl available; writing unavailable checksum marker"
    printf 'UNAVAILABLE  %s\n' "$file" > "$out"
  fi
}

verify_checksum_file(){
  local dir="$1" checksum_file="$2" checksum_path expected file file_path actual
  checksum_path="$dir/$checksum_file"
  [[ -s "$checksum_path" ]] || return 1
  expected=$(awk '{print $1; exit}' "$checksum_path" 2>/dev/null || true)
  file=$(awk '{print $2; exit}' "$checksum_path" 2>/dev/null || true)
  if [[ "$file" == */* ]]; then file_path="$file"; else file_path="$dir/$file"; fi
  [[ "$expected" != "UNAVAILABLE" && -n "$expected" && -n "$file" && -s "$file_path" ]] || return 0
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$checksum_path"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "$checksum_path"
  elif command -v openssl >/dev/null 2>&1; then
    actual=$(openssl dgst -sha256 -r "$file_path" | awk '{print $1}')
    [[ "$actual" == "$expected" ]]
  else
    return 0
  fi
}

package_evidence(){
  write_metadata || true
  cp "$OUT_DIR/p2p_convergence.json" "$OUT_DIR/final-convergence-table.json" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR/summaries/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/command-log.txt" "$OUT_DIR_ROOT/command-log.txt" 2>/dev/null || true
  cp "$OUT_DIR/bootnode.txt" "$OUT_DIR_ROOT/bootnode.txt" 2>/dev/null || true
  printf "%s\n" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt" 2>/dev/null || true
  printf "%s\n" "$OUT_DIR" > "$OUT_DIR/current-run-dir.txt" 2>/dev/null || true
  for i in $(seq 1 "$NODE_COUNT"); do cp "$OUT_DIR/logs/n${i}.log" "$OUT_DIR/nodes/n${i}.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do cp "$OUT_DIR/logs/miner-${i}.log" "$OUT_DIR/miners/miner-${i}.log" 2>/dev/null || true; done
  local tar_tmp manifest item tar_rc=0
  tar_tmp=$(mktemp -p /tmp evidence.XXXXXX.tar.gz) || return 1
  manifest=$(mktemp -p /tmp evidence-manifest.XXXXXX) || { rm -f "$tar_tmp"; return 1; }
  for item in endpoints logs miners nodes samples summaries evidence-summary.md command-log.txt process-pids.txt p2p_convergence.json final-convergence-table.json quiescence-metrics.json restart_rejoin.log global-watchdog-timeout.txt current-run-dir.txt; do
    [[ -e "$OUT_DIR/$item" ]] && printf '%s\n' "$item" >> "$manifest"
  done
  if [[ -s "$manifest" ]]; then
    (cd "$OUT_DIR" && tar -czf "$tar_tmp" --exclude='evidence.tar.gz' --exclude='evidence.tar.gz.sha256' -T "$manifest") || tar_rc=$?
  else
    (cd "$OUT_DIR" && tar -czf "$tar_tmp" --files-from /dev/null) || tar_rc=$?
  fi
  if (( tar_rc != 0 )); then
    record_warn "tar returned non-zero while packaging available evidence; retrying with empty evidence archive"
    (cd "$OUT_DIR" && tar -czf "$tar_tmp" --files-from /dev/null) || { rm -f "$manifest" "$tar_tmp"; return 1; }
  fi
  rm -f "$manifest"
  mv "$tar_tmp" "$OUT_DIR/evidence.tar.gz" || { rm -f "$tar_tmp"; return 1; }
  write_checksum_file "$OUT_DIR/evidence.tar.gz" "$OUT_DIR/evidence.tar.gz.sha256" || record_warn "failed to write evidence checksum"
  cp "$OUT_DIR/evidence.tar.gz" "$OUT_DIR_ROOT/evidence.tar.gz" 2>/dev/null || true
  write_checksum_file "$OUT_DIR_ROOT/evidence.tar.gz" "$OUT_DIR_ROOT/evidence.tar.gz.sha256" || cp "$OUT_DIR/evidence.tar.gz.sha256" "$OUT_DIR_ROOT/evidence.tar.gz.sha256" 2>/dev/null || true
  verify_checksum_file "$OUT_DIR_ROOT" evidence.tar.gz.sha256 || record_warn "root evidence checksum verification failed"
  verify_checksum_file "$OUT_DIR" evidence.tar.gz.sha256 || record_warn "run evidence checksum verification failed"
  [[ -s "$OUT_DIR/evidence.tar.gz" ]] || return 1
  [[ -s "$OUT_DIR/evidence.tar.gz.sha256" ]] || return 1
}

on_signal(){
  local signal_name="$1" exit_code="$2" class msg
  if (( CLEANUP_STARTED == 1 )); then
    return 0
  fi
  IN_CLEANUP=1
  class="SIGNAL_${signal_name}"
  msg="received SIG${signal_name}; finalizing evidence before exit"
  if [[ "$signal_name" == "TERM" && -f "$OUT_DIR/global-watchdog-timeout.txt" ]]; then
    class="GLOBAL_WATCHDOG_TIMEOUT"
    msg="global watchdog timeout after ${GLOBAL_DEADLINE_SECS}s"
  fi
  record_fail "$class" "$msg"
  exit "$exit_code"
}

cleanup(){
  local exit_code=${1:-$?} package_rc=0
  if (( CLEANUP_STARTED == 1 )); then
    return 0
  fi
  CLEANUP_STARTED=1
  IN_CLEANUP=1
  EXIT_CODE=$exit_code
  trap - EXIT
  trap '' INT TERM
  stop_global_deadline_watchdog
  if (( exit_code == 124 )); then
    if (( ${#FAIL_REASONS[@]} == 0 )); then record_fail "GLOBAL_DEADLINE_TIMEOUT" "script exited with timeout status 124 before classified failure"; fi
  elif (( exit_code != 0 && ${#FAIL_REASONS[@]} == 0 )); then
    record_fail "HARNESS_ERROR" "script exited non-zero before classified failure: $exit_code"
  fi
  if (( QUIESCENCE_COMPLETED == 0 )); then
    collect_final_state cleanup-pre-quiescence || true
    snapshot_current_as_pre || true
    compute_metrics_from_current PRE || true
    POST_CONVERGED=$PRE_CONVERGED; POST_WORST_LAG=$PRE_WORST_LAG; POST_DISTINCT_TIPS=$PRE_DISTINCT_TIPS; LAG_IMPROVED=0
    write_quiescence_metrics || true
  fi
  if (( ${#FAIL_REASONS[@]} == 0 )); then RESULT="PASS"; else RESULT="FAIL"; fi
  capture_log_tails || true
  write_evidence_summary || true
  write_p2p_convergence_json || true
  write_restart_rejoin_log || true
  stop_pids "${MINER_PIDS[@]:-}"
  stop_pids "${NODE_PIDS[@]:-}"
  package_evidence || package_rc=$?
  if (( package_rc != 0 )); then
    echo "FATAL: evidence packaging failed for $OUT_DIR (rc=$package_rc)"
    exit_code=1
    EXIT_CODE=$exit_code
    record_fail "EVIDENCE_PACKAGING_FAILED" "evidence packaging failed with rc=$package_rc"
    RESULT="FAIL"
    write_evidence_summary || true
  fi
  echo "RUN_DIR=$OUT_DIR"
  echo "FINAL_EXIT_CODE=$exit_code"
  echo "FINAL_RESULT=$RESULT"
  exit "$exit_code"
}
trap 'cleanup $?' EXIT
trap 'on_signal INT 130' INT
trap 'on_signal TERM 143' TERM

start_global_deadline_watchdog
OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"
ensure_ports_free
cargo build --workspace --release --locked

start_node(){
  local idx="$1" rpc="$2" p2p="$3" bootnode="$4" name data
  name="n${idx}"
  data="$OUT_DIR/data-${name}"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --network "$NETWORK_PROFILE" --rpc-listen "127.0.0.1:${rpc}" --p2p-listen "/ip4/127.0.0.1/tcp/${p2p}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  echo "launch node-${name}: PULSEDAG_ROCKSDB_PATH=$data/rocksdb ${cmd[*]}"
  PULSEDAG_ROCKSDB_PATH="$data/rocksdb" "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  NODE_PIDS+=("$!")
  echo "$! node-${name}" >> "$OUT_DIR/process-pids.txt"
}

wait_node_ready(){
  local idx="$1" rpc
  rpc=$((BASE_RPC_PORT+idx))
  for _ in $(seq 1 60); do
    safe_curl_required "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${idx}-status-ready.json" && return 0
    sleep_with_deadline 2
  done
  record_fail "RPC_UNAVAILABLE" "node n${idx} failed status readiness polling"
  return 1
}

start_node 1 $((BASE_RPC_PORT+1)) $((BASE_P2P_PORT+1)) ""; sleep_with_deadline 3
safe_curl_required "http://127.0.0.1:$((BASE_RPC_PORT+1))/p2p/status" "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json"
NODE_1_ID=$(jq -r '.data.peer_id // .data.local_node_id // empty' "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json" 2>/dev/null || true)
if [[ -z "$NODE_1_ID" ]]; then
  record_fail "READINESS_SCHEMA_MISMATCH" "failed to extract bootnode peer id from n1 /p2p/status"
  echo "FATAL: unable to build bootnode multiaddr because peer id extraction failed"
  exit 1
fi
BOOT_1="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+1))/p2p/${NODE_1_ID}"
echo "$BOOT_1" > "$OUT_DIR/bootnode.txt"
for i in 2 3 4 5; do start_node "$i" $((BASE_RPC_PORT+i)) $((BASE_P2P_PORT+i)) "$BOOT_1"; done
sleep_with_deadline 3

for i in $(seq 1 "$NODE_COUNT"); do wait_node_ready "$i" || true; done

peer_wait_deadline=$(( $(date +%s) + P2P_CONNECT_WAIT_SECS ))
peers_total=0
while (( $(date +%s) < peer_wait_deadline )); do
  peers_total=0
  for i in $(seq 1 "$NODE_COUNT"); do
    safe_curl_optional "http://127.0.0.1:$((BASE_RPC_PORT+i))/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" "n${i}:/p2p/status pre-mining" || true
    peers_total=$((peers_total + $(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" 2>/dev/null || echo 0)))
  done
  (( peers_total > 0 )) && break
  sleep_with_deadline 2
done
(( peers_total > 0 )) || { capture_p2p_gate_failure; record_fail "P2P_NOT_CONNECTED" "pre-mining p2p peers remained zero after ${P2P_CONNECT_WAIT_SECS}s"; exit 1; }

for i in $(seq 1 "$MINER_COUNT"); do
  local_node="http://127.0.0.1:$((BASE_RPC_PORT+i))"
  echo "launch miner-${i}: $MINER_BIN --node $local_node --miner-address v2219-${RUN_ID}-miner-${i} --backend cpu --threads 1 --loop"
  "$MINER_BIN" --node "$local_node" --miner-address "v2219-${RUN_ID}-miner-${i}" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner-${i}.log" 2>&1 &
  MINER_PIDS+=("$!")
  echo "$! miner-${i}" >> "$OUT_DIR/process-pids.txt"
done

printf "timestamp,n1_height,n2_height,n3_height,n4_height,n5_height,tip_match,distinct_tips,total_orphans,total_missing_parents\n" > "$OUT_DIR/samples/height-samples.csv"
end=$(( $(date +%s) + DURATION_SECS ))
while (( $(date +%s) < end )); do
  heights=(); tips=()
  for i in 1 2 3 4 5; do
    rpc=$((BASE_RPC_PORT+i))
    safe_curl_optional "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${i}-status.json" "n${i}:/status" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status.json" "n${i}:/p2p/status" || true
    heights+=("$(json_get_or_default '.data.best_height' "$OUT_DIR/endpoints/n${i}-status.json" '0')")
    tips+=("$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo '')")
  done
  tip_match=1; ref_tip="${tips[0]}"; for t in "${tips[@]}"; do [[ "$t" == "$ref_tip" ]] || tip_match=0; done
  echo "$(date -u +%FT%TZ),${heights[0]},${heights[1]},${heights[2]},${heights[3]},${heights[4]},$tip_match" >> "$OUT_DIR/samples/height-samples.csv"

  collect_miner_metrics
  sleep_with_deadline 10
done

echo "entering quiescence: collecting pre-quiescence sample before stopping miners"
collect_final_state pre-quiescence
snapshot_current_as_pre
compute_metrics_from_current PRE

echo "entering quiescence: stopping miners and waiting ${QUIESCENCE_WAIT_SECS}s before final tips/readiness sample"
stop_pids "${MINER_PIDS[@]:-}"
MINERS_STOPPED_FOR_QUIESCENCE=1
sleep_with_deadline "$QUIESCENCE_WAIT_SECS"
collect_final_state quiescent
compute_metrics_from_current POST
write_quiescence_metrics
QUIESCENCE_COMPLETED=1

BASELINE_OK=1
INTERMEDIATE_OK=1
STRESS_OK=1

for i in 1 2 3 4 5; do
  if [[ "${NODE_HEALTHY[$i]:-0}" != "1" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if [[ "${NODE_READY[$i]:-0}" != "1" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; fi
  if (( ${NODE_HEIGHT[$i]:-0} <= 0 )); then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if [[ "${NODE_P2P_OK[$i]:-0}" != "1" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if [[ -z "${NODE_CHAIN_ID[$i]:-}" || "${NODE_CHAIN_ID[$i]:-}" != "$CHAIN_ID_EXPECTED" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
done

final_tip="${NODE_TIP[1]:-}"
for i in 1 2 3 4 5; do
  [[ "${NODE_TIP[$i]:-}" == "$final_tip" ]] || { BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; }
done

(( ${miner_template[1]:-0} == 1 )) || BASELINE_OK=0
(( ${miner_submit[1]:-0} == 1 )) || BASELINE_OK=0
for i in 1 2; do
  (( ${miner_template[$i]:-0} == 1 )) || INTERMEDIATE_OK=0
  (( ${miner_submit[$i]:-0} == 1 )) || INTERMEDIATE_OK=0
done
for i in $(seq 1 "$MINER_COUNT"); do
  (( ${miner_template[$i]:-0} == 1 )) || STRESS_OK=0
  (( ${miner_submit[$i]:-0} == 1 )) || STRESS_OK=0
done

(( BASELINE_OK == 1 )) && GATE_5N_1M_BASELINE=PASS || GATE_5N_1M_BASELINE=FAIL
if (( MINER_COUNT >= 2 )); then
  (( INTERMEDIATE_OK == 1 )) && GATE_5N_2M_INTERMEDIATE=PASS || GATE_5N_2M_INTERMEDIATE=FAIL
else
  GATE_5N_2M_INTERMEDIATE=NOT_RUN
fi
if (( MINER_COUNT >= 4 )); then
  (( STRESS_OK == 1 )) && GATE_5N_4M_STRESS=PASS || GATE_5N_4M_STRESS=OBSERVE_FAIL
else
  GATE_5N_4M_STRESS=NOT_RUN
fi

[[ "$GATE_5N_1M_BASELINE" == "PASS" ]] || record_fail "STAGED_GATE_5N_1M" "5N/1M baseline gate failed after quiescence"
if (( MINER_COUNT >= 2 )); then
  [[ "$GATE_5N_2M_INTERMEDIATE" == "PASS" ]] || record_fail "STAGED_GATE_5N_2M" "5N/2M intermediate gate failed after quiescence"
fi
if (( MINER_COUNT >= 4 )) && [[ "$GATE_5N_4M_STRESS" != "PASS" ]]; then
  record_warn "5N/4M stress gate did not pass; retained as non-mandatory readiness evidence for v2.2.19"
fi

if (( ACCEPTED_BLOCKS < 1 )); then
  if (( WAIVE_ACCEPTED_BLOCK_GATE == 1 )); then
    [[ -n "$WAIVE_ACCEPTED_BLOCK_REASON" ]] && record_warn "accepted blocks gate waived: $WAIVE_ACCEPTED_BLOCK_REASON" || record_fail "MINER_NO_ACCEPTED_BLOCKS" "accepted blocks gate waived without reason"
  else
    record_fail "MINER_NO_ACCEPTED_BLOCKS" "accepted blocks is zero"
  fi
fi

if (( ${#FAIL_REASONS[@]} > 0 )); then
  echo "FAIL $STAGE_NAME rehearsal: $OUT_DIR"
  echo "FINAL_RESULT=FAIL"
  exit 1
fi

echo "PASS $STAGE_NAME rehearsal complete: $OUT_DIR"
echo "FINAL_RESULT=PASS"
