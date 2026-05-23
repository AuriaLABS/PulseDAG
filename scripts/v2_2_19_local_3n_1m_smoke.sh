#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-900}
GRACE_SECS=${GRACE_SECS:-120}
SAMPLE_INTERVAL_SECS=${SAMPLE_INTERVAL_SECS:-10}
STARTUP_WAIT_SECS=${STARTUP_WAIT_SECS:-12}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
START_TS=$(date +%s)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_OUT_DIR="$ROOT_DIR/artifacts/v2_2_19/local_3n_1m_smoke/${RUN_ID}"
OUT_DIR=${OUT_DIR:-$DEFAULT_OUT_DIR}
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
MINER_ADDRESS="${MINER_ADDRESS:-v2219-${RUN_ID}-miner-a}"

RPC_BASE_PORT=${RPC_BASE_PORT:-18080}
P2P_BASE_PORT=${P2P_BASE_PORT:-19080}
RPC_PORT_A=${RPC_PORT_A:-$RPC_BASE_PORT}
RPC_PORT_B=${RPC_PORT_B:-$((RPC_BASE_PORT+1))}
RPC_PORT_C=${RPC_PORT_C:-$((RPC_BASE_PORT+2))}
P2P_PORT_A=${P2P_PORT_A:-$P2P_BASE_PORT}
P2P_PORT_B=${P2P_PORT_B:-$((P2P_BASE_PORT+1))}
P2P_PORT_C=${P2P_PORT_C:-$((P2P_BASE_PORT+2))}

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs" "$OUT_DIR/miners" "$OUT_DIR/nodes" "$OUT_DIR/samples" "$OUT_DIR/summaries"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

PIDS=()
WARNINGS=()
FAILURES=()
RESULT="PENDING"
EXIT_CODE=0
WAIVE_ACCEPTED_BLOCK_GATE=${WAIVE_ACCEPTED_BLOCK_GATE:-0}
WAIVE_ACCEPTED_BLOCK_REASON=${WAIVE_ACCEPTED_BLOCK_REASON:-""}

record_warn(){ local msg; msg="$1"; echo "WARN: $msg"; WARNINGS+=("$msg"); }
record_fail(){ local msg; msg="$1"; echo "FAIL: $msg"; FAILURES+=("$msg"); }

safe_curl_required(){ local url out; url="$1"; out="$2"; if ! curl -fsS "$url" -o "$out"; then record_fail "required endpoint failed: $url"; return 1; fi; }
safe_curl_optional(){ local url out label; url="$1"; out="$2"; label="${3:-$url}"; if ! curl -fsS "$url" -o "$out"; then record_warn "optional endpoint failed: $label"; return 1; fi; }
json_get_or_default(){ local expr file def; expr="$1"; file="$2"; def="$3"; jq -r "$expr // $def" "$file" 2>/dev/null || echo "$def"; }

text_has_match(){
  local pattern="$1"
  shift
  if command -v rg >/dev/null 2>&1; then
    rg -qE -- "$pattern" "$@"
  else
    grep -qE -- "$pattern" "$@"
  fi
}

count_matches(){
  local pattern="$1" file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -cE -- "$pattern" "$file" 2>/dev/null || echo 0
  else
    grep -cE -- "$pattern" "$file" 2>/dev/null || echo 0
  fi
}

