#!/usr/bin/env bash
set -euo pipefail

RPC_URL="${RPC_URL:-http://127.0.0.1:8080}"
KEEP_RECENT_BLOCKS="${KEEP_RECENT_BLOCKS:-64}"
DO_PRUNE="${DO_PRUNE:-0}"
DO_REBUILD="${DO_REBUILD:-0}"

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

echo "[restore-drill] rpc=$RPC_URL keep_recent_blocks=$KEEP_RECENT_BLOCKS do_prune=$DO_PRUNE do_rebuild=$DO_REBUILD"

echo "[restore-drill] baseline: /status"
status_json="$(get_json /status)"
echo "$status_json" | jq '{ok:true,best_height,best_hash,chain_id}'

echo "[restore-drill] baseline: /snapshot"
snapshot_json="$(get_json /snapshot)"
echo "$snapshot_json" | jq '{snapshot_exists,snapshot_height,recommended_keep_from_height,snapshot_captured_at_unix}'

snapshot_exists="$(echo "$snapshot_json" | jq -r '.snapshot_exists // false')"
if [[ "$snapshot_exists" != "true" ]]; then
  echo "error: snapshot does not exist; create one first or run /snapshot/create" >&2
  exit 1
fi

echo "[restore-drill] baseline: /sync/replay-plan"
get_json /sync/replay-plan | jq '{mode,from_height,to_height,reason}'

echo "[restore-drill] baseline: /sync/rebuild-preview"
get_json /sync/rebuild-preview | jq '{eligible,requires_force,warnings}'

echo "[restore-drill] creating fresh snapshot"
post_json /snapshot/create '{}' | jq .

if [[ "$DO_PRUNE" == "1" ]]; then
  echo "[restore-drill] pruning with keep_recent_blocks=$KEEP_RECENT_BLOCKS"
  post_json /prune "{\"keep_recent_blocks\":$KEEP_RECENT_BLOCKS}" | jq .
else
  echo "[restore-drill] prune skipped (set DO_PRUNE=1 to enable)"
fi

if [[ "$DO_REBUILD" == "1" ]]; then
  echo "[restore-drill] running rebuild"
  post_json /sync/rebuild '{"force":true,"allow_partial_replay":false,"persist_after_rebuild":true,"reconcile_mempool":true}' | jq .
else
  echo "[restore-drill] rebuild skipped (set DO_REBUILD=1 to enable)"
fi

echo "[restore-drill] verification: /sync/verify"
get_json /sync/verify | jq .

echo "[restore-drill] verification: /readiness"
get_json /readiness | jq .

echo "[restore-drill] verification: /status"
get_json /status | jq '{best_height,best_hash,chain_id}'

echo "[restore-drill] complete"
