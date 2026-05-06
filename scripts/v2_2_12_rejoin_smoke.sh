#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_12_common.sh"

WAIT_SECS="${PULSEDAG_REJOIN_WAIT_SECS:-${PULSEDAG_REHEARSAL_WAIT_SECS:-120}}"
MINE_WAIT_SECS="${PULSEDAG_REJOIN_MINE_WAIT_SECS:-${PULSEDAG_REHEARSAL_MINE_WAIT_SECS:-240}}"
SYNC_WAIT_SECS="${PULSEDAG_REJOIN_SYNC_WAIT_SECS:-${PULSEDAG_REHEARSAL_SYNC_WAIT_SECS:-180}}"
COLLECT_DIR="${PULSEDAG_REJOIN_COLLECT_DIR:-$STATE_DIR/rejoin-evidence}"
KEEP_RUNNING="${PULSEDAG_REHEARSAL_KEEP_RUNNING:-0}"
NODES=(a b c)
REQUIRED_ENDPOINTS=(/health /status /p2p/status /sync/status)
DIAGNOSTIC_ENDPOINTS=(/sync/missing /orphans /p2p/peers /p2p/topology)

cleanup() {
  if [[ "$KEEP_RUNNING" == "1" || "$KEEP_RUNNING" == "true" ]]; then
    echo "[info] leaving rehearsal processes running because PULSEDAG_REHEARSAL_KEEP_RUNNING=$KEEP_RUNNING"
  else
    stop_all_v2_2_12
  fi
}
trap cleanup EXIT

fail() { echo "[error] $*" >&2; exit 1; }

json_data_field() {
  python3 -c 'import json,sys
obj=json.load(sys.stdin)
cur=obj.get("data", {})
for part in sys.argv[1].split("."):
    if not part:
        continue
    if isinstance(cur, list):
        cur=cur[int(part)]
    elif isinstance(cur, dict):
        cur=cur.get(part)
    else:
        cur=None
    if cur is None:
        print("")
        sys.exit(0)
print(cur if not isinstance(cur, (dict, list)) else json.dumps(cur, sort_keys=True))' "$1"
}

node_status_height() { http_get "$(rpc_url "$1")/status" | json_data_field "best_height"; }
node_status_chain_id() { http_get "$(rpc_url "$1")/status" | json_data_field "chain_id"; }
sync_value() { http_get "$(rpc_url "$1")/sync/status" | json_data_field "$2"; }

node_bootnode_with_peer() {
  local node="$1" addr peer_id
  addr="$(node_p2p "$node" | sed 's#/ip4/0\.0\.0\.0/#/ip4/127.0.0.1/#')"
  peer_id="$(node_peer_id "$node")"
  [[ -n "$peer_id" ]] || fail "node-$node did not expose a peer_id for bootnode dialing"
  echo "$addr/p2p/$peer_id"
}

endpoint_slug() {
  local endpoint="$1"
  echo "${endpoint#/}" | tr '/' '_'
}

collect_endpoint() {
  local node="$1" endpoint="$2" out_dir="$3" required="$4"
  local slug out err url
  slug="$(endpoint_slug "$endpoint")"
  out="$out_dir/node-$node/$slug.json"
  err="$out_dir/node-$node/$slug.stderr"
  url="$(rpc_url "$node")$endpoint"
  mkdir -p "$(dirname "$out")"

  if curl -fsS -m "$CURL_TIMEOUT_SECS" "$url" -o "$out" 2>"$err"; then
    rm -f "$err"
    echo "[ok] collected node-$node $endpoint -> $out"
  else
    local rc=$?
    rm -f "$out"
    if [[ "$required" == "1" ]]; then
      echo "[error] failed to collect required node-$node $endpoint (curl rc=$rc, stderr=$err)" >&2
      return 1
    fi
    echo "[warn] optional diagnostic node-$node $endpoint unavailable (curl rc=$rc, stderr=$err)" >&2
  fi
}

