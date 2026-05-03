#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_9_common.sh"

# Bootnode wiring is best-effort because CLI flag names can vary across builds.
BOOT_A="$(node_p2p a)"
BOOT_B="$(node_p2p b)"
start_node c --bootnode "$BOOT_A" --bootnode "$BOOT_B"
