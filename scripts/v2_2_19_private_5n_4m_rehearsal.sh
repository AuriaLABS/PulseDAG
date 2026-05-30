#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-1800}
P2P_CONNECT_WAIT_SECS=${P2P_CONNECT_WAIT_SECS:-120}
QUIESCENCE_SECS=${QUIESCENCE_SECS:-90}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
START_TS=$(date +%s)
START_UTC=$(date -u +%FT%TZ)
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

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs" "$OUT_DIR/miners" "$OUT_DIR/nodes" "$OUT_DIR/samples" "$OUT_DIR/summaries"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

declare -a NODE_PIDS=()
declare -a MINER_PIDS=()
: > "$OUT_DIR/process-pids.txt"

declare -A NODE_READY NODE_HEALTHY NODE_TIP NODE_HEIGHT NODE_P2P_OK NODE_PEERS NODE_P2P_INBOUND NODE_P2P_OUTBOUND NODE_CHAIN_ID NODE_ORPHANS NODE_MISSING_PARENTS NODE_SYNC_STATE NODE_SYNC_STAGE NODE_READINESS_SCHEMA_OK NODE_RPC_OK
declare -A PRE_NODE_HEIGHT PRE_NODE_TIP PRE_NODE_ORPHANS PRE_NODE_MISSING_PARENTS PRE_NODE_SYNC_STATE PRE_NODE_PEERS
declare -A miner_submit miner_accept miner_reject miner_template
FAIL_REASONS=()
FAIL_CLASSES=()
WARNINGS=()
RESULT="PENDING"
EXIT_CODE=0
WAIVE_ACCEPTED_BLOCK_GATE=${WAIVE_ACCEPTED_BLOCK_GATE:-0}
WAIVE_ACCEPTED_BLOCK_REASON=${WAIVE_ACCEPTED_BLOCK_REASON:-""}
ACCEPTED_BLOCKS=0
REJECTED_BLOCKS=0
PRE_CONVERGED=0
POST_CONVERGED=0
PRE_WORST_LAG=0
POST_WORST_LAG=0
PRE_DISTINCT_TIPS=0
POST_DISTINCT_TIPS=0
LAG_IMPROVED=0
CLEANUP_STARTED=0
QUIESCENCE_COMPLETED=0
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

record_warn(){ local msg="$1"; echo "WARN: $msg"; WARNINGS+=("$msg"); }
record_fail(){
  local class="$1" msg="$2"
  echo "FAIL[$class]: $msg"
  FAIL_CLASSES+=("$class")
  FAIL_REASONS+=("$class: $msg")
}

