#!/usr/bin/env bash
set -euo pipefail

NODE_URL="${NODE_URL:-http://127.0.0.1:18080}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUT_DIR="${OUT_DIR:-artifacts/v2_2_19_rpc_security_smoke/${RUN_ID}}"
PROFILE="${PROFILE:-public_safe}"
mkdir -p "${OUT_DIR}"

pass=0
fail=0

http_code() {
  local name="$1" method="$2" path="$3"
  curl -sS -X "$method" -D "${OUT_DIR}/${name}.headers" -o "${OUT_DIR}/${name}.body" -w "%{http_code}" "${NODE_URL}${path}" > "${OUT_DIR}/${name}.code" || echo "000" > "${OUT_DIR}/${name}.code"
  cat "${OUT_DIR}/${name}.code"
}

check_eq() {
  local label="$1" got="$2" expected="$3"
  if [[ "$got" == "$expected" ]]; then
    echo "PASS ${label}: ${got}" | tee -a "${OUT_DIR}/checks.log"
    pass=$((pass+1))
  else
    echo "FAIL ${label}: expected ${expected}, got ${got}" | tee -a "${OUT_DIR}/checks.log"
    fail=$((fail+1))
  fi
}

admin_default_code=$(http_code admin_default GET /admin/diagnostics)
check_eq "admin disabled default" "$admin_default_code" "403"

if [[ "$PROFILE" == "public_safe" ]]; then
  for ep in /admin/snapshot/create /admin/prune /admin/sync/rebuild /admin/diagnostics /operator/query-pack; do
    c=$(http_code "public_forbidden_${ep//\//_}" POST "$ep")
    [[ "$ep" == "/admin/diagnostics" || "$ep" == "/operator/query-pack" ]] && c=$(http_code "public_forbidden_${ep//\//_}" GET "$ep")
    check_eq "public_safe forbidden ${ep}" "$c" "404"
  done
fi

for ep in /health /status /api/v1/health /api/v1/status; do
  c=$(http_code "local_allowed_${ep//\//_}" GET "$ep")
  check_eq "local allowed ${ep}" "$c" "200"
done

secret_pattern='(PRIVATE KEY|BEGIN [A-Z ]*PRIVATE KEY|mnemonic|seed phrase|secret_key|api[_-]?key|token)'
if command -v rg >/dev/null 2>&1; then
  scanner="rg"
  rg -i -n "$secret_pattern" "${OUT_DIR}" > "${OUT_DIR}/secret_scan_hits.txt" || true
elif command -v grep >/dev/null 2>&1; then
  scanner="grep fallback"
  grep -R -E -i -n "$secret_pattern" "${OUT_DIR}" > "${OUT_DIR}/secret_scan_hits.txt" || true
else
  echo "FAIL scanner missing" | tee -a "${OUT_DIR}/checks.log"
  fail=$((fail+1))
  scanner="none"
fi

if [[ -s "${OUT_DIR}/secret_scan_hits.txt" ]]; then
  echo "FAIL secret scan" | tee -a "${OUT_DIR}/checks.log"
  fail=$((fail+1))
else
  echo "PASS secret scan" | tee -a "${OUT_DIR}/checks.log"
  pass=$((pass+1))
fi

echo "PASS=${pass} FAIL=${fail}" | tee "${OUT_DIR}/summary.txt"
[[ $fail -eq 0 ]]
