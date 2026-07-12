#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIX="$ROOT_DIR/scripts/tests/fixtures/v2_2_20_evidence/4m-convergence-pass-storage-fail-manifest.json"
jq -e '
  .startup_result == "PASS" and
  .convergence_result == "PASS" and
  .consensus_state_result == "PASS" and
  .storage_consistency_result == "FAIL" and
  .readiness_result == "FAIL" and
  .overall_result == "FAIL" and
  .primary_failure_class == "storage_consistency" and
  .failure_class == .primary_failure_class and
  .failure_classes == ["storage_consistency","readiness"] and
  .gates.stress_5n_4m != null and
  .gates.intermediate_5n_2m == "NOT_PROVIDED" and
  .distinct_tips == 1 and
  .worst_lag_from_max_height == 0 and
  .post_quiescence.distinct_tips == 1 and
  .post_quiescence.worst_lag_from_max_height == 0 and
  (.rich_node_state[0].storage.memory_count == 1053) and
  (.rich_node_state[0].storage.persisted_count == 1054) and
  (.rich_node_state[0].storage.mismatch_source == "memory_persisted_count") and
  (.mining | has("accepted") | not)
' "$FIX" >/dev/null
for f in 1m-pass-manifest.json 2m-pass-manifest.json 4m-convergence-pass-storage-fail-manifest.json; do
  jq -e '(.rich_node_state | length > 0) and (.distinct_tips != 0) and ((.gates.stress_5n_4m // "NOT_EXECUTED") != "")' "$ROOT_DIR/scripts/tests/fixtures/v2_2_20_evidence/$f" >/dev/null
done
