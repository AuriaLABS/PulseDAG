#!/usr/bin/env bash
set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:8080}"
KEEP_RECENT_BLOCKS="${KEEP_RECENT_BLOCKS:-64}"
APPLY_PRUNE="${APPLY_PRUNE:-0}"

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: required command not found: $1" >&2
    exit 1
  }
}

need_cmd curl
need_cmd jq

get_json() {
  local path="$1"
  curl -fsS "$RPC_URL$path"
}

post_json() {
  local path="$1"
  local body="$2"
  curl -fsS -X POST -H 'content-type: application/json' -d "$body" "$RPC_URL$path"
}

echo "[prune-safety] rpc=$RPC_URL keep_recent_blocks=$KEEP_RECENT_BLOCKS apply_prune=$APPLY_PRUNE"

snapshot_json="$(get_json /snapshot)"
status_json="$(get_json /status)"

snapshot_exists="$(echo "$snapshot_json" | jq -r '.snapshot_exists // false')"
if [[ "$snapshot_exists" != "true" ]]; then
  echo "error: snapshot is required before pruning" >&2
  exit 1
fi

snapshot_height="$(echo "$snapshot_json" | jq -r '.snapshot_height // 0')"
recommended_keep_from="$(echo "$snapshot_json" | jq -r '.recommended_keep_from_height // 0')"
best_height="$(echo "$status_json" | jq -r '.best_height // 0')"

if (( snapshot_height < recommended_keep_from )); then
  echo "error: snapshot_height ($snapshot_height) is below recommended_keep_from_height ($recommended_keep_from)" >&2
  exit 1
fi

echo "[prune-safety] precheck summary"
echo "$snapshot_json" | jq '{snapshot_exists,snapshot_height,recommended_keep_from_height,snapshot_captured_at_unix}'
echo "$status_json" | jq '{best_height,best_hash,chain_id}'

echo "[prune-safety] requesting replay plan preview"
get_json /sync/replay-plan | jq '{mode,from_height,to_height,reason}'

if [[ "$APPLY_PRUNE" == "1" ]]; then
  echo "[prune-safety] applying prune"
  post_json /prune "{\"keep_recent_blocks\":$KEEP_RECENT_BLOCKS}" | jq .
  echo "[prune-safety] post-prune verify"
  get_json /sync/verify | jq .
else
  echo "[prune-safety] dry-run only; set APPLY_PRUNE=1 to execute prune"
fi

echo "[prune-safety] complete"