collect_all_nodes() {
  local label="$1"
  local out_dir="$COLLECT_DIR/$label"
  echo "[info] collecting $label evidence under $out_dir"
  mkdir -p "$out_dir"
  for node in "${NODES[@]}"; do
    for endpoint in "${REQUIRED_ENDPOINTS[@]}"; do
      collect_endpoint "$node" "$endpoint" "$out_dir" 1
    done
  done
}

collect_orphan_diagnostics() {
  local label="$1"
  local node="$2"
  local out_dir="$COLLECT_DIR/$label-orphan-diagnostics"
  echo "[warn] collecting orphan diagnostics for node-$node under $out_dir"
  for endpoint in "${REQUIRED_ENDPOINTS[@]}" "${DIAGNOSTIC_ENDPOINTS[@]}"; do
    collect_endpoint "$node" "$endpoint" "$out_dir" 0 || true
  done
  tail -120 "$(node_log_file "$node")" > "$out_dir/node-$node/recent.log" 2>/dev/null || true
  echo "[warn] node-$node orphan_count remained non-zero; diagnostics were saved for review under $out_dir"
}

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

wait_for_current_peer_count() {
  local node="$1" min_count="$2" deadline=$((SECONDS + WAIT_SECS)) connected
  until connected="$(node_peer_count "$node" 2>/dev/null)" && [[ "$connected" =~ ^[0-9]+$ ]] && (( connected >= min_count )); do
    if (( SECONDS >= deadline )); then
      http_get "$(rpc_url "$node")/p2p/status" || true
      fail "node-$node current connected peer count did not reach $min_count"
    fi
    sleep 2
  done
  echo "[ok] node-$node has current connected peer count $connected"
}

wait_for_height_gt() {
  local node="$1" min_height="$2" timeout="$3" height
  local deadline=$((SECONDS + timeout))
  until height="$(node_status_height "$node" 2>/dev/null)" && [[ "$height" =~ ^[0-9]+$ ]] && (( height > min_height )); do
    if (( SECONDS >= deadline )); then
      fail "node-$node best_height did not exceed $min_height (last=${height:-unknown})"
    fi
    sleep 3
  done
  echo "$height"
}

wait_for_height_at_least() {
  local node="$1" expected="$2" timeout="$3" height
  local deadline=$((SECONDS + timeout))
  until height="$(node_status_height "$node" 2>/dev/null)" && [[ "$height" =~ ^[0-9]+$ ]] && (( height >= expected )); do
    if (( SECONDS >= deadline )); then
      fail "node-$node best_height did not reach $expected (last=${height:-unknown})"
    fi
    sleep 3
  done
  echo "$height"
}

wait_for_convergence_to_a() {
  local label="$1" timeout="${2:-$SYNC_WAIT_SECS}"
  local deadline=$((SECONDS + timeout)) height_a height_b height_c
  until height_a="$(node_status_height a 2>/dev/null)" && \
        height_b="$(node_status_height b 2>/dev/null)" && \
        height_c="$(node_status_height c 2>/dev/null)" && \
        [[ "$height_a" =~ ^[0-9]+$ && "$height_b" =~ ^[0-9]+$ && "$height_c" =~ ^[0-9]+$ ]] && \
        (( height_b >= height_a && height_c >= height_a )); do
    if (( SECONDS >= deadline )); then
      fail "$label convergence timed out (a=${height_a:-unknown}, b=${height_b:-unknown}, c=${height_c:-unknown})"
    fi
    sleep 3
  done
  echo "[ok] $label convergence reached node-a observed height $height_a (a=$height_a b=$height_b c=$height_c)"
}

assert_chain_ids_match() {
  local expected="" node chain_id
  for node in "${NODES[@]}"; do
    chain_id="$(node_status_chain_id "$node")"
    [[ -n "$chain_id" ]] || fail "node-$node did not report chain_id"
    if [[ -z "$expected" ]]; then
      expected="$chain_id"
    elif [[ "$chain_id" != "$expected" ]]; then
      fail "chain_id mismatch: expected $expected but node-$node reported $chain_id"
    fi
  done
  [[ "$expected" == "$CHAIN_ID" ]] || fail "chain_id mismatch: nodes report $expected but rehearsal CHAIN_ID is $CHAIN_ID"
  echo "[ok] A/B/C chain_id matches: $expected"
}

