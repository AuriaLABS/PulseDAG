#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="v2.2.15"
CHAIN_ID_MAIN="${PULSEDAG_CHAIN_ID_ISOLATION_MAIN:-pulsedag-v2-2-15-isolation-main}"
CHAIN_ID_FOREIGN="${PULSEDAG_CHAIN_ID_ISOLATION_FOREIGN:-pulsedag-v2-2-15-isolation-foreign}"
RPC_BASE_PORT="${PULSEDAG_CHAIN_ID_ISOLATION_RPC_BASE_PORT:-19280}"
P2P_BASE_PORT="${PULSEDAG_CHAIN_ID_ISOLATION_P2P_BASE_PORT:-19380}"
STARTUP_WAIT_SECS="${PULSEDAG_CHAIN_ID_ISOLATION_STARTUP_WAIT_SECS:-60}"
PEER_WAIT_SECS="${PULSEDAG_CHAIN_ID_ISOLATION_PEER_WAIT_SECS:-45}"
CURL_TIMEOUT_SECS="${PULSEDAG_CHAIN_ID_ISOLATION_CURL_TIMEOUT_SECS:-5}"
P2P_MODE="${PULSEDAG_CHAIN_ID_ISOLATION_P2P_MODE:-libp2p-real}"
EVIDENCE_ROOT="${PULSEDAG_CHAIN_ID_ISOLATION_EVIDENCE_ROOT:-$ROOT_DIR/evidence/v2.2.15/chain-id-isolation}"
RUN_ID="${PULSEDAG_CHAIN_ID_ISOLATION_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RUNTIME_ROOT="$RUN_DIR/runtime"
DATA_ROOT="$RUNTIME_ROOT/data"
LOG_DIR="$RUN_DIR/logs"
NODE_BIN="${PULSEDAGD_BIN:-}"
FAILURES=0

if [[ -z "$NODE_BIN" ]]; then
  if [[ -x "$ROOT_DIR/target/release/pulsedagd" ]]; then NODE_BIN="$ROOT_DIR/target/release/pulsedagd";
  elif [[ -x "$ROOT_DIR/target/debug/pulsedagd" ]]; then NODE_BIN="$ROOT_DIR/target/debug/pulsedagd";
  else NODE_BIN="$ROOT_DIR/target/debug/pulsedagd"; fi
fi

section() { echo; echo "========== $* =========="; }
pass() { echo "PASS: $*"; }
fail_msg() { echo "FAIL: $*" >&2; FAILURES=$((FAILURES + 1)); }
fatal() { echo "FAIL: $*" >&2; exit 1; }
node_name() { case "$1" in 1) echo main-a;; 2) echo main-b;; 3) echo foreign-c;; esac; }
rpc_port() { echo $((RPC_BASE_PORT + $1 - 1)); }
p2p_port() { echo $((P2P_BASE_PORT + $1 - 1)); }
rpc_addr() { echo "127.0.0.1:$(rpc_port "$1")"; }
rpc_url() { echo "http://$(rpc_addr "$1")"; }
p2p_listen() { echo "/ip4/0.0.0.0/tcp/$(p2p_port "$1")"; }
p2p_dial() { echo "/ip4/127.0.0.1/tcp/$(p2p_port "$1")"; }
node_dir() { echo "$DATA_ROOT/$(node_name "$1")"; }
node_db() { echo "$(node_dir "$1")/rocksdb"; }
pid_file() { echo "$RUNTIME_ROOT/$(node_name "$1").pid"; }
log_file() { echo "$LOG_DIR/$(node_name "$1").log"; }
node_chain() { [[ "$1" == "3" ]] && echo "$CHAIN_ID_FOREIGN" || echo "$CHAIN_ID_MAIN"; }

