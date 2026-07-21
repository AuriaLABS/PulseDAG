#!/usr/bin/env bash
set -euo pipefail

# Current v2.3.0 entrypoint. The underlying v2.2.20 harness remains the
# accepted compatibility implementation until its behavior-preserving module
# extraction is completed.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PULSEDAG_REHEARSAL_VERSION="v2.3.0"
exec bash "$script_dir/v2_2_20_private_5n_1m_rehearsal.sh" "$@"
