#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

tmp_dir="$(mktemp -d)"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

port="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"

cat > "$tmp_dir/fake_rpc.py" <<'PY'
#!/usr/bin/env python3
"""Serve deterministic public-safe RPC fixtures for the v2.4 observability contract."""

import http.server
import json
import os
import socketserver

STATUS = {
    "best_height": 2400,
    "uptime_secs": 7200,
    "snapshot_exists": True,
    "persisted_block_count": 2401,
    "peer_count": 4,
    "rpc_response_degraded": False,
    "rpc_response_stale": False,
    "p2p_status_degraded": False,
}
MEMPOOL = {
    "transaction_count": 9,
    "orphan_transaction_count": 2,
    "orphan_limit": 128,
    "orphaned_total": 20,
    "orphan_promoted_total": 7,
}
METRICS = {
    "accepted_commit_generation_conflict_total": 0,
    "accepted_commit_reprepare_total": 0,
    "accepted_commit_serialized_total": 2400,
    "accepted_commit_publish_mismatch_total": 0,
    "chain_state_mutation_conflict_total": 0,
    "chain_state_reprepare_total": 0,
    "accepted_hash_lost_from_memory_total": 0,
    "accepted_hash_terminalization_prevented_total": 0,
    "accepted_storage_repair_total": 0,
    "invalid_state_root_total": 0,
    "invalid_state_root_stale_template_total": 0,
    "invalid_state_root_unknown_context_total": 0,
    "parent_state_context_rebuild_total": 2,
    "parent_state_context_unavailable_total": 0,
    "snapshot_verification_generation_changed_total": 1,
    "snapshot_verification_stable_failure_total": 0,
    "snapshot_verification_retry_total": 1,
    "blocks_accepted_total": 2400,
    "blocks_rejected_total": 3,
    "invalid_pow_total": 1,
    "mining_templates_total": 2500,
    "mining_submits_total": 2404,
    "external_mining_submit_actor_queue_len": 0,
    "external_mining_submit_actor_queue_full_total": 0,
    "external_mining_submit_actor_timeout_total": 1,
    "external_mining_submit_actor_completed_total": 2403,
    "external_mining_submit_actor_oldest_pending_age_ms": 0,
    "template_stale_reject_total": 2,
    "peer_count": 4,
    "peer_effective_count": 4,
    "peer_min_target_missed_total": 1,
    "peer_min_target_reconnect_attempt_total": 1,
    "peer_min_target_reconnect_success_total": 1,
    "peer_zero_count_duration_seconds": 0,
    "peer_zero_reconnect_attempt_total": 0,
    "peer_zero_reconnect_success_total": 0,
    "recovery_queue_depth": 0,
    "recovery_queue_dropped_total": 0,
    "recovery_queue_delayed_total": 2,
    "network_selected_height_gap": 0,
    "storage_replay_gap": 0,
    "selected_tip_mismatch": False,
    "live_sync_error_active": 0,
    "catchup_recovery_started_total": 1,
    "catchup_recovery_completed_total": 1,
    "final_quiescence_height_gap_after": 0,
    "final_quiescence_distinct_tips_after": 1,
    "final_quiescence_tip_reconcile_blocked_total": 0,
    "selected_segment_gap_blocks": 0,
    "selected_segment_restarts_total": 0,
    "rpc_degraded_response_total": 0,
    "rpc_snapshot_stale_total": 0,
    "rpc_handler_degraded_total": 0,
    "rpc_liveness_current_degraded": False,
    "rpc_liveness_historical_degraded_total": 0,
    "oldest_inflight_rpc_handler_age_ms": 0,
}
RESPONSES = {"/status": STATUS, "/metrics": METRICS, "/mempool": MEMPOOL}


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path not in RESPONSES:
            self.send_error(404)
            return
        payload = json.dumps(
            {"ok": True, "data": RESPONSES[self.path], "error": None, "meta": {}}
        ).encode()
        self.send_response(200)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, *_args):
        return


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True


with Server(("127.0.0.1", int(os.environ["FIXTURE_PORT"])), Handler) as server:
    server.serve_forever()
PY

FIXTURE_PORT="$port" python3 "$tmp_dir/fake_rpc.py" &
server_pid=$!
for _ in {1..50}; do
  if curl --fail --silent "http://127.0.0.1:${port}/status" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
curl --fail --silent "http://127.0.0.1:${port}/status" >/dev/null

python3 scripts/validate_v2_4_0_observability.py
python3 scripts/validate_observability_package.py
python3 -m py_compile scripts/private_testnet/runtime_metrics_exporter.py

python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url "http://127.0.0.1:${port}" \
  --inventory ops/observability/v2.4.0/metrics-inventory.json \
  --instance node-fixture \
  --once > "$tmp_dir/metrics.txt"

grep -q '^pulsedag_exporter_scrape_success 1$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_node_best_height 2400$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_chain_commit_publish_mismatch_total 0$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_mining_submit_actor_timeout_total 1$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_sync_selected_tip_mismatch 0$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_exporter_info{instance="node-fixture",release_line="v2.4.0"} 1$' \
  "$tmp_dir/metrics.txt"

if python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url "http://127.0.0.1:1" \
  --inventory ops/observability/v2.4.0/metrics-inventory.json \
  --timeout 0.2 \
  --once > "$tmp_dir/failed-metrics.txt"; then
  echo "expected unreachable RPC collection to return non-zero" >&2
  exit 1
fi
grep -q '^pulsedag_exporter_scrape_success 0$' "$tmp_dir/failed-metrics.txt"

echo "PASS: v2.4.0 observability contract"
