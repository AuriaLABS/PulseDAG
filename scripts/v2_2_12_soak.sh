#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_12_common.sh"

NODES=(a b c)
ENDPOINTS=(/status /p2p/status /sync/status)

DURATION_SECS="${PULSEDAG_SOAK_DURATION_SECS:-${PULSEDAG_REHEARSAL_SOAK_DURATION_SECS:-120}}"
INTERVAL_SECS="${PULSEDAG_SOAK_INTERVAL_SECS:-${PULSEDAG_REHEARSAL_SOAK_INTERVAL_SECS:-10}}"
CONVERGENCE_LOSS_THRESHOLD_SECS="${PULSEDAG_SOAK_CONVERGENCE_LOSS_THRESHOLD_SECS:-${PULSEDAG_REHEARSAL_CONVERGENCE_LOSS_THRESHOLD_SECS:-45}}"
MAX_HEIGHT_LAG="${PULSEDAG_SOAK_MAX_HEIGHT_LAG:-${PULSEDAG_REHEARSAL_MAX_HEIGHT_LAG:-2}}"
MIN_CONNECTED_PEERS="${PULSEDAG_SOAK_MIN_CONNECTED_PEERS:-0}"
START_STACK="${PULSEDAG_SOAK_START:-0}"
BUILD_BEFORE_START="${PULSEDAG_SOAK_BUILD:-0}"
CLEAN_DATA_ON_START="${PULSEDAG_SOAK_CLEAN_DATA:-0}"
KEEP_RUNNING="${PULSEDAG_SOAK_KEEP_RUNNING:-1}"
TIMESTAMP="${PULSEDAG_REHEARSAL_EVIDENCE_TIMESTAMP:-$(date -u +%Y%m%dT%H%M%SZ)}"
EVIDENCE_ROOT="${PULSEDAG_REHEARSAL_EVIDENCE_DIR:-$STATE_DIR/evidence}"
EVIDENCE_DIR="${PULSEDAG_SOAK_EVIDENCE_DIR:-$EVIDENCE_ROOT/v2_2_12_soak_$TIMESTAMP}"
SNAPSHOT_DIR="$EVIDENCE_DIR/snapshots"
SUMMARY_JSONL="$EVIDENCE_DIR/summary.jsonl"
SUMMARY_FILE="$EVIDENCE_DIR/summary.md"

started_stack=0
convergence_lost_since=""
failures=0
samples=0

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--start] [--duration SECS] [--interval SECS] [--threshold SECS]

Sustained v2.2.12 A/B/C rehearsal soak monitor. By default this script assumes
node A/B/C and miner-a are already running and polls their RPC endpoints. Set
--start or PULSEDAG_SOAK_START=1 to start node A/B/C plus miner-a first.

What is collected every interval:
  - GET /status, /p2p/status, and /sync/status from node A/B/C
  - best_height per node and chain_id consistency
  - connected peer count
  - pending_block_requests, pending_missing_parents, and orphan_count
  - duplicate counters when exposed by /p2p/status
  - chain mismatch drops when exposed by /p2p/status or /sync/status
  - last rejected peer block reason when exposed by diagnostics/status payloads

Outputs:
  $EVIDENCE_DIR/snapshots/<timestamp>.json     timestamped raw + normalized JSON snapshots
  $SUMMARY_JSONL                              one normalized JSON summary per sample
  $SUMMARY_FILE                               operator-readable run summary

