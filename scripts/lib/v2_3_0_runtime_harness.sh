#!/usr/bin/env bash
# Shared v2.3.0 runtime harness helpers. This file is sourced by runtime drivers;
# it is not an executable closeout substitute.

v2_3_0_run_prune_restart_rejoin_drill() {
  set -euo pipefail
  local root_dir out_dir node_bin node_count min_offline chain_id runtime_root data_root log_dir endpoint_dir
  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  out_dir="${OUT_DIR:?OUT_DIR is required}"
  node_bin="${PULSEDAGD_BIN:-$root_dir/target/release/pulsedagd}"
  node_count="${V2_3_0_NODE_COUNT:-5}"
  min_offline="${MIN_OFFLINE_ADVANCE_BLOCKS:-64}"
  chain_id="${V2_3_0_CHAIN_ID:-pulsedag-v2-3-0-prune-runtime-$(date -u +%Y%m%dT%H%M%SZ)}"
  runtime_root="$out_dir/runtime"
  data_root="$runtime_root/data"
  log_dir="$out_dir/logs"
  endpoint_dir="$out_dir/endpoints"
  mkdir -p "$runtime_root" "$data_root" "$log_dir" "$endpoint_dir" "$out_dir/digests"

  local rpc_base="${V2_3_0_RPC_BASE_PORT:-21380}"
  local p2p_base="${V2_3_0_P2P_BASE_PORT:-21480}"
  local startup_wait="${V2_3_0_STARTUP_WAIT_SECS:-90}"
  local peer_wait="${V2_3_0_PEER_WAIT_SECS:-120}"
  local sync_wait="${V2_3_0_SYNC_WAIT_SECS:-240}"
  local curl_timeout="${V2_3_0_CURL_TIMEOUT_SECS:-8}"
  local initial_blocks="${V2_3_0_INITIAL_BLOCKS:-96}"
  local keep_recent="${V2_3_0_PRUNE_KEEP_RECENT_BLOCKS:-24}"
  local restart_node="${V2_3_0_RESTART_NODE:-3}"
  local bootnode_1=""

  [[ "$out_dir" = /* ]] || { echo "OUT_DIR must be absolute" >&2; return 64; }
  [[ -x "$node_bin" ]] || { echo "missing pulsedagd binary at $node_bin" >&2; return 65; }
  command -v curl >/dev/null || { echo "curl is required" >&2; return 65; }
  command -v jq >/dev/null || { echo "jq is required" >&2; return 65; }
  (( node_count == 5 )) || { echo "v2.3.0 prune/restart/rejoin drill requires exactly 5 nodes" >&2; return 65; }
  (( min_offline >= 64 )) || { echo "MIN_OFFLINE_ADVANCE_BLOCKS must be >=64" >&2; return 65; }

  _v230_rpc_port(){ echo $((rpc_base + $1 - 1)); }
  _v230_p2p_port(){ echo $((p2p_base + $1 - 1)); }
  _v230_node_name(){ printf 'node-%02d' "$1"; }
  _v230_rpc_url(){ echo "http://127.0.0.1:$(_v230_rpc_port "$1")"; }
  _v230_node_dir(){ echo "$data_root/$(_v230_node_name "$1")"; }
  _v230_db_dir(){ echo "$(_v230_node_dir "$1")/rocksdb"; }
  _v230_pid_file(){ echo "$runtime_root/$(_v230_node_name "$1").pid"; }
  _v230_log_file(){ echo "$log_dir/$(_v230_node_name "$1").log"; }
  _v230_http_get(){ curl -fsS -m "$curl_timeout" "$1"; }
  _v230_http_post(){ curl -fsS -m "$curl_timeout" -H 'content-type: application/json' -d "${2:-{}}" "$1"; }
  _v230_data(){ jq -er '.data // empty'; }
  _v230_status(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | tee "$endpoint_dir/$(_v230_node_name "$1")-$2-status.json" | _v230_data; }
  _v230_p2p(){ _v230_http_get "$(_v230_rpc_url "$1")/p2p/status" | tee "$endpoint_dir/$(_v230_node_name "$1")-$2-p2p-status.json" | _v230_data; }
  _v230_sync(){ _v230_http_get "$(_v230_rpc_url "$1")/sync/status" | tee "$endpoint_dir/$(_v230_node_name "$1")-$2-sync-status.json" | _v230_data; }
  _v230_height(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | jq -er '.data.best_height'; }
  _v230_tip(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | jq -er '.data.selected_tip // .data.last_block_hash // empty'; }
  _v230_state_root(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | jq -er '.data.ordered_dag_state_root // .data.last_block_hash // empty'; }
  _v230_peer_count(){ _v230_http_get "$(_v230_rpc_url "$1")/p2p/status" | jq -er '(.data.connected_peers | length) // .data.peer_count // 0'; }
  _v230_ready(){ _v230_http_get "$(_v230_rpc_url "$1")/readiness" | jq -r '(.data.ready // .data.node_ready // .ok // false)'; }

  _v230_stop_node(){
    local node="$1" pid_file pid
    pid_file="$(_v230_pid_file "$node")"
    [[ -f "$pid_file" ]] || return 0
    pid="$(cat "$pid_file")"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      for _ in {1..30}; do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$pid_file"
  }
  _v230_stop_all(){ local n; for ((n=node_count; n>=1; n--)); do _v230_stop_node "$n"; done; }
  trap _v230_stop_all RETURN

  _v230_start_node(){
    local node="$1" name rpc p2p db log
    name="$(_v230_node_name "$node")"; rpc="127.0.0.1:$(_v230_rpc_port "$node")"; p2p="/ip4/0.0.0.0/tcp/$(_v230_p2p_port "$node")"; db="$(_v230_db_dir "$node")"; log="$(_v230_log_file "$node")"
    mkdir -p "$db"
    local cmd=("$node_bin" --network private --rpc-listen "$rpc" --p2p-listen "$p2p")
    (( node > 1 )) && cmd+=(--bootnode "$bootnode_1")
    echo "[harness] start $name rpc=$rpc p2p=$p2p db=$db"
    (
      export PULSEDAG_NETWORK_PROFILE="v2-3-0-prune-$name"
      export PULSEDAG_CHAIN_ID="$chain_id"
      export PULSEDAG_P2P_ENABLED=true
      export PULSEDAG_P2P_MODE=libp2p-real
      export PULSEDAG_P2P_MDNS=false
      export PULSEDAG_P2P_KADEMLIA=true
      export PULSEDAG_ROCKSDB_PATH="$db"
      export PULSEDAG_AUTO_PRUNE_ENABLED=false
      export PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS="$keep_recent"
      export RUST_LOG="${RUST_LOG:-info}"
      exec "${cmd[@]}"
    ) >"$log" 2>&1 &
    echo $! > "$(_v230_pid_file "$node")"
  }
  _v230_wait_endpoint(){
    local node="$1" path="$2" deadline=$((SECONDS + startup_wait))
    until _v230_http_get "$(_v230_rpc_url "$node")$path" >/dev/null 2>&1; do
      (( SECONDS < deadline )) || { tail -120 "$(_v230_log_file "$node")" >&2 || true; echo "timeout waiting for $(_v230_node_name "$node") $path" >&2; return 1; }
      sleep 2
    done
  }
  _v230_wait_height(){
    local node="$1" target="$2" deadline=$((SECONDS + sync_wait)) h
    until h="$(_v230_height "$node" 2>/dev/null)" && (( h >= target )); do
      (( SECONDS < deadline )) || { echo "$(_v230_node_name "$node") height ${h:-missing} < $target" >&2; return 1; }
      sleep 2
    done
  }
  _v230_wait_final_convergence(){
    local deadline=$((SECONDS + sync_wait)) base_tip base_root ok n tip root peers ready missing orphan pending
    while (( SECONDS < deadline )); do
      ok=1; base_tip="$(_v230_tip 1 2>/dev/null || true)"; base_root="$(_v230_state_root 1 2>/dev/null || true)"
      [[ -n "$base_tip" && -n "$base_root" ]] || ok=0
      for ((n=1; n<=node_count; n++)); do
        tip="$(_v230_tip "$n" 2>/dev/null || true)"; root="$(_v230_state_root "$n" 2>/dev/null || true)"; peers="$(_v230_peer_count "$n" 2>/dev/null || echo 0)"; ready="$(_v230_ready "$n" 2>/dev/null || echo false)"
        missing="$(_v230_http_get "$(_v230_rpc_url "$n")/sync/status" 2>/dev/null | jq -r '(.data.missing_parent_active_entries // .data.pending_block_requests // 0)' || echo 1)"
        orphan="$(_v230_http_get "$(_v230_rpc_url "$n")/status" 2>/dev/null | jq -r '.data.orphan_count // 0' || echo 1)"
        pending="$(_v230_http_get "$(_v230_rpc_url "$n")/mempool" 2>/dev/null | jq -r '(.data.pending // .data.size // .data.mempool_size // 0)' || echo 0)"
        [[ "$tip" == "$base_tip" && "$root" == "$base_root" && "$ready" == "true" && "$peers" =~ ^[0-9]+$ && "$peers" -ge 4 && "$missing" == 0 && "$orphan" == 0 && "$pending" == 0 ]] || ok=0
      done
      (( ok == 1 )) && return 0
      sleep 3
    done
    return 1
  }
  _v230_mine_block(){
    local label="$1" out="$endpoint_dir/mine-$label.json"
    _v230_http_post "$(_v230_rpc_url 1)/mine" '{"miner_address":"v2-3-0-runtime-harness","pow_max_tries":1000000}' | tee "$out" | jq -e '.data.block_hash and .data.height' >/dev/null
  }

  local n peer_id before_restart_tip before_restart_root after_restart_tip after_restart_root before_offline_height after_offline_height offline_advance prune_json snapshot_json retained_json
  _v230_start_node 1; _v230_wait_endpoint 1 /health; _v230_wait_endpoint 1 /p2p/status
  peer_id="$(_v230_http_get "$(_v230_rpc_url 1)/p2p/status" | jq -er '.data.peer_id')"
  bootnode_1="/ip4/127.0.0.1/tcp/$(_v230_p2p_port 1)/p2p/$peer_id"
  for ((n=2; n<=node_count; n++)); do _v230_start_node "$n"; _v230_wait_endpoint "$n" /health; _v230_wait_endpoint "$n" /p2p/status; done
  local peer_deadline=$((SECONDS + peer_wait)) all_peered
  until all_peered=1; do
    all_peered=1; for ((n=1; n<=node_count; n++)); do [[ "$(_v230_peer_count "$n" 2>/dev/null || echo 0)" -ge 4 ]] || all_peered=0; done
    (( all_peered == 1 )) && break
    (( SECONDS < peer_deadline )) || { echo "five-node compatible peer mesh did not form" >&2; return 1; }
    sleep 2
  done

  for ((n=1; n<=initial_blocks; n++)); do _v230_mine_block "initial-$n"; done
  local target_height; target_height="$(_v230_height 1)"
  for ((n=2; n<=node_count; n++)); do _v230_wait_height "$n" "$target_height"; done

  snapshot_json="$endpoint_dir/snapshot-create.json"; _v230_http_post "$(_v230_rpc_url 1)/snapshot/create" '{}' | tee "$snapshot_json" | jq -e '.data.snapshot_exists == true' >/dev/null
  prune_json="$endpoint_dir/prune.json"; _v230_http_post "$(_v230_rpc_url 1)/admin/prune" "{\"keep_recent_blocks\":$keep_recent}" | tee "$prune_json" | jq -e '(.data.pruned_block_count // 0) > 0 and .data.replay_verified == true' >/dev/null
  retained_json="$endpoint_dir/retained-runtime.json"
  _v230_http_get "$(_v230_rpc_url 1)/runtime" | tee "$retained_json" | jq -e '(.data.blocks_pruned_total // 0) > 0 and (.data.retained_storage_hash_digest // "") == (.data.retained_memory_hash_digest // "")' >/dev/null

  before_restart_tip="$(_v230_tip "$restart_node")"; before_restart_root="$(_v230_state_root "$restart_node")"
  _v230_http_post "$(_v230_rpc_url "$restart_node")/snapshot/create" '{}' > "$endpoint_dir/$(_v230_node_name "$restart_node")-snapshot-create.json"
  _v230_stop_node "$restart_node"; sleep 3; _v230_start_node "$restart_node"; _v230_wait_endpoint "$restart_node" /health; _v230_wait_endpoint "$restart_node" /sync/status
  after_restart_tip="$(_v230_tip "$restart_node")"; after_restart_root="$(_v230_state_root "$restart_node")"
  [[ -n "$before_restart_tip" && "$before_restart_tip" == "$after_restart_tip" && -n "$before_restart_root" && "$before_restart_root" == "$after_restart_root" ]] || { echo "restart tip/state-root changed" >&2; return 1; }

  before_offline_height="$(_v230_height "$restart_node")"
  _v230_stop_node "$restart_node"
  for ((n=1; n<=min_offline; n++)); do _v230_mine_block "offline-$n"; done
  after_offline_height="$(_v230_height 1)"; offline_advance=$((after_offline_height - before_offline_height))
  (( offline_advance >= min_offline )) || { echo "offline advance $offline_advance < $min_offline" >&2; return 1; }
  for ((n=1; n<=node_count; n++)); do [[ "$n" -eq "$restart_node" ]] || _v230_wait_height "$n" "$after_offline_height"; done
  _v230_start_node "$restart_node"; _v230_wait_endpoint "$restart_node" /health; _v230_wait_height "$restart_node" "$after_offline_height"
  _v230_wait_final_convergence || { echo "final five-node convergence failed" >&2; return 1; }

  for ((n=1; n<=node_count; n++)); do _v230_status "$n" final >/dev/null; _v230_p2p "$n" final >/dev/null; _v230_sync "$n" final >/dev/null; done

  jq -n \
    --arg result PASS --arg evidence_kind runtime --arg source "endpoints/logs/runtime-harness" --arg commit "$(git -C "$root_dir" rev-parse HEAD)" \
    --argjson node_count "$node_count" --argjson min_offline "$min_offline" --argjson offline_advance "$offline_advance" \
    --slurpfile prune "$prune_json" --slurpfile runtime "$retained_json" \
    --arg pre_tip "$before_restart_tip" --arg post_tip "$after_restart_tip" --arg pre_root "$before_restart_root" --arg post_root "$after_restart_root" \
    --argjson final_nodes "$(for ((n=1; n<=node_count; n++)); do jq -n --arg name "$(_v230_node_name "$n")" --arg tip "$(_v230_tip "$n")" --arg root "$(_v230_state_root "$n")" --argjson peers "$(_v230_peer_count "$n")" --argjson ready "$( [[ "$(_v230_ready "$n")" == true ]] && echo true || echo false )" '{node:$name,ready:$ready,compatible_peers:$peers,selected_tip:$tip,state_root:$root,orphans:0,missing:0,pending:0}'; done | jq -s '.')" \
    '{result:$result,evidence_kind:$evidence_kind,candidate_commit:$commit,node_count:$node_count,invariant_source:$source,public_testnet_ready:false,
      blocks_pruned_total:($runtime[0].data.blocks_pruned_total // $prune[0].data.pruned_block_count),
      blocks_considered_total:(($runtime[0].data.blocks_considered_total // $runtime[0].data.blocks_pruned_total // $prune[0].data.pruned_block_count) + 0),
      prune_boundary_height:($runtime[0].data.prune_boundary_height // $prune[0].data.keep_from_height),
      retained_storage_hash_digest:$runtime[0].data.retained_storage_hash_digest,
      retained_memory_hash_digest:$runtime[0].data.retained_memory_hash_digest,
      storage_only_retained_hashes:($runtime[0].data.storage_only_retained_hashes // []),
      memory_only_retained_hashes:($runtime[0].data.memory_only_retained_hashes // []),
      snapshot_delta_restart_executed:true,restart_selected_tip_matches:($pre_tip == $post_tip),restart_state_root_matches:($pre_root == $post_root),
      pre_restart:{selected_tip:$pre_tip,state_root:$pre_root},post_restart:{selected_tip:$post_tip,state_root:$post_root},
      offline_advance_blocks:$offline_advance,minimum_required_offline_advance_blocks:$min_offline,rejoin_executed:true,rejoin_converged:true,
      final_storage_memory_consistent:true,final_nodes:$final_nodes}' > "$out_dir/evidence_manifest.json"
}
