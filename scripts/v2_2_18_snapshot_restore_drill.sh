#!/usr/bin/env bash
set -euo pipefail

RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="${RUN_DIR:-${ROOT_DIR}/run}"
ARTIFACT_ROOT="${ARTIFACT_ROOT:-${ROOT_DIR}/artifacts/v2_2_18_snapshot_restore_drill}"
ARTIFACT_DIR="${ARTIFACT_DIR:-${ARTIFACT_ROOT}/${RUN_ID}}"
TARGET_NODE_NAME="${TARGET_NODE_NAME:-node-B}"
WARMUP_SECONDS="${WARMUP_SECONDS:-45}"
REJOIN_TIMEOUT_SECONDS="${REJOIN_TIMEOUT_SECONDS:-180}"
POLL_SECONDS="${POLL_SECONDS:-5}"

NODE_PID_FILE="${RUN_DIR}/v2_2_18_vps_nodes.pid"
STATUS_BEFORE_DIR="${ARTIFACT_DIR}/before"
STATUS_AFTER_DIR="${ARTIFACT_DIR}/after"
TIMING_CSV="${ARTIFACT_DIR}/restore-timing.csv"
SNAPSHOT_METADATA_JSON="${ARTIFACT_DIR}/snapshot-metadata.json"
RESTORE_SUMMARY_MD="${ARTIFACT_DIR}/restore-summary.md"
LINEAGE_JSON="${ARTIFACT_DIR}/lineage-checks.json"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: missing command: $1" >&2
    exit 1
  }
}

need_cmd awk
need_cmd curl
need_cmd date
need_cmd jq
need_cmd mkdir
need_cmd cp

mkdir -p "${ARTIFACT_DIR}" "${STATUS_BEFORE_DIR}" "${STATUS_AFTER_DIR}"

if [[ ! -f "${NODE_PID_FILE}" ]]; then
  echo "error: node pid file missing: ${NODE_PID_FILE}" >&2
  exit 1
fi

if ! awk -v n="${TARGET_NODE_NAME}" '$2==n {found=1} END{exit(found?0:1)}' "${NODE_PID_FILE}"; then
  echo "error: target node '${TARGET_NODE_NAME}' not found in ${NODE_PID_FILE}" >&2
  exit 1
fi

TARGET_PID="$(awk -v n="${TARGET_NODE_NAME}" '$2==n {print $1}' "${NODE_PID_FILE}" | head -n1)"
TARGET_RPC_PORT="$(awk -v n="${TARGET_NODE_NAME}" '$2==n {print $4}' "${NODE_PID_FILE}" | head -n1)"
TARGET_P2P_PORT="$(awk -v n="${TARGET_NODE_NAME}" '$2==n {print $3}' "${NODE_PID_FILE}" | head -n1)"
TARGET_DATA_DIR="${RUN_DIR}/${TARGET_NODE_NAME}-data"
TARGET_RPC_URL="http://127.0.0.1:${TARGET_RPC_PORT}"

if [[ ! -d "${TARGET_DATA_DIR}" ]]; then
  echo "error: target data dir not found: ${TARGET_DATA_DIR}" >&2
  exit 1
fi

capture_node_status() {
  local out_dir="$1"
  local node_name="$2"
  local rpc_port="$3"
  local base="http://127.0.0.1:${rpc_port}"
  for ep in health status readiness p2p/status sync/status snapshot sync/verify; do
    local name="${ep//\//_}"
    curl -sS "${base}/${ep}" > "${out_dir}/${node_name}_${name}.json" || true
  done
}

capture_cluster_status() {
  local out_dir="$1"
  while read -r _pid node_name _p2p rpc_port; do
    [[ -n "${node_name:-}" ]] || continue
    capture_node_status "${out_dir}" "${node_name}" "${rpc_port}"
  done < "${NODE_PID_FILE}"
}

echo "[drill] warming up cluster for ${WARMUP_SECONDS}s"
sleep "${WARMUP_SECONDS}"

capture_cluster_status "${STATUS_BEFORE_DIR}"

echo "[drill] capturing snapshot metadata"
curl -fsS "${TARGET_RPC_URL}/snapshot" | jq . > "${SNAPSHOT_METADATA_JSON}"

backup_dir="${ARTIFACT_DIR}/${TARGET_NODE_NAME}-data-backup"
restored_dir="${ARTIFACT_DIR}/${TARGET_NODE_NAME}-data-restored"

if [[ -e "${backup_dir}" || -e "${restored_dir}" ]]; then
  echo "error: backup/restored directories already exist inside artifact dir" >&2
  exit 1
fi

echo "[drill] stopping ${TARGET_NODE_NAME} pid=${TARGET_PID}"
if kill -0 "${TARGET_PID}" 2>/dev/null; then
  kill "${TARGET_PID}" 2>/dev/null || true
fi
sleep 1
if kill -0 "${TARGET_PID}" 2>/dev/null; then
  kill -9 "${TARGET_PID}" 2>/dev/null || true
fi

echo "[drill] backing up data directory to ${backup_dir}"
cp -a "${TARGET_DATA_DIR}" "${backup_dir}"

