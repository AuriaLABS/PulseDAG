#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-900}
GRACE_SECS=${GRACE_SECS:-120}
SAMPLE_INTERVAL_SECS=${SAMPLE_INTERVAL_SECS:-10}
STARTUP_WAIT_SECS=${STARTUP_WAIT_SECS:-12}
P2P_CONNECT_WAIT_SECS=${P2P_CONNECT_WAIT_SECS:-120}
STAMP=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
RUN_ID="$STAMP"
START_TS=$(date +%s)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_OUT_DIR="$ROOT_DIR/artifacts/v2_2_19/local_3n_1m_smoke"
OUT_DIR=${OUT_DIR:-$DEFAULT_OUT_DIR}
RUN_DIR="$OUT_DIR/$STAMP"
OUT_DIR_ROOT="$OUT_DIR"
OUT_DIR="$RUN_DIR"
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
NODE_A_LAUNCHED=0
NODE_B_LAUNCHED=0
NODE_C_LAUNCHED=0
MINER_LAUNCHED=0
WARNINGS=()
FAILURES=()
RESULT="PENDING"
EXIT_CODE=0
WAIVE_ACCEPTED_BLOCK_GATE=${WAIVE_ACCEPTED_BLOCK_GATE:-0}
WAIVE_ACCEPTED_BLOCK_REASON=${WAIVE_ACCEPTED_BLOCK_REASON:-""}
ha=0
hb=0
hc=0
ta=""
tb=""
tc=""
final_converged=0
final_peers_ok=0
healthy_nodes=0
ready_nodes=0
peers_total=0
miner_templates=0
miner_submissions=0
miner_accepted=0
miner_rejected=0
miner_not_started_reason=""
chain_id="unknown"
miner_submits=0
accepted_count=0
rejected_count=0
duplicate_sync_degraded_blocker=0

record_warn(){ local msg; msg="$1"; echo "WARN: $msg"; WARNINGS+=("$msg"); }
record_fail(){ local msg; msg="$1"; echo "FAIL: $msg"; FAILURES+=("$msg"); }

safe_curl_json(){
  local url out label required rc
  url="$1"; out="$2"; label="${3:-$url}"; required="${4:-0}"
  if ! curl -fsS "$url" -o "$out"; then
    rc=$?
    jq -n --arg url "$url" --argjson exit_code "$rc" '{ok:false,error:"curl failed",url:$url,exit_code:$exit_code}' > "$out"
    if (( required == 1 )); then record_fail "required endpoint failed: $url"; else record_warn "optional endpoint failed: $label"; fi
    return 1
  fi
}
safe_curl_required(){ safe_curl_json "$1" "$2" "${3:-$1}" 1; }
safe_curl_optional(){ safe_curl_json "$1" "$2" "${3:-$1}" 0; }
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
  local pattern="$1" file="$2" count="0"
  [[ -f "$file" ]] || { echo 0; return 0; }
  if command -v rg >/dev/null 2>&1; then
    count=$(rg -cE -- "$pattern" "$file" 2>/dev/null | head -n1 | tr -d '[:space:]' || true)
  else
    count=$(grep -cE -- "$pattern" "$file" 2>/dev/null | head -n1 | tr -d '[:space:]' || true)
  fi
  [[ "$count" =~ ^[0-9]+$ ]] || count=0
  echo "$count"
}

