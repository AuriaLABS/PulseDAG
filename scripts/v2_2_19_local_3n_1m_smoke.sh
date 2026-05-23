#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-900}
GRACE_SECS=${GRACE_SECS:-120}
SAMPLE_INTERVAL_SECS=${SAMPLE_INTERVAL_SECS:-10}
STARTUP_WAIT_SECS=${STARTUP_WAIT_SECS:-12}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFAULT_OUT_DIR="$ROOT_DIR/artifacts/v2_2_19_public_testnet_readiness/local-3n-1m/${RUN_ID}"
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

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

PIDS=()
cleanup(){
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
}
trap cleanup EXIT INT TERM

is_port_busy(){
  local port="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltn | awk '{print $4}' | rg -q "[:.]${port}$"
  elif command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"${port}" -sTCP:LISTEN >/dev/null 2>&1
  elif command -v netstat >/dev/null 2>&1; then
    netstat -ltn 2>/dev/null | awk '{print $4}' | rg -q "[:.]${port}$"
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
NODE_A_ID=$(rg -n "local_node_id|peer_id" "$OUT_DIR/logs/a.log" | head -n1 | sed -E 's/.*(12D[[:alnum:]]+).*/\1/' || true)
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
printf "node,endpoint,path,status\n" > "$OUT_DIR/endpoints-manifest.txt"
printf "timestamp,height_a,height_b,height_c,tip_a,tip_b,tip_c\n" > "$OUT_DIR/height-samples.csv"
printf "timestamp,phase,peers_total,inbound_blocks\n" > "$OUT_DIR/readiness-samples.csv"
printf "timestamp,accepted,rejected\n" > "$OUT_DIR/miner-block-counters.csv"

sample(){
  local node="$1" rpc="$2" ep="$3" path="/$3" out="$OUT_DIR/endpoints/${node}-${ep//\//_}.json"
  if curl -fsS "http://127.0.0.1:${rpc}${path}" -o "$out"; then
    cp "$out" "$OUT_DIR/endpoints/${node}-${ep//\//_}-$(date -u +%s).json"
    echo "$node,$ep,$path,OK" >> "$OUT_DIR/endpoints-manifest.txt"
  else
    if [[ "$ep" == "p2p/status" || "$ep" == "sync/status" ]]; then
      echo "SKIP" > "$OUT_DIR/endpoints/${node}-${ep//\//_}.skip"
      echo "$node,$ep,$path,SKIP_OPTIONAL" >> "$OUT_DIR/endpoints-manifest.txt"
    else
      echo "$node,$ep,$path,FAIL" >> "$OUT_DIR/endpoints-manifest.txt"
      return 1
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

  ha=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/a-status.json" 2>/dev/null || echo 0)
  hb=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/b-status.json" 2>/dev/null || echo 0)
  hc=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/c-status.json" 2>/dev/null || echo 0)
  ta=$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/a-status.json" 2>/dev/null || echo "")
  tb=$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/b-status.json" 2>/dev/null || echo "")
  tc=$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/c-status.json" 2>/dev/null || echo "")
  echo "$(date -u +%FT%TZ),$ha,$hb,$hc,$ta,$tb,$tc" >> "$OUT_DIR/height-samples.csv"

  if (( elapsed < GRACE_SECS )) && [[ "$ta" != "$tb" || "$tb" != "$tc" ]]; then
    tip_divergence_seen=1
    echo "WARN temporary tip divergence observed during startup grace elapsed=${elapsed}s"
  fi

  pa=$(jq -r ".data.peer_count // .data.connected_peer_count // 0" "$OUT_DIR/endpoints/a-p2p_status.json" 2>/dev/null || echo 0)
  pb=$(jq -r ".data.peer_count // .data.connected_peer_count // 0" "$OUT_DIR/endpoints/b-p2p_status.json" 2>/dev/null || echo 0)
  pc=$(jq -r ".data.peer_count // .data.connected_peer_count // 0" "$OUT_DIR/endpoints/c-p2p_status.json" 2>/dev/null || echo 0)
  inbound_blocks=$(( $(rg -c "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/b.log" || echo 0) + $(rg -c "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/c.log" || echo 0) ))
  echo "$(date -u +%FT%TZ),$readiness_phase,$((pa+pb+pc)),$inbound_blocks" >> "$OUT_DIR/readiness-samples.csv"

  if (( pa + pb + pc == 0 )); then readiness_phase="no_peers";
  elif (( inbound_blocks == 0 )); then readiness_phase="peers_connected_no_propagation";
  elif (( ha>0 && hb>0 && hc>0 )) && [[ "$ta" == "$tb" && "$tb" == "$tc" ]]; then readiness_phase="converged";
  else readiness_phase="propagation_active"; fi

  accepted_count=$(rg -i -c "accepted" "$OUT_DIR/logs/miner.log" || echo 0)
  rejected_count=$(rg -i -c "reject|rejected" "$OUT_DIR/logs/miner.log" || echo 0)
  echo "$(date -u +%FT%TZ),$accepted_count,$rejected_count" >> "$OUT_DIR/miner-block-counters.csv"

  if rg -qi "template" "$OUT_DIR/logs/miner.log"; then miner_templates=1; fi
  if rg -qi "submit|accepted|reject" "$OUT_DIR/logs/miner.log"; then miner_submits=1; fi

  if (( elapsed >= GRACE_SECS )) && (( ha>0 && hb>0 && hc>0 )) && [[ "$ta" == "$tb" && "$tb" == "$tc" ]]; then
    final_converged=1
  fi

  sleep "$SAMPLE_INTERVAL_SECS"
done

for n in a b c; do
  cp "$OUT_DIR/endpoints/${n}-status.json" "$OUT_DIR/final-status-node-${n}.json" 2>/dev/null || true
done

if (( final_converged == 0 )); then
  echo "FAIL final convergence not reached within deadline (duration=${DURATION_SECS}s, grace=${GRACE_SECS}s)"
  exit 1
fi

rg -q "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/b.log" || { echo "FAIL node_b missing inbound p2p block activity"; exit 1; }
rg -q "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/c.log" || { echo "FAIL node_c missing inbound p2p block activity"; exit 1; }
(( miner_templates == 1 )) || { echo "FAIL miner never receives templates"; exit 1; }
(( miner_submits == 1 )) || { echo "FAIL miner never submits"; exit 1; }
if (( accepted_count < 1 )); then
  if rg -qi "difficulty|target too high|share.*reject" "$OUT_DIR/logs/miner.log"; then
    echo "NO-GO: no accepted block, difficulty may prevent acceptance" | tee "$OUT_DIR/no-go.txt"
    exit 1
  fi
  echo "FAIL no accepted block recorded"
  exit 1
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

{
  echo "# v2.2.19 local 3N/1M smoke evidence"
  echo "- run_id: $RUN_ID"
  echo "- duration_secs: $DURATION_SECS"
  echo "- grace_secs: $GRACE_SECS"
  echo "- sample_interval_secs: $SAMPLE_INTERVAL_SECS"
  echo "- miner_address: $MINER_ADDRESS"
  echo "- output_dir: $OUT_DIR"
  echo "- tip_divergence_seen_during_run: $tip_divergence_seen"
  echo "- final_converged: $final_converged"
  echo ""
  echo "## Final tips/heights"
  cat "$OUT_DIR/final-tips-heights.csv"
  echo ""
  echo "## Process exit summary"
  for p in "${PIDS[@]:-}"; do
    if kill -0 "$p" 2>/dev/null; then
      echo "- pid $p: running_at_summary_time"
    else
      echo "- pid $p: not_running_at_summary_time"
    fi
  done
} > "$OUT_DIR/evidence-summary.md"

cat > "$OUT_DIR/manifest.txt" <<MAN
command-log.txt
process-pids.txt
final-status-node-a.json
final-status-node-b.json
final-status-node-c.json
endpoints-manifest.txt
height-samples.csv
readiness-samples.csv
miner-block-counters.csv
final-tips-heights.csv
node-height-summary.json
miner-submit-summary.json
readiness-summary.json
p2p-summary.json
evidence-summary.md
MAN

[[ -d "$OUT_DIR" ]] || { echo "FAIL no evidence directory produced"; exit 1; }
[[ -f "$OUT_DIR/evidence-summary.md" ]] || { echo "FAIL missing evidence-summary.md"; exit 1; }

"$ROOT_DIR/scripts/v2_2_19_collect_local_evidence.sh" "$OUT_DIR"
[[ -f "$OUT_DIR/evidence.tar.gz" ]] || { echo "FAIL evidence tarball missing"; exit 1; }
[[ -f "$OUT_DIR/evidence.tar.gz.sha256" ]] || { echo "FAIL evidence checksum missing"; exit 1; }

echo "PASS local smoke complete: $OUT_DIR"
