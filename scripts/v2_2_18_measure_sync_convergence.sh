#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="v2.2.18"
RUN_ID="${PULSEDAG_SYNC_CONVERGENCE_RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)-sync-convergence}"
EVIDENCE_ROOT="${PULSEDAG_SYNC_CONVERGENCE_EVIDENCE_ROOT:-$ROOT_DIR/evidence/v2.2.18/sync-convergence}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
SAMPLE_INTERVAL_SECS="${PULSEDAG_SYNC_CONVERGENCE_SAMPLE_INTERVAL_SECS:-5}"
SAMPLE_COUNT="${PULSEDAG_SYNC_CONVERGENCE_SAMPLE_COUNT:-120}"
CURL_TIMEOUT_SECS="${PULSEDAG_SYNC_CONVERGENCE_CURL_TIMEOUT_SECS:-5}"

# Comma-separated node labels and RPC urls must align by index.
NODE_LABELS_CSV="${PULSEDAG_SYNC_CONVERGENCE_NODE_LABELS:-node-a,node-b,node-c}"
NODE_URLS_CSV="${PULSEDAG_SYNC_CONVERGENCE_NODE_URLS:-http://127.0.0.1:19480,http://127.0.0.1:19481,http://127.0.0.1:19482}"
RESTARTED_NODE="${PULSEDAG_SYNC_CONVERGENCE_RESTARTED_NODE:-node-b}"
ISOLATED_NODE="${PULSEDAG_SYNC_CONVERGENCE_ISOLATED_NODE:-node-c}"

SYNC_SAMPLES_CSV="$RUN_DIR/sync-samples.csv"
SYNC_SUMMARY_MD="$RUN_DIR/sync-summary.md"
CONVERGENCE_EVENTS_MD="$RUN_DIR/convergence-events.md"

section() { echo; echo "========== $* =========="; }
fatal() { echo "FAIL: $*" >&2; exit 1; }

http_get() { curl -fsS -m "$CURL_TIMEOUT_SECS" "$1"; }
json_field() {
  python3 -c 'import json,sys
obj=json.load(sys.stdin)
cur=obj
for part in sys.argv[1].split("."):
    if not part:
        continue
    if isinstance(cur, dict):
        cur=cur.get(part)
    elif isinstance(cur, list):
        try: cur=cur[int(part)]
        except Exception: cur=None
    else:
        cur=None
    if cur is None:
        print("")
        sys.exit(0)
if isinstance(cur,(dict,list)):
    print(json.dumps(cur,separators=(",",":"),sort_keys=True))
else:
    print(cur)' "$1"
}

require_tools() {
  command -v curl >/dev/null || fatal "curl is required"
  command -v python3 >/dev/null || fatal "python3 is required"
  [[ "$SAMPLE_INTERVAL_SECS" =~ ^[0-9]+$ ]] || fatal "sample interval must be numeric"
  [[ "$SAMPLE_COUNT" =~ ^[0-9]+$ ]] || fatal "sample count must be numeric"
}

split_csv_to_array() {
  local input="$1"; local -n out="$2"
  IFS=',' read -r -a out <<< "$input"
}

safe_endpoint() {
  local url="$1" endpoint="$2"
  if http_get "$url$endpoint" 2>/dev/null; then
    return 0
  fi
  echo '{"available":false}'
}

extract_height_like() {
  local status_json="$1"
  printf '%s' "$status_json" | json_field "data.best_height"
}

extract_tip_like() {
  local status_json="$1"
  local tip
  tip="$(printf '%s' "$status_json" | json_field "data.selected_tip")"
  if [[ -n "$tip" ]]; then
    printf '%s\n' "$tip"; return
  fi
  printf '%s' "$status_json" | json_field "data.tip"
}

extract_block_count_like() {
  local status_json="$1"
  local count
  count="$(printf '%s' "$status_json" | json_field "data.persisted_block_count")"
  if [[ -n "$count" ]]; then
    printf '%s\n' "$count"; return
  fi
  printf '%s' "$status_json" | json_field "data.block_count"
}

extract_peer_count_like() {
  local p2p_json="$1"
  local count
  count="$(printf '%s' "$p2p_json" | json_field "data.peer_count")"
  if [[ -n "$count" ]]; then
    printf '%s\n' "$count"; return
  fi
  count="$(printf '%s' "$p2p_json" | json_field "data.connected_peer_count")"
  if [[ -n "$count" ]]; then
    printf '%s\n' "$count"; return
  fi
  printf '%s' "$p2p_json" | python3 -c 'import json,sys
obj=json.load(sys.stdin).get("data") or {}
peers=obj.get("connected_peers")
print(len(peers) if isinstance(peers,list) else "")'
}

prepare_output() {
  mkdir -p "$RUN_DIR"
  cat > "$SYNC_SAMPLES_CSV" <<'CSV'
timestamp,node,status_ok,sync_ok,p2p_ok,dag_consistency_ok,height,tip,block_count,sync_lag_estimate,peer_count
CSV
  cat > "$CONVERGENCE_EVENTS_MD" <<'MD'
# Convergence Events

MD
}

