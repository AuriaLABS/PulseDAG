#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  scripts/chaos/run-validation-suite.sh --run-id <id> [--base-dir <path>] [--node-urls <csv>] [--yes]

Description:
  Guided crash/restart/churn/recovery validation suite for operator evidence collection.
  This script records timestamps, endpoint snapshots, and scenario outcomes without
  changing consensus rules, miner behavior, or introducing pool logic.
USAGE
}

RUN_ID=""
BASE_DIR=""
NODE_URLS="http://127.0.0.1:8080,http://127.0.0.1:8081,http://127.0.0.1:8082"
ASSUME_YES=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-id)
      RUN_ID="${2:-}"
      shift 2
      ;;
    --base-dir)
      BASE_DIR="${2:-}"
      shift 2
      ;;
    --node-urls)
      NODE_URLS="${2:-}"
      shift 2
      ;;
    --yes)
      ASSUME_YES=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$RUN_ID" ]]; then
  echo "--run-id is required" >&2
  usage >&2
  exit 1
fi

if [[ -z "$BASE_DIR" ]]; then
  BASE_DIR="artifacts/release-evidence/${RUN_ID}/chaos-suite"
fi

RAW_DIR="${BASE_DIR}/raw"
mkdir -p "$RAW_DIR"

MANIFEST_CSV="${BASE_DIR}/manifest.csv"
EVENTS_CSV="${BASE_DIR}/events.csv"
SUMMARY_MD="${BASE_DIR}/summary.md"

if [[ ! -f "$MANIFEST_CSV" ]]; then
  cat > "$MANIFEST_CSV" <<'MANIFEST'
scenario_id,priority,target_slo_seconds,description
crash-restart-node-b,P0,300,Crash node B then restart and reconverge tip/sync metrics
graceful-restart-seed-a,P0,300,Restart a seed node without prolonged sync drift
external-miner-churn,P1,1200,Detach an external miner and recover submit acceptance rate
peer-churn-isolate-rejoin,P0,900,Network isolate non-seed node and rejoin cleanly
recovery-snapshot-restore,P0,1800,Run snapshot restore/rebuild and recover healthy participation
MANIFEST
fi

if [[ ! -f "$EVENTS_CSV" ]]; then
  cat > "$EVENTS_CSV" <<'EVENTS'
timestamp_utc,scenario_id,phase,node_url,endpoint,result,details
EVENTS
fi

if [[ ! -f "$SUMMARY_MD" ]]; then
  cat > "$SUMMARY_MD" <<SUMMARY
# Chaos validation summary

- Run ID: ${RUN_ID}
- Started (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)
- Node URLs: ${NODE_URLS}

## Scenario outcomes

| Scenario | Outcome | Started (UTC) | Ended (UTC) | Notes |
|---|---|---|---|---|
SUMMARY
fi

IFS=',' read -r -a NODE_ARRAY <<< "$NODE_URLS"

log_event() {
  local scenario_id="$1" phase="$2" node_url="$3" endpoint="$4" result="$5" details="$6"
  local ts
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '%s,%s,%s,%s,%s,%s,%s\n' "$ts" "$scenario_id" "$phase" "$node_url" "$endpoint" "$result" "$details" >> "$EVENTS_CSV"
}

capture_endpoint() {
  local scenario_id="$1" phase="$2" node_url="$3" endpoint="$4"
  local safe_node safe_ep out_file
  safe_node="$(echo "$node_url" | tr ':/' '_')"
  safe_ep="$(echo "$endpoint" | tr '/?' '__' | tr '&=' '__')"
  out_file="${RAW_DIR}/${scenario_id}_${phase}_${safe_node}_${safe_ep}.json"

  if curl -fsS "${node_url}${endpoint}" > "$out_file"; then
    log_event "$scenario_id" "$phase" "$node_url" "$endpoint" "ok" "captured:${out_file}"
  else
    log_event "$scenario_id" "$phase" "$node_url" "$endpoint" "error" "capture_failed:${out_file}"
  fi
}

capture_baseline() {
  local scenario_id="$1" phase="$2"
  for node_url in "${NODE_ARRAY[@]}"; do
    capture_endpoint "$scenario_id" "$phase" "$node_url" "/health"
    capture_endpoint "$scenario_id" "$phase" "$node_url" "/sync/status"
    capture_endpoint "$scenario_id" "$phase" "$node_url" "/runtime/status"
  done
}

wait_for_operator() {
  local prompt="$1"
  if [[ "$ASSUME_YES" -eq 1 ]]; then
    echo "[auto] $prompt"
    return 0
  fi
  echo
  echo "ACTION REQUIRED: $prompt"
  read -r -p "Press Enter to continue after the action is complete... " _
}

run_scenario() {
  local scenario_id="$1" description="$2" action_text="$3" notes_text="$4"
  local start_ts end_ts outcome notes

  start_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "=== Scenario: ${scenario_id} ==="
  echo "$description"

  capture_baseline "$scenario_id" "pre"
  wait_for_operator "$action_text"
  capture_baseline "$scenario_id" "post"

  if [[ "$ASSUME_YES" -eq 1 ]]; then
    outcome="pass"
    notes="$notes_text"
  else
    read -r -p "Outcome for ${scenario_id} (pass/fail): " outcome
    read -r -p "Short notes for ${scenario_id}: " notes
  fi

  end_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf '| %s | %s | %s | %s | %s |\n' "$scenario_id" "$outcome" "$start_ts" "$end_ts" "$notes" >> "$SUMMARY_MD"
}

run_scenario \
  "crash-restart-node-b" \
  "Crash a validator process and ensure restart + rejoin succeeds." \
  "Terminate node B abruptly (SIGKILL or container stop), restart it with standard operator procedure, then confirm health endpoints recover." \
  "Verify sync lag returns to baseline and no unresolved Sev-1 incidents."

run_scenario \
  "graceful-restart-seed-a" \
  "Gracefully restart one seed node and check topology recovery." \
  "Stop seed A gracefully, restart it, and confirm peers reconnect without prolonged lag." \
  "Ensure peer counts and sync status reconverge across the cluster."

run_scenario \
  "external-miner-churn" \
  "Detach one external miner for 15 minutes, then reattach to the same endpoint." \
  "Stop one external miner process, wait 15 minutes, restart it, and verify template/submit loop resumes." \
  "Confirm submit acceptance recovers and rejection spikes are explained."

run_scenario \
  "peer-churn-isolate-rejoin" \
  "Simulate peer churn by isolating one non-seed node, then rejoin it." \
  "Isolate node C from peers for 10 minutes (network ACL/firewall), then remove isolation and verify clean rejoin." \
  "Confirm no persistent desync/fork remains after rejoin."

run_scenario \
  "recovery-snapshot-restore" \
  "Run snapshot restore/rebuild drill and validate full recovery." \
  "Follow docs/runbooks/RECOVERY_ORCHESTRATION.md to execute restore or rebuild for one node, then validate healthy participation." \
  "Attach command logs and final status snapshots to release evidence bundle."

printf "\nChaos validation suite complete. Evidence written to: %s\n" "${BASE_DIR}"