Config environment variables:
  PULSEDAG_SOAK_DURATION_SECS                 default: $DURATION_SECS
  PULSEDAG_SOAK_INTERVAL_SECS                 default: $INTERVAL_SECS
  PULSEDAG_SOAK_CONVERGENCE_LOSS_THRESHOLD_SECS default: $CONVERGENCE_LOSS_THRESHOLD_SECS
  PULSEDAG_SOAK_MAX_HEIGHT_LAG                default: $MAX_HEIGHT_LAG
  PULSEDAG_SOAK_MIN_CONNECTED_PEERS           default: $MIN_CONNECTED_PEERS
  PULSEDAG_SOAK_START                         default: $START_STACK (1 starts A/B/C and miner-a)
  PULSEDAG_SOAK_BUILD                         default: $BUILD_BEFORE_START (1 runs cargo build --workspace --release before --start)
  PULSEDAG_SOAK_CLEAN_DATA                    default: $CLEAN_DATA_ON_START (1 cleans node data when starting)
  PULSEDAG_SOAK_KEEP_RUNNING                  default: $KEEP_RUNNING (0 stops processes started by this script on exit)
  PULSEDAG_SOAK_EVIDENCE_DIR                  default: $EVIDENCE_DIR

Long private-testnet rehearsal example:
  PULSEDAG_SOAK_DURATION_SECS=14400 \
  PULSEDAG_SOAK_INTERVAL_SECS=30 \
  PULSEDAG_SOAK_CONVERGENCE_LOSS_THRESHOLD_SECS=300 \
  PULSEDAG_SOAK_MAX_HEIGHT_LAG=5 \
  PULSEDAG_SOAK_MIN_CONNECTED_PEERS=1 \
  scripts/v2_2_12_soak.sh

Local start example:
  PULSEDAG_SOAK_START=1 PULSEDAG_SOAK_BUILD=1 scripts/v2_2_12_soak.sh
USAGE
}

fail() { echo "[error] $*" >&2; exit 1; }
warn() { echo "[warn] $*" >&2; }

require_positive_int() {
  local name="$1" value="$2"
  [[ "$value" =~ ^[0-9]+$ && "$value" -gt 0 ]] || fail "$name must be a positive integer, got '$value'"
}

require_nonnegative_int() {
  local name="$1" value="$2"
  [[ "$value" =~ ^[0-9]+$ ]] || fail "$name must be a non-negative integer, got '$value'"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --start) START_STACK=1; shift ;;
    --duration) DURATION_SECS="$2"; shift 2 ;;
    --interval) INTERVAL_SECS="$2"; shift 2 ;;
    --threshold) CONVERGENCE_LOSS_THRESHOLD_SECS="$2"; shift 2 ;;
    --max-height-lag) MAX_HEIGHT_LAG="$2"; shift 2 ;;
    --min-connected-peers) MIN_CONNECTED_PEERS="$2"; shift 2 ;;
    --evidence-dir) EVIDENCE_DIR="$2"; SNAPSHOT_DIR="$EVIDENCE_DIR/snapshots"; SUMMARY_JSONL="$EVIDENCE_DIR/summary.jsonl"; SUMMARY_FILE="$EVIDENCE_DIR/summary.md"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown argument: $1" ;;
  esac
done

require_positive_int PULSEDAG_SOAK_DURATION_SECS "$DURATION_SECS"
require_positive_int PULSEDAG_SOAK_INTERVAL_SECS "$INTERVAL_SECS"
require_positive_int PULSEDAG_SOAK_CONVERGENCE_LOSS_THRESHOLD_SECS "$CONVERGENCE_LOSS_THRESHOLD_SECS"
require_nonnegative_int PULSEDAG_SOAK_MAX_HEIGHT_LAG "$MAX_HEIGHT_LAG"
require_nonnegative_int PULSEDAG_SOAK_MIN_CONNECTED_PEERS "$MIN_CONNECTED_PEERS"

command -v curl >/dev/null || fail "curl is required"
command -v python3 >/dev/null || fail "python3 is required"

cleanup() {
  if [[ "$started_stack" == "1" && !( "$KEEP_RUNNING" == "1" || "$KEEP_RUNNING" == "true" ) ]]; then
    echo "[info] stopping processes started by soak because PULSEDAG_SOAK_KEEP_RUNNING=$KEEP_RUNNING"
    stop_all_v2_2_12
  elif [[ "$started_stack" == "1" ]]; then
    echo "[info] leaving started processes running because PULSEDAG_SOAK_KEEP_RUNNING=$KEEP_RUNNING"
  fi
}
trap cleanup EXIT

