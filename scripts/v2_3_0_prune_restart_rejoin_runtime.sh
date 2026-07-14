#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:?OUT_DIR must be an absolute output directory}"
[[ "$OUT_DIR" = /* ]] || { echo "OUT_DIR must be absolute" >&2; exit 64; }
mkdir -p "$OUT_DIR" "$OUT_DIR/logs" "$OUT_DIR/endpoints" "$OUT_DIR/digests"
COMMAND_LOG="$OUT_DIR/command.log"
MANIFEST="$OUT_DIR/evidence_manifest.json"
START_TS="$(date -u +%FT%TZ)"
REAL_HARNESS="${PRUNE_RESTART_REJOIN_HARNESS:-$ROOT_DIR/scripts/lib/v2_3_0_runtime_harness.sh}"
MIN_OFFLINE_ADVANCE_BLOCKS="${MIN_OFFLINE_ADVANCE_BLOCKS:-64}"

log(){ printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$COMMAND_LOG"; }
run_logged(){ log "+ $*"; "$@" 2>&1 | tee -a "$COMMAND_LOG"; }
fail(){ log "FAIL: $*"; write_fail_manifest "$*"; exit 1; }

write_fail_manifest(){
  local reason="$1" commit
  commit="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
  jq -n \
    --arg result FAIL \
    --arg evidence_kind runtime \
    --arg candidate_commit "$commit" \
    --arg start_utc "$START_TS" \
    --arg end_utc "$(date -u +%FT%TZ)" \
    --arg failure_reason "$reason" \
    '{result:$result,evidence_kind:$evidence_kind,candidate_commit:$candidate_commit,started_at_utc:$start_utc,completed_at_utc:$end_utc,failure_reason:$failure_reason,public_testnet_ready:false}' \
    > "$MANIFEST"
}

require_json(){
  local filter="$1" message="$2"
  jq -e "$filter" "$MANIFEST" >/dev/null || fail "$message"
}

finish(){
  local rc=$?
  if [[ $rc -ne 0 && ! -s "$MANIFEST" ]]; then
    write_fail_manifest "driver failed with exit code $rc" || true
  fi
  (cd "$OUT_DIR" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS) || true
  exit $rc
}
trap finish EXIT

log "starting v2.3.0 prune/restart/rejoin runtime driver"
command -v jq >/dev/null || { write_fail_manifest "jq is required"; exit 65; }
run_logged git -C "$ROOT_DIR" rev-parse HEAD > "$OUT_DIR/candidate-sha.txt"

run_logged cargo build -p pulsedagd -p pulsedag-miner --release --locked
printf '{"node_binary":"%s","node_sha256":"%s","miner_binary":"%s","miner_sha256":"%s"}\n' \
  "$ROOT_DIR/target/release/pulsedagd" "$(sha256sum "$ROOT_DIR/target/release/pulsedagd" | awk '{print $1}')" \
  "$ROOT_DIR/target/release/pulsedag-miner" "$(sha256sum "$ROOT_DIR/target/release/pulsedag-miner" | awk '{print $1}')" \
  > "$OUT_DIR/release-binaries.json"

[[ -r "$REAL_HARNESS" ]] || fail "real runtime harness missing or unreadable: $REAL_HARNESS"
# shellcheck source=/dev/null
source "$REAL_HARNESS"
declare -F v2_3_0_run_prune_restart_rejoin_drill >/dev/null || \
  fail "$REAL_HARNESS does not define v2_3_0_run_prune_restart_rejoin_drill"

log "delegating to real prune/restart/rejoin function from $REAL_HARNESS"
v2_3_0_run_prune_restart_rejoin_drill \
  --out-dir "$OUT_DIR" \
  --node-bin "$ROOT_DIR/target/release/pulsedagd" \
  --miner-bin "$ROOT_DIR/target/release/pulsedag-miner" \
  --node-count 5 \
  --offline-node n5 \
  --min-offline-advance-blocks "$MIN_OFFLINE_ADVANCE_BLOCKS" \
  2>&1 | tee -a "$COMMAND_LOG"

[[ -s "$MANIFEST" ]] || fail "real runtime harness did not write $MANIFEST"
jq -e . "$MANIFEST" >/dev/null || fail "runtime manifest is invalid JSON"

require_json '.result == "PASS"' 'runtime harness did not report PASS'
require_json '.evidence_kind == "runtime"' 'manifest is not runtime evidence'
require_json '(.node_count // 0) == 5' 'manifest must prove five real nodes'
require_json '(.blocks_pruned_total // 0) > 0' 'blocks_pruned_total must be observed and > 0'
require_json '(.blocks_considered_total // 0) >= (.blocks_pruned_total // 0)' 'blocks considered/pruned counts are missing or inconsistent'
require_json '(.prune_boundary_height // null) != null' 'prune boundary height is missing'
require_json '((.retained_storage_hash_digest // "") | length) > 0 and .retained_storage_hash_digest == .retained_memory_hash_digest' 'retained storage/memory digests are missing or unequal'
require_json '(.storage_only_retained_hashes | type == "array") and (.storage_only_retained_hashes | length == 0)' 'storage-only retained hashes must be an empty observed array'
require_json '(.memory_only_retained_hashes | type == "array") and (.memory_only_retained_hashes | length == 0)' 'memory-only retained hashes must be an empty observed array'
require_json '.snapshot_delta_restart_executed == true and .restart_selected_tip_matches == true and .restart_state_root_matches == true' 'snapshot+delta restart invariants are missing or false'
require_json '((.pre_restart.selected_tip // "") | length) > 0 and .pre_restart.selected_tip == .post_restart.selected_tip' 'pre/post restart selected tip is missing or changed'
require_json '((.pre_restart.state_root // "") | length) > 0 and .pre_restart.state_root == .post_restart.state_root' 'pre/post restart state root is missing or changed'
jq -e --argjson min "$MIN_OFFLINE_ADVANCE_BLOCKS" '(.offline_advance_blocks // 0) >= $min' "$MANIFEST" >/dev/null || fail "offline selected-block advance must be at least $MIN_OFFLINE_ADVANCE_BLOCKS"
require_json '.rejoin_executed == true and .rejoin_converged == true' 'offline rejoin did not execute and converge'
require_json '.final_storage_memory_consistent == true' 'final storage/memory consistency was not proven'
require_json '.public_testnet_ready == false' 'public_testnet_ready guardrail must stay false'
require_json '(.final_nodes | type == "array") and (.final_nodes | length == 5) and all(.final_nodes[]; .ready == true and (.compatible_peers // -1) >= 4 and ((.selected_tip // "") | length) > 0 and ((.state_root // "") | length) > 0)' 'final five-node endpoint convergence evidence is missing or incomplete'
require_json '((.invariant_source // "") | test("endpoint|log|runtime|harness"; "i"))' 'manifest must identify real endpoint/log runtime evidence source'

for marker in 'storage-test' 'BLOCKS_PRUNED_TOTAL=1' 'pulsedag-v2.3.0-retained'; do
  if rg -n "$marker" "$OUT_DIR" >/dev/null 2>&1; then
    fail "fabricated marker or SHA-derived retained digest found in evidence"
  fi
done

log "PASS: validated real runtime evidence in $OUT_DIR"
