#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_9_common.sh"

# Bootnode wiring is best-effort because CLI flag names can vary across builds.
# If unsupported, node-b still starts for manual peering.
BOOT_A="$(node_p2p a)"
start_node b --bootnode "$BOOT_A"