wait_for_http_ok() {
  local node="$1" endpoint="$2" wait_timeout="${3:-60}" deadline url
  deadline=$((SECONDS + wait_timeout))
  url="$(rpc_url "$node")$endpoint"
  until http_get "$url" >/dev/null 2>&1; do
    (( SECONDS < deadline )) || fail "timed out waiting for $url"
    sleep 2
  done
  echo "[ok] node-$node $endpoint reachable"
}

start_rehearsal_stack() {
  started_stack=1
  if [[ "$BUILD_BEFORE_START" == "1" || "$BUILD_BEFORE_START" == "true" ]]; then
    command -v cargo >/dev/null || fail "cargo is required when PULSEDAG_SOAK_BUILD=1"
    echo "[info] building workspace release binaries"
    (cd "$ROOT_DIR" && cargo build --workspace --release)
  fi

  export PULSEDAG_CLEAN_DATA="$CLEAN_DATA_ON_START"
  stop_all_v2_2_12
  start_node a
  wait_for_http_ok a /p2p/status

  local bootnode_a peer_id bootnode_b
  bootnode_a="$(node_bootnode_a)"
  if [[ -z "${PULSEDAG_NODE_A_BOOTNODE:-}" && "$bootnode_a" != */p2p/* ]]; then
    peer_id="$(node_peer_id a)"
    [[ -n "$peer_id" ]] || fail "node-a did not expose a peer_id for bootnode dialing"
    bootnode_a="$bootnode_a/p2p/$peer_id"
  fi

  start_node b --bootnode "$bootnode_a"
  wait_for_http_ok b /p2p/status
  bootnode_b="$(node_p2p b | sed 's#/ip4/0\.0\.0\.0/#/ip4/127.0.0.1/#')/p2p/$(node_peer_id b)"

  start_node c --bootnode "$bootnode_a" --bootnode "$bootnode_b"
  wait_for_http_ok c /p2p/status
  unset PULSEDAG_CLEAN_DATA
  start_miner_a
}

write_run_summary_header() {
  mkdir -p "$SNAPSHOT_DIR"
  : > "$SUMMARY_JSONL"
  cat > "$SUMMARY_FILE" <<SUMMARY
# v2.2.12 Sustained Rehearsal Soak

- Started at UTC: $TIMESTAMP
- Evidence directory: $EVIDENCE_DIR
- Duration seconds: $DURATION_SECS
- Poll interval seconds: $INTERVAL_SECS
- Convergence loss threshold seconds: $CONVERGENCE_LOSS_THRESHOLD_SECS
- Max height lag: $MAX_HEIGHT_LAG
- Minimum connected peers per node: $MIN_CONNECTED_PEERS
- Started stack: $START_STACK
- Chain id expected by launcher: $CHAIN_ID
- Node A RPC: $(rpc_url a)
- Node B RPC: $(rpc_url b)
- Node C RPC: $(rpc_url c)

Operators can run a longer private-testnet rehearsal by increasing
\`PULSEDAG_SOAK_DURATION_SECS\` (for example 14400 for four hours), using a
larger \`PULSEDAG_SOAK_INTERVAL_SECS\` such as 30, and setting
\`PULSEDAG_SOAK_CONVERGENCE_LOSS_THRESHOLD_SECS\` to the maximum acceptable
sustained lag window for the testnet.

## Samples
SUMMARY
}

append_summary_line() {
  local sample_file="$1" status="$2"
  python3 - "$sample_file" "$SUMMARY_FILE" "$status" <<'PY'
import json, sys
sample_file, summary_file, status = sys.argv[1:4]
with open(sample_file, encoding="utf-8") as fh:
    sample = json.load(fh)
parts = []
for name in ("a", "b", "c"):
    n = sample["nodes"].get(name, {})
    parts.append(
        f"node-{name}: height={n.get('best_height')} chain_id={n.get('chain_id')} "
        f"peers={n.get('connected_peer_count')} pending_block_requests={n.get('pending_block_requests')} "
        f"pending_missing_parents={n.get('pending_missing_parents')} orphan_count={n.get('orphan_count')}"
    )
with open(summary_file, "a", encoding="utf-8") as out:
    out.write(f"- {sample['timestamp_utc']} [{status}] convergence={sample['convergence']['ok']} " + "; ".join(parts) + "\n")
PY
}

collect_snapshot() {
  local ts compact_ts sample_file tmp_dir
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  compact_ts="$(date -u +%Y%m%dT%H%M%SZ)"
  sample_file="$SNAPSHOT_DIR/$compact_ts.json"
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/pulsedag-soak.XXXXXX")"

  for node in "${NODES[@]}"; do
    mkdir -p "$tmp_dir/$node"
    for endpoint in "${ENDPOINTS[@]}"; do
      local slug url out err rc_file
      slug="${endpoint#/}"
      slug="${slug//\//_}"
      url="$(rpc_url "$node")$endpoint"
      out="$tmp_dir/$node/$slug.json"
      err="$tmp_dir/$node/$slug.stderr"
      rc_file="$tmp_dir/$node/$slug.rc"
      if curl -fsS -m "$CURL_TIMEOUT_SECS" "$url" -o "$out" 2>"$err"; then
        echo 0 > "$rc_file"
      else
        local rc=$?
        echo "$rc" > "$rc_file"
        rm -f "$out"
      fi
    done
  done

  python3 - "$tmp_dir" "$sample_file" "$ts" "${NODES[*]}" "$MAX_HEIGHT_LAG" "$MIN_CONNECTED_PEERS" <<'PY'
import json, os, sys
from pathlib import Path

tmp_dir = Path(sys.argv[1])
sample_file = Path(sys.argv[2])
ts = sys.argv[3]
nodes = sys.argv[4].split()
max_height_lag = int(sys.argv[5])
min_connected_peers = int(sys.argv[6])

ENDPOINT_SLUGS = {"status": "status", "p2p_status": "p2p/status", "sync_status": "sync/status"}
MISSING = None

def deep_get(obj, *paths):
    for path in paths:
        cur = obj
        ok = True
        for part in path.split("."):
            if part == "":
                continue
            if isinstance(cur, dict) and part in cur:
                cur = cur[part]
            else:
                ok = False
                break
        if ok and cur is not None:
            return cur
    return MISSING

def as_int(value):
    if value is None or value == "":
        return None
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, (int, float)):
        return int(value)
    try:
        return int(value)
    except (TypeError, ValueError):
        return None

def load_endpoint(node, slug):
    base = tmp_dir / node / slug
    rc_path = base.with_suffix(".rc")
    err_path = base.with_suffix(".stderr")
    json_path = base.with_suffix(".json")
    rc = int(rc_path.read_text().strip()) if rc_path.exists() else 99
    err = err_path.read_text(errors="replace") if err_path.exists() else ""
    body = None
    parse_error = None
    if json_path.exists():
        try:
            body = json.loads(json_path.read_text(encoding="utf-8"))
        except Exception as exc:  # keep malformed endpoint output in evidence
            parse_error = str(exc)
    return {"ok": rc == 0 and body is not None, "curl_rc": rc, "stderr": err, "json": body, "parse_error": parse_error}

def connected_peer_count(p2p):
    peers = deep_get(p2p, "data.connected_peers", "connected_peers", "data.peers", "peers")
    if isinstance(peers, list):
        return len(peers)
    if isinstance(peers, dict):
        return len(peers)
    count = deep_get(p2p, "data.connected_peer_count", "connected_peer_count", "data.peer_count", "peer_count")
    return as_int(count)

def counter_family(obj, *names):
    out = {}
    for name in names:
        value = deep_get(obj, f"data.{name}", name)
        if value is not None:
            out[name] = value
    return out

snapshot = {
    "timestamp_utc": ts,
    "evidence_file": str(sample_file),
    "nodes": {},
    "convergence": {"ok": False, "reasons": []},
    "raw_endpoints": {},
}
heights = []
chain_ids = []

for node in nodes:
    status_ep = load_endpoint(node, "status")
    p2p_ep = load_endpoint(node, "p2p_status")
    sync_ep = load_endpoint(node, "sync_status")
    status = status_ep.get("json") or {}
    p2p = p2p_ep.get("json") or {}
    sync = sync_ep.get("json") or {}

    height = as_int(deep_get(status, "data.best_height", "best_height", "data.height", "height"))
    chain_id = deep_get(status, "data.chain_id", "chain_id")
    peer_count = connected_peer_count(p2p)
    pending_block_requests = as_int(deep_get(sync, "data.pending_block_requests", "pending_block_requests"))
    pending_missing_parents = as_int(deep_get(sync, "data.pending_missing_parents", "pending_missing_parents"))
    orphan_count = as_int(deep_get(sync, "data.orphan_count", "orphan_count", "data.orphans", "orphans"))
    duplicate_counters = counter_family(
        p2p,
        "duplicate_suppression_counters",
        "block_propagation_counters",
        "inbound_duplicates_suppressed",
        "block_outbound_duplicates_suppressed",
    )
    chain_mismatch_drops = counter_family(
        p2p,
        "inbound_chain_mismatch_dropped",
        "chain_id_mismatch_drops",
        "last_drop_reason",
    )
    chain_mismatch_drops.update(counter_family(sync, "chain_id_mismatch_drops", "last_drop_reason"))
    last_rejected_reason = deep_get(
        status,
        "data.last_rejected_peer_block_reason",
        "last_rejected_peer_block_reason",
        "data.diagnostics.last_rejected_peer_block_reason",
        "diagnostics.last_rejected_peer_block_reason",
    )
    if last_rejected_reason is None:
        last_rejected_reason = deep_get(sync, "data.last_rejected_peer_block_reason", "last_rejected_peer_block_reason")

    node_summary = {
        "rpc_url": None,
        "status_ok": status_ep["ok"],
        "p2p_status_ok": p2p_ep["ok"],
        "sync_status_ok": sync_ep["ok"],
        "best_height": height,
        "chain_id": chain_id,
        "connected_peer_count": peer_count,
        "pending_block_requests": pending_block_requests,
        "pending_missing_parents": pending_missing_parents,
        "orphan_count": orphan_count,
        "duplicate_counters": duplicate_counters,
        "chain_mismatch_drops": chain_mismatch_drops,
        "last_rejected_peer_block_reason": last_rejected_reason,
    }
    snapshot["nodes"][node] = node_summary
    snapshot["raw_endpoints"][node] = {
        "/status": status_ep,
        "/p2p/status": p2p_ep,
        "/sync/status": sync_ep,
    }
    if height is not None:
        heights.append(height)
    else:
        snapshot["convergence"]["reasons"].append(f"node-{node} missing best_height")
    if chain_id:
        chain_ids.append(str(chain_id))
    else:
        snapshot["convergence"]["reasons"].append(f"node-{node} missing chain_id")
    if peer_count is None:
        snapshot["convergence"]["reasons"].append(f"node-{node} missing connected peer count")
    elif peer_count < min_connected_peers:
        snapshot["convergence"]["reasons"].append(f"node-{node} connected peers {peer_count} below minimum {min_connected_peers}")

if len(heights) == len(nodes):
    lag = max(heights) - min(heights)
    snapshot["convergence"]["height_lag"] = lag
    snapshot["convergence"]["min_height"] = min(heights)
    snapshot["convergence"]["max_height"] = max(heights)
    if lag > max_height_lag:
        snapshot["convergence"]["reasons"].append(f"height lag {lag} exceeds max {max_height_lag}")
else:
    snapshot["convergence"]["height_lag"] = None

if len(chain_ids) == len(nodes):
    unique_chain_ids = sorted(set(chain_ids))
    snapshot["convergence"]["chain_ids"] = unique_chain_ids
    if len(unique_chain_ids) != 1:
        snapshot["convergence"]["reasons"].append(f"chain_id mismatch: {unique_chain_ids}")

snapshot["convergence"]["ok"] = not snapshot["convergence"]["reasons"]
sample_file.parent.mkdir(parents=True, exist_ok=True)
sample_file.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(json.dumps(snapshot, sort_keys=True))
PY

  rm -rf "$tmp_dir"
}

if [[ "$START_STACK" == "1" || "$START_STACK" == "true" ]]; then
  start_rehearsal_stack
else
  echo "[info] assuming node A/B/C and miner-a are already running; set PULSEDAG_SOAK_START=1 to start them"
fi

write_run_summary_header

echo "[info] writing v2.2.12 soak evidence under $EVIDENCE_DIR"
echo "[info] duration=${DURATION_SECS}s interval=${INTERVAL_SECS}s threshold=${CONVERGENCE_LOSS_THRESHOLD_SECS}s max_height_lag=$MAX_HEIGHT_LAG"

end_epoch=$(( $(date +%s) + DURATION_SECS ))
while :; do
  now_epoch="$(date +%s)"
  (( now_epoch <= end_epoch )) || break

  sample_json="$(collect_snapshot)"
  sample_file="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["evidence_file"])' <<<"$sample_json")"
  printf '%s\n' "$sample_json" >> "$SUMMARY_JSONL"
  samples=$((samples + 1))

  convergence_ok="$(python3 -c 'import json,sys; print("true" if json.load(sys.stdin)["convergence"]["ok"] else "false")' <<<"$sample_json")"
  heights_line="$(python3 -c 'import json,sys; s=json.load(sys.stdin); print(" ".join("{}={}".format(n, d.get("best_height")) for n,d in s["nodes"].items()))' <<<"$sample_json")"

  if [[ "$convergence_ok" == "true" ]]; then
    convergence_lost_since=""
    echo "[ok] sample $samples convergence ok ($heights_line)"
    append_summary_line "$sample_file" ok
  else
    reasons="$(python3 -c 'import json,sys; print("; ".join(json.load(sys.stdin)["convergence"]["reasons"]))' <<<"$sample_json")"
    [[ -n "$convergence_lost_since" ]] || convergence_lost_since="$now_epoch"
    lost_for=$(( now_epoch - convergence_lost_since ))
    warn "sample $samples convergence lost for ${lost_for}s: $reasons"
    append_summary_line "$sample_file" warn
    if (( lost_for >= CONVERGENCE_LOSS_THRESHOLD_SECS )); then
      failures=$((failures + 1))
      echo "[error] convergence lost longer than ${CONVERGENCE_LOSS_THRESHOLD_SECS}s" >&2
      break
    fi
  fi

  next_epoch=$(( now_epoch + INTERVAL_SECS ))
  remaining=$(( end_epoch - $(date +%s) ))
  (( remaining > 0 )) || break
  sleep_for=$INTERVAL_SECS
  (( sleep_for <= remaining )) || sleep_for=$remaining
  sleep "$sleep_for"
done

cat >> "$SUMMARY_FILE" <<SUMMARY

## Result

- Completed at UTC: $(date -u +%Y-%m-%dT%H:%M:%SZ)
- Samples collected: $samples
- Failures: $failures
- Summary JSONL: $SUMMARY_JSONL
- Snapshot directory: $SNAPSHOT_DIR
SUMMARY

if (( failures > 0 )); then
  fail "v2.2.12 soak failed; evidence saved under $EVIDENCE_DIR"
fi

echo "[ok] v2.2.12 soak completed; evidence saved under $EVIDENCE_DIR"