http_get() { curl -fsS -m "$CURL_TIMEOUT_SECS" "$1"; }
json_field() {
  python3 -c 'import json,sys
obj=json.load(sys.stdin); cur=obj
for part in sys.argv[1].split("."):
    if part == "": continue
    cur = cur[int(part)] if isinstance(cur, list) else cur.get(part) if isinstance(cur, dict) else None
    if cur is None:
        print(""); sys.exit(0)
print(cur if not isinstance(cur,(dict,list)) else json.dumps(cur, sort_keys=True))' "$1"
}
pretty_json_file() { python3 -m json.tool "$1" > "$2.tmp" && mv "$2.tmp" "$2"; }

require_tools() {
  command -v curl >/dev/null || fatal "curl is required"
  command -v python3 >/dev/null || fatal "python3 is required"
  [[ -x "$NODE_BIN" ]] || fatal "missing pulsedagd binary at $NODE_BIN. Run cargo build --workspace or set PULSEDAGD_BIN."
}

is_pid_running() { [[ -n "${1:-}" ]] && kill -0 "$1" 2>/dev/null; }
stop_node() {
  local node="$1" file pid
  file="$(pid_file "$node")"; [[ -f "$file" ]] || return 0
  pid="$(cat "$file")"
  if is_pid_running "$pid"; then
    kill "$pid" 2>/dev/null || true
    for _ in {1..20}; do is_pid_running "$pid" || break; sleep 1; done
    is_pid_running "$pid" && kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$file"
}
cleanup() { for node in 3 2 1; do stop_node "$node"; done; }
trap cleanup EXIT

start_node() {
  local node="$1" name rpc p2p db log chain cmd
  name="$(node_name "$node")"; rpc="$(rpc_addr "$node")"; p2p="$(p2p_listen "$node")"; db="$(node_db "$node")"; log="$(log_file "$node")"; chain="$(node_chain "$node")"
  mkdir -p "$db" "$LOG_DIR" "$RUN_DIR/$name"
  cmd=("$NODE_BIN" --network private --rpc-listen "$rpc" --p2p-listen "$p2p")
  if [[ "$node" != "1" ]]; then cmd+=(--bootnode "$(p2p_dial 1)"); fi
  echo "[info] starting $name chain_id=$chain rpc=$rpc p2p=$p2p"
  (
    export PULSEDAG_NETWORK_PROFILE="chain-id-isolation-$VERSION-$name"
    export PULSEDAG_CHAIN_ID="$chain"
    export PULSEDAG_P2P_ENABLED="true"
    export PULSEDAG_P2P_MODE="$P2P_MODE"
    export PULSEDAG_P2P_MDNS="false"
    export PULSEDAG_P2P_KADEMLIA="true"
    export PULSEDAG_ROCKSDB_PATH="$db"
    export PULSEDAG_P2P_CONNECTION_SLOT_BUDGET="16"
    export RUST_LOG="${RUST_LOG:-info}"
    exec "${cmd[@]}"
  ) >"$log" 2>&1 &
  echo "$!" >"$(pid_file "$node")"
}

wait_for_endpoint() {
  local node="$1" endpoint="$2" deadline=$((SECONDS + STARTUP_WAIT_SECS))
  until http_get "$(rpc_url "$node")$endpoint" >/dev/null 2>&1; do
    if (( SECONDS >= deadline )); then tail -100 "$(log_file "$node")" >&2 || true; fatal "timed out waiting for $(node_name "$node") $endpoint"; fi
    sleep 2
  done
}

collect_node_evidence() {
  local node="$1"
  local phase="$2"
  local dir="$RUN_DIR/$(node_name "$node")/$phase"
  local endpoint file
  mkdir -p "$dir"
  for endpoint in health status p2p/status p2p/peers sync/status; do
    file="${endpoint//\//-}.json"
    if http_get "$(rpc_url "$node")/$endpoint" >"$dir/$file.raw" 2>"$dir/$file.err"; then
      pretty_json_file "$dir/$file.raw" "$dir/$file"; rm -f "$dir/$file.raw" "$dir/$file.err"
    fi
  done
}

