#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd "$script_dir/.." && pwd)"
export PULSEDAG_REHEARSAL_VERSION="v2.3.0"
export PULSEDAG_REHEARSAL_VERSION_SLUG="v2_3_0"
export MINER_COUNT=1
export STAGE_NAME="${STAGE_NAME:-5N/1M baseline}"
export OUT_DIR="${OUT_DIR:-$root_dir/artifacts/v2_3_0/private_5n_1m_rehearsal}"
exec bash "$script_dir/v2_3_0_private_rehearsal_compat.sh" "$@"
