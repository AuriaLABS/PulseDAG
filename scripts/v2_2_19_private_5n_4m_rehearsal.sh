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

mkdir -p "$OUT_DIR" "$OUT_DIR/endpoints" "$OUT_DIR/logs"
exec > >(tee -a "$OUT_DIR/command-log.txt") 2>&1

declare -a NODE_PIDS=()
declare -a MINER_PIDS=()
declare -A NODE_READY NODE_TIP NODE_HEIGHT NODE_P2P_OK
FAIL_REASONS=()
ACCEPTED_BLOCKS=0
REJECTED_BLOCKS=0
TEMPLATES_OK=0

record_fail(){
  local msg="$1"
  echo "FAIL: $msg"
  FAIL_REASONS+=("$msg")
}

stop_pids(){
  local kind="$1"; shift
  local pids=("$@")
  for p in "${pids[@]:-}"; do
    kill "$p" 2>/dev/null || true
  done
  sleep 1
  for p in "${pids[@]:-}"; do
    kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null || true
  done
}

collect_final_state(){
  for i in 1 2 3 4 5; do
    local rpc=$((18179+i))
    local status_file="$OUT_DIR/endpoints/n${i}-status-final.json"
    local p2p_file="$OUT_DIR/endpoints/n${i}-p2p-status-final.json"
    if curl -fsS "http://127.0.0.1:${rpc}/status" -o "$status_file"; then
      NODE_HEIGHT[$i]="$(jq -r '.data.best_height // 0' "$status_file" 2>/dev/null || echo 0)"
      NODE_TIP[$i]="$(jq -r '.data.selected_tip // ""' "$status_file" 2>/dev/null || echo '')"
      NODE_READY[$i]=1
    else
      NODE_HEIGHT[$i]=0
      NODE_TIP[$i]=""
      NODE_READY[$i]=0
    fi
    if curl -fsS "http://127.0.0.1:${rpc}/p2p/status" -o "$p2p_file"; then
      local peers
      peers="$(jq -r '.data.peers | length // 0' "$p2p_file" 2>/dev/null || echo 0)"
      if (( i == 1 )); then
        (( peers >= 1 )) && NODE_P2P_OK[$i]=1 || NODE_P2P_OK[$i]=0
      else
        (( peers >= 1 )) && NODE_P2P_OK[$i]=1 || NODE_P2P_OK[$i]=0
      fi
    else
      NODE_P2P_OK[$i]=0
    fi
  done
}

