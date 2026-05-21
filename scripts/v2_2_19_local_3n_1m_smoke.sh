#!/usr/bin/env bash
set -euo pipefail

DURATION_SECS=${DURATION_SECS:-900}
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/artifacts/v2_2_19_public_testnet_readiness/local-3n-1m/${RUN_ID}"
NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
MINER_ADDRESS="${MINER_ADDRESS:-v2219-${RUN_ID}-miner-a}"

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

PIDS=()
cleanup(){
  for p in "${PIDS[@]:-}"; do kill "$p" 2>/dev/null || true; done
  wait || true
}
trap cleanup EXIT INT TERM

OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"

cargo fmt --check
cargo test --workspace
cargo build --workspace --release
[[ -x "$NODE_BIN" ]] || { echo "FAIL missing binary: $NODE_BIN"; exit 1; }
[[ -x "$MINER_BIN" ]] || { echo "FAIL missing binary: $MINER_BIN"; exit 1; }

start_node(){
  local name="$1" rpc="$2" p2p="$3" bootnode="$4"
  local data="$OUT_DIR/data-$name"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --rpc-bind "127.0.0.1:${rpc}" --p2p-bind "/ip4/127.0.0.1/tcp/${p2p}" --data-dir "$data" --network-profile "rehearsal-${name}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  PIDS+=("$!")
  echo "$! node-$name" >> "$OUT_DIR/process-pids.txt"
}

start_node a 18080 19080 ""
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

eps=(health status readiness release p2p/status sync/status)
printf "node,endpoint,path,status\n" > "$OUT_DIR/endpoints-manifest.txt"

sample(){
  local node="$1" rpc="$2" ep="$3" path="/$3" out="$OUT_DIR/endpoints/${node}-${ep//\//_}.json"
  if curl -fsS "http://127.0.0.1:${rpc}${path}" -o "$out"; then
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
divergence=0

end=$(( $(date +%s) + DURATION_SECS ))
while (( $(date +%s) < end )); do
  for n in a b c; do
    rpc=$([[ "$n" == a ]] && echo 18080 || ([[ "$n" == b ]] && echo 18081 || echo 18082))
    for ep in "${eps[@]}"; do sample "$n" "$rpc" "$ep" || true; done
  done

  ha=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/a-status.json" 2>/dev/null || echo 0)
  hb=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/b-status.json" 2>/dev/null || echo 0)
  hc=$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/c-status.json" 2>/dev/null || echo 0)
  echo "$(date -u +%FT%TZ),$ha,$hb,$hc" >> "$OUT_DIR/height-samples.csv"

  if (( (ha == 0 || hb == 0 || hc == 0) && (ha > 0 || hb > 0 || hc > 0) )); then
    echo "FAIL node divergence: at least one node remains at height 0 while another advanced"
    divergence=1
    break
  fi

  if rg -qi "template" "$OUT_DIR/logs/miner.log"; then miner_templates=1; fi
  if rg -qi "submit|accepted|reject" "$OUT_DIR/logs/miner.log"; then miner_submits=1; fi
  sleep 10
done

(( divergence == 0 )) || exit 1
(( miner_templates == 1 )) || { echo "FAIL miner never receives templates"; exit 1; }
(( miner_submits == 1 )) || { echo "FAIL miner never submits"; exit 1; }

jq -n --arg run_id "$RUN_ID" --argjson heights "$(tail -n1 "$OUT_DIR/height-samples.csv" | awk -F, '{printf "{\"a\":%s,\"b\":%s,\"c\":%s}",$2,$3,$4}')" '{run_id:$run_id, final_heights:$heights}' > "$OUT_DIR/node-height-summary.json"
jq -n --arg run_id "$RUN_ID" --arg templates "$miner_templates" --arg submits "$miner_submits" '{run_id:$run_id, templates_seen:($templates=="1"), submits_seen:($submits=="1")}' > "$OUT_DIR/miner-submit-summary.json"
for n in a b c; do
  jq -n --arg node "$n" --slurpfile d "$OUT_DIR/endpoints/${n}-readiness.json" '{node:$node, captured:(($d|length)>0)}' >> "$OUT_DIR/readiness-summary.json"
  jq -n --arg node "$n" --slurpfile d "$OUT_DIR/endpoints/${n}-release.json" '{node:$node, captured:(($d|length)>0)}' >> "$OUT_DIR/release-summary.json"
done

cat > "$OUT_DIR/summary.md" <<SUM
# v2.2.19 local 3N/1M smoke
- run_id: $RUN_ID
- duration_secs: $DURATION_SECS
- miner_address: $MINER_ADDRESS
- rpc_bind: 127.0.0.1 only
- output_dir: $OUT_DIR
SUM

cat > "$OUT_DIR/manifest.txt" <<MAN
summary.md
command-log.txt
process-pids.txt
endpoints-manifest.txt
node-height-summary.json
miner-submit-summary.json
readiness-summary.json
release-summary.json
MAN

[[ -d "$OUT_DIR" ]] || { echo "FAIL no evidence directory produced"; exit 1; }

"$ROOT_DIR/scripts/v2_2_19_collect_local_evidence.sh" "$OUT_DIR"
[[ -f "$OUT_DIR/evidence.tar.gz" ]] || { echo "FAIL evidence tarball missing"; exit 1; }
[[ -f "$OUT_DIR/evidence.tar.gz.sha256" ]] || { echo "FAIL evidence checksum missing"; exit 1; }

echo "PASS local smoke complete: $OUT_DIR"
