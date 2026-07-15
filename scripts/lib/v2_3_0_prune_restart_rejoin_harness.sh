#!/usr/bin/env bash
# Real five-node Task 04 runtime drill. This module is sourced by the public
# fail-closed driver and intentionally remains separate from the Task 03 harness.

v2_3_0_run_prune_restart_rejoin_drill() (
  set -euo pipefail

  local root_dir out_dir node_bin miner_bin node_count offline_node min_offline
  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
  out_dir="${OUT_DIR:-}"
  node_bin="${PULSEDAGD_BIN:-$root_dir/target/release/pulsedagd}"
  miner_bin="${MINER_BIN:-$root_dir/target/release/pulsedag-miner}"
  node_count="${V2_3_0_NODE_COUNT:-5}"
  offline_node="${V2_3_0_OFFLINE_NODE:-n5}"
  min_offline="${MIN_OFFLINE_ADVANCE_BLOCKS:-64}"

  while (($#)); do
    case "$1" in
      --out-dir) out_dir="${2:?missing --out-dir value}"; shift 2 ;;
      --node-bin) node_bin="${2:?missing --node-bin value}"; shift 2 ;;
      --miner-bin) miner_bin="${2:?missing --miner-bin value}"; shift 2 ;;
      --node-count) node_count="${2:?missing --node-count value}"; shift 2 ;;
      --offline-node) offline_node="${2:?missing --offline-node value}"; shift 2 ;;
      --min-offline-advance-blocks) min_offline="${2:?missing minimum value}"; shift 2 ;;
      *) echo "unknown prune/restart/rejoin harness argument: $1" >&2; exit 64 ;;
    esac
  done

  [[ -n "$out_dir" && "$out_dir" = /* ]] || { echo "--out-dir must be absolute" >&2; exit 64; }
  [[ -x "$node_bin" ]] || { echo "missing pulsedagd binary: $node_bin" >&2; exit 65; }
  [[ -x "$miner_bin" ]] || { echo "missing external miner binary: $miner_bin" >&2; exit 65; }
  command -v curl >/dev/null || { echo "curl is required" >&2; exit 65; }
  command -v jq >/dev/null || { echo "jq is required" >&2; exit 65; }
  [[ "$node_count" =~ ^[0-9]+$ && "$node_count" -eq 5 ]] || { echo "Task 04 requires exactly five nodes" >&2; exit 65; }
  [[ "$min_offline" =~ ^[0-9]+$ && "$min_offline" -ge 64 ]] || { echo "minimum offline advance must be at least 64" >&2; exit 65; }
  [[ "$offline_node" =~ ^n([1-5])$ ]] || { echo "offline node must be n1..n5" >&2; exit 65; }
  local offline_idx="${BASH_REMATCH[1]}"
  (( offline_idx > 1 )) || { echo "offline node must be non-root" >&2; exit 65; }

  local chain_id="${V2_3_0_CHAIN_ID:-v2_3_0_prune_$(date -u +%Y%m%dT%H%M%SZ)_$$}"
  local runtime_root="$out_dir/runtime"
  local data_root="$runtime_root/data"
  local log_dir="$out_dir/logs"
  local endpoint_dir="$out_dir/endpoints"
  local miner_dir="$out_dir/miners"
  local rpc_base="${V2_3_0_RPC_BASE_PORT:-21380}"
  local p2p_base="${V2_3_0_P2P_BASE_PORT:-21480}"
  local startup_wait="${V2_3_0_STARTUP_WAIT_SECS:-120}"
  local peer_wait="${V2_3_0_PEER_WAIT_SECS:-180}"
  local sync_wait="${V2_3_0_SYNC_WAIT_SECS:-900}"
  local curl_timeout="${V2_3_0_CURL_TIMEOUT_SECS:-10}"
  local initial_blocks="${V2_3_0_INITIAL_BLOCKS:-96}"
  local keep_recent="${V2_3_0_PRUNE_KEEP_RECENT_BLOCKS:-24}"
  local bootnode_1=""
  local -a node_pids=()

  mkdir -p "$runtime_root" "$data_root" "$log_dir" "$endpoint_dir" "$miner_dir" "$out_dir/digests"
  : > "$out_dir/prune-command.log"

  _v230_prune_log(){ printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$out_dir/prune-command.log"; }
  _v230_rpc_port(){ echo $((rpc_base + $1 - 1)); }
  _v230_p2p_port(){ echo $((p2p_base + $1 - 1)); }
  _v230_node_name(){ printf 'n%d' "$1"; }
  _v230_rpc_url(){ echo "http://127.0.0.1:$(_v230_rpc_port "$1")"; }
  _v230_node_dir(){ echo "$data_root/$(_v230_node_name "$1")"; }
  _v230_pid_file(){ echo "$runtime_root/$(_v230_node_name "$1").pid"; }
  _v230_log_file(){ echo "$log_dir/$(_v230_node_name "$1").log"; }
  _v230_http_get(){ curl -fsS --connect-timeout 2 --max-time "$curl_timeout" "$1"; }
  _v230_http_post(){ local body="${2:-}"; [[ -n "$body" ]] || body='{}'; curl -fsS --connect-timeout 2 --max-time "$curl_timeout" -H 'content-type: application/json' -d "$body" "$1"; }
  _v230_height(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | jq -er '.data.best_height // .data.selected_height'; }
  _v230_tip(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | jq -er '.data.selected_tip // .data.tip // .data.last_block_hash // empty'; }
  _v230_state_root(){ _v230_http_get "$(_v230_rpc_url "$1")/status" | jq -er '.data.ordered_dag_state_root // .data.state_root // empty'; }
  _v230_peer_count(){ _v230_http_get "$(_v230_rpc_url "$1")/p2p/status" | jq -er '[((.data.connected_peers? // []) | length),(.data.peer_count? // 0),(.data.connected_peer_count? // 0),((.data.peers? // []) | length)] | max // 0'; }
  _v230_ready(){ _v230_http_get "$(_v230_rpc_url "$1")/readiness" | jq -r '(.data.node_operational_ready == true or .data.private_conservative_ready == true or .data.ghostdag_dev_ready == true)'; }

  _v230_stop_node(){
    local idx="$1" pid_file pid
    pid_file="$(_v230_pid_file "$idx")"
    [[ -s "$pid_file" ]] || return 0
    pid="$(cat "$pid_file")"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      for _ in {1..30}; do kill -0 "$pid" 2>/dev/null || break; sleep 1; done
      kill -9 "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
    rm -f "$pid_file"
  }
  _v230_stop_all(){ local idx; for ((idx=node_count; idx>=1; idx--)); do _v230_stop_node "$idx"; done; }
  trap _v230_stop_all EXIT INT TERM

  _v230_start_node(){
    local idx="$1"
    local data="$(_v230_node_dir "$idx")"
    local log="$(_v230_log_file "$idx")"
    local -a args=("$node_bin" --network private --rpc-listen "127.0.0.1:$(_v230_rpc_port "$idx")" --p2p-listen "/ip4/127.0.0.1/tcp/$(_v230_p2p_port "$idx")")
    (( idx > 1 )) && args+=(--bootnode "$bootnode_1")
    mkdir -p "$data/rocksdb"
    _v230_prune_log "start n$idx rpc=$(_v230_rpc_port "$idx") p2p=$(_v230_p2p_port "$idx")"
    PULSEDAG_CHAIN_ID="$chain_id" \
    PULSEDAG_ROCKSDB_PATH="$data/rocksdb" \
    PULSEDAG_API_PROFILE=local_dev \
    PULSEDAG_ADMIN_ENABLED=true \
    PULSEDAG_P2P_MODE=libp2p-real \
    PULSEDAG_P2P_MDNS=false \
    PULSEDAG_P2P_KADEMLIA=true \
    PULSEDAG_AUTO_PRUNE_ENABLED=false \
    PULSEDAG_PRUNE_KEEP_RECENT_BLOCKS="$keep_recent" \
    RUST_LOG="${RUST_LOG:-pulsedagd=info,pulsedag_p2p=info}" \
    RUST_LOG_STYLE=never \
      "${args[@]}" > "$log" 2>&1 &
    local pid=$!
    echo "$pid" > "$(_v230_pid_file "$idx")"
    node_pids+=("$pid")
  }

  _v230_wait_endpoint(){
    local idx="$1" path="$2" deadline=$(( $(date +%s) + startup_wait ))
    while (( $(date +%s) < deadline )); do
      _v230_http_get "$(_v230_rpc_url "$idx")$path" >/dev/null 2>&1 && return 0
      local pid=""; [[ -s "$(_v230_pid_file "$idx")" ]] && pid="$(cat "$(_v230_pid_file "$idx")")"
      [[ -z "$pid" || -z "$(ps -p "$pid" -o pid= 2>/dev/null)" ]] && break
      sleep 2
    done
    tail -120 "$(_v230_log_file "$idx")" >&2 || true
    echo "timeout waiting for n$idx $path" >&2
    return 1
  }

  _v230_wait_height(){
    local idx="$1" target="$2" deadline=$(( $(date +%s) + sync_wait )) h=0
    while (( $(date +%s) < deadline )); do
      h="$(_v230_height "$idx" 2>/dev/null || echo 0)"
      [[ "$h" =~ ^[0-9]+$ ]] && (( h >= target )) && return 0
      sleep 2
    done
    echo "n$idx height ${h:-missing} < $target" >&2
    return 1
  }

  _v230_wait_mesh(){
    local deadline=$(( $(date +%s) + peer_wait )) idx peers ok stable=0
    while (( $(date +%s) < deadline )); do
      ok=1
      for ((idx=1; idx<=node_count; idx++)); do
        peers="$(_v230_peer_count "$idx" 2>/dev/null || echo 0)"
        [[ "$peers" =~ ^[0-9]+$ ]] || peers=0
        (( peers >= node_count - 1 )) || ok=0
      done
      if (( ok == 1 )); then stable=$((stable + 1)); else stable=0; fi
      (( stable >= 3 )) && return 0
      sleep 2
    done
    return 1
  }

  _v230_mine_block(){
    local label="$1" before after deadline latest
    before="$(_v230_height 1)"
    "$miner_bin" --node "$(_v230_rpc_url 1)" --miner-address "v230-task04-$label" --max-tries 1000000 > "$miner_dir/$label.log" 2>&1
    deadline=$(( $(date +%s) + 60 ))
    while (( $(date +%s) < deadline )); do
      after="$(_v230_height 1 2>/dev/null || echo 0)"
      [[ "$after" =~ ^[0-9]+$ ]] && (( after > before )) && break
      sleep 1
    done
    [[ "$after" =~ ^[0-9]+$ ]] && (( after > before )) || { echo "external miner did not advance height for $label" >&2; return 1; }
    latest="$endpoint_dir/mine-$label.json"
    _v230_http_get "$(_v230_rpc_url 1)/blocks/latest" > "$latest"
    jq -e '.data.hash and .data.height' "$latest" >/dev/null
  }

  _v230_wait_final_convergence(){
    local deadline=$(( $(date +%s) + sync_wait )) idx base_tip base_root tip root peers ready ok stable=0
    while (( $(date +%s) < deadline )); do
      ok=1
      base_tip="$(_v230_tip 1 2>/dev/null || true)"
      base_root="$(_v230_state_root 1 2>/dev/null || true)"
      [[ -n "$base_tip" && -n "$base_root" ]] || ok=0
      for ((idx=1; idx<=node_count; idx++)); do
        tip="$(_v230_tip "$idx" 2>/dev/null || true)"
        root="$(_v230_state_root "$idx" 2>/dev/null || true)"
        peers="$(_v230_peer_count "$idx" 2>/dev/null || echo 0)"
        ready="$(_v230_ready "$idx" 2>/dev/null || echo false)"
        [[ "$tip" == "$base_tip" && "$root" == "$base_root" && "$ready" == true && "$peers" =~ ^[0-9]+$ ]] || ok=0
        [[ "$peers" =~ ^[0-9]+$ ]] && (( peers >= node_count - 1 )) || ok=0
      done
      if (( ok == 1 )); then stable=$((stable + 1)); else stable=0; fi
      (( stable >= 3 )) && return 0
      sleep 3
    done
    return 1
  }

  local idx peer_id target_height
  _v230_start_node 1
  _v230_wait_endpoint 1 /health
  _v230_wait_endpoint 1 /p2p/status
  peer_id="$(_v230_http_get "$(_v230_rpc_url 1)/p2p/status" | jq -er '.data.peer_id // .data.local_peer_id')"
  bootnode_1="/ip4/127.0.0.1/tcp/$(_v230_p2p_port 1)/p2p/$peer_id"
  echo "$bootnode_1" > "$out_dir/bootnode.txt"
  for ((idx=2; idx<=node_count; idx++)); do
    _v230_start_node "$idx"
    _v230_wait_endpoint "$idx" /health
    _v230_wait_endpoint "$idx" /p2p/status
  done
  _v230_wait_mesh || { echo "five-node peer mesh did not form" >&2; exit 1; }

  for ((idx=1; idx<=initial_blocks; idx++)); do _v230_mine_block "initial-$idx"; done
  target_height="$(_v230_height 1)"
  for ((idx=2; idx<=node_count; idx++)); do _v230_wait_height "$idx" "$target_height"; done
  _v230_wait_final_convergence || { echo "pre-prune convergence failed" >&2; exit 1; }

  local prune_rows='[]' runtime_rows='[]' snapshot_file prune_file runtime_file
  for ((idx=1; idx<=node_count; idx++)); do
    snapshot_file="$endpoint_dir/n${idx}-snapshot-create.json"
    prune_file="$endpoint_dir/n${idx}-prune.json"
    runtime_file="$endpoint_dir/n${idx}-post-prune-runtime.json"
    _v230_http_post "$(_v230_rpc_url "$idx")/snapshot/create" '{}' | tee "$snapshot_file" | jq -e '.data.snapshot_exists == true' >/dev/null
    _v230_http_post "$(_v230_rpc_url "$idx")/admin/prune" "{\"keep_recent_blocks\":$keep_recent}" | tee "$prune_file" | jq -e '(.data.pruned_block_count // 0) > 0 and .data.replay_verified == true' >/dev/null
    _v230_http_get "$(_v230_rpc_url "$idx")/runtime" | tee "$runtime_file" | jq -e '(.data.blocks_pruned_total // 0) > 0 and ((.data.retained_storage_hash_digest // "") | length) > 0 and .data.retained_storage_hash_digest == .data.retained_memory_hash_digest and ((.data.storage_only_retained_hashes // []) | length) == 0 and ((.data.memory_only_retained_hashes // []) | length) == 0' >/dev/null
    prune_rows="$(jq --arg node "n$idx" --slurpfile prune "$prune_file" '. + [{node:$node,pruned:($prune[0].data.pruned_block_count // 0),keep_from_height:$prune[0].data.keep_from_height}]' <<< "$prune_rows")"
    runtime_rows="$(jq --arg node "n$idx" --slurpfile runtime "$runtime_file" '. + [{node:$node,data:$runtime[0].data}]' <<< "$runtime_rows")"
  done
  echo "$prune_rows" > "$out_dir/prune-results.json"
  echo "$runtime_rows" > "$out_dir/post-prune-runtime-results.json"
  jq -e 'length == 5 and all(.[]; .pruned > 0)' "$out_dir/prune-results.json" >/dev/null
  jq -e 'length == 5 and ([.[].data.retained_storage_hash_digest] | unique | length) == 1 and all(.[]; .data.retained_storage_hash_digest == .data.retained_memory_hash_digest)' "$out_dir/post-prune-runtime-results.json" >/dev/null

  # Mine a real delta after the validated snapshots so restart must exercise the
  # snapshot+delta startup path rather than a snapshot-only fast boot.
  for idx in 1 2 3 4; do _v230_mine_block "post-prune-delta-$idx"; done
  target_height="$(_v230_height 1)"
  for ((idx=2; idx<=node_count; idx++)); do _v230_wait_height "$idx" "$target_height"; done
  _v230_wait_final_convergence || { echo "post-prune delta convergence failed" >&2; exit 1; }

  local before_restart_tip before_restart_root after_restart_tip after_restart_root restart_runtime
  before_restart_tip="$(_v230_tip "$offline_idx")"
  before_restart_root="$(_v230_state_root "$offline_idx")"
  _v230_stop_node "$offline_idx"
  sleep 3
  _v230_start_node "$offline_idx"
  _v230_wait_endpoint "$offline_idx" /health
  _v230_wait_endpoint "$offline_idx" /runtime
  _v230_wait_height "$offline_idx" "$target_height"
  after_restart_tip="$(_v230_tip "$offline_idx")"
  after_restart_root="$(_v230_state_root "$offline_idx")"
  [[ -n "$before_restart_tip" && "$before_restart_tip" == "$after_restart_tip" && -n "$before_restart_root" && "$before_restart_root" == "$after_restart_root" ]] || { echo "restart changed selected tip or state root" >&2; exit 1; }
  restart_runtime="$endpoint_dir/n${offline_idx}-post-restart-runtime.json"
  _v230_http_get "$(_v230_rpc_url "$offline_idx")/runtime" > "$restart_runtime"
  jq -e '.data.startup_snapshot_detected == true and .data.startup_snapshot_validated == true and .data.startup_delta_applied == true' "$restart_runtime" >/dev/null || { echo "restart did not prove snapshot+delta startup" >&2; exit 1; }

  local before_offline_height after_offline_height offline_advance
  before_offline_height="$(_v230_height "$offline_idx")"
  _v230_stop_node "$offline_idx"
  for ((idx=1; idx<=min_offline; idx++)); do _v230_mine_block "offline-$idx"; done
  after_offline_height="$(_v230_height 1)"
  offline_advance=$((after_offline_height - before_offline_height))
  (( offline_advance >= min_offline )) || { echo "offline advance $offline_advance < $min_offline" >&2; exit 1; }
  for ((idx=1; idx<=node_count; idx++)); do (( idx == offline_idx )) || _v230_wait_height "$idx" "$after_offline_height"; done

  _v230_start_node "$offline_idx"
  _v230_wait_endpoint "$offline_idx" /health
  _v230_wait_height "$offline_idx" "$after_offline_height"
  _v230_wait_mesh || { echo "peer mesh did not recover after rejoin" >&2; exit 1; }
  _v230_wait_final_convergence || { echo "final five-node convergence failed" >&2; exit 1; }

  local final_nodes='[]' final_runtime='[]' status_file readiness_file p2p_file final_runtime_file tip root peers ready
  for ((idx=1; idx<=node_count; idx++)); do
    status_file="$endpoint_dir/n${idx}-final-status.json"
    readiness_file="$endpoint_dir/n${idx}-final-readiness.json"
    p2p_file="$endpoint_dir/n${idx}-final-p2p.json"
    final_runtime_file="$endpoint_dir/n${idx}-final-runtime.json"
    _v230_http_get "$(_v230_rpc_url "$idx")/status" > "$status_file"
    _v230_http_get "$(_v230_rpc_url "$idx")/readiness" > "$readiness_file"
    _v230_http_get "$(_v230_rpc_url "$idx")/p2p/status" > "$p2p_file"
    _v230_http_get "$(_v230_rpc_url "$idx")/runtime" > "$final_runtime_file"
    tip="$(jq -r '.data.selected_tip // .data.tip // .data.last_block_hash // ""' "$status_file")"
    root="$(jq -r '.data.ordered_dag_state_root // .data.state_root // ""' "$status_file")"
    peers="$(jq -r '[((.data.connected_peers? // []) | length),(.data.peer_count? // 0),(.data.connected_peer_count? // 0),((.data.peers? // []) | length)] | max // 0' "$p2p_file")"
    ready="$(jq -r '(.data.node_operational_ready == true or .data.private_conservative_ready == true or .data.ghostdag_dev_ready == true)' "$readiness_file")"
    final_nodes="$(jq --arg node "n$idx" --arg tip "$tip" --arg root "$root" --argjson peers "$peers" --argjson ready "$ready" '. + [{node:$node,ready:$ready,compatible_peers:$peers,selected_tip:$tip,state_root:$root}]' <<< "$final_nodes")"
    final_runtime="$(jq --arg node "n$idx" --slurpfile runtime "$final_runtime_file" '. + [{node:$node,data:$runtime[0].data}]' <<< "$final_runtime")"
  done
  echo "$final_nodes" > "$out_dir/final-nodes.json"
  echo "$final_runtime" > "$out_dir/final-runtime-results.json"
  jq -e 'length == 5 and all(.[]; .ready == true and .compatible_peers >= 4 and (.selected_tip | length) > 0 and (.state_root | length) > 0)' "$out_dir/final-nodes.json" >/dev/null
  jq -e 'length == 5 and all(.[]; .data.retained_storage_hash_digest == .data.retained_memory_hash_digest and ((.data.storage_only_retained_hashes // []) | length) == 0 and ((.data.memory_only_retained_hashes // []) | length) == 0)' "$out_dir/final-runtime-results.json" >/dev/null

  local blocks_pruned blocks_considered prune_boundary retained_digest
  blocks_pruned="$(jq '[.[].pruned] | add' "$out_dir/prune-results.json")"
  blocks_considered="$(jq 'map(.data.blocks_considered_total // .data.blocks_pruned_total // 0) | add' "$out_dir/post-prune-runtime-results.json")"
  (( blocks_considered >= blocks_pruned )) || blocks_considered="$blocks_pruned"
  prune_boundary="$(jq 'map(.keep_from_height) | min' "$out_dir/prune-results.json")"
  retained_digest="$(jq -r '.[0].data.retained_storage_hash_digest' "$out_dir/post-prune-runtime-results.json")"

  jq -n \
    --arg commit "$(git -C "$root_dir" rev-parse HEAD)" \
    --arg source "endpoint/log/runtime-harness" \
    --arg pre_tip "$before_restart_tip" --arg post_tip "$after_restart_tip" \
    --arg pre_root "$before_restart_root" --arg post_root "$after_restart_root" \
    --arg retained_digest "$retained_digest" \
    --argjson node_count "$node_count" --argjson blocks_pruned "$blocks_pruned" \
    --argjson blocks_considered "$blocks_considered" --argjson prune_boundary "$prune_boundary" \
    --argjson offline_advance "$offline_advance" --argjson min_offline "$min_offline" \
    --slurpfile final_nodes "$out_dir/final-nodes.json" \
    '{result:"PASS",evidence_kind:"runtime",candidate_commit:$commit,node_count:$node_count,invariant_source:$source,public_testnet_ready:false,
      blocks_pruned_total:$blocks_pruned,blocks_considered_total:$blocks_considered,prune_boundary_height:$prune_boundary,
      retained_storage_hash_digest:$retained_digest,retained_memory_hash_digest:$retained_digest,
      storage_only_retained_hashes:[],memory_only_retained_hashes:[],snapshot_delta_restart_executed:true,
      restart_selected_tip_matches:($pre_tip == $post_tip),restart_state_root_matches:($pre_root == $post_root),
      pre_restart:{selected_tip:$pre_tip,state_root:$pre_root},post_restart:{selected_tip:$post_tip,state_root:$post_root},
      offline_advance_blocks:$offline_advance,minimum_required_offline_advance_blocks:$min_offline,
      rejoin_executed:true,rejoin_converged:true,final_storage_memory_consistent:true,final_nodes:$final_nodes[0]}' \
    > "$out_dir/evidence_manifest.json"

  pulsedag_write_checksums "$out_dir"
  _v230_prune_log "runtime prune/restart/rejoin evidence PASS"
)
