#!/usr/bin/env bash
set -euo pipefail

RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_ROOT="${ARTIFACT_ROOT:-${ROOT_DIR}/artifacts/v2_2_20_snapshot_restore}"
OUT_DIR="${OUT_DIR:-${ARTIFACT_ROOT}/${RUN_ID}}"
DATA_ROOT="${DATA_ROOT:-${ROOT_DIR}/run/v2_2_20_snapshot_restore/${RUN_ID}}"
NODE_BIN="${NODE_BIN:-${ROOT_DIR}/target/debug/pulsedagd}"
RPC_PORT="${RPC_PORT:-29220}"
RPC_BIND="127.0.0.1:${RPC_PORT}"
CHAIN_ID="${CHAIN_ID:-pulsedag-restore-drill-v2-2-20}"
HEIGHT_THRESHOLD="${HEIGHT_THRESHOLD:-3}"
POLL_SECONDS="${POLL_SECONDS:-1}"
START_TIMEOUT_SECONDS="${START_TIMEOUT_SECONDS:-60}"
MINE_MAX_TRIES="${MINE_MAX_TRIES:-1000000}"
CI_MODE="${CI_MODE:-0}"
BUILD_NODE="${BUILD_NODE:-0}"

if [[ "${1:-}" == "--ci" ]]; then
  CI_MODE=1
  HEIGHT_THRESHOLD="${HEIGHT_THRESHOLD:-2}"
  START_TIMEOUT_SECONDS="${START_TIMEOUT_SECONDS:-45}"
fi

ORIG_DB="${DATA_ROOT}/original-rocksdb"
RESTORE_DB="${DATA_ROOT}/restored-rocksdb"
ORIG_LOG="${OUT_DIR}/original-node.log"
RESTORE_LOG="${OUT_DIR}/restored-node.log"
SNAPSHOT_BUNDLE="${OUT_DIR}/snapshot_bundle.bin"
SNAPSHOT_SHA256="${OUT_DIR}/snapshot_bundle.bin.sha256"
EXPORT_REPORT="${OUT_DIR}/snapshot_export_report.json"
IMPORT_REPORT="${OUT_DIR}/snapshot_import_report.json"
ORIG_STATUS="${OUT_DIR}/original_status.json"
RESTORED_STATUS="${OUT_DIR}/restored_status.json"
RESTORE_REPORT="${OUT_DIR}/restore_report.json"
MANIFEST_JSON="${OUT_DIR}/evidence_manifest.json"
SUMMARY_MD="${OUT_DIR}/summary.md"
TARBALL="${OUT_DIR}/evidence.tar.gz"
TARBALL_SHA256="${OUT_DIR}/evidence.tar.gz.sha256"

need_cmd() { command -v "$1" >/dev/null 2>&1 || { echo "error: missing command: $1" >&2; exit 1; }; }
for c in jq curl tar sha256sum; do need_cmd "$c"; done

if [[ "${1:-}" == "--validate-snapshot-metadata" ]]; then
  file="${2:?metadata file required}"
  jq -e '
    .chain_id != null and (.chain_id|type)=="string" and (.chain_id|length)>0 and
    .schema_version != null and (.schema_version|type)=="number" and .schema_version >= 1 and
    .best_height != null and (.best_height|type)=="number" and .best_height >= 0 and
    .selected_tip != null and (.selected_tip|type)=="string" and (.selected_tip|length)>0 and
    .state_root != null and (.state_root|type)=="string" and
    .created_at != null and (.created_at|type)=="number" and .created_at > 0
  ' "$file" >/dev/null
  exit 0
fi

if [[ "${1:-}" == "--compare-summaries" ]]; then
  a="${2:?original summary required}"
  b="${3:?restored summary required}"
  jq -n --argjson a "$(cat "$a")" --argjson b "$(cat "$b")" '{chain_id_match:($a.chain_id==$b.chain_id),best_height_match:($a.best_height==$b.best_height),selected_tip_match:($a.selected_tip==$b.selected_tip),block_count_match:($a.block_count==$b.block_count),snapshot_height_match:($a.snapshot_height==$b.snapshot_height),snapshot_checksum_present:(($a.snapshot_sha256 // "") != "" and ($a.snapshot_sha256 == ($b.snapshot_sha256 // $a.snapshot_sha256)))}'
  exit 0
