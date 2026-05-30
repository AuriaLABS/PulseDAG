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
OUT_DIR_BASE="${OUT_DIR:-$ROOT_DIR/artifacts/v2_2_19/private_5n_4m_rehearsal}"
RUN_DIR="$OUT_DIR_BASE/$RUN_ID"
OUT_DIR_ROOT="$OUT_DIR_BASE"
OUT_DIR="$RUN_DIR"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
NODE_COUNT=5
MINER_COUNT=4
NETWORK_PROFILE="private"
CHAIN_ID_EXPECTED="pulsedag-private"
BASE_RPC_PORT=28544
BASE_P2P_PORT=32302

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs" "$OUT_DIR/miners" "$OUT_DIR/nodes" "$OUT_DIR/samples" "$OUT_DIR/summaries"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

declare -a NODE_PIDS=()
: > "$OUT_DIR/process-pids.txt"
declare -a MINER_PIDS=()
declare -A NODE_READY NODE_HEALTHY NODE_ADVANCED NODE_TIP NODE_HEIGHT NODE_P2P_OK NODE_PEERS NODE_P2P_INBOUND NODE_P2P_OUTBOUND NODE_CHAIN_ID NODE_ORPHAN_COUNT NODE_PENDING_MISSING_PARENTS NODE_INV_HASHES_REQUESTED NODE_PEER_RECOVERY_SUCCESS_COUNT NODE_MISSING_PARENTS_COUNT
FAIL_REASONS=()
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
declare -A miner_submit miner_accept miner_template
for i in 1 2 3 4; do miner_submit[$i]=0; miner_accept[$i]=0; miner_template[$i]=0; done
REPO_COMMIT="$(git -C "$ROOT_DIR" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
NODE_VERSION="$("$NODE_BIN" --version 2>/dev/null | head -n1 || echo unknown)"

text_has_match(){
  local pattern="$1" file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -qi -- "$pattern" "$file"
  else
    grep -Eqi -- "$pattern" "$file"
  fi
}

count_matches_in_logs(){
  local pattern="$1"
  if command -v rg >/dev/null 2>&1; then
    rg -ci -- "$pattern" "$OUT_DIR"/logs/miner-*.log 2>/dev/null | awk -F: '{s+=$2} END {print s+0}'
  else
    grep -Eih -c -- "$pattern" "$OUT_DIR"/logs/miner-*.log 2>/dev/null | awk '{s+=$1} END {print s+0}'
  fi
}

record_warn(){ local msg; msg="$1"; echo "WARN: $msg"; WARNINGS+=("$msg"); }
record_fail(){ local msg; msg="$1"; echo "FAIL: $msg"; FAIL_REASONS+=("$msg"); }

assert_global_deadline(){
  (( ${IN_CLEANUP:-0} == 1 )) && return 0
  if (( $(date +%s) >= GLOBAL_DEADLINE_TS )); then
    echo "FATAL: global deadline exceeded after ${GLOBAL_DEADLINE_SECS}s"
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
    (( remaining > 0 )) || { echo "FATAL: global deadline exhausted before sleep"; exit 124; }
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
    kill -TERM $$ 2>/dev/null || true
  ) &
  DEADLINE_WATCHDOG_PID=$!
}

safe_curl_json(){
  local url out label required rc
  url="$1"; out="$2"; label="${3:-$url}"; required="${4:-0}"
  assert_global_deadline
  if ! curl -fsS --connect-timeout "$CURL_CONNECT_TIMEOUT_SECS" --max-time "$CURL_MAX_TIME_SECS" "$url" -o "$out"; then
    rc=$?
    jq -n --arg url "$url" --argjson exit_code "$rc" '{ok:false,error:"curl failed",url:$url,exit_code:$exit_code}' > "$out"
    if (( required == 1 )); then record_fail "required endpoint failed: $url"; else record_warn "optional endpoint failed: $label"; fi
    return 1
  fi
}
safe_curl_required(){ safe_curl_json "$1" "$2" "${3:-$1}" 1; }
safe_curl_optional(){ safe_curl_json "$1" "$2" "${3:-$1}" 0; }
json_get_or_default(){ local expr file def; expr="$1"; file="$2"; def="$3"; jq -r "$expr // $def" "$file" 2>/dev/null || echo "$def"; }

