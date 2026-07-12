#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIX_DIR="$ROOT_DIR/scripts/tests/fixtures/v2_2_20_evidence"
jq -e '.failure_classes == ["convergence"] and .primary_failure_class == "convergence" and .failure_class == .primary_failure_class' "$FIX_DIR/4m-convergence-only-fail-manifest.json" >/dev/null
jq -e '.failure_classes == ["storage_consistency"] and .primary_failure_class == "storage_consistency" and .failure_class == .primary_failure_class' "$FIX_DIR/4m-storage-only-fail-manifest.json" >/dev/null
jq -e '.failure_classes == ["convergence","storage_consistency","readiness"] and .primary_failure_class == "convergence" and .failure_class == .primary_failure_class and .mining_semantics.local_miner_submits_total == 1139 and .mining_semantics.local_miner_submits_accepted == 997 and .mining_semantics.local_miner_submits_rejected == 142 and (.mining_semantics.local_miner_submits_rejected_by_reason[] | select(.reason == "stale_template" and .count == 142))' "$FIX_DIR/4m-convergence-storage-readiness-fail-manifest.json" >/dev/null
jq -e '.failure_classes == ["readiness"] and .primary_failure_class == "readiness" and .failure_class == .primary_failure_class' "$FIX_DIR/4m-readiness-only-fail-manifest.json" >/dev/null
jq -e '.gates.baseline_5n_1m == "NOT_PROVIDED" and .gates.intermediate_5n_2m == "NOT_PROVIDED" and ((.failure_reasons // []) | index("STAGED_GATE_5N_2M") | not)' "$FIX_DIR/4m-standalone-prior-gates-missing-manifest.json" >/dev/null
jq -e 'all(.rich_node_state[]; if ((.sync.harness_observed_gap // 0) >= 94 and ((.sync.state // "") == "synced" or (.sync.state // "") == "steady")) then (.sync.sync_observability_incomplete == true) else true end)' "$FIX_DIR/4m-convergence-storage-readiness-fail-manifest.json" >/dev/null