write_summary(){
  local healthy_count ready_count peers_total chain_id="unknown"
  healthy_count=0
  ready_count=0
  peers_total=0
  for n in a b c; do
    [[ -f "$OUT_DIR/endpoints/${n}-health.json" ]] && [[ "$(jq -r '(.ok // .data.ok // false)' "$OUT_DIR/endpoints/${n}-health.json" 2>/dev/null)" == "true" ]] && ((healthy_count+=1))
    [[ -f "$OUT_DIR/endpoints/${n}-readiness.json" ]] && [[ "$(jq -r '(.data.ready_for_release // .ready_for_release // false)' "$OUT_DIR/endpoints/${n}-readiness.json" 2>/dev/null)" == "true" ]] && ((ready_count+=1))
    if [[ -f "$OUT_DIR/endpoints/${n}-p2p_status.json" ]]; then
      peers_total=$((peers_total + $(jq -r '(.data.peer_count // .data.connected_peer_count // 0)' "$OUT_DIR/endpoints/${n}-p2p_status.json" 2>/dev/null || echo 0)))
    fi
  done
  {
    echo "# v2.2.19 local 3N/1M smoke evidence"
    echo "- result: $RESULT"
    echo "- exit_code: $EXIT_CODE"
    echo "- node_count: 3"
    echo "- miner_count: 1"
    echo "- chain_id: $chain_id"
    echo "- healthy_nodes: $healthy_count"
    echo "- ready_nodes: $ready_count"
    echo "- peers_total: $peers_total"
    echo "- templates_seen: $miner_templates"
    echo "- submissions_seen: $miner_submits"
    echo "- accepted_blocks: $accepted_count"
    echo "- rejected_blocks: ${rejected_count:-0}"
    echo "- final_heights: a=${ha:-0}, b=${hb:-0}, c=${hc:-0}"
    echo "- final_tips: a=${ta:-}, b=${tb:-}, c=${tc:-}"
    echo ""
    echo "## Warnings"
    if (( ${#WARNINGS[@]} == 0 )); then echo "- none"; else for w in "${WARNINGS[@]}"; do echo "- $w"; done; fi
    echo ""
    echo "## Failure reasons"
    if (( ${#FAILURES[@]} == 0 )); then echo "- none"; else for f in "${FAILURES[@]}"; do echo "- $f"; done; fi
    echo ""
    echo "## Required gates"
    echo "| gate | status |"
    echo "|---|---|"
    echo "| 3 nodes launched | $([[ -f "$OUT_DIR/a.pid" && -f "$OUT_DIR/b.pid" && -f "$OUT_DIR/c.pid" ]] && echo PASS || echo FAIL) |"
    echo "| 1 miner launched | $([[ -f "$OUT_DIR/miner.pid" ]] && echo PASS || echo FAIL) |"
    echo "| all nodes healthy/status | $( (( healthy_count==3 )) && echo PASS || echo FAIL ) |"
    echo "| all nodes readiness | $( (( ready_count==3 )) && echo PASS || echo FAIL ) |"
    echo "| miner templates >=1 | $( (( miner_templates==1 )) && echo PASS || echo FAIL ) |"
    echo "| miner submissions >=1 | $( (( miner_submits==1 )) && echo PASS || echo FAIL ) |"
    echo "| accepted blocks >0 (or waived) | $( (( accepted_count>0 || WAIVE_ACCEPTED_BLOCK_GATE==1 )) && echo PASS || echo FAIL ) |"
    echo "| heights > genesis | $( (( ha>0 && hb>0 && hc>0 )) && echo PASS || echo FAIL ) |"
    echo "| final convergence | $( (( final_converged==1 )) && echo PASS || echo FAIL ) |"
  } > "$OUT_DIR/evidence-summary.md"
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
    echo "start_utc=$RUN_ID"
    echo "end_utc=$(date -u +%FT%TZ)"
    echo "duration_seconds=$(( $(date +%s) - START_TS ))"
    echo "exit_code=$EXIT_CODE"
  } > "$OUT_DIR/summaries/package-metadata.txt"
}

package_evidence(){
  write_metadata || true
  cp "$OUT_DIR/logs/miner.log" "$OUT_DIR/miners/miner.log" 2>/dev/null || true
  cp "$OUT_DIR/process-pids.txt" "$OUT_DIR/nodes/process-pids.txt" 2>/dev/null || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR/summaries/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/samples/height-samples.csv" "$OUT_DIR/final-convergence-table.txt" 2>/dev/null || true
  local tar_tmp
  tar_tmp=$(mktemp -p /tmp evidence.XXXXXX.tar.gz)
  (cd "$OUT_DIR" && tar -czf "$tar_tmp" --exclude='evidence.tar.gz' --exclude='evidence.tar.gz.sha256' endpoints logs miners nodes samples summaries evidence-summary.md command-log.txt process-pids.txt final-convergence-table.txt 2>/dev/null || true)
  mv "$tar_tmp" "$OUT_DIR/evidence.tar.gz"
  (cd "$OUT_DIR" && sha256sum evidence.tar.gz > evidence.tar.gz.sha256)
  (cd "$OUT_DIR" && test -s evidence.tar.gz && test -s evidence.tar.gz.sha256 && sha256sum -c evidence.tar.gz.sha256)
}


cleanup(){
  local exit_code=$?
  EXIT_CODE=$exit_code
  echo "[cleanup] terminating spawned processes"
  for p in "${PIDS[@]:-}"; do
    kill "$p" 2>/dev/null || true
  done
  sleep 1
  for p in "${PIDS[@]:-}"; do
    kill -9 "$p" 2>/dev/null || true
  done
  wait || true
  rm -f "$OUT_DIR"/*.pid
  if (( exit_code != 0 )); then
    record_fail "script exited non-zero: $exit_code"
  fi
  if (( ${#FAILURES[@]} == 0 )); then RESULT="PASS"; else RESULT="FAIL"; fi
  write_summary || true
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR/summaries/evidence-summary.md" 2>/dev/null || true
  package_evidence || true
  exit "$exit_code"
}
trap cleanup EXIT INT TERM

is_port_busy(){
  local port="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltn | awk '{print $4}' | grep -Eq "[:.]${port}$"
  elif command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
  elif command -v netstat >/dev/null 2>&1; then
    netstat -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "[:.]${port}$"
  else
    echo "WARN cannot verify ports (missing ss/lsof/netstat)" >&2
    return 1
  fi
}

for port in "$RPC_PORT_A" "$RPC_PORT_B" "$RPC_PORT_C" "$P2P_PORT_A" "$P2P_PORT_B" "$P2P_PORT_C"; do
  if is_port_busy "$port"; then
    echo "FAIL port already in use: ${port}"
    exit 1
  fi
done

OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"

if [[ ! -x "$NODE_BIN" || ! -x "$MINER_BIN" ]]; then
  cargo build --workspace --release --locked
fi
[[ -x "$NODE_BIN" ]] || { echo "FAIL missing binary: $NODE_BIN"; exit 1; }
[[ -x "$MINER_BIN" ]] || { echo "FAIL missing binary: $MINER_BIN"; exit 1; }

start_node(){
  local name="$1" rpc="$2" p2p="$3" bootnode="$4"
  local data="$OUT_DIR/data-$name"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --network "private" --rpc-listen "127.0.0.1:${rpc}" --p2p-listen "/ip4/127.0.0.1/tcp/${p2p}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  PULSEDAG_ROCKSDB_PATH="$data/rocksdb" "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  local pid="$!"
  PIDS+=("$pid")
  echo "$pid" > "$OUT_DIR/${name}.pid"
  echo "$pid node-$name" >> "$OUT_DIR/process-pids.txt"
}

start_node a "$RPC_PORT_A" "$P2P_PORT_A" ""
sleep "$STARTUP_WAIT_SECS"
NODE_A_ID=$(grep -En "local_node_id|peer_id" "$OUT_DIR/logs/a.log" | head -n1 | sed -E 's/.*(12D[[:alnum:]]+).*/\1/' || true)
BOOT_A=""
[[ -n "$NODE_A_ID" ]] && BOOT_A="/ip4/127.0.0.1/tcp/${P2P_PORT_A}/p2p/${NODE_A_ID}"
start_node b "$RPC_PORT_B" "$P2P_PORT_B" "$BOOT_A"
start_node c "$RPC_PORT_C" "$P2P_PORT_C" "$BOOT_A"
sleep "$STARTUP_WAIT_SECS"

"$MINER_BIN" --node "http://127.0.0.1:${RPC_PORT_A}" --miner-address "$MINER_ADDRESS" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner.log" 2>&1 &
PIDS+=("$!")
echo "$!" > "$OUT_DIR/miner.pid"
echo "$! miner" >> "$OUT_DIR/process-pids.txt"

eps=(health status readiness release p2p/status sync/status)
printf "node,endpoint,path,status\n" > "$OUT_DIR/summaries/endpoints-manifest.txt"
printf "timestamp,height_a,height_b,height_c,tip_a,tip_b,tip_c\n" > "$OUT_DIR/samples/height-samples.csv"
printf "timestamp,phase,peers_total,inbound_blocks\n" > "$OUT_DIR/samples/readiness-samples.csv"
printf "timestamp,accepted,rejected\n" > "$OUT_DIR/samples/miner-block-counters.csv"

sample(){
  local node rpc ep path out
  node="$1"
  rpc="$2"
  ep="$3"
  path="/$ep"
  out="$OUT_DIR/endpoints/${node}-${ep//\//_}.json"
  if safe_curl_required "http://127.0.0.1:${rpc}${path}" "$out"; then
    cp "$out" "$OUT_DIR/endpoints/${node}-${ep//\//_}-$(date -u +%s).json"
    echo "$node,$ep,$path,OK" >> "$OUT_DIR/summaries/endpoints-manifest.txt"
  else
    if [[ "$ep" == "p2p/status" || "$ep" == "sync/status" ]]; then
      safe_curl_optional "http://127.0.0.1:${rpc}${path}" "$out" "$node:$ep" || true
      echo "SKIP" > "$OUT_DIR/endpoints/${node}-${ep//\//_}.skip"
      echo "$node,$ep,$path,SKIP_OPTIONAL" >> "$OUT_DIR/summaries/endpoints-manifest.txt"
    else
      echo "$node,$ep,$path,FAIL" >> "$OUT_DIR/summaries/endpoints-manifest.txt"
    fi
  fi
}

miner_templates=0
miner_submits=0
tip_divergence_seen=0
final_converged=0
readiness_phase="no_peers"
accepted_count=0

end=$(( $(date +%s) + DURATION_SECS ))
while (( $(date +%s) < end )); do
  now_epoch=$(date +%s)
  elapsed=$(( now_epoch - (end - DURATION_SECS) ))

  for n in a b c; do
    rpc="$RPC_PORT_A"
    [[ "$n" == b ]] && rpc="$RPC_PORT_B"
    [[ "$n" == c ]] && rpc="$RPC_PORT_C"
    for ep in "${eps[@]}"; do sample "$n" "$rpc" "$ep" || true; done
  done

  ha=$(json_get_or_default '.data.best_height' "$OUT_DIR/endpoints/a-status.json" '0')
  hb=$(json_get_or_default '.data.best_height' "$OUT_DIR/endpoints/b-status.json" '0')
  hc=$(json_get_or_default '.data.best_height' "$OUT_DIR/endpoints/c-status.json" '0')
  ta=$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/a-status.json" 2>/dev/null || echo "")
  tb=$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/b-status.json" 2>/dev/null || echo "")
  tc=$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/c-status.json" 2>/dev/null || echo "")
  echo "$(date -u +%FT%TZ),$ha,$hb,$hc,$ta,$tb,$tc" >> "$OUT_DIR/samples/height-samples.csv"

  if (( elapsed < GRACE_SECS )) && [[ "$ta" != "$tb" || "$tb" != "$tc" ]]; then
    tip_divergence_seen=1
    echo "WARN temporary tip divergence observed during startup grace elapsed=${elapsed}s"
  fi

  pa=$(jq -r ".data.peer_count // .data.connected_peer_count // 0" "$OUT_DIR/endpoints/a-p2p_status.json" 2>/dev/null || echo 0)
  pb=$(jq -r ".data.peer_count // .data.connected_peer_count // 0" "$OUT_DIR/endpoints/b-p2p_status.json" 2>/dev/null || echo 0)
  pc=$(jq -r ".data.peer_count // .data.connected_peer_count // 0" "$OUT_DIR/endpoints/c-p2p_status.json" 2>/dev/null || echo 0)
  inbound_blocks=$(( $(count_matches "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/b.log") + $(count_matches "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/c.log") ))
  echo "$(date -u +%FT%TZ),$readiness_phase,$((pa+pb+pc)),$inbound_blocks" >> "$OUT_DIR/samples/readiness-samples.csv"

  if (( pa + pb + pc == 0 )); then readiness_phase="no_peers";
  elif (( inbound_blocks == 0 )); then readiness_phase="peers_connected_no_propagation";
  elif (( ha>0 && hb>0 && hc>0 )) && [[ "$ta" == "$tb" && "$tb" == "$tc" ]]; then readiness_phase="converged";
  else readiness_phase="propagation_active"; fi

  accepted_count=$(count_matches "[Aa]ccepted" "$OUT_DIR/logs/miner.log")
  rejected_count=$(count_matches "[Rr]eject|[Rr]ejected" "$OUT_DIR/logs/miner.log")
  echo "$(date -u +%FT%TZ),$accepted_count,$rejected_count" >> "$OUT_DIR/samples/miner-block-counters.csv"

  if text_has_match "template" "$OUT_DIR/logs/miner.log"; then miner_templates=1; fi
  if text_has_match "submit|accepted|reject" "$OUT_DIR/logs/miner.log"; then miner_submits=1; fi

  if (( elapsed >= GRACE_SECS )) && (( ha>0 && hb>0 && hc>0 )) && [[ "$ta" == "$tb" && "$tb" == "$tc" ]]; then
    final_converged=1
  fi

  sleep "$SAMPLE_INTERVAL_SECS"
done

for n in a b c; do
  cp "$OUT_DIR/endpoints/${n}-status.json" "$OUT_DIR/final-status-node-${n}.json" 2>/dev/null || true
done

(( final_converged == 1 )) || record_fail "final convergence not reached within deadline (duration=${DURATION_SECS}s, grace=${GRACE_SECS}s)"
text_has_match "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/b.log" || record_fail "node_b missing inbound p2p block activity"
text_has_match "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/c.log" || record_fail "node_c missing inbound p2p block activity"
(( miner_templates == 1 )) || record_fail "miner never receives templates"
(( miner_submits == 1 )) || record_fail "miner never submits"
if (( accepted_count < 1 )); then
  if (( WAIVE_ACCEPTED_BLOCK_GATE == 1 )); then
    if [[ -z "$WAIVE_ACCEPTED_BLOCK_REASON" ]]; then
      record_fail "accepted block gate waived without reason"
    else
      record_warn "accepted block gate waived: $WAIVE_ACCEPTED_BLOCK_REASON"
    fi
  else
    record_fail "no accepted block recorded"
  fi
fi

echo "node,height,tip" > "$OUT_DIR/final-tips-heights.csv"
echo "a,$ha,$ta" >> "$OUT_DIR/final-tips-heights.csv"
echo "b,$hb,$tb" >> "$OUT_DIR/final-tips-heights.csv"
echo "c,$hc,$tc" >> "$OUT_DIR/final-tips-heights.csv"

jq -n --arg run_id "$RUN_ID" --arg phase "$readiness_phase" --argjson heights "$(printf '{"a":%s,"b":%s,"c":%s}' "$ha" "$hb" "$hc")" '{run_id:$run_id, final_heights:$heights, readiness_phase:$phase}' > "$OUT_DIR/node-height-summary.json"
jq -n --arg run_id "$RUN_ID" --arg templates "$miner_templates" --arg submits "$miner_submits" '{run_id:$run_id, templates_seen:($templates=="1"), submits_seen:($submits=="1")}' > "$OUT_DIR/miner-submit-summary.json"

for n in a b c; do
  jq -n --arg node "$n" --slurpfile d "$OUT_DIR/endpoints/${n}-readiness.json" '{node:$node, captured:(($d|length)>0)}' >> "$OUT_DIR/readiness-summary.json"
  jq -n --arg node "$n" --slurpfile d "$OUT_DIR/endpoints/${n}-p2p_status.json" '{node:$node, captured:(($d|length)>0)}' >> "$OUT_DIR/p2p-summary.json"
done


if (( ${#FAILURES[@]} > 0 )); then
  echo "FAIL local smoke: $OUT_DIR"
  exit 1
fi

echo "PASS local smoke complete: $OUT_DIR"
