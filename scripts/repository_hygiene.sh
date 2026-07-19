#!/usr/bin/env bash
set -euo pipefail

# Keep a shell entrypoint for existing workflows while the implementation stays
# testable and maintainable in Python.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$script_dir/repository_hygiene.py" "$@"
