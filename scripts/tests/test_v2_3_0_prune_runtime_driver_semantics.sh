#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(git rev-parse --show-toplevel)"
cd "$ROOT_DIR"
DRIVER="scripts/v2_3_0_prune_restart_rejoin_runtime.sh"
HARNESS="scripts/lib/v2_3_0_prune_restart_rejoin_harness.sh"

bash -n "$DRIVER"
bash -n "$HARNESS"
source "$HARNESS"
declare -F v2_3_0_run_prune_restart_rejoin_drill >/dev/null

grep -Fq 'target/release/pulsedag-miner' "$DRIVER"
grep -Fq 'source "$TASK04_HARNESS"' "$DRIVER"
grep -Fq 'minimum offline advance must be at least 64' "$HARNESS"
grep -Fq '"$miner_bin" --node' "$HARNESS"
grep -Fq '/snapshot/create' "$HARNESS"
grep -Fq '/admin/prune' "$HARNESS"
grep -Fq 'startup_snapshot_validated == true' "$HARNESS"
grep -Fq 'startup_delta_applied == true' "$HARNESS"
grep -Fq 'offline_advance_blocks:$offline_advance' "$HARNESS"
grep -Fq 'public_testnet_ready:false' "$HARNESS"
grep -Fq 'retained_storage_hash_digest == .data.retained_memory_hash_digest' "$HARNESS"

for marker in 'BLOCKS_PRUNED_TOTAL=1' 'pulsedag-v2.3.0-retained' 'storage-test'; do
  if grep -Fq "$marker" "$HARNESS"; then
    echo "fabricated Task 04 evidence marker present: $marker" >&2
    exit 1
  fi
done
