#!/usr/bin/env bash
# Shared runtime harness helpers for v2.3.0 local five-node drills.

pulsedag_repo_root() { git rev-parse --show-toplevel; }

pulsedag_sha256_file() {
  local file="$1"
  sha256sum "$file" | awk '{print $1}'
}

pulsedag_wait_http_ok() {
  local url="$1" out="$2" timeout="${3:-60}" start
  start=$(date +%s)
  while (( $(date +%s) - start < timeout )); do
    if curl -fsS --connect-timeout 1 --max-time 3 "$url" > "$out.tmp"; then
      mv "$out.tmp" "$out"
      return 0
    fi
    sleep 1
  done
  rm -f "$out.tmp"
  return 1
}

pulsedag_wait_port_closed() {
  local port="$1" timeout="${2:-30}" start
  start=$(date +%s)
  while (( $(date +%s) - start < timeout )); do
    if ! (exec 3<>"/dev/tcp/127.0.0.1/${port}") 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  return 1
}

pulsedag_json_txids_sorted() {
  local file="$1"
  jq -r '(.data.txids // [])[]' "$file" | sort -u
}

pulsedag_write_checksums() {
  local dir="$1"
  (cd "$dir" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
}

# Run the v2.3.0 Task 03 five-node/four-miner lag-injection drill.
# n5 remains alive but is SIGSTOPed while n1-n4 advance. Before n5 resumes,
# its established P2P sockets are destroyed so queued gossip cannot masquerade
# as a correlated selected-segment recovery.
v2_3_0_run_lag_injection_selected_segment_drill() {
  local out_dir="" run_id="" min_selected_gap=96 isolated_node="n5" node_count=5 miner_count=4
  while (( $# )); do
    case "$1" in
      --out-dir) out_dir="${2:?missing --out-dir value}"; shift 2 ;;
      --run-id) run_id="${2:?missing --run-id value}"; shift 2 ;;
      --min-selected-gap) min_selected_gap="${2:?missing --min-selected-gap value}"; shift 2 ;;
      --isolated-node) isolated_node="${2:?missing --isolated-node value}"; shift 2 ;;
      --node-count) node_count="${2:?missing --node-count value}"; shift 2 ;;
      --miner-count) miner_count="${2:?missing --miner-count value}"; shift 2 ;;
      *) echo "unknown lag-injection harness argument: $1" >&2; return 2 ;;
    esac
  done

  [[ -n "$out_dir" && "$out_dir" = /* ]] || { echo "--out-dir must be absolute" >&2; return 2; }
  [[ -n "$run_id" ]] || { echo "--run-id is required" >&2; return 2; }
  [[ "$min_selected_gap" =~ ^[0-9]+$ ]] || { echo "--min-selected-gap must be numeric" >&2; return 2; }
  (( min_selected_gap >= 64 )) || { echo "--min-selected-gap must be at least 64" >&2; return 2; }
  [[ "$isolated_node" == "n5" && "$node_count" == 5 && "$miner_count" == 4 ]] || {
    echo "Task 03 requires isolated-node=n5, node-count=5, miner-count=4" >&2
    return 2
  }

  local root_dir node_bin miner_bin base_rpc_port base_p2p_port chain_id
  root_dir="$(pulsedag_repo_root)"
  node_bin="${NODE_BIN:-$root_dir/target/release/pulsedagd}"
  miner_bin="${MINER_BIN:-$root_dir/target/release/pulsedag-miner}"
  base_rpc_port="${BASE_RPC_PORT:-29300}"
  base_p2p_port="${BASE_P2P_PORT:-29400}"
  chain_id="v2_3_0_lag_${run_id}_$$"

  local startup_timeout="${STARTUP_TIMEOUT:-120}"
  local topology_timeout="${TOPOLOGY_TIMEOUT:-180}"
  local pre_isolation_min_height="${PRE_ISOLATION_MIN_HEIGHT:-8}"
  local gap_build_margin="${V2_3_0_GAP_BUILD_MARGIN_BLOCKS:-16}"
  [[ "$gap_build_margin" =~ ^[0-9]+$ ]] || { echo "gap build margin must be numeric" >&2; return 2; }
  local target_gap=$((min_selected_gap + gap_build_margin))
  local baseline_timeout="${BASELINE_TIMEOUT:-600}"
  local gap_build_timeout="${GAP_BUILD_TIMEOUT:-3600}"
  local recovery_timeout="${RECOVERY_TIMEOUT:-900}"
  local final_convergence_timeout="${FINAL_CONVERGENCE_TIMEOUT:-300}"
  local sample_interval="${SAMPLE_INTERVAL:-2}"

  mkdir -p "$out_dir" "$out_dir/endpoints" "$out_dir/logs" "$out_dir/miners" \
    "$out_dir/pids" "$out_dir/data" "$out_dir/samples"
  local command_log="$out_dir/command-log.txt"
  local timeline_jsonl="$out_dir/transition_timeline.jsonl"
  local gap_jsonl="$out_dir/gap_timeline.jsonl"
  local topology_jsonl="$out_dir/topology_samples.jsonl"
  local manifest_json="$out_dir/evidence_manifest.json"
  : > "$command_log"
  : > "$timeline_jsonl"
  : > "$gap_jsonl"
  : > "$topology_jsonl"

  local start_utc
  start_utc="$(date -u +%FT%TZ)"
  local -a node_pids=() miner_pids=() failures=()
  local isolated_stopped=0 cleanup_done=0
  local observed_gap=0 built_gap=0 harness_gap_max=0 canonical_gap_max=0 baseline_height=0 network_height=0
  local baseline_remote_received=0 baseline_remote_accepted=0
  local baseline_header_requests=0 baseline_headers_received=0 baseline_uncorrelated_headers=0
  local baseline_block_requests=0 baseline_blocks_applied=0 baseline_chunks_completed=0 baseline_peer_addressed_getblock=0
  local final_remote_received=0 final_remote_accepted=0 final_header_requests=0 final_headers_received=0
  local final_uncorrelated_headers=0 final_block_requests=0 final_blocks_applied=0 final_chunks_completed=0 final_peer_addressed_getblock=0
  local final_pending_selected=0 final_orphans=0 final_missing_parent_blockers=0
  local final_convergence=false storage_memory_consistent=false readiness_healthy=false topology_final=false
  local selected_session_seen=false session_completed=false canonical_gap_consistent=false

  _v230_lag_log() {
    printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$command_log"
  }
  _v230_lag_fail() {
    failures+=("$*")
    _v230_lag_log "FAIL: $*"
  }
  _v230_lag_rpc_url() {
    local idx="$1"
    printf 'http://127.0.0.1:%s' "$((base_rpc_port + idx))"
  }
  _v230_lag_json_num() {
    local file="$1" expr="$2"
    jq -r "$expr // 0" "$file" 2>/dev/null | head -n1 | awk '/^[0-9]+$/ {print; found=1} END {if (!found) print 0}'
  }
  _v230_lag_event() {
    local event="$1" node="${2:-}" details="${3:-{}}"
    jq -nc --arg at "$(date -u +%FT%TZ)" --arg event "$event" --arg node "$node" --argjson details "$details" \
      '{at:$at,event:$event} + (if $node == "" then {} else {node:$node} end) + $details' >> "$timeline_jsonl"
  }
  _v230_lag_capture_node() {
    local stage="$1" idx="$2" url ep out
    url="$(_v230_lag_rpc_url "$idx")"
    mkdir -p "$out_dir/endpoints/$stage"
    for ep in status p2p/status sync/status metrics checks readiness runtime; do
      out="$out_dir/endpoints/$stage/n${idx}-${ep//\//-}.json"
      curl -fsS --connect-timeout 1 --max-time 8 "$url/$ep" > "$out.tmp" 2>/dev/null && mv "$out.tmp" "$out" || rm -f "$out.tmp"
    done
  }
  _v230_lag_capture_all() {
    local stage="$1" idx
    for idx in 1 2 3 4 5; do _v230_lag_capture_node "$stage" "$idx"; done
  }
  _v230_lag_stop_pids() {
    local pid deadline
    for pid in "$@"; do [[ -n "$pid" ]] && kill -TERM "$pid" 2>/dev/null || true; done
    deadline=$(( $(date +%s) + 8 ))
    while (( $(date +%s) < deadline )); do
      local alive=0
      for pid in "$@"; do [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && alive=1; done
      (( alive == 0 )) && break
      sleep 1
    done
    for pid in "$@"; do [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null || true; done
    for pid in "$@"; do [[ -n "$pid" ]] && wait "$pid" 2>/dev/null || true; done
  }
  _v230_lag_stop_miners() {
    ((${#miner_pids[@]})) && _v230_lag_stop_pids "${miner_pids[@]}"
    miner_pids=()
  }
  _v230_lag_cleanup() {
    local rc="${1:-0}" idx
    (( cleanup_done == 0 )) || return 0
    cleanup_done=1
    if (( isolated_stopped == 1 )) && [[ -n "${node_pids[4]:-}" ]]; then
      kill -CONT "${node_pids[4]}" 2>/dev/null || true
      isolated_stopped=0
    fi
    _v230_lag_stop_miners
    ((${#node_pids[@]})) && _v230_lag_stop_pids "${node_pids[@]}"
    for idx in 1 2 3 4 5; do
      pulsedag_wait_port_closed "$((base_rpc_port + idx))" 15 || true
      pulsedag_wait_port_closed "$((base_p2p_port + idx))" 15 || true
    done
    pulsedag_write_checksums "$out_dir" 2>/dev/null || true
    return "$rc"
  }
  _v230_lag_package_failure() {
    local tarball="$out_dir/evidence.tar.gz" sha_file="$out_dir/evidence.tar.gz.sha256"
    (
      cd "$out_dir"
      find . -type f \
        ! -name 'evidence.tar.gz' \
        ! -name 'evidence.tar.gz.sha256' \
        ! -name 'SHA256SUMS' -print0 | sort -z | \
        tar --null -czf "$tarball" -T -
    ) 2>/dev/null || true
    [[ -s "$tarball" ]] && sha256sum "$tarball" > "$sha_file" 2>/dev/null || true
    pulsedag_write_checksums "$out_dir" 2>/dev/null || true
  }
  _v230_lag_write_failure_manifest() {
    local end_utc failures_json
    end_utc="$(date -u +%FT%TZ)"
    failures_json="$(printf '%s\n' "${failures[@]:-runtime drill aborted}" | jq -R . | jq -s .)"
    jq -n \
      --arg commit "$(git -C "$root_dir" rev-parse HEAD 2>/dev/null || echo unknown)" \
      --arg run_id "$run_id" --arg start "$start_utc" --arg end "$end_utc" \
      --argjson gap "$min_selected_gap" --argjson observed "$observed_gap" \
      --argjson canonical "$canonical_gap_max" --argjson failures "$failures_json" \
      '{manifest_version:"v2.3.0-task03",result:"FAIL",evidence_kind:"runtime",candidate_commit:$commit,run_id:$run_id,ci_mode:false,node_count:5,external_miners:4,isolated_node:"n5",configured_min_gap:$gap,configured_min_selected_height_gap:$gap,observed_network_selected_height_gap:$observed,canonical_network_selected_height_gap:$canonical,primary_session_path:"unproven",final_convergence:false,storage_memory_consistent:false,public_testnet_ready:false,closeout_eligible:false,synthetic_schema_evidence:false,broadcast_getblock_primary_path:false,pending_selected_segment_requests:0,final_orphan_count:0,final_missing_parent_blockers:0,timestamps:{start_utc:$start,end_utc:$end},failure_reasons:$failures}' > "$manifest_json"
  }
  _v230_lag_abort() {
    _v230_lag_fail "$1"
    jq -s . "$timeline_jsonl" > "$out_dir/transition_timeline.json" 2>/dev/null || printf '[]\n' > "$out_dir/transition_timeline.json"
    jq -s . "$gap_jsonl" > "$out_dir/gap_timeline.json" 2>/dev/null || printf '[]\n' > "$out_dir/gap_timeline.json"
    jq -s . "$topology_jsonl" > "$out_dir/topology_samples.json" 2>/dev/null || printf '[]\n' > "$out_dir/topology_samples.json"
    _v230_lag_write_failure_manifest
    _v230_lag_cleanup 1 || true
    _v230_lag_package_failure
    trap - EXIT INT TERM
    return 1
  }
  _v230_lag_unexpected_exit() {
    local rc="$1"
    if (( rc != 0 && cleanup_done == 0 )); then
      failures+=("unexpected shell exit rc=$rc")
      [[ -s "$manifest_json" ]] || _v230_lag_write_failure_manifest
      _v230_lag_cleanup "$rc" || true
    fi
  }
  trap '_v230_lag_unexpected_exit $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  _v230_lag_start_node() {
    local idx="$1" boot="$2" data="$out_dir/data/n$idx"
    local -a args=("$node_bin" --network private --rpc-listen "127.0.0.1:$((base_rpc_port + idx))" --p2p-listen "/ip4/127.0.0.1/tcp/$((base_p2p_port + idx))")
    [[ -n "$boot" ]] && args+=(--bootnode "$boot")
    mkdir -p "$data"
    _v230_lag_log "start n$idx ${args[*]}"
    PULSEDAG_CHAIN_ID="$chain_id" \
    PULSEDAG_ROCKSDB_PATH="$data/rocksdb" \
    PULSEDAG_API_PROFILE=local_dev \
    PULSEDAG_P2P_MODE=libp2p-real \
    PULSEDAG_P2P_MDNS=false \
    PULSEDAG_P2P_KADEMLIA=true \
    RUST_LOG="${RUST_LOG:-pulsedagd=info,pulsedag_p2p=info}" \
    RUST_LOG_STYLE=never \
      "${args[@]}" > "$out_dir/logs/n$idx.log" 2>&1 &
    node_pids+=("$!")
    printf '%s node-n%s\n' "$!" "$idx" >> "$out_dir/pids/process-pids.txt"
  }
  _v230_lag_start_miners() {
    local phase="$1" idx url
    miner_pids=()
    for idx in 1 2 3 4; do
      url="$(_v230_lag_rpc_url "$idx")"
      _v230_lag_log "start miner-$idx phase=$phase node=$url"
      "$miner_bin" --node "$url" --miner-address "v230-${run_id}-${phase}-miner-${idx}" --backend cpu --threads 1 --loop \
        >> "$out_dir/miners/miner-${idx}.log" 2>&1 &
      miner_pids+=("$!")
      printf '%s miner-%s-%s\n' "$!" "$idx" "$phase" >> "$out_dir/pids/process-pids.txt"
    done
  }
  _v230_lag_peer_count() {
    local file="$1"
    jq -r '[((.data.connected_peers? // []) | length),(.data.peer_count? // 0),(.data.connected_peer_count? // 0),((.data.peers? // []) | length)] | max // 0' "$file" 2>/dev/null || echo 0
  }
  _v230_lag_wait_topology() {
    local deadline stable=0 idx peers sample="[]" file
    deadline=$(( $(date +%s) + topology_timeout ))
    while (( $(date +%s) < deadline )); do
      sample="[]"
      local ok=1
      for idx in 1 2 3 4 5; do
        file="$out_dir/endpoints/topology-current-n${idx}.json"
        curl -fsS --connect-timeout 1 --max-time 5 "$(_v230_lag_rpc_url "$idx")/p2p/status" > "$file.tmp" 2>/dev/null && mv "$file.tmp" "$file" || { rm -f "$file.tmp"; ok=0; }
        peers="$(_v230_lag_peer_count "$file")"
        [[ "$peers" =~ ^[0-9]+$ ]] || peers=0
        (( peers >= 4 )) || ok=0
        sample="$(jq --arg node "n$idx" --argjson peers "$peers" '. + [{node:$node,connected_peers:$peers}]' <<< "$sample")"
      done
      jq -nc --arg at "$(date -u +%FT%TZ)" --arg phase startup --argjson nodes "$sample" '{at:$at,phase:$phase,nodes:$nodes}' >> "$topology_jsonl"
      if (( ok == 1 )); then stable=$((stable + 1)); else stable=0; fi
      (( stable >= 3 )) && return 0
      sleep 2
    done
    return 1
  }
  _v230_lag_status_height() {
    jq -r '.data.best_height // .data.selected_height // 0' "$1" 2>/dev/null || echo 0
  }
  _v230_lag_status_tip() {
    jq -r '.data.selected_tip // .data.tip // ""' "$1" 2>/dev/null || true
  }
  _v230_lag_wait_active_height() {
    local target="$1" deadline idx height min_height
    deadline=$(( $(date +%s) + baseline_timeout ))
    while (( $(date +%s) < deadline )); do
      min_height=999999999
      for idx in 1 2 3 4 5; do
        local file="$out_dir/endpoints/baseline-height-n${idx}.json"
        curl -fsS --connect-timeout 1 --max-time 5 "$(_v230_lag_rpc_url "$idx")/status" > "$file.tmp" 2>/dev/null && mv "$file.tmp" "$file" || { min_height=0; continue; }
        height="$(_v230_lag_status_height "$file")"
        (( height < min_height )) && min_height="$height"
      done
      (( min_height >= target )) && return 0
      sleep "$sample_interval"
    done
    return 1
  }
  _v230_lag_wait_converged_subset() {
    local nodes="$1" timeout="$2" deadline idx file height tip heights="" tips=""
    deadline=$(( $(date +%s) + timeout ))
    while (( $(date +%s) < deadline )); do
      heights=""; tips=""
      for idx in $nodes; do
        file="$out_dir/endpoints/converge-n${idx}.json"
        curl -fsS --connect-timeout 1 --max-time 5 "$(_v230_lag_rpc_url "$idx")/status" > "$file.tmp" 2>/dev/null && mv "$file.tmp" "$file" || { heights+=" 0"; tips+=$'\n'; continue; }
        height="$(_v230_lag_status_height "$file")"
        tip="$(_v230_lag_status_tip "$file")"
        heights+=" $height"
        tips+="$tip"$'\n'
      done
      local distinct_heights distinct_tips
      distinct_heights="$(tr ' ' '\n' <<< "$heights" | awk 'NF' | sort -u | wc -l | tr -d ' ')"
      distinct_tips="$(printf '%s' "$tips" | awk 'NF' | sort -u | wc -l | tr -d ' ')"
      [[ "$distinct_heights" == 1 && "$distinct_tips" == 1 ]] && return 0
      sleep "$sample_interval"
    done
    return 1
  }
  _v230_lag_kill_stopped_node_sockets() {
    local pid="$1" endpoint port killed=0
    local -a _v230_lag_ports=()
    command -v ss >/dev/null 2>&1 || return 1
    mapfile -t _v230_lag_ports < <(
      ss -Htnp state established 2>/dev/null | awk -v token="pid=$pid," 'index($0, token) {print $4}' | sed -E 's/.*:([0-9]+)$/\1/' | awk '/^[0-9]+$/' | sort -u
    )
    ((${#_v230_lag_ports[@]})) || return 1
    for port in "${_v230_lag_ports[@]}"; do
      if ss -K state established sport = ":$port" >/dev/null 2>&1; then
        killed=$((killed + 1))
      elif command -v sudo >/dev/null 2>&1 && sudo -n ss -K state established sport = ":$port" >/dev/null 2>&1; then
        killed=$((killed + 1))
      fi
    done
    sleep 1
    endpoint="$(ss -Htnp state established 2>/dev/null | awk -v token="pid=$pid," 'index($0, token) {print; exit}')"
    [[ -z "$endpoint" && "$killed" -gt 0 ]]
  }

  _v230_lag_log "building release binaries"
  if ! cargo build -p pulsedagd -p pulsedag-miner --release --locked 2>&1 | tee "$out_dir/build.log"; then
    _v230_lag_abort "release build failed"; return 1
  fi
  [[ -x "$node_bin" && -x "$miner_bin" ]] || { _v230_lag_abort "release binaries missing"; return 1; }
  command -v jq >/dev/null 2>&1 || { _v230_lag_abort "jq is required"; return 1; }
  command -v curl >/dev/null 2>&1 || { _v230_lag_abort "curl is required"; return 1; }
  command -v ss >/dev/null 2>&1 || { _v230_lag_abort "ss from iproute2 is required for socket isolation"; return 1; }

  _v230_lag_start_node 1 ""
  if ! pulsedag_wait_http_ok "$(_v230_lag_rpc_url 1)/p2p/status" "$out_dir/endpoints/n1-p2p-bootstrap.json" "$startup_timeout"; then
    _v230_lag_abort "n1 p2p bootstrap status unavailable"; return 1
  fi
  local peer_id boot idx
  peer_id="$(jq -r '.data.peer_id // .data.local_node_id // empty' "$out_dir/endpoints/n1-p2p-bootstrap.json")"
  [[ -n "$peer_id" ]] || { _v230_lag_abort "unable to extract n1 peer id"; return 1; }
  boot="/ip4/127.0.0.1/tcp/$((base_p2p_port + 1))/p2p/$peer_id"
  printf '%s\n' "$boot" > "$out_dir/bootnode.txt"
  for idx in 2 3 4 5; do _v230_lag_start_node "$idx" "$boot"; done
  for idx in 1 2 3 4 5; do
    if ! pulsedag_wait_http_ok "$(_v230_lag_rpc_url "$idx")/status" "$out_dir/endpoints/n${idx}-status-ready.json" "$startup_timeout"; then
      _v230_lag_abort "n$idx RPC readiness failed"; return 1
    fi
  done
  if ! _v230_lag_wait_topology; then _v230_lag_abort "topology did not stabilize at four peers per node"; return 1; fi
  _v230_lag_event stable_four_peer_topology "" '{"required_peers_per_node":4}'

  _v230_lag_start_miners baseline
  if ! _v230_lag_wait_active_height "$pre_isolation_min_height"; then
    _v230_lag_abort "five-node baseline did not reach the pre-isolation height"; return 1
  fi
  _v230_lag_stop_miners
  if ! _v230_lag_wait_converged_subset "1 2 3 4 5" "$final_convergence_timeout"; then
    _v230_lag_abort "five-node baseline did not converge before isolation"; return 1
  fi
  _v230_lag_capture_all pre_isolation
  baseline_height="$(_v230_lag_status_height "$out_dir/endpoints/pre_isolation/n5-status.json")"
  baseline_remote_received="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-p2p-status.json" '.data.remote_tip_inventory_received_total')"
  baseline_remote_accepted="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-p2p-status.json" '.data.remote_tip_inventory_accepted_total')"
  baseline_header_requests="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.selected_segment_header_requests_total')"
  baseline_headers_received="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.selected_segment_headers_received_total')"
  baseline_uncorrelated_headers="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.selected_segment_uncorrelated_headers_total')"
  baseline_block_requests="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.selected_segment_block_requests_total')"
  baseline_blocks_applied="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.selected_segment_blocks_applied_total')"
  baseline_chunks_completed="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.selected_segment_chunks_completed_total')"
  baseline_peer_addressed_getblock="$(_v230_lag_json_num "$out_dir/endpoints/pre_isolation/n5-metrics.json" '.data.peer_addressed_getblock_sent_total')"
  _v230_lag_event baseline_converged n5 "$(jq -nc --argjson height "$baseline_height" '{selected_height:$height}')"

  local n5_pid="${node_pids[4]}"
  kill -STOP "$n5_pid"
  isolated_stopped=1
  kill -0 "$n5_pid" 2>/dev/null || { _v230_lag_abort "n5 process exited instead of remaining alive during isolation"; return 1; }
  _v230_lag_event isolation_started n5 "$(jq -nc --argjson pid "$n5_pid" '{mechanism:"SIGSTOP",process_alive:true,pid:$pid}')"

  _v230_lag_start_miners gap
  local gap_deadline min_height height gap_sample active_tip
  gap_deadline=$(( $(date +%s) + gap_build_timeout ))
  while (( $(date +%s) < gap_deadline )); do
    min_height=999999999
    active_tip=""
    for idx in 1 2 3 4; do
      local status_file="$out_dir/endpoints/gap-current-n${idx}.json"
      curl -fsS --connect-timeout 1 --max-time 5 "$(_v230_lag_rpc_url "$idx")/status" > "$status_file.tmp" 2>/dev/null && mv "$status_file.tmp" "$status_file" || { min_height=0; continue; }
      height="$(_v230_lag_status_height "$status_file")"
      (( height < min_height )) && min_height="$height"
      [[ -n "$active_tip" ]] || active_tip="$(_v230_lag_status_tip "$status_file")"
    done
    (( min_height >= baseline_height )) || min_height="$baseline_height"
    observed_gap=$((min_height - baseline_height))
    jq -nc --arg at "$(date -u +%FT%TZ)" --arg phase isolated --arg node n5 --argjson network "$min_height" --argjson local "$baseline_height" --argjson gap "$observed_gap" '{at:$at,phase:$phase,isolated_node:$node,network_selected_height:$network,n5_selected_height:$local,gap:$gap}' >> "$gap_jsonl"
    (( observed_gap >= target_gap )) && { network_height="$min_height"; break; }
    sleep "$sample_interval"
  done
  _v230_lag_stop_miners
  (( observed_gap >= target_gap )) || { _v230_lag_abort "n1-n4 did not build the configured selected-height gap plus recovery margin"; return 1; }
  if ! _v230_lag_wait_converged_subset "1 2 3 4" "$final_convergence_timeout"; then
    _v230_lag_abort "n1-n4 did not quiesce on one selected tip before n5 rejoin"; return 1
  fi
  _v230_lag_capture_node gap_ready 1
  network_height="$(_v230_lag_status_height "$out_dir/endpoints/gap_ready/n1-status.json")"
  observed_gap=$((network_height - baseline_height))
  built_gap="$observed_gap"
  _v230_lag_event network_selected_height_gap_observed n5 "$(jq -nc --argjson gap "$built_gap" --argjson required "$min_selected_gap" --argjson target "$target_gap" '{gap:$gap,required_gap:$required,target_gap_with_margin:$target}')"
  _v230_lag_event miners_stopped_at_gap "" "$(jq -nc --argjson height "$network_height" '{network_selected_height:$height}')"

  if ! _v230_lag_kill_stopped_node_sockets "$n5_pid"; then
    _v230_lag_abort "failed to destroy n5 established P2P sockets before resume"; return 1
  fi
  _v230_lag_event stale_p2p_sockets_closed n5 '{"queued_gossip_discarded":true}'
  kill -CONT "$n5_pid"
  isolated_stopped=0
  _v230_lag_event node_resumed n5 '{"process_restarted":false,"storage_preserved":true}'

  local recovery_deadline sample_seq=0 stage peer session_id remote_height remote_tip
  local canonical_gap_sample=0 harness_gap_sample=0 current_height=0 current_tip="" network_tip=""
  local seen_inventory=0 seen_gap=0 seen_locating=0 seen_locator=0 seen_headers=0 seen_session=0 seen_blocks=0 seen_applied=0 seen_chunks=0 seen_tip=0
  recovery_deadline=$(( $(date +%s) + recovery_timeout ))
  while (( $(date +%s) < recovery_deadline )); do
    sample_seq=$((sample_seq + 1))
    local stage_name="recovery-$(printf '%04d' "$sample_seq")"
    _v230_lag_capture_node "$stage_name" 5
    local p2p_file="$out_dir/endpoints/$stage_name/n5-p2p-status.json"
    local metrics_file="$out_dir/endpoints/$stage_name/n5-metrics.json"
    local sync_file="$out_dir/endpoints/$stage_name/n5-sync-status.json"
    local status_file="$out_dir/endpoints/$stage_name/n5-status.json"
    [[ -s "$p2p_file" && -s "$metrics_file" && -s "$status_file" ]] || { sleep 1; continue; }

    final_remote_received="$(_v230_lag_json_num "$p2p_file" '.data.remote_tip_inventory_received_total')"
    final_remote_accepted="$(_v230_lag_json_num "$p2p_file" '.data.remote_tip_inventory_accepted_total')"
    final_header_requests="$(_v230_lag_json_num "$metrics_file" '.data.selected_segment_header_requests_total')"
    final_headers_received="$(_v230_lag_json_num "$metrics_file" '.data.selected_segment_headers_received_total')"
    final_uncorrelated_headers="$(_v230_lag_json_num "$metrics_file" '.data.selected_segment_uncorrelated_headers_total')"
    final_block_requests="$(_v230_lag_json_num "$metrics_file" '.data.selected_segment_block_requests_total')"
    final_blocks_applied="$(_v230_lag_json_num "$metrics_file" '.data.selected_segment_blocks_applied_total')"
    final_chunks_completed="$(_v230_lag_json_num "$metrics_file" '.data.selected_segment_chunks_completed_total')"
    final_peer_addressed_getblock="$(_v230_lag_json_num "$metrics_file" '.data.peer_addressed_getblock_sent_total')"
    final_pending_selected="$(_v230_lag_json_num "$metrics_file" '.data.active_session_remaining_blocks')"
    stage="$(jq -r '.data.sync_state // .data.catchup_stage // .data.state // "unknown"' "$sync_file" 2>/dev/null || echo unknown)"
    canonical_gap_sample="$(_v230_lag_json_num "$metrics_file" '.data.network_selected_height_gap')"
    (( canonical_gap_sample > canonical_gap_max )) && canonical_gap_max="$canonical_gap_sample"
    session_id="$(jq -r '.data.active_session_id // empty' "$metrics_file" 2>/dev/null || true)"
    peer="$(jq -r '.data.active_session_peer // empty' "$metrics_file" 2>/dev/null || true)"
    remote_height="$(jq -r '(.data.remote_selected_tip_inventory // []) | sort_by(.selected_height // .height // 0) | last | (.selected_height // .height // 0)' "$p2p_file" 2>/dev/null || echo 0)"
    remote_tip="$(jq -r '(.data.remote_selected_tip_inventory // []) | sort_by(.selected_height // .height // 0) | last | (.selected_tip // .tip // "")' "$p2p_file" 2>/dev/null || true)"
    [[ -n "$peer" ]] || peer="$(jq -r '(.data.remote_selected_tip_inventory // []) | sort_by(.selected_height // .height // 0) | last | (.peer_id // .peer // "")' "$p2p_file" 2>/dev/null || true)"
    current_height="$(_v230_lag_status_height "$status_file")"
    if [[ "$remote_height" =~ ^[0-9]+$ && "$current_height" =~ ^[0-9]+$ ]] && (( remote_height >= current_height )); then
      harness_gap_sample=$((remote_height - current_height))
      (( harness_gap_sample > harness_gap_max )) && harness_gap_max="$harness_gap_sample"
    fi

    if (( seen_inventory == 0 && final_remote_accepted > baseline_remote_accepted )); then
      _v230_lag_event remote_inventory_accepted n5 "$(jq -nc --arg peer "$peer" --arg remote_tip "$remote_tip" --argjson remote_height "${remote_height:-0}" '{peer:$peer,remote_selected_tip:$remote_tip,remote_selected_height:$remote_height}')"
      seen_inventory=1
    fi
    if (( seen_gap == 0 && canonical_gap_sample >= min_selected_gap )); then
      _v230_lag_event canonical_network_gap_detected n5 "$(jq -nc --argjson gap "$canonical_gap_sample" --argjson harness_gap "$harness_gap_sample" '{network_selected_height_gap:$gap,harness_observed_gap:$harness_gap}')"
      seen_gap=1
    fi
    if [[ "$stage" == *locating* || "$stage" == *ancestor* ]] && (( seen_locating == 0 )); then
      _v230_lag_event sync_state_locating_common_ancestor n5 "$(jq -nc --arg state "$stage" '{sync_state:$state}')"
      seen_locating=1
    fi
    if (( seen_locator == 0 && final_header_requests > baseline_header_requests )); then
      _v230_lag_event locator_request_sent n5 "$(jq -nc --arg peer "$peer" --arg session "$session_id" '{peer:$peer,session_id:$session}')"
      seen_locator=1
    fi
    if (( seen_headers == 0 && final_headers_received > baseline_headers_received && final_headers_received - baseline_headers_received > final_uncorrelated_headers - baseline_uncorrelated_headers )); then
      _v230_lag_event matching_locator_header_response_accepted n5 "$(jq -nc --arg peer "$peer" --arg session "$session_id" '{peer:$peer,session_id:$session}')"
      seen_headers=1
    fi
    if (( seen_session == 0 )) && [[ -n "$session_id" ]]; then
      _v230_lag_event selected_segment_session_active n5 "$(jq -nc --arg peer "$peer" --arg session "$session_id" '{peer:$peer,session_id:$session}')"
      seen_session=1
      selected_session_seen=true
    fi
    if (( seen_blocks == 0 && final_block_requests > baseline_block_requests )); then
      _v230_lag_event parent_first_block_requests_sent n5 "$(jq -nc --arg peer "$peer" --arg session "$session_id" --argjson requests "$((final_block_requests - baseline_block_requests))" '{peer:$peer,session_id:$session,requests:$requests}')"
      seen_blocks=1
    fi
    if (( seen_applied == 0 && final_blocks_applied > baseline_blocks_applied )); then
      _v230_lag_event blocks_received_and_applied n5 "$(jq -nc --arg session "$session_id" --argjson applied "$((final_blocks_applied - baseline_blocks_applied))" '{session_id:$session,applied_blocks:$applied}')"
      seen_applied=1
    fi
    if (( seen_chunks == 0 && final_chunks_completed > baseline_chunks_completed )); then
      _v230_lag_event chunks_completed n5 "$(jq -nc --argjson chunks "$((final_chunks_completed - baseline_chunks_completed))" '{completed_chunks:$chunks}')"
      seen_chunks=1
    fi

    current_height="$(_v230_lag_status_height "$status_file")"
    current_tip="$(_v230_lag_status_tip "$status_file")"
    network_tip="$(_v230_lag_status_tip "$out_dir/endpoints/gap_ready/n1-status.json")"
    if (( seen_tip == 0 )) && [[ "$current_height" == "$network_height" && -n "$network_tip" && "$current_tip" == "$network_tip" ]]; then
      _v230_lag_event remote_selected_tip_selected_locally n5 "$(jq -nc --arg tip "$current_tip" --argjson height "$current_height" '{selected_tip:$tip,selected_height:$height}')"
      seen_tip=1
    fi
    if (( seen_tip == 1 && seen_chunks == 1 )) && [[ -z "$session_id" ]] && (( final_pending_selected == 0 )); then
      _v230_lag_event session_completed n5 '{"pending_selected_segment_requests":0}'
      session_completed=true
      break
    fi
    sleep 1
  done

  if [[ "$selected_session_seen" != true || "$session_completed" != true ]]; then
    _v230_lag_abort "correlated selected-segment session was not observed through completion"; return 1
  fi
  if ! _v230_lag_wait_converged_subset "1 2 3 4 5" "$final_convergence_timeout"; then
    _v230_lag_abort "all five nodes did not converge after n5 recovery"; return 1
  fi
  _v230_lag_capture_all final

  local final_rows="[]" ref_height="" ref_tip="" ref_ordered="" ref_root="" ref_digest="" all_equal=1 storage_ok=1 ready_ok=1 topology_ok=1
  for idx in 1 2 3 4 5; do
    local status_file="$out_dir/endpoints/final/n${idx}-status.json"
    local checks_file="$out_dir/endpoints/final/n${idx}-checks.json"
    local readiness_file="$out_dir/endpoints/final/n${idx}-readiness.json"
    local p2p_file="$out_dir/endpoints/final/n${idx}-p2p-status.json"
    local metrics_file="$out_dir/endpoints/final/n${idx}-metrics.json"
    local h tip ordered root digest memory_count persisted_count coherent ready public_ready peers rpc_ok
    h="$(_v230_lag_status_height "$status_file")"
    tip="$(_v230_lag_status_tip "$status_file")"
    ordered="$(jq -r '.data.ordered_dag_tip // .data.selected_tip // ""' "$status_file" 2>/dev/null || true)"
    root="$(jq -r '.data.ordered_dag_state_root // .data.state_root // ""' "$status_file" 2>/dev/null || true)"
    digest="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.accepted_hash_set_digest // $d.accepted_hash_set_digest // ""' "$checks_file" 2>/dev/null || true)"
    memory_count="$(_v230_lag_json_num "$checks_file" '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.in_memory_dag_count // $s.memory_count // $d.in_memory_dag_count // $d.memory_count')"
    persisted_count="$(_v230_lag_json_num "$checks_file" '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.accepted_storage_count // $s.persisted_count // $d.accepted_storage_count // $d.persisted_count')"
    coherent="$(jq -r '(.data // .) as $d | ($d.checks // [] | map(select(.name == "storage_consistency")) | first // {}) as $s | $s.ok // $d.overall_ok // false' "$checks_file" 2>/dev/null || echo false)"
    ready="$(jq -r '(.data // .) as $r | (($r.node_operational_ready // false) or ($r.private_conservative_ready // false) or ($r.ready_for_release // false))' "$readiness_file" 2>/dev/null || echo false)"
    public_ready="$(jq -r '(.data // .) as $r | ($r.public_testnet_ready // false)' "$readiness_file" 2>/dev/null || echo false)"
    peers="$(_v230_lag_peer_count "$p2p_file")"
    rpc_ok="$(jq -r '.ok // false' "$status_file" 2>/dev/null || echo false)"
    [[ "$coherent" == true && "$memory_count" == "$persisted_count" && "$memory_count" -gt 0 ]] || storage_ok=0
    [[ "$ready" == true && "$public_ready" == false && "$rpc_ok" == true ]] || ready_ok=0
    [[ "$h" -gt 0 && -n "$tip" && -n "$ordered" && -n "$root" && -n "$digest" ]] || all_equal=0
    [[ "$peers" -ge 4 ]] || topology_ok=0
    if [[ -z "$ref_height" ]]; then
      ref_height="$h"; ref_tip="$tip"; ref_ordered="$ordered"; ref_root="$root"; ref_digest="$digest"
    elif [[ "$h" != "$ref_height" || "$tip" != "$ref_tip" || "$ordered" != "$ref_ordered" || "$root" != "$ref_root" || "$digest" != "$ref_digest" ]]; then
      all_equal=0
    fi
    final_rows="$(jq --arg node "n$idx" --arg tip "$tip" --arg ordered "$ordered" --arg root "$root" --arg digest "$digest" --argjson height "$h" --argjson memory "$memory_count" --argjson persisted "$persisted_count" --argjson ready "$ready" --argjson rpc "$rpc_ok" '. + [{node:$node,selected_height:$height,selected_tip:$tip,ordered_dag_tip:$ordered,state_root:$root,retained_hash_digest:$digest,storage_memory_retained_count:$memory,storage_persisted_retained_count:$persisted,ready:$ready,rpc_liveness:$rpc}]' <<< "$final_rows")"
  done
  echo "$final_rows" > "$out_dir/final_convergence.json"
  {
    echo '| node | selected height | selected tip | ordered DAG tip | state root | retained hash digest | memory retained | storage retained | ready | rpc liveness |'
    echo '| --- | ---: | --- | --- | --- | --- | ---: | ---: | --- | --- |'
    jq -r '.[] | "| \(.node) | \(.selected_height) | \(.selected_tip) | \(.ordered_dag_tip) | \(.state_root) | \(.retained_hash_digest) | \(.storage_memory_retained_count) | \(.storage_persisted_retained_count) | \(.ready) | \(.rpc_liveness) |"' "$out_dir/final_convergence.json"
  } > "$out_dir/final_convergence_table.md"

  final_convergence=$([[ "$all_equal" == 1 ]] && echo true || echo false)
  storage_memory_consistent=$([[ "$storage_ok" == 1 ]] && echo true || echo false)
  readiness_healthy=$([[ "$ready_ok" == 1 ]] && echo true || echo false)
  topology_final=$([[ "$topology_ok" == 1 ]] && echo true || echo false)

  local n5_metrics="$out_dir/endpoints/final/n5-metrics.json"
  local n5_p2p="$out_dir/endpoints/final/n5-p2p-status.json"
  final_remote_received="$(_v230_lag_json_num "$n5_p2p" '.data.remote_tip_inventory_received_total')"
  final_remote_accepted="$(_v230_lag_json_num "$n5_p2p" '.data.remote_tip_inventory_accepted_total')"
  final_header_requests="$(_v230_lag_json_num "$n5_metrics" '.data.selected_segment_header_requests_total')"
  final_headers_received="$(_v230_lag_json_num "$n5_metrics" '.data.selected_segment_headers_received_total')"
  final_uncorrelated_headers="$(_v230_lag_json_num "$n5_metrics" '.data.selected_segment_uncorrelated_headers_total')"
  final_block_requests="$(_v230_lag_json_num "$n5_metrics" '.data.selected_segment_block_requests_total')"
  final_blocks_applied="$(_v230_lag_json_num "$n5_metrics" '.data.selected_segment_blocks_applied_total')"
  final_chunks_completed="$(_v230_lag_json_num "$n5_metrics" '.data.selected_segment_chunks_completed_total')"
  final_peer_addressed_getblock="$(_v230_lag_json_num "$n5_metrics" '.data.peer_addressed_getblock_sent_total')"
  final_pending_selected="$(_v230_lag_json_num "$n5_metrics" '.data.active_session_remaining_blocks')"
  final_orphans="$(_v230_lag_json_num "$n5_metrics" '.data.orphan_current_count')"
  final_missing_parent_blockers="$(_v230_lag_json_num "$n5_metrics" '((.data.terminal_missing_parent_active_blocking_total // 0) + (.data.missing_parent_active_entries // 0))')"
  observed_gap="$canonical_gap_max"
  canonical_gap_consistent=$([[ "$observed_gap" -ge "$min_selected_gap" && "$built_gap" -ge "$target_gap" && "$harness_gap_max" -ge "$min_selected_gap" ]] && echo true || echo false)

  local remote_received_delta=$((final_remote_received - baseline_remote_received))
  local remote_accepted_delta=$((final_remote_accepted - baseline_remote_accepted))
  local header_requests_delta=$((final_header_requests - baseline_header_requests))
  local headers_received_delta=$((final_headers_received - baseline_headers_received))
  local uncorrelated_headers_delta=$((final_uncorrelated_headers - baseline_uncorrelated_headers))
  local correlated_headers_delta=$((headers_received_delta - uncorrelated_headers_delta))
  (( correlated_headers_delta < 0 )) && correlated_headers_delta=0
  local block_requests_delta=$((final_block_requests - baseline_block_requests))
  local blocks_applied_delta=$((final_blocks_applied - baseline_blocks_applied))
  local chunks_completed_delta=$((final_chunks_completed - baseline_chunks_completed))
  local peer_addressed_getblock_delta=$((final_peer_addressed_getblock - baseline_peer_addressed_getblock))

  jq -n --argjson remote_received "$remote_received_delta" --argjson remote_accepted "$remote_accepted_delta" \
    --argjson locator_requests "$header_requests_delta" --argjson locator_correlated "$correlated_headers_delta" \
    --argjson block_requests "$block_requests_delta" --argjson blocks_applied "$blocks_applied_delta" \
    --argjson chunks "$chunks_completed_delta" --argjson uncorrelated "$uncorrelated_headers_delta" \
    --argjson peer_addressed "$peer_addressed_getblock_delta" \
    '{remote_tip_inventory_received_total:$remote_received,remote_tip_inventory_accepted_total:$remote_accepted,locator_requests_sent_total:$locator_requests,locator_responses_correlated_total:$locator_correlated,selected_segment_block_requests_total:$block_requests,selected_segment_blocks_applied_total:$blocks_applied,selected_segment_chunks_completed_total:$chunks,selected_segment_uncorrelated_headers_total:$uncorrelated,peer_addressed_getblock_sent_total:$peer_addressed,broadcast_getblock_primary_path:false}' > "$out_dir/selected_segment_counter_summary.json"

  jq -s . "$timeline_jsonl" > "$out_dir/transition_timeline.json"
  jq -s . "$gap_jsonl" > "$out_dir/gap_timeline.json"
  jq -s . "$topology_jsonl" > "$out_dir/topology_samples.json"

  if [[ "$final_convergence" != true || "$storage_memory_consistent" != true || "$readiness_healthy" != true || "$topology_final" != true ]]; then
    _v230_lag_abort "final convergence, storage, readiness, or topology invariants failed"; return 1
  fi
  if [[ "$canonical_gap_consistent" != true || "$remote_received_delta" -le 0 || "$remote_accepted_delta" -le 0 || "$header_requests_delta" -le 0 || "$correlated_headers_delta" -le 0 || "$block_requests_delta" -le 0 || "$blocks_applied_delta" -le 0 || "$chunks_completed_delta" -le 0 || "$peer_addressed_getblock_delta" -lt "$block_requests_delta" ]]; then
    _v230_lag_abort "selected-segment correlation counters or canonical gap consistency failed"; return 1
  fi
  if (( final_pending_selected != 0 || final_orphans != 0 || final_missing_parent_blockers != 0 )); then
    _v230_lag_abort "final recovery queues or missing-parent/orphan blockers were non-zero"; return 1
  fi

  local end_utc
  end_utc="$(date -u +%FT%TZ)"
  jq -n \
    --arg commit "$(git -C "$root_dir" rev-parse HEAD 2>/dev/null || echo unknown)" \
    --arg run_id "$run_id" --arg start "$start_utc" --arg end "$end_utc" \
    --argjson configured "$min_selected_gap" --argjson observed "$observed_gap" --argjson canonical "$canonical_gap_max" \
    --argjson remote_received "$remote_received_delta" --argjson remote_accepted "$remote_accepted_delta" \
    --argjson locator_requests "$header_requests_delta" --argjson locator_correlated "$correlated_headers_delta" \
    --argjson block_requests "$block_requests_delta" --argjson blocks_applied "$blocks_applied_delta" --argjson chunks "$chunks_completed_delta" --argjson peer_addressed "$peer_addressed_getblock_delta" \
    --argjson final_pending "$final_pending_selected" --argjson final_orphans "$final_orphans" --argjson final_missing "$final_missing_parent_blockers" \
    --argjson harness_gap "$harness_gap_max" --argjson built_gap "$built_gap" --argjson target_gap "$target_gap" \
    '{manifest_version:"v2.3.0-task03",result:"PASS",evidence_kind:"runtime",candidate_commit:$commit,run_id:$run_id,ci_mode:false,node_count:5,external_miners:4,isolated_node:"n5",configured_min_gap:$configured,configured_min_selected_height_gap:$configured,observed_network_selected_height_gap:$observed,canonical_network_selected_height_gap:$canonical,harness_observed_gap_max:$harness_gap,gap_built_before_resume:$built_gap,gap_target_with_margin:$target_gap,remote_tip_inventory_received_total:$remote_received,remote_tip_inventory_accepted_total:$remote_accepted,locator_requests_sent_total:$locator_requests,locator_responses_correlated_total:$locator_correlated,selected_segment_block_requests_total:$block_requests,selected_segment_blocks_applied_total:$blocks_applied,selected_segment_chunks_completed_total:$chunks,peer_addressed_getblock_sent_total:$peer_addressed,primary_session_path:"correlated_selected_segment",final_convergence:true,storage_memory_consistent:true,readiness_healthy:true,final_topology_stable:true,public_testnet_ready:false,closeout_eligible:true,synthetic_schema_evidence:false,broadcast_getblock_primary_path:false,pending_selected_segment_requests:$final_pending,final_orphan_count:$final_orphans,final_missing_parent_blockers:$final_missing,timestamps:{start_utc:$start,end_utc:$end},failure_reasons:[],outputs:["transition_timeline.json","gap_timeline.json","topology_samples.json","selected_segment_counter_summary.json","final_convergence.json","final_convergence_table.md","endpoints","logs","miners","command-log.txt"]}' > "$manifest_json"

  _v230_lag_log "runtime selected-segment lag-injection evidence PASS"
  _v230_lag_cleanup 0 || true
  trap - EXIT INT TERM
  return 0
}
