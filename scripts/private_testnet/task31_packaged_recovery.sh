#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NODE_BIN="${NODE_BIN:?NODE_BIN is required}"
MINER_BIN="${MINER_BIN:?MINER_BIN is required}"
CANDIDATE_SHA="${CANDIDATE_SHA:?CANDIDATE_SHA is required}"
OUT_DIR="${OUT_DIR:?OUT_DIR is required}"
DATA_ROOT="${DATA_ROOT:?DATA_ROOT is required}"
NODE_URL="${NODE_URL:-http://127.0.0.1:18080}"
RPC_BIND="${RPC_BIND:-127.0.0.1:18080}"
MINE_TIMEOUT_SECS="${MINE_TIMEOUT_SECS:-420}"
MAX_TRIES="${MAX_TRIES:-1000000}"
THREADS="${THREADS:-2}"

NODE_BIN="$(realpath "$NODE_BIN")"
MINER_BIN="$(realpath "$MINER_BIN")"
DATA_ROOT="$(realpath -m "$DATA_ROOT")"
OUT_DIR="$(realpath -m "$OUT_DIR")"

case "$NODE_BIN" in
  "$ROOT_DIR"/target/*) echo "refusing source-tree node binary: $NODE_BIN" >&2; exit 2 ;;
esac
case "$MINER_BIN" in
  "$ROOT_DIR"/target/*) echo "refusing source-tree miner binary: $MINER_BIN" >&2; exit 2 ;;
esac

test -x "$NODE_BIN"
test -x "$MINER_BIN"
mkdir -p "$OUT_DIR" "$DATA_ROOT"
ROCKSDB_PATH="$DATA_ROOT/rocksdb"
ENV_FILE="$OUT_DIR/operator.env"

cat > "$ENV_FILE" <<EOF
PULSEDAG_SINGLE_NODE_MODE=true
PULSEDAG_PRIVATE_TESTNET_ROLE=single
PULSEDAG_CONFIG_PROFILE=private
PULSEDAG_NETWORK_PROFILE=private-testnet-v2.4.0
PULSEDAG_CHAIN_ID=pulsedag-private-v2.4.0
PULSEDAG_CONSENSUS_MODE=legacy
PULSEDAG_PROTOCOL_CONSENSUS_MODE=ghostdag_v1
PULSEDAG_AUTO_PRUNE_ENABLED=false
PULSEDAG_P2P_ENABLED=false
PULSEDAG_P2P_BOOTSTRAP=
PULSEDAG_PUBLIC_P2P_MULTIADDR=
PULSEDAG_RPC_BIND=$RPC_BIND
PULSEDAG_API_PROFILE=private_operator
PULSEDAG_ROCKSDB_PATH=$ROCKSDB_PATH
PULSEDAG_PRUNE_REQUIRE_SNAPSHOT=true
PULSEDAG_CONTRACTS_ENABLED=false
PULSEDAG_ADMIN_ENABLED=false
PULSEDAG_PUBLIC_TESTNET_READY=false
PULSEDAG_THIRTY_DAY_PUBLIC_TESTNET_CLOCK_STARTED=false
EOF

OUT_DIR="$OUT_DIR/preflight" bash "$ROOT_DIR/scripts/v2_4_0_single_node_preflight.sh" "$ENV_FILE"

NODE_PID=""
MINER_PID=""
cleanup() {
  set +e
  if [[ -n "$MINER_PID" ]] && kill -0 "$MINER_PID" 2>/dev/null; then
    kill "$MINER_PID" 2>/dev/null || true
    wait "$MINER_PID" 2>/dev/null || true
  fi
  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" 2>/dev/null; then
    kill "$NODE_PID" 2>/dev/null || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

status_file() {
  local path="$1" tmp="${1}.tmp"
  if ! curl --fail --silent --show-error --connect-timeout 1 --max-time 5 "$NODE_URL/status" > "$tmp"; then
    rm -f "$tmp"
    return 1
  fi
  if ! jq -e '.data != null' "$tmp" >/dev/null; then
    rm -f "$tmp"
    return 1
  fi
  mv "$tmp" "$path"
}

validate_live_status() {
  local path="$1"
  jq -e '
    .data.chain_id == "pulsedag-private-v2.4.0" and
    .data.protocol_consensus_mode == "ghostdag_v1" and
    .data.high_cadence_allowed == false and
    .data.contracts_enabled == false and
    .data.p2p_enabled == false and
    .data.rpc_response_stale == false
  ' "$path" >/dev/null
}

start_node() {
  local log="$1" probe="$2"
  (
    set -a
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    set +a
    exec "$NODE_BIN"
  ) > "$log" 2>&1 &
  NODE_PID=$!

  for _ in $(seq 1 90); do
    if ! kill -0 "$NODE_PID" 2>/dev/null; then
      echo "packaged node exited before becoming ready" >&2
      tail -n 160 "$log" >&2 || true
      return 1
    fi
    if status_file "$probe" 2>/dev/null && validate_live_status "$probe"; then
      return 0
    fi
    sleep 1
  done
  echo "packaged node did not become ready" >&2
  tail -n 160 "$log" >&2 || true
  return 1
}

stop_node() {
  if [[ -n "$NODE_PID" ]] && kill -0 "$NODE_PID" 2>/dev/null; then
    kill "$NODE_PID"
    for _ in $(seq 1 30); do
      kill -0 "$NODE_PID" 2>/dev/null || break
      sleep 1
    done
    if kill -0 "$NODE_PID" 2>/dev/null; then
      echo "packaged node did not stop cleanly" >&2
      return 1
    fi
    wait "$NODE_PID" 2>/dev/null || true
  fi
  NODE_PID=""
}

start_node "$OUT_DIR/node-first.log" "$OUT_DIR/status-initial.json"
initial_height="$(jq -r '.data.best_height' "$OUT_DIR/status-initial.json")"

"$MINER_BIN" \
  --node "$NODE_URL" \
  --miner-address task31-packaged-recovery \
  --threads "$THREADS" \
  --max-tries "$MAX_TRIES" \
  --loop \
  --sleep-ms 250 > "$OUT_DIR/miner.log" 2>&1 &
MINER_PID=$!

deadline=$((SECONDS + MINE_TIMEOUT_SECS))
advanced=0
while (( SECONDS < deadline )); do
  if ! kill -0 "$MINER_PID" 2>/dev/null; then
    echo "packaged miner exited before producing an accepted block" >&2
    tail -n 160 "$OUT_DIR/miner.log" >&2 || true
    exit 1
  fi
  if status_file "$OUT_DIR/status-mining.json" 2>/dev/null; then
    current_height="$(jq -r '.data.best_height // 0' "$OUT_DIR/status-mining.json")"
    if (( current_height > initial_height )); then
      advanced=1
      break
    fi
  fi
  sleep 2
done

if (( advanced != 1 )); then
  echo "packaged miner did not advance chain within ${MINE_TIMEOUT_SECS}s" >&2
  tail -n 160 "$OUT_DIR/miner.log" >&2 || true
  exit 1
fi

kill "$MINER_PID" 2>/dev/null || true
wait "$MINER_PID" 2>/dev/null || true
MINER_PID=""
status_file "$OUT_DIR/status-before-restart.json"
validate_live_status "$OUT_DIR/status-before-restart.json"

before_height="$(jq -r '.data.best_height' "$OUT_DIR/status-before-restart.json")"
before_tip="$(jq -r '.data.selected_tip // empty' "$OUT_DIR/status-before-restart.json")"
before_root="$(jq -r '.data.ordered_dag_state_root // empty' "$OUT_DIR/status-before-restart.json")"
before_persisted="$(jq -r '.data.persisted_block_count' "$OUT_DIR/status-before-restart.json")"

test "$before_height" -gt "$initial_height"
test -n "$before_tip"
test "$before_persisted" -gt 0

stop_node
start_node "$OUT_DIR/node-restart.log" "$OUT_DIR/status-after-restart.json"
status_file "$OUT_DIR/status-after-restart.json"
validate_live_status "$OUT_DIR/status-after-restart.json"

after_height="$(jq -r '.data.best_height' "$OUT_DIR/status-after-restart.json")"
after_tip="$(jq -r '.data.selected_tip // empty' "$OUT_DIR/status-after-restart.json")"
after_root="$(jq -r '.data.ordered_dag_state_root // empty' "$OUT_DIR/status-after-restart.json")"
after_persisted="$(jq -r '.data.persisted_block_count' "$OUT_DIR/status-after-restart.json")"

test "$after_height" = "$before_height"
test "$after_tip" = "$before_tip"
test "$after_root" = "$before_root"
test "$after_persisted" = "$before_persisted"

stop_node

du -sh "$DATA_ROOT" > "$OUT_DIR/storage-size.txt" || true
sha256sum "$NODE_BIN" > "$OUT_DIR/node-binary.sha256"
sha256sum "$MINER_BIN" > "$OUT_DIR/miner-binary.sha256"

jq -n \
  --arg candidate_sha "$CANDIDATE_SHA" \
  --arg node_bin "$NODE_BIN" \
  --arg miner_bin "$MINER_BIN" \
  --argjson initial_height "$initial_height" \
  --argjson recovered_height "$after_height" \
  --arg selected_tip "$after_tip" \
  --arg state_root "$after_root" \
  --argjson persisted_block_count "$after_persisted" \
  '{
    task: "v2.4.0-task31-packaged-recovery",
    candidate_sha: $candidate_sha,
    package_execution: true,
    source_tree_binary_execution: false,
    node_binary: $node_bin,
    miner_binary: $miner_bin,
    chain_id: "pulsedag-private-v2.4.0",
    protocol_consensus_mode: "ghostdag_v1",
    initial_height: $initial_height,
    recovered_height: $recovered_height,
    selected_tip: $selected_tip,
    ordered_dag_state_root: $state_root,
    persisted_block_count: $persisted_block_count,
    same_storage_restart: true,
    restart_tip_parity: true,
    restart_state_root_parity: true,
    restart_persisted_count_parity: true,
    public_testnet_ready: false,
    release_tag_authorized: false,
    default_high_cadence_authorized: false,
    smart_contract_activation_authorized: false,
    result: "PASS"
  }' > "$OUT_DIR/evidence.json"

jq -e --arg sha "$CANDIDATE_SHA" '
  .candidate_sha == $sha and
  .package_execution == true and
  .source_tree_binary_execution == false and
  .protocol_consensus_mode == "ghostdag_v1" and
  .restart_tip_parity == true and
  .restart_state_root_parity == true and
  .restart_persisted_count_parity == true and
  .public_testnet_ready == false and
  .release_tag_authorized == false and
  .default_high_cadence_authorized == false and
  .smart_contract_activation_authorized == false and
  .result == "PASS"
' "$OUT_DIR/evidence.json" >/dev/null

cat "$OUT_DIR/evidence.json"
