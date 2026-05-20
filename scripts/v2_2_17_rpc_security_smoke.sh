#!/usr/bin/env bash
set -euo pipefail

NODE_URL="${NODE_URL:-http://127.0.0.1:18080}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUT_DIR="${OUT_DIR:-artifacts/v2_2_17_rpc_security_smoke/${RUN_ID}}"
AUTH_HEADER="${AUTH_HEADER:-}"
mkdir -p "${OUT_DIR}"

curl_capture() {
  local name="$1" url="$2"
  curl -sS -D "${OUT_DIR}/${name}.headers" -o "${OUT_DIR}/${name}.body" -w "%{http_code}" "$url" > "${OUT_DIR}/${name}.code" || echo "000" > "${OUT_DIR}/${name}.code"
}

check_public() {
  local ep="$1" name="${ep#/}"
  curl_capture "$name" "${NODE_URL}${ep}"
}

for ep in /health /status /release /readiness; do
  check_public "$ep"
done

for ep in /admin/runtime /runtime /admin/diagnostics /diagnostics; do
  n="admin_${ep//\//_}"
  curl_capture "$n" "${NODE_URL}${ep}"
done

# protected check without storing tokens
for ep in /metrics /p2p/status /sync/status; do
  n="protected_${ep//\//_}"
  if [[ -n "$AUTH_HEADER" ]]; then
    curl -sS -D "${OUT_DIR}/${n}.headers" -o "${OUT_DIR}/${n}.body" -w "%{http_code}" -H "$AUTH_HEADER" "${NODE_URL}${ep}" > "${OUT_DIR}/${n}.code" || echo "000" > "${OUT_DIR}/${n}.code"
  else
    curl_capture "$n" "${NODE_URL}${ep}"
  fi
done

secret_pattern='(PRIVATE KEY|BEGIN [A-Z ]*PRIVATE KEY|mnemonic|seed phrase|secret_key|api[_-]?key|token)'
rg -i -n "$secret_pattern" "${OUT_DIR}" > "${OUT_DIR}/secret_scan_hits.txt" || true

cat > "${OUT_DIR}/summary.md" <<SUM
# v2.2.17 RPC security smoke summary
- node_url: ${NODE_URL}
- run_id: ${RUN_ID}
- artifacts: ${OUT_DIR}
- checked: /health /status /release /readiness
- checked admin defaults: /admin/runtime /runtime /admin/diagnostics /diagnostics
- checked protected endpoints: /metrics /p2p/status /sync/status
- note: auth tokens are never printed or persisted by this script.
SUM

echo "Smoke artifacts: ${OUT_DIR}"
