#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT_DIR/scripts/v2_3_0_prune_restart_rejoin_runtime.sh"
bash -n "$SCRIPT"
grep -q 'OUT_DIR must be absolute' "$SCRIPT"
grep -q 'cargo build -p pulsedagd --bin pulsedagd --release --locked' "$SCRIPT"
grep -q 'cargo test -p pulsedag-storage non_zero --locked -- --nocapture' "$SCRIPT"
grep -q 'blocks_pruned_total' "$SCRIPT"
grep -q 'retained_storage_hash_digest' "$SCRIPT"
grep -q 'snapshot_delta_restart_executed' "$SCRIPT"
grep -q 'rejoin_converged' "$SCRIPT"
grep -q 'public_testnet_ready:false' "$SCRIPT"
if grep -Eq 'cp -a .*rocksdb|rsync .*rocksdb|blocks_pruned_total=0' "$SCRIPT"; then
  echo "forbidden RocksDB copy or zero prune marker found" >&2
  exit 1
fi
