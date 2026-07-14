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

# Real five-node lag-injection drill. Runs in a subshell so the EXIT trap always
# tears down nodes, miners, iptables rules, and the temporary n5 service account.
v2_3_0_run_lag_injection_selected_segment_drill() (
  set -euo pipefail

  local OUT_DIR=""
  local RUN_ID=""
  local MIN_SELECTED_GAP=96
  local ISOLATED_NODE="n5"
  local NODE_COUNT=5
  local MINER_COUNT=4

  while (($#)); do
    case "$1" in
      --out-dir) OUT_DIR="$2"; shift 2 ;;
      --run-id) RUN_ID="$2"; shift 2 ;;
      --min-selected-gap) MIN_SELECTED_GAP="$2"; shift 2 ;;
      --isolated-node) ISOLATED_NODE="$2"; shift 2 ;;
      --node-count) NODE_COUNT="$2"; shift 2 ;;
      --miner-count) MINER_COUNT="$2"; shift 2 ;;
      *) echo "unknown lag runtime argument: $1" >&2; exit 64 ;;
    esac
  done

  [[ -n "$OUT_DIR" && "$OUT_DIR" = /* ]] || {
    echo "--out-dir must be an absolute path" >&2
    exit 64
  }
  [[ -n "$RUN_ID" ]] || RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
  (( MIN_SELECTED_GAP >= 64 )) || {
    echo "--min-selected-gap must be at least 64" >&2
    exit 64
  }
  [[ "$ISOLATED_NODE" == "n5" && "$NODE_COUNT" == 5 && "$MINER_COUNT" == 4 ]] || {
    echo "runtime closeout requires isolated-node=n5, node-count=5, miner-count=4" >&2
    exit 64
  }

  local ROOT_DIR
  ROOT_DIR="$(pulsedag_repo_root)"
  local BASE_RPC_PORT="${BASE_RPC_PORT:-29500}"
  local BASE_P2P_PORT="${BASE_P2P_PORT:-29600}"
  local STARTUP_TIMEOUT_SECS="${STARTUP_TIMEOUT_SECS:-240}"
  local GAP_TIMEOUT_SECS="${GAP_TIMEOUT_SECS:-1200}"
  local RECOVERY_TIMEOUT_SECS="${RECOVERY_TIMEOUT_SECS:-1200}"
  local SAMPLE_INTERVAL_SECS="${SAMPLE_INTERVAL_SECS:-2}"
  local NODE_BIN="${PULSEDAGD_BIN:-$ROOT_DIR/target/release/pulsedagd}"
  local MINER_BIN="${PULSEDAG_MINER_BIN:-$ROOT_DIR/target/release/pulsedag-miner}"
  local CHAIN_ID="v2_3_0_lag_runtime_${GITHUB_RUN_ID:-local}_$$"
  local N5_USER="${N5_RUNTIME_USER:-pulsedag-lag-n5}"
  local MANIFEST_JSON="$OUT_DIR/evidence_manifest.json"
  local TIMELINE_JSON="$OUT_DIR/transition_timeline.json"
  local FINAL_TABLE="$OUT_DIR/final_convergence_table.md"
  local GAP_TIMELINE="$OUT_DIR/gap_timeline.json"
  local COUNTER_SUMMARY="$OUT_DIR/selected_segment_counter_summary.json"
  local TOPOLOGY_SAMPLES="$OUT_DIR/topology_samples.json"
  local COMMAND_LOG="$OUT_DIR/command-log.txt"
  local START_UTC
  START_UTC="$(date -u +%FT%TZ)"

  mkdir -p "$OUT_DIR"/{endpoints,logs,miners,pids,data,samples,diagnostics}
  : > "$COMMAND_LOG"
  printf '[]\n' > "$GAP_TIMELINE"
  printf '[]\n' > "$TOPOLOGY_SAMPLES"

  log() {
    printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$COMMAND_LOG"
  }

  append_json_array() {
    local file="$1" item="$2" tmp
    tmp="$(mktemp)"
    jq --argjson item "$item" '. + [$item]' "$file" > "$tmp"
    mv "$tmp" "$file"
  }

  json_number() {
    local file="$1" expr="$2"
    jq -r "$expr // 0" "$file" 2>/dev/null |
      head -n1 |
      awk '/^[0-9]+$/ {print; found=1} END {if (!found) print 0}'
  }

  peer_count() {
    local file="$1"
    jq -r '
      (.data // .) as $d |
      [
        (($d.connected_peers // []) | length),
        ($d.peer_count // 0),
        ($d.connected_peer_count // 0),
        ($d.peer_accounting.peer_count // 0),
        (($d.inbound_peer_final_state // [])
          | map(select(.state == "connected" and (.active_connections // 0) > 0))
          | length)
          +
        (($d.outbound_peer_final_state // [])
          | map(select(.state == "connected" and (.active_connections // 0) > 0))
          | length)
      ] | max // 0
    ' "$file" 2>/dev/null || echo 0
  }

  selected_height() {
    local file="$1"
    jq -r '(.data // .) | .selected_height // .best_height // .height // 0' "$file" 2>/dev/null || echo 0
  }

  selected_tip() {
    local file="$1"
    jq -r '(.data // .) | .selected_tip // .tip // ""' "$file" 2>/dev/null || true
  }

  capture_node() {
    local idx="$1" stage="$2"
    local base="http://127.0.0.1:$((BASE_RPC_PORT + idx))"
    local dir="$OUT_DIR/endpoints/$stage"
    local endpoint name
    mkdir -p "$dir"
    for endpoint in status p2p/status metrics sync/status sync/missing readiness checks health; do
      name="${endpoint//\//-}"
      if ! curl -fsS --connect-timeout 2 --max-time 10 "$base/$endpoint" > "$dir/n${idx}-${name}.json.tmp"; then
        printf '{"ok":false,"endpoint":"%s","captured_at":"%s"}\n' \
          "$endpoint" "$(date -u +%FT%TZ)" > "$dir/n${idx}-${name}.json.tmp"
      fi
      mv "$dir/n${idx}-${name}.json.tmp" "$dir/n${idx}-${name}.json"
    done
  }

  capture_all() {
    local stage="$1" idx
    for idx in $(seq 1 "$NODE_COUNT"); do
      capture_node "$idx" "$stage"
    done
  }

  wait_http() {
    local url="$1" out="$2" timeout="${3:-120}" start
    start="$(date +%s)"
    while (( $(date +%s) - start < timeout )); do
      if curl -fsS --connect-timeout 1 --max-time 4 "$url" > "$out.tmp"; then
        mv "$out.tmp" "$out"
        return 0
      fi
      sleep 1
    done
    rm -f "$out.tmp"
    return 1
  }

  metric_delta() {
    local before="$1" after="$2" expr="$3" b a
    b="$(json_number "$before" "$expr")"
    a="$(json_number "$after" "$expr")"
    echo $(( a > b ? a - b : 0 ))
  }

  for dependency in cargo curl jq python3 sudo iptables pgrep awk sed sha256sum tar; do
    command -v "$dependency" >/dev/null || {
      echo "runtime-closeout requires dependency '$dependency'; refusing to fabricate evidence" >&2
      exit 78
    }
  done

  declare -a NODE_PIDS=()
  declare -a NODE_LAUNCHERS=()
  declare -a MINER_PIDS=()
  local ISOLATION_ACTIVE=0
  local N5_UID=""
  local N5_GID=""

  remove_isolation() {
    (( ISOLATION_ACTIVE == 1 )) || return 0
    local port
    set +e
    sudo iptables -D INPUT -p tcp --dport "$((BASE_P2P_PORT + 5))" -j DROP 2>/dev/null || true
    sudo iptables -D OUTPUT -p tcp --sport "$((BASE_P2P_PORT + 5))" -j DROP 2>/dev/null || true
    for port in $(seq "$((BASE_P2P_PORT + 1))" "$((BASE_P2P_PORT + NODE_COUNT))"); do
      sudo iptables -D OUTPUT -m owner --uid-owner "$N5_UID" -p tcp --dport "$port" -j DROP 2>/dev/null || true
    done
    ISOLATION_ACTIVE=0
    set -e
    log "removed n5 P2P isolation rules"
  }

  cleanup() {
    local rc=$?
    trap - EXIT INT TERM
    set +e
    remove_isolation
    local pid port
    for pid in "${MINER_PIDS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
    for pid in "${MINER_PIDS[@]:-}"; do wait "$pid" 2>/dev/null || true; done
    for pid in "${NODE_PIDS[@]:-}"; do
      sudo kill "$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
    done
    for pid in "${NODE_LAUNCHERS[@]:-}"; do kill "$pid" 2>/dev/null || true; done
    if id "$N5_USER" >/dev/null 2>&1; then
      sudo userdel "$N5_USER" >/dev/null 2>&1 || true
    fi
    for port in $(seq "$((BASE_RPC_PORT + 1))" "$((BASE_RPC_PORT + NODE_COUNT))") \
                $(seq "$((BASE_P2P_PORT + 1))" "$((BASE_P2P_PORT + NODE_COUNT))"); do
      pulsedag_wait_port_closed "$port" 20 || true
    done
    exit "$rc"
  }
  trap cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  log "building release node and miner binaries"
  cargo build --release --locked --bin pulsedagd --bin pulsedag-miner \
    2>&1 | tee "$OUT_DIR/logs/cargo-build-release.log"
  [[ -x "$NODE_BIN" && -x "$MINER_BIN" ]] || {
    echo "missing release binaries; refusing runtime evidence" >&2
    exit 3
  }

  if ! id "$N5_USER" >/dev/null 2>&1; then
    sudo useradd --system --no-create-home --shell /usr/sbin/nologin "$N5_USER"
  fi
  N5_UID="$(id -u "$N5_USER")"
  N5_GID="$(id -g "$N5_USER")"
  printf '{"user":"%s","uid":%s,"gid":%s}\n' \
    "$N5_USER" "$N5_UID" "$N5_GID" > "$OUT_DIR/diagnostics/n5-runtime-identity.json"

  wait_for_child_pulsedagd() {
    local launcher="$1" deadline candidate
    deadline=$(( $(date +%s) + 20 ))
    while (( $(date +%s) < deadline )); do
      if [[ -e "/proc/$launcher/exe" ]] &&
         [[ "$(basename "$(readlink "/proc/$launcher/exe")")" == pulsedagd ]]; then
        echo "$launcher"
        return 0
      fi
      candidate="$(pgrep -P "$launcher" -f '(^|/)pulsedagd([[:space:]]|$)' | head -n1 || true)"
      if [[ -n "$candidate" && -e "/proc/$candidate/exe" ]] &&
         [[ "$(basename "$(readlink "/proc/$candidate/exe")")" == pulsedagd ]]; then
        echo "$candidate"
        return 0
      fi
      sleep 1
    done
    return 1
  }

  start_node() {
    local idx="$1"
    local bootnode="$2"
    local data="$OUT_DIR/data/n$idx"
    local log_file="$OUT_DIR/logs/n$idx.log"
    local rpc_port=$((BASE_RPC_PORT + idx))
    local p2p_port=$((BASE_P2P_PORT + idx))
    local -a cmd=(
      "$NODE_BIN"
      --network private
      --rpc-listen "127.0.0.1:$rpc_port"
      --p2p-listen "/ip4/127.0.0.1/tcp/$p2p_port"
      --consensus-mode ghostdag_dev
    )
    [[ -n "$bootnode" ]] && cmd+=(--bootnode "$bootnode")
    mkdir -p "$data"

    local launcher actual
    if (( idx == 5 )); then
      sudo chown -R "$N5_UID:$N5_GID" "$data"
      sudo -u "$N5_USER" env \
        PULSEDAG_CHAIN_ID="$CHAIN_ID" \
        PULSEDAG_ROCKSDB_PATH="$data/rocksdb" \
        PULSEDAG_P2P_IDENTITY_KEY="$data/p2p/identity.key" \
        PULSEDAG_API_PROFILE=local_dev \
        PULSEDAG_P2P_MODE=libp2p-real \
        PULSEDAG_P2P_MDNS=false \
        PULSEDAG_P2P_KADEMLIA=true \
        PULSEDAG_AUTO_PRUNE_ENABLED=false \
        RUST_LOG="pulsedagd=info,pulsedag_p2p=info" \
        RUST_LOG_STYLE=never \
        "${cmd[@]}" > "$log_file" 2>&1 &
      launcher=$!
      actual="$(wait_for_child_pulsedagd "$launcher")" || {
        echo "unable to identify live pulsedagd child for n5" >&2
        tail -n 120 "$log_file" >&2 || true
        return 1
      }
      NODE_LAUNCHERS[$idx]="$launcher"
    else
      PULSEDAG_CHAIN_ID="$CHAIN_ID" \
      PULSEDAG_ROCKSDB_PATH="$data/rocksdb" \
      PULSEDAG_P2P_IDENTITY_KEY="$data/p2p/identity.key" \
      PULSEDAG_API_PROFILE=local_dev \
      PULSEDAG_P2P_MODE=libp2p-real \
      PULSEDAG_P2P_MDNS=false \
      PULSEDAG_P2P_KADEMLIA=true \
      PULSEDAG_AUTO_PRUNE_ENABLED=false \
      RUST_LOG="pulsedagd=info,pulsedag_p2p=info" \
      RUST_LOG_STYLE=never \
      "${cmd[@]}" > "$log_file" 2>&1 &
      actual=$!
      NODE_LAUNCHERS[$idx]="$actual"
    fi

    NODE_PIDS[$idx]="$actual"
    printf '%s\n' "$actual" > "$OUT_DIR/pids/n$idx.pid"
    printf '%s\n' "${NODE_LAUNCHERS[$idx]}" > "$OUT_DIR/pids/n$idx-launcher.pid"
    log "started n$idx pid=$actual rpc=$rpc_port p2p=$p2p_port"
  }

  start_node 1 ""
  wait_http \
    "http://127.0.0.1:$((BASE_RPC_PORT + 1))/p2p/status" \
    "$OUT_DIR/endpoints/n1-p2p-bootstrap.json" \
    "$STARTUP_TIMEOUT_SECS" || {
      echo "n1 P2P status unavailable" >&2
      exit 1
    }

  local PEER_ID
  PEER_ID="$(jq -r '.data.peer_id // .data.local_node_id // .data.p2p_peer_id // empty' \
    "$OUT_DIR/endpoints/n1-p2p-bootstrap.json")"
  [[ -n "$PEER_ID" ]] || {
    echo "unable to extract n1 peer id" >&2
    exit 1
  }
  local BOOTNODE="/ip4/127.0.0.1/tcp/$((BASE_P2P_PORT + 1))/p2p/$PEER_ID"
  printf '%s\n' "$BOOTNODE" > "$OUT_DIR/bootnode.txt"

  local idx
  for idx in 2 3 4 5; do start_node "$idx" "$BOOTNODE"; done
  for idx in $(seq 1 "$NODE_COUNT"); do
    wait_http \
      "http://127.0.0.1:$((BASE_RPC_PORT + idx))/status" \
      "$OUT_DIR/endpoints/n${idx}-status-ready.json" \
      "$STARTUP_TIMEOUT_SECS" || {
        echo "n$idx status unavailable" >&2
        exit 1
      }
  done

  log "waiting for stable full-mesh 5N topology"
  local topology_deadline=$(( $(date +%s) + STARTUP_TIMEOUT_SECS ))
  local stable_samples=0
  while (( $(date +%s) < topology_deadline )); do
    capture_all topology
    local sample='[]'
    local topology_ok=1
    local count
    for idx in $(seq 1 "$NODE_COUNT"); do
      count="$(peer_count "$OUT_DIR/endpoints/topology/n${idx}-p2p-status.json")"
      [[ "$count" =~ ^[0-9]+$ ]] || count=0
      (( count >= 4 )) || topology_ok=0
      sample="$(jq \
        --arg node "n$idx" \
        --argjson peers "$count" \
        '. + [{node:$node,connected_peers:$peers}]' <<<"$sample")"
    done
    append_json_array "$TOPOLOGY_SAMPLES" \
      "$(jq -n --arg at "$(date -u +%FT%TZ)" --argjson nodes "$sample" \
        '{at:$at,phase:"startup",nodes:$nodes}')"
    if (( topology_ok == 1 )); then
      stable_samples=$((stable_samples + 1))
      (( stable_samples >= 3 )) && break
    else
      stable_samples=0
    fi
    sleep "$SAMPLE_INTERVAL_SECS"
  done
  (( stable_samples >= 3 )) || {
    echo "5N topology did not reach four peers per node" >&2
    exit 1
  }
  log "stable 5N/4-peer topology proven"

  capture_all pre-isolation
  cp "$OUT_DIR/endpoints/pre-isolation/n5-metrics.json" \
    "$OUT_DIR/endpoints/n5-metrics-before.json"
  cp "$OUT_DIR/endpoints/pre-isolation/n5-p2p-status.json" \
    "$OUT_DIR/endpoints/n5-p2p-before.json"

  local N5_PID="${NODE_PIDS[5]}"
  local N5_STARTTIME
  N5_STARTTIME="$(awk '{print $22}' "/proc/$N5_PID/stat")"
  local N5_PEER_ID_BEFORE
  N5_PEER_ID_BEFORE="$(jq -r '.data.peer_id // .data.local_node_id // empty' \
    "$OUT_DIR/endpoints/n5-p2p-before.json")"
  printf '{"pid":%s,"proc_starttime":"%s","peer_id":"%s","exe":"%s"}\n' \
    "$N5_PID" "$N5_STARTTIME" "$N5_PEER_ID_BEFORE" "$(readlink "/proc/$N5_PID/exe")" \
    > "$OUT_DIR/diagnostics/n5-process-before-isolation.json"

  log "installing process-scoped P2P isolation for n5 while preserving RPC"
  sudo iptables -I INPUT 1 -p tcp --dport "$((BASE_P2P_PORT + 5))" -j DROP
  sudo iptables -I OUTPUT 1 -p tcp --sport "$((BASE_P2P_PORT + 5))" -j DROP
  local port
  for port in $(seq "$((BASE_P2P_PORT + 1))" "$((BASE_P2P_PORT + NODE_COUNT))"); do
    sudo iptables -I OUTPUT 1 -m owner --uid-owner "$N5_UID" -p tcp --dport "$port" -j DROP
  done
  ISOLATION_ACTIVE=1
  sudo iptables-save > "$OUT_DIR/diagnostics/iptables-isolated.rules"
  curl -fsS --connect-timeout 1 --max-time 4 \
    "http://127.0.0.1:$((BASE_RPC_PORT + 5))/health" \
    > "$OUT_DIR/endpoints/n5-health-during-isolation.json"
  kill -0 "$N5_PID"
  [[ "$(awk '{print $22}' "/proc/$N5_PID/stat")" == "$N5_STARTTIME" ]] || {
    echo "n5 process identity changed during isolation" >&2
    exit 1
  }

  log "starting four external miners on n1-n4"
  local pid
  for idx in $(seq 1 "$MINER_COUNT"); do
    "$MINER_BIN" \
      --node "http://127.0.0.1:$((BASE_RPC_PORT + idx))" \
      --miner-address "v230-lag-${RUN_ID}-miner-$idx" \
      --backend cpu \
      --threads 1 \
      --max-tries 1000000 \
      --loop \
      --sleep-ms 100 \
      --no-heartbeat \
      > "$OUT_DIR/miners/miner-$idx.log" 2>&1 &
    pid=$!
    MINER_PIDS+=("$pid")
    printf '%s\n' "$pid" > "$OUT_DIR/pids/miner-$idx.pid"
  done

  log "waiting for n1-n4 to advance at least $MIN_SELECTED_GAP blocks beyond live isolated n5"
  local gap_deadline=$(( $(date +%s) + GAP_TIMEOUT_SECS ))
  local OBSERVED_GAP=0
  local NETWORK_HEIGHT=0
  local N5_HEIGHT=0
  declare -a heights=()
  while (( $(date +%s) < gap_deadline )); do
    for idx in $(seq 1 "$NODE_COUNT"); do
      local status_file="$OUT_DIR/endpoints/gap-n${idx}-status.json"
      curl -fsS --connect-timeout 1 --max-time 5 \
        "http://127.0.0.1:$((BASE_RPC_PORT + idx))/status" > "$status_file"
      heights[$idx]="$(selected_height "$status_file")"
    done
    NETWORK_HEIGHT="${heights[1]}"
    for idx in 2 3 4; do
      (( heights[idx] < NETWORK_HEIGHT )) && NETWORK_HEIGHT="${heights[idx]}"
    done
    N5_HEIGHT="${heights[5]}"
    OBSERVED_GAP=$(( NETWORK_HEIGHT > N5_HEIGHT ? NETWORK_HEIGHT - N5_HEIGHT : 0 ))
    append_json_array "$GAP_TIMELINE" \
      "$(jq -n \
        --arg at "$(date -u +%FT%TZ)" \
        --argjson network_height "$NETWORK_HEIGHT" \
        --argjson n5_height "$N5_HEIGHT" \
        --argjson gap "$OBSERVED_GAP" \
        '{at:$at,phase:"isolated",network_selected_height:$network_height,n5_selected_height:$n5_height,gap:$gap,n5_rpc_alive:true,n5_process_alive:true}')"
    curl -fsS --connect-timeout 1 --max-time 4 \
      "http://127.0.0.1:$((BASE_RPC_PORT + 5))/health" >/dev/null
    kill -0 "$N5_PID"
    (( OBSERVED_GAP >= MIN_SELECTED_GAP )) && break
    sleep "$SAMPLE_INTERVAL_SECS"
  done
  (( OBSERVED_GAP >= MIN_SELECTED_GAP )) || {
    echo "isolated n5 did not reach configured selected-height gap" >&2
    exit 1
  }
  log "observed gap=$OBSERVED_GAP network_height=$NETWORK_HEIGHT n5_height=$N5_HEIGHT"

  for pid in "${MINER_PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${MINER_PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
  MINER_PIDS=()

  capture_all at-gap
  cp "$OUT_DIR/endpoints/at-gap/n5-metrics.json" \
    "$OUT_DIR/endpoints/n5-metrics-before-reconnect.json"
  cp "$OUT_DIR/endpoints/at-gap/n5-p2p-status.json" \
    "$OUT_DIR/endpoints/n5-p2p-before-reconnect.json"

  remove_isolation
  sudo iptables-save > "$OUT_DIR/diagnostics/iptables-reconnected.rules"
  log "waiting for correlated selected-segment recovery and final convergence"

  local recovery_deadline=$(( $(date +%s) + RECOVERY_TIMEOUT_SECS ))
  local RECOVERED=0
  while (( $(date +%s) < recovery_deadline )); do
    capture_all recovering
    local final_tip=""
    local final_height=""
    local converged=1
    local peers_ok=1
    local h t
    for idx in $(seq 1 "$NODE_COUNT"); do
      local status="$OUT_DIR/endpoints/recovering/n${idx}-status.json"
      local p2p="$OUT_DIR/endpoints/recovering/n${idx}-p2p-status.json"
      h="$(selected_height "$status")"
      t="$(selected_tip "$status")"
      count="$(peer_count "$p2p")"
      if [[ -z "$final_tip" ]]; then
        final_tip="$t"
        final_height="$h"
      fi
      [[ -n "$t" && "$t" == "$final_tip" && "$h" == "$final_height" ]] || converged=0
      (( count >= 4 )) || peers_ok=0
    done

    local before_metrics="$OUT_DIR/endpoints/n5-metrics-before-reconnect.json"
    local after_metrics="$OUT_DIR/endpoints/recovering/n5-metrics.json"
    local hdr_req_delta hdr_recv_delta block_req_delta block_apply_delta chunk_delta active_remaining
    hdr_req_delta="$(metric_delta "$before_metrics" "$after_metrics" \
      '.data.selected_segment_header_requests_total')"
    hdr_recv_delta="$(metric_delta "$before_metrics" "$after_metrics" \
      '.data.selected_segment_headers_received_total')"
    block_req_delta="$(metric_delta "$before_metrics" "$after_metrics" \
      '.data.selected_segment_block_requests_total')"
    block_apply_delta="$(metric_delta "$before_metrics" "$after_metrics" \
      '.data.selected_segment_blocks_applied_total')"
    chunk_delta="$(metric_delta "$before_metrics" "$after_metrics" \
      '.data.selected_segment_chunks_completed_total')"
    active_remaining="$(json_number "$after_metrics" '.data.active_session_remaining_blocks')"

    if (( converged == 1 && peers_ok == 1 &&
          hdr_req_delta > 0 && hdr_recv_delta > 0 &&
          block_req_delta > 0 && block_apply_delta > 0 &&
          chunk_delta > 0 && active_remaining == 0 )); then
      RECOVERED=1
      break
    fi

    local current_n5_height current_gap
    current_n5_height="$(selected_height "$OUT_DIR/endpoints/recovering/n5-status.json")"
    current_gap=$(( final_height > current_n5_height ? final_height - current_n5_height : 0 ))
    append_json_array "$GAP_TIMELINE" \
      "$(jq -n \
        --arg at "$(date -u +%FT%TZ)" \
        --argjson network_height "${final_height:-0}" \
        --argjson n5_height "$current_n5_height" \
        --argjson gap "$current_gap" \
        --argjson header_requests "$hdr_req_delta" \
        --argjson blocks_applied "$block_apply_delta" \
        '{at:$at,phase:"recovering",network_selected_height:$network_height,n5_selected_height:$n5_height,gap:$gap,selected_segment_header_requests_delta:$header_requests,selected_segment_blocks_applied_delta:$blocks_applied}')"
    sleep "$SAMPLE_INTERVAL_SECS"
  done

  (( RECOVERED == 1 )) || {
    echo "n5 did not prove correlated selected-segment recovery before timeout" >&2
    capture_all recovery-timeout
    exit 1
  }

  capture_all final
  cp "$OUT_DIR/endpoints/final/n5-metrics.json" "$OUT_DIR/endpoints/n5-metrics-after.json"
  cp "$OUT_DIR/endpoints/final/n5-p2p-status.json" "$OUT_DIR/endpoints/n5-p2p-after.json"
  kill -0 "$N5_PID"

  local N5_STARTTIME_AFTER
  N5_STARTTIME_AFTER="$(awk '{print $22}' "/proc/$N5_PID/stat")"
  local N5_PEER_ID_AFTER
  N5_PEER_ID_AFTER="$(jq -r '.data.peer_id // .data.local_node_id // empty' \
    "$OUT_DIR/endpoints/n5-p2p-after.json")"
  [[ "$N5_STARTTIME_AFTER" == "$N5_STARTTIME" &&
     "$N5_PEER_ID_AFTER" == "$N5_PEER_ID_BEFORE" ]] || {
    echo "n5 process or P2P identity changed; isolation evidence invalid" >&2
    exit 1
  }

  printf '{"pid":%s,"proc_starttime":"%s","peer_id":"%s","exe":"%s","same_process":true,"same_peer_id":true}\n' \
    "$N5_PID" "$N5_STARTTIME_AFTER" "$N5_PEER_ID_AFTER" "$(readlink "/proc/$N5_PID/exe")" \
    > "$OUT_DIR/diagnostics/n5-process-after-recovery.json"

  python3 - "$OUT_DIR" "$MIN_SELECTED_GAP" "$OBSERVED_GAP" "$START_UTC" "$RUN_ID" "$CHAIN_ID" <<'PY'
import datetime
import json
import pathlib
import subprocess
import sys

out = pathlib.Path(sys.argv[1])
min_gap = int(sys.argv[2])
observed_gap = int(sys.argv[3])
start_utc, run_id, chain_id = sys.argv[4:7]

def read(path):
    return json.loads(path.read_text())

def data(doc):
    return doc.get("data", doc)

def num(doc, key):
    value = data(doc).get(key, 0)
    return int(value or 0) if not isinstance(value, dict) else 0

def delta(before, after, key):
    return max(0, num(after, key) - num(before, key))

def sum_counter(value):
    if isinstance(value, dict):
        return sum(int(v or 0) for v in value.values())
    return int(value or 0)

before_m = read(out / "endpoints/n5-metrics-before-reconnect.json")
after_m = read(out / "endpoints/n5-metrics-after.json")
before_p = read(out / "endpoints/n5-p2p-before-reconnect.json")
after_p = read(out / "endpoints/n5-p2p-after.json")
bp, ap = data(before_p), data(after_p)

remote_inventory_delta = max(
    0,
    sum_counter(ap.get("remote_tip_inventory_received_total", {}))
    - sum_counter(bp.get("remote_tip_inventory_received_total", {})),
)
remote_inventory_accepted_delta = max(
    0,
    int(ap.get("remote_tip_inventory_accepted_total", 0) or 0)
    - int(bp.get("remote_tip_inventory_accepted_total", 0) or 0),
)
header_requests = delta(before_m, after_m, "selected_segment_header_requests_total")
headers_received = delta(before_m, after_m, "selected_segment_headers_received_total")
locator_requests = max(
    header_requests,
    delta(before_m, after_m, "final_quiescence_selected_locator_request_total"),
)
locator_responses = max(
    headers_received,
    delta(before_m, after_m, "final_quiescence_selected_locator_success_total"),
)
block_requests = delta(before_m, after_m, "selected_segment_block_requests_total")
blocks_applied = delta(before_m, after_m, "selected_segment_blocks_applied_total")
chunks = delta(before_m, after_m, "selected_segment_chunks_completed_total")
peer_getblock_sent = max(
    0,
    int(ap.get("peer_addressed_getblock_sent_total", 0) or 0)
    - int(bp.get("peer_addressed_getblock_sent_total", 0) or 0),
)
peer_getblock_responses = max(
    0,
    int(ap.get("peer_addressed_getblock_response_total", 0) or 0)
    - int(bp.get("peer_addressed_getblock_response_total", 0) or 0),
)
generic_responses = max(
    0,
    int(ap.get("generic_getblock_response_total", 0) or 0)
    - int(bp.get("generic_getblock_response_total", 0) or 0),
)
unsolicited = max(
    0,
    int(ap.get("unsolicited_blockdata_total", 0) or 0)
    - int(bp.get("unsolicited_blockdata_total", 0) or 0),
)

nodes = []
tips, ordered, roots, digests, heights = set(), set(), set(), set(), set()
storage_ok = True
ready_ok = True
orphans = 0
missing = 0
pending = 0

for idx in range(1, 6):
    stage = out / "endpoints/final"
    status = data(read(stage / f"n{idx}-status.json"))
    p2p = data(read(stage / f"n{idx}-p2p-status.json"))
    metrics = data(read(stage / f"n{idx}-metrics.json"))
    sync = data(read(stage / f"n{idx}-sync-status.json"))
    readiness = data(read(stage / f"n{idx}-readiness.json"))
    checks_doc = data(read(stage / f"n{idx}-checks.json"))
    checks = checks_doc.get("checks", [])
    storage = next((x for x in checks if x.get("name") == "storage_consistency"), {})

    height = int(status.get("selected_height", status.get("best_height", status.get("height", 0))) or 0)
    tip = status.get("selected_tip") or status.get("tip") or ""
    ordered_tip = status.get("ordered_dag_tip") or ""
    root = status.get("ordered_dag_state_root") or status.get("state_root") or ""
    digest = storage.get("accepted_hash_set_digest") or checks_doc.get("accepted_hash_set_digest") or ""
    memory_digest = storage.get("memory_digest") or storage.get("memory_generation") or checks_doc.get("memory_digest") or checks_doc.get("memory_generation") or ""
    persisted_digest = storage.get("storage_digest") or storage.get("storage_generation") or checks_doc.get("storage_digest") or checks_doc.get("storage_generation") or ""
    coherent = bool(storage.get("ok", checks_doc.get("overall_ok", checks_doc.get("coherent_storage_invariant", False))))
    memory_only = storage.get("memory_only_hashes", checks_doc.get("memory_only_hashes", [])) or []
    storage_only = storage.get("storage_only_hashes", checks_doc.get("storage_only_hashes", [])) or []
    peers = max(
        len(p2p.get("connected_peers", []) or []),
        int(p2p.get("peer_count", 0) or 0),
        int((p2p.get("peer_accounting") or {}).get("peer_count", 0) or 0),
    )
    ready = bool(readiness.get("ready_for_release", False))
    orphan_count = int(sync.get("orphan_count", 0) or 0)
    pending_missing = int(sync.get("pending_missing_parents", 0) or 0)
    pending_requests = (
        int(metrics.get("active_session_remaining_blocks", 0) or 0)
        + int(metrics.get("pending_block_requests", 0) or 0)
    )
    node_storage_ok = (
        coherent
        and bool(memory_digest)
        and memory_digest == persisted_digest
        and not memory_only
        and not storage_only
    )
    nodes.append({
        "node": f"n{idx}",
        "selected_height": height,
        "selected_tip": tip,
        "ordered_dag_tip": ordered_tip,
        "state_root": root,
        "accepted_hash_set_digest": digest,
        "memory_digest": memory_digest,
        "storage_digest": persisted_digest,
        "storage_memory_digest_equal": bool(memory_digest and memory_digest == persisted_digest),
        "storage_only_hashes": storage_only,
        "memory_only_hashes": memory_only,
        "storage_coherent": node_storage_ok,
        "ready": ready,
        "compatible_peers": peers,
        "orphan_count": orphan_count,
        "pending_missing_parents": pending_missing,
        "pending_selected_segment_requests": pending_requests,
    })
    heights.add(height)
    tips.add(tip)
    ordered.add(ordered_tip)
    roots.add(root)
    digests.add(digest)
    storage_ok &= node_storage_ok
    ready_ok &= ready and peers >= 4
    orphans += orphan_count
    missing += pending_missing
    pending += pending_requests

final_convergence = (
    len(heights) == len(tips) == len(ordered) == len(roots) == len(digests) == 1
    and all(next(iter(values), "") for values in (tips, ordered, roots, digests))
    and ready_ok
)
counter_correlation = (
    remote_inventory_delta > 0
    and remote_inventory_accepted_delta > 0
    and locator_requests > 0
    and locator_responses > 0
    and block_requests > 0
    and 0 < blocks_applied <= block_requests
    and chunks > 0
)
primary_correlated = (
    peer_getblock_sent > 0
    and peer_getblock_responses > 0
    and peer_getblock_responses <= peer_getblock_sent
    and unsolicited == 0
)
result = (
    observed_gap >= min_gap
    and counter_correlation
    and primary_correlated
    and final_convergence
    and storage_ok
    and orphans == 0
    and missing == 0
    and pending == 0
)
failure_reasons = []
for ok, reason in [
    (observed_gap >= min_gap, "selected-height gap below configured minimum"),
    (remote_inventory_delta > 0 and remote_inventory_accepted_delta > 0, "remote selected-tip inventory was not observed and accepted"),
    (locator_requests > 0 and locator_responses > 0, "locator/header correlation counters are invalid"),
    (block_requests > 0 and 0 < blocks_applied <= block_requests and chunks > 0, "selected-segment block/chunk counters are invalid"),
    (primary_correlated, "peer-addressed correlated GetBlock path was not proven primary"),
    (final_convergence, "five nodes did not converge to identical final state"),
    (storage_ok, "storage/memory consistency was not proven"),
    (orphans == 0 and missing == 0 and pending == 0, "orphan, missing-parent, or pending selected-segment work remained"),
]:
    if not ok:
        failure_reasons.append(reason)

summary = {
    "remote_tip_inventory_received_total": remote_inventory_delta,
    "remote_tip_inventory_accepted_total": remote_inventory_accepted_delta,
    "locator_requests_sent_total": locator_requests,
    "locator_responses_correlated_total": locator_responses,
    "selected_segment_header_requests_total": header_requests,
    "selected_segment_headers_received_total": headers_received,
    "selected_segment_block_requests_total": block_requests,
    "selected_segment_blocks_applied_total": blocks_applied,
    "selected_segment_chunks_completed_total": chunks,
    "peer_addressed_getblock_sent_total": peer_getblock_sent,
    "peer_addressed_getblock_response_total": peer_getblock_responses,
    "generic_getblock_response_total": generic_responses,
    "unsolicited_blockdata_total": unsolicited,
}
(out / "selected_segment_counter_summary.json").write_text(json.dumps(summary, indent=2) + "\n")

manifest = {
    "manifest_version": "v2.3.0-lag-runtime-v2",
    "result": "PASS" if result else "FAIL",
    "evidence_kind": "runtime",
    "candidate_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
    "run_id": run_id,
    "chain_id": chain_id,
    "ci_mode": False,
    "closeout_eligible": bool(result),
    "synthetic_schema_evidence": False,
    "node_count": 5,
    "external_miners": 4,
    "isolated_node": "n5",
    "configured_min_gap": min_gap,
    "configured_min_selected_height_gap": min_gap,
    "observed_network_selected_height_gap": observed_gap,
    "canonical_network_selected_height_gap": observed_gap,
    **summary,
    "primary_session_path": "correlated_selected_segment" if primary_correlated else "unproven",
    "broadcast_getblock_primary_path": False if primary_correlated else None,
    "final_convergence": final_convergence,
    "storage_memory_consistent": storage_ok,
    "public_testnet_ready": False,
    "pending_selected_segment_requests": pending,
    "final_orphan_count": orphans,
    "final_missing_parent_blockers": missing,
    "final_state_by_node": nodes,
    "process_continuity": {
        "n5_same_process": True,
        "n5_same_peer_id": True,
        "rpc_remained_available_during_isolation": True,
        "isolation_mechanism": "iptables owner UID plus n5 P2P port rules",
    },
    "counter_sources": {
        "locator_requests_sent_total": "max(delta metrics.selected_segment_header_requests_total, delta metrics.final_quiescence_selected_locator_request_total)",
        "locator_responses_correlated_total": "max(delta metrics.selected_segment_headers_received_total, delta metrics.final_quiescence_selected_locator_success_total)",
        "block_and_chunk_totals": "delta of n5 /metrics selected_segment_* counters",
        "remote_tip_inventory": "delta of n5 /p2p/status remote_tip_inventory_* counters",
        "primary_path": "delta of n5 /p2p/status peer-addressed correlated GetBlock counters",
    },
    "timestamps": {
        "start_utc": start_utc,
        "end_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    },
    "failure_reasons": failure_reasons,
}
(out / "evidence_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")

timeline = [
    {"event": "remote_inventory_accepted", "evidence_source": "n5 /p2p/status counter delta", "count": remote_inventory_accepted_delta},
    {"event": "best_remote_selected_height_gt_local_height", "evidence_source": "gap_timeline.json", "gap": observed_gap},
    {"event": "network_selected_height_gap_observed", "evidence_source": "gap_timeline.json", "gap": observed_gap},
    {"event": "sync_state_locating_common_ancestor", "evidence_source": "selected-segment locator/header counter activation"},
    {"event": "locator_request_sent", "evidence_source": "n5 /metrics delta", "count": locator_requests},
    {"event": "matching_locator_header_response_accepted", "evidence_source": "n5 /metrics delta", "count": locator_responses},
    {"event": "selected_segment_session_active", "evidence_source": "selected-segment counters and active-session fields"},
    {"event": "parent_first_block_requests_sent", "evidence_source": "n5 /metrics delta", "count": block_requests},
    {"event": "blocks_received_and_applied", "evidence_source": "n5 /metrics delta", "count": blocks_applied},
    {"event": "chunks_completed", "evidence_source": "n5 /metrics delta", "count": chunks},
    {"event": "remote_selected_tip_selected_locally", "evidence_source": "five-node final selected-tip equality"},
    {"event": "session_completed", "evidence_source": "zero remaining/pending requests and final convergence"},
]
(out / "transition_timeline.json").write_text(json.dumps(timeline, indent=2) + "\n")

lines = [
    "| node | selected height | selected tip | ordered DAG tip | state root | accepted hash digest | storage coherent | ready | peers |",
    "| --- | ---: | --- | --- | --- | --- | --- | --- | ---: |",
]
for node in nodes:
    lines.append(
        f"| {node['node']} | {node['selected_height']} | {node['selected_tip']} | "
        f"{node['ordered_dag_tip']} | {node['state_root']} | "
        f"{node['accepted_hash_set_digest']} | {str(node['storage_coherent']).lower()} | "
        f"{str(node['ready']).lower()} | {node['compatible_peers']} |"
    )
(out / "final_convergence_table.md").write_text("\n".join(lines) + "\n")
PY

  jq -e '
    .result == "PASS" and
    .ci_mode == false and
    .evidence_kind == "runtime" and
    .closeout_eligible == true and
    .node_count == 5 and
    .external_miners == 4 and
    .isolated_node == "n5" and
    .observed_network_selected_height_gap >= .configured_min_selected_height_gap and
    .remote_tip_inventory_received_total > 0 and
    .remote_tip_inventory_accepted_total > 0 and
    .locator_requests_sent_total > 0 and
    .locator_responses_correlated_total > 0 and
    .selected_segment_block_requests_total > 0 and
    .selected_segment_blocks_applied_total > 0 and
    .selected_segment_blocks_applied_total <= .selected_segment_block_requests_total and
    .selected_segment_chunks_completed_total > 0 and
    .primary_session_path == "correlated_selected_segment" and
    .broadcast_getblock_primary_path == false and
    .final_convergence == true and
    .storage_memory_consistent == true and
    .pending_selected_segment_requests == 0 and
    .final_orphan_count == 0 and
    .final_missing_parent_blockers == 0 and
    .public_testnet_ready == false and
    ([.final_state_by_node[].ready] | all) and
    ([.final_state_by_node[].compatible_peers] | min) >= 4
  ' "$MANIFEST_JSON" >/dev/null || {
    echo "runtime lag evidence failed strict closeout semantics" >&2
    cat "$MANIFEST_JSON" >&2
    exit 1
  }

  log "runtime selected-segment lag evidence PASS: $OUT_DIR"
)
