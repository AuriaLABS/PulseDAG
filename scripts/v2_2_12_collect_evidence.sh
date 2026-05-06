#!/usr/bin/env bash
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/v2_2_12_common.sh"

TIMESTAMP="${PULSEDAG_REHEARSAL_EVIDENCE_TIMESTAMP:-$(date -u +%Y%m%dT%H%M%SZ)}"
EVIDENCE_ROOT="${PULSEDAG_REHEARSAL_EVIDENCE_DIR:-$STATE_DIR/evidence}"
EVIDENCE_DIR="$EVIDENCE_ROOT/v2_2_12_rehearsal_$TIMESTAMP"
RESPONSES_DIR="$EVIDENCE_DIR/responses"
LOGS_OUT_DIR="$EVIDENCE_DIR/logs"
SUMMARY_FILE="$EVIDENCE_DIR/environment_summary.md"
MANIFEST_FILE="$EVIDENCE_DIR/MANIFEST.txt"
ARCHIVE_PATH="$EVIDENCE_ROOT/v2_2_12_rehearsal_$TIMESTAMP.tar.gz"

NODES=(a b c)
REQUIRED_ENDPOINTS=(/health /status /p2p/status /sync/status)
OPTIONAL_ENDPOINTS=(/sync/missing /p2p/propagation /p2p/peers /p2p/topics)

failures=0

usage() {
  cat <<USAGE
Usage: $(basename "$0")

Collect v2.2.12 rehearsal closeout evidence from node A/B/C RPC endpoints,
copy rehearsal logs, write an environment/config summary, and produce a tar.gz.

Overrides are inherited from scripts/v2_2_12_common.sh, including:
  PULSEDAG_REHEARSAL_STATE_DIR       default: $STATE_DIR
  PULSEDAG_REHEARSAL_LOG_DIR         default: $LOG_DIR
  PULSEDAG_REHEARSAL_EVIDENCE_DIR    default: <state>/evidence
  PULSEDAG_NODE_A_RPC/B_RPC/C_RPC    default: 127.0.0.1:18080/18081/18082
  PULSEDAG_REHEARSAL_CURL_TIMEOUT_SECS default: $CURL_TIMEOUT_SECS
USAGE
}

endpoint_slug() {
  local endpoint="$1"
  echo "${endpoint#/}" | tr '/' '_'
}

collect_endpoint() {
  local node="$1" endpoint="$2" required="$3"
  local slug url out err rc
  slug="$(endpoint_slug "$endpoint")"
  url="$(rpc_url "$node")$endpoint"
  out="$RESPONSES_DIR/node-$node/$slug.json"
  err="$RESPONSES_DIR/node-$node/$slug.stderr"
  mkdir -p "$(dirname "$out")"

  if curl -fsS -m "$CURL_TIMEOUT_SECS" "$url" -o "$out" 2>"$err"; then
    printf '[ok] node-%s %s -> %s\n' "$node" "$endpoint" "$out"
    if [[ ! -s "$err" ]]; then
      rm -f "$err"
    fi
    return 0
  else
    rc=$?
    printf '[%s] node-%s %s unavailable (curl rc=%s, stderr=%s)\n' \
      "$([[ "$required" == "1" ]] && echo error || echo warn)" "$node" "$endpoint" "$rc" "$err" >&2
    rm -f "$out"
    if [[ "$required" == "1" ]]; then
      failures=$((failures + 1))
    fi
    return 0
  fi
}

