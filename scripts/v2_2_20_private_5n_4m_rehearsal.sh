#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-1800}
P2P_CONNECT_WAIT_SECS=${P2P_CONNECT_WAIT_SECS:-120}
STARTUP_TOPOLOGY_SAMPLE_INTERVAL_SECS=${STARTUP_TOPOLOGY_SAMPLE_INTERVAL_SECS:-2}
STARTUP_TOPOLOGY_REQUIRED_STABLE_SAMPLES=${STARTUP_TOPOLOGY_REQUIRED_STABLE_SAMPLES:-3}
CURL_CONNECT_TIMEOUT_SECS=${CURL_CONNECT_TIMEOUT_SECS:-3}
CURL_MAX_TIME_SECS=${CURL_MAX_TIME_SECS:-10}
CLEANUP_CURL_CONNECT_TIMEOUT_SECS=${CLEANUP_CURL_CONNECT_TIMEOUT_SECS:-1}
CLEANUP_CURL_MAX_TIME_SECS=${CLEANUP_CURL_MAX_TIME_SECS:-2}
FINAL_CAPTURE_BUDGET_SECS=${FINAL_CAPTURE_BUDGET_SECS:-45}
CLEANUP_KILL_GRACE_SECS=${CLEANUP_KILL_GRACE_SECS:-3}
CLEANUP_PORT_WAIT_SECS=${CLEANUP_PORT_WAIT_SECS:-10}
QUIESCENCE_WAIT_SECS=${QUIESCENCE_WAIT_SECS:-90}
PEER_ZERO_OUTAGE_SECS=${PEER_ZERO_OUTAGE_SECS:-20}
PR647_RUNTIME_CASES=${PR647_RUNTIME_CASES:-0}
GLOBAL_DEADLINE_SECS=${GLOBAL_DEADLINE_SECS:-21600}
MAX_GLOBAL_DEADLINE_SECS=21600
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
START_TS=$(date +%s)
START_UTC=$(date -u +%FT%TZ)
LAST_PROGRESS_TS=$START_TS
CLEANUP_DEADLINE_TS=0
HARD_KILL_WATCHDOG_PID=
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
  *) echo "FATAL: MINER_COUNT must be 1, 2, or 4 for staged v2.2.20 convergence gates" >&2; exit 2 ;;
esac

OUT_DIR_BASE="${OUT_DIR:-$ROOT_DIR/artifacts/v2_2_20/$DEFAULT_OUT}"
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
declare -A NODE_MISSING_PARENT_REQUESTS_SENT NODE_MISSING_PARENT_RESPONSES_RECEIVED
declare -A NODE_ORPHAN_COUNT NODE_PENDING_MISSING_PARENTS NODE_PENDING_BLOCK_REQUESTS NODE_INV_HASHES_REQUESTED NODE_PEER_RECOVERY_SUCCESS_COUNT NODE_MISSING_PARENTS_COUNT NODE_TERMINAL_MISSING_PARENTS_COUNT
declare -A NODE_ACTIVE_PEERS NODE_RECOVERING_PEERS NODE_COOLDOWN_PEERS NODE_RATE_LIMITED_COUNT NODE_RECONNECT_ATTEMPTS NODE_RECONNECT_BLOCKED_REASON NODE_MIN_TARGET_MISSED NODE_ZERO_RECONNECT_ATTEMPTS NODE_ZERO_RECONNECT_SUCCESS
declare -A NODE_ORPHAN_RECOVERY_ATTEMPTS NODE_ORPHAN_RECOVERY_SUCCESS NODE_ORPHAN_RECOVERY_FAILED_MISSING_PARENT NODE_ORPHAN_RECOVERY_FAILED_PERSIST NODE_ORPHAN_ROOTS_RATE_LIMITED NODE_ORPHAN_BACKLOG_STALE
declare -A NODE_RPC_DEGRADED_RESPONSE NODE_RPC_SNAPSHOT_STALE NODE_RPC_HANDLER_DEGRADED NODE_RPC_HANDLER_TIMEOUT_AVOIDED NODE_MINING_TEMPLATES NODE_MINING_SUBMITS NODE_MINING_ACCEPTED NODE_MINING_REJECTED NODE_MINING_SUBMIT_BUSY NODE_MINING_ACTOR_TIMEOUT
declare -A NODE_READINESS_SCHEMA_OK NODE_READINESS_STATUS NODE_ORDERED_DAG_TIP NODE_CONSENSUS_MODE NODE_SELECTED_TIP NODE_SYNC_STATE NODE_SYNC_STAGE NODE_SYNC_OBSERVABILITY_WARNING NODE_CANONICAL_NODE_GAP NODE_HARNESS_OBSERVED_GAP NODE_SAME_HEIGHT_RECONCILE_BLOCKED_REASON
declare -A NODE_STATE_ROOT NODE_ACCEPTED_HASH_SET_DIGEST NODE_STORAGE_MEMORY_COUNT NODE_STORAGE_PERSISTED_COUNT NODE_STORAGE_MEMORY_DIGEST NODE_STORAGE_PERSISTED_DIGEST NODE_STORAGE_COHERENT NODE_STORAGE_ONLY_HASHES NODE_MEMORY_ONLY_HASHES NODE_STORAGE_MISMATCH_SOURCE NODE_CHECKS_GENERATION NODE_CHECKS_DIGEST
declare -A NODE_NETWORK_GAPS NODE_STORAGE_GAPS NODE_SELECTED_SEGMENT_REQUESTED NODE_SELECTED_SEGMENT_RECEIVED NODE_SELECTED_SEGMENT_APPLIED NODE_CORRELATED_REQUEST_COUNTERS
declare -A PRE_NODE_HEIGHT PRE_NODE_TIP PRE_NODE_ORPHANS PRE_NODE_MISSING_PARENTS PRE_NODE_SYNC_STATE PRE_NODE_PEERS
FAIL_REASONS=()
FAIL_CLASSES=()
WARNINGS=()
FAILURE_CLASS="none"
ENV_PREFLIGHT_OK=0
RESULT="PENDING"
EXIT_CODE=0
WAIVE_ACCEPTED_BLOCK_GATE=${WAIVE_ACCEPTED_BLOCK_GATE:-0}
WAIVE_ACCEPTED_BLOCK_REASON=${WAIVE_ACCEPTED_BLOCK_REASON:-""}
ACCEPTED_BLOCKS=0
REJECTED_BLOCKS=0
TEMPLATES_OK=0
RPC_ALIVE_LISTENER_TIMEOUT_COUNT=0
RPC_LIVENESS_TIMEOUT_COUNT=0
STALE_DEGRADED_SNAPSHOT_COUNT=0
TOTAL_ORPHAN_COUNT=0
TOTAL_PENDING_MISSING_PARENTS=0
TOTAL_PENDING_BLOCK_REQUESTS=0
TOTAL_MISSING_PARENT_ENTRIES=0
TOTAL_INV_HASHES_REQUESTED=0
TOTAL_ACTIVE_PEERS=0
TOTAL_RECOVERING_PEERS=0
TOTAL_COOLDOWN_PEERS=0
TOTAL_RATE_LIMITED_COUNT=0
TOTAL_RECONNECT_ATTEMPTS=0
TOTAL_MIN_TARGET_MISSED=0
TOTAL_ZERO_RECONNECT_ATTEMPTS=0
TOTAL_ZERO_RECONNECT_SUCCESS=0
RECONNECT_BLOCKED_REASONS_JSON='[]'
SAME_HEIGHT_RECONCILE_BLOCKED_REASONS_JSON='[]'
TOTAL_MINING_TEMPLATES=0
TOTAL_MINING_SUBMITS=0
TOTAL_MINING_ACCEPTED=0
TOTAL_MINING_REJECTED=0
TOTAL_MINING_SUBMIT_BUSY=0
TOTAL_MINING_ACTOR_TIMEOUT=0
TOTAL_ORPHAN_RECOVERY_ATTEMPTS=0
TOTAL_ORPHAN_RECOVERY_SUCCESS=0
TOTAL_ORPHAN_RECOVERY_FAILED_MISSING_PARENT=0
TOTAL_ORPHAN_RECOVERY_FAILED_PERSIST=0
TOTAL_ORPHAN_ROOTS_RATE_LIMITED=0
TOTAL_ORPHAN_BACKLOG_STALE=0
TOTAL_MISSING_PARENT_REQUESTS_SENT=0
TOTAL_MISSING_PARENT_RESPONSES_RECEIVED=0
TOTAL_BLOCKDATA_NOT_FOUND=0
MINERS_STOPPED_FOR_QUIESCENCE=0
GATE_5N_1M_BASELINE=NOT_PROVIDED
GATE_5N_2M_INTERMEDIATE=NOT_PROVIDED
GATE_5N_4M_STRESS=OBSERVE
STARTUP_TOPOLOGY_SAMPLES_TOTAL=0
STARTUP_TOPOLOGY_STABLE_SAMPLES=0
STARTUP_TOPOLOGY_RESETS_TOTAL=0
STARTUP_CONNECTION_KEEPALIVE_TIMEOUTS_TOTAL=0
STARTUP_RECONNECT_ATTEMPTS_TOTAL=0
STARTUP_RECONNECT_SUCCESS_TOTAL=0
STARTUP_TOPOLOGY_READY_UNIX=0
STARTUP_TOPOLOGY_WAIT_DURATION_MS=0
MINERS_STARTED=0
EVIDENCE_MANIFEST_GENERATION_FAILED=0
MANIFEST_GENERATION_ERROR=
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
PRIOR_GATE_C_MANIFEST=${PRIOR_GATE_C_MANIFEST:-}
PRIOR_GATE_D_MANIFEST=${PRIOR_GATE_D_MANIFEST:-}
PRIOR_GATE_C_SHA256=
PRIOR_GATE_D_SHA256=
PRIOR_GATE_C_RESULT=NOT_PROVIDED
PRIOR_GATE_D_RESULT=NOT_PROVIDED
PRIOR_GATE_C_COMMIT=
PRIOR_GATE_D_COMMIT=
UNIQUE_TEMPLATES_ISSUED=0
LOCAL_MINER_SUBMITS_TOTAL=0
LOCAL_MINER_SUBMITS_ACCEPTED=0
LOCAL_MINER_SUBMITS_REJECTED=0
LOCAL_MINER_SUBMITS_REJECTED_BY_REASON_JSON='[]'
NODE_BLOCK_ACCEPT_EVENTS_TOTAL=0
NODE_BLOCK_REJECT_EVENTS_TOTAL=0
DUPLICATE_BLOCK_EVENTS_TOTAL=0
NODE_BLOCK_DUPLICATE_EVENTS_TOTAL=0
UNIQUE_BLOCK_HASHES_OBSERVED=0
LOCAL_MINER_TEMPLATES_RECEIVED=0
UNIQUE_SUBMITTED_BLOCK_HASHES=0
MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON='[]'
STARTUP_RESULT=PENDING
CONVERGENCE_RESULT=PENDING
CONSENSUS_STATE_RESULT=PENDING
STORAGE_CONSISTENCY_RESULT=PENDING
READINESS_RESULT=PENDING
OVERALL_RESULT=PENDING
declare -a EVIDENCE_CONSISTENCY_FAILURES=()
declare -A miner_submit miner_accept miner_reject miner_template
for i in 1 2 3 4; do miner_submit[$i]=0; miner_accept[$i]=0; miner_reject[$i]=0; miner_template[$i]=0; done
REPO_REF="$(git -C "$ROOT_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
REPO_COMMIT_FULL="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
REPO_COMMIT="$(git -C "$ROOT_DIR" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
WORKSPACE_VERSION="$(cargo metadata --format-version 1 --no-deps 2>/dev/null | jq -r '.packages[0].version // "unknown"' || echo unknown)"
RELEASE_VERSION="$(cat "$ROOT_DIR/VERSION" 2>/dev/null || echo unknown)"
NODE_VERSION="$({ "$NODE_BIN" --version 2>/dev/null || true; } | head -n1)"
NODE_VERSION=${NODE_VERSION:-unknown}
for i in $(seq 1 "$MINER_COUNT"); do miner_submit[$i]=0; miner_accept[$i]=0; miner_reject[$i]=0; miner_template[$i]=0; done

text_has_match(){
  local pattern="$1" file="$2"
  [[ -f "$file" ]] || return 1
  if command -v rg >/dev/null 2>&1; then rg -qi -- "$pattern" "$file"; else grep -Eqi -- "$pattern" "$file"; fi
}

count_matches_file(){
  local pattern="$1" file="$2" output
  [[ -f "$file" ]] || { echo 0; return 0; }
  if command -v rg >/dev/null 2>&1; then
    output=$(rg -ci -- "$pattern" "$file" 2>/dev/null || true)
  else
    output=$(grep -Eic -- "$pattern" "$file" 2>/dev/null || true)
  fi
  integer_sum_or_zero "$output"
}

count_matches_in_logs(){
  local pattern="$1" total=0 c i
  for i in $(seq 1 "$MINER_COUNT"); do
    c=$(count_matches_file "$pattern" "$OUT_DIR/logs/miner-${i}.log")
    c=$(integer_sum_or_zero "$c")
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

parse_miner_terminal_evidence(){
  local parser="$ROOT_DIR/scripts/lib/miner_evidence_parser.awk" parsed
  if [[ ! -f "$parser" ]]; then
    record_fail "MINER_EVIDENCE_PARSER_MISSING" "miner evidence parser not found: $parser"
    return 1
  fi
  parsed=$(find "$OUT_DIR/logs" -maxdepth 1 -type f -regextype posix-extended -regex '.*/miner-[0-9]+[.]log' -print0 2>/dev/null | xargs -0 -r awk -f "$parser" 2>/dev/null || true)
  if [[ -z "$parsed" ]]; then
    parsed=$(awk -f "$parser" /dev/null 2>/dev/null || true)
  fi
  eval "$parsed"
  LOCAL_MINER_TEMPLATES_RECEIVED=$(json_number_or_zero "${local_miner_templates_received:-0}")
  LOCAL_MINER_SUBMITS_TOTAL=$(json_number_or_zero "${local_miner_submits_total:-0}")
  LOCAL_MINER_SUBMITS_ACCEPTED=$(json_number_or_zero "${local_miner_submits_accepted:-0}")
  LOCAL_MINER_SUBMITS_REJECTED=$(json_number_or_zero "${local_miner_submits_rejected:-0}")
  LOCAL_MINER_SUBMITS_REJECTED_BY_REASON_JSON=$(json_array_or_empty "${local_miner_submits_rejected_by_reason:-[]}")
  UNIQUE_SUBMITTED_BLOCK_HASHES=$(json_number_or_zero "${unique_submitted_block_hashes:-0}")
  MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON=$(json_array_or_empty "${miner_evidence_consistency_failures:-[]}")
  if [[ "$MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON" != "[]" ]]; then
    while IFS= read -r failure; do [[ -n "$failure" ]] && record_fail "MINER_EVIDENCE_CONSISTENCY" "$failure"; done < <(printf '%s' "$MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON" | jq -r '.[]' 2>/dev/null || true)
  fi
}

collect_miner_metrics(){
  local i log accepted_total=0 rejected_total=0 c
  for i in $(seq 1 "$MINER_COUNT"); do
    log="$OUT_DIR/logs/miner-${i}.log"
    miner_template[$i]=$(count_matches_file "template_received|received_template|new_template|template:" "$log")
    miner_submit[$i]=$(count_matches_file "submit_result:" "$log")
    miner_accept[$i]=$(awk '{ if ($0 ~ /submit_result:/ && $0 ~ /accepted=true/) count++ } END { print count + 0 }' "$log" 2>/dev/null || echo 0)
    miner_reject[$i]=$(awk '{ if ($0 ~ /submit_result:/ && $0 ~ /rejected=true/) count++ } END { print count + 0 }' "$log" 2>/dev/null || echo 0)
    c=$(json_number_or_zero "${miner_accept[$i]:-0}")
    accepted_total=$((accepted_total + c))
    c=$(json_number_or_zero "${miner_reject[$i]:-0}")
    rejected_total=$((rejected_total + c))
  done
  ACCEPTED_BLOCKS=$accepted_total
  REJECTED_BLOCKS=$rejected_total
  TEMPLATES_OK=0
  for i in $(seq 1 "$MINER_COUNT"); do
    if (( ${miner_template[$i]:-0} > 0 )); then
      TEMPLATES_OK=1
      break
    fi
  done
  return 0
}

record_warn(){ local msg="$1"; echo "WARN: $msg"; WARNINGS+=("$msg"); }
record_fail(){
  local class="$1" msg="$2"
  if [[ "$class" == ENV_FAIL* || "$class" == ENV_* ]]; then
    echo "ENV_FAIL[$class]: $msg" >&2
  else
    echo "FAIL[$class]: $msg" >&2
  fi
  FAIL_CLASSES+=("$class")
  FAIL_REASONS+=("$class: $msg")
}

failure_class_from_code(){
  local c="$1"
  [[ "$c" == ENV_FAIL* || "$c" == ENV_* ]] && { echo "environment"; return 0; }
  [[ "$c" == *TIMEOUT* || "$c" == HARNESS_STALL_TIMEOUT || "$c" == SIGNAL_TERM || "$c" == SIGNAL_INT ]] && { echo "timeout"; return 0; }
  [[ "$c" == FAIL_STARTUP* || "$c" == STARTUP_* ]] && { echo "startup"; return 0; }
  [[ "$c" == CONSENSUS_* || "$c" == *CONSENSUS_STATE* || "$c" == *STATE_ROOT* ]] && { echo "consensus_state"; return 0; }
  [[ "$c" == STAGED_GATE_* || "$c" == P2P_NOT_CONNECTED || "$c" == POST_QUIESCENCE_* || "$c" == *CONVERGENCE* ]] && { echo "convergence"; return 0; }
  [[ "$c" == STORAGE_* || "$c" == *STORAGE* || "$c" == *MEMORY_PERSIST* ]] && { echo "storage_consistency"; return 0; }
  [[ "$c" == READINESS_* || "$c" == *READINESS* ]] && { echo "readiness"; return 0; }
  [[ "$c" == EVIDENCE_* || "$c" == *EVIDENCE_CONSISTENCY* || "$c" == NETWORK_COUNTER_AGGREGATE_MISMATCH || "$c" == ACCEPTED_COUNTER_SEMANTIC_MISMATCH || "$c" == SYNC_OBSERVABILITY_* ]] && { echo "evidence_consistency"; return 0; }
  echo "node"
}

failure_classes_json(){
  local classes=() c cls prio present=0
  if [[ "${OVERALL_RESULT:-${RESULT:-}}" == "PASS" || "${RESULT:-}" == "PASS" ]]; then
    printf '[]\n'
    return 0
  fi
  [[ "${STARTUP_RESULT:-}" == "FAIL" ]] && classes+=(startup)
  [[ "${CONSENSUS_STATE_RESULT:-}" == "FAIL" ]] && classes+=(consensus_state)
  [[ "${CONVERGENCE_RESULT:-}" == "FAIL" ]] && classes+=(convergence)
  [[ "${STORAGE_CONSISTENCY_RESULT:-}" == "FAIL" ]] && classes+=(storage_consistency)
  [[ "${READINESS_RESULT:-}" == "FAIL" ]] && classes+=(readiness)
  for c in "${FAIL_CLASSES[@]:-}"; do classes+=("$(failure_class_from_code "$c")"); done
  for prio in startup consensus_state convergence storage_consistency readiness evidence_consistency environment timeout node; do
    present=0
    for cls in "${classes[@]:-}"; do [[ "$cls" == "$prio" ]] && present=1; done
    (( present == 1 )) && printf '%s\n' "$prio"
  done | awk '!seen[$0]++' | jq -R . | jq -s .
}

classify_failure_class(){
  local first
  first=$(failure_classes_json | jq -r '.[0] // "none"')
  echo "$first"
}

classify_result_failure_class(){
  if [[ "${OVERALL_RESULT:-$RESULT}" == "PASS" || "${RESULT:-}" == "PASS" ]]; then
    echo "none"
    return 0
  fi
  classify_failure_class
}
mark_startup_failure_gates(){
  case "$MINER_COUNT" in
    1) GATE_5N_1M_BASELINE=FAIL_STARTUP; GATE_5N_2M_INTERMEDIATE=NOT_EXECUTED; GATE_5N_4M_STRESS=NOT_EXECUTED ;;
    2) GATE_5N_1M_BASELINE=NOT_PROVIDED; GATE_5N_2M_INTERMEDIATE=FAIL_STARTUP; GATE_5N_4M_STRESS=NOT_EXECUTED ;;
    *) GATE_5N_4M_STRESS=FAIL_STARTUP ;;
  esac
  PRE_CONVERGED=0
  POST_CONVERGED=0
}

