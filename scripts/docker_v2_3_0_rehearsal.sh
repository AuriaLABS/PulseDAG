#!/usr/bin/env bash
set -euo pipefail

# Current Docker entrypoint for v2.3.0. The accepted v2.2.20 harness remains
# the compatibility engine until its behavior-preserving extraction is complete.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PULSEDAG_REHEARSAL_VERSION="v2.3.0"
exec bash "$script_dir/docker_v2_2_20_rehearsal.sh" "$@"
