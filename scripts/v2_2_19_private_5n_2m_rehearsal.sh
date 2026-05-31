#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export MINER_COUNT=2
export STAGE_NAME="${STAGE_NAME:-5N/2M intermediate}"
export OUT_DIR="${OUT_DIR:-$ROOT_DIR/artifacts/v2_2_19/private_5n_2m_rehearsal}"
exec "$ROOT_DIR/scripts/v2_2_19_private_5n_4m_rehearsal.sh" "$@"
