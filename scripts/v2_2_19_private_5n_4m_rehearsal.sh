#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-1800}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
START_TS=$(date +%s)
START_UTC=$(date -u +%FT%TZ)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/artifacts/private-testnet/v2_2_19/rc-5n-4m/${RUN_ID}}"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
NODE_COUNT=5
MINER_COUNT=4
NETWORK_PROFILE="private"
CHAIN_ID_EXPECTED="private-testnet-v2-2-19"
BASE_RPC_PORT=28544
BASE_P2P_PORT=32302

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

declare -a NODE_PIDS=()
declare -a MINER_PIDS=()
declare -A NODE_READY NODE_HEALTHY NODE_ADVANCED NODE_TIP NODE_HEIGHT NODE_P2P_OK NODE_PEERS NODE_P2P_INBOUND NODE_P2P_OUTBOUND
FAIL_REASONS=()
ACCEPTED_BLOCKS=0
REJECTED_BLOCKS=0
TEMPLATES_OK=0

record_fail(){ echo "FAIL: $1"; FAIL_REASONS+=("$1"); }

port_in_use(){
  local p="$1"
  if command -v ss >/dev/null 2>&1; then ss -ltn "( sport = :$p )" | rg -q ":$p\b"; return $?; fi
  if command -v lsof >/dev/null 2>&1; then lsof -nP -iTCP:"$p" -sTCP:LISTEN >/dev/null 2>&1; return $?; fi
  if command -v netstat >/dev/null 2>&1; then netstat -ltn 2>/dev/null | rg -q "[:.]$p[[:space:]]"; return $?; fi
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

stop_pids(){ for p in "$@"; do kill "$p" 2>/dev/null || true; done; sleep 1; for p in "$@"; do kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null || true; done; }

capture_log_tails(){
  for i in $(seq 1 "$NODE_COUNT"); do tail -n 120 "$OUT_DIR/logs/n${i}.log" > "$OUT_DIR/logs/n${i}-tail.log" 2>/dev/null || true; done
  for i in $(seq 1 "$MINER_COUNT"); do tail -n 120 "$OUT_DIR/logs/miner-${i}.log" > "$OUT_DIR/logs/miner-${i}-tail.log" 2>/dev/null || true; done
}

collect_final_state(){
  for i in $(seq 1 "$NODE_COUNT"); do
    local rpc=$((BASE_RPC_PORT+i))
    curl -fsS "http://127.0.0.1:${rpc}/status" -o "$OUT_DIR/endpoints/n${i}-status-final.json" || true
    curl -fsS "http://127.0.0.1:${rpc}/readiness" -o "$OUT_DIR/endpoints/n${i}-readiness-final.json" || true
    curl -fsS "http://127.0.0.1:${rpc}/p2p/status" -o "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" || true
    NODE_HEIGHT[$i]="$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo 0)"
    NODE_TIP[$i]="$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null || echo '')"
    NODE_READY[$i]="$(jq -r '.data.ready_for_release // .ready_for_release // 0' "$OUT_DIR/endpoints/n${i}-readiness-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_HEALTHY[$i]="$(jq -r '.ok // .data.ok // 0' "$OUT_DIR/endpoints/n${i}-status-final.json" 2>/dev/null | sed 's/true/1/;s/false/0/' || echo 0)"
    NODE_PEERS[$i]="$(jq -r '.data.peer_count // (.data.peers|length) // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_INBOUND[$i]="$(jq -r '.data.inbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_OUTBOUND[$i]="$(jq -r '.data.outbound_peer_count // 0' "$OUT_DIR/endpoints/n${i}-p2p-status-final.json" 2>/dev/null || echo 0)"
    NODE_P2P_OK[$i]=$(( NODE_PEERS[$i] > 0 ? 1 : 0 ))
  done
}

write_evidence_summary(){
  local end_ts now_utc duration result
  end_ts=$(date +%s); now_utc=$(date -u +%FT%TZ); duration=$((end_ts - START_TS)); result="PASS"
  (( ${#FAIL_REASONS[@]} > 0 )) && result="FAIL"
  {
    echo "# v2.2.19 Private 5N/4M Rehearsal Evidence"
    echo "- chain id expected: \`$CHAIN_ID_EXPECTED\`"
    echo "- network profile: \`$NETWORK_PROFILE\`"
    echo "- start utc: $START_UTC"
    echo "- end utc: $now_utc"
    echo "- runtime duration (s): $duration"
    echo
    echo "## Status/readiness per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: healthy=${NODE_HEALTHY[$i]:-0} ready=${NODE_READY[$i]:-0}"; done
    echo
    echo "## P2P status per node"
    for i in $(seq 1 "$NODE_COUNT"); do echo "- n${i}: peers=${NODE_PEERS[$i]:-0} inbound=${NODE_P2P_INBOUND[$i]:-0} outbound=${NODE_P2P_OUTBOUND[$i]:-0} ok=${NODE_P2P_OK[$i]:-0}"; done
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
    echo "## Result"
    echo "- pass/fail: $result"
    if (( ${#FAIL_REASONS[@]} > 0 )); then echo "- reasons:"; for r in "${FAIL_REASONS[@]}"; do echo "  - $r"; done; fi
  } > "$OUT_DIR/evidence-summary.md"
}

package_evidence(){
  (cd "$OUT_DIR/.." && tar -czf "$(basename "$OUT_DIR")/evidence.tar.gz" "$(basename "$OUT_DIR")")
  (cd "$OUT_DIR" && sha256sum evidence.tar.gz > evidence.tar.gz.sha256)
}

cleanup(){
  collect_final_state || true
  capture_log_tails || true
  write_evidence_summary || true
  stop_pids "${MINER_PIDS[@]:-}"; stop_pids "${NODE_PIDS[@]:-}"; wait || true
  package_evidence || true
}
trap cleanup EXIT INT TERM

OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"
ensure_ports_free
cargo build --workspace --release --locked

start_node(){
  local idx="$1" rpc="$2" p2p="$3" bootnode="$4" name="n${idx}" data="$OUT_DIR/data-${name}"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --network "$NETWORK_PROFILE" --rpc-listen "127.0.0.1:${rpc}" --p2p-listen "/ip4/127.0.0.1/tcp/${p2p}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  PULSEDAG_ROCKSDB_PATH="$data/rocksdb" "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  NODE_PIDS+=("$!")
}

wait_node_ready(){
  local idx="$1" rpc=$((BASE_RPC_PORT+idx))
  for _ in $(seq 1 60); do
    curl -fsS "http://127.0.0.1:${rpc}/status" -o "$OUT_DIR/endpoints/n${idx}-status-ready.json" && return 0
    sleep 2
  done
  record_fail "node n${idx} failed readiness"
  return 1
}

start_node 1 $((BASE_RPC_PORT+1)) $((BASE_P2P_PORT+1)) ""; sleep 3
NODE_1_ID=$(rg -o "12D[[:alnum:]]+" "$OUT_DIR/logs/n1.log" | head -n1 || true)
BOOT_1=""; [[ -n "$NODE_1_ID" ]] && BOOT_1="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT+1))/p2p/${NODE_1_ID}"
for i in 2 3 4 5; do start_node "$i" $((BASE_RPC_PORT+i)) $((BASE_P2P_PORT+i)) "$BOOT_1"; done
sleep 3

for i in 1 2 3 4 5; do wait_node_ready "$i" || true; done

for i in 1 2 3 4; do
  local_node="http://127.0.0.1:$((BASE_RPC_PORT+i))"
  "$MINER_BIN" --node "$local_node" --miner-address "v2219-${RUN_ID}-miner-${i}" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner-${i}.log" 2>&1 &
  MINER_PIDS+=("$!")
done

printf "timestamp,n1,n2,n3,n4,n5,tip_match\n" > "$OUT_DIR/height-samples.csv"
declare -A miner_submit miner_accept miner_template
for i in 1 2 3 4; do miner_submit[$i]=0; miner_accept[$i]=0; miner_template[$i]=0; done

end=$(( $(date +%s) + DURATION_SECS ))
while (( $(date +%s) < end )); do
  heights=(); tips=()
  for i in 1 2 3 4 5; do
    rpc=$((BASE_RPC_PORT+i))
    curl -fsS "http://127.0.0.1:${rpc}/status" -o "$OUT_DIR/endpoints/n${i}-status.json" || true
    curl -fsS "http://127.0.0.1:${rpc}/p2p/status" -o "$OUT_DIR/endpoints/n${i}-p2p-status.json" || true
    heights+=("$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo 0)")
    tips+=("$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo '')")
  done
  tip_match=1; ref_tip="${tips[0]}"; for t in "${tips[@]}"; do [[ "$t" == "$ref_tip" ]] || tip_match=0; done
  echo "$(date -u +%FT%TZ),${heights[0]},${heights[1]},${heights[2]},${heights[3]},${heights[4]},$tip_match" >> "$OUT_DIR/height-samples.csv"

  for i in 1 2 3 4; do
    rg -qi "template" "$OUT_DIR/logs/miner-${i}.log" && miner_template[$i]=1 || true
    rg -qi "submit" "$OUT_DIR/logs/miner-${i}.log" && miner_submit[$i]=1 || true
    rg -qi "accepted" "$OUT_DIR/logs/miner-${i}.log" && miner_accept[$i]=1 || true
  done
  ACCEPTED_BLOCKS=$(rg -ci "accepted" "$OUT_DIR"/logs/miner-*.log 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')
  REJECTED_BLOCKS=$(rg -ci "reject" "$OUT_DIR"/logs/miner-*.log 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')
  (( ACCEPTED_BLOCKS > 0 )) && TEMPLATES_OK=1
  sleep 10
done

collect_final_state

for i in 1 2 3 4 5; do
  [[ "${NODE_HEALTHY[$i]:-0}" == "1" ]] || record_fail "node n${i} unhealthy"
  [[ "${NODE_READY[$i]:-0}" == "1" ]] || record_fail "node n${i} not ready enough"
  (( ${NODE_HEIGHT[$i]:-0} > 0 )) || record_fail "node n${i} did not advance"
  [[ "${NODE_P2P_OK[$i]:-0}" == "1" ]] || record_fail "node n${i} missing peers"
done

final_tip="${NODE_TIP[1]:-}"
for i in 1 2 3 4 5; do [[ "${NODE_TIP[$i]:-}" == "$final_tip" ]] || record_fail "node tips did not converge"; done

for i in 1 2 3 4; do
  (( miner_template[$i] == 1 )) || record_fail "miner-${i} did not receive templates"
  (( miner_submit[$i] == 1 )) || record_fail "miner-${i} did not submit work"
done

if (( ${#FAIL_REASONS[@]} > 0 )); then
  echo "FAIL private rehearsal: $OUT_DIR"
  exit 1
fi

echo "PASS private rehearsal complete: $OUT_DIR"