extract_chain_id(){
  local status_file="$1" release_file="$2" p2p_file="$3"
  jq -r '
    .data.chain_id // .chain_id // empty
  ' "$status_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' && return 0
  jq -r '
    .data.chain_id // .chain_id // .data.network_id // .network_id // empty
  ' "$release_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' && return 0
  jq -r '
    .data.chain_id // .chain_id // empty
  ' "$p2p_file" 2>/dev/null | head -n1 | awk 'NF {print; exit}' && return 0
  return 1
}

port_in_use(){
  local p="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltn "( sport = :$p )" | grep -Eq ":$p\b"
    return $?
  fi
  if command -v lsof >/dev/null 2>&1; then lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; return $?; fi
  if command -v netstat >/dev/null 2>&1; then
    netstat -ltn 2>/dev/null | grep -Eq "[:.]$p[[:space:]]"
    return $?
  fi
  echo "WARN: no ss/lsof/netstat available for port check"
  return 1
}

ensure_ports_free(){
  local -a ports=()
  for i in $(seq 1 "$NODE_COUNT"); do
    ports+=("$((BASE_RPC_PORT+i))" "$((BASE_P2P_PORT+i))")
  done
  for p in "${ports[@]}"; do
    if port_in_use "$p"; then
      echo "FATAL: port $p is already in use"
      command -v ss >/dev/null 2>&1 && ss -ltnp "( sport = :$p )" || true
      exit 1
    fi
  done
}

stop_pids(){ for p in "$@"; do kill "$p" 2>/dev/null || true; done; sleep_with_deadline 1; for p in "$@"; do kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null || true; done; }
capture_p2p_gate_failure(){
  local i
  for i in 1 2 3 4 5; do
    rpc=$((BASE_RPC_PORT+i))
    for ep in health status readiness p2p/status sync/status sync/missing orphans; do
      safe_curl_optional "http://127.0.0.1:${rpc}/${ep}" "$OUT_DIR/endpoints/n${i}-${ep//\//_}.json" "n${i}:/${ep}" || true
    done
  done
  {
    echo "# p2p gate failure diagnostics"
    echo "- bootnode: $(cat "$OUT_DIR/bootnode.txt" 2>/dev/null || echo unknown)"
    for i in 1 2 3 4 5; do
      f="$OUT_DIR/endpoints/n${i}-p2p_status.json"
      echo "- n${i}.peer_count: $(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$f" 2>/dev/null || echo 0)"
      echo "- n${i}.connected_peers: $(jq -c '.data.connected_peers // .data.connected_peer_ids // []' "$f" 2>/dev/null || echo '[]')"
    done
  } > "$OUT_DIR/p2p-gate-failure.md"
}

capture_log_tails(){
  for i in $(seq 1 "$NODE_COUNT"); do tail -n 120 "$OUT_DIR/logs/n${i}.log" > "$OUT_DIR/logs/n${i}-tail.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do tail -n 120 "$OUT_DIR/logs/miner-${i}.log" > "$OUT_DIR/logs/miner-${i}-tail.log" 2>/dev/null || true; done
}

collect_final_state(){
  local phase
  phase="${1:-final}"
  for i in $(seq 1 "$NODE_COUNT"); do
    local rpc
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
    NODE_P2P_OK[$i]=$(( NODE_PEERS[$i] > 0 ? 1 : 0 ))
  done
}

