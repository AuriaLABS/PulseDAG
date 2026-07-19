#!/usr/bin/env bash
set -euo pipefail

# Compatibility entrypoint retained for existing workflows and operator docs.
# The version-agnostic implementation lives in repository_hygiene.sh.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "$script_dir/repository_hygiene.sh" "$@"