capture_sample_for_node() {
  local ts="$1" idx="$2" label="$3" url="$4"
  local status_json sync_json p2p_json dag_json
  local status_ok sync_ok p2p_ok dag_ok height tip block_count sync_lag peer_count

  status_json="$(safe_endpoint "$url" "/status")"
  sync_json="$(safe_endpoint "$url" "/sync/status")"
  p2p_json="$(safe_endpoint "$url" "/p2p/status")"
  dag_json="$(safe_endpoint "$url" "/dag/consistency")"

  status_ok="$(printf '%s' "$status_json" | json_field "ok")"
  sync_ok="$(printf '%s' "$sync_json" | json_field "ok")"
  p2p_ok="$(printf '%s' "$p2p_json" | json_field "ok")"
  dag_ok="$(printf '%s' "$dag_json" | json_field "ok")"

  height="$(extract_height_like "$status_json")"
  tip="$(extract_tip_like "$status_json")"
  block_count="$(extract_block_count_like "$status_json")"
  sync_lag="$(printf '%s' "$sync_json" | json_field "data.sync_lag_estimate")"
  [[ -n "$sync_lag" ]] || sync_lag="$(printf '%s' "$sync_json" | json_field "data.lag")"
  peer_count="$(extract_peer_count_like "$p2p_json")"

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$ts" "$label" "${status_ok:-}" "${sync_ok:-}" "${p2p_ok:-}" "${dag_ok:-}" \
    "${height:-}" "${tip:-}" "${block_count:-}" "${sync_lag:-}" "${peer_count:-}" >> "$SYNC_SAMPLES_CSV"

  if [[ -n "$height" ]]; then
    HEIGHTS[$idx]="$height"
  else
    HEIGHTS[$idx]=""
  fi
}

declare -a NODE_LABELS=()
declare -a NODE_URLS=()
declare -a HEIGHTS=()

evaluate_convergence() {
  local ts="$1" baseline="" max_height=-1 min_height=-1 i h label
  for i in "${!HEIGHTS[@]}"; do
    h="${HEIGHTS[$i]}"
    [[ "$h" =~ ^[0-9]+$ ]] || continue
    if (( max_height < 0 || h > max_height )); then max_height=$h; fi
    if (( min_height < 0 || h < min_height )); then min_height=$h; fi
  done

  if (( min_height >= 0 && max_height >= 0 )); then
    if (( max_height - min_height <= 1 )); then
      printf -- '- %s UTC: converged height band [%s,%s] (delta=%s)\n' "$ts" "$min_height" "$max_height" "$((max_height-min_height))" >> "$CONVERGENCE_EVENTS_MD"
    else
      printf -- '- %s UTC: divergence detected height band [%s,%s] (delta=%s)\n' "$ts" "$min_height" "$max_height" "$((max_height-min_height))" >> "$CONVERGENCE_EVENTS_MD"
    fi
  fi

  for i in "${!NODE_LABELS[@]}"; do
    label="${NODE_LABELS[$i]}"
    if [[ "$label" == "$RESTARTED_NODE" || "$label" == "$ISOLATED_NODE" ]]; then
      printf -- '- %s UTC: observed %s height=%s during recovery tracking\n' "$ts" "$label" "${HEIGHTS[$i]:-n/a}" >> "$CONVERGENCE_EVENTS_MD"
    fi
  done
}

write_summary() {
  python3 - "$SYNC_SAMPLES_CSV" "$SYNC_SUMMARY_MD" "$RESTARTED_NODE" "$ISOLATED_NODE" <<'PY'
import csv, sys
samples, out_md, restarted, isolated = sys.argv[1:]
rows = list(csv.DictReader(open(samples, newline='')))
by_node = {}
for r in rows:
    by_node.setdefault(r['node'], []).append(r)

def numeric(v):
    try: return int(v)
    except: return None

all_heights = [numeric(r['height']) for r in rows if numeric(r['height']) is not None]
max_delta = 0
for ts in sorted(set(r['timestamp'] for r in rows)):
    hs = [numeric(r['height']) for r in rows if r['timestamp']==ts and numeric(r['height']) is not None]
    if hs:
        max_delta = max(max_delta, max(hs)-min(hs))

lines = ["# Sync Convergence Summary", "", f"- samples: {len(rows)}", f"- nodes: {len(by_node)}", f"- max height delta: {max_delta}"]

def recovered(node):
    rs = by_node.get(node, [])
    hs = [numeric(r['height']) for r in rs if numeric(r['height']) is not None]
    if len(hs) < 2: return False
    return abs(hs[-1]-hs[-2]) <= 1

startup_ok = (max_delta <= 1)
restart_ok = recovered(restarted)
isolation_ok = recovered(isolated)
persistent_divergence = (max_delta > 1)

audit = [
    ("nodes should converge after startup", startup_ok),
    ("restarted node should return to baseline", restart_ok),
    ("isolated node should recover after reconnect", isolation_ok),
    ("any persistent divergence is FAIL", not persistent_divergence),
]

lines += ["", "## Pass/Fail", ""]
for name, ok in audit:
    lines.append(f"- {'PASS' if ok else 'FAIL'}: {name}")

open(out_md, 'w').write("\n".join(lines)+"\n")
PY
}

main() {
  require_tools
  split_csv_to_array "$NODE_LABELS_CSV" NODE_LABELS
  split_csv_to_array "$NODE_URLS_CSV" NODE_URLS
  [[ "${#NODE_LABELS[@]}" -gt 0 ]] || fatal "at least one node label required"
  [[ "${#NODE_LABELS[@]}" -eq "${#NODE_URLS[@]}" ]] || fatal "node labels and urls must match in count"
  HEIGHTS=("${NODE_LABELS[@]}")
  prepare_output

  section "sampling sync convergence"
  for ((sample=1; sample<=SAMPLE_COUNT; sample++)); do
    ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for i in "${!NODE_LABELS[@]}"; do
      capture_sample_for_node "$ts" "$i" "${NODE_LABELS[$i]}" "${NODE_URLS[$i]}"
    done
    evaluate_convergence "$ts"
    sleep "$SAMPLE_INTERVAL_SECS"
  done

  write_summary
  echo "[ok] wrote $SYNC_SAMPLES_CSV"
  echo "[ok] wrote $SYNC_SUMMARY_MD"
  echo "[ok] wrote $CONVERGENCE_EVENTS_MD"
}

main "$@"
