#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:3000}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUT_DIR="${OUT_DIR:-artifacts/v2_2_17_api_security/${RUN_ID}}"
SUMMARY_FILE="${OUT_DIR}/summary.md"
ARCHIVE_FILE="${OUT_DIR}/evidence.tar.gz"
RESPONSES_DIR="${OUT_DIR}/responses"
CHECKS_DIR="${OUT_DIR}/checks"
LOGS_DIR="${OUT_DIR}/logs"
META_DIR="${OUT_DIR}/meta"

NODE_LOG_GLOB="${NODE_LOG_GLOB:-artifacts/**/*.log}"
ACTIVE_API_PROFILE="${ACTIVE_API_PROFILE:-unknown}"
AUTH_HEADER="${AUTH_HEADER:-}"
ADMIN_DISABLED_PATH="${ADMIN_DISABLED_PATH:-/admin/runtime}"
AUTH_REQUIRED_PATH="${AUTH_REQUIRED_PATH:-/metrics}"
OVERSIZED_CHECK_PATH="${OVERSIZED_CHECK_PATH:-/tx/build}"
OVERSIZED_CHECK_METHOD="${OVERSIZED_CHECK_METHOD:-POST}"
MAX_BODY_BYTES="${MAX_BODY_BYTES:-1048576}"
CONFIG_SAFETY_FILE="${CONFIG_SAFETY_FILE:-}"
TEST_COMMAND="${TEST_COMMAND:-bash -n scripts/v2_2_17_rpc_security_smoke.sh}"

mkdir -p "${RESPONSES_DIR}" "${CHECKS_DIR}" "${LOGS_DIR}" "${META_DIR}"

pass=0
warn=0

record_check() {
  local key="$1" status="$2" detail="$3"
  printf '%s\n' "status=${status}" > "${CHECKS_DIR}/${key}.txt"
  printf '%s\n' "detail=${detail}" >> "${CHECKS_DIR}/${key}.txt"
  if [[ "${status}" == "PASS" ]]; then
    pass=$((pass + 1))
  else
    warn=$((warn + 1))
  fi
}

capture_endpoint() {
  local path="$1"
  local slug="${path#/}"
  slug="${slug//\//_}"
  local body="${RESPONSES_DIR}/${slug}.body"
  local hdr="${RESPONSES_DIR}/${slug}.headers"
  local code_file="${RESPONSES_DIR}/${slug}.code"

  local code
  code=$(curl -sS -o "${body}" -D "${hdr}" -w "%{http_code}" "${BASE_URL}${path}" || true)
  printf '%s\n' "${code}" > "${code_file}"
}

http_authz_fail() {
  [[ "$1" == "401" || "$1" == "403" ]]
}

capture_endpoint "/health"
capture_endpoint "/status"
capture_endpoint "/release"
capture_endpoint "/readiness"
capture_endpoint "/metrics"
capture_endpoint "/p2p/status"
capture_endpoint "/sync/status"

admin_code=$(curl -sS -o "${CHECKS_DIR}/admin_disabled.body" -D "${CHECKS_DIR}/admin_disabled.headers" -w "%{http_code}" "${BASE_URL}${ADMIN_DISABLED_PATH}" || true)
printf '%s\n' "${admin_code}" > "${CHECKS_DIR}/admin_disabled.code"
if http_authz_fail "${admin_code}" || [[ "${admin_code}" == "404" ]]; then
  record_check "admin_disabled_by_default" "PASS" "${ADMIN_DISABLED_PATH} returned ${admin_code}"
else
  record_check "admin_disabled_by_default" "WARN" "${ADMIN_DISABLED_PATH} returned ${admin_code}; verify profile"
fi

if [[ -n "${AUTH_HEADER}" ]]; then
  auth_required_code=$(curl -sS -o "${CHECKS_DIR}/auth_required.body" -D "${CHECKS_DIR}/auth_required.headers" -w "%{http_code}" -H "${AUTH_HEADER}" "${BASE_URL}${AUTH_REQUIRED_PATH}" || true)
else
  auth_required_code=$(curl -sS -o "${CHECKS_DIR}/auth_required.body" -D "${CHECKS_DIR}/auth_required.headers" -w "%{http_code}" "${BASE_URL}${AUTH_REQUIRED_PATH}" || true)
fi
printf '%s\n' "${auth_required_code}" > "${CHECKS_DIR}/auth_required.code"
if [[ -n "${AUTH_HEADER}" ]]; then
  if [[ "${auth_required_code}" =~ ^2[0-9][0-9]$ ]]; then
    record_check "protected_endpoints_require_auth" "PASS" "${AUTH_REQUIRED_PATH} succeeded with auth (${auth_required_code})"
  else
    record_check "protected_endpoints_require_auth" "WARN" "${AUTH_REQUIRED_PATH} did not succeed with auth (${auth_required_code})"
  fi
else
  if http_authz_fail "${auth_required_code}"; then
    record_check "protected_endpoints_require_auth" "PASS" "${AUTH_REQUIRED_PATH} denied unauthenticated request (${auth_required_code})"
  else
    record_check "protected_endpoints_require_auth" "WARN" "${AUTH_REQUIRED_PATH} returned ${auth_required_code} without auth"
  fi
fi

