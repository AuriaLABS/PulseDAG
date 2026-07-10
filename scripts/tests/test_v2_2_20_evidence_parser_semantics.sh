#!/usr/bin/env bash
set -euo pipefail
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT_DIR/scripts/v2_2_20_private_5n_4m_rehearsal.sh"
STAGED="$ROOT_DIR/scripts/v2_2_20_staged_private_network_gates.sh"

grep_q(){ rg -q -- "$1" "$2"; }

grep_q 'GATE_5N_1M_BASELINE=NOT_PROVIDED' "$SCRIPT"
grep_q 'GATE_5N_2M_INTERMEDIATE=NOT_PROVIDED' "$SCRIPT"
grep_q 'GATE_5N_1M_BASELINE=FAIL_STARTUP' "$SCRIPT"
grep_q 'GATE_5N_2M_INTERMEDIATE=FAIL_STARTUP' "$SCRIPT"
grep_q 'NOT_EXECUTED' "$SCRIPT"
grep_q 'MINERS_STARTED > 0 && TOTAL_MINING_ACCEPTED > 0' "$SCRIPT"
grep_q 'startup_topology_timeout' "$SCRIPT"
grep_q 'STARTUP_TOPOLOGY_REQUIRED_STABLE_SAMPLES' "$SCRIPT"
grep_q 'STARTUP_TOPOLOGY_SAMPLE_INTERVAL_SECS' "$SCRIPT"
grep_q 'root_inbound >= expected' "$SCRIPT"
grep_q 'STARTUP_CONNECTION_KEEPALIVE_TIMEOUTS_TOTAL' "$SCRIPT"
grep_q 'STARTUP_RECONNECT_ATTEMPTS_TOTAL' "$SCRIPT"
grep_q 'STARTUP_RECONNECT_SUCCESS_TOTAL' "$SCRIPT"
grep_q 'PRIOR_GATE_C_MANIFEST' "$STAGED"
grep_q 'PRIOR_GATE_D_MANIFEST' "$STAGED"
grep_q 'prior_evidence' "$SCRIPT"
grep_q 'PRIOR_GATE_COMMIT_MISMATCH' "$SCRIPT"
grep_q 'PRIOR_GATE_RESULT_CONTRADICTION' "$SCRIPT"
grep_q 'public_testnet_ready' "$SCRIPT"
grep_q 'node_operational_ready' "$SCRIPT"
grep_q 'private_conservative_ready' "$SCRIPT"
grep_q 'fast_cadence_ready' "$SCRIPT"
grep_q 'ready_for_release' "$SCRIPT"
grep_q 'NODE_MISSING_PARENT_REQUESTS_SENT' "$SCRIPT"
grep_q 'NETWORK_COUNTER_AGGREGATE_MISMATCH' "$SCRIPT"
grep_q 'unique_templates_issued' "$SCRIPT"
grep_q 'local_miner_submits_total' "$SCRIPT"
grep_q 'node_block_accept_events_total' "$SCRIPT"
grep_q 'unique_block_hashes_observed' "$SCRIPT"
grep_q 'not unique network blocks' "$SCRIPT"

readiness='{"data":{"node_operational_ready":true,"private_conservative_ready":true,"fast_cadence_ready":false,"public_testnet_ready":false,"ready_for_release":true}}'
printf '%s' "$readiness" | jq -e '(.data // .) as $r | ($r|has("node_operational_ready")) and ($r.node_operational_ready|type == "boolean") and ($r|has("private_conservative_ready")) and ($r.private_conservative_ready|type == "boolean") and ($r|has("fast_cadence_ready")) and ($r.fast_cadence_ready|type == "boolean") and ($r|has("public_testnet_ready")) and ($r.public_testnet_ready|type == "boolean") and ($r|has("ready_for_release")) and ($r.ready_for_release|type == "boolean")' >/dev/null

sync_status='{"data":{"missing_parent_requests_sent":158,"network_counters":{"missing_parent_requests_sent":158}}}'
[[ "$(printf '%s' "$sync_status" | jq -r '.data.missing_parent_requests_sent // .data.network_counters.missing_parent_requests_sent // .data.sync_counters.missing_parent_requests_sent // .missing_parent_requests_sent // 0')" == "158" ]]
grep_q 'json_array_or_empty' "$SCRIPT"
grep_q 'json_number_or_zero' "$SCRIPT"
grep_q 'evidence-manifest-jq-diagnostics.txt' "$SCRIPT"
grep_q 'write_minimal_fallback_manifest' "$SCRIPT"
grep_q 'manifest_generation_error' "$SCRIPT"
grep_q 'EVIDENCE_MANIFEST_GENERATION_FAILED' "$SCRIPT"
grep_q 'assert_packaged_evidence' "$SCRIPT"
grep_q 'RUST_LOG_STYLE=never' "$SCRIPT"
grep_q 'pulsedagd=info,pulsedag_p2p=info' "$SCRIPT"
