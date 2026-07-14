#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT_DIR/scripts/v2_3_0_prune_restart_rejoin_runtime.sh"
WORKFLOW="$ROOT_DIR/.github/workflows/v2_3_0_prune_restart_rejoin_gate.yml"
bash -n "$SCRIPT"
grep -q 'OUT_DIR must be absolute' "$SCRIPT"
grep -q 'cargo build -p pulsedagd --bin pulsedagd --release --locked' "$SCRIPT"
grep -q 'scripts/lib/v2_3_0_runtime_harness.sh' "$SCRIPT"
grep -q 'source "$REAL_HARNESS"' "$SCRIPT"
grep -q 'v2_3_0_run_prune_restart_rejoin_drill' "$SCRIPT"
grep -q 'real runtime harness missing' "$SCRIPT"
grep -q 'PULSEDAGD_BIN=' "$SCRIPT"
grep -q 'MIN_OFFLINE_ADVANCE_BLOCKS="${MIN_OFFLINE_ADVANCE_BLOCKS:-64}"' "$SCRIPT"
grep -q 'blocks_pruned_total.*> 0' "$SCRIPT"
grep -q 'retained_storage_hash_digest == .retained_memory_hash_digest' "$SCRIPT"
grep -q 'snapshot_delta_restart_executed == true' "$SCRIPT"
grep -q 'rejoin_converged == true' "$SCRIPT"
grep -q 'final_nodes.*length == 5' "$SCRIPT"
grep -q 'public_testnet_ready.*false' "$SCRIPT"
grep -q '.offline_advance_blocks >= 64' "$WORKFLOW"
if grep -Eq 'cargo test -p pulsedag-storage|BLOCKS_PRUNED_TOTAL=1|storage-test-|pulsedag-v2\.3\.0-retained|tip-[0-9]+|sr-[0-9]+|rh-[0-9]+' "$SCRIPT"; then
  echo "fabricated evidence marker or unit-test closeout path found" >&2
  exit 1
fi
if [[ -e "$ROOT_DIR/scripts/v2_3_0_runtime_harness.sh" ]]; then
  echo "obsolete runtime harness path exists" >&2
  exit 1
fi

HARNESS="$ROOT_DIR/scripts/lib/v2_3_0_runtime_harness.sh"
bash -n "$HARNESS"
grep -q 'v2_3_0_run_prune_restart_rejoin_drill()' "$HARNESS"
grep -q 'V2_3_0_NODE_COUNT:-5' "$HARNESS"
grep -q 'MIN_OFFLINE_ADVANCE_BLOCKS:-64' "$HARNESS"
grep -q '/admin/prune' "$HARNESS"
grep -q '/snapshot/create' "$HARNESS"
grep -q '/runtime' "$HARNESS"
grep -q 'offline_advance >= min_offline' "$HARNESS"
grep -q 'compatible_peers' "$HARNESS"
if grep -Eq 'BLOCKS_PRUNED_TOTAL=1|storage-test-|pulsedag-v2\.3\.0-retained|tip-[0-9]+|sr-[0-9]+|rh-[0-9]+|retained_storage_hash_digest:.*sha|blocks_pruned_total: *1|node-[1-5].*ready.*true' "$HARNESS"; then
  echo "fabricated evidence marker, constant counter, SHA-derived digest, or fake node row found" >&2
  exit 1
fi