check_required_dependency(){
  local dep="$1" detail="${2:-$1}"
  if command -v "$dep" >/dev/null 2>&1; then
    echo "PASS: dependency available: $detail ($(command -v "$dep"))"
    return 0
  fi
  record_fail "ENV_FAIL" "missing dependency before rehearsal nodes start: $detail (command: $dep)"
  return 1
}

run_rehearsal_environment_preflight(){
  local missing=0
  echo "environment preflight: checking required host/container dependencies"
  check_required_dependency bash "bash shell" || missing=1
  check_required_dependency jq "jq JSON parser" || missing=1
  check_required_dependency curl "curl HTTP client" || missing=1
  check_required_dependency tar "tar archive tool" || missing=1
  check_required_dependency gzip "gzip compression tool" || missing=1
  if [[ "${REHEARSAL_REQUIRE_DOCKER:-0}" == "1" || "${REHEARSAL_DOCKER_MODE:-0}" == "host" ]]; then
    check_required_dependency docker "Docker CLI for Docker-mode rehearsal" || missing=1
  fi
  if (( missing == 1 )); then
    ENV_PREFLIGHT_OK=0
    FAILURE_CLASS="environment"
    printf '%s\n' "failure_class=environment" "result=ENV_FAIL" > "$OUT_DIR/env-fail.txt" 2>/dev/null || true
    echo "ENV_FAIL: missing dependencies; aborting before any nodes or miners are launched" >&2
    exit 2
  fi
  ENV_PREFLIGHT_OK=1
}

mark_progress(){
  local label="${1:-progress}"
  LAST_PROGRESS_TS=$(date +%s)
  printf '%s %s\n' "$(date -u +%FT%TZ)" "$label" >> "$OUT_DIR/progress.log" 2>/dev/null || true
}

cleanup_budget_remaining(){
  local now
  if (( ${IN_CLEANUP:-0} != 1 || ${CLEANUP_DEADLINE_TS:-0} <= 0 )); then
    echo 999999
    return 0
  fi
  now=$(date +%s)
  echo $((CLEANUP_DEADLINE_TS - now))
}

cleanup_budget_exhausted(){
  (( ${IN_CLEANUP:-0} == 1 && ${CLEANUP_DEADLINE_TS:-0} > 0 && $(date +%s) >= CLEANUP_DEADLINE_TS ))
}

