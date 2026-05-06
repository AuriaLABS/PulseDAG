#!/usr/bin/env bash
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_11_common.sh"

WAIT_SECS="${PULSEDAG_REHEARSAL_WAIT_SECS:-120}"
MINE_WAIT_SECS="${PULSEDAG_REHEARSAL_MINE_WAIT_SECS:-240}"
SYNC_WAIT_SECS="${PULSEDAG_REHEARSAL_SYNC_WAIT_SECS:-180}"
KEEP_RUNNING="${PULSEDAG_REHEARSAL_KEEP_RUNNING:-0}"

cleanup() {
  if [[ "$KEEP_RUNNING" == "1" || "$KEEP_RUNNING" == "true" ]]; then
    echo "[info] leaving rehearsal processes running because PULSEDAG_REHEARSAL_KEEP_RUNNING=$KEEP_RUNNING"
  else
    stop_all_v2_2_11
  fi
}
trap cleanup EXIT

fail() { echo "[error] $*" >&2; exit 1; }

wait_for_http_ok() {
  local node="$1" endpoint="$2" deadline=$((SECONDS + WAIT_SECS)) url
  url="$(rpc_url "$node")$endpoint"
  until http_get "$url" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then
      echo "[debug] last log for node-$node:" >&2
      tail -80 "$(node_log_file "$node")" >&2 || true
      fail "timed out waiting for $url"
    fi
    sleep 2
  done
  echo "[ok] node-$node $endpoint reachable"
}

wait_for_peer_observation() {
  local node="$1" min_count="$2" deadline=$((SECONDS + WAIT_SECS)) observed connected
  until observed="$(node_peer_observation_count "$node" 2>/dev/null)" && [[ "$observed" =~ ^[0-9]+$ ]] && (( observed >= min_count )); do
    if (( SECONDS >= deadline )); then
      http_get "$(rpc_url "$node")/p2p/status" || true
      fail "node-$node did not observe real peer connectivity >= $min_count"
    fi
    sleep 2
  done
  connected="$(node_peer_count "$node" 2>/dev/null || echo unknown)"
  echo "[ok] node-$node observed peer connectivity (connected_now=$connected observed=$observed)"
}

wait_for_height_gt() {
  local node="$1" min_height="$2" timeout="$3" deadline=$((SECONDS + timeout)) height
  until height="$(node_height "$node" 2>/dev/null)" && [[ "$height" =~ ^[0-9]+$ ]] && (( height > min_height )); do
    if (( SECONDS >= deadline )); then
      fail "node-$node height did not exceed $min_height (last=${height:-unknown})"
    fi
    sleep 3
  done
  echo "$height"
}

wait_for_height_at_least() {
  local node="$1" expected="$2" timeout="$3" deadline=$((SECONDS + timeout)) height
  until height="$(node_height "$node" 2>/dev/null)" && [[ "$height" =~ ^[0-9]+$ ]] && (( height >= expected )); do
    if (( SECONDS >= deadline )); then
      fail "node-$node height did not reach $expected (last=${height:-unknown})"
    fi
    sleep 3
  done
  echo "$height"
}

print_status_summary() {
  echo
  echo "== Final v2.2.11 P2P rehearsal status =="
  for node in a b c; do
    local height peers mode rpc log
    rpc="$(rpc_url "$node")"
    height="$(node_height "$node" 2>/dev/null || echo unknown)"
    peers="$(node_peer_count "$node" 2>/dev/null || echo unknown)"
    mode="$(node_p2p_mode "$node" 2>/dev/null || echo unknown)"
    log="$(node_log_file "$node")"
    echo "node-$node rpc=$rpc height=$height peers=$peers p2p_mode=$mode log=$log"
  done
  echo "miner-a log=$(miner_log_file)"
}

command -v cargo >/dev/null || fail "cargo is required"
command -v curl >/dev/null || fail "curl is required"
command -v python3 >/dev/null || fail "python3 is required for JSON validation"

cd "$ROOT_DIR"
echo "[info] building workspace release binaries"
cargo build --workspace --release

export PULSEDAG_CLEAN_DATA=1
stop_all_v2_2_11
start_node a
wait_for_http_ok a /health
wait_for_http_ok a /p2p/status
mode="$(node_p2p_mode a)"
[[ "$mode" == "libp2p-real" ]] || fail "node-a expected p2p mode libp2p-real, got $mode"
echo "[ok] node-a reports p2p mode $mode"

bootnode_a="$(node_bootnode_a)"
echo "[info] using node-a bootnode $bootnode_a"
start_node b --bootnode "$bootnode_a"
start_node c --bootnode "$bootnode_a"
unset PULSEDAG_CLEAN_DATA

for node in b c; do
  wait_for_http_ok "$node" /health
  wait_for_http_ok "$node" /p2p/status
  mode="$(node_p2p_mode "$node")"
  [[ "$mode" == "libp2p-real" ]] || fail "node-$node expected p2p mode libp2p-real, got $mode"
  echo "[ok] node-$node reports p2p mode $mode"
done

wait_for_peer_observation a 1
wait_for_peer_observation b 1
wait_for_peer_observation c 1

start_miner_a
height_a="$(wait_for_height_gt a 0 "$MINE_WAIT_SECS")"
echo "[ok] node-a mined/saw height $height_a"
height_b="$(wait_for_height_at_least b "$height_a" "$SYNC_WAIT_SECS")"
height_c="$(wait_for_height_at_least c "$height_a" "$SYNC_WAIT_SECS")"
echo "[ok] node-b height=$height_b and node-c height=$height_c converged to node-a height=$height_a"

echo "[info] restarting node-b to verify catch-up"
stop_node b
start_node b --bootnode "$bootnode_a"
wait_for_http_ok b /health
wait_for_http_ok b /p2p/status
wait_for_peer_observation b 1
height_after_restart="$(wait_for_height_at_least b "$height_a" "$SYNC_WAIT_SECS")"
echo "[ok] node-b caught up after restart at height=$height_after_restart"

print_status_summary