write_summary(){
  local healthy_count ready_count peers_total chain_id="unknown"
  healthy_count=0
  ready_count=0
  peers_total=0
  for n in a b c; do
    [[ -f "$OUT_DIR/endpoints/${n}-health.json" ]] && [[ "$(jq -r '(.ok // .data.ok // false)' "$OUT_DIR/endpoints/${n}-health.json" 2>/dev/null)" == "true" ]] && ((healthy_count+=1))
    if [[ -f "$OUT_DIR/endpoints/${n}-readiness.json" ]]; then
      ready_release="$(jq -r '(.data.ready_for_release // .ready_for_release // false)' "$OUT_DIR/endpoints/${n}-readiness.json" 2>/dev/null || echo false)"
      p2p_ready="$(jq -r '(.data.p2p_ready_for_private_rehearsal // false)' "$OUT_DIR/endpoints/${n}-p2p_status.json" 2>/dev/null || echo false)"
      if [[ "$ready_release" == "true" && "$p2p_ready" == "true" ]]; then
        ((ready_count+=1))
      fi
    fi
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
    [[ -n "$miner_not_started_reason" ]] && echo "- miner_not_started_reason: $miner_not_started_reason"
    echo "- final_heights: a=${ha:-0}, b=${hb:-0}, c=${hc:-0}"
    echo "- final_tips: a=${ta:-}, b=${tb:-}, c=${tc:-}"
    echo "- final_peer_counts: a=${pa:-0}, b=${pb:-0}, c=${pc:-0}"
    echo "- duplicate_sync_degraded_blocker: $duplicate_sync_degraded_blocker"
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
    echo "| 3 nodes launched | $( (( NODE_A_LAUNCHED==1 && NODE_B_LAUNCHED==1 && NODE_C_LAUNCHED==1 )) && echo PASS || echo FAIL ) |"
    echo "| 1 miner launched | $( (( MINER_LAUNCHED==1 )) && echo PASS || echo FAIL ) |"
    echo "| all nodes healthy/status | $( (( healthy_count==3 )) && echo PASS || echo FAIL ) |"
    echo "| all nodes readiness | $( (( ready_count==3 )) && echo PASS || echo FAIL ) |"
    echo "| miner templates >=1 | $( (( miner_templates>=1 )) && echo PASS || echo FAIL ) |"
    echo "| miner submissions >=1 | $( (( miner_submits>=1 )) && echo PASS || echo FAIL ) |"
    echo "| accepted blocks >0 (or waived) | $( (( accepted_count>0 || WAIVE_ACCEPTED_BLOCK_GATE==1 )) && echo PASS || echo FAIL ) |"
    echo "| heights > genesis | $( (( ha>0 && hb>0 && hc>0 )) && echo PASS || echo FAIL ) |"
    echo "| p2p peers sustained (a>=2,b>=1,c>=1) | $( (( final_peers_ok==1 )) && echo PASS || echo FAIL ) |"
    echo "| duplicate sync degraded false-blocker | $( (( duplicate_sync_degraded_blocker==0 )) && echo PASS || echo FAIL ) |"
    echo "| final convergence | $( (( final_converged==1 )) && echo PASS || echo FAIL ) |"
  } > "$OUT_DIR/evidence-summary.md"
}

capture_node_endpoints_stable(){
  local node="$1" rpc="$2" ep slug stable
  for ep in health status readiness p2p/status sync/status; do
    slug="${ep//\//_}"
    stable="$OUT_DIR/endpoints/${node}-${slug}.json"
    safe_curl_optional "http://127.0.0.1:${rpc}/${ep}" "$stable" "${node}:/${ep}" || true
    cp "$stable" "$OUT_DIR/endpoints/${node}-${slug}-$(date -u +%s).json" 2>/dev/null || true
  done
}

capture_p2p_failure_evidence(){
  capture_node_endpoints_stable a "$RPC_PORT_A"
  capture_node_endpoints_stable b "$RPC_PORT_B"
  capture_node_endpoints_stable c "$RPC_PORT_C"
  cp "$OUT_DIR/logs/a.log" "$OUT_DIR/logs/a.log" 2>/dev/null || true
  cp "$OUT_DIR/logs/b.log" "$OUT_DIR/logs/b.log" 2>/dev/null || true
  cp "$OUT_DIR/logs/c.log" "$OUT_DIR/logs/c.log" 2>/dev/null || true
  {
    echo "# p2p gate failure diagnostics"
    echo "- bootnode: $(cat "$OUT_DIR/bootnode.txt" 2>/dev/null || echo unknown)"
    echo "- command_a: $(rg -n \"launch node-a:\" "$OUT_DIR/command-log.txt" | tail -n1 | cut -d: -f2-)"
    echo "- command_b: $(rg -n \"launch node-b:\" "$OUT_DIR/command-log.txt" | tail -n1 | cut -d: -f2-)"
    echo "- command_c: $(rg -n \"launch node-c:\" "$OUT_DIR/command-log.txt" | tail -n1 | cut -d: -f2-)"
    for n in a b c; do
      f="$OUT_DIR/endpoints/${n}-p2p_status.json"
      echo "- ${n}_peer_count: $(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$f" 2>/dev/null || echo 0)"
      echo "- ${n}_connected_peers: $(jq -c '.data.connected_peers // .data.connected_peer_ids // []' "$f" 2>/dev/null || echo '[]')"
      echo "- ${n}_last_swarm_event: $(jq -r '.data.last_swarm_event // "n/a"' "$f" 2>/dev/null || echo n/a)"
      echo "- ${n}_last_connection_error: $(jq -r '.data.last_connection_error // "n/a"' "$f" 2>/dev/null || echo n/a)"
      echo "- ${n}_last_disconnect_reason: $(jq -r '.data.last_disconnect_reason // "n/a"' "$f" 2>/dev/null || echo n/a)"
    done
    echo "## node log tails"
    for n in a b c; do
      echo "### node-${n}"
      tail -n 80 "$OUT_DIR/logs/${n}.log" 2>/dev/null || true
    done
  } > "$OUT_DIR/p2p-gate-failure.md"
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
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  cp "$OUT_DIR/command-log.txt" "$OUT_DIR_ROOT/command-log.txt" 2>/dev/null || true
  cp "$OUT_DIR/bootnode.txt" "$OUT_DIR_ROOT/bootnode.txt" 2>/dev/null || true
  printf "%s
" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt"
  cp "$OUT_DIR/samples/height-samples.csv" "$OUT_DIR/final-convergence-table.txt" 2>/dev/null || true
  local tar_tmp
  tar_tmp=$(mktemp -p /tmp evidence.XXXXXX.tar.gz)
  (cd "$OUT_DIR" && tar -czf "$tar_tmp" --exclude='evidence.tar.gz' --exclude='evidence.tar.gz.sha256' endpoints logs miners nodes samples summaries evidence-summary.md command-log.txt process-pids.txt final-convergence-table.txt 2>/dev/null || true)
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
  cp "$OUT_DIR/evidence-summary.md" "$OUT_DIR_ROOT/evidence-summary.md" 2>/dev/null || true
  printf "%s
" "$OUT_DIR" > "$OUT_DIR_ROOT/current-run-dir.txt"
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
  echo "launch node-$name: PULSEDAG_ROCKSDB_PATH=$data/rocksdb ${cmd[*]}"
  PULSEDAG_ROCKSDB_PATH="$data/rocksdb" "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  local pid="$!"
  PIDS+=("$pid")
  echo "$pid" > "$OUT_DIR/${name}.pid"
  echo "$pid node-$name" >> "$OUT_DIR/process-pids.txt"
  case "$name" in
    a) NODE_A_LAUNCHED=1 ;;
    b) NODE_B_LAUNCHED=1 ;;
    c) NODE_C_LAUNCHED=1 ;;
  esac
}

if ! "$NODE_BIN" --help | grep -q -- '--bootnode'; then
  echo "FATAL: pulsedagd missing --bootnode support"
  "$NODE_BIN" --help || true
  exit 1
fi

start_node a "$RPC_PORT_A" "$P2P_PORT_A" ""
for _ in $(seq 1 "$STARTUP_WAIT_SECS"); do
  if curl -fsS "http://127.0.0.1:${RPC_PORT_A}/p2p/status" -o "$OUT_DIR/endpoints/a-p2p_status.bootstrap.json"; then break; fi
  sleep 1
done
safe_curl_required "http://127.0.0.1:${RPC_PORT_A}/p2p/status" "$OUT_DIR/endpoints/a-p2p_status.bootstrap.json"
NODE_A_ID=$(jq -r '.data.peer_id // .data.local_node_id // empty' "$OUT_DIR/endpoints/a-p2p_status.bootstrap.json")
NODE_A_LISTENING=$(jq -r '.data.listening[0] // .data.listening_addresses[0] // empty' "$OUT_DIR/endpoints/a-p2p_status.bootstrap.json")
[[ -n "$NODE_A_ID" ]] || { echo "FATAL: unable to resolve node A peer id"; exit 1; }
BOOT_A="/ip4/127.0.0.1/tcp/${P2P_PORT_A}/p2p/${NODE_A_ID}"
echo "$BOOT_A" > "$OUT_DIR/bootnode.txt"
echo "$NODE_A_ID" > "$OUT_DIR/node-a-peer-id.txt"
echo "$NODE_A_LISTENING" > "$OUT_DIR/node-a-listening.txt"
start_node b "$RPC_PORT_B" "$P2P_PORT_B" "$BOOT_A"
start_node c "$RPC_PORT_C" "$P2P_PORT_C" "$BOOT_A"
sleep "$STARTUP_WAIT_SECS"

