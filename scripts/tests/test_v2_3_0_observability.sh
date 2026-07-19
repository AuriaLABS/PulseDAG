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
"""Serve deterministic RPC fixtures for the observability contract."""

import http.server
import json
import os
import socketserver

STATUS = {
    "best_height": 1200,
    "uptime_secs": 3600,
    "block_count": 1201,
    "tip_count": 2,
    "orphan_count": 3,
    "mempool_size": 7,
    "peer_count": 4,
    "rpc_response_degraded": False,
    "rpc_response_stale": False,
    "p2p_status_degraded": False,
    "snapshot_exists": True,
    "snapshot_height": 1100,
    "persisted_block_count": 1201,
    "recommended_keep_from_height": 400,
}
SYNC = {
    "pending_block_requests": 2,
    "inflight_block_requests": 1,
    "pending_missing_parents": 0,
    "consistency_ok": True,
    "consistency_issue_count": 0,
    "lag_blocks": 4,
    "catchup_progress_bps": 5000,
    "network_selected_height_gap": 4,
    "storage_replay_gap": 0,
    "live_sync_error_active": 0,
    "missing_parent_request_timeouts": 3,
    "missing_parent_request_fallbacks": 1,
}
MEMPOOL = {
    "transaction_count": 7,
    "orphan_transaction_count": 2,
    "orphan_limit": 128,
    "orphaned_total": 10,
    "orphan_promoted_total": 4,
    "orphan_dropped_total": 2,
    "orphan_pruned_total": 1,
}
POW = {
    "status": "ok",
    "snapshot_count": 8,
    "latest_suggested_difficulty": 42,
    "latest_avg_block_interval_secs": 61,
}
RESPONSES = {
    "/status": STATUS,
    "/sync/status": SYNC,
    "/tx/mempool": MEMPOOL,
    "/pow/health": POW,
}


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

python3 scripts/validate_v2_3_0_observability.py
python3 scripts/validate_observability_package.py
python3 -m py_compile scripts/private_testnet/runtime_metrics_exporter.py

python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url "http://127.0.0.1:${port}" \
  --instance node-fixture \
  --once > "$tmp_dir/metrics.txt"

grep -q '^pulsedag_exporter_scrape_success 1$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_node_best_height 1200$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_sync_catchup_progress_ratio 0.5$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_mempool_orphan_transactions 2$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_pow_health_status{status="ok"} 1$' "$tmp_dir/metrics.txt"
grep -q '^pulsedag_exporter_info{instance="node-fixture",release_line="v2.3.0"} 1$' \
  "$tmp_dir/metrics.txt"

if python3 scripts/private_testnet/runtime_metrics_exporter.py \
  --node-url "http://127.0.0.1:1" \
  --timeout 0.2 \
  --once > "$tmp_dir/failed-metrics.txt"; then
  echo "expected unreachable RPC collection to return non-zero" >&2
  exit 1
fi
grep -q '^pulsedag_exporter_scrape_success 0$' "$tmp_dir/failed-metrics.txt"

echo "PASS: v2.3.0 observability contract"