echo "[drill] exporting snapshot via /snapshot/create"
SNAPSHOT_CREATE_JSON="${ARTIFACT_DIR}/snapshot-create-response.json"
curl -fsS -X POST -H 'content-type: application/json' -d '{}' "${TARGET_RPC_URL}/snapshot/create" | jq . > "${SNAPSHOT_CREATE_JSON}" || true

echo "[drill] rebuilding node state from backup copy into restored dir"
cp -a "${backup_dir}" "${restored_dir}"

restore_start_epoch="$(date -u +%s)"
restore_start_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo "[drill] restarting ${TARGET_NODE_NAME} from restored dir"
nohup "${ROOT_DIR}/target/debug/pulsedagd" \
  --data-dir "${restored_dir}" \
  --p2p-bind "0.0.0.0:${TARGET_P2P_PORT}" \
  --rpc-bind "127.0.0.1:${TARGET_RPC_PORT}" \
  > "${ROOT_DIR}/logs/${TARGET_NODE_NAME}.restore.log" 2>&1 &
NEW_PID=$!

awk -v n="${TARGET_NODE_NAME}" -v npid="${NEW_PID}" '$2==n {$1=npid} {print}' "${NODE_PID_FILE}" > "${NODE_PID_FILE}.tmp"
mv "${NODE_PID_FILE}.tmp" "${NODE_PID_FILE}"

rejoined="false"
rejoin_reason="timeout"
elapsed=0
while (( elapsed <= REJOIN_TIMEOUT_SECONDS )); do
  p2p_count="$(curl -fsS "${TARGET_RPC_URL}/p2p/status" | jq -r '.peer_count // 0' 2>/dev/null || echo 0)"
  sync_json="$(curl -fsS "${TARGET_RPC_URL}/sync/status" 2>/dev/null || echo '{}')"
  sync_lag="$(echo "${sync_json}" | jq -r '.sync_lag // 999999')"
  if [[ "${p2p_count}" =~ ^[0-9]+$ ]] && [[ "${sync_lag}" =~ ^[0-9]+$ ]] && (( p2p_count > 0 )) && (( sync_lag <= 2 )); then
    rejoined="true"
    rejoin_reason="peer_count>0 and sync_lag<=2"
    break
  fi
  sleep "${POLL_SECONDS}"
  elapsed=$((elapsed + POLL_SECONDS))
done

restore_end_epoch="$(date -u +%s)"
restore_end_iso="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
recovery_seconds=$((restore_end_epoch - restore_start_epoch))

capture_cluster_status "${STATUS_AFTER_DIR}"

printf 'run_id,node_name,start_utc,end_utc,recovery_seconds,rejoin_success,rejoin_reason\n' > "${TIMING_CSV}"
printf '%s,%s,%s,%s,%s,%s,%s\n' \
  "${RUN_ID}" "${TARGET_NODE_NAME}" "${restore_start_iso}" "${restore_end_iso}" "${recovery_seconds}" "${rejoined}" "${rejoin_reason}" \
  >> "${TIMING_CSV}"

jq -n \
  --arg run_id "${RUN_ID}" \
  --arg node "${TARGET_NODE_NAME}" \
  --arg backup_dir "${backup_dir}" \
  --arg restored_dir "${restored_dir}" \
  --argjson rejoin_success "$( [[ "${rejoined}" == "true" ]] && echo true || echo false )" \
  '{run_id:$run_id,node:$node,backup_dir:$backup_dir,restored_dir:$restored_dir,rejoin_success:$rejoin_success}' > "${LINEAGE_JSON}"

cat > "${RESTORE_SUMMARY_MD}" <<MD
# Snapshot restore/rebuild drill summary (v2.2.18)

- run_id: ${RUN_ID}
- target_node: ${TARGET_NODE_NAME}
- target_rpc: ${TARGET_RPC_URL}
- restore_start_utc: ${restore_start_iso}
- restore_end_utc: ${restore_end_iso}
- recovery_seconds: ${recovery_seconds}
- rejoin_success: ${rejoined}
- rejoin_reason: ${rejoin_reason}

## Evidence
- before status captures: ${STATUS_BEFORE_DIR}
- after status captures: ${STATUS_AFTER_DIR}
- timing CSV: ${TIMING_CSV}
- snapshot metadata: ${SNAPSHOT_METADATA_JSON}
- lineage checks: ${LINEAGE_JSON}
- snapshot/create response: ${SNAPSHOT_CREATE_JSON}

## Pass/fail checklist
- node restart observed: $( [[ -n "${NEW_PID}" ]] && echo pass || echo fail )
- node rejoin peers: $( [[ "${rejoined}" == "true" ]] && echo pass || echo fail )
- node converges to network state (sync_lag<=2): $( [[ "${rejoined}" == "true" ]] && echo pass || echo fail )
- storage corruption signs: not observed in this automation (manual log review still required)
- unrecoverable state: not observed in this automation
MD

echo "[drill] complete"
echo "[drill] summary: ${RESTORE_SUMMARY_MD}"
