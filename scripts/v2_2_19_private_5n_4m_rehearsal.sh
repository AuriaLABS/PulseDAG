#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-1800}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-$ROOT_DIR/artifacts/private-testnet/v2_2_19/rc-5n-4m/${RUN_ID}}"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

PIDS=()
cleanup(){ for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done; wait || true; }
trap cleanup EXIT INT TERM

OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"
cargo build --workspace --release

start_node(){
  local idx="$1" rpc="$2" p2p="$3" bootnode="$4"
  local name="n${idx}" data="$OUT_DIR/data-${name}"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --rpc-bind "127.0.0.1:${rpc}" --p2p-bind "/ip4/127.0.0.1/tcp/${p2p}" --data-dir "$data" --network-profile "rehearsal-${name}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  PIDS+=("$!")
}

start_node 1 18180 19180 ""; sleep 2
NODE_1_ID=$(rg -n "local_node_id|peer_id" "$OUT_DIR/logs/n1.log" | head -n1 | sed -E 's/.*(12D[[:alnum:]]+).*/\1/' || true)
BOOT_1=""; [[ -n "$NODE_1_ID" ]] && BOOT_1="/ip4/127.0.0.1/tcp/19180/p2p/${NODE_1_ID}"
start_node 2 18181 19181 "$BOOT_1"
start_node 3 18182 19182 "$BOOT_1"
start_node 4 18183 19183 "$BOOT_1"
start_node 5 18184 19184 "$BOOT_1"
sleep 5

for i in 1 2 3 4; do
  "$MINER_BIN" --node "http://127.0.0.1:$((18179+i))" --miner-address "v2219-${RUN_ID}-miner-${i}" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner-${i}.log" 2>&1 &
  PIDS+=("$!")
done

printf "timestamp,n1,n2,n3,n4,n5,tip_match\n" > "$OUT_DIR/height-samples.csv"

declare -A miner_submit miner_accept
for i in 1 2 3 4; do miner_submit[$i]=0; miner_accept[$i]=0; done

end=$(( $(date +%s) + DURATION_SECS ))
grace_end=$(( $(date +%s) + 120 ))
while (( $(date +%s) < end )); do
  heights=(); tips=()
  for i in 1 2 3 4 5; do
    rpc=$((18179+i))
    curl -fsS "http://127.0.0.1:${rpc}/status" -o "$OUT_DIR/endpoints/n${i}-status.json"
    curl -fsS "http://127.0.0.1:${rpc}/p2p/status" -o "$OUT_DIR/endpoints/n${i}-p2p-status.json" || true
    heights+=("$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo 0)")
    tips+=("$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo "")")
  done

  tip_match=1; ref_tip="${tips[0]}"
  for t in "${tips[@]}"; do [[ "$t" == "$ref_tip" ]] || tip_match=0; done
  echo "$(date -u +%FT%TZ),${heights[0]},${heights[1]},${heights[2]},${heights[3]},${heights[4]},$tip_match" >> "$OUT_DIR/height-samples.csv"

  advanced=0; zero_with_advanced=0
  for h in "${heights[@]}"; do (( h > 0 )) && advanced=1; done
  if (( advanced == 1 )); then for h in "${heights[@]}"; do (( h == 0 )) && zero_with_advanced=1; done; fi
  (( zero_with_advanced == 0 )) || { echo "FAIL one node stayed at genesis while others advanced"; exit 1; }

  if (( $(date +%s) > grace_end )) && (( tip_match == 0 )); then
    echo "FAIL selected tips diverged after grace window"
    exit 1
  fi

  for i in 2 3; do
    rg -q "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/n${i}.log" || true
  done

  for i in 1 2 3 4; do
    rg -qi "submit|accepted|reject" "$OUT_DIR/logs/miner-${i}.log" && miner_submit[$i]=1 || true
    rg -qi "accepted" "$OUT_DIR/logs/miner-${i}.log" && miner_accept[$i]=1 || true
  done

  sleep 10
done

# hard fail if inbound activity missing on node_b and node_c equivalent
rg -q "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/n2.log" || { echo "FAIL node_b missing inbound p2p block activity"; exit 1; }
rg -q "peer_block_received|peer_block_accepted" "$OUT_DIR/logs/n3.log" || { echo "FAIL node_c missing inbound p2p block activity"; exit 1; }

for i in 1 2 3 4 5; do
  h=$(tail -n1 "$OUT_DIR/height-samples.csv" | awk -F, -v col=$((i+1)) '{print $col}')
  (( h > 0 )) || { echo "FAIL n${i} never advanced"; exit 1; }
done

for i in 1 2 3 4; do
  (( miner_submit[$i] == 1 )) || { echo "FAIL miner-${i} submit count is zero"; exit 1; }
done

jq -n \
  --arg run_id "$RUN_ID" \
  --arg duration_secs "$DURATION_SECS" \
  --argjson miner_submit "$(for i in 1 2 3 4; do printf '"miner-%s":%s,' "$i" "${miner_submit[$i]}"; done | sed 's/,$//;s/^/{/;s/$/}/')" \
  --argjson miner_accept "$(for i in 1 2 3 4; do printf '"miner-%s":%s,' "$i" "${miner_accept[$i]}"; done | sed 's/,$//;s/^/{/;s/$/}/')" \
  '{run_id:$run_id,duration_secs:($duration_secs|tonumber),miners_submit: $miner_submit, miners_accepted: $miner_accept, note:"miners without accepted block can occur if network difficulty prevented finding a valid PoW in time"}' > "$OUT_DIR/rehearsal-summary.json"

echo "PASS private rehearsal complete: $OUT_DIR"
