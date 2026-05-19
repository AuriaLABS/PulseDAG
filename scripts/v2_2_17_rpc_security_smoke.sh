#!/usr/bin/env bash
set -euo pipefail

# v2.2.17 RPC security smoke checks.
# - Safe-by-default read probes.
# - Optional auth checks for protected/admin endpoints.
# - Artifacts saved for closeout evidence.

BASE_URL="${BASE_URL:-http://127.0.0.1:3000}"
ARTIFACT_DIR="${ARTIFACT_DIR:-artifacts/v2_2_17_rpc_security_smoke}"
PROTECTED_ENDPOINTS="${PROTECTED_ENDPOINTS:-/p2p/status /metrics /sync/status}"
ADMIN_ENDPOINTS="${ADMIN_ENDPOINTS:-/admin/runtime /runtime /admin/diagnostics /diagnostics}"
# If AUTH_HEADER is empty, auth-positive checks are skipped.
AUTH_HEADER="${AUTH_HEADER:-}"
# If START_LOCAL_NODE=1, NODE_START_CMD is executed when /health is not reachable.
START_LOCAL_NODE="${START_LOCAL_NODE:-0}"
NODE_START_CMD="${NODE_START_CMD:-}"
MAX_BODY_BYTES="${MAX_BODY_BYTES:-1048576}"
OVERSIZE_ENDPOINT="${OVERSIZE_ENDPOINT:-/tx/build}"
CORS_ORIGIN="${CORS_ORIGIN:-https://security-smoke.local}"

mkdir -p "${ARTIFACT_DIR}"

pass() { echo "[PASS] $*"; }
fail() { echo "[FAIL] $*"; exit 1; }
warn() { echo "[WARN] $*"; }

curl_capture() {
  local name="$1"; shift
  local body_file="${ARTIFACT_DIR}/${name}.body"
  local hdr_file="${ARTIFACT_DIR}/${name}.headers"
  local code_file="${ARTIFACT_DIR}/${name}.code"

  local code
  code=$(curl -sS -o "${body_file}" -D "${hdr_file}" -w "%{http_code}" "$@") || code="000"
  printf "%s\n" "${code}" > "${code_file}"
  echo "${code}"
}

http_ok() {
  local code="$1"
  [[ "${code}" =~ ^2[0-9][0-9]$ ]]
}

http_authz_fail() {
  local code="$1"
  [[ "${code}" == "401" || "${code}" == "403" ]]
}

node_ready() {
  local code
  code=$(curl_capture "health_probe" "${BASE_URL}/health")
  http_ok "${code}"
}

if ! node_ready; then
  if [[ "${START_LOCAL_NODE}" == "1" && -n "${NODE_START_CMD}" ]]; then
    warn "Node not reachable at ${BASE_URL}; starting via NODE_START_CMD"
    bash -lc "${NODE_START_CMD}" >"${ARTIFACT_DIR}/node_start.log" 2>&1 || fail "NODE_START_CMD failed"
    sleep 3
    node_ready || fail "Node still unreachable after start command"
  else
    fail "Node not reachable at ${BASE_URL}. Set START_LOCAL_NODE=1 and NODE_START_CMD to auto-start."
  fi
fi

pass "Node reachable at ${BASE_URL}"

for ep in /health /status /release /readiness; do
  name="public_${ep#/}"
  code=$(curl_capture "${name}" "${BASE_URL}${ep}")
  if http_ok "${code}"; then
    pass "Public-safe endpoint ${ep} returned ${code}"
  else
    fail "Public-safe endpoint ${ep} returned ${code}"
  fi
done

for ep in ${PROTECTED_ENDPOINTS}; do
  safe_name=$(echo "${ep}" | tr '/:' '__')
  code=$(curl_capture "protected_no_auth${safe_name}" "${BASE_URL}${ep}")
  if http_authz_fail "${code}"; then
    pass "Protected endpoint ${ep} denied unauthenticated request (${code})"
  else
    warn "Protected endpoint ${ep} returned ${code} without auth (policy may differ in local profile)"
  fi

  if [[ -n "${AUTH_HEADER}" ]]; then
    code=$(curl_capture "protected_with_auth${safe_name}" -H "${AUTH_HEADER}" "${BASE_URL}${ep}")
    if http_ok "${code}"; then
      pass "Protected endpoint ${ep} succeeded with auth (${code})"
    else
      warn "Protected endpoint ${ep} did not succeed with auth (${code})"
    fi
  fi
done

for ep in ${ADMIN_ENDPOINTS}; do
  safe_name=$(echo "${ep}" | tr '/:' '__')
  code=$(curl_capture "admin_default${safe_name}" "${BASE_URL}${ep}")
  if http_authz_fail "${code}" || [[ "${code}" == "404" ]]; then
    pass "Admin endpoint ${ep} is not openly enabled by default (${code})"
  else
    warn "Admin endpoint ${ep} responded ${code}; verify default hardening"
  fi
done

# Request size limit probe (best effort).
oversize_file="${ARTIFACT_DIR}/oversize_payload.bin"
python3 - <<PY > "${oversize_file}"
import sys
sys.stdout.write('A' * (${MAX_BODY_BYTES} + 1024))
PY

code=$(curl_capture "oversize_request" -X POST -H "Content-Type: application/json" --data-binary @"${oversize_file}" "${BASE_URL}${OVERSIZE_ENDPOINT}")
if [[ "${code}" == "413" || "${code}" == "400" || "${code}" == "422" ]]; then
  pass "Oversized body check returned defensive status ${code}"
else
  warn "Oversized body check returned ${code}; confirm request-size policy"
fi

# CORS behavior probe (best effort).
code=$(curl_capture "cors_options" -X OPTIONS -H "Origin: ${CORS_ORIGIN}" -H "Access-Control-Request-Method: GET" "${BASE_URL}/status")
if http_ok "${code}" || [[ "${code}" == "204" || "${code}" == "405" ]]; then
  pass "CORS preflight probe completed with status ${code}"
else
  warn "CORS preflight probe returned ${code}"
fi

# Secret string smoke scan (obvious leak indicators only).
secret_pattern='(PRIVATE KEY|BEGIN [A-Z ]*PRIVATE KEY|mnemonic|seed phrase|secret_key|api[_-]?key|token)'
for ep in status release readiness; do
  body_file="${ARTIFACT_DIR}/public_${ep}.body"
  if rg -i -n "${secret_pattern}" "${body_file}" > "${ARTIFACT_DIR}/secret_scan_${ep}.txt"; then
    fail "Potential secret-like string(s) found in /${ep}; see ${ARTIFACT_DIR}/secret_scan_${ep}.txt"
  else
    pass "No obvious secret-like strings found in /${ep}"
  fi
done

echo "Security smoke completed. Artifacts: ${ARTIFACT_DIR}"
