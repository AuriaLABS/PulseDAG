#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NODE_URL="${NODE_URL:-http://127.0.0.1:8080}"
MINER_ADDRESS="${MINER_ADDRESS:-}"
MAX_TRIES="${MAX_TRIES:-25000}"
THREADS="${THREADS:-2}"
SLEEP_MS="${SLEEP_MS:-1200}"
WAIT_SECONDS="${WAIT_SECONDS:-30}"
NODE_BIN="${NODE_BIN:-}"
MINER_BIN="${MINER_BIN:-}"
RPC_BIND="${RPC_BIND:-}"

usage() {
  cat <<'EOF'
Standalone operator smoke (external miner only).

Usage:
  scripts/release/standalone_operator_smoke.sh --miner-address <ADDRESS> [options]

Options:
  --miner-address <ADDRESS>  Required miner payout address.
  --node-url <URL>           Node URL (default: http://127.0.0.1:8080)
  --max-tries <N>            Miner max tries for one-shot probe (default: 25000)
  --threads <N>              Miner threads for one-shot probe (default: 2)
  --sleep-ms <N>             Loop sleep guidance for logs (default: 1200)
  --wait-seconds <N>         Node readiness wait timeout (default: 30)
  --node-bin <PATH>          Reuse an already-built pulsedagd binary.
  --miner-bin <PATH>         Reuse an already-built pulsedag-miner binary.
  --rpc-bind <ADDR>          Override PULSEDAG_RPC_BIND for the smoke node.

When --node-bin/--miner-bin are omitted, the helper preserves the historical
cargo build/cargo run behavior. If either prebuilt binary is supplied, both are
required.

Scope guardrails:
  - Miner remains external and standalone.
  - No pool logic.
  - No consensus changes.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --miner-address)
      MINER_ADDRESS="$2"
      shift 2
      ;;
    --node-url)
      NODE_URL="$2"
      shift 2
      ;;
    --max-tries)
      MAX_TRIES="$2"
      shift 2
      ;;
    --threads)
      THREADS="$2"
      shift 2
      ;;
    --sleep-ms)
      SLEEP_MS="$2"
      shift 2
      ;;
    --wait-seconds)
      WAIT_SECONDS="$2"
      shift 2
      ;;
    --node-bin)
      NODE_BIN="$2"
      shift 2
      ;;
    --miner-bin)
      MINER_BIN="$2"
      shift 2
      ;;
    --rpc-bind)
      RPC_BIND="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "${MINER_ADDRESS}" ]]; then
  echo "Error: --miner-address is required." >&2
  usage
  exit 1
fi

resolve_binary() {
  local path="$1"
  if [[ "${path}" != /* ]]; then
    path="${ROOT_DIR}/${path}"
  fi
  printf '%s\n' "${path}"
}

USE_PREBUILT=false
if [[ -n "${NODE_BIN}" || -n "${MINER_BIN}" ]]; then
  if [[ -z "${NODE_BIN}" || -z "${MINER_BIN}" ]]; then
    echo "Error: --node-bin and --miner-bin must be supplied together." >&2
    exit 1
  fi
  NODE_BIN="$(resolve_binary "${NODE_BIN}")"
  MINER_BIN="$(resolve_binary "${MINER_BIN}")"
  if [[ ! -x "${NODE_BIN}" ]]; then
    echo "Error: node binary is not executable: ${NODE_BIN}" >&2
    exit 1
  fi
  if [[ ! -x "${MINER_BIN}" ]]; then
    echo "Error: miner binary is not executable: ${MINER_BIN}" >&2
    exit 1
  fi
  USE_PREBUILT=true
fi

echo "== Standalone packaging smoke checks =="
if [[ "${USE_PREBUILT}" == true ]]; then
  echo "Using prebuilt node: ${NODE_BIN}"
  echo "Using prebuilt miner: ${MINER_BIN}"
else
  (
    cd "${ROOT_DIR}"
    cargo build --quiet -p pulsedagd --bin pulsedagd
    cargo build --quiet -p pulsedag-miner --bin pulsedag-miner
  )
fi

echo "== Launch local node =="
NODE_LOG="$(mktemp -t pulsedag-node-smoke.XXXXXX.log)"
NODE_DATA_ROOT="$(mktemp -d -t pulsedag-node-data.XXXXXX)"
NODE_ROCKSDB_PATH="${NODE_DATA_ROOT}/rocksdb"
cleanup() {
  if [[ -n "${NODE_PID:-}" ]] && kill -0 "${NODE_PID}" 2>/dev/null; then
    kill "${NODE_PID}" || true
    wait "${NODE_PID}" 2>/dev/null || true
  fi
  rm -rf "${NODE_DATA_ROOT}"
  echo "Node log: ${NODE_LOG}"
}
trap cleanup EXIT

run_node() {
  cd "${ROOT_DIR}"
  export PULSEDAG_ROCKSDB_PATH="${NODE_ROCKSDB_PATH}"
  if [[ -n "${RPC_BIND}" ]]; then
    export PULSEDAG_RPC_BIND="${RPC_BIND}"
  fi
  if [[ "${USE_PREBUILT}" == true ]]; then
    exec "${NODE_BIN}"
  fi
  exec cargo run --quiet -p pulsedagd
}

run_node >"${NODE_LOG}" 2>&1 &
NODE_PID=$!

echo "Using temporary RocksDB path: ${NODE_ROCKSDB_PATH}"
if [[ -n "${RPC_BIND}" ]]; then
  echo "Using isolated RPC bind: ${RPC_BIND}"
fi
echo "Waiting for ${NODE_URL}/status (timeout ${WAIT_SECONDS}s)..."
for _ in $(seq 1 "${WAIT_SECONDS}"); do
  if curl -fsS "${NODE_URL}/status" >/dev/null 2>&1; then
    echo "Node is ready."
    break
  fi
  if ! kill -0 "${NODE_PID}" 2>/dev/null; then
    echo "Smoke node exited before readiness." >&2
    tail -n 100 "${NODE_LOG}" >&2 || true
    exit 1
  fi
  sleep 1
done

if ! curl -fsS "${NODE_URL}/status" >/dev/null 2>&1; then
  echo "Node did not become ready in time." >&2
  tail -n 100 "${NODE_LOG}" >&2 || true
  exit 1
fi

echo "== External standalone miner one-shot probe =="
(
  cd "${ROOT_DIR}"
  if [[ "${USE_PREBUILT}" == true ]]; then
    "${MINER_BIN}" \
      --node "${NODE_URL}" \
      --miner-address "${MINER_ADDRESS}" \
      --threads "${THREADS}" \
      --max-tries "${MAX_TRIES}"
  else
    cargo run --quiet -p pulsedag-miner -- \
      --node "${NODE_URL}" \
      --miner-address "${MINER_ADDRESS}" \
      --threads "${THREADS}" \
      --max-tries "${MAX_TRIES}"
  fi
)

if [[ "${USE_PREBUILT}" == true ]]; then
  LOOP_COMMAND="${MINER_BIN} --node ${NODE_URL} --miner-address ${MINER_ADDRESS} --threads ${THREADS} --max-tries ${MAX_TRIES} --loop --sleep-ms ${SLEEP_MS}"
else
  LOOP_COMMAND="cargo run -p pulsedag-miner -- --node ${NODE_URL} --miner-address ${MINER_ADDRESS} --threads ${THREADS} --max-tries ${MAX_TRIES} --loop --sleep-ms ${SLEEP_MS}"
fi

cat <<EOF
Smoke completed.

Suggested continuous loop command (external standalone miner, no pool semantics):
${LOOP_COMMAND}
EOF
