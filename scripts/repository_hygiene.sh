#!/usr/bin/env bash
set -euo pipefail

# Keep a shell entrypoint for existing workflows while the implementations stay
# testable and maintainable in Python.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$script_dir/repository_hygiene.py" "$@"
python3 "$script_dir/repository_version_surface_audit.py" "$@"