write_evidence_summary(){
  local end_ts now_utc duration
  end_ts=$(date +%s); now_utc=$(date -u +%FT%TZ); duration=$((end_ts - START_TS))
  {
    echo "# v2.2.19 Private 5N/4M Rehearsal Evidence"
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
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: healthy=${NODE_HEALTHY[$i]:-0} ready=${NODE_READY[$i]:-0}"; done
    echo
    echo "## P2P status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: peers=${NODE_PEERS[$i]:-0} inbound=${NODE_P2P_INBOUND[$i]:-0} outbound=${NODE_P2P_OUTBOUND[$i]:-0} ok=${NODE_P2P_OK[$i]:-0}"; done
    echo
    echo "## Chain identity per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: chain_id=${NODE_CHAIN_ID[$i]:-unknown}"; done
    echo
    echo "## Final convergence table"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: height=${NODE_HEIGHT[$i]:-0} tip=${NODE_TIP[$i]:-}"; done
    echo
    echo "## Miner summaries"
    for i in $(seq 1 "$MINER_COUNT"); do
      echo "- miner-${i}: templates=${miner_template[$i]:-0} submit=${miner_submit[$i]:-0} accepted=${miner_accept[$i]:-0}"
    done
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
    echo "| 4 miners launched | $( (( ${#MINER_PIDS[@]}>=4 || MINERS_STOPPED_FOR_QUIESCENCE==1 )) && echo PASS || echo FAIL ) |"
    echo "| bootnode peer id extracted | $([[ -n "${NODE_1_ID:-}" ]] && echo PASS || echo FAIL) |"
    echo "| all nodes healthy/status | $( for i in $(seq 1 "$NODE_COUNT"); do [[ "${NODE_HEALTHY[$i]:-0}" == "1" ]] || exit 1; done; echo PASS ) |"
    echo "| all nodes readiness (baseline/intermediate only) | $( for i in $(seq 1 "$NODE_COUNT"); do [[ "${NODE_READY[$i]:-0}" == "1" ]] || exit 1; done; echo PASS ) |"
    echo "| peer count network non-zero | $( (( $(for i in $(seq 1 "$NODE_COUNT"); do echo ${NODE_PEERS[$i]:-0}; done | awk '{s+=$1} END{print s+0}') > 0 )) && echo PASS || echo FAIL ) |"
    echo "| heights above genesis | $( for i in $(seq 1 "$NODE_COUNT"); do (( ${NODE_HEIGHT[$i]:-0} > 0 )) || exit 1; done; echo PASS ) |"
    echo "| baseline miner receives templates | $( (( ${miner_template[1]:-0} == 1 )) && echo PASS || echo FAIL ) |"
    echo "| baseline miner submits work | $( (( ${miner_submit[1]:-0} == 1 )) && echo PASS || echo FAIL ) |"
    echo "| intermediate miners receive templates | $( for i in 1 2; do (( ${miner_template[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    echo "| intermediate miners submit work | $( for i in 1 2; do (( ${miner_submit[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    echo "| stress miners receive templates (evidence only) | $( for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_template[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    echo "| stress miners submit work (evidence only) | $( for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_submit[$i]:-0} == 1 )) || exit 1; done; echo PASS ) |"
    echo "| accepted blocks >0 (or waived) | $( (( ACCEPTED_BLOCKS>0 || WAIVE_ACCEPTED_BLOCK_GATE==1 )) && echo PASS || echo FAIL ) |"
    echo "## Build/runtime metadata"
    echo "- commit: $REPO_COMMIT"
    echo "- version: $NODE_VERSION"
    echo
    echo "## Result"
    echo "- result: $RESULT"
    echo "- exit_code: $EXIT_CODE"
    echo "- node_count: $NODE_COUNT"
    echo "- miner_count: $MINER_COUNT"
    echo "- warnings:"
    if (( ${#WARNINGS[@]} > 0 )); then for w in "${WARNINGS[@]}"; do echo "  - $w"; done; else echo "  - none"; fi
    echo "- failure reasons:"
    if (( ${#FAIL_REASONS[@]} > 0 )); then for r in "${FAIL_REASONS[@]}"; do echo "  - $r"; done; else echo "  - none"; fi
  } > "$OUT_DIR/evidence-summary.md"
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  printf "%s
" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt"
}

write_p2p_convergence_json(){
  jq -n \
    --arg chain_id "$CHAIN_ID_EXPECTED" \
    --arg version "$NODE_VERSION" \
    --arg commit "$REPO_COMMIT" \
    --arg tip "${NODE_TIP[1]:-}" \
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
    echo "note=this rehearsal validates steady-state convergence; restart/rejoin drill not invoked by this script"
    echo "timestamp_utc=$(date -u +%FT%TZ)"
  } > "$OUT_DIR/restart_rejoin.log"
}

write_metadata(){
  {
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

package_evidence(){
  write_metadata || true
  cp "$OUT_DIR/p2p_convergence.json" "$OUT_DIR/final-convergence-table.json" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR/summaries/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/command-log.txt" "$OUT_DIR_ROOT/command-log.txt" 2>/dev/null || true
  cp "$OUT_DIR/bootnode.txt" "$OUT_DIR_ROOT/bootnode.txt" 2>/dev/null || true
  printf "%s\n" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt"
  for i in $(seq 1 "$NODE_COUNT"); do
    cp "$OUT_DIR/logs/n${i}.log" "$OUT_DIR/nodes/n${i}.log" 2>/dev/null || true
  done
  for i in $(seq 1 "$MINER_COUNT"); do
    cp "$OUT_DIR/logs/miner-${i}.log" "$OUT_DIR/miners/miner-${i}.log" 2>/dev/null || true
  done
  local tar_tmp
  tar_tmp=$(mktemp -p /tmp evidence.XXXXXX.tar.gz)
  (cd "$OUT_DIR" && tar -czf "$tar_tmp" --exclude='evidence.tar.gz' --exclude='evidence.tar.gz.sha256' endpoints logs miners nodes samples summaries evidence-summary.md command-log.txt process-pids.txt p2p_convergence.json final-convergence-table.json restart_rejoin.log 2>/dev/null || true)
  mv "$tar_tmp" "$OUT_DIR/evidence.tar.gz"
  (cd "$OUT_DIR" && sha256sum evidence.tar.gz > evidence.tar.gz.sha256)
  cp "$OUT_DIR/evidence.tar.gz" "$OUT_DIR_ROOT/evidence.tar.gz" 2>/dev/null || true
  cp "$OUT_DIR/evidence.tar.gz.sha256" "$OUT_DIR_ROOT/evidence.tar.gz.sha256" 2>/dev/null || true
  (cd "$OUT_DIR_ROOT" && test -s evidence.tar.gz && test -s evidence.tar.gz.sha256 && sha256sum -c evidence.tar.gz.sha256)
  (cd "$OUT_DIR" && test -s evidence.tar.gz && test -s evidence.tar.gz.sha256 && sha256sum -c evidence.tar.gz.sha256)
}


cleanup(){
  local exit_code=$?
  IN_CLEANUP=1
  EXIT_CODE=$exit_code
  if (( exit_code != 0 )); then
    record_fail "script exited non-zero: $exit_code"
  fi
  if (( ${#FAIL_REASONS[@]} == 0 )); then RESULT="PASS"; else RESULT="FAIL"; fi
  collect_final_state cleanup || true
  capture_log_tails || true
  write_evidence_summary || true
  write_p2p_convergence_json || true
  write_restart_rejoin_log || true
  [[ -n "${DEADLINE_WATCHDOG_PID:-}" ]] && kill "$DEADLINE_WATCHDOG_PID" 2>/dev/null || true
  stop_pids "${MINER_PIDS[@]:-}"; stop_pids "${NODE_PIDS[@]:-}"; wait || true
  package_evidence || true
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

start_global_deadline_watchdog
OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"
ensure_ports_free
cargo build --workspace --release --locked

start_node(){
  local idx rpc p2p bootnode name data
  idx="$1"
  rpc="$2"
  p2p="$3"
  bootnode="$4"
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
  local idx rpc
  idx="$1"
  rpc=$((BASE_RPC_PORT+idx))
  for _ in $(seq 1 60); do
    safe_curl_required "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${idx}-status-ready.json" && return 0
    sleep_with_deadline 2
  done
  record_fail "node n${idx} failed readiness"
  return 1
}

start_node 1 $((BASE_RPC_PORT+1)) $((BASE_P2P_PORT+1)) ""; sleep_with_deadline 3
safe_curl_required "http://127.0.0.1:$((BASE_RPC_PORT+1))/p2p/status" "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json"
NODE_1_ID=$(jq -r '.data.peer_id // .data.local_node_id // empty' "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json" 2>/dev/null || true)
if [[ -z "$NODE_1_ID" ]]; then
  record_fail "failed to extract bootnode peer id from n1 log"
  echo "FATAL: unable to build bootnode multiaddr because peer id extraction failed"
  exit 1
fi
BOOT_1=""; [[ -n "$NODE_1_ID" ]] && BOOT_1="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+1))/p2p/${NODE_1_ID}"
echo "$BOOT_1" > "$OUT_DIR/bootnode.txt"
for i in 2 3 4 5; do start_node "$i" $((BASE_RPC_PORT+i)) $((BASE_P2P_PORT+i)) "$BOOT_1"; done
sleep_with_deadline 3

for i in 1 2 3 4 5; do wait_node_ready "$i" || true; done

peer_wait_deadline=$(( $(date +%s) + P2P_CONNECT_WAIT_SECS ))
peers_total=0
while (( $(date +%s) < peer_wait_deadline )); do
  peers_total=0
  for i in 1 2 3 4 5; do
    safe_curl_optional "http://127.0.0.1:$((BASE_RPC_PORT+i))/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" "n${i}:/p2p/status pre-mining" || true
    peers_total=$((peers_total + $(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-pre-mining.json" 2>/dev/null || echo 0)))
  done
  (( peers_total > 0 )) && break
  sleep_with_deadline 2
done
(( peers_total > 0 )) || { capture_p2p_gate_failure; record_fail "pre-mining p2p peers remained zero after ${P2P_CONNECT_WAIT_SECS}s"; exit 1; }

for i in 1 2 3 4; do
  local_node="http://127.0.0.1:$((BASE_RPC_PORT+i))"
  echo "launch miner-${i}: $MINER_BIN --node $local_node --miner-address v2219-${RUN_ID}-miner-${i} --backend cpu --threads 1 --loop"
  "$MINER_BIN" --node "$local_node" --miner-address "v2219-${RUN_ID}-miner-${i}" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner-${i}.log" 2>&1 &
  MINER_PIDS+=("$!")
  echo "$! miner-${i}" >> "$OUT_DIR/process-pids.txt"
done

printf "timestamp,n1,n2,n3,n4,n5,tip_match\n" > "$OUT_DIR/samples/height-samples.csv"

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

  for i in 1 2 3 4; do
    text_has_match "template" "$OUT_DIR/logs/miner-${i}.log" && miner_template[$i]=1 || true
    text_has_match "submit" "$OUT_DIR/logs/miner-${i}.log" && miner_submit[$i]=1 || true
    text_has_match "accepted" "$OUT_DIR/logs/miner-${i}.log" && miner_accept[$i]=1 || true
  done
  ACCEPTED_BLOCKS=$(count_matches_in_logs "accepted")
  REJECTED_BLOCKS=$(count_matches_in_logs "reject")
  (( ACCEPTED_BLOCKS > 0 )) && TEMPLATES_OK=1
  sleep_with_deadline 10
done

echo "entering quiescence: stopping miners and waiting ${QUIESCENCE_WAIT_SECS}s before final tips/readiness sample"
stop_pids "${MINER_PIDS[@]:-}"
MINERS_STOPPED_FOR_QUIESCENCE=1
sleep_with_deadline "$QUIESCENCE_WAIT_SECS"
collect_final_state quiescent

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
for i in 1 2 3 4; do
  (( ${miner_template[$i]:-0} == 1 )) || STRESS_OK=0
  (( ${miner_submit[$i]:-0} == 1 )) || STRESS_OK=0
done

(( BASELINE_OK == 1 )) && GATE_5N_1M_BASELINE=PASS || GATE_5N_1M_BASELINE=FAIL
(( INTERMEDIATE_OK == 1 )) && GATE_5N_2M_INTERMEDIATE=PASS || GATE_5N_2M_INTERMEDIATE=FAIL
(( STRESS_OK == 1 )) && GATE_5N_4M_STRESS=PASS || GATE_5N_4M_STRESS=OBSERVE_FAIL

[[ "$GATE_5N_1M_BASELINE" == "PASS" ]] || record_fail "5N/1M baseline gate failed after quiescence"
[[ "$GATE_5N_2M_INTERMEDIATE" == "PASS" ]] || record_fail "5N/2M intermediate gate failed after quiescence"
if [[ "$GATE_5N_4M_STRESS" != "PASS" ]]; then
  record_warn "5N/4M stress gate did not pass; retained as non-mandatory readiness evidence for v2.2.19"
fi

if (( ACCEPTED_BLOCKS < 1 )); then
  if (( WAIVE_ACCEPTED_BLOCK_GATE == 1 )); then
    [[ -n "$WAIVE_ACCEPTED_BLOCK_REASON" ]] && record_warn "accepted blocks gate waived: $WAIVE_ACCEPTED_BLOCK_REASON" || record_fail "accepted blocks gate waived without reason"
  else
    record_fail "accepted blocks is zero"
  fi
fi

if (( ${#FAIL_REASONS[@]} > 0 )); then
  echo "FAIL private rehearsal: $OUT_DIR"
  echo "FINAL_RESULT=FAIL"
  exit 1
fi

echo "PASS private rehearsal complete: $OUT_DIR"
echo "FINAL_RESULT=PASS"
