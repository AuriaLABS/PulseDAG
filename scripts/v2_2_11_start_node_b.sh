#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_11_common.sh"
start_node b --bootnode "$(node_bootnode_a)" "$@"