peer_count() { http_get "$(rpc_url "$1")/p2p/status" | json_field data.peer_count; }
status_chain() { http_get "$(rpc_url "$1")/p2p/status" | json_field data.chain_id; }
chain_mismatch_drops() { http_get "$(rpc_url "$1")/p2p/status" | json_field data.inbound_chain_mismatch_dropped; }
compatible_count() { http_get "$(rpc_url "$1")/p2p/status" | json_field data.peer_state_summary.chain_compatible; }
incompatible_count() { http_get "$(rpc_url "$1")/p2p/status" | json_field data.peer_state_summary.chain_incompatible_or_unknown; }

write_summary() {
  local summary="$RUN_DIR/summary.md"
  {
    echo "# PulseDAG $VERSION chain-id isolation evidence"
    echo
    echo "- Date (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- Main chain id: $CHAIN_ID_MAIN"
    echo "- Foreign chain id: $CHAIN_ID_FOREIGN"
    echo "- P2P mode: $P2P_MODE"
    echo "- Result: $([[ $FAILURES -eq 0 ]] && echo PASS || echo FAIL)"
    echo
    echo "| node | reported_chain_id | peer_count | compatible_peers | incompatible_or_unknown | mismatch_drops |"
    echo "| --- | --- | ---: | ---: | ---: | ---: |"
    for node in 1 2 3; do
      echo "| $(node_name "$node") | $(status_chain "$node" 2>/dev/null || echo unavailable) | $(peer_count "$node" 2>/dev/null || echo 0) | $(compatible_count "$node" 2>/dev/null || echo 0) | $(incompatible_count "$node" 2>/dev/null || echo 0) | $(chain_mismatch_drops "$node" 2>/dev/null || echo 0) |"
    done
    echo
    echo "Evidence directory: $RUN_DIR"
  } >"$summary"
  echo "[info] wrote $summary"
}

main() {
  require_tools
  mkdir -p "$RUN_DIR" "$RUNTIME_ROOT" "$DATA_ROOT" "$LOG_DIR"
  cat >"$RUN_DIR/config.json" <<JSON
{"version":"$VERSION","main_chain_id":"$CHAIN_ID_MAIN","foreign_chain_id":"$CHAIN_ID_FOREIGN","p2p_mode":"$P2P_MODE","rpc_base_port":$RPC_BASE_PORT,"p2p_base_port":$P2P_BASE_PORT,"node_binary":"$NODE_BIN","run_id":"$RUN_ID"}
JSON
  section "start nodes"
  start_node 1; sleep 2; start_node 2; sleep 2; start_node 3
  for node in 1 2 3; do wait_for_endpoint "$node" /status; wait_for_endpoint "$node" /p2p/status; done
  section "collect evidence"
  sleep "$PEER_WAIT_SECS"
  for node in 1 2 3; do collect_node_evidence "$node" final; done
  section "assert isolation"
  [[ "$(status_chain 1)" == "$CHAIN_ID_MAIN" ]] || fail_msg "main-a reports wrong chain_id"
  [[ "$(status_chain 2)" == "$CHAIN_ID_MAIN" ]] || fail_msg "main-b reports wrong chain_id"
  [[ "$(status_chain 3)" == "$CHAIN_ID_FOREIGN" ]] || fail_msg "foreign-c reports wrong chain_id"
  local main_a_peers main_b_peers foreign_peers
  main_a_peers="$(peer_count 1 || echo 0)"; main_b_peers="$(peer_count 2 || echo 0)"; foreign_peers="$(peer_count 3 || echo 0)"
  if [[ "$main_a_peers" =~ ^[0-9]+$ && "$main_b_peers" =~ ^[0-9]+$ && "$foreign_peers" =~ ^[0-9]+$ ]]; then
    (( main_a_peers <= 1 )) || fail_msg "main-a has unexpected peer_count=$main_a_peers"
    (( main_b_peers <= 1 )) || fail_msg "main-b has unexpected peer_count=$main_b_peers"
    (( foreign_peers == 0 )) || fail_msg "foreign node has compatible peer_count=$foreign_peers"
  else
    fail_msg "could not parse peer counts"
  fi
  write_summary
  if (( FAILURES > 0 )); then exit 1; fi
  pass "chain-id isolation evidence complete: $RUN_DIR"
}

main "$@"