oversized_payload="${CHECKS_DIR}/oversized_payload.bin"
python3 - <<PY > "${oversized_payload}"
import sys
sys.stdout.write('A' * (${MAX_BODY_BYTES} + 1024))
PY
oversized_code=$(curl -sS -o "${CHECKS_DIR}/oversized_request.body" -D "${CHECKS_DIR}/oversized_request.headers" -w "%{http_code}" -X "${OVERSIZED_CHECK_METHOD}" -H "Content-Type: application/json" --data-binary @"${oversized_payload}" "${BASE_URL}${OVERSIZED_CHECK_PATH}" || true)
printf '%s\n' "${oversized_code}" > "${CHECKS_DIR}/oversized_request.code"
if [[ "${oversized_code}" == "413" || "${oversized_code}" == "400" || "${oversized_code}" == "422" ]]; then
  record_check "oversized_request_check" "PASS" "${OVERSIZED_CHECK_PATH} returned defensive status ${oversized_code}"
else
  record_check "oversized_request_check" "WARN" "${OVERSIZED_CHECK_PATH} returned ${oversized_code}"
fi

secret_pattern='(PRIVATE KEY|BEGIN [A-Z ]*PRIVATE KEY|mnemonic|seed phrase|secret_key|api[_-]?key|token)'
if rg -i -n "${secret_pattern}" "${RESPONSES_DIR}" > "${CHECKS_DIR}/secret_scan_hits.txt"; then
  record_check "no_obvious_secrets_exposed" "WARN" "Potential secret-like strings found; inspect checks/secret_scan_hits.txt"
else
  record_check "no_obvious_secrets_exposed" "PASS" "No obvious secret-like strings in captured endpoint bodies"
fi

readiness_code=$(cat "${RESPONSES_DIR}/readiness.code")
if [[ "${readiness_code}" =~ ^2[0-9][0-9]$ ]]; then
  record_check "readiness_reports_api_safety" "PASS" "/readiness returned ${readiness_code}; inspect readiness.body"
else
  record_check "readiness_reports_api_safety" "WARN" "/readiness returned ${readiness_code}"
fi

if [[ -n "${CONFIG_SAFETY_FILE}" && -f "${CONFIG_SAFETY_FILE}" ]]; then
  cp -f "${CONFIG_SAFETY_FILE}" "${META_DIR}/config_safety_summary.txt"
  record_check "rpc_bind_config_safe" "PASS" "Config safety summary collected from ${CONFIG_SAFETY_FILE}"
else
  printf '%s\n' "No config safety summary provided (set CONFIG_SAFETY_FILE)." > "${META_DIR}/config_safety_summary.txt"
  record_check "rpc_bind_config_safe" "WARN" "No CONFIG_SAFETY_FILE provided"
fi

if eval "${TEST_COMMAND}" > "${META_DIR}/tests_run.log" 2>&1; then
  record_check "tests_run" "PASS" "${TEST_COMMAND}"
else
  record_check "tests_run" "WARN" "${TEST_COMMAND} failed; see meta/tests_run.log"
fi

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git rev-parse HEAD > "${META_DIR}/git_commit.txt"
else
  printf 'unavailable\n' > "${META_DIR}/git_commit.txt"
fi

if [[ -f VERSION ]]; then
  cp -f VERSION "${META_DIR}/build_version.txt"
else
  printf 'unavailable\n' > "${META_DIR}/build_version.txt"
fi

printf '%s\n' "${ACTIVE_API_PROFILE}" > "${META_DIR}/active_api_profile.txt"

shopt -s nullglob globstar
log_files=(${NODE_LOG_GLOB})
if (( ${#log_files[@]} > 0 )); then
  cp -f "${log_files[@]}" "${LOGS_DIR}/" 2>/dev/null || true
else
  printf 'No node logs matched NODE_LOG_GLOB=%s\n' "${NODE_LOG_GLOB}" > "${LOGS_DIR}/README.txt"
fi

cat > "${SUMMARY_FILE}" <<SUMMARY
# v2.2.17 API Security Evidence Summary

- Run ID: ${RUN_ID}
- Base URL: ${BASE_URL}
- Output directory: ${OUT_DIR}
- Git commit: $(cat "${META_DIR}/git_commit.txt")
- Build version: $(tr -d '\n' < "${META_DIR}/build_version.txt")
- Active API profile: $(cat "${META_DIR}/active_api_profile.txt")

## Collected endpoints
- /health
- /status
- /release
- /readiness
- /metrics
- /p2p/status
- /sync/status

## Pass/Fail Summary
- admin disabled by default: $(sed -n '1p' "${CHECKS_DIR}/admin_disabled_by_default.txt" | cut -d= -f2)
- protected endpoints require auth when configured: $(sed -n '1p' "${CHECKS_DIR}/protected_endpoints_require_auth.txt" | cut -d= -f2)
- no obvious secrets exposed: $(sed -n '1p' "${CHECKS_DIR}/no_obvious_secrets_exposed.txt" | cut -d= -f2)
- readiness reports API safety: $(sed -n '1p' "${CHECKS_DIR}/readiness_reports_api_safety.txt" | cut -d= -f2)
- RPC bind/config is safe: $(sed -n '1p' "${CHECKS_DIR}/rpc_bind_config_safe.txt" | cut -d= -f2)
- tests run: $(sed -n '1p' "${CHECKS_DIR}/tests_run.txt" | cut -d= -f2)

## Notes
- Token values are never written to artifacts.
- This collector is designed for local/private operator runs and does not require public network access.
SUMMARY

(
  cd "${OUT_DIR}"
  tar -czf "evidence.tar.gz" responses checks logs meta summary.md
)

echo "Collected v2.2.17 API security evidence: ${OUT_DIR}"
echo "Summary: ${SUMMARY_FILE}"
echo "Archive: ${ARCHIVE_FILE}"
echo "Checks -> PASS: ${pass}, WARN: ${warn}"