safe_curl_json(){
  local url="$1" out="$2" label="${3:-$1}" required="${4:-0}" rc
  if ! curl -fsS --max-time 10 "$url" -o "$out"; then
    rc=$?
    jq -n --arg url "$url" --arg label "$label" --argjson exit_code "$rc" '{ok:false,error:"curl failed",label:$label,url:$url,exit_code:$exit_code}' > "$out" 2>/dev/null || true
    if (( required == 1 )); then record_fail "RPC_UNAVAILABLE" "required endpoint failed: $label ($url)"; else record_warn "optional endpoint failed: $label"; fi
    return 1
  fi
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

port_in_use(){
  local p="$1"
  if command -v ss >/dev/null 2>&1; then ss -ltn "( sport = :$p )" | grep -Eq ":$p\b"; return $?; fi
  if command -v lsof >/dev/null 2>&1; then lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; return $?; fi
  if command -v netstat >/dev/null 2>&1; then netstat -ltn 2>/dev/null | grep -Eq "[:.]$p[[:space:]]"; return $?; fi
  record_warn "no ss/lsof/netstat available for port check"
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
  local role="$1"; shift || true
  local p alive=0
  for p in "$@"; do [[ -n "${p:-}" ]] && kill "$p" 2>/dev/null || true; done
  sleep 1
  for p in "$@"; do [[ -n "${p:-}" ]] && kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null || true; done
  sleep 1
  for p in "$@"; do [[ -n "${p:-}" ]] && kill -0 "$p" 2>/dev/null && alive=1; done
  (( alive == 0 )) || record_fail "CLEANUP_HANG" "$role process cleanup did not terminate cleanly"
}

capture_p2p_gate_failure(){
  local i rpc ep f
  for i in $(seq 1 "$NODE_COUNT"); do
    rpc=$((BASE_RPC_PORT+i))
    for ep in health status readiness p2p/status sync/status; do
      safe_curl_optional "http://127.0.0.1:${rpc}/${ep}" "$OUT_DIR/endpoints/n${i}-${ep//\//_}-p2p-gate.json" "n${i}:/${ep}" || true
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

collect_miner_metrics(){
  local i log
  ACCEPTED_BLOCKS=0
  REJECTED_BLOCKS=0
  for i in $(seq 1 "$MINER_COUNT"); do
    log="$OUT_DIR/logs/miner-${i}.log"
    miner_template[$i]=$(count_matches_file "template" "$log")
    miner_submit[$i]=$(count_matches_file "submit" "$log")
    miner_accept[$i]=$(count_matches_file "accepted" "$log")
    miner_reject[$i]=$(count_matches_file "reject" "$log")
    ACCEPTED_BLOCKS=$((ACCEPTED_BLOCKS + miner_accept[$i]))
    REJECTED_BLOCKS=$((REJECTED_BLOCKS + miner_reject[$i]))
  done
}

collect_state(){
  local suffix="$1" i rpc readiness_has_ready readiness_has_public
  for i in $(seq 1 "$NODE_COUNT"); do
    rpc=$((BASE_RPC_PORT+i))
    safe_curl_optional "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${i}-status-${suffix}.json" "n${i}:/status ${suffix}" && NODE_RPC_OK[$i]=1 || NODE_RPC_OK[$i]=0
    safe_curl_optional "http://127.0.0.1:${rpc}/release" "$OUT_DIR/endpoints/n${i}-release-${suffix}.json" "n${i}:/release ${suffix}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/readiness" "$OUT_DIR/endpoints/n${i}-readiness-${suffix}.json" "n${i}:/readiness ${suffix}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/p2p/status" "$OUT_DIR/endpoints/n${i}-p2p-status-${suffix}.json" "n${i}:/p2p/status ${suffix}" || true
    safe_curl_optional "http://127.0.0.1:${rpc}/sync/status" "$OUT_DIR/endpoints/n${i}-sync-status-${suffix}.json" "n${i}:/sync/status ${suffix}" || true
    NODE_HEIGHT[$i]="$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status-${suffix}.json" 2>/dev/null || echo 0)"
    NODE_TIP[$i]="$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status-${suffix}.json" 2>/dev/null || echo '')"
    NODE_READY[$i]="$(jq -r '.data.ready_for_release // .ready_for_release // 0' "$OUT_DIR/endpoints/n${i}-readiness-${suffix}.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_HEALTHY[$i]="$(jq -r '.ok // .data.ok // 0' "$OUT_DIR/endpoints/n${i}-status-${suffix}.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_PEERS[$i]="$(jq -r '.data.peer_count // (.data.peers|length) // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-${suffix}.json" 2>/dev/null || echo 0)"
    NODE_P2P_INBOUND[$i]="$(jq -r '.data.inbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-${suffix}.json" 2>/dev/null || echo 0)"
    NODE_P2P_OUTBOUND[$i]="$(jq -r '.data.outbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-${suffix}.json" 2>/dev/null || echo 0)"
    NODE_CHAIN_ID[$i]="$(extract_chain_id "$OUT_DIR/endpoints/n${i}-status-${suffix}.json" "$OUT_DIR/endpoints/n${i}-release-${suffix}.json" "$OUT_DIR/endpoints/n${i}-p2p-status-${suffix}.json" || true)"
    NODE_ORPHANS[$i]="$(jq -r '.data.orphan_count // 0' "$OUT_DIR/endpoints/n${i}-sync-status-${suffix}.json" 2>/dev/null || echo 0)"
    NODE_MISSING_PARENTS[$i]="$(jq -r '.data.pending_missing_parents // 0' "$OUT_DIR/endpoints/n${i}-sync-status-${suffix}.json" 2>/dev/null || echo 0)"
    NODE_SYNC_STATE[$i]="$(jq -r '.data.sync_state // "unknown"' "$OUT_DIR/endpoints/n${i}-sync-status-${suffix}.json" 2>/dev/null || echo unknown)"
    NODE_SYNC_STAGE[$i]="$(jq -r '.data.catchup_stage // "unknown"' "$OUT_DIR/endpoints/n${i}-sync-status-${suffix}.json" 2>/dev/null || echo unknown)"
    NODE_P2P_OK[$i]=$(( NODE_PEERS[$i] > 0 ? 1 : 0 ))
    readiness_has_ready=$(jq -e '(.data.ready_for_release? // .ready_for_release?) | type == "boolean"' "$OUT_DIR/endpoints/n${i}-readiness-${suffix}.json" >/dev/null 2>&1 && echo 1 || echo 0)
    readiness_has_public=$(jq -e '(.data.public_testnet_ready? // .public_testnet_ready?) == false' "$OUT_DIR/endpoints/n${i}-readiness-${suffix}.json" >/dev/null 2>&1 && echo 1 || echo 0)
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
    --argjson quiescence_secs "$QUIESCENCE_SECS" \
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
    echo "- mining duration target (s): $DURATION_SECS"
    echo "- quiescence duration target (s): $QUIESCENCE_SECS"
    echo
    echo "## Status/readiness per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: healthy=${NODE_HEALTHY[$i]:-0} ready=${NODE_READY[$i]:-0} readiness_schema_ok=${NODE_READINESS_SCHEMA_OK[$i]:-0}"; done
    echo
    echo "## P2P status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: peers=${NODE_PEERS[$i]:-0} inbound=${NODE_P2P_INBOUND[$i]:-0} outbound=${NODE_P2P_OUTBOUND[$i]:-0} ok=${NODE_P2P_OK[$i]:-0}"; done
    echo
    echo "## Sync/orphan status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: sync_state=${NODE_SYNC_STATE[$i]:-unknown} catchup_stage=${NODE_SYNC_STAGE[$i]:-unknown} orphan_count=${NODE_ORPHANS[$i]:-0} missing_parent_count=${NODE_MISSING_PARENTS[$i]:-0}"; done
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
    echo "## Required gates"
    echo "| gate | status |"
    echo "|---|---|"
    echo "| 5 nodes launched | $( (( ${#NODE_PIDS[@]}==5 )) && echo PASS || echo FAIL ) |"
    echo "| ${MINER_COUNT} miners launched | $( (( ${#MINER_PIDS[@]}==MINER_COUNT )) && echo PASS || echo FAIL ) |"
    echo "| bootnode peer id extracted | $([[ -n "${NODE_1_ID:-}" ]] && echo PASS || echo FAIL) |"
    echo "| all nodes healthy/status | $(for i in $(seq 1 "$NODE_COUNT"); do [[ "${NODE_HEALTHY[$i]:-0}" == "1" ]] || { echo FAIL; break; }; [[ $i == $NODE_COUNT ]] && echo PASS; done) |"
    echo "| all nodes readiness schema | $(for i in $(seq 1 "$NODE_COUNT"); do [[ "${NODE_READINESS_SCHEMA_OK[$i]:-0}" == "1" ]] || { echo FAIL; break; }; [[ $i == $NODE_COUNT ]] && echo PASS; done) |"
    echo "| peer count network non-zero | $( (( $(for i in $(seq 1 "$NODE_COUNT"); do echo ${NODE_PEERS[$i]:-0}; done | awk '{s+=$1} END{print s+0}') > 0 )) && echo PASS || echo FAIL ) |"
    echo "| heights above genesis | $(for i in $(seq 1 "$NODE_COUNT"); do (( ${NODE_HEIGHT[$i]:-0} > 0 )) || { echo FAIL; break; }; [[ $i == $NODE_COUNT ]] && echo PASS; done) |"
    echo "| miners receive templates | $(for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_template[$i]:-0} > 0 )) || { echo FAIL; break; }; [[ $i == $MINER_COUNT ]] && echo PASS; done) |"
    echo "| miners submit work | $(for i in $(seq 1 "$MINER_COUNT"); do (( ${miner_submit[$i]:-0} > 0 )) || { echo FAIL; break; }; [[ $i == $MINER_COUNT ]] && echo PASS; done) |"
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
    --argjson pre_converged "$PRE_CONVERGED" \
    --argjson post_converged "$POST_CONVERGED" \
    --argjson pre_worst_lag "$PRE_WORST_LAG" \
    --argjson post_worst_lag "$POST_WORST_LAG" \
    --argjson distinct_final_tips "$POST_DISTINCT_TIPS" \
    --argjson lag_improved "$LAG_IMPROVED" \
    --argjson nodes "$(for i in $(seq 1 "$NODE_COUNT"); do jq -n --arg node "n$i" --arg chain_id "${NODE_CHAIN_ID[$i]:-}" --argjson height "${NODE_HEIGHT[$i]:-0}" --arg tip "${NODE_TIP[$i]:-}" --argjson peer_count "${NODE_PEERS[$i]:-0}" --argjson orphan_count "${NODE_ORPHANS[$i]:-0}" --argjson missing_parent_count "${NODE_MISSING_PARENTS[$i]:-0}" --arg sync_state "${NODE_SYNC_STATE[$i]:-unknown}" '{node:$node,chain_id:$chain_id,height:$height,tip:$tip,peer_count:$peer_count,orphan_count:$orphan_count,missing_parent_count:$missing_parent_count,sync_state:$sync_state}'; done | jq -s '.')" \
    --argjson miners "$(for i in $(seq 1 "$MINER_COUNT"); do jq -n --arg miner "miner-$i" --argjson templates "${miner_template[$i]:-0}" --argjson submits "${miner_submit[$i]:-0}" --argjson accepted "${miner_accept[$i]:-0}" --argjson rejected "${miner_reject[$i]:-0}" '{miner:$miner,templates:$templates,submits:$submits,accepted:$accepted,rejected:$rejected}'; done | jq -s '.')" \
    '{stage:$stage,chain_id:$chain_id,version:$version,commit:$commit,tip:$tip,miner_count:$miner_count,accepted_blocks:$accepted_blocks,rejected_blocks:$rejected_blocks,convergence:{before_quiescence:($pre_converged==1),after_quiescence:($post_converged==1),pre_worst_lag_from_max_height:$pre_worst_lag,post_worst_lag_from_max_height:$post_worst_lag,distinct_final_tips:$distinct_final_tips,lag_improved_during_quiescence:($lag_improved==1)},nodes:$nodes,miners:$miners}' \
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
  for i in $(seq 1 "$NODE_COUNT"); do cp "$OUT_DIR/logs/n${i}.log" "$OUT_DIR/nodes/n${i}.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do cp "$OUT_DIR/logs/miner-${i}.log" "$OUT_DIR/miners/miner-${i}.log" 2>/dev/null || true; done
  local tar_tmp
  tar_tmp=$(mktemp -p /tmp evidence.XXXXXX.tar.gz)
  (cd "$OUT_DIR" && tar -czf "$tar_tmp" --exclude='evidence.tar.gz' --exclude='evidence.tar.gz.sha256' endpoints logs miners nodes samples summaries evidence-summary.md command-log.txt process-pids.txt p2p_convergence.json final-convergence-table.json quiescence-metrics.json restart_rejoin.log 2>/dev/null || true)
  mv "$tar_tmp" "$OUT_DIR/evidence.tar.gz"
  (cd "$OUT_DIR" && sha256sum evidence.tar.gz > evidence.tar.gz.sha256)
  cp "$OUT_DIR/evidence.tar.gz" "$OUT_DIR_ROOT/evidence.tar.gz" 2>/dev/null || true
  cp "$OUT_DIR/evidence.tar.gz.sha256" "$OUT_DIR_ROOT/evidence.tar.gz.sha256" 2>/dev/null || true
  (cd "$OUT_DIR_ROOT" && test -s evidence.tar.gz && test -s evidence.tar.gz.sha256 && sha256sum -c evidence.tar.gz.sha256)
  (cd "$OUT_DIR" && test -s evidence.tar.gz && test -s evidence.tar.gz.sha256 && sha256sum -c evidence.tar.gz.sha256)
}