assert_p2p_real_modes() {
  local node mode
  for node in "${NODES[@]}"; do
    mode="$(node_p2p_mode "$node")"
    [[ "$mode" == "libp2p-real" ]] || fail "node-$node expected p2p mode libp2p-real, got ${mode:-empty}"
    echo "[ok] node-$node reports p2p mode $mode"
  done
}

assert_sync_clear() {
  local label="$1" node pending requests missing orphans
  for node in "${NODES[@]}"; do
    requests="$(sync_value "$node" pending_block_requests)"
    missing="$(sync_value "$node" pending_missing_parents)"
    orphans="$(sync_value "$node" orphan_count)"
    [[ "${requests:-}" =~ ^[0-9]+$ ]] || fail "node-$node pending_block_requests was not numeric during $label: ${requests:-empty}"
    [[ "${missing:-}" =~ ^[0-9]+$ ]] || fail "node-$node pending_missing_parents was not numeric during $label: ${missing:-empty}"
    [[ "${orphans:-}" =~ ^[0-9]+$ ]] || fail "node-$node orphan_count was not numeric during $label: ${orphans:-empty}"
    (( requests == 0 )) || fail "node-$node has $requests pending_block_requests after $label"
    (( missing == 0 )) || fail "node-$node has $missing pending_missing_parents after $label"
    if (( orphans != 0 )); then
      collect_orphan_diagnostics "$label" "$node"
      fail "node-$node has $orphans orphan(s) after $label; diagnostics were collected"
    fi
    echo "[ok] node-$node sync clear after $label (pending_block_requests=0 pending_missing_parents=0 orphan_count=0)"
  done
}

print_status_summary() {
  echo
  echo "== v2.2.12 restart/rejoin rehearsal status =="
  for node in "${NODES[@]}"; do
    local height chain_id mode peers requests missing orphans rpc log
    rpc="$(rpc_url "$node")"
    height="$(node_status_height "$node" 2>/dev/null || echo unknown)"
    chain_id="$(node_status_chain_id "$node" 2>/dev/null || echo unknown)"
    mode="$(node_p2p_mode "$node" 2>/dev/null || echo unknown)"
    peers="$(node_peer_count "$node" 2>/dev/null || echo unknown)"
    requests="$(sync_value "$node" pending_block_requests 2>/dev/null || echo unknown)"
    missing="$(sync_value "$node" pending_missing_parents 2>/dev/null || echo unknown)"
    orphans="$(sync_value "$node" orphan_count 2>/dev/null || echo unknown)"
    log="$(node_log_file "$node")"
    echo "node-$node rpc=$rpc chain_id=$chain_id height=$height peers=$peers p2p_mode=$mode pending_block_requests=$requests pending_missing_parents=$missing orphan_count=$orphans log=$log"
  done
  echo "miner-a log=$(miner_log_file)"
  echo "evidence=$COLLECT_DIR"
}

command -v cargo >/dev/null || fail "cargo is required"
command -v curl >/dev/null || fail "curl is required"
command -v python3 >/dev/null || fail "python3 is required for JSON validation"

cd "$ROOT_DIR"
echo "[info] building workspace release binaries"
cargo build --workspace --release

rm -rf "$COLLECT_DIR"
mkdir -p "$COLLECT_DIR"

export PULSEDAG_CLEAN_DATA=1
stop_all_v2_2_12
start_node a
wait_for_http_ok a /health
wait_for_http_ok a /status
wait_for_http_ok a /p2p/status

