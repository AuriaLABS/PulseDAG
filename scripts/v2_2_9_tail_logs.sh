#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_9_common.sh"

ensure_dirs

touch "$(node_log_file a)" "$(node_log_file b)" "$(node_log_file c)"
exec tail -n 200 -f "$(node_log_file a)" "$(node_log_file b)" "$(node_log_file c)"
