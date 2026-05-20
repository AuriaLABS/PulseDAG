#!/usr/bin/env bash
set -euo pipefail

NODE_URL="${NODE_URL:-http://127.0.0.1:18080}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUT_DIR="${OUT_DIR:-artifacts/v2_2_17_api_security/${RUN_ID}}"
AUTH_HEADER="${AUTH_HEADER:-}"
NODE_LOG_PATH="${NODE_LOG_PATH:-}"
mkdir -p "${OUT_DIR}/responses" "${OUT_DIR}/checks" "${OUT_DIR}/meta" "${OUT_DIR}/logs"

capture() {
  local ep="$1" n="${1#/}"; n="${n//\//_}"
  curl -sS -D "${OUT_DIR}/responses/${n}.headers" -o "${OUT_DIR}/responses/${n}.body" -w "%{http_code}" "${NODE_URL}${ep}" > "${OUT_DIR}/responses/${n}.code" || echo "000" > "${OUT_DIR}/responses/${n}.code"
}

for ep in /health /status /release /readiness /metrics /p2p/status /sync/status; do
  capture "$ep"
done

git rev-parse HEAD > "${OUT_DIR}/meta/git_commit.txt" || true
cat VERSION > "${OUT_DIR}/meta/VERSION.txt" || true
awk '/\[workspace.package\]/{f=1} f&&/version =/{print; exit}' Cargo.toml > "${OUT_DIR}/meta/cargo_workspace_version.txt" || true

# admin-disabled check
curl -sS -o "${OUT_DIR}/checks/admin_disabled.body" -D "${OUT_DIR}/checks/admin_disabled.headers" -w "%{http_code}" "${NODE_URL}/admin/runtime" > "${OUT_DIR}/checks/admin_disabled.code" || echo "000" > "${OUT_DIR}/checks/admin_disabled.code"

# auth-required check (no token persisted)
if [[ -n "$AUTH_HEADER" ]]; then
  curl -sS -o "${OUT_DIR}/checks/auth_required.body" -D "${OUT_DIR}/checks/auth_required.headers" -w "%{http_code}" -H "$AUTH_HEADER" "${NODE_URL}/metrics" > "${OUT_DIR}/checks/auth_required.code" || echo "000" > "${OUT_DIR}/checks/auth_required.code"
else
  curl -sS -o "${OUT_DIR}/checks/auth_required.body" -D "${OUT_DIR}/checks/auth_required.headers" -w "%{http_code}" "${NODE_URL}/metrics" > "${OUT_DIR}/checks/auth_required.code" || echo "000" > "${OUT_DIR}/checks/auth_required.code"
fi

if [[ -n "$NODE_LOG_PATH" && -f "$NODE_LOG_PATH" ]]; then
  cp "$NODE_LOG_PATH" "${OUT_DIR}/logs/"
fi

cat > "${OUT_DIR}/summary.md" <<SUM
# v2.2.17 API/operator/security evidence summary
- run_id: ${RUN_ID}
- node_url: ${NODE_URL}
- git_commit: $(cat "${OUT_DIR}/meta/git_commit.txt" 2>/dev/null || echo unavailable)
- version: $(cat "${OUT_DIR}/meta/VERSION.txt" 2>/dev/null || echo unavailable)
- cargo_workspace_version: $(cat "${OUT_DIR}/meta/cargo_workspace_version.txt" 2>/dev/null || echo unavailable)
- endpoints: /health /status /release /readiness /metrics /p2p/status /sync/status
- checks: admin-disabled + auth-required
- security constraints: no public network required, no GPU required, no real funds required, no token persistence
SUM

( cd "${OUT_DIR}" && tar -czf evidence.tar.gz responses checks meta logs summary.md )
echo "Evidence bundle: ${OUT_DIR}/evidence.tar.gz"
