#!/usr/bin/env bash
set -euo pipefail

RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

RUN_ENV="${RUN_ENV:-run/v2_2_18_vps_rehearsal.env}"
if [[ -f "${RUN_ENV}" ]]; then
  # shellcheck disable=SC1090
  source "${RUN_ENV}"
fi

ART_DIR="${ART_DIR:-${artifact_dir:-artifacts/v2_2_18_private_rc/${RUN_ID}}}"
mkdir -p "${ART_DIR}"/{endpoints,meta,logs,run,summaries}

UTC_NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
SUMMARY_FILE="${ART_DIR}/summary.md"
CHECKLIST_FILE="${ART_DIR}/CHECKLIST.md"

# Endpoint allowlist only; never add auth-bearing config captures.
PRIMARY_ENDPOINTS=(
  /health /status /release /readiness /metrics
  /p2p/status /p2p/peers
  /sync/status
)
OPTIONAL_ENDPOINTS=(
  /sync/missing /dag/consistency /pow/metrics
  /mining/worker/stats /snapshot/metadata
)

sanitize_file() {
  local file="$1"
  [[ -f "$file" ]] || return 0
  sed -E \
    -e 's/(Authorization:)[[:space:]]*[^[:space:]]+/\1 [REDACTED]/Ig' \
    -e 's/("?(token|access_token|refresh_token|api_key|apikey|secret|password|passphrase)"?[[:space:]]*[:=][[:space:]]*")([^"]+)(")/\1[REDACTED]\4/Ig' \
    -e 's/([?&](token|access_token|refresh_token|api_key|apikey|secret|password)=[^&[:space:]]+)/\2=[REDACTED]/Ig' \
    "$file" > "${file}.redacted"
  mv "${file}.redacted" "$file"
}

STATUS_ROWS=()

record_status() {
  local item="$1" status="$2" details="$3"
  STATUS_ROWS+=("| ${item} | ${status} | ${details} |")
}

capture_text_cmd() {
  local name="$1"
  shift
  if "$@" > "${ART_DIR}/meta/${name}" 2>&1; then
    sanitize_file "${ART_DIR}/meta/${name}"
    record_status "${name}" "PASS" "captured"
  else
    sanitize_file "${ART_DIR}/meta/${name}"
    record_status "${name}" "PENDING" "command failed; inspect meta/${name}"
  fi
}

capture_file_if_exists() {
  local src="$1" dst_rel="$2" required="$3"
  local dst="${ART_DIR}/${dst_rel}"
  if [[ -f "$src" ]]; then
    mkdir -p "$(dirname "$dst")"
    cp -f "$src" "$dst"
    sanitize_file "$dst"
    record_status "$dst_rel" "PASS" "copied"
  else
    if [[ "$required" == "required" ]]; then
      record_status "$dst_rel" "PENDING" "missing source: $src"
    else
      record_status "$dst_rel" "SKIP" "optional source missing: $src"
    fi
  fi
}

NODE_URLS="${NODE_URLS:-}"
if [[ -z "${NODE_URLS}" && -f run/v2_2_18_vps_nodes.pid ]]; then
  NODE_URLS="$(awk '{print "http://127.0.0.1:"$4}' run/v2_2_18_vps_nodes.pid | tr '\n' ' ')"
fi
if [[ -z "${NODE_URLS}" ]]; then
  NODE_URLS="http://127.0.0.1:18080 http://127.0.0.1:28080 http://127.0.0.1:38080"
fi

capture_endpoint() {
  local base="$1" ep="$2" node_label="$3" optional="$4"
  local name="${ep#/}"
  name="${name//\//_}"
  local out="${ART_DIR}/endpoints/${node_label}_${name}.txt"
  local code
  code="$(curl -sS -m 8 -o "$out" -w '%{http_code}' "${base}${ep}" || true)"
  sanitize_file "$out"

  if [[ "$code" =~ ^2[0-9][0-9]$ ]]; then
    record_status "endpoint ${node_label} ${ep}" "PASS" "HTTP ${code}"
  elif [[ "$optional" == "optional" ]]; then
    record_status "endpoint ${node_label} ${ep}" "SKIP" "HTTP ${code:-ERR}"
  else
    record_status "endpoint ${node_label} ${ep}" "PENDING" "HTTP ${code:-ERR}"
  fi
}

# Core metadata captures
capture_text_cmd git_commit.txt git rev-parse HEAD
capture_file_if_exists VERSION meta/VERSION.txt required
capture_text_cmd cargo_workspace_version.txt bash -lc "cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name==\"pulsedag\") | .version'"
capture_text_cmd cargo_metadata.json cargo metadata --format-version 1 --no-deps
capture_text_cmd build_info.txt bash -lc "cargo --version && rustc --version && uname -a"

capture_file_if_exists run/v2_2_18_topology_manifest.json meta/topology_manifest.json required
capture_file_if_exists run/v2_2_18_timeline.md meta/timeline.md required
capture_file_if_exists timeline.md meta/timeline_repo_fallback.md optional

for f in run/v2_2_18_vps_nodes.pid run/v2_2_18_vps_miners.pid run/v2_2_18_vps_rehearsal.env; do
  capture_file_if_exists "$f" "run/$(basename "$f")" optional
done

cp -f logs/node-*.log "${ART_DIR}/logs/" 2>/dev/null || true
cp -f logs/miner-*.log "${ART_DIR}/logs/" 2>/dev/null || true
for f in "${ART_DIR}"/logs/*.log; do
  [[ -e "$f" ]] || continue
  sanitize_file "$f"
done
record_status "logs/node-*.log" "PASS" "copied if present"
record_status "logs/miner-*.log" "PASS" "copied if present"

# Endpoint captures
idx=0
for url in ${NODE_URLS}; do
  idx=$((idx + 1))
  node_label="node${idx}"
  for ep in "${PRIMARY_ENDPOINTS[@]}"; do
    capture_endpoint "$url" "$ep" "$node_label" required
  done
  for ep in "${OPTIONAL_ENDPOINTS[@]}"; do
    capture_endpoint "$url" "$ep" "$node_label" optional
  done
done

# Summary files for RC topics
capture_file_if_exists run/v2_2_18_perturbation_summary.md summaries/perturbation_summary.md optional
capture_file_if_exists run/v2_2_18_sync_convergence_summary.md summaries/sync_convergence_summary.md optional
capture_file_if_exists run/v2_2_18_miner_telemetry_summary.md summaries/miner_telemetry_summary.md optional

cat > "${SUMMARY_FILE}" <<SUM
# v2.2.18 private-testnet RC evidence summary
- run_id: ${RUN_ID}
- collected_at_utc: ${UTC_NOW}
- artifact_dir: ${ART_DIR}
- node_urls: ${NODE_URLS}
- bundle: evidence.tar.gz
- policy: missing optional captures are SKIP; missing required captures are PENDING.
SUM

{
  echo "# v2.2.18 private-testnet RC evidence checklist"
  echo
  echo "- run_id: ${RUN_ID}"
  echo "- collected_at_utc: ${UTC_NOW}"
  echo
  echo "| Item | Status | Details |"
  echo "|---|---|---|"
  printf '%s\n' "${STATUS_ROWS[@]}"
} > "${CHECKLIST_FILE}"

tar -czf "${ART_DIR}/evidence.tar.gz" -C "${ART_DIR}" \
  endpoints meta logs run summaries summary.md CHECKLIST.md
record_status "evidence.tar.gz" "PASS" "bundle generated"

echo "Evidence collected at: ${ART_DIR}"
echo "Checklist: ${CHECKLIST_FILE}"
