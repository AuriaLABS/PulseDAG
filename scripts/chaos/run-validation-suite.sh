#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  scripts/chaos/run-validation-suite.sh --run-id <id> [--base-dir <path>] [--node-urls <csv>] [--scenario-manifest <path>] [--yes]

Description:
  Guided crash/restart/churn/recovery validation suite for operator evidence collection.
  The suite captures repeatable pre/post endpoint snapshots and writes reviewer-friendly
  evidence outputs without changing consensus rules, miner behavior, or introducing pool logic.
USAGE
}

RUN_ID=""
BASE_DIR=""
NODE_URLS="http://127.0.0.1:8080,http://127.0.0.1:8081,http://127.0.0.1:8082"
SCENARIO_MANIFEST="scripts/chaos/scenarios.csv"
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
    --scenario-manifest)
      SCENARIO_MANIFEST="${2:-}"
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

if [[ ! -f "$SCENARIO_MANIFEST" ]]; then
  echo "scenario manifest not found: $SCENARIO_MANIFEST" >&2
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
OUTCOMES_CSV="${BASE_DIR}/scenario-outcomes.csv"
RUN_INFO_JSON="${BASE_DIR}/run-info.json"

cp "$SCENARIO_MANIFEST" "$MANIFEST_CSV"

cat > "$EVENTS_CSV" <<'EVENTS'
timestamp_utc,scenario_id,phase,node_url,endpoint,result,details
EVENTS

cat > "$OUTCOMES_CSV" <<'OUTCOMES'
scenario_id,priority,target_slo_seconds,outcome,duration_seconds,slo_met,started_utc,ended_utc,notes
OUTCOMES

cat > "$SUMMARY_MD" <<SUMMARY
# Chaos validation summary

- Run ID: ${RUN_ID}
- Started (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)
- Node URLs: ${NODE_URLS}
- Scenario manifest: ${SCENARIO_MANIFEST}

## Scenario outcomes

| Scenario | Priority | Outcome | Duration (s) | SLO target (s) | SLO met | Started (UTC) | Ended (UTC) | Notes |
|---|---|---|---:|---:|---|---|---|---|
SUMMARY

cat > "$RUN_INFO_JSON" <<JSON
{
  "run_id": "${RUN_ID}",
  "started_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "node_urls": "${NODE_URLS}",
  "scenario_manifest": "${SCENARIO_MANIFEST}",
  "base_dir": "${BASE_DIR}",
  "mode": "$( [[ "$ASSUME_YES" -eq 1 ]] && echo dryrun || echo operator )"
}
JSON

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
  local scenario_id="$1" priority="$2" target_slo_seconds="$3" description="$4" action_text="$5" notes_hint="$6"
  local start_epoch end_epoch start_ts end_ts outcome notes duration_seconds slo_met

  start_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  start_epoch="$(date -u +%s)"
  echo
  echo "=== Scenario: ${scenario_id} (${priority}) ==="
  echo "$description"

  capture_baseline "$scenario_id" "pre"
  wait_for_operator "$action_text"
  capture_baseline "$scenario_id" "post"

  if [[ "$ASSUME_YES" -eq 1 ]]; then
    outcome="pass"
    notes="${notes_hint} [dryrun-auto]"
  else
    read -r -p "Outcome for ${scenario_id} (pass/fail): " outcome
    read -r -p "Short notes for ${scenario_id}: " notes
  fi

  end_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  end_epoch="$(date -u +%s)"
  duration_seconds="$((end_epoch - start_epoch))"

  if [[ "$duration_seconds" -le "$target_slo_seconds" ]]; then
    slo_met="yes"
  else
    slo_met="no"
  fi

  printf '| %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
    "$scenario_id" "$priority" "$outcome" "$duration_seconds" "$target_slo_seconds" "$slo_met" "$start_ts" "$end_ts" "$notes" >> "$SUMMARY_MD"

  printf '%s,%s,%s,%s,%s,%s,%s,%s,%s\n' \
    "$scenario_id" "$priority" "$target_slo_seconds" "$outcome" "$duration_seconds" "$slo_met" "$start_ts" "$end_ts" "$notes" >> "$OUTCOMES_CSV"
}

while IFS=$'\t' read -r scenario_id priority target_slo_seconds description action_text notes_hint; do
  run_scenario "$scenario_id" "$priority" "$target_slo_seconds" "$description" "$action_text" "$notes_hint"
done < <(
  python3 - "$MANIFEST_CSV" <<'PY'
import csv
import sys

with open(sys.argv[1], newline="", encoding="utf-8") as f:
    reader = csv.DictReader(f)
    for row in reader:
        print("\t".join([
            row["scenario_id"],
            row["priority"],
            row["target_slo_seconds"],
            row["description"],
            row["operator_action"],
            row["notes_hint"],
        ]))
PY
)

printf "\nChaos validation suite complete. Evidence written to: %s\n" "${BASE_DIR}"
echo "Next steps:"
echo "  1) scripts/chaos/validate-evidence.sh --run-id ${RUN_ID}"
echo "  2) scripts/chaos/archive-evidence.sh --run-id ${RUN_ID}"
