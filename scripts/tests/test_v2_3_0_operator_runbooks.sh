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
"""Serve deterministic incident-evidence fixtures."""

import http.server
import json
import os
import socketserver


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            payload = {"ok": True, "data": {"status": "ok", "operator_token": "must-redact"}}
            status = 200
        elif self.path == "/status":
            payload = {
                "ok": True,
                "data": {
                    "best_height": 99,
                    "nested": {"private_key": "must-redact", "safe": "visible"},
                },
            }
            status = 200
        else:
            payload = {"ok": False, "error": {"message": "missing"}}
            status = 404
        encoded = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

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
  if curl --fail --silent "http://127.0.0.1:${port}/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
curl --fail --silent "http://127.0.0.1:${port}/health" >/dev/null

collector=(
  python3 scripts/private_testnet/collect_incident_evidence.py
  --node-url "http://127.0.0.1:${port}"
  --severity SEV-2
  --operator operator-fixture
  --out-dir "$tmp_dir/evidence"
)

"${collector[@]}" \
  --incident-id INC-2026-0001-node-1 \
  --endpoint /health \
  --endpoint /status > "$tmp_dir/result.json"

jq -e '.result == "PASS" and .endpoint_count == 2 and .collection_failure_count == 0' \
  "$tmp_dir/result.json" >/dev/null
bundle="$tmp_dir/evidence/INC-2026-0001-node-1"
test -f "$bundle/manifest.json"
test -f "$bundle/SHA256SUMS"
(
  cd "$bundle"
  sha256sum --check SHA256SUMS >/dev/null
)
jq -e '.payload.data.operator_token == "<redacted>"' \
  "$bundle/responses/health.json" >/dev/null
jq -e '.payload.data.nested.private_key == "<redacted>" and .payload.data.nested.safe == "visible"' \
  "$bundle/responses/status.json" >/dev/null

if "${collector[@]}" \
  --incident-id INC-2026-0001-node-1 \
  --endpoint /health >/dev/null 2>&1; then
  echo "expected duplicate incident evidence bundle to fail" >&2
  exit 1
fi

set +e
"${collector[@]}" \
  --incident-id INC-2026-0002-node-1 \
  --endpoint /health \
  --endpoint /missing > "$tmp_dir/partial.json"
partial_status=$?
set -e
test "$partial_status" -eq 2
jq -e '.result == "PARTIAL" and .collection_failure_count == 1' \
  "$tmp_dir/partial.json" >/dev/null

python3 scripts/validate_v2_3_0_runbooks.py

echo "PASS: v2.3.0 operator runbook and incident evidence contract"