write_evidence_summary(){
  local end_ts now_utc duration result git_ref git_commit version cmdline
  end_ts=$(date +%s)
  now_utc=$(date -u +%FT%TZ)
  duration=$((end_ts - START_TS))
  result="PASS"
  (( ${#FAIL_REASONS[@]} > 0 )) && result="FAIL"
  git_ref="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
  git_commit="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  version="$($NODE_BIN --version 2>/dev/null || echo unknown)"
  cmdline="$0 $*"

  {
    echo "# v2.2.19 Private 5N/4M Rehearsal Evidence"
    echo
    echo "- command line: \`$cmdline\`"
    echo "- git ref: \`$git_ref\`"
    echo "- git commit: \`$git_commit\`"
    echo "- version: \`$version\`"
    echo "- node count: $NODE_COUNT"
    echo "- miner count: $MINER_COUNT"
    echo "- start utc: $START_UTC"
    echo "- end utc: $now_utc"
    echo "- runtime duration (s): $duration"
    echo
    echo "## Final status per node"
    for i in 1 2 3 4 5; do
      echo "- n${i}: process_log=$OUT_DIR/logs/n${i}.log"
    done
    echo
    echo "## Final readiness per node"
    for i in 1 2 3 4 5; do
      echo "- n${i}: ${NODE_READY[$i]:-0}"
    done
    echo
    echo "## Final p2p status per node"
    for i in 1 2 3 4 5; do
      echo "- n${i}: ${NODE_P2P_OK[$i]:-0}"
    done
    echo
    echo "## Final tips/heights"
    for i in 1 2 3 4 5; do
      echo "- n${i}: height=${NODE_HEIGHT[$i]:-0} tip=${NODE_TIP[$i]:-}"
    done
    echo
    echo "## Block acceptance/rejection counters"
    echo "- accepted blocks: $ACCEPTED_BLOCKS"
    echo "- rejected blocks: $REJECTED_BLOCKS"
    echo
    echo "## Result"
    echo "- pass/fail: $result"
    if (( ${#FAIL_REASONS[@]} > 0 )); then
      echo "- reasons:"
      for r in "${FAIL_REASONS[@]}"; do echo "  - $r"; done
    fi
  } > "$OUT_DIR/evidence-summary.md"
}

cleanup(){
  collect_final_state || true
  write_evidence_summary "$@" || true
  stop_pids nodes "${NODE_PIDS[@]:-}"
  stop_pids miners "${MINER_PIDS[@]:-}"
  wait || true
}
trap cleanup EXIT INT TERM

OUT_DIR="$OUT_DIR" "$ROOT_DIR/scripts/v2_2_19_preflight_check.sh"
cargo build --locked --bin pulsedagd --bin pulsedag-miner --release

start_node(){
  local idx="$1" rpc="$2" p2p="$3" bootnode="$4"
  local name="n${idx}" data="$OUT_DIR/data-${name}"
  mkdir -p "$data"
  local cmd=("$NODE_BIN" --rpc-bind "127.0.0.1:${rpc}" --p2p-bind "/ip4/127.0.0.1/tcp/${p2p}" --data-dir "$data" --network-profile "rehearsal-${name}")
  [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
  "${cmd[@]}" > "$OUT_DIR/logs/${name}.log" 2>&1 &
  NODE_PIDS+=("$!")
}

wait_node_ready(){
  local idx="$1" rpc=$((18179+idx))
  for _ in $(seq 1 30); do
    if curl -fsS "http://127.0.0.1:${rpc}/status" -o "$OUT_DIR/endpoints/n${idx}-status-ready.json"; then
      NODE_READY[$idx]=1
      return 0
    fi
    sleep 2
  done
  NODE_READY[$idx]=0
  record_fail "node n${idx} failed readiness"
  tail -n 120 "$OUT_DIR/logs/n${idx}.log" > "$OUT_DIR/logs/n${idx}-readiness-fail-tail.log" || true
  return 1
}

start_node 1 18180 19180 ""; sleep 2
NODE_1_ID=$(rg -n "local_node_id|peer_id" "$OUT_DIR/logs/n1.log" | head -n1 | sed -E 's/.*(12D[[:alnum:]]+).*/\1/' || true)
BOOT_1=""; [[ -n "$NODE_1_ID" ]] && BOOT_1="/ip4/127.0.0.1/tcp/19180/p2p/${NODE_1_ID}"
start_node 2 18181 19181 "$BOOT_1"
start_node 3 18182 19182 "$BOOT_1"
start_node 4 18183 19183 "$BOOT_1"
start_node 5 18184 19184 "$BOOT_1"
sleep 3

for i in 1 2 3 4 5; do wait_node_ready "$i" || true; done

for i in 1 2 3 4; do
  local_node="http://127.0.0.1:$((18179+i))"
  "$MINER_BIN" --node "$local_node" --miner-address "v2219-${RUN_ID}-miner-${i}" --backend cpu --threads 1 --loop > "$OUT_DIR/logs/miner-${i}.log" 2>&1 &
  MINER_PIDS+=("$!")
  echo "$local_node" > "$OUT_DIR/endpoints/miner-${i}-node-endpoint.txt"
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
    curl -fsS "http://127.0.0.1:${rpc}/status" -o "$OUT_DIR/endpoints/n${i}-status.json" || true
    curl -fsS "http://127.0.0.1:${rpc}/p2p/status" -o "$OUT_DIR/endpoints/n${i}-p2p-status.json" || true
    heights+=("$(jq -r '.data.best_height // 0' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo 0)")
    tips+=("$(jq -r '.data.selected_tip // ""' "$OUT_DIR/endpoints/n${i}-status.json" 2>/dev/null || echo '')")
  done

  tip_match=1; ref_tip="${tips[0]}"
  for t in "${tips[@]}"; do [[ "$t" == "$ref_tip" ]] || tip_match=0; done
  echo "$(date -u +%FT%TZ),${heights[0]},${heights[1]},${heights[2]},${heights[3]},${heights[4]},$tip_match" >> "$OUT_DIR/height-samples.csv"

  for i in 1 2 3 4; do
    rg -qi "submit|accepted|reject" "$OUT_DIR/logs/miner-${i}.log" && miner_submit[$i]=1 || true
    rg -qi "accepted" "$OUT_DIR/logs/miner-${i}.log" && miner_accept[$i]=1 || true
  done

  ACCEPTED_BLOCKS=$(rg -c "accepted" "$OUT_DIR/logs/miner-"*.log 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')
  REJECTED_BLOCKS=$(rg -ci "reject" "$OUT_DIR/logs/miner-"*.log 2>/dev/null | awk -F: '{s+=$2} END {print s+0}')

  (( ACCEPTED_BLOCKS > 0 )) && TEMPLATES_OK=1

  if (( $(date +%s) > grace_end )) && (( tip_match == 0 )); then
    record_fail "selected tips diverged after grace window"
    for i in 1 2 3 4 5; do
      echo "n${i}: height=${heights[$((i-1))]} tip=${tips[$((i-1))]}" | tee -a "$OUT_DIR/convergence-failure.txt"
    done
    break
  fi

  sleep 10
done

for i in 1 2 3 4; do
  (( miner_submit[$i] == 1 )) || {
    record_fail "miner-${i} submit count is zero"
    tail -n 120 "$OUT_DIR/logs/miner-${i}.log" > "$OUT_DIR/logs/miner-${i}-submit-fail-tail.log" || true
    curl -sS "http://127.0.0.1:$((18179+i))/status" > "$OUT_DIR/endpoints/miner-${i}-node-status-on-fail.json" || true
  }
done

(( TEMPLATES_OK == 1 )) || record_fail "no accepted blocks observed; mining templates/submit path may be unavailable"
for i in 1 2 3 4 5; do
  [[ "${NODE_READY[$i]:-0}" == "1" ]] || record_fail "node n${i} not ready"
  [[ "${NODE_P2P_OK[$i]:-0}" == "1" ]] || record_fail "node n${i} missing expected visible p2p peers"
done

if (( ${#FAIL_REASONS[@]} > 0 )); then
  echo "FAIL private rehearsal: $OUT_DIR"
  exit 1
fi

jq -n \
  --arg run_id "$RUN_ID" \
  --arg duration_secs "$DURATION_SECS" \
  --argjson miner_submit "$(for i in 1 2 3 4; do printf '\"miner-%s\":%s,' "$i" "${miner_submit[$i]}"; done | sed 's/,$//;s/^/{/;s/$/}/')" \
  --argjson miner_accept "$(for i in 1 2 3 4; do printf '\"miner-%s\":%s,' "$i" "${miner_accept[$i]}"; done | sed 's/,$//;s/^/{/;s/$/}/')" \
  --arg accepted "$ACCEPTED_BLOCKS" \
  --arg rejected "$REJECTED_BLOCKS" \
  '{run_id:$run_id,duration_secs:($duration_secs|tonumber),miners_submit:$miner_submit,miners_accepted:$miner_accept,accepted_blocks:($accepted|tonumber),rejected_blocks:($rejected|tonumber)}' > "$OUT_DIR/rehearsal-summary.json"

echo "PASS private rehearsal complete: $OUT_DIR"