cleanup(){
  local exit_code=$?
  EXIT_CODE=$exit_code
  CLEANUP_STARTED=1
  if (( exit_code != 0 && ${#FAIL_REASONS[@]} == 0 )); then record_fail "HARNESS_TIMEOUT" "script exited non-zero before classified failure: $exit_code"; fi
  collect_state "cleanup-final" || true
  if (( QUIESCENCE_COMPLETED == 0 )); then
    compute_metrics_from_current PRE || true
    POST_CONVERGED=$PRE_CONVERGED; POST_WORST_LAG=$PRE_WORST_LAG; POST_DISTINCT_TIPS=$PRE_DISTINCT_TIPS; LAG_IMPROVED=0
    write_quiescence_metrics || true
  fi
  capture_log_tails || true
  if (( ${#FAIL_REASONS[@]} == 0 )); then RESULT="PASS"; else RESULT="FAIL"; fi
  write_evidence_summary || true
  write_p2p_convergence_json || true
  write_restart_rejoin_log || true
  stop_pids "miner" "${MINER_PIDS[@]:-}"
  stop_pids "node" "${NODE_PIDS[@]:-}"
  wait || true
  package_evidence || true
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

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
    safe_curl_json "http://127.0.0.1:${rpc}/status" "$OUT_DIR/endpoints/n${idx}-status-ready.json" "n${idx}:/status ready" 0 && return 0
    sleep 2
  done
  record_fail "RPC_UNAVAILABLE" "node n${idx} failed status readiness polling"
  return 1
}

start_node 1 $((BASE_RPC_PORT+1)) $((BASE_P2P_PORT+1)) ""
sleep 3
safe_curl_required "http://127.0.0.1:$((BASE_RPC_PORT+1))/p2p/status" "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json" "n1:/p2p/status bootstrap"
NODE_1_ID=$(jq -r '.data.peer_id // .data.local_node_id // empty' "$OUT_DIR/endpoints/n1-p2p-status-bootstrap.json" 2>/dev/null || true)
if [[ -z "$NODE_1_ID" ]]; then
  record_fail "READINESS_SCHEMA_MISMATCH" "failed to extract bootnode peer id from n1 /p2p/status"
  echo "FATAL: unable to build bootnode multiaddr because peer id extraction failed"
  exit 1
fi
BOOT_1="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+1))/p2p/${NODE_1_ID}"
echo "$BOOT_1" > "$OUT_DIR/bootnode.txt"
for i in 2 3 4 5; do start_node "$i" $((BASE_RPC_PORT+i)) $((BASE_P2P_PORT+i)) "$BOOT_1"; done
sleep 3

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
  sleep 2
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
  collect_state "sample" || true
  compute_metrics_from_current PRE || true
  total_orphans=0; total_missing=0
  for i in $(seq 1 "$NODE_COUNT"); do total_orphans=$((total_orphans + ${NODE_ORPHANS[$i]:-0})); total_missing=$((total_missing + ${NODE_MISSING_PARENTS[$i]:-0})); done
  echo "$(date -u +%FT%TZ),${NODE_HEIGHT[1]:-0},${NODE_HEIGHT[2]:-0},${NODE_HEIGHT[3]:-0},${NODE_HEIGHT[4]:-0},${NODE_HEIGHT[5]:-0},$PRE_CONVERGED,$PRE_DISTINCT_TIPS,$total_orphans,$total_missing" >> "$OUT_DIR/samples/height-samples.csv"
  sleep 10
done

collect_state "pre-quiescence"
snapshot_current_as_pre
compute_metrics_from_current PRE
stop_pids "miner" "${MINER_PIDS[@]:-}"
MINER_PIDS=()
echo "miners stopped; waiting ${QUIESCENCE_SECS}s for quiescence"
sleep "$QUIESCENCE_SECS"
collect_state "final"
compute_metrics_from_current POST
write_quiescence_metrics
QUIESCENCE_COMPLETED=1

for i in $(seq 1 "$NODE_COUNT"); do
  [[ "${NODE_RPC_OK[$i]:-0}" == "1" ]] || record_fail "RPC_UNAVAILABLE" "node n${i} final /status unavailable"
  [[ "${NODE_HEALTHY[$i]:-0}" == "1" ]] || record_fail "RPC_UNAVAILABLE" "node n${i} unhealthy"
  [[ "${NODE_READINESS_SCHEMA_OK[$i]:-0}" == "1" ]] || record_fail "READINESS_SCHEMA_MISMATCH" "node n${i} readiness missing ready_for_release boolean or public_testnet_ready=false"
  (( ${NODE_HEIGHT[$i]:-0} > 0 )) || record_fail "SYNC_DIVERGED" "node n${i} did not advance"
  [[ "${NODE_P2P_OK[$i]:-0}" == "1" ]] || record_fail "P2P_NOT_CONNECTED" "node n${i} missing peers"
  [[ -n "${NODE_CHAIN_ID[$i]:-}" ]] || record_fail "READINESS_SCHEMA_MISMATCH" "node n${i} chain_id missing (/status,/release,/p2p/status)"
  [[ "${NODE_CHAIN_ID[$i]:-}" == "$CHAIN_ID_EXPECTED" ]] || record_fail "READINESS_SCHEMA_MISMATCH" "node n${i} chain_id mismatch: got=${NODE_CHAIN_ID[$i]:-unset} expected=$CHAIN_ID_EXPECTED"
  (( ${NODE_MISSING_PARENTS[$i]:-0} == 0 && ${NODE_ORPHANS[$i]:-0} == 0 )) || record_fail "MISSING_PARENT_BACKLOG" "node n${i} orphan_count=${NODE_ORPHANS[$i]:-0} missing_parent_count=${NODE_MISSING_PARENTS[$i]:-0}"
done

(( POST_CONVERGED == 1 )) || record_fail "SYNC_DIVERGED" "post-quiescence convergence failed: distinct_tips=$POST_DISTINCT_TIPS worst_lag=$POST_WORST_LAG"

for i in $(seq 1 "$MINER_COUNT"); do
  (( ${miner_template[$i]:-0} > 0 )) || record_fail "MINER_NO_TEMPLATE" "miner-${i} did not receive templates"
  (( ${miner_submit[$i]:-0} > 0 )) || record_fail "MINER_NO_ACCEPTED_BLOCKS" "miner-${i} did not submit work"
done

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
