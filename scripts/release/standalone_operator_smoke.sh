#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NODE_URL="${NODE_URL:-http://127.0.0.1:8080}"
MINER_ADDRESS="${MINER_ADDRESS:-}"
MAX_TRIES="${MAX_TRIES:-25000}"
THREADS="${THREADS:-2}"
SLEEP_MS="${SLEEP_MS:-1200}"
WAIT_SECONDS="${WAIT_SECONDS:-30}"

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

echo "== Standalone packaging smoke checks =="
(
  cd "${ROOT_DIR}"
  cargo run --quiet --bin pulsedagd -- --version
  cargo run --quiet -p pulsedag-miner -- --help >/dev/null
)

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

(
  cd "${ROOT_DIR}"
  PULSEDAG_ROCKSDB_PATH="${NODE_ROCKSDB_PATH}" cargo run --quiet -p pulsedagd
) >"${NODE_LOG}" 2>&1 &
NODE_PID=$!

echo "Using temporary RocksDB path: ${NODE_ROCKSDB_PATH}"
echo "Waiting for ${NODE_URL}/status (timeout ${WAIT_SECONDS}s)..."
for _ in $(seq 1 "${WAIT_SECONDS}"); do
  if curl -fsS "${NODE_URL}/status" >/dev/null 2>&1; then
    echo "Node is ready."
    break
  fi
  sleep 1
done

if ! curl -fsS "${NODE_URL}/status" >/dev/null 2>&1; then
  echo "Node did not become ready in time." >&2
  exit 1
fi

echo "== External standalone miner one-shot probe =="
(
  cd "${ROOT_DIR}"
  cargo run --quiet -p pulsedag-miner -- \
    --node "${NODE_URL}" \
    --miner-address "${MINER_ADDRESS}" \
    --threads "${THREADS}" \
    --max-tries "${MAX_TRIES}"
)

cat <<EOF
Smoke completed.

Suggested continuous loop command (external standalone miner, no pool semantics):
cargo run -p pulsedag-miner -- --node ${NODE_URL} --miner-address ${MINER_ADDRESS} --threads ${THREADS} --max-tries ${MAX_TRIES} --loop --sleep-ms ${SLEEP_MS}
EOF
