#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

tmp_dir="$(mktemp -d)"
lifecycle_root="$tmp_dir/lifecycle"
env_file="$tmp_dir/node.env"
preflight="$repo_root/scripts/v2_3_0_private_testnet_preflight.sh"
controller="$repo_root/scripts/private_testnet/node_lifecycle.py"

cleanup() {
  python3 "$controller" \
    --root "$lifecycle_root" \
    --env-file "$env_file" \
    --preflight-script "$preflight" \
    --stop-timeout 2 \
    stop >/dev/null 2>&1 || true
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

cat > "$env_file" <<ENV
PULSEDAG_PRIVATE_TESTNET_ROLE=node
PULSEDAG_CONFIG_PROFILE=private
PULSEDAG_NETWORK_PROFILE=private-testnet-v2.3.0
PULSEDAG_CHAIN_ID=pulsedag-private-v2.3.0
PULSEDAG_CONSENSUS_MODE=legacy
PULSEDAG_P2P_ENABLED=true
PULSEDAG_P2P_MODE=libp2p-real
PULSEDAG_P2P_LISTEN=/ip4/0.0.0.0/tcp/32333
PULSEDAG_P2P_BOOTSTRAP=/dns4/localhost/tcp/32333
PULSEDAG_P2P_MDNS=false
PULSEDAG_P2P_KADEMLIA=true
PULSEDAG_P2P_IDENTITY_KEY=/var/lib/pulsedag-task09/identity.key
PULSEDAG_PUBLIC_P2P_MULTIADDR=/dns4/node-task09.local/tcp/32334
PULSEDAG_RPC_BIND=127.0.0.1:${port}
PULSEDAG_API_PROFILE=private_operator
PULSEDAG_ADMIN_ENABLED=false
PULSEDAG_RPC_RATE_LIMIT_REQUESTS_PER_MINUTE=120
PULSEDAG_RPC_RATE_LIMIT_PER_IP=true
PULSEDAG_ROCKSDB_PATH=/var/lib/pulsedag-task09/rocksdb
PULSEDAG_AUTO_REBUILD_ON_START=true
PULSEDAG_PERSIST_SNAPSHOT_ON_START=true
PULSEDAG_AUTO_PRUNE_ENABLED=true
PULSEDAG_AUTO_PRUNE_EVERY_BLOCKS=100
PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS=800
PULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true
PULSEDAG_PUBLIC_TESTNET_READY=false
PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false
ENV

make_fake_node() {
  local path="$1"
  local label="$2"
  cat > "$path" <<PY
#!/usr/bin/env python3
"""Fake PulseDAG node used by the lifecycle contract test."""

import http.server
import json
import os
import socketserver

LABEL = ${label@Q}
host, port = os.environ["PULSEDAG_RPC_BIND"].rsplit(":", 1)


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            payload = json.dumps({"status": "ok", "release": LABEL}).encode()
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
            return
        self.send_error(404)

    def log_message(self, *_args):
        return


class Server(socketserver.TCPServer):
    allow_reuse_address = True


with Server((host, int(port)), Handler) as server:
    server.serve_forever()
PY
  chmod +x "$path"
}

make_fake_node "$tmp_dir/pulsedagd-v1" "v1"
make_fake_node "$tmp_dir/pulsedagd-v2" "v2"

common=(
  python3 "$controller"
  --root "$lifecycle_root"
  --env-file "$env_file"
  --preflight-script "$preflight"
  --health-timeout 8
  --stop-timeout 3
)

"${common[@]}" install --binary "$tmp_dir/pulsedagd-v1" --release-id v1 >/dev/null

malicious_env="$tmp_dir/malicious.env"
marker="$tmp_dir/must-not-exist"
cp "$env_file" "$malicious_env"
printf 'PULSEDAG_CHAIN_ID=$(touch %s)\n' "$marker" >> "$malicious_env"
if python3 "$controller" \
  --root "$lifecycle_root" \
  --env-file "$malicious_env" \
  --preflight-script "$preflight" \
  verify >/dev/null 2>&1; then
  echo "expected shell expansion in environment data to fail" >&2
  exit 1
fi
if [[ -e "$marker" ]]; then
  echo "environment data executed shell code" >&2
  exit 1
fi

"${common[@]}" verify >/dev/null
"${common[@]}" start > "$tmp_dir/start-v1.json"
jq -e '.result == "PASS" and .changed == true and .release_id == "v1"' "$tmp_dir/start-v1.json" >/dev/null

"${common[@]}" start > "$tmp_dir/start-idempotent.json"
jq -e '.result == "PASS" and .changed == false and .status == "running"' \
  "$tmp_dir/start-idempotent.json" >/dev/null

"${common[@]}" upgrade --binary "$tmp_dir/pulsedagd-v2" --release-id v2 \
  > "$tmp_dir/upgrade.json"
jq -e '.result == "PASS" and .current_release == "v2" and .previous_release == "v1"' \
  "$tmp_dir/upgrade.json" >/dev/null

"${common[@]}" rollback > "$tmp_dir/rollback.json"
jq -e '.result == "PASS" and .current_release == "v1" and .previous_release == "v2"' \
  "$tmp_dir/rollback.json" >/dev/null

cat > "$tmp_dir/pulsedagd-bad" <<'BAD'
#!/usr/bin/env bash
# Exit immediately to prove that a failed upgrade restores the previous release.
exit 23
BAD
chmod +x "$tmp_dir/pulsedagd-bad"
if "${common[@]}" upgrade --binary "$tmp_dir/pulsedagd-bad" --release-id v3 \
  >/dev/null 2>&1; then
  echo "expected unhealthy upgrade to fail" >&2
  exit 1
fi
"${common[@]}" status > "$tmp_dir/status-after-failed-upgrade.json"
jq -e '.result == "PASS" and .status == "running" and .current_release == "v1"' \
  "$tmp_dir/status-after-failed-upgrade.json" >/dev/null

"${common[@]}" status > "$tmp_dir/status.json"
jq -e '.result == "PASS" and .status == "running" and .current_release == "v1"' \
  "$tmp_dir/status.json" >/dev/null

"${common[@]}" stop > "$tmp_dir/stop.json"
jq -e '.result == "PASS" and .changed == true and .status == "stopped"' \
  "$tmp_dir/stop.json" >/dev/null

"${common[@]}" stop > "$tmp_dir/stop-idempotent.json"
jq -e '.result == "PASS" and .changed == false and .status == "stopped"' \
  "$tmp_dir/stop-idempotent.json" >/dev/null

# A different binary may not reuse an existing release identifier.
cp "$tmp_dir/pulsedagd-v2" "$tmp_dir/pulsedagd-conflict"
printf '\n# conflict\n' >> "$tmp_dir/pulsedagd-conflict"
chmod +x "$tmp_dir/pulsedagd-conflict"
if "${common[@]}" install --binary "$tmp_dir/pulsedagd-conflict" --release-id v1 \
  >/dev/null 2>&1; then
  echo "expected conflicting release identifier to fail" >&2
  exit 1
fi

echo "PASS: private-testnet node lifecycle contract"
