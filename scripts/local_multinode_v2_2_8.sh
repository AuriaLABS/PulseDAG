#!/usr/bin/env bash
set -euo pipefail

# PulseDAG v2.2.8 local multi-node lab preflight.
# Intentionally conservative: verifies workspace health and prints the manual lab doc path.
# It does not pretend to automate environment-specific node orchestration.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOC_PATH="$ROOT_DIR/docs/LOCAL_MULTINODE_LAB_V2_2_8.md"

if [[ ! -f "$DOC_PATH" ]]; then
  echo "[error] Missing lab doc: $DOC_PATH" >&2
  exit 1
fi

echo "[info] Running v2.2.8 local multi-node lab preflight checks"

(
  cd "$ROOT_DIR"
  cargo fmt --check
  cargo test --workspace
  cargo build --workspace
)

echo
 echo "[ok] Preflight checks completed."
echo "[next] Follow manual multi-node lab procedure in:"
echo "       docs/LOCAL_MULTINODE_LAB_V2_2_8.md"