fi

if [[ "$BUILD_NODE" == "1" || ! -x "$NODE_BIN" ]]; then
  (cd "$ROOT_DIR" && cargo build -p pulsedagd --locked)
fi
[[ -x "$NODE_BIN" ]] || { echo "error: node binary not executable: ${NODE_BIN}" >&2; exit 1; }

rm -rf "$OUT_DIR" "$DATA_ROOT"
mkdir -p "$OUT_DIR" "$ORIG_DB" "$RESTORE_DB"

NODE_PID=""
RESTORED_PID=""
cleanup() {
  [[ -n "$NODE_PID" ]] && kill "$NODE_PID" >/dev/null 2>&1 || true
  [[ -n "$RESTORED_PID" ]] && kill "$RESTORED_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT

rpc_raw() { curl -fsS "http://${RPC_BIND}/$1"; }
rpc_data() { rpc_raw "$1" | jq -e '.data'; }
rpc_post() { curl -fsS -X POST -H 'content-type: application/json' --data "${2:-{}}" "http://${RPC_BIND}/$1" | jq -e '.ok == true' >/dev/null; }

wait_ready() {
  local deadline=$(( $(date +%s) + START_TIMEOUT_SECONDS ))
  while (( $(date +%s) <= deadline )); do
    if rpc_raw readiness | jq -e '.ok == true' >/dev/null 2>&1; then return 0; fi
    sleep "$POLL_SECONDS"
  done
  return 1
}

status_summary() {
  local checksum="$1"
  jq -n \
    --arg snapshot_sha256 "$checksum" \
    --argjson status "$(rpc_data status)" \
    --argjson readiness "$(rpc_data readiness)" \
    --argjson snapshot "$(rpc_data snapshot)" \
    '{chain_id:$status.chain_id,best_height:$status.best_height,selected_tip:($status.selected_tip // ""),block_count:$status.block_count,snapshot_exists:$snapshot.snapshot_exists,snapshot_height:$snapshot.snapshot_height,schema_version:$snapshot.schema_version,snapshot_metadata:$snapshot.snapshot_metadata,snapshot_sha256:$snapshot_sha256,readiness:$readiness}'
}

validate_status_summary() {
  jq -e '.chain_id != "" and (.best_height|tonumber) >= 0 and .selected_tip != "" and (.block_count|tonumber) >= 1 and .snapshot_exists == true and .snapshot_height != null and .snapshot_metadata != null and .snapshot_sha256 != ""' "$1" >/dev/null
}

STARTED_NODE_PID=""
start_node() {
  local db="$1"
  local log="$2"
  PULSEDAG_CONFIG_PROFILE=dev \
  PULSEDAG_CHAIN_ID="$CHAIN_ID" \
  PULSEDAG_ROCKSDB_PATH="$db" \
  PULSEDAG_RPC_BIND="$RPC_BIND" \
  PULSEDAG_P2P_ENABLED=false \
  PULSEDAG_ADMIN_ENABLED=true \
  PULSEDAG_SNAPSHOT_AUTO_EVERY_BLOCKS=0 \
    "$NODE_BIN" >"$log" 2>&1 &
  STARTED_NODE_PID=$!
}

start_node "$ORIG_DB" "$ORIG_LOG"
NODE_PID="$STARTED_NODE_PID"
wait_ready || { echo "error: original node did not become ready" >&2; exit 1; }

while true; do
  height="$(rpc_data status | jq -r '.best_height // 0')"
  if [[ "$height" =~ ^[0-9]+$ ]] && (( height >= HEIGHT_THRESHOLD )); then
    break
  fi
  rpc_post mine "{\"miner_address\":\"restore-drill\",\"pow_max_tries\":${MINE_MAX_TRIES}}"
done

rpc_post admin/snapshot/create '{}'
status_summary "pending" > "$ORIG_STATUS"

kill "$NODE_PID" >/dev/null 2>&1 || true
wait "$NODE_PID" || true
NODE_PID=""

PULSEDAG_CONFIG_PROFILE=dev \
PULSEDAG_CHAIN_ID="$CHAIN_ID" \
PULSEDAG_ROCKSDB_PATH="$ORIG_DB" \
PULSEDAG_RPC_BIND="$RPC_BIND" \
PULSEDAG_P2P_ENABLED=false \
  "$NODE_BIN" --snapshot-export "$SNAPSHOT_BUNDLE" >"$EXPORT_REPORT"
sha256sum "$SNAPSHOT_BUNDLE" > "$SNAPSHOT_SHA256"
snapshot_digest="$(cut -d' ' -f1 "$SNAPSHOT_SHA256")"
tmp_status="${ORIG_STATUS}.tmp"
jq --arg snapshot_sha256 "$snapshot_digest" '.snapshot_sha256 = $snapshot_sha256' "$ORIG_STATUS" > "$tmp_status"
mv "$tmp_status" "$ORIG_STATUS"
validate_status_summary "$ORIG_STATUS" || { echo "error: invalid original status summary" >&2; exit 1; }
"$0" --validate-snapshot-metadata <(jq '.snapshot_metadata' "$ORIG_STATUS")

PULSEDAG_CONFIG_PROFILE=dev \
PULSEDAG_CHAIN_ID="$CHAIN_ID" \
PULSEDAG_ROCKSDB_PATH="$RESTORE_DB" \
PULSEDAG_RPC_BIND="$RPC_BIND" \
PULSEDAG_P2P_ENABLED=false \
  "$NODE_BIN" --snapshot-import "$SNAPSHOT_BUNDLE" >"$IMPORT_REPORT"

start_node "$RESTORE_DB" "$RESTORE_LOG"
RESTORED_PID="$STARTED_NODE_PID"
wait_ready || { echo "error: restored node did not become ready" >&2; exit 1; }
status_summary "$snapshot_digest" > "$RESTORED_STATUS"
validate_status_summary "$RESTORED_STATUS" || { echo "error: invalid restored status summary" >&2; exit 1; }
"$0" --compare-summaries "$ORIG_STATUS" "$RESTORED_STATUS" > "$RESTORE_REPORT"
all_ok="$(jq -r '.chain_id_match and .best_height_match and .selected_tip_match and .block_count_match and .snapshot_height_match and .snapshot_checksum_present' "$RESTORE_REPORT")"
[[ "$all_ok" == "true" ]] || { echo "error: restore comparison mismatch" >&2; exit 1; }

cat > "$SUMMARY_MD" <<MD
# v2.2.20 deterministic snapshot restore drill summary

- run_id: ${RUN_ID}
- ci_mode: ${CI_MODE}
- chain_id: ${CHAIN_ID}
- height_threshold: ${HEIGHT_THRESHOLD}
- restored height/tip gate: pass
- snapshot checksum: ${snapshot_digest}
- snapshot artifact: snapshot_bundle.bin
MD

jq -n \
  --arg run_id "$RUN_ID" \
  --arg chain_id "$CHAIN_ID" \
  --arg snapshot_sha256 "$snapshot_digest" \
  --arg ci_mode "$CI_MODE" \
  --argjson original "$(cat "$ORIG_STATUS")" \
  --argjson restored "$(cat "$RESTORED_STATUS")" \
  --argjson restore_report "$(cat "$RESTORE_REPORT")" \
  '{run_id:$run_id,chain_id:$chain_id,ci_mode:($ci_mode == "1"),snapshot_artifact:"snapshot_bundle.bin",snapshot_sha256:$snapshot_sha256,original:$original,restored:$restored,restore_report:$restore_report,files:["summary.md","snapshot_bundle.bin","snapshot_bundle.bin.sha256","snapshot_export_report.json","snapshot_import_report.json","original_status.json","restored_status.json","restore_report.json","original-node.log","restored-node.log"]}' > "$MANIFEST_JSON"

(
  cd "$OUT_DIR"
  tar -czf "$TARBALL" summary.md snapshot_bundle.bin snapshot_bundle.bin.sha256 snapshot_export_report.json snapshot_import_report.json original_status.json restored_status.json restore_report.json evidence_manifest.json original-node.log restored-node.log
  sha256sum "$(basename "$TARBALL")" > "$(basename "$TARBALL_SHA256")"
)

echo "[v2.2.20] snapshot restore drill evidence: ${OUT_DIR}"
echo "[v2.2.20] snapshot sha256: ${snapshot_digest}"
