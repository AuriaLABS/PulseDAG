#!/usr/bin/env bash
set -euo pipefail

RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

RUN_ENV="run/v2_2_18_vps_rehearsal.env"
if [[ -f "${RUN_ENV}" ]]; then
  # shellcheck disable=SC1090
  source "${RUN_ENV}"
fi

ART_DIR="${artifact_dir:-artifacts/v2_2_18_private_rc/${RUN_ID}}"
mkdir -p "${ART_DIR}/endpoints" "${ART_DIR}/meta" "${ART_DIR}/logs" "${ART_DIR}/run"

NODE_URLS="${NODE_URLS:-}"
if [[ -z "${NODE_URLS}" && -f run/v2_2_18_vps_nodes.pid ]]; then
  NODE_URLS=$(awk '{print "http://127.0.0.1:"$4}' run/v2_2_18_vps_nodes.pid | tr '\n' ' ')
fi

if [[ -z "${NODE_URLS}" ]]; then
  NODE_URLS="http://127.0.0.1:18080 http://127.0.0.1:28080 http://127.0.0.1:38080"
fi

for f in run/v2_2_18_vps_nodes.pid run/v2_2_18_vps_miners.pid run/v2_2_18_vps_rehearsal.env; do
  [[ -f "$f" ]] && cp "$f" "${ART_DIR}/run/"
done

cp -f logs/node-*.log "${ART_DIR}/logs/" 2>/dev/null || true
cp -f logs/miner-*.log "${ART_DIR}/logs/" 2>/dev/null || true

git rev-parse HEAD > "${ART_DIR}/meta/git_commit.txt" || true
cat VERSION > "${ART_DIR}/meta/VERSION.txt" || true
cargo metadata --format-version 1 --no-deps > "${ART_DIR}/meta/cargo_metadata.json" 2>/dev/null || true

capture_ep() {
  local base="$1" ep="$2" node_label="$3"
  local name="${ep#/}"
  name="${name//\//_}"
  curl -sS "${base}${ep}" > "${ART_DIR}/endpoints/${node_label}_${name}.json" || true
}

idx=0
for url in ${NODE_URLS}; do
  idx=$((idx+1))
  node_label="node${idx}"
  for ep in /health /status /release /readiness /p2p/status /sync/status; do
    capture_ep "${url}" "${ep}" "${node_label}"
  done
done

cat > "${ART_DIR}/summary.md" <<SUM
# Ubuntu/VPS rehearsal evidence summary (v2.2.18)
- artifacts_dir: ${ART_DIR}
- node_urls: ${NODE_URLS}
- captured_endpoints: /health /status /release /readiness /p2p/status /sync/status
- includes: logs, run metadata, git commit, VERSION, cargo metadata (if available)
SUM

echo "Evidence collected at: ${ART_DIR}"
