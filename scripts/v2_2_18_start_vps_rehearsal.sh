#!/usr/bin/env bash
set -euo pipefail

RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
REHEARSAL_MODE="${REHEARSAL_MODE:-local}"
NODE_SHAPE="${NODE_SHAPE:-}"
MINER_COUNT="${MINER_COUNT:-}"
P2P_BASE_PORT="${P2P_BASE_PORT:-30333}"
RPC_BASE_PORT="${RPC_BASE_PORT:-18080}"


case "${REHEARSAL_MODE}" in
  smoke)
    : "${NODE_SHAPE:=3}"
    : "${MINER_COUNT:=1}"
    ;;
  local)
    : "${NODE_SHAPE:=3}"
    : "${MINER_COUNT:=1}"
    ;;
  staging)
    : "${NODE_SHAPE:=5}"
    : "${MINER_COUNT:=2}"
    ;;
  rc-full)
    : "${NODE_SHAPE:=5}"
    : "${MINER_COUNT:=4}"
    ;;
  *)
    echo "REHEARSAL_MODE must be one of: smoke, local, staging, rc-full"
    exit 1
    ;;
esac


if [[ "${NODE_SHAPE}" != "3" && "${NODE_SHAPE}" != "5" ]]; then
  echo "NODE_SHAPE must be 3 or 5"
  exit 1
fi

NODE_COUNT="${NODE_SHAPE}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

LOG_DIR="logs"
RUN_DIR="run"
ART_DIR="artifacts/v2_2_18_private_rc/${RUN_ID}"
mkdir -p "${LOG_DIR}" "${RUN_DIR}" "${ART_DIR}" "${ART_DIR}/meta" "${ART_DIR}/endpoints"

if [[ ! -x target/debug/pulsedagd || ! -x target/debug/pulsedag-miner ]]; then
  echo "Binaries not found in target/debug. Building workspace..."
  cargo build --workspace
fi

git rev-parse HEAD > "${ART_DIR}/meta/git_commit.txt" || true
cat VERSION > "${ART_DIR}/meta/VERSION.txt" || true
cargo metadata --format-version 1 --no-deps > "${ART_DIR}/meta/cargo_metadata.json" 2>/dev/null || true

: > "${RUN_DIR}/v2_2_18_vps_nodes.pid"
: > "${RUN_DIR}/v2_2_18_vps_miners.pid"

echo "run_id=${RUN_ID}" > "${RUN_DIR}/v2_2_18_vps_rehearsal.env"
echo "rehearsal_mode=${REHEARSAL_MODE}" >> "${RUN_DIR}/v2_2_18_vps_rehearsal.env"
echo "node_shape=${NODE_SHAPE}" >> "${RUN_DIR}/v2_2_18_vps_rehearsal.env"
echo "node_count=${NODE_COUNT}" >> "${RUN_DIR}/v2_2_18_vps_rehearsal.env"
echo "artifact_dir=${ART_DIR}" >> "${RUN_DIR}/v2_2_18_vps_rehearsal.env"

for ((i=0; i<NODE_COUNT; i++)); do
  node_name=$(printf "node-%s" "$(echo "ABCDE" | cut -c $((i+1)))")
  p2p_port=$((P2P_BASE_PORT + i))
  rpc_port=$((RPC_BASE_PORT + i))
  data_dir="${RUN_DIR}/${node_name}-data"
  log_file="${LOG_DIR}/${node_name}.log"

  mkdir -p "${data_dir}"

  nohup target/debug/pulsedagd \
    --data-dir "${data_dir}" \
    --p2p-bind "0.0.0.0:${p2p_port}" \
    --rpc-bind "127.0.0.1:${rpc_port}" \
    > "${log_file}" 2>&1 &

  pid=$!
  echo "${pid} ${node_name} ${p2p_port} ${rpc_port}" >> "${RUN_DIR}/v2_2_18_vps_nodes.pid"
  echo "Started ${node_name} pid=${pid} p2p=${p2p_port} rpc=127.0.0.1:${rpc_port}"
done

sleep 2

for ((m=0; m<MINER_COUNT; m++)); do
  miner_name="miner-$((m+1))"
  target_rpc_port=$((RPC_BASE_PORT + (m % NODE_COUNT)))
  log_file="${LOG_DIR}/${miner_name}.log"
  miner_address="${RUN_ID}-${miner_name}"

  nohup target/debug/pulsedag-miner \
    --node "http://127.0.0.1:${target_rpc_port}" \
    --miner-address "${miner_address}" \
    > "${log_file}" 2>&1 &

  pid=$!
  echo "${pid} ${miner_name} ${target_rpc_port}" >> "${RUN_DIR}/v2_2_18_vps_miners.pid"
  echo "Started ${miner_name} pid=${pid} target_rpc=http://127.0.0.1:${target_rpc_port} miner_address=${miner_address}"
done

cat > "${ART_DIR}/summary.md" <<SUM
# Ubuntu/VPS rehearsal start summary (v2.2.18)
- run_id: ${RUN_ID}
- rehearsal_mode: ${REHEARSAL_MODE}
- node_shape: ${NODE_SHAPE}
- node_count: ${NODE_COUNT}
- miner_count: ${MINER_COUNT}
- logs_dir: ${LOG_DIR}
- run_dir: ${RUN_DIR}
- artifacts_dir: ${ART_DIR}
- rpc_bind: localhost only (127.0.0.1)
SUM

echo "Rehearsal started. Artifacts: ${ART_DIR}"
