#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MINER_BIN="${PULSEDAG_MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"

default_rpc="${1:-http://127.0.0.1:18080}"
PULSEDAG_NODE_RPC="${PULSEDAG_NODE_RPC:-$default_rpc}"
PULSEDAG_MINER_ADDRESS="${PULSEDAG_MINER_ADDRESS-}"
PULSEDAG_MINER_THREADS="${PULSEDAG_MINER_THREADS:-$(nproc)}"
PULSEDAG_MINER_MAX_TRIES="${PULSEDAG_MINER_MAX_TRIES:-0}"
PULSEDAG_MINER_SLEEP_MS="${PULSEDAG_MINER_SLEEP_MS:-250}"

if [[ -z "${PULSEDAG_MINER_ADDRESS// }" ]]; then
  echo "[error] PULSEDAG_MINER_ADDRESS cannot be empty"
  exit 1
fi

if [[ ! -x "$MINER_BIN" ]]; then
  echo "[error] Missing external miner binary: $MINER_BIN"
  echo "[hint] Build it first: cargo build --release -p pulsedag-miner"
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "[error] curl is required but was not found in PATH"
  exit 1
fi

if ! curl -fsS "$PULSEDAG_NODE_RPC/health" >/dev/null 2>&1; then
  echo "[error] Node RPC is unreachable at $PULSEDAG_NODE_RPC/health"
  echo "[hint] Start the target node first (for node A: scripts/v2_2_9_start_node_a.sh)"
  exit 1
fi

echo "[info] Starting external miner"
echo "       bin=$MINER_BIN"
echo "       rpc=$PULSEDAG_NODE_RPC"
echo "       address=$PULSEDAG_MINER_ADDRESS"
echo "       threads=$PULSEDAG_MINER_THREADS"
echo "       max-tries=$PULSEDAG_MINER_MAX_TRIES"
echo "       sleep-ms=$PULSEDAG_MINER_SLEEP_MS"
echo "       loop=true"

exec "$MINER_BIN" \
  --node "$PULSEDAG_NODE_RPC" \
  --miner-address "$PULSEDAG_MINER_ADDRESS" \
  --threads "$PULSEDAG_MINER_THREADS" \
  --max-tries "$PULSEDAG_MINER_MAX_TRIES" \
  --sleep-ms "$PULSEDAG_MINER_SLEEP_MS" \
  --loop