bootnode_a="$(node_bootnode_a)"
if [[ -z "${PULSEDAG_NODE_A_BOOTNODE:-}" && "$bootnode_a" != */p2p/* ]]; then
  peer_id="$(node_peer_id a)"
  [[ -n "$peer_id" ]] || fail "node-a did not expose a peer_id for bootnode dialing"
  bootnode_a="$bootnode_a/p2p/$peer_id"
fi
echo "[info] using node-a bootnode $bootnode_a"

start_node b --bootnode "$bootnode_a"
wait_for_http_ok b /health
wait_for_http_ok b /status
wait_for_http_ok b /p2p/status
bootnode_b="$(node_bootnode_with_peer b)"
echo "[info] using node-b bootnode $bootnode_b"

start_node c --bootnode "$bootnode_a" --bootnode "$bootnode_b"
unset PULSEDAG_CLEAN_DATA

wait_for_http_ok c /health
wait_for_http_ok c /status
wait_for_http_ok c /p2p/status
bootnode_c="$(node_bootnode_with_peer c)"
echo "[info] using node-c bootnode $bootnode_c"

echo "[info] refreshing node-b with node-a and node-c bootnodes for a local full-mesh rejoin path"
start_node b --bootnode "$bootnode_a" --bootnode "$bootnode_c"
wait_for_http_ok b /health
wait_for_http_ok b /status
wait_for_http_ok b /p2p/status

assert_p2p_real_modes
assert_chain_ids_match
wait_for_current_peer_count b 1
wait_for_current_peer_count c 1
collect_all_nodes baseline

start_miner_a
height_a="$(wait_for_height_gt a 0 "$MINE_WAIT_SECS")"
echo "[ok] external miner advanced node-a best_height to $height_a"
stop_miner_a
echo "[info] paused miner-a so downstream convergence can settle at a stable node-a height"
wait_for_height_at_least b "$height_a" "$SYNC_WAIT_SECS" >/dev/null
wait_for_height_at_least c "$height_a" "$SYNC_WAIT_SECS" >/dev/null
wait_for_convergence_to_a initial-catchup "$SYNC_WAIT_SECS"
assert_chain_ids_match
assert_p2p_real_modes
assert_sync_clear initial-catchup
collect_all_nodes initial-catchup

height_before_b_stop="$(node_status_height a)"
echo "[info] stopping node-b at node-a height $height_before_b_stop"
stop_node b
start_miner_a
height_after_b_stop="$(wait_for_height_gt a "$height_before_b_stop" "$MINE_WAIT_SECS")"
echo "[ok] node-a continued mining while node-b was stopped (height $height_before_b_stop -> $height_after_b_stop)"
stop_miner_a
echo "[info] paused miner-a so node-b rejoin convergence can settle at a stable node-a height"

start_node b --bootnode "$bootnode_a" --bootnode "$bootnode_c"
wait_for_http_ok b /health
wait_for_http_ok b /status
wait_for_http_ok b /p2p/status
wait_for_current_peer_count b 1
wait_for_height_at_least b "$height_after_b_stop" "$SYNC_WAIT_SECS" >/dev/null
wait_for_convergence_to_a node-b-rejoin "$SYNC_WAIT_SECS"
assert_chain_ids_match
assert_p2p_real_modes
assert_sync_clear node-b-rejoin
collect_all_nodes node-b-rejoin

height_before_c_restart="$(node_status_height a)"
echo "[info] restarting node-c with the same data directory at node-a height $height_before_c_restart"
stop_node c
start_node c --bootnode "$bootnode_a" --bootnode "$bootnode_b"
wait_for_http_ok c /health
wait_for_http_ok c /status
wait_for_http_ok c /p2p/status
wait_for_current_peer_count c 1
wait_for_height_at_least c "$height_before_c_restart" "$SYNC_WAIT_SECS" >/dev/null
wait_for_convergence_to_a node-c-rejoin "$SYNC_WAIT_SECS"
assert_chain_ids_match
assert_p2p_real_modes
assert_sync_clear node-c-rejoin
collect_all_nodes final

print_status_summary
echo "[ok] v2.2.12 restart/rejoin smoke passed"
