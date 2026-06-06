#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

GATE="${1:-${REHEARSAL_GATE:-5n1m}}"
case "$GATE" in
  5n1m|5N1M|5n/1m|5N/1M)
    SCRIPT="scripts/v2_2_20_private_5n_1m_rehearsal.sh"
    DEFAULT_OUT="artifacts/v2_2_20/private_5n_1m_rehearsal"
    ;;
  5n2m|5N2M|5n/2m|5N/2M)
    SCRIPT="scripts/v2_2_20_private_5n_2m_rehearsal.sh"
    DEFAULT_OUT="artifacts/v2_2_20/private_5n_2m_rehearsal"
    ;;
  5n4m|5N4M|5n/4m|5N/4M|stress)
    SCRIPT="scripts/v2_2_20_private_5n_4m_rehearsal.sh"
    DEFAULT_OUT="artifacts/v2_2_20/private_5n_4m_rehearsal"
    ;;
  *)
    echo "Usage: $0 {5n1m|5n2m|5n4m}" >&2
    exit 2
    ;;
esac

export OUT_DIR="${OUT_DIR:-$ROOT_DIR/$DEFAULT_OUT}"
export DURATION_SECS="${DURATION_SECS:-600}"
export QUIESCENCE_WAIT_SECS="${QUIESCENCE_WAIT_SECS:-180}"
export GLOBAL_DEADLINE_SECS="${GLOBAL_DEADLINE_SECS:-2700}"
export RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
export NODE_BIN="${NODE_BIN:-$ROOT_DIR/target/release/pulsedagd}"
export MINER_BIN="${MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"

mkdir -p "$OUT_DIR"

echo "docker_v2_2_20_rehearsal gate=$GATE"
echo "script=$SCRIPT"
echo "out_dir=$OUT_DIR"
echo "duration_secs=$DURATION_SECS quiescence_wait_secs=$QUIESCENCE_WAIT_SECS global_deadline_secs=$GLOBAL_DEADLINE_SECS"
echo "node_bin=$NODE_BIN"
echo "miner_bin=$MINER_BIN"

exec bash "$SCRIPT"