write_environment_summary() {
  local git_commit git_dirty node_bin_version miner_bin_version bootnode
  git_commit="unavailable"
  git_dirty="unavailable"
  if command -v git >/dev/null 2>&1 && git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git_commit="$(git -C "$ROOT_DIR" rev-parse HEAD 2>/dev/null || echo unavailable)"
    if git -C "$ROOT_DIR" diff --quiet --ignore-submodules -- 2>/dev/null && \
       git -C "$ROOT_DIR" diff --cached --quiet --ignore-submodules -- 2>/dev/null; then
      git_dirty="false"
    else
      git_dirty="true"
    fi
  fi

  node_bin_version="unavailable"
  miner_bin_version="unavailable"
  if [[ -x "$NODE_BIN" ]]; then
    node_bin_version="$("$NODE_BIN" --version 2>/dev/null || true)"
    [[ -n "$node_bin_version" ]] || node_bin_version="available at path; --version returned no output"
  fi
  if [[ -x "$MINER_BIN" ]]; then
    miner_bin_version="$("$MINER_BIN" --version 2>/dev/null || true)"
    [[ -n "$miner_bin_version" ]] || miner_bin_version="available at path; --version returned no output"
  fi

  bootnode="$(node_bootnode_a)"

  cat > "$SUMMARY_FILE" <<SUMMARY
# v2.2.12 Rehearsal Evidence Summary

- Collected at UTC: $TIMESTAMP
- Repository root: $ROOT_DIR
- State directory: $STATE_DIR
- Evidence directory: $EVIDENCE_DIR
- Log directory: $LOG_DIR
- Chain id: $CHAIN_ID
- Network profile: $NETWORK_PROFILE
- P2P mode: $P2P_MODE
- Node A RPC: $(node_rpc a)
- Node B RPC: $(node_rpc b)
- Node C RPC: $(node_rpc c)
- Node A P2P: $(node_p2p a)
- Node B P2P: $(node_p2p b)
- Node C P2P: $(node_p2p c)
- Node A bootnode value: $bootnode
- Node binary path: $NODE_BIN
- Node binary version: $node_bin_version
- Miner binary path: $MINER_BIN
- Miner binary version: $miner_bin_version
- Miner node URL: ${PULSEDAG_MINER_NODE_URL:-$(rpc_url a)}
- Miner address: $MINER_ADDRESS
- Git commit: $git_commit
- Git working tree dirty: $git_dirty
- Required endpoints: ${REQUIRED_ENDPOINTS[*]}
- Optional endpoints: ${OPTIONAL_ENDPOINTS[*]}
SUMMARY
}

copy_logs() {
  mkdir -p "$LOGS_OUT_DIR"
  if [[ ! -d "$LOG_DIR" ]]; then
    echo "[warn] rehearsal log directory does not exist: $LOG_DIR" >&2
    return 0
  fi

  shopt -s nullglob
  local logs=("$LOG_DIR"/*.log "$LOG_DIR"/*.out "$LOG_DIR"/*.err)
  if (( ${#logs[@]} == 0 )); then
    echo "[warn] no node/miner log files found in $LOG_DIR" >&2
    return 0
  fi

  cp -a "${logs[@]}" "$LOGS_OUT_DIR/"
  printf '[ok] copied %s log file(s) into %s\n' "${#logs[@]}" "$LOGS_OUT_DIR"
}

write_manifest() {
  (
    echo "v2.2.12 rehearsal evidence manifest"
    echo "created_utc=$TIMESTAMP"
    echo "archive=$ARCHIVE_PATH"
    echo
    find "$EVIDENCE_DIR" -type f | sort | sed "s#^$EVIDENCE_DIR/##"
  ) > "$MANIFEST_FILE"
}

main() {
  if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
  fi
  if [[ $# -gt 0 ]]; then
    usage >&2
    exit 2
  fi

  command -v curl >/dev/null || { echo "[error] curl is required" >&2; exit 1; }
  command -v tar >/dev/null || { echo "[error] tar is required" >&2; exit 1; }

  mkdir -p "$RESPONSES_DIR" "$LOGS_OUT_DIR"
  write_environment_summary

  for node in "${NODES[@]}"; do
    for endpoint in "${REQUIRED_ENDPOINTS[@]}"; do
      collect_endpoint "$node" "$endpoint" 1
    done
    for endpoint in "${OPTIONAL_ENDPOINTS[@]}"; do
      collect_endpoint "$node" "$endpoint" 0
    done
  done

  copy_logs
  write_manifest

  mkdir -p "$EVIDENCE_ROOT"
  tar -C "$EVIDENCE_ROOT" -czf "$ARCHIVE_PATH" "$(basename "$EVIDENCE_DIR")"
  echo "[ok] evidence archive created: $ARCHIVE_PATH"

  if (( failures > 0 )); then
    echo "[error] $failures required endpoint collection(s) failed; archive retained for debugging" >&2
    exit 1
  fi
}

main "$@"
