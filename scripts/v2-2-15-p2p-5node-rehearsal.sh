#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PULSEDAG_REHEARSAL_NODE_COUNT="${PULSEDAG_REHEARSAL_NODE_COUNT:-5}"
exec "$SCRIPT_DIR/v2-2-15-p2p-3node-rehearsal.sh" "$@"
