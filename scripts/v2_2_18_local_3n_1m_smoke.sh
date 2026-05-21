#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-600}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/artifacts/v2_2_18_private_rc/local-3n-1m/${RUN_ID}"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
MINER_ADDRESS="${MINER_ADDRESS:-rc-${RUN_ID}-miner-a}"

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

scripts/v2_2_18_preflight_check.sh
[[ -x "$NODE_BIN" ]] || cargo build --workspace --release
[[ -x "$MINER_BIN" ]] || cargo build -p pulsedag-miner --release
[[ -x "$NODE_BIN" ]] || { echo "FAIL missing $NODE_BIN"; exit 1; }
[[ -x "$MINER_BIN" ]] || { echo "FAIL missing $MINER_BIN"; exit 1; }

PIDS=()
cleanup(){
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done
  wait || true
}
trap cleanup EXIT INT TERM

start_node(){
  local name="$1" rpc="$2" p2p="$3" boot="${4:-}"
  local data="$OUT_DIR/data-$name"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --rpc-bind "127.0.0.1:${rpc}" --p2p-bind "/ip4/127.0.0.1/tcp/${p2p}" --data-dir "$data" --network-profile "rehearsal-${name}")
  [[ -n "$boot" ]] && cmd+=(--bootnode "$boot")
  "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  PIDS+=("$!")
  echo "$! $name" >> "$OUT_DIR/process-pids.txt"
}

start_node a 18080 19080
sleep 2
NODE_A_ID=$(rg -n "local_node_id|peer_id" "$OUT_DIR/logs/a.log" | head -n1 | sed -E 's/.*(12D[[:alnum:]]+).*/\1/' || true)
BOOT_A=""
[[ -n "$NODE_A_ID" ]] && BOOT_A="/ip4/127.0.0.1/tcp/19080/p2p/${NODE_A_ID}"
start_node b 18081 19081 "$BOOT_A"
start_node c 18082 19082 "$BOOT_A"
sleep 4

"$MINER_BIN" --node http://127.0.0.1:18080 --miner-address "$MINER_ADDRESS" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner.log" 2>&1 &
PIDS+=("$!")
echo "$! miner" >> "$OUT_DIR/process-pids.txt"

sample(){ curl -fsS "http://127.0.0.1:$1$2" -o "$OUT_DIR/endpoints/$3-$4.json" 2>/dev/null || echo "SKIP" > "$OUT_DIR/endpoints/$3-$4.skip"; }

end=$(( $(date +%s) + DURATION_SECS ))
converged=0
while (( $(date +%s) < end )); do
  for n in a b c; do
    rpc=$([[ "$n" == a ]] && echo 18080 || ([[ "$n" == b ]] && echo 18081 || echo 18082))
    for ep in health status release readiness p2p/status sync/status; do sample "$rpc" "/$ep" "$n" "${ep//\//_}"; done
  done
  ha=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/a-status.json" 2>/dev/null || echo 0)
  hb=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/b-status.json" 2>/dev/null || echo 0)
  hc=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/c-status.json" 2>/dev/null || echo 0)
  ta=$(jq -r '.data.selected_tip // empty' "$OUT_DIR/endpoints/a-status.json" 2>/dev/null || true)
  tb=$(jq -r '.data.selected_tip // empty' "$OUT_DIR/endpoints/b-status.json" 2>/dev/null || true)
  tc=$(jq -r '.data.selected_tip // empty' "$OUT_DIR/endpoints/c-status.json" 2>/dev/null || true)
  echo "$(date -u +%FT%TZ),$ha,$hb,$hc,$ta,$tb,$tc" >> "$OUT_DIR/height-samples.csv"
  if (( ha > 0 && hb > 0 && hc > 0 )) && [[ -n "$ta" && "$ta" == "$tb" && "$tb" == "$tc" ]]; then converged=1; break; fi
  sleep 10
done

rg -q "template" "$OUT_DIR/logs/miner.log" || { echo "FAIL miner never received templates"; exit 1; }
rg -q "submit|accepted|reject" "$OUT_DIR/logs/miner.log" || { echo "FAIL miner never attempted submits"; exit 1; }
if (( converged == 0 )); then echo "FAIL nodes did not converge"; exit 1; fi

cat > "$OUT_DIR/summary.md" <<SUM
# v2.2.18 local 3N/1M smoke
- run_id: $RUN_ID
- duration_secs: $DURATION_SECS
- miner_address: $MINER_ADDRESS
- converged: yes
- output_dir: $OUT_DIR
SUM