peer_wait_deadline=$(( $(date +%s) + P2P_CONNECT_WAIT_SECS ))
peers_total=0
while (( $(date +%s) < peer_wait_deadline )); do
  safe_curl_optional "http://127.0.0.1:${RPC_PORT_A}/p2p/status" "$OUT_DIR/endpoints/a-p2p_status.pre_mining.json" || true
  safe_curl_optional "http://127.0.0.1:${RPC_PORT_B}/p2p/status" "$OUT_DIR/endpoints/b-p2p_status.pre_mining.json" || true
  safe_curl_optional "http://127.0.0.1:${RPC_PORT_C}/p2p/status" "$OUT_DIR/endpoints/c-p2p_status.pre_mining.json" || true
  pa=$(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/a-p2p_status.pre_mining.json" 2>/dev/null || echo 0)
  pb=$(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/b-p2p_status.pre_mining.json" 2>/dev/null || echo 0)
  pc=$(jq -r '.data.peer_count // (.data.connected_peers|length) // .data.connected_peer_count // 0' "$OUT_DIR/endpoints/c-p2p_status.pre_mining.json" 2>/dev/null || echo 0)
  peers_total=$((pa + pb + pc))
  if (( pa >= 2 && pb >= 1 && pc >= 1 )); then
    break
  fi
  sleep 2
done
if ! (( pa >= 2 && pb >= 1 && pc >= 1 )); then
  miner_not_started_reason="pre-mining p2p peer gate failed"
  capture_p2p_failure_evidence
  record_fail "pre-mining p2p peer gate failed after ${P2P_CONNECT_WAIT_SECS}s (a=${pa}, b=${pb}, c=${pc})"
  exit 1
fi

echo "launch miner: $MINER_BIN --node http://127.0.0.1:${RPC_PORT_A} --miner-address $MINER_ADDRESS --backend cpu --threads 1 --loop"
"$MINER_BIN" --node "http://127.0.0.1:${RPC_PORT_A}" --miner-address "$MINER_ADDRESS" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner.log" 2>&1 &
PIDS+=("$!")
echo "$!" > "$OUT_DIR/miner.pid"
echo "$! miner" >> "$OUT_DIR/process-pids.txt"
MINER_LAUNCHED=1

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

tip_divergence_seen=0
final_converged=0
readiness_phase="no_peers"

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
  if (( pa >= 2 && pb >= 1 && pc >= 1 )); then final_peers_ok=1; fi
  inbound_blocks=$(( $(count_matches "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/b.log") + $(count_matches "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/c.log") ))
  echo "$(date -u +%FT%TZ),$readiness_phase,$((pa+pb+pc)),$inbound_blocks" >> "$OUT_DIR/samples/readiness-samples.csv"

  if (( pa + pb + pc == 0 )); then readiness_phase="no_peers";
  elif (( inbound_blocks == 0 )); then readiness_phase="peers_connected_no_propagation";
  elif (( ha>0 && hb>0 && hc>0 )) && [[ "$ta" == "$tb" && "$tb" == "$tc" ]]; then readiness_phase="converged";
  else readiness_phase="propagation_active"; fi

  accepted_count=$(count_matches "[Aa]ccepted" "$OUT_DIR/logs/miner.log")
  rejected_count=$(count_matches "[Rr]eject|[Rr]ejected" "$OUT_DIR/logs/miner.log")
  echo "$(date -u +%FT%TZ),$accepted_count,$rejected_count" >> "$OUT_DIR/samples/miner-block-counters.csv"

  if text_has_match "template_received|template" "$OUT_DIR/logs/miner.log"; then miner_templates=1; fi
  if text_has_match "submit_result accepted=true|submit_accepted|submit" "$OUT_DIR/logs/miner.log"; then miner_submits=1; fi

  if (( elapsed >= GRACE_SECS )) && (( ha>0 && hb>0 && hc>0 )) && [[ "$ta" == "$tb" && "$tb" == "$tc" ]]; then
    final_converged=1
  fi

  sleep "$SAMPLE_INTERVAL_SECS"
done

for n in a b c; do
  cp "$OUT_DIR/endpoints/${n}-status.json" "$OUT_DIR/final-status-node-${n}.json" 2>/dev/null || true
done

check_duplicate_degraded_false_blocker(){
  local node="$1" f stage reason lag consistent dup
  f="$OUT_DIR/endpoints/${node}-sync_status.json"
  [[ -f "$f" ]] || return 0
  stage="$(jq -r '.data.catchup_stage // ""' "$f" 2>/dev/null || echo "")"
  reason="$(jq -r '.data.recovery_reason // ""' "$f" 2>/dev/null || echo "")"
  lag="$(jq -r '.data.lag_blocks // -1' "$f" 2>/dev/null || echo -1)"
  consistent="$(jq -r '.data.consistency_ok // false' "$f" 2>/dev/null || echo false)"
  dup="$(jq -r '.data.duplicate_blocks_received // 0' "$f" 2>/dev/null || echo 0)"
  if [[ "$stage" == "degraded" && "$reason" =~ [Dd]uplicate && "$lag" == "0" && "$consistent" == "true" && "$dup" =~ ^[1-9][0-9]*$ ]]; then
    duplicate_sync_degraded_blocker=1
    record_fail "node_${node} sync degraded only due to duplicate while aligned (lag=0, consistency_ok=true, duplicate_blocks_received=${dup})"
  fi
}

check_duplicate_degraded_false_blocker a
check_duplicate_degraded_false_blocker b
check_duplicate_degraded_false_blocker c

(( final_converged == 1 )) || record_fail "final convergence not reached within deadline (duration=${DURATION_SECS}s, grace=${GRACE_SECS}s)"
(( final_peers_ok == 1 )) || record_fail "final p2p topology gate not satisfied (need a>=2,b>=1,c>=1; got a=${pa},b=${pb},c=${pc})"
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