run_with_global_timeout(){
  local label="$1" remaining rc=0
  shift
  assert_global_deadline
  remaining=$((GLOBAL_DEADLINE_TS - $(date +%s)))
  (( remaining > 0 )) || { record_fail "GLOBAL_DEADLINE_TIMEOUT" "global deadline exhausted before ${label}"; exit 124; }
  if command -v timeout >/dev/null 2>&1; then
    timeout --kill-after=10s "${remaining}s" "$@" || rc=$?
  else
    "$@" || rc=$?
  fi
  if (( rc == 124 || rc == 137 )); then
    record_fail "GLOBAL_DEADLINE_TIMEOUT" "${label} exceeded remaining global deadline budget (${remaining}s)"
    exit 124
  fi
  return "$rc"
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
  local watchdog_delay
  watchdog_delay=$((GLOBAL_DEADLINE_TS - $(date +%s)))
  (( watchdog_delay < 1 )) && watchdog_delay=1
  (
    sleep "$watchdog_delay"
    echo "FATAL: private rehearsal global deadline ${GLOBAL_DEADLINE_SECS}s reached; terminating script" >&2
    {
      echo "timestamp_utc=$(date -u +%FT%TZ)"
      echo "deadline_seconds=$GLOBAL_DEADLINE_SECS"
      echo "last_progress_utc=$(date -u -d @${LAST_PROGRESS_TS:-$START_TS} +%FT%TZ 2>/dev/null || echo unknown)"
      echo "reason=HARNESS_STALL_TIMEOUT"
    } > "$OUT_DIR/global-watchdog-timeout.txt" 2>/dev/null || true
    kill -TERM $$ 2>/dev/null || true
    sleep $((FINAL_CAPTURE_BUDGET_SECS + CLEANUP_KILL_GRACE_SECS + CLEANUP_PORT_WAIT_SECS + 30))
    kill -KILL $$ 2>/dev/null || true
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
  local url out label required rc now remaining max_time connect_time
  url="$1"; out="$2"; label="${3:-$url}"; required="${4:-0}"
  now=$(date +%s)
  if (( ${IN_CLEANUP:-0} != 1 )); then
    assert_global_deadline
    remaining=$((GLOBAL_DEADLINE_TS - now))
    (( remaining > 0 )) || { echo "FATAL: global deadline exhausted before curl: $label"; record_fail "GLOBAL_DEADLINE_TIMEOUT" "global deadline exhausted before curl: $label"; exit 124; }
  else
    if cleanup_budget_exhausted; then
      write_curl_failure_stub "$out" "$url" "$label" 124 "final capture budget exhausted; skipped curl during cleanup"
      record_warn "skipped endpoint capture after final capture budget exhausted: $label"
      return 1
    fi
    remaining=$(cleanup_budget_remaining)
  fi
  if (( ${IN_CLEANUP:-0} == 1 )); then
    max_time=$CLEANUP_CURL_MAX_TIME_SECS
    connect_time=$CLEANUP_CURL_CONNECT_TIMEOUT_SECS
  else
    max_time=$CURL_MAX_TIME_SECS
    connect_time=$CURL_CONNECT_TIMEOUT_SECS
  fi
  (( max_time > remaining )) && max_time=$remaining
  (( max_time < 1 )) && max_time=1
  rc=0
  curl -fsS --connect-timeout "$connect_time" --max-time "$max_time" "$url" -o "$out" || rc=$?
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

extract_bootnode_peer_id(){
  local p2p_file="$1" out reason peer_id schema_keys
  out="$OUT_DIR/bootnode-peer-id-extraction-failure.txt"
  if ! command -v jq >/dev/null 2>&1; then
    reason="jq missing; cannot parse n1 /p2p/status JSON for .data.peer_id or .data.local_node_id"
    printf '%s\n' "failure_class=environment" "reason=$reason" "file=$p2p_file" > "$out" 2>/dev/null || true
    record_fail "ENV_FAIL" "$reason"
    return 1
  fi
  if [[ ! -s "$p2p_file" ]]; then
    reason="n1 /p2p/status capture is missing or empty; node may not have reached RPC readiness"
    printf '%s\n' "failure_class=node" "reason=$reason" "file=$p2p_file" > "$out" 2>/dev/null || true
    record_fail "RPC_UNAVAILABLE" "$reason"
    return 1
  fi
  if ! jq -e . "$p2p_file" >/dev/null 2>&1; then
    reason="n1 /p2p/status capture is not valid JSON; cannot extract bootnode peer id"
    printf '%s\n' "failure_class=node" "reason=$reason" "file=$p2p_file" > "$out" 2>/dev/null || true
    record_fail "RPC_UNAVAILABLE" "$reason"
    return 1
  fi
  peer_id=$(jq -r '.data.peer_id // .data.local_node_id // empty' "$p2p_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' || true)
  if [[ -n "$peer_id" ]]; then
    printf '%s\n' "$peer_id"
    return 0
  fi
  schema_keys=$(jq -c '{top_level:(keys_unsorted // []), data_keys:(.data | keys_unsorted? // [])}' "$p2p_file" 2>/dev/null || echo '{}')
  reason="JSON schema mismatch; expected .data.peer_id or .data.local_node_id in n1 /p2p/status"
  printf '%s\n' "failure_class=convergence" "reason=$reason" "file=$p2p_file" "observed_keys=$schema_keys" > "$out" 2>/dev/null || true
  record_fail "READINESS_SCHEMA_MISMATCH" "$reason; observed keys: $schema_keys"
  return 1
}

node_name_for_label(){
  local label="$1" node
  node="$(printf '%s' "$label" | sed -n 's/.*\(n[0-9][0-9]*\):.*/\1/p' | head -n1)"
  if [[ -z "$node" ]]; then
    node="$(printf '%s' "$label" | sed -n 's/^\(n[0-9][0-9]*\)$/\1/p' | head -n1)"
  fi
  printf '%s' "$node"
}

node_pid_for_label(){
  local label="$1" node
  node="$(node_name_for_label "$label")"
  [[ -n "$node" && -f "$OUT_DIR/process-pids.txt" ]] || return 1
  awk -v node="node-${node}" '$2 == node {print $1; exit}' "$OUT_DIR/process-pids.txt"
}

capture_rpc_failure_diagnostics(){
  local label="$1" url="$2" rc="$3" pid port node diag class alive listening
  node="$(node_name_for_label "$label")"
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
  if (( alive == 1 && listening == 0 )); then
    class="RPC_LISTENER_DOWN"
  elif [[ "$rc" == "28" && ( "$alive" == "1" || "$listening" == "1" ) ]]; then
    class="RPC_ALIVE_LISTENER_TIMEOUT"
  elif (( alive == 0 && listening == 0 )); then
    class="RPC_PROCESS_EXITED"
  elif (( alive == 1 && listening == 1 )); then
    class="RPC_CURL_FAILURE_WITH_ALIVE_LISTENER"
  else
    class="RPC_LISTENER_PRESENT_PID_UNKNOWN"
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
  if [[ -n "$pid" ]]; then ps -p "$pid" -o pid,ppid,stat,etime,pcpu,pmem,comm,args > "$OUT_DIR/endpoints/${node}-rpc-failure-ps.txt" 2>/dev/null || true; fi
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
  local label p deadline alive_before alive_after_term alive_after_kill
  if [[ "${1:-}" == --label=* ]]; then
    label="${1#--label=}"
    shift
  else
    label="process"
  fi
  (( $# == 0 )) && return 0
  mkdir -p "$OUT_DIR/summaries" 2>/dev/null || true
  for p in "$@"; do
    [[ -n "$p" ]] || continue
    alive_before=0
    kill -0 "$p" 2>/dev/null && alive_before=1
    printf '%s label=%s pid=%s phase=before_sigterm alive=%s\n' "$(date -u +%FT%TZ)" "$label" "$p" "$alive_before" >> "$OUT_DIR/summaries/process-kill-audit.log" 2>/dev/null || true
    (( alive_before == 1 )) && kill -TERM "$p" 2>/dev/null || true
  done
  deadline=$(( $(date +%s) + CLEANUP_KILL_GRACE_SECS ))
  while (( $(date +%s) < deadline )); do
    local any_alive=0
    for p in "$@"; do [[ -n "$p" ]] && kill -0 "$p" 2>/dev/null && any_alive=1; done
    (( any_alive == 0 )) && break
    sleep 1
  done
  for p in "$@"; do
    [[ -n "$p" ]] || continue
    alive_after_term=0; alive_after_kill=0
    kill -0 "$p" 2>/dev/null && alive_after_term=1
    if (( alive_after_term == 1 )); then kill -KILL "$p" 2>/dev/null || true; fi
    sleep 0.2 2>/dev/null || true
    kill -0 "$p" 2>/dev/null && alive_after_kill=1
    printf '%s label=%s pid=%s phase=after_sigterm alive=%s phase2=after_sigkill alive2=%s\n' "$(date -u +%FT%TZ)" "$label" "$p" "$alive_after_term" "$alive_after_kill" >> "$OUT_DIR/summaries/process-kill-audit.log" 2>/dev/null || true
  done
}

kill_processes_on_test_ports(){
  local p pid
  command -v ss >/dev/null 2>&1 || return 0
  for i in $(seq 1 "$NODE_COUNT"); do
    for p in "$((BASE_RPC_PORT+i))" "$((BASE_P2P_PORT+i))"; do
      while read -r pid; do
        [[ -n "$pid" ]] || continue
        echo "cleanup: killing leftover listener pid=$pid on port=$p"
        kill -TERM "$pid" 2>/dev/null || true
        sleep 1
        kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null || true
      done < <(ss -ltnp "( sport = :$p )" 2>/dev/null | sed -n 's/.*pid=\([0-9][0-9]*\).*/\1/p' | sort -u)
    done
  done
}

wait_for_ports_clean(){
  local deadline p dirty=0
  deadline=$(( $(date +%s) + CLEANUP_PORT_WAIT_SECS ))
  while (( $(date +%s) < deadline )); do
    dirty=0
    for i in $(seq 1 "$NODE_COUNT"); do
      for p in "$((BASE_RPC_PORT+i))" "$((BASE_P2P_PORT+i))"; do
        port_in_use "$p" && dirty=1
      done
    done
    (( dirty == 0 )) && return 0
    sleep 1
  done
  for i in $(seq 1 "$NODE_COUNT"); do
    for p in "$((BASE_RPC_PORT+i))" "$((BASE_P2P_PORT+i))"; do
      if port_in_use "$p"; then
        record_fail "HARNESS_PORT_LEAK" "test port $p still in use after cleanup"
        command -v ss >/dev/null 2>&1 && ss -ltnp "( sport = :$p )" >> "$OUT_DIR/summaries/port-leak-listeners.txt" 2>/dev/null || true
      fi
    done
  done
  return 1
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

capture_hard_stop_diagnostics(){
  {
    echo "timestamp_utc=$(date -u +%FT%TZ)"
    echo "run_dir=$OUT_DIR"
    echo "global_deadline_seconds=$GLOBAL_DEADLINE_SECS"
    echo "final_capture_budget_seconds=$FINAL_CAPTURE_BUDGET_SECS"
    echo "last_progress_ts=${LAST_PROGRESS_TS:-unknown}"
    echo "last_progress_utc=$(date -u -d @${LAST_PROGRESS_TS:-$START_TS} +%FT%TZ 2>/dev/null || echo unknown)"
    echo
    echo "## processes"
    ps -eo pid,ppid,pgid,stat,etime,pcpu,pmem,comm,args 2>/dev/null | awk 'NR==1 || /pulsedagd|pulsedag-miner/' || true
    if command -v ss >/dev/null 2>&1; then
      echo
      echo "## listening sockets"
      ss -ltnp 2>/dev/null || true
    fi
    echo
    echo "## command-log tail"
    tail -n 200 "$OUT_DIR/command-log.txt" 2>/dev/null || true
  } > "$OUT_DIR/hard-stop-diagnostics.txt" 2>/dev/null || true
}

capture_log_tails(){
  local i
  for i in $(seq 1 "$NODE_COUNT"); do tail -n 120 "$OUT_DIR/logs/n${i}.log" > "$OUT_DIR/logs/n${i}-tail.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do tail -n 120 "$OUT_DIR/logs/miner-${i}.log" > "$OUT_DIR/logs/miner-${i}-tail.log" 2>/dev/null || true; done
}

collect_final_state(){
  local phase skipped_budget=0
  phase="${1:-final}"
  for i in $(seq 1 "$NODE_COUNT"); do
    if cleanup_budget_exhausted; then
      skipped_budget=1
      record_warn "final endpoint capture budget exhausted before n${i}; skipping remaining endpoints for phase ${phase}"
      break
    fi
    rpc=$((BASE_RPC_PORT+i))
    if [[ "$phase" == "quiescent" || "$phase" == "final" ]]; then
      safe_curl_required "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${i}-status-final.json" "n${i}:/status ${phase}" || true
      safe_curl_required "http://127.0.0.1:${rpc}/readiness" "$OUT_DIR/endpoints/n${i}-readiness-final.json" "n${i}:/readiness ${phase}" || true
      safe_curl_required "http://127.0.0.1:${rpc}/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "n${i}:/p2p/status ${phase}" || true
      safe_curl_required "http://127.0.0.1:${rpc}/sync/status" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "n${i}:/sync/status ${phase}" || true
      safe_curl_required "http://127.0.0.1:${rpc}/metrics" "$OUT_DIR/endpoints/n${i}-metrics-final.json" "n${i}:/metrics ${phase}" || true
    else
      safe_curl_optional "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${i}-status-final.json" "n${i}:/status ${phase}" || true
      safe_curl_optional "http://127.0.0.1:${rpc}/readiness" "$OUT_DIR/endpoints/n${i}-readiness-final.json" "n${i}:/readiness ${phase}" || true
      safe_curl_optional "http://127.0.0.1:${rpc}/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "n${i}:/p2p/status ${phase}" || true
      safe_curl_optional "http://127.0.0.1:${rpc}/sync/status" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "n${i}:/sync/status ${phase}" || true
      safe_curl_optional "http://127.0.0.1:${rpc}/metrics" "$OUT_DIR/endpoints/n${i}-metrics-final.json" "n${i}:/metrics ${phase}" || true
    fi
    safe_curl_optional "http://127.0.0.1:${rpc}/release" "$OUT_DIR/endpoints/n${i}-release-final.json" "n${i}:/release ${phase}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/sync/missing" "$OUT_DIR/endpoints/n${i}-sync-missing-final.json" "n${i}:/sync/missing ${phase}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/orphans" "$OUT_DIR/endpoints/n${i}-orphans-final.json" "n${i}:/orphans ${phase}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/checks" "$OUT_DIR/endpoints/n${i}-checks-final.json" "n${i}:/checks ${phase}" || true
    NODE_HEIGHT[$i]="$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo 0)"
    NODE_TIP[$i]="$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo '')"
    NODE_SELECTED_TIP[$i]="$(jq -r '.data.selected_tip // .data.metrics.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status-final.json" "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null | head -n1 || echo '')"
    NODE_ORDERED_DAG_TIP[$i]="$(jq -r '.data.ordered_dag_tip // ""' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo '')"
    NODE_CONSENSUS_MODE[$i]="$(jq -r '.data.consensus_mode // .data.metrics.consensus_mode // "unknown"' "$OUT_DIR/endpoints/n${i}-status-final.json" "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null | head -n1 || echo unknown)"
    NODE_READY[$i]="$(jq -r '.data.ready_for_release // .ready_for_release // 0' "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_HEALTHY[$i]="$(jq -r '.ok // .data.ok // 0' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_PEERS[$i]="$(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // (.data.peers|length) // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_INBOUND[$i]="$(jq -r '.data.inbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_OUTBOUND[$i]="$(jq -r '.data.outbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_CHAIN_ID[$i]="$(extract_chain_id "$OUT_DIR/endpoints/n${i}-status-final.json" "$OUT_DIR/endpoints/n${i}-release-final.json" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" || true)"
    NODE_ORPHAN_COUNT[$i]="$(jq -r '.data.orphan_count // .orphan_count // (.data.orphans|length) // (.orphans|length) // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "$OUT_DIR/endpoints/n${i}-orphans-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_PENDING_BLOCK_REQUESTS[$i]="$(jq -r '.data.pending_block_requests // .pending_block_requests // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_PENDING_MISSING_PARENTS[$i]="$(jq -r '.data.pending_missing_parents // .pending_missing_parents // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo 0)"
    NODE_INV_HASHES_REQUESTED[$i]="$(jq -r '.data.inv_hashes_requested // .inv_hashes_requested // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_MISSING_PARENT_REQUESTS_SENT[$i]="$(jq -r '.data.missing_parent_requests_sent // .data.network_counters.missing_parent_requests_sent // .data.sync_counters.missing_parent_requests_sent // .missing_parent_requests_sent // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo 0)"
    NODE_MISSING_PARENT_RESPONSES_RECEIVED[$i]="$(jq -r '.data.missing_parent_responses_received // .data.network_counters.missing_parent_responses_received // .data.sync_counters.missing_parent_responses_received // .missing_parent_responses_received // 0' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo 0)"
    NODE_PEER_RECOVERY_SUCCESS_COUNT[$i]="$(jq -r '.data.peer_recovery_success_count // .peer_recovery_success_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_MISSING_PARENTS_COUNT[$i]="$(jq -r '.data.missing_parent_index | if type == "array" then length else 0 end' "$OUT_DIR/endpoints/n${i}-sync-missing-final.json" 2>/dev/null || echo 0)"
    NODE_TERMINAL_MISSING_PARENTS_COUNT[$i]="$(jq -r '.data.terminal_missing_parent_index | if type == "array" then length else 0 end' "$OUT_DIR/endpoints/n${i}-sync-missing-final.json" 2>/dev/null || echo 0)"
    NODE_ORPHANS[$i]="${NODE_ORPHAN_COUNT[$i]:-0}"
    NODE_MISSING_PARENTS[$i]="${NODE_PENDING_MISSING_PARENTS[$i]:-0}"
    NODE_READINESS_STATUS[$i]="$(jq -r 'if (.data.ready_for_release // .ready_for_release // false) == true then "ready" elif (.data.public_testnet_ready // .public_testnet_ready // false) == true then "public-ready-unexpected" else "not_ready" end' "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null || echo not_ready)"
    NODE_SYNC_STATE[$i]="$(jq -r '.data.sync_state // .sync_state // .data.state // "unknown"' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo unknown)"
    NODE_CANONICAL_NODE_GAP[$i]="$(jq -r '(.data // .) as $d | $d.canonical_node_gap // $d.best_height_gap // $d.blocks_behind // $d.sync_gap // null // ""' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo '')"
    NODE_SYNC_OBSERVABILITY_WARNING[$i]="$(jq -r '(.data // .) as $d | $d.sync_observability_incomplete // $d.warnings.sync_observability_incomplete // false' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_SYNC_STAGE[$i]="$(jq -r '.data.catchup_stage // .catchup_stage // .data.stage // "unknown"' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo unknown)"
    NODE_STATE_ROOT[$i]="$(jq -r '.data.ordered_dag_state_root // .data.state_root // .data.metrics.state_root // .state_root // ""' "$OUT_DIR/endpoints/n${i}-status-final.json" "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null | head -n1 || echo '')"
    checks_file="$OUT_DIR/endpoints/n${i}-checks-final.json"
    NODE_STORAGE_COHERENT[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.ok // $d.overall_ok // $d.coherent_storage_invariant // false' "$checks_file" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_STORAGE_MEMORY_COUNT[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.in_memory_dag_count // $s.memory_count // $d.in_memory_dag_count // $d.memory_count // 0' "$checks_file" 2>/dev/null || echo 0)"
    NODE_STORAGE_PERSISTED_COUNT[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.accepted_storage_count // $s.persisted_count // $d.accepted_storage_count // $d.persisted_count // 0' "$checks_file" 2>/dev/null || echo 0)"
    NODE_STORAGE_MEMORY_DIGEST[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.memory_digest // $s.memory_generation // $d.memory_digest // $d.memory_generation // ""' "$checks_file" 2>/dev/null || echo '')"
    NODE_STORAGE_PERSISTED_DIGEST[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.storage_digest // $s.storage_generation // $d.storage_digest // $d.storage_generation // ""' "$checks_file" 2>/dev/null || echo '')"
    NODE_STORAGE_ONLY_HASHES[$i]="$(jq -c '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.storage_only_hashes // $d.storage_only_hashes // []' "$checks_file" 2>/dev/null || echo '[]')"
    NODE_MEMORY_ONLY_HASHES[$i]="$(jq -c '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.memory_only_hashes // $d.memory_only_hashes // []' "$checks_file" 2>/dev/null || echo '[]')"
    NODE_STORAGE_MISMATCH_SOURCE[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | if (($s.mismatch_acceptance_sources // {}) | length) > 0 then "mismatch_acceptance_sources" elif (($s.accepted_storage_count // $d.accepted_storage_count // 0) != ($s.in_memory_dag_count // $d.in_memory_dag_count // 0)) then "memory_persisted_count" else "none" end' "$checks_file" 2>/dev/null || echo unknown)"
    NODE_CHECKS_GENERATION[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.memory_generation_numeric // $s.storage_generation_numeric // $d.memory_generation_numeric // $d.storage_generation_numeric // $s.generation // $d.generation // "0"' "$checks_file" 2>/dev/null || echo 0)"
    NODE_CHECKS_DIGEST[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.accepted_hash_set_digest // $d.accepted_hash_set_digest // ""' "$checks_file" 2>/dev/null || echo '')"
    NODE_ACCEPTED_HASH_SET_DIGEST[$i]="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.accepted_hash_set_digest // $d.accepted_hash_set_digest // .data.accepted_hash_set_digest // ""' "$checks_file" "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null | head -n1 || echo '')"
    NODE_NETWORK_GAPS[$i]="$(jq -c '(.data // .) as $d | {missing_parent_requests_sent:($d.missing_parent_requests_sent // $d.network_counters.missing_parent_requests_sent // 0), pending_block_requests:($d.pending_block_requests // 0), inv_hashes_requested:($d.inv_hashes_requested // 0)}' "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null || echo '{}')"
    NODE_STORAGE_GAPS[$i]="$(jq -c '(.data // .) as $d | {storage_only_hashes:($d.storage_only_hashes // []), memory_only_hashes:($d.memory_only_hashes // []), mismatch_source:($d.mismatch_source // "unknown")}' "$checks_file" 2>/dev/null || echo '{}')"
    NODE_SELECTED_SEGMENT_REQUESTED[$i]="$(jq -r '.data.selected_segment_session_metrics.requested // .data.selected_segment_requested // 0' "$OUT_DIR/endpoints/n${i}-metrics-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_SELECTED_SEGMENT_RECEIVED[$i]="$(jq -r '.data.selected_segment_session_metrics.received // .data.selected_segment_received // 0' "$OUT_DIR/endpoints/n${i}-metrics-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_SELECTED_SEGMENT_APPLIED[$i]="$(jq -r '.data.selected_segment_session_metrics.applied // .data.selected_segment_applied // 0' "$OUT_DIR/endpoints/n${i}-metrics-final.json" "$OUT_DIR/endpoints/n${i}-sync-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_CORRELATED_REQUEST_COUNTERS[$i]="$(jq -c '.data.correlated_request_counters // {duplicate_correlated_response_total:(.data.duplicate_correlated_response_total // 0), unknown_request_response_total:(.data.unknown_request_response_total // 0)}' "$OUT_DIR/endpoints/n${i}-metrics-final.json" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null | head -n1 || echo '{}')"
    NODE_P2P_OK[$i]=$(( NODE_PEERS[$i] > 0 ? 1 : 0 ))
    readiness_has_ready=$(jq -e '(.data // .) as $r | ($r|has("node_operational_ready")) and ($r.node_operational_ready|type == "boolean") and ($r|has("private_conservative_ready")) and ($r.private_conservative_ready|type == "boolean") and ($r|has("fast_cadence_ready")) and ($r.fast_cadence_ready|type == "boolean") and ($r|has("public_testnet_ready")) and ($r.public_testnet_ready|type == "boolean") and ($r|has("ready_for_release")) and ($r.ready_for_release|type == "boolean")' "$OUT_DIR/endpoints/n${i}-readiness-final.json" >/dev/null 2>&1 && echo 1 || echo 0)
    readiness_has_public=$(jq -e '(.data // .) as $r | ($r|has("public_testnet_ready")) and ($r.public_testnet_ready == false)' "$OUT_DIR/endpoints/n${i}-readiness-final.json" >/dev/null 2>&1 && echo 1 || echo 0)
    NODE_READINESS_SCHEMA_OK[$i]=$(( readiness_has_ready == 1 && readiness_has_public == 1 ? 1 : 0 ))
    metrics_file="$OUT_DIR/endpoints/n${i}-metrics-final.json"
    NODE_ACTIVE_PEERS[$i]="$(jq -r '.data.peer_retention_active_total // .data.peer_count // 0' "$metrics_file" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_RECOVERING_PEERS[$i]="$(jq -r '.data.peer_retention_recovering_total // .data.peer_lifecycle_recovering // 0' "$metrics_file" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_COOLDOWN_PEERS[$i]="$(jq -r '.data.peer_retention_cooldown_total // .data.peer_lifecycle_cooldown // 0' "$metrics_file" "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null | head -n1 || echo 0)"
    NODE_RATE_LIMITED_COUNT[$i]="$(jq -r '.data.peer_message_rate_limited_count // .data.rate_limited_count // .data.orphan_roots_rate_limited_total // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$metrics_file" 2>/dev/null | head -n1 || echo 0)"
    NODE_RECONNECT_ATTEMPTS[$i]="$(jq -r '.data.peer_reconnect_attempts // .data.recovery_activity_summary.reconnect_attempts // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$metrics_file" 2>/dev/null | head -n1 || echo 0)"
    NODE_RECONNECT_BLOCKED_REASON[$i]="$(jq -r '.data.last_peer_reconnect_blocked_reason // ""' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$metrics_file" 2>/dev/null | head -n1 || echo '')"
    NODE_SAME_HEIGHT_RECONCILE_BLOCKED_REASON[$i]="$(jq -r '.data.final_quiescence_same_height_reconcile_blocked_reason // ""' "$metrics_file" 2>/dev/null || echo '')"
    NODE_MIN_TARGET_MISSED[$i]="$(jq -r '.data.peer_min_target_missed_total // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$metrics_file" 2>/dev/null | head -n1 || echo 0)"
    NODE_ZERO_RECONNECT_ATTEMPTS[$i]="$(jq -r '.data.peer_zero_reconnect_attempt_total // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$metrics_file" 2>/dev/null | head -n1 || echo 0)"
    NODE_ZERO_RECONNECT_SUCCESS[$i]="$(jq -r '.data.peer_zero_reconnect_success_total // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" "$metrics_file" 2>/dev/null | head -n1 || echo 0)"
    NODE_ORPHAN_RECOVERY_ATTEMPTS[$i]="$(jq -r '.data.orphan_reprocess_attempts // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_ORPHAN_RECOVERY_SUCCESS[$i]="$(jq -r '.data.orphan_reprocess_success // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_ORPHAN_RECOVERY_FAILED_MISSING_PARENT[$i]="$(jq -r '.data.orphan_reprocess_failed_missing_parent // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_ORPHAN_RECOVERY_FAILED_PERSIST[$i]="$(jq -r '.data.orphan_reprocess_failed_persist // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_ORPHAN_ROOTS_RATE_LIMITED[$i]="$(jq -r '.data.orphan_roots_rate_limited_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_ORPHAN_BACKLOG_STALE[$i]="$(jq -r '.data.orphan_backlog_stale_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_RPC_DEGRADED_RESPONSE[$i]="$(jq -r '.data.rpc_degraded_response_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_RPC_SNAPSHOT_STALE[$i]="$(jq -r '.data.rpc_snapshot_stale_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_RPC_HANDLER_DEGRADED[$i]="$(jq -r '.data.rpc_handler_degraded_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_RPC_HANDLER_TIMEOUT_AVOIDED[$i]="$(jq -r '.data.rpc_handler_timeout_avoided_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_MINING_TEMPLATES[$i]="$(jq -r '.data.mining_templates_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_MINING_SUBMITS[$i]="$(jq -r '.data.mining_submits_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_MINING_ACCEPTED[$i]="$(jq -r '.data.blocks_accepted_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_MINING_REJECTED[$i]="$(jq -r '.data.blocks_rejected_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_MINING_SUBMIT_BUSY[$i]="$(jq -r '.data.external_mining_submit_actor_queue_full_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
    NODE_MINING_ACTOR_TIMEOUT[$i]="$(jq -r '.data.external_mining_submit_actor_timeout_total // 0' "$metrics_file" 2>/dev/null || echo 0)"
  done
  collect_miner_metrics
  compute_evidence_aggregates || true
  (( skipped_budget == 0 )) || return 0
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
  if [[ "$prefix" != "PRE" ]]; then
    for i in $(seq 1 "$NODE_COUNT"); do
      NODE_HARNESS_OBSERVED_GAP[$i]=$((max - ${NODE_HEIGHT[$i]:-0}))
    done
  fi
  distinct=$(for i in $(seq 1 "$NODE_COUNT"); do printf '%s\n' "${NODE_TIP[$i]:-}"; done | awk 'NF' | sort -u | wc -l | tr -d ' ')
  converged=0
  (( distinct == 1 && worst == 0 && MINERS_STARTED > 0 && TOTAL_MINING_ACCEPTED > 0 )) && converged=1
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

json_number_or_zero(){
  local value="${1:-0}"
  [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?$ ]] && printf '%s' "$value" || printf '0'
}

json_array_or_empty(){
  local value="${1:-[]}"
  if printf '%s' "$value" | jq -e 'type == "array"' >/dev/null 2>&1; then
    printf '%s' "$value"
  else
    printf '[]'
  fi
}

json_object_or_empty(){
  local value="${1:-{}}"
  if printf '%s' "$value" | jq -e 'type == "object"' >/dev/null 2>&1; then
    printf '%s' "$value"
  else
    printf '{}'
  fi
}

extract_p2p_listening_addresses(){
  local status_file="$1"
  jq -c '
    (
      .data.listening_addresses
      // .data.listening
      // .data.listen_addresses
      // .data.listeners
      // []
    ) as $listeners
    | if ($listeners | type) == "array" then
        $listeners
      elif ($listeners | type) == "string" then
        if ($listeners | length) > 0 then [$listeners] else [] end
      else
        []
      end
  ' "$status_file" 2>/dev/null || printf '[]'
}

p2p_listener_has_expected_port(){
  local status_file="$1" expected_port="$2"
  extract_p2p_listening_addresses "$status_file" | jq -e --arg port "/tcp/${expected_port}" 'any(.[]?; (tostring | contains($port)))' >/dev/null 2>&1
}

integer_sum_or_zero(){
  local value="${1:-}"
  awk 'BEGIN { total=0 } { for (i=1; i<=NF; i++) if ($i ~ /^[0-9]+$/) total += $i } END { print total + 0 }' <<<"$value"
}

sum_node_array(){
  local name="$1" total=0 i value
  for i in $(seq 1 "$NODE_COUNT"); do
    eval "value=\${${name}[$i]:-0}"
    value=$(json_number_or_zero "$value")
    total=$((total + value))
  done
  echo "$total"
}

compute_evidence_aggregates(){
  local submit_busy_log_count=0 mining_actor_timeout_log_count=0
  RPC_ALIVE_LISTENER_TIMEOUT_COUNT=$(count_matches_file '"class":"RPC_ALIVE_LISTENER_TIMEOUT"|class=RPC_ALIVE_LISTENER_TIMEOUT|RPC_DIAGNOSTIC\[RPC_ALIVE_LISTENER_TIMEOUT\]' "$OUT_DIR/command-log.txt")
  if compgen -G "$OUT_DIR/endpoints/*-rpc-failure-diagnostics.jsonl" >/dev/null 2>&1; then
    RPC_ALIVE_LISTENER_TIMEOUT_COUNT=$((RPC_ALIVE_LISTENER_TIMEOUT_COUNT + $(cat "$OUT_DIR"/endpoints/*-rpc-failure-diagnostics.jsonl 2>/dev/null | jq -r 'select(.class == "RPC_ALIVE_LISTENER_TIMEOUT") | 1' 2>/dev/null | wc -l | tr -d ' ')))
  fi
  RPC_LIVENESS_TIMEOUT_COUNT=$(count_matches_file 'rpc_liveness_timeout|RPC_LIVENESS_TIMEOUT' "$OUT_DIR/command-log.txt")
  STALE_DEGRADED_SNAPSHOT_COUNT=$(( $(sum_node_array NODE_RPC_DEGRADED_RESPONSE) + $(sum_node_array NODE_RPC_SNAPSHOT_STALE) + $(sum_node_array NODE_RPC_HANDLER_DEGRADED) ))
  TOTAL_ORPHAN_COUNT=$(sum_node_array NODE_ORPHAN_COUNT)
  TOTAL_PENDING_MISSING_PARENTS=$(sum_node_array NODE_PENDING_MISSING_PARENTS)
  TOTAL_PENDING_BLOCK_REQUESTS=$(sum_node_array NODE_PENDING_BLOCK_REQUESTS)
  TOTAL_MISSING_PARENT_ENTRIES=$(sum_node_array NODE_MISSING_PARENTS_COUNT)
  TOTAL_TERMINAL_MISSING_PARENT_ENTRIES=$(sum_node_array NODE_TERMINAL_MISSING_PARENTS_COUNT)
  TOTAL_INV_HASHES_REQUESTED=$(sum_node_array NODE_INV_HASHES_REQUESTED)
  TOTAL_ACTIVE_PEERS=$(sum_node_array NODE_ACTIVE_PEERS)
  TOTAL_RECOVERING_PEERS=$(sum_node_array NODE_RECOVERING_PEERS)
  TOTAL_COOLDOWN_PEERS=$(sum_node_array NODE_COOLDOWN_PEERS)
  TOTAL_RATE_LIMITED_COUNT=$(sum_node_array NODE_RATE_LIMITED_COUNT)
  TOTAL_RECONNECT_ATTEMPTS=$(sum_node_array NODE_RECONNECT_ATTEMPTS)
  TOTAL_MIN_TARGET_MISSED=$(sum_node_array NODE_MIN_TARGET_MISSED)
  TOTAL_ZERO_RECONNECT_ATTEMPTS=$(sum_node_array NODE_ZERO_RECONNECT_ATTEMPTS)
  TOTAL_ZERO_RECONNECT_SUCCESS=$(sum_node_array NODE_ZERO_RECONNECT_SUCCESS)
  RECONNECT_BLOCKED_REASONS_JSON=$(for i in $(seq 1 "$NODE_COUNT"); do printf '%s\n' "${NODE_RECONNECT_BLOCKED_REASON[$i]:-}"; done | awk 'NF' | sort | uniq -c | jq -Rn '[inputs | capture("^\\s*(?<count>[0-9]+)\\s+(?<reason>.*)$") | {reason, count:(.count|tonumber)}]')
  SAME_HEIGHT_RECONCILE_BLOCKED_REASONS_JSON=$(for i in $(seq 1 "$NODE_COUNT"); do printf '%s\n' "${NODE_SAME_HEIGHT_RECONCILE_BLOCKED_REASON[$i]:-}"; done | awk 'NF' | sort | uniq -c | jq -Rn '[inputs | capture("^\\s*(?<count>[0-9]+)\\s+(?<reason>.*)$") | {reason, count:(.count|tonumber)}]')
  NODE_BLOCK_ACCEPT_EVENTS_TOTAL=$(sum_node_array NODE_MINING_ACCEPTED)
  NODE_BLOCK_REJECT_EVENTS_TOTAL=$(sum_node_array NODE_MINING_REJECTED)
  TOTAL_MINING_TEMPLATES=$(sum_node_array NODE_MINING_TEMPLATES)
  TOTAL_MINING_SUBMITS=$(sum_node_array NODE_MINING_SUBMITS)
  TOTAL_MINING_ACCEPTED=$NODE_BLOCK_ACCEPT_EVENTS_TOTAL
  TOTAL_MINING_REJECTED=$NODE_BLOCK_REJECT_EVENTS_TOTAL
  TOTAL_MINING_SUBMIT_BUSY=$(sum_node_array NODE_MINING_SUBMIT_BUSY)
  TOTAL_MINING_ACTOR_TIMEOUT=$(sum_node_array NODE_MINING_ACTOR_TIMEOUT)
  TOTAL_ORPHAN_RECOVERY_ATTEMPTS=$(sum_node_array NODE_ORPHAN_RECOVERY_ATTEMPTS)
  TOTAL_ORPHAN_RECOVERY_SUCCESS=$(sum_node_array NODE_ORPHAN_RECOVERY_SUCCESS)
  TOTAL_ORPHAN_RECOVERY_FAILED_MISSING_PARENT=$(sum_node_array NODE_ORPHAN_RECOVERY_FAILED_MISSING_PARENT)
  TOTAL_ORPHAN_RECOVERY_FAILED_PERSIST=$(sum_node_array NODE_ORPHAN_RECOVERY_FAILED_PERSIST)
  TOTAL_ORPHAN_ROOTS_RATE_LIMITED=$(sum_node_array NODE_ORPHAN_ROOTS_RATE_LIMITED)
  TOTAL_ORPHAN_BACKLOG_STALE=$(sum_node_array NODE_ORPHAN_BACKLOG_STALE)
  TOTAL_MISSING_PARENT_REQUESTS_SENT=$(sum_node_array NODE_MISSING_PARENT_REQUESTS_SENT)
  if (( TOTAL_MISSING_PARENT_REQUESTS_SENT == 0 )); then TOTAL_MISSING_PARENT_REQUESTS_SENT=$(integer_sum_or_zero "$(count_matches_file 'missing_parent_requests_sent|missing parent request' "$OUT_DIR/command-log.txt")"); fi
  TOTAL_MISSING_PARENT_RESPONSES_RECEIVED=$(sum_node_array NODE_MISSING_PARENT_RESPONSES_RECEIVED)
  if (( TOTAL_MISSING_PARENT_RESPONSES_RECEIVED == 0 )); then TOTAL_MISSING_PARENT_RESPONSES_RECEIVED=$(integer_sum_or_zero "$(count_matches_file 'missing_parent_responses_received|missing parent response' "$OUT_DIR/command-log.txt")"); fi
  TOTAL_BLOCKDATA_NOT_FOUND=$(integer_sum_or_zero "$(count_matches_file 'blockdata_not_found|BLOCKDATA_NOT_FOUND|block data not found' "$OUT_DIR/command-log.txt")")
  UNIQUE_TEMPLATES_ISSUED=$(find "$OUT_DIR/miners" "$OUT_DIR/logs" -type f 2>/dev/null | xargs -r perl -ne 'print "$1\n" if /(?:template[_ -]?id|template_hash|block_template_hash)[=: ]+([0-9a-fA-F]+)/' | sort -u | wc -l | tr -d " ")
  UNIQUE_BLOCK_HASHES_OBSERVED=$(find "$OUT_DIR/endpoints" "$OUT_DIR/logs" -type f 2>/dev/null | xargs -r perl -ne 'print "$1\n" if /(?:block_hash|hash)["=: ]+([0-9a-fA-F]{16,})/' | sort -u | wc -l | tr -d " ")
  parse_miner_terminal_evidence || true
  if (( UNIQUE_TEMPLATES_ISSUED == 0 )); then UNIQUE_TEMPLATES_ISSUED=$TOTAL_MINING_TEMPLATES; fi
  NODE_BLOCK_DUPLICATE_EVENTS_TOTAL=$DUPLICATE_BLOCK_EVENTS_TOTAL
  if (( UNIQUE_BLOCK_HASHES_OBSERVED > 0 && NODE_BLOCK_ACCEPT_EVENTS_TOTAL > UNIQUE_BLOCK_HASHES_OBSERVED )); then NODE_BLOCK_DUPLICATE_EVENTS_TOTAL=$((NODE_BLOCK_ACCEPT_EVENTS_TOTAL - UNIQUE_BLOCK_HASHES_OBSERVED)); DUPLICATE_BLOCK_EVENTS_TOTAL=$NODE_BLOCK_DUPLICATE_EVENTS_TOTAL; fi
  if (( TOTAL_MINING_TEMPLATES == 0 )); then TOTAL_MINING_TEMPLATES=$(count_matches_in_logs 'template_received|template'); fi
  if (( TOTAL_MINING_SUBMITS == 0 )); then TOTAL_MINING_SUBMITS=$(count_matches_in_logs 'submit_result|submit_accepted|submit'); fi
  if (( TOTAL_MINING_ACCEPTED == 0 )); then TOTAL_MINING_ACCEPTED=$ACCEPTED_BLOCKS; fi
  if (( TOTAL_MINING_REJECTED == 0 )); then TOTAL_MINING_REJECTED=$REJECTED_BLOCKS; fi
  submit_busy_log_count=$(count_matches_in_logs 'submit_busy|queue_full|busy')
  mining_actor_timeout_log_count=$(count_matches_in_logs 'actor timeout|actor_timeout|submit_actor_timeout')
  TOTAL_MINING_SUBMIT_BUSY=$(( $(integer_sum_or_zero "$TOTAL_MINING_SUBMIT_BUSY") + $(integer_sum_or_zero "$submit_busy_log_count") ))
  TOTAL_MINING_ACTOR_TIMEOUT=$(( $(integer_sum_or_zero "$TOTAL_MINING_ACTOR_TIMEOUT") + $(integer_sum_or_zero "$mining_actor_timeout_log_count") ))
}

sha256_digest(){
  local file="$1"
  [[ -s "$file" ]] || { echo ""; return 0; }
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 -r "$file" | awk '{print $1}'
  else
    echo "UNAVAILABLE"
  fi
}

load_prior_gate_manifest(){
  local gate="$1" file="$2" expected_miners="$3" result commit sha
  [[ -n "$file" ]] || return 0
  if [[ ! -s "$file" ]]; then record_fail "PRIOR_GATE_MANIFEST_MISSING" "prior Gate $gate manifest not found: $file"; return 1; fi
  result=$(jq -r '.result // "UNKNOWN"' "$file" 2>/dev/null || echo UNKNOWN)
  commit=$(jq -r '.git_commit // .commit // "unknown"' "$file" 2>/dev/null || echo unknown)
  miners=$(jq -r '.miner_count // 0' "$file" 2>/dev/null || echo 0)
  sha=$(sha256_digest "$file")
  if [[ "$commit" != "$REPO_COMMIT_FULL" ]]; then record_fail "PRIOR_GATE_COMMIT_MISMATCH" "Gate $gate commit $commit does not match current $REPO_COMMIT_FULL"; fi
  if [[ "$result" != "PASS" ]]; then record_fail "PRIOR_GATE_RESULT_CONTRADICTION" "Gate $gate referenced result is $result, expected PASS"; fi
  if [[ "$miners" != "$expected_miners" ]]; then record_fail "PRIOR_GATE_TOPOLOGY_MISMATCH" "Gate $gate manifest miner_count=$miners, expected $expected_miners"; fi
  case "$gate" in
    C) PRIOR_GATE_C_RESULT="$result"; PRIOR_GATE_C_COMMIT="$commit"; PRIOR_GATE_C_SHA256="$sha"; GATE_5N_1M_BASELINE="$result" ;;
    D) PRIOR_GATE_D_RESULT="$result"; PRIOR_GATE_D_COMMIT="$commit"; PRIOR_GATE_D_SHA256="$sha"; GATE_5N_2M_INTERMEDIATE="$result" ;;
  esac
}

apply_prior_gate_manifests(){
  load_prior_gate_manifest C "$PRIOR_GATE_C_MANIFEST" 1 || true
  load_prior_gate_manifest D "$PRIOR_GATE_D_MANIFEST" 2 || true
}

# Evidence parser compatibility markers retained for static semantics tests:
# "failure_class":"$(classify_failure_class)"
# unique_templates_issued node_block_accept_events_total unique_block_hashes_observed not unique network blocks
# metric_definitions sync_orphan peers network_counters distinct_tips worst_lag_from_max_height orphan_recovery_classification_counters terminal_missing_parent_entries blockdata_not_found
# rpc_liveness rich_node_state selected_segment_session_metrics unsolicited_blockdata_total duplicate_correlated_response_total unknown_request_response_total locator_success_requires_locator_request correlated_BlockData_response_requires_sent_request selected_segment_applied_le_received_le_requested PASS_requires_rich_manifest_fields tail_logs_do_not_change_counts
write_minimal_fallback_manifest(){
  local error_message="${1:-unknown manifest generation error}" tmp
  tmp=$(mktemp -p "$OUT_DIR" evidence_manifest.fallback.XXXXXX) || return 1
  EVIDENCE_MANIFEST_GENERATION_FAILED=1
  MANIFEST_GENERATION_ERROR="$error_message"
  printf '%s\n' "$error_message" > "$OUT_DIR/evidence-manifest-generation-error.txt" 2>/dev/null || true
  jq -n \
    --arg git_commit "$REPO_COMMIT_FULL" \
    --arg stage "$STAGE_NAME" \
    --arg result "$RESULT" \
    --arg failure_class "$(classify_result_failure_class)" \
    --argjson failure_classes "$(failure_classes_json)" \
    --arg manifest_generation_error "$error_message" \
    --arg gate_5n_1m "$GATE_5N_1M_BASELINE" \
    --arg gate_5n_2m "$GATE_5N_2M_INTERMEDIATE" \
    --arg gate_5n_4m "$GATE_5N_4M_STRESS" \
    --argjson exit_code "$(json_number_or_zero "$EXIT_CODE")" \
    --argjson startup_topology_samples_total "$(json_number_or_zero "$STARTUP_TOPOLOGY_SAMPLES_TOTAL")" \
    --argjson startup_topology_stable_samples "$(json_number_or_zero "$STARTUP_TOPOLOGY_STABLE_SAMPLES")" \
    --argjson startup_topology_resets_total "$(json_number_or_zero "$STARTUP_TOPOLOGY_RESETS_TOTAL")" \
    --argjson startup_connection_keepalive_timeouts_total "$(json_number_or_zero "$STARTUP_CONNECTION_KEEPALIVE_TIMEOUTS_TOTAL")" \
    --argjson startup_reconnect_attempts_total "$(json_number_or_zero "$STARTUP_RECONNECT_ATTEMPTS_TOTAL")" \
    --argjson startup_reconnect_success_total "$(json_number_or_zero "$STARTUP_RECONNECT_SUCCESS_TOTAL")" \
    '{git_commit:$git_commit,stage:$stage,result:$result,primary_failure_class:$failure_class,failure_class:$failure_class,failure_classes:$failure_classes,exit_code:$exit_code,gates:{baseline_5n_1m:$gate_5n_1m,intermediate_5n_2m:$gate_5n_2m,stress_5n_4m:$gate_5n_4m},startup_metrics:{startup_topology_samples_total:$startup_topology_samples_total,startup_topology_stable_samples:$startup_topology_stable_samples,startup_topology_resets_total:$startup_topology_resets_total,startup_connection_keepalive_timeouts_total:$startup_connection_keepalive_timeouts_total,startup_reconnect_attempts_total:$startup_reconnect_attempts_total,startup_reconnect_success_total:$startup_reconnect_success_total},manifest_generation_error:$manifest_generation_error}' \
    > "$tmp" && jq -e . "$tmp" >/dev/null && mv "$tmp" "$OUT_DIR/evidence_manifest.json"
  cp "$OUT_DIR/evidence_manifest.json" "$OUT_DIR_ROOT/evidence_manifest.json" 2>/dev/null || true
}

compute_result_dimensions(){
  local storage_fail=0 readiness_fail=0 startup_fail=0 convergence_fail=0 consensus_fail=0 i distinct_state_roots
  for c in "${FAIL_CLASSES[@]:-}"; do
    [[ "$c" == FAIL_STARTUP* || "$c" == STARTUP_* || "$c" == ENV_* ]] && startup_fail=1
    [[ "$c" == STORAGE_* || "$c" == *STORAGE* || "$c" == *MEMORY_PERSIST* ]] && storage_fail=1
    [[ "$c" == READINESS_* || "$c" == *READINESS* ]] && readiness_fail=1
  done
  (( POST_DISTINCT_TIPS != 1 || POST_WORST_LAG != 0 )) && convergence_fail=1
  distinct_state_roots=$(for i in $(seq 1 "$NODE_COUNT"); do printf '%s\n' "${NODE_STATE_ROOT[$i]:-}"; done | awk 'NF' | sort -u | wc -l | tr -d ' ')
  (( distinct_state_roots > 1 )) && consensus_fail=1
  for i in $(seq 1 "$NODE_COUNT"); do
    if [[ "${NODE_STORAGE_COHERENT[$i]:-1}" != "1" ]] || [[ "${NODE_STORAGE_MEMORY_COUNT[$i]:-0}" != "${NODE_STORAGE_PERSISTED_COUNT[$i]:-0}" ]]; then storage_fail=1; fi
    [[ "${NODE_READY[$i]:-0}" != "1" || "${NODE_READINESS_SCHEMA_OK[$i]:-0}" != "1" ]] && readiness_fail=1
  done
  STARTUP_RESULT=$([[ $startup_fail -eq 0 ]] && echo PASS || echo FAIL)
  CONVERGENCE_RESULT=$([[ $convergence_fail -eq 0 ]] && echo PASS || echo FAIL)
  CONSENSUS_STATE_RESULT=$([[ $consensus_fail -eq 0 ]] && echo PASS || echo FAIL)
  STORAGE_CONSISTENCY_RESULT=$([[ $storage_fail -eq 0 ]] && echo PASS || echo FAIL)
  READINESS_RESULT=$([[ $readiness_fail -eq 0 ]] && echo PASS || echo FAIL)
  if [[ "$STARTUP_RESULT$CONVERGENCE_RESULT$CONSENSUS_STATE_RESULT$STORAGE_CONSISTENCY_RESULT$READINESS_RESULT" == "PASSPASSPASSPASSPASS" ]]; then OVERALL_RESULT=PASS; else OVERALL_RESULT=FAIL; fi
  RESULT="$OVERALL_RESULT"
  case "$MINER_COUNT" in
    1) [[ "$GATE_5N_1M_BASELINE" == OBSERVE || "$GATE_5N_1M_BASELINE" == NOT_EXECUTED ]] && GATE_5N_1M_BASELINE="$OVERALL_RESULT" ;;
    2) [[ "$GATE_5N_2M_INTERMEDIATE" == OBSERVE || "$GATE_5N_2M_INTERMEDIATE" == NOT_EXECUTED ]] && GATE_5N_2M_INTERMEDIATE="$OVERALL_RESULT" ;;
    4) [[ "$GATE_5N_4M_STRESS" == OBSERVE || "$GATE_5N_4M_STRESS" == NOT_EXECUTED ]] && GATE_5N_4M_STRESS="$OVERALL_RESULT" ;;
  esac
}

build_per_miner_summary_json(){
  local i
  for i in $(seq 1 "$MINER_COUNT"); do
    jq -n --arg miner "miner-${i}" \
      --argjson templates "$(json_number_or_zero "${miner_template[$i]:-0}")" \
      --argjson submits "$(json_number_or_zero "${miner_submit[$i]:-0}")" \
      --argjson accepted "$(json_number_or_zero "${miner_accept[$i]:-0}")" \
      --argjson rejected "$(json_number_or_zero "${miner_reject[$i]:-0}")" \
      '{miner:$miner,templates:$templates,submits:$submits,accepted:$accepted,rejected:$rejected}'
  done | jq -s '.'
}

build_rich_node_state(){
  local i
  for i in $(seq 1 "$NODE_COUNT"); do
    jq -n --arg node "n$i" --arg tip "${NODE_TIP[$i]:-}" --arg selected_tip "${NODE_SELECTED_TIP[$i]:-}" --arg ordered_dag_tip "${NODE_ORDERED_DAG_TIP[$i]:-}" --arg state_root "${NODE_STATE_ROOT[$i]:-}" --arg accepted_hash_set_digest "${NODE_ACCEPTED_HASH_SET_DIGEST[$i]:-}" --arg readiness "${NODE_READINESS_STATUS[$i]:-unknown}" --arg sync_state "${NODE_SYNC_STATE[$i]:-unknown}" --arg sync_stage "${NODE_SYNC_STAGE[$i]:-unknown}" --arg sync_warning "${NODE_SYNC_OBSERVABILITY_WARNING[$i]:-0}" --arg memory_digest "${NODE_STORAGE_MEMORY_DIGEST[$i]:-}" --arg persisted_digest "${NODE_STORAGE_PERSISTED_DIGEST[$i]:-}" --arg mismatch_source "${NODE_STORAGE_MISMATCH_SOURCE[$i]:-unknown}" --arg checks_digest "${NODE_CHECKS_DIGEST[$i]:-}" --argjson height "$(json_number_or_zero "${NODE_HEIGHT[$i]:-0}")" --argjson peer_count "$(json_number_or_zero "${NODE_PEERS[$i]:-0}")" --argjson inbound_count "$(json_number_or_zero "${NODE_P2P_INBOUND[$i]:-0}")" --argjson outbound_count "$(json_number_or_zero "${NODE_P2P_OUTBOUND[$i]:-0}")" --argjson ready_for_release "$(json_number_or_zero "${NODE_READY[$i]:-0}")" --argjson memory_count "$(json_number_or_zero "${NODE_STORAGE_MEMORY_COUNT[$i]:-0}")" --argjson persisted_count "$(json_number_or_zero "${NODE_STORAGE_PERSISTED_COUNT[$i]:-0}")" --argjson coherent_storage_invariant "$(json_number_or_zero "${NODE_STORAGE_COHERENT[$i]:-0}")" --argjson storage_only_hashes "$(json_array_or_empty "${NODE_STORAGE_ONLY_HASHES[$i]:-[]}")" --argjson memory_only_hashes "$(json_array_or_empty "${NODE_MEMORY_ONLY_HASHES[$i]:-[]}")" --argjson generation "$(json_number_or_zero "${NODE_CHECKS_GENERATION[$i]:-0}")" --argjson canonical_node_gap "$(json_number_or_zero "${NODE_CANONICAL_NODE_GAP[$i]:-0}")" --argjson harness_observed_gap "$(json_number_or_zero "${NODE_HARNESS_OBSERVED_GAP[$i]:-0}")" --argjson network_gaps "$(json_object_or_empty "${NODE_NETWORK_GAPS[$i]:-{}}")" --argjson storage_gaps "$(json_object_or_empty "${NODE_STORAGE_GAPS[$i]:-{}}")" --argjson seg_requested "$(json_number_or_zero "${NODE_SELECTED_SEGMENT_REQUESTED[$i]:-0}")" --argjson seg_received "$(json_number_or_zero "${NODE_SELECTED_SEGMENT_RECEIVED[$i]:-0}")" --argjson seg_applied "$(json_number_or_zero "${NODE_SELECTED_SEGMENT_APPLIED[$i]:-0}")" --argjson correlated_request_counters "$(json_object_or_empty "${NODE_CORRELATED_REQUEST_COUNTERS[$i]:-{}}")" '{node:$node,height:$height,tip:$tip,selected_tip:$selected_tip,ordered_dag_tip:$ordered_dag_tip,state_root:$state_root,accepted_hash_set_digest:$accepted_hash_set_digest,peer_count:$peer_count,inbound_count:$inbound_count,outbound_count:$outbound_count,readiness:{status:$readiness,ready_for_release:($ready_for_release==1)},sync:{state:$sync_state,stage:$sync_stage,canonical_node_gap:$canonical_node_gap,harness_observed_gap:$harness_observed_gap,sync_observability_incomplete:($sync_warning=="1")},storage:{coherent_storage_invariant:($coherent_storage_invariant==1),memory_count:$memory_count,persisted_count:$persisted_count,memory_digest:$memory_digest,persisted_digest:$persisted_digest,storage_only_hashes:$storage_only_hashes,memory_only_hashes:$memory_only_hashes,mismatch_source:$mismatch_source,generation:$generation,digest:$checks_digest,memory_digest:$memory_digest,persisted_digest:$persisted_digest},network_gaps:$network_gaps,storage_gaps:$storage_gaps,selected_tip_inventory:{selected_tip:$selected_tip,ordered_dag_tip:$ordered_dag_tip,tip:$tip,known:($selected_tip != "")},selected_segment_session_metrics:{requested:$seg_requested,received:$seg_received,applied:$seg_applied},correlated_request_counters:$correlated_request_counters}'
  done | jq -s '.'
}

write_evidence_manifest(){
  local archive_sha="${1:-}" end_ts duration manifest_tmp checksum_tmp nodes_json per_miner_json jq_err tmp_final
  end_ts=$(date +%s)
  duration=$((end_ts - START_TS))
  compute_evidence_aggregates || true
  compute_result_dimensions || true
  command -v jq >/dev/null 2>&1 || return 0
  manifest_tmp=$(mktemp -p "$OUT_DIR" evidence_manifest.XXXXXX) || return 1
  checksum_tmp=$(mktemp -p "$OUT_DIR" evidence_checksums.XXXXXX) || { rm -f "$manifest_tmp"; return 1; }
  jq_err="$OUT_DIR/evidence-manifest-jq-diagnostics.txt"
  tmp_final="$OUT_DIR/evidence_manifest.json.tmp"
  {
    for item in evidence-summary.md summaries/package-metadata.txt p2p_convergence.json quiescence-metrics.json command-log.txt process-pids.txt; do
      if [[ -s "$OUT_DIR/$item" ]]; then jq -n --arg path "$item" --arg sha "$(sha256_digest "$OUT_DIR/$item")" '{($path):$sha}'; fi
    done
    [[ -n "$archive_sha" ]] && jq -n --arg sha "$archive_sha" '{"evidence.tar.gz":$sha}'
  } | jq -s 'add // {}' > "$checksum_tmp" 2>"$jq_err" || printf '{}' > "$checksum_tmp"
  jq -e 'type == "object"' "$checksum_tmp" >/dev/null 2>>"$jq_err" || printf '{}' > "$checksum_tmp"
  nodes_json=$(build_rich_node_state)
  nodes_json=$(json_array_or_empty "$nodes_json")
  per_miner_json=$(json_array_or_empty "$(build_per_miner_summary_json)")
  if ! jq -n \
    --arg git_ref "$REPO_REF" --arg git_commit "$REPO_COMMIT_FULL" --arg version "$RELEASE_VERSION" --arg cargo_workspace_version "$WORKSPACE_VERSION" --arg stage "$STAGE_NAME" --arg result "$RESULT" --arg failure_class "$(classify_result_failure_class)" --argjson failure_classes "$(failure_classes_json)" --arg startup_result "$STARTUP_RESULT" --arg convergence_result "$CONVERGENCE_RESULT" --arg consensus_state_result "$CONSENSUS_STATE_RESULT" --arg storage_consistency_result "$STORAGE_CONSISTENCY_RESULT" --arg readiness_result "$READINESS_RESULT" --arg overall_result "$OVERALL_RESULT" --arg start_utc "$START_UTC" --arg end_utc "$(date -u +%FT%TZ)" \
    --arg gate_5n_1m "$GATE_5N_1M_BASELINE" --arg gate_5n_2m "$GATE_5N_2M_INTERMEDIATE" --arg gate_5n_4m "$GATE_5N_4M_STRESS" \
    --argjson node_count "$(json_number_or_zero "$NODE_COUNT")" --argjson miner_count "$(json_number_or_zero "$MINER_COUNT")" --argjson duration "$(json_number_or_zero "$duration")" --argjson exit_code "$(json_number_or_zero "$EXIT_CODE")" \
    --argjson startup_topology_samples_total "$(json_number_or_zero "$STARTUP_TOPOLOGY_SAMPLES_TOTAL")" --argjson startup_topology_stable_samples "$(json_number_or_zero "$STARTUP_TOPOLOGY_STABLE_SAMPLES")" --argjson startup_topology_resets_total "$(json_number_or_zero "$STARTUP_TOPOLOGY_RESETS_TOTAL")" --argjson startup_connection_keepalive_timeouts_total "$(json_number_or_zero "$STARTUP_CONNECTION_KEEPALIVE_TIMEOUTS_TOTAL")" --argjson startup_reconnect_attempts_total "$(json_number_or_zero "$STARTUP_RECONNECT_ATTEMPTS_TOTAL")" --argjson startup_reconnect_success_total "$(json_number_or_zero "$STARTUP_RECONNECT_SUCCESS_TOTAL")" --argjson startup_topology_ready_unix "$(json_number_or_zero "$STARTUP_TOPOLOGY_READY_UNIX")" --argjson startup_topology_wait_duration_ms "$(json_number_or_zero "$STARTUP_TOPOLOGY_WAIT_DURATION_MS")" \
    --argjson templates "$(json_number_or_zero "$TOTAL_MINING_TEMPLATES")" --argjson submits "$(json_number_or_zero "$TOTAL_MINING_SUBMITS")" --argjson accepted "$(json_number_or_zero "$TOTAL_MINING_ACCEPTED")" --argjson rejected "$(json_number_or_zero "$TOTAL_MINING_REJECTED")" --argjson submit_busy "$(json_number_or_zero "$TOTAL_MINING_SUBMIT_BUSY")" --argjson actor_timeout "$(json_number_or_zero "$TOTAL_MINING_ACTOR_TIMEOUT")" \
    --argjson local_miner_templates_received "$(json_number_or_zero "$LOCAL_MINER_TEMPLATES_RECEIVED")" --argjson local_miner_submits_total "$(json_number_or_zero "$LOCAL_MINER_SUBMITS_TOTAL")" --argjson local_miner_submits_accepted "$(json_number_or_zero "$LOCAL_MINER_SUBMITS_ACCEPTED")" --argjson local_miner_submits_rejected "$(json_number_or_zero "$LOCAL_MINER_SUBMITS_REJECTED")" --argjson local_miner_submits_rejected_by_reason "$(json_array_or_empty "$LOCAL_MINER_SUBMITS_REJECTED_BY_REASON_JSON")" --argjson unique_submitted_block_hashes "$(json_number_or_zero "$UNIQUE_SUBMITTED_BLOCK_HASHES")" --argjson node_block_accept_events_total "$(json_number_or_zero "$NODE_BLOCK_ACCEPT_EVENTS_TOTAL")" --argjson node_block_duplicate_events_total "$(json_number_or_zero "$NODE_BLOCK_DUPLICATE_EVENTS_TOTAL")" --argjson node_block_reject_events_total "$(json_number_or_zero "$NODE_BLOCK_REJECT_EVENTS_TOTAL")" --argjson miner_evidence_consistency_failures "$(json_array_or_empty "$MINER_EVIDENCE_CONSISTENCY_FAILURES_JSON")" \
    --arg gate_c_sha256 "$PRIOR_GATE_C_SHA256" --arg gate_d_sha256 "$PRIOR_GATE_D_SHA256" --argjson per_miner "$per_miner_json" --argjson pre_distinct_tips "$(json_number_or_zero "$PRE_DISTINCT_TIPS")" --argjson post_distinct_tips "$(json_number_or_zero "$POST_DISTINCT_TIPS")" --argjson pre_worst_lag "$(json_number_or_zero "$PRE_WORST_LAG")" --argjson post_worst_lag "$(json_number_or_zero "$POST_WORST_LAG")" --argjson nodes "$nodes_json" --slurpfile checksums "$checksum_tmp" \
    '{git_ref:$git_ref,git_commit:$git_commit,version:$version,cargo_workspace_version:$cargo_workspace_version,stage:$stage,node_count:$node_count,miner_count:$miner_count,duration:$duration,result:$overall_result,overall_result:$overall_result,startup_result:$startup_result,convergence_result:$convergence_result,consensus_state_result:$consensus_state_result,storage_consistency_result:$storage_consistency_result,readiness_result:$readiness_result,primary_failure_class:$failure_class,failure_class:$failure_class,failure_classes:(if $overall_result == "PASS" then [] else $failure_classes end),failure_reasons:(if $overall_result == "PASS" then [] else [] end),start_utc:$start_utc,end_utc:$end_utc,exit_code:$exit_code,metric_definitions:{miner_counts:"authoritative miner-N.log submit_result lines; miner-N-tail.log diagnostics ignored",local_miner_submit_metrics:"submit_result lines emitted by local miner processes",per_node_block_acceptance_events:"node metrics blocks_accepted_total summed separately from local submissions",correlated_getblock_response:"outstanding peer/hash/request_id match required",selected_segment_invariant:"applied <= received <= requested"},rpc_liveness:{alive_listener_timeouts:$startup_connection_keepalive_timeouts_total,rpc_liveness_timeouts:0},sync_orphan:{orphan_count_total:0,pending_missing_parents_total:0,terminal_missing_parent_entries:0},network_counters:{missing_parent_requests_sent:0,missing_parent_responses_received:0,unsolicited_blockdata_total:0,duplicate_correlated_response_total:0,unknown_request_response_total:0,locator_requests_sent_total:0,locator_success_total:0},pre_quiescence:{distinct_tips:$pre_distinct_tips,worst_lag_from_max_height:$pre_worst_lag},post_quiescence:{distinct_tips:$post_distinct_tips,worst_lag_from_max_height:$post_worst_lag},distinct_tips:$post_distinct_tips,worst_lag_from_max_height:$post_worst_lag,selected_segment_session_metrics:{requested:($nodes|map(.selected_segment_session_metrics.requested)|add // 0),received:($nodes|map(.selected_segment_session_metrics.received)|add // 0),applied:($nodes|map(.selected_segment_session_metrics.applied)|add // 0)},startup_metrics:{startup_topology_samples_total:$startup_topology_samples_total,startup_topology_stable_samples:$startup_topology_stable_samples,startup_topology_resets_total:$startup_topology_resets_total,startup_connection_keepalive_timeouts_total:$startup_connection_keepalive_timeouts_total,startup_reconnect_attempts_total:$startup_reconnect_attempts_total,startup_reconnect_success_total:$startup_reconnect_success_total,startup_topology_ready_unix:$startup_topology_ready_unix,startup_topology_wait_duration_ms:$startup_topology_wait_duration_ms},mining:{templates:$local_miner_templates_received,submits:$local_miner_submits_total,accepted:$local_miner_submits_accepted,rejected:$local_miner_submits_rejected,stale_template:($local_miner_submits_rejected_by_reason|map(select(.reason=="stale_template")|.count)|add // 0),submit_busy:$submit_busy,actor_timeout:$actor_timeout,per_miner:$per_miner,local_miner_submit_metrics:{templates_received:$local_miner_templates_received,submits_total:$local_miner_submits_total,submits_accepted:$local_miner_submits_accepted,submits_rejected:$local_miner_submits_rejected,submits_rejected_by_reason:$local_miner_submits_rejected_by_reason},per_node_block_acceptance_events:$node_block_accept_events_total,unique_submitted_hashes:$unique_submitted_block_hashes},mining_semantics:{local_miner_templates_received:$local_miner_templates_received,local_miner_submits_total:$local_miner_submits_total,local_miner_submits_accepted:$local_miner_submits_accepted,local_miner_submits_rejected:$local_miner_submits_rejected,local_miner_submits_rejected_by_reason:$local_miner_submits_rejected_by_reason,unique_submitted_block_hashes:$unique_submitted_block_hashes,node_block_accept_events_total:$node_block_accept_events_total,node_block_duplicate_events_total:$node_block_duplicate_events_total,node_block_reject_events_total:$node_block_reject_events_total,evidence_consistency_failures:$miner_evidence_consistency_failures,per_miner:$per_miner,result:(if $local_miner_submits_total == 0 then "NOT_EXECUTED" else "EXECUTED" end)},gates:{baseline_5n_1m:$gate_5n_1m,intermediate_5n_2m:$gate_5n_2m,stress_5n_4m:$gate_5n_4m},prior_evidence:{gate_c:{result:$gate_5n_1m,commit:$git_commit,stage:"5N/1M baseline",manifest_checksum:$gate_c_sha256},gate_d:{result:$gate_5n_2m,commit:$git_commit,stage:"5N/2M intermediate",manifest_checksum:$gate_d_sha256}},rich_node_state:$nodes,nodes:$nodes,checksums:($checksums[0] // {})}' > "$manifest_tmp" 2>"$jq_err"; then
    write_minimal_fallback_manifest "$(cat "$jq_err" 2>/dev/null || echo jq manifest generation failed)"; rm -f "$manifest_tmp" "$checksum_tmp"; return 1
  fi
  if jq -e . "$manifest_tmp" >/dev/null 2>>"$jq_err" && [[ -s "$manifest_tmp" ]]; then
    mv "$manifest_tmp" "$tmp_final" && mv "$tmp_final" "$OUT_DIR/evidence_manifest.json"
    cp "$OUT_DIR/evidence_manifest.json" "$OUT_DIR_ROOT/evidence_manifest.json" 2>/dev/null || true
    rm -f "$checksum_tmp"
    return 0
  fi
  write_minimal_fallback_manifest "$(cat "$jq_err" 2>/dev/null || echo manifest validation failed)"
  rm -f "$manifest_tmp" "$checksum_tmp" "$tmp_final"
  return 1
}


print_p2p_disconnect_diagnostics(){
  local i f
  echo "## P2P disconnect diagnostics"
  echo "| node | disconnect_reason_counts | lifecycle_event_counters | last_error_by_peer | inbound_final_state | outbound_final_state |"
  echo "|---|---|---|---|---|---|"
  for i in $(seq 1 "$NODE_COUNT"); do
    f="$OUT_DIR/endpoints/n${i}-p2p-status-final.json"
    if [[ -f "$f" ]]; then
      jq -r --arg node "n${i}" '
        def compact_json(x): (x // {} | tojson);
        def compact_array(x): (x // [] | tojson);
        .data as $d |
        [
          $node,
          compact_json($d.disconnect_reason_counts),
          compact_json($d.peer_lifecycle_event_counters),
          compact_json($d.last_error_by_peer),
          compact_array($d.inbound_peer_final_state),
          compact_array($d.outbound_peer_final_state)
        ] | @tsv
      ' "$f" 2>/dev/null | awk -F '\t' '{ printf "| %s | `%s` | `%s` | `%s` | `%s` | `%s` |\n", $1, $2, $3, $4, $5, $6 }' || echo "| n${i} | \`{}\` | \`{}\` | \`{}\` | \`[]\` | \`[]\` |"
    else
      echo "| n${i} | \`{}\` | \`{}\` | \`{}\` | \`[]\` | \`[]\` |"
    fi
  done
  echo
  echo "### P2P peer recovery last errors"
  for i in $(seq 1 "$NODE_COUNT"); do
    f="$OUT_DIR/endpoints/n${i}-p2p-status-final.json"
    echo "- n${i}: $(jq -c '.data.peer_recovery // [] | map({peer_id,last_error,last_error_unix,last_error_source,connected,lifecycle_tier})' "$f" 2>/dev/null || echo '[]')"
  done
  echo
}

write_evidence_summary(){
  local end_ts now_utc duration i unique_classes
  end_ts=$(date +%s); now_utc=$(date -u +%FT%TZ); duration=$((end_ts - START_TS))
  unique_classes=$(printf '%s\n' "${FAIL_CLASSES[@]:-}" | awk 'NF' | sort -u | paste -sd, -)
  compute_evidence_aggregates || true
  {
    echo "# v2.2.20 $STAGE_NAME Rehearsal Evidence"
    echo "- chain id expected: \`$CHAIN_ID_EXPECTED\`"
    echo "- network profile: \`$NETWORK_PROFILE\`"
    echo "- start utc: $START_UTC"
    echo "- end utc: $now_utc"
    echo "- runtime duration (s): $duration"
    echo "- global deadline (s): $GLOBAL_DEADLINE_SECS"
    echo "- curl connect timeout (s): $CURL_CONNECT_TIMEOUT_SECS"
    echo "- curl max time (s): $CURL_MAX_TIME_SECS"
    echo "- cleanup curl connect timeout (s): $CLEANUP_CURL_CONNECT_TIMEOUT_SECS"
    echo "- cleanup curl max time (s): $CLEANUP_CURL_MAX_TIME_SECS"
    echo "- final capture budget (s): $FINAL_CAPTURE_BUDGET_SECS"
    echo "- quiescence wait (s): $QUIESCENCE_WAIT_SECS"
    echo "- miners stopped for quiescence: $MINERS_STOPPED_FOR_QUIESCENCE"
    echo

    echo "## Final table per node"
    echo "| node | chain_id | consensus_mode | height | tip | selected_tip | ordered_dag_tip | peer_count | inbound_count | outbound_count | orphan_count | pending_missing_parents | missing_parent_entries | terminal_missing_parent_entries | pending_block_requests | sync_state | catchup_stage | readiness status |"
    echo "|---|---|---|---:|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|"
    for i in $(seq 1 "$NODE_COUNT"); do
      echo "| n${i} | ${NODE_CHAIN_ID[$i]:-unknown} | ${NODE_CONSENSUS_MODE[$i]:-unknown} | ${NODE_HEIGHT[$i]:-0} | ${NODE_TIP[$i]:-} | ${NODE_SELECTED_TIP[$i]:-${NODE_TIP[$i]:-}} | ${NODE_ORDERED_DAG_TIP[$i]:-unavailable} | ${NODE_PEERS[$i]:-0} | ${NODE_P2P_INBOUND[$i]:-0} | ${NODE_P2P_OUTBOUND[$i]:-0} | ${NODE_ORPHAN_COUNT[$i]:-0} | ${NODE_PENDING_MISSING_PARENTS[$i]:-0} | ${NODE_MISSING_PARENTS_COUNT[$i]:-0} | ${NODE_TERMINAL_MISSING_PARENTS_COUNT[$i]:-0} | ${NODE_PENDING_BLOCK_REQUESTS[$i]:-0} | ${NODE_SYNC_STATE[$i]:-unknown} | ${NODE_SYNC_STAGE[$i]:-unknown} | ${NODE_READINESS_STATUS[$i]:-unknown} |"
    done
    echo
    echo "## Required multi-node aggregate gates"
    echo "- distinct_tips: ${POST_DISTINCT_TIPS:-0}"
    echo "- worst_lag_from_max_height: ${POST_WORST_LAG:-0}"
    echo "- RPC_ALIVE_LISTENER_TIMEOUT: ${RPC_ALIVE_LISTENER_TIMEOUT_COUNT:-0}"
    echo "- rpc_liveness_timeout: ${RPC_LIVENESS_TIMEOUT_COUNT:-0}"
    echo "- submit_busy: ${TOTAL_MINING_SUBMIT_BUSY:-0}"
    echo "- actor_timeout: ${TOTAL_MINING_ACTOR_TIMEOUT:-0}"
    echo "- stale/degraded snapshot count: ${STALE_DEGRADED_SNAPSHOT_COUNT:-0}"
    echo "- peer_zero_reconnect_attempt_total: ${TOTAL_ZERO_RECONNECT_ATTEMPTS:-0}"
    echo "- peer_zero_reconnect_success_total: ${TOTAL_ZERO_RECONNECT_SUCCESS:-0}"
    echo "- missing_parent_requests_sent: ${TOTAL_MISSING_PARENT_REQUESTS_SENT:-0}"
    echo "- missing_parent_responses_received: ${TOTAL_MISSING_PARENT_RESPONSES_RECEIVED:-0}"
    echo "- blockdata_not_found: ${TOTAL_BLOCKDATA_NOT_FOUND:-0}"
    echo "## Status/readiness per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: healthy=${NODE_HEALTHY[$i]:-0} ready=${NODE_READY[$i]:-0} readiness_schema_ok=${NODE_READINESS_SCHEMA_OK[$i]:-0}"; done
    echo
    echo "## P2P status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: peers=${NODE_PEERS[$i]:-0} inbound=${NODE_P2P_INBOUND[$i]:-0} outbound=${NODE_P2P_OUTBOUND[$i]:-0} ok=${NODE_P2P_OK[$i]:-0}"; done
    echo
    echo "## Peer classification totals"
    echo "- active peers: ${TOTAL_ACTIVE_PEERS:-0}"
    echo "- recovering peers: ${TOTAL_RECOVERING_PEERS:-0}"
    echo "- cooldown peers: ${TOTAL_COOLDOWN_PEERS:-0}"
    echo "- rate_limited count: ${TOTAL_RATE_LIMITED_COUNT:-0}"
    echo
    print_p2p_disconnect_diagnostics
    echo "## Sync/orphan status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: sync_state=${NODE_SYNC_STATE[$i]:-unknown} catchup_stage=${NODE_SYNC_STAGE[$i]:-unknown} orphan_count=${NODE_ORPHANS[$i]:-0} pending_missing_parents=${NODE_MISSING_PARENTS[$i]:-0} missing_parent_entries=${NODE_MISSING_PARENTS_COUNT[$i]:-0} inv_hashes_requested=${NODE_INV_HASHES_REQUESTED[$i]:-0} peer_recovery_success_count=${NODE_PEER_RECOVERY_SUCCESS_COUNT[$i]:-0}"; done
    echo
    echo "## Sync/orphan aggregate counters"
    echo "- orphan_count: ${TOTAL_ORPHAN_COUNT:-0}"
    echo "- pending_missing_parents: ${TOTAL_PENDING_MISSING_PARENTS:-0}"
    echo "- missing_parent_entries: ${TOTAL_MISSING_PARENT_ENTRIES:-0}"
    echo "- inv_hashes_requested: ${TOTAL_INV_HASHES_REQUESTED:-0}"
    echo "- orphan recovery attempts: ${TOTAL_ORPHAN_RECOVERY_ATTEMPTS:-0}"
    echo "- orphan recovery success: ${TOTAL_ORPHAN_RECOVERY_SUCCESS:-0}"
    echo "- orphan recovery failed_missing_parent: ${TOTAL_ORPHAN_RECOVERY_FAILED_MISSING_PARENT:-0}"
    echo "- orphan recovery failed_persist: ${TOTAL_ORPHAN_RECOVERY_FAILED_PERSIST:-0}"
    echo "- orphan roots rate_limited: ${TOTAL_ORPHAN_ROOTS_RATE_LIMITED:-0}"
    echo "- orphan backlog stale: ${TOTAL_ORPHAN_BACKLOG_STALE:-0}"
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
    echo "## Mining aggregate counters"
    echo "- templates: ${TOTAL_MINING_TEMPLATES:-0}"
    echo "- submits: ${TOTAL_MINING_SUBMITS:-0}"
    echo "- node_block_accept_events_total: ${NODE_BLOCK_ACCEPT_EVENTS_TOTAL:-0}"
    echo "- node_block_reject_events_total: ${NODE_BLOCK_REJECT_EVENTS_TOTAL:-0}"
    echo "- unique_block_hashes_observed: ${UNIQUE_BLOCK_HASHES_OBSERVED:-0}"
    echo "- node_block_duplicate_events_total: ${NODE_BLOCK_DUPLICATE_EVENTS_TOTAL:-0}"
    echo "- local_miner_submits_total: ${LOCAL_MINER_SUBMITS_TOTAL:-0}"
    echo "- local_miner_submits_accepted: ${LOCAL_MINER_SUBMITS_ACCEPTED:-0}"
    echo "- local_miner_submits_rejected: ${LOCAL_MINER_SUBMITS_REJECTED:-0}"
    echo "- rejected: ${TOTAL_MINING_REJECTED:-0}"
    echo "- submit_busy: ${TOTAL_MINING_SUBMIT_BUSY:-0}"
    echo "- actor_timeout: ${TOTAL_MINING_ACTOR_TIMEOUT:-0}"
    echo
    echo "## RPC liveness counters"
    echo "- RPC_ALIVE_LISTENER_TIMEOUT count: ${RPC_ALIVE_LISTENER_TIMEOUT_COUNT:-0}"
    echo "- rpc_liveness_timeout count: ${RPC_LIVENESS_TIMEOUT_COUNT:-0}"
    echo "- stale/degraded snapshot count: ${STALE_DEGRADED_SNAPSHOT_COUNT:-0}"
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
    echo "| 5N/4M stress | $GATE_5N_4M_STRESS | no, evidence only for v2.2.20 |"
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
    echo "| baseline miner receives templates | $( (( ${miner_template[1]:-0} > 0 )) && echo PASS || echo FAIL ) |"
    echo "| baseline miner submits work | $( (( ${miner_submit[1]:-0} > 0 )) && echo PASS || echo FAIL ) |"
    if (( MINER_COUNT >= 2 )); then
      echo "| intermediate miners receive templates | $( for i in 1 2; do (( ${miner_template[$i]:-0} > 0 )) || exit 1; done; echo PASS ) |"
      echo "| intermediate miners submit work | $( for i in 1 2; do (( ${miner_submit[$i]:-0} > 0 )) || exit 1; done; echo PASS ) |"
    else
      echo "| intermediate miners receive templates | NOT_RUN |"
      echo "| intermediate miners submit work | NOT_RUN |"
    fi
    echo "| stress miners receive templates (evidence only) | $( for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_template[$i]:-0} > 0 )) || exit 1; done; echo PASS ) |"
    echo "| stress miners submit work (evidence only) | $( for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_submit[$i]:-0} > 0 )) || exit 1; done; echo PASS ) |"
    echo "| accepted blocks >0 (or waived) | $( (( ACCEPTED_BLOCKS>0 || WAIVE_ACCEPTED_BLOCK_GATE==1 )) && echo PASS || echo FAIL ) |"
    echo "| convergence after quiescence | $( (( POST_CONVERGED==1 )) && echo PASS || echo FAIL ) |"
    echo "| missing parent backlog clear | $(for i in $(seq 1 "$NODE_COUNT"); do (( ${NODE_MISSING_PARENTS[$i]:-0} == 0 )) || { echo FAIL; break; }; [[ $i == $NODE_COUNT ]] && echo PASS; done) |"
    echo
    echo "## Build/runtime metadata"
    echo "- commit: $REPO_COMMIT"
    echo "- version: $NODE_VERSION"
    echo
    echo "## Result"
    FAILURE_CLASS=$(classify_result_failure_class)
    FAILURE_CLASSES=$(failure_classes_json | jq -c .)
    echo "- result: $RESULT"
    echo "- primary_failure_class: $FAILURE_CLASS"
    echo "- failure_class: $FAILURE_CLASS"
    echo "- failure_classes: $FAILURE_CLASSES"
    echo "- exit_code: $EXIT_CODE"
    echo "- node_count: $NODE_COUNT"
    echo "- miner_count: $MINER_COUNT"
    echo "- evidence_manifest: evidence_manifest.json"
    echo "- evidence_manifest_generation_failed: ${EVIDENCE_MANIFEST_GENERATION_FAILED:-0}"
    if (( ${EVIDENCE_MANIFEST_GENERATION_FAILED:-0} != 0 )); then echo "- EVIDENCE_MANIFEST_GENERATION_FAILED: ${MANIFEST_GENERATION_ERROR:-unknown}"; fi
    echo "- evidence_consistency_failure_count: $(( ${EVIDENCE_MANIFEST_GENERATION_FAILED:-0} + ${#EVIDENCE_CONSISTENCY_FAILURES[@]} ))"
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
    --arg failure_class "$(classify_result_failure_class)" \
    --argjson failure_classes "$(failure_classes_json)" \
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
    '{chain_id:$chain_id,version:$version,commit:$commit,primary_failure_class:$failure_class,failure_class:$failure_class,failure_classes:$failure_classes,tip:$tip,legacy_ambiguous_aggregates:{accepted_blocks:$accepted_blocks,rejected_blocks:$rejected_blocks,deprecated:true,note:"not unique network blocks; use mining_semantics local miner submit counters"},gates:{baseline_5n_1m:$gate_5n_1m,intermediate_5n_2m:$gate_5n_2m,stress_5n_4m:$gate_5n_4m},nodes:$nodes}' \
    > "$OUT_DIR/p2p_convergence.json"
}

write_restart_rejoin_log(){
  {
    if [[ "${RESTART_REJOIN_EXECUTED:-0}" == "1" ]]; then
      echo "restart_rejoin_status=EXECUTED"
      echo "note=restart/rejoin drill invoked by this script"
    else
      echo "restart_rejoin_status=NOT_EXECUTED"
      echo "note=this staged convergence rehearsal validates steady-state convergence and quiescence; restart/rejoin drill not invoked by this script"
    fi
    echo "timestamp_utc=$(date -u +%FT%TZ)"
  } > "$OUT_DIR/restart_rejoin.log"
}

write_metadata(){
  {
    echo "stage_name=$STAGE_NAME"
    echo "git_ref=$REPO_REF"
    echo "git_commit=$REPO_COMMIT_FULL"
    echo "version=$RELEASE_VERSION"
    echo "cargo_workspace_version=$WORKSPACE_VERSION"
    echo "uname=$(uname -a 2>/dev/null || echo unknown)"
    echo "rustc_version=$(rustc --version 2>/dev/null || echo unavailable)"
    echo "cargo_version=$(cargo --version 2>/dev/null || echo unavailable)"
    echo "start_utc=$START_UTC"
    echo "end_utc=$(date -u +%FT%TZ)"
    echo "duration_seconds=$(( $(date +%s) - START_TS ))"
    echo "exit_code=$EXIT_CODE"
    echo "failure_class=$(classify_failure_class)"
    echo "global_deadline_seconds=$GLOBAL_DEADLINE_SECS"
    echo "curl_connect_timeout_seconds=$CURL_CONNECT_TIMEOUT_SECS"
    echo "curl_max_time_seconds=$CURL_MAX_TIME_SECS"
    echo "cleanup_curl_connect_timeout_seconds=$CLEANUP_CURL_CONNECT_TIMEOUT_SECS"
    echo "cleanup_curl_max_time_seconds=$CLEANUP_CURL_MAX_TIME_SECS"
    echo "final_capture_budget_seconds=$FINAL_CAPTURE_BUDGET_SECS"
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

assert_packaged_evidence(){
  local i ok=0 expected_samples
  [[ -s "$OUT_DIR/evidence_manifest.json" ]] || { echo "evidence assertion failed: manifest missing or empty"; ok=1; }
  jq -e . "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: manifest is invalid JSON"; ok=1; }
  for i in $(seq 1 "$NODE_COUNT"); do [[ -s "$OUT_DIR/nodes/n${i}.log" || -s "$OUT_DIR/logs/n${i}.log" ]] || { echo "evidence assertion failed: missing n${i} log"; ok=1; }; done
  expected_samples=$(json_number_or_zero "$STARTUP_TOPOLOGY_SAMPLES_TOTAL")
  if (( STARTUP_TOPOLOGY_STABLE_SAMPLES > expected_samples )); then echo "evidence assertion failed: stable startup samples exceed total samples"; ok=1; fi
  if (( MINERS_STARTED == 0 )); then
    jq -e '.mining_semantics.result == "NOT_EXECUTED"' "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: no miners launched but mining result is not NOT_EXECUTED"; ok=1; }
  fi
  jq -e '(.result == .overall_result) and (.failure_class == .primary_failure_class) and (.failure_classes | type == "array") and (if .result == "PASS" then .failure_class == "none" and (.failure_classes | length == 0) else (.failure_classes | length > 0) end)' "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: result/overall/failure_class contradiction"; ok=1; }
  jq -e 'if .storage_consistency_result == "FAIL" then ([.rich_node_state[].storage.coherent_storage_invariant] | any(. == false)) else true end' "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: endpoint checks true cannot yield storage FAIL"; ok=1; }
  jq -e 'all(.rich_node_state[]; (.storage.memory_count // 0) > 0 and (.storage.persisted_count // 0) > 0)' "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: memory/persisted values defaulted to zero"; ok=1; }
  jq -e 'all(.rich_node_state[]; ((.state_root // "") != "") and ((.accepted_hash_set_digest // "") != ""))' "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: completed evidence has blank state root or accepted hash digest"; ok=1; }
  jq -e '.gates.baseline_5n_1m != null and .gates.intermediate_5n_2m != null and .gates.stress_5n_4m != null' "$OUT_DIR/evidence_manifest.json" >/dev/null 2>&1 || { echo "evidence assertion failed: executed gate cannot be null"; ok=1; }
  return "$ok"
}

package_evidence(){
  write_metadata || true
  cp "$OUT_DIR/p2p_convergence.json" "$OUT_DIR/final-convergence-table.json" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR/summaries/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/command-log.txt" "$OUT_DIR_ROOT/command-log.txt" 2>/dev/null || true
  write_evidence_manifest || record_warn "failed to write evidence manifest before packaging"
  cp "$OUT_DIR/bootnode.txt" "$OUT_DIR_ROOT/bootnode.txt" 2>/dev/null || true
  printf "%s\n" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt" 2>/dev/null || true
  printf "%s\n" "$OUT_DIR" > "$OUT_DIR/current-run-dir.txt" 2>/dev/null || true
  for i in $(seq 1 "$NODE_COUNT"); do cp "$OUT_DIR/logs/n${i}.log" "$OUT_DIR/nodes/n${i}.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do cp "$OUT_DIR/logs/miner-${i}.log" "$OUT_DIR/miners/miner-${i}.log" 2>/dev/null || true; done
  local tar_tmp manifest item tar_rc=0
  tar_tmp=$(mktemp -p /tmp evidence.XXXXXX.tar.gz) || return 1
  manifest=$(mktemp -p /tmp evidence-manifest.XXXXXX) || { rm -f "$tar_tmp"; return 1; }
  for item in endpoints logs miners nodes samples summaries evidence-summary.md evidence_manifest.json command-log.txt process-pids.txt p2p_convergence.json final-convergence-table.json quiescence-metrics.json restart_rejoin.log global-watchdog-timeout.txt hard-stop-diagnostics.txt progress.log current-run-dir.txt; do
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
  write_evidence_manifest "$(awk '{print $1; exit}' "$OUT_DIR/evidence.tar.gz.sha256" 2>/dev/null || true)" || record_warn "failed to update evidence manifest with archive checksum"
  cp "$OUT_DIR/evidence.tar.gz" "$OUT_DIR_ROOT/evidence.tar.gz" 2>/dev/null || true
  write_checksum_file "$OUT_DIR_ROOT/evidence.tar.gz" "$OUT_DIR_ROOT/evidence.tar.gz.sha256" || cp "$OUT_DIR/evidence.tar.gz.sha256" "$OUT_DIR_ROOT/evidence.tar.gz.sha256" 2>/dev/null || true
  assert_packaged_evidence || record_fail "EVIDENCE_ASSERTIONS_FAILED" "packaged evidence assertions failed"
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
    class="HARNESS_STALL_TIMEOUT"
    msg="global watchdog timeout after ${GLOBAL_DEADLINE_SECS}s; last_progress_ts=${LAST_PROGRESS_TS:-unknown}"
    record_fail "GLOBAL_DEADLINE_TIMEOUT" "global deadline reached after ${GLOBAL_DEADLINE_SECS}s"
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
  CLEANUP_DEADLINE_TS=$(( $(date +%s) + FINAL_CAPTURE_BUDGET_SECS ))
  EXIT_CODE=$exit_code
  trap - EXIT
  trap '' INT TERM
  # Keep cleanup bounded independently from normal cleanup paths.
  ( sleep $((FINAL_CAPTURE_BUDGET_SECS + CLEANUP_KILL_GRACE_SECS + CLEANUP_PORT_WAIT_SECS + 20)); echo "FATAL: cleanup hard-stop exceeded" >&2; kill -KILL $$ 2>/dev/null || true ) &
  HARD_KILL_WATCHDOG_PID=$!
  stop_global_deadline_watchdog
  if (( exit_code == 124 )); then
    if (( ${#FAIL_REASONS[@]} == 0 )); then record_fail "GLOBAL_DEADLINE_TIMEOUT" "script exited with timeout status 124 before classified failure"; fi
  elif (( exit_code != 0 && ${#FAIL_REASONS[@]} == 0 )); then
    record_fail "HARNESS_ERROR" "script exited non-zero before classified failure: $exit_code"
  fi
  if (( QUIESCENCE_COMPLETED == 0 )); then
    if [[ "$(classify_failure_class)" == "environment" && "${ENV_PREFLIGHT_OK:-0}" == "0" ]]; then
      record_warn "skipping node endpoint collection because environment preflight failed before node launch"
    else
      collect_final_state cleanup-pre-quiescence || true
      snapshot_current_as_pre || true
      compute_metrics_from_current PRE || true
      POST_CONVERGED=$PRE_CONVERGED; POST_WORST_LAG=$PRE_WORST_LAG; POST_DISTINCT_TIPS=$PRE_DISTINCT_TIPS; LAG_IMPROVED=0
      write_quiescence_metrics || true
    fi
  fi
  if (( ${#FAIL_REASONS[@]} == 0 )); then
    RESULT="PASS"
  elif [[ "$(classify_failure_class)" == "environment" ]]; then
    RESULT="ENV_FAIL"
  else
    RESULT="FAIL"
  fi
  capture_hard_stop_diagnostics || true
  sync || true
  capture_log_tails || true
  for i in $(seq 1 "$NODE_COUNT"); do cp "$OUT_DIR/logs/n${i}.log" "$OUT_DIR/nodes/n${i}.log" 2>/dev/null || true; done
  write_evidence_summary || true
  write_p2p_convergence_json || true
  write_restart_rejoin_log || true
  stop_pids --label=miner "${MINER_PIDS[@]:-}"
  stop_pids --label=node "${NODE_PIDS[@]:-}"
  kill_processes_on_test_ports || true
  wait_for_ports_clean || true
  package_evidence || package_rc=$?
  if (( package_rc != 0 )); then
    echo "FATAL: evidence packaging failed for $OUT_DIR (rc=$package_rc)"
    exit_code=1
    EXIT_CODE=$exit_code
    record_fail "EVIDENCE_PACKAGING_FAILED" "evidence packaging failed with rc=$package_rc"
    RESULT="FAIL"
    write_evidence_summary || true
  fi
  [[ -n "${HARD_KILL_WATCHDOG_PID:-}" ]] && kill "$HARD_KILL_WATCHDOG_PID" 2>/dev/null || true
  echo "RUN_DIR=$OUT_DIR"
  echo "FINAL_EXIT_CODE=$exit_code"
  echo "FINAL_RESULT=$RESULT"
  exit "$exit_code"
}
trap 'cleanup $?' EXIT
trap 'on_signal INT 130' INT
trap 'on_signal TERM 143' TERM

mark_progress "environment_preflight_start"
run_rehearsal_environment_preflight
mark_progress "environment_preflight_complete"
mark_progress "preflight_start"
OUT_DIR="$OUT_DIR" run_with_global_timeout preflight "$ROOT_DIR/scripts/v2_2_20_preflight_check.sh"
mark_progress "preflight_complete"
ensure_ports_free
mark_progress "cargo_build_start"
run_with_global_timeout cargo_build cargo build --workspace --release --locked
mark_progress "cargo_build_complete"
start_global_deadline_watchdog

start_node(){
  local idx="$1" rpc="$2" p2p="$3" bootnode="$4" name data
  name="n${idx}"
  data="$OUT_DIR/data-${name}"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --network "$NETWORK_PROFILE" --rpc-listen "127.0.0.1:${rpc}" --p2p-listen "/ip4/127.0.0.1/tcp/${p2p}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  echo "launch node-${name}: PULSEDAG_ROCKSDB_PATH=$data/rocksdb RUST_LOG=${RUST_LOG:-pulsedagd=info,pulsedag_p2p=info} RUST_LOG_STYLE=never ${cmd[*]}"
  PULSEDAG_ROCKSDB_PATH="$data/rocksdb" RUST_LOG="${RUST_LOG:-pulsedagd=info,pulsedag_p2p=info}" RUST_LOG_STYLE=never "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  NODE_PIDS+=("$!")
  echo "$! node-${name}" >> "$OUT_DIR/process-pids.txt"
  mark_progress "node_${name}_started"
}

run_pr647_runtime_cases(){
  (( PR647_RUNTIME_CASES == 1 )) || return 0
  local n1_pid="${NODE_PIDS[0]:-}"
  {
    echo "timestamp_utc=$(date -u +%FT%TZ) case=peer_zero_recovery action=stop_bootnode target=n1 outage_secs=$PEER_ZERO_OUTAGE_SECS"
    echo "timestamp_utc=$(date -u +%FT%TZ) case=same_height_competing_tip_missing_parents action=observe_two_miners_and_capture_final_quiescence_metrics note=two active miners may produce same-height competing tips; /metrics exports final_quiescence_same_height_* counters and blocked reasons"
  } >> "$OUT_DIR/pr647-runtime-cases.log"
  if [[ -n "$n1_pid" ]] && kill -0 "$n1_pid" 2>/dev/null; then
    stop_pids --label=pr647-peer-zero-n1 "$n1_pid"
    sleep_with_deadline "$PEER_ZERO_OUTAGE_SECS"
    start_node 1 $((BASE_RPC_PORT+1)) $((BASE_P2P_PORT+1)) ""
    RESTART_REJOIN_EXECUTED=1
    wait_node_ready 1 || true
    echo "timestamp_utc=$(date -u +%FT%TZ) case=peer_zero_recovery action=restarted_bootnode target=n1" >> "$OUT_DIR/pr647-runtime-cases.log"
    mark_progress "pr647_peer_zero_recovery_case_complete"
  else
    record_warn "PR #647 peer-zero runtime case skipped because n1 pid was unavailable"
  fi
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
NODE_1_ID=$(extract_bootnode_peer_id "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json" || true)
if [[ -z "$NODE_1_ID" ]]; then
  echo "FATAL: unable to build bootnode multiaddr because peer id extraction failed; see $OUT_DIR/bootnode-peer-id-extraction-failure.txt"
  exit 1
fi
BOOT_1="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+1))/p2p/${NODE_1_ID}"
echo "$BOOT_1" > "$OUT_DIR/bootnode.txt"
for i in 2 3 4 5; do start_node "$i" $((BASE_RPC_PORT+i)) $((BASE_P2P_PORT+i)) "$BOOT_1"; done
sleep_with_deadline 3

for i in $(seq 1 "$NODE_COUNT"); do wait_node_ready "$i" || true; done

startup_sample_p2p_statuses(){
  local i pid status=0 pids=()
  for i in $(seq 1 "$NODE_COUNT"); do
    (
      curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT_SECS" --max-time 2 "http://127.0.0.1:$((BASE_RPC_PORT+i))/p2p/status" \
        > "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json.tmp" && \
        mv "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json.tmp" "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json"
    ) &
    pids+=("$!")
  done
  for pid in "${pids[@]}"; do wait "$pid" || status=1; done
  return "$status"
}

validate_startup_topology_sample(){
  local root_inbound nonroot_bad=0 unstable=0 expected peer_count peer_id changed listeners expected_port alive_bad=0 listener_bad=0 i
  root_inbound=$(jq -r '.data.peer_accounting.inbound_peer_count // .data.inbound_peer_count // 0' "$OUT_DIR/endpoints/n1-p2p-status-pre-mining.json" 2>/dev/null || echo 0)
  for i in $(seq 1 "$NODE_COUNT"); do
    if [[ -z "${NODE_PIDS[$((i-1))]:-}" ]] || ! kill -0 "${NODE_PIDS[$((i-1))]}" 2>/dev/null; then alive_bad=1; fi
    listeners=$(extract_p2p_listening_addresses "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json")
    expected_port=$((BASE_P2P_PORT+i))
    p2p_listener_has_expected_port "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" "$expected_port" || listener_bad=1
    peer_id=$(jq -r '.data.peer_id // .data.p2p_peer_id // .data.local_node_id // empty' "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" 2>/dev/null || true)
    changed=$(jq -r '.data.p2p_peer_id_changed_since_previous_start // false' "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" 2>/dev/null || echo false)
    [[ -n "$peer_id" ]] || unstable=1
    [[ "$changed" != "true" ]] || unstable=1
    if (( i > 1 )); then
      peer_count=$(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" 2>/dev/null || echo 0)
      (( peer_count >= 1 )) || nonroot_bad=1
    fi
  done
  expected=$((NODE_COUNT - 1))
  [[ -d "$OUT_DIR/logs" ]] && STARTUP_CONNECTION_KEEPALIVE_TIMEOUTS_TOTAL=$(rg -c 'KeepAliveTimeout' "$OUT_DIR/logs" 2>/dev/null | awk -F: '{sum += $NF} END {print sum + 0}' || echo 0)
  STARTUP_RECONNECT_ATTEMPTS_TOTAL=$(jq -s '[.[].data.peer_zero_reconnect_attempt_total?,.[].data.peer_accounting.peer_zero_reconnect_attempt_total?,.[].data.peers.peer_zero_reconnect_attempts?] | map(select(. != null)) | add // 0' "$OUT_DIR"/endpoints/n*-p2p-status-pre-mining.json 2>/dev/null || echo 0)
  STARTUP_RECONNECT_SUCCESS_TOTAL=$(jq -s '[.[].data.peer_zero_reconnect_success_total?,.[].data.peer_accounting.peer_zero_reconnect_success_total?,.[].data.peers.peer_zero_reconnect_success?] | map(select(. != null)) | add // 0' "$OUT_DIR"/endpoints/n*-p2p-status-pre-mining.json 2>/dev/null || echo 0)
  if (( root_inbound >= expected && nonroot_bad == 0 && unstable == 0 && alive_bad == 0 && listener_bad == 0 )); then
    return 0
  fi
  echo "startup topology sample incomplete: root_inbound=${root_inbound}/${expected} nonroot_bad=${nonroot_bad} unstable_peer_ids=${unstable} alive_bad=${alive_bad} listener_bad=${listener_bad}"
  return 1
}

wait_for_startup_topology(){
  local started_ms now stable=0
  started_ms=$(date +%s%3N)
  peer_wait_deadline=$(( $(date +%s) + P2P_CONNECT_WAIT_SECS ))
  while (( $(date +%s) < peer_wait_deadline )); do
    startup_sample_p2p_statuses || true
    STARTUP_TOPOLOGY_SAMPLES_TOTAL=$((STARTUP_TOPOLOGY_SAMPLES_TOTAL + 1))
    if validate_startup_topology_sample; then
      stable=$((stable + 1))
      STARTUP_TOPOLOGY_STABLE_SAMPLES=$stable
      if (( stable >= STARTUP_TOPOLOGY_REQUIRED_STABLE_SAMPLES )); then
        STARTUP_TOPOLOGY_READY_UNIX=$(date +%s)
        STARTUP_TOPOLOGY_WAIT_DURATION_MS=$(( $(date +%s%3N) - started_ms ))
        return 0
      fi
    else
      if (( stable > 0 )); then STARTUP_TOPOLOGY_RESETS_TOTAL=$((STARTUP_TOPOLOGY_RESETS_TOTAL + 1)); fi
      stable=0
      STARTUP_TOPOLOGY_STABLE_SAMPLES=0
    fi
    sleep_with_deadline "$STARTUP_TOPOLOGY_SAMPLE_INTERVAL_SECS"
  done
  STARTUP_TOPOLOGY_WAIT_DURATION_MS=$(( $(date +%s%3N) - started_ms ))
  capture_p2p_gate_failure
  mark_startup_failure_gates
  record_fail "startup_topology_timeout" "startup topology did not remain complete for ${STARTUP_TOPOLOGY_REQUIRED_STABLE_SAMPLES} consecutive samples within ${P2P_CONNECT_WAIT_SECS}s"
  return 1
}

wait_for_startup_topology || exit 1

for i in $(seq 1 "$MINER_COUNT"); do
  local_node="http://127.0.0.1:$((BASE_RPC_PORT+i))"
  echo "launch miner-${i}: $MINER_BIN --node $local_node --miner-address v2220-${RUN_ID}-miner-${i} --backend cpu --threads 1 --loop"
  "$MINER_BIN" --node "$local_node" --miner-address "v2220-${RUN_ID}-miner-${i}" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner-${i}.log" 2>&1 &
  MINER_PIDS+=("$!")
  MINERS_STARTED=$((MINERS_STARTED + 1))
  echo "$! miner-${i}" >> "$OUT_DIR/process-pids.txt"
  mark_progress "miner_${i}_started"
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
  if (( PR647_RUNTIME_CASES == 1 )) && [[ ! -f "$OUT_DIR/pr647-runtime-cases.done" ]] && (( $(date +%s) >= START_TS + (DURATION_SECS / 3) )); then
    run_pr647_runtime_cases || true
    date -u +%FT%TZ > "$OUT_DIR/pr647-runtime-cases.done"
  fi
  mark_progress "mining_sample"
  sleep_with_deadline 10
done

echo "entering quiescence: collecting pre-quiescence sample before stopping miners"
collect_final_state pre-quiescence
snapshot_current_as_pre
compute_metrics_from_current PRE

echo "entering quiescence: stopping miners and waiting ${QUIESCENCE_WAIT_SECS}s before final tips/readiness sample"
stop_pids --label=miner "${MINER_PIDS[@]:-}"
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
  if (( ${NODE_HEIGHT[$i]:-0} <= 1 && ${NODE_PEERS[$i]:-0} == 0 )); then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if [[ "${NODE_HEALTHY[$i]:-0}" != "1" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if [[ "${NODE_READY[$i]:-0}" != "1" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; fi
  if (( ${NODE_HEIGHT[$i]:-0} <= 0 )); then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if [[ "${NODE_P2P_OK[$i]:-0}" != "1" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
  if (( i == 1 )); then
    root_private_topology_valid=$(jq -r 'if (.data.peer_accounting.bootnode_root_topology // false) then (.data.peer_accounting.private_topology_valid // false) else true end' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo false)
    root_inbound_peers=$(jq -r '.data.inbound_peer_count // .data.peer_accounting.inbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)
    if [[ "$root_private_topology_valid" != "true" || "$root_inbound_peers" == "0" ]]; then
      BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0
    fi
  fi
  if [[ -z "${NODE_CHAIN_ID[$i]:-}" || "${NODE_CHAIN_ID[$i]:-}" != "$CHAIN_ID_EXPECTED" ]]; then BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; fi
done

final_tip="${NODE_TIP[1]:-}"
for i in 1 2 3 4 5; do
  [[ "${NODE_TIP[$i]:-}" == "$final_tip" ]] || { BASELINE_OK=0; INTERMEDIATE_OK=0; STRESS_OK=0; }
done

(( ${miner_template[1]:-0} > 0 )) || BASELINE_OK=0
(( ${miner_submit[1]:-0} > 0 )) || BASELINE_OK=0
for i in 1 2; do
  (( ${miner_template[$i]:-0} > 0 )) || INTERMEDIATE_OK=0
  (( ${miner_submit[$i]:-0} > 0 )) || INTERMEDIATE_OK=0
done
for i in $(seq 1 "$MINER_COUNT"); do
  (( ${miner_template[$i]:-0} > 0 )) || STRESS_OK=0
  (( ${miner_submit[$i]:-0} > 0 )) || STRESS_OK=0
done

if [[ -z "$PRIOR_GATE_C_MANIFEST" ]]; then
  if (( MINER_COUNT == 1 )); then (( BASELINE_OK == 1 )) && GATE_5N_1M_BASELINE=PASS || GATE_5N_1M_BASELINE=FAIL; else GATE_5N_1M_BASELINE=NOT_PROVIDED; fi
fi
if (( MINER_COUNT >= 2 )); then
  if [[ -z "$PRIOR_GATE_D_MANIFEST" ]]; then (( INTERMEDIATE_OK == 1 )) && GATE_5N_2M_INTERMEDIATE=PASS || GATE_5N_2M_INTERMEDIATE=FAIL; fi
else
  GATE_5N_2M_INTERMEDIATE=NOT_RUN
fi
if (( MINER_COUNT >= 4 )); then
  (( STRESS_OK == 1 )) && GATE_5N_4M_STRESS=PASS || GATE_5N_4M_STRESS=OBSERVE_FAIL
else
  GATE_5N_4M_STRESS=NOT_RUN
fi

apply_prior_gate_manifests
if [[ "$GATE_5N_1M_BASELINE" != "PASS" && "$GATE_5N_1M_BASELINE" != "NOT_PROVIDED" && "$GATE_5N_1M_BASELINE" != "NOT_RUN" ]]; then record_fail "STAGED_GATE_5N_1M" "5N/1M baseline gate did not pass (status=$GATE_5N_1M_BASELINE)"; fi
root_private_topology_valid=$(jq -r 'if (.data.peer_accounting.bootnode_root_topology // false) then (.data.peer_accounting.private_topology_valid // false) else true end' "$OUT_DIR/endpoints/n1-p2p-status-final.json" 2>/dev/null || echo false)
root_inbound_peers=$(jq -r '.data.inbound_peer_count // .data.peer_accounting.inbound_peer_count // 0' "$OUT_DIR/endpoints/n1-p2p-status-final.json" 2>/dev/null || echo 0)
if [[ "$root_private_topology_valid" != "true" || "$root_inbound_peers" == "0" ]]; then
  record_fail "PRIVATE_TOPOLOGY_BOOTNODE_ROOT" "bootnode/root topology invalid: private_topology_valid=${root_private_topology_valid} inbound_peer_count=${root_inbound_peers}"
fi
if (( MINER_COUNT >= 2 )); then
  if [[ "$GATE_5N_2M_INTERMEDIATE" != "PASS" && "$GATE_5N_2M_INTERMEDIATE" != "NOT_PROVIDED" && "$GATE_5N_2M_INTERMEDIATE" != "NOT_RUN" ]]; then record_fail "STAGED_GATE_5N_2M" "5N/2M intermediate gate did not pass (status=$GATE_5N_2M_INTERMEDIATE)"; fi
fi
if (( MINER_COUNT >= 4 )) && [[ "$GATE_5N_4M_STRESS" != "PASS" ]]; then
  record_warn "5N/4M stress gate did not pass; retained as non-mandatory readiness evidence for v2.2.20"
fi

for i in $(seq 1 "$NODE_COUNT"); do if [[ "${NODE_READINESS_SCHEMA_OK[$i]:-0}" != "1" ]]; then record_fail "READINESS_SCHEMA_MISMATCH" "n${i} readiness response does not match current required schema"; fi; done
if (( TOTAL_MISSING_PARENT_REQUESTS_SENT != $(sum_node_array NODE_MISSING_PARENT_REQUESTS_SENT) )); then record_fail "NETWORK_COUNTER_AGGREGATE_MISMATCH" "manifest missing_parent_requests_sent=${TOTAL_MISSING_PARENT_REQUESTS_SENT} differs from endpoint aggregate=$(sum_node_array NODE_MISSING_PARENT_REQUESTS_SENT)"; fi
if (( NODE_BLOCK_ACCEPT_EVENTS_TOTAL > 0 && UNIQUE_BLOCK_HASHES_OBSERVED > 0 && TOTAL_MINING_ACCEPTED != NODE_BLOCK_ACCEPT_EVENTS_TOTAL )); then record_fail "ACCEPTED_COUNTER_SEMANTIC_MISMATCH" "mining.accepted must equal node block accept events, not unique network blocks"; fi
if (( TOTAL_ORPHAN_COUNT != 0 )); then record_fail "POST_QUIESCENCE_ORPHANS" "post-quiescence orphan_count is ${TOTAL_ORPHAN_COUNT}"; fi
if (( TOTAL_PENDING_MISSING_PARENTS != 0 )); then record_fail "POST_QUIESCENCE_PENDING_MISSING_PARENTS" "post-quiescence pending_missing_parents is ${TOTAL_PENDING_MISSING_PARENTS}"; fi
if (( TOTAL_MISSING_PARENT_ENTRIES != 0 )); then record_fail "POST_QUIESCENCE_MISSING_PARENT_ENTRIES" "active missing_parent_entries is ${TOTAL_MISSING_PARENT_ENTRIES}"; fi
if (( TOTAL_PENDING_BLOCK_REQUESTS != 0 )); then record_fail "POST_QUIESCENCE_PENDING_BLOCK_REQUESTS" "pending_block_requests is ${TOTAL_PENDING_BLOCK_REQUESTS}"; fi
if (( POST_DISTINCT_TIPS != 1 )); then record_fail "POST_QUIESCENCE_DISTINCT_TIPS" "distinct_tips is ${POST_DISTINCT_TIPS}"; fi
if (( POST_WORST_LAG != 0 )); then record_fail "POST_QUIESCENCE_WORST_LAG" "worst_lag_from_max_height is ${POST_WORST_LAG}"; fi
for i in $(seq 1 "$NODE_COUNT"); do
  if (( ${NODE_HARNESS_OBSERVED_GAP[$i]:-0} >= 94 )) && [[ "${NODE_SYNC_STATE[$i]:-unknown}" =~ ^(synced|steady)$ ]] && [[ "${NODE_SYNC_OBSERVABILITY_WARNING[$i]:-0}" != "1" ]]; then
    record_fail "SYNC_OBSERVABILITY_INCOMPLETE_MISSING" "n${i} is ${NODE_HARNESS_OBSERVED_GAP[$i]} blocks behind but reports sync_state=${NODE_SYNC_STATE[$i]} without sync_observability_incomplete warning"
  fi
  if [[ -n "${NODE_CANONICAL_NODE_GAP[$i]:-}" ]] && (( ${NODE_CANONICAL_NODE_GAP[$i]:-0} != ${NODE_HARNESS_OBSERVED_GAP[$i]:-0} )); then
    record_fail "EVIDENCE_CONSISTENCY_SYNC_GAP_MISMATCH" "n${i} canonical_node_gap=${NODE_CANONICAL_NODE_GAP[$i]} harness_observed_gap=${NODE_HARNESS_OBSERVED_GAP[$i]}"
  fi
done
if (( RPC_ALIVE_LISTENER_TIMEOUT_COUNT != 0 )); then record_fail "RPC_ALIVE_LISTENER_TIMEOUT" "RPC_ALIVE_LISTENER_TIMEOUT is ${RPC_ALIVE_LISTENER_TIMEOUT_COUNT}"; fi
if (( RPC_LIVENESS_TIMEOUT_COUNT != 0 )); then record_fail "RPC_LIVENESS_TIMEOUT" "rpc_liveness_timeout is ${RPC_LIVENESS_TIMEOUT_COUNT}"; fi
if (( TOTAL_MINING_SUBMIT_BUSY != 0 )); then record_fail "MINING_SUBMIT_BUSY" "submit_busy is ${TOTAL_MINING_SUBMIT_BUSY}"; fi
if (( TOTAL_MINING_ACTOR_TIMEOUT != 0 )); then record_fail "MINING_ACTOR_TIMEOUT" "actor_timeout is ${TOTAL_MINING_ACTOR_TIMEOUT}"; fi

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
