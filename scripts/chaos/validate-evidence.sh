#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  scripts/chaos/validate-evidence.sh --run-id <id> [--base-dir <path>] [--node-urls <csv>]
USAGE
}

RUN_ID=""
BASE_DIR=""
NODE_URLS="http://127.0.0.1:8080,http://127.0.0.1:8081,http://127.0.0.1:8082"

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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      if [[ -z "$RUN_ID" && "$1" != --* ]]; then
        RUN_ID="$1"
        shift
      else
        echo "unknown argument: $1" >&2
        usage >&2
        exit 1
      fi
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

required=(
  "${BASE_DIR}/manifest.csv"
  "${BASE_DIR}/events.csv"
  "${BASE_DIR}/summary.md"
  "${BASE_DIR}/scenario-outcomes.csv"
  "${BASE_DIR}/run-info.json"
)

missing=0
for path in "${required[@]}"; do
  if [[ ! -f "$path" ]]; then
    echo "missing: $path"
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

scenario_rows=$(tail -n +2 "${BASE_DIR}/manifest.csv" | wc -l | tr -d ' ')
outcome_rows=$(tail -n +2 "${BASE_DIR}/scenario-outcomes.csv" | wc -l | tr -d ' ')
summary_rows=$(grep -Ec '^\| .*\| (pass|fail) \|' "${BASE_DIR}/summary.md" || true)

if [[ "$scenario_rows" -eq 0 ]]; then
  echo "manifest has no scenarios"
  exit 1
fi

if [[ "$outcome_rows" -ne "$scenario_rows" ]]; then
  echo "scenario-outcomes mismatch: expected ${scenario_rows}, got ${outcome_rows}"
  exit 1
fi

if [[ "$summary_rows" -lt "$scenario_rows" ]]; then
  echo "summary is incomplete: expected at least ${scenario_rows} outcome rows, got ${summary_rows}"
  exit 1
fi

IFS=',' read -r -a NODE_ARRAY <<< "$NODE_URLS"
expected_capture_count=$((scenario_rows * ${#NODE_ARRAY[@]} * 3 * 2))
actual_capture_count=$(grep -Ec ',(pre|post),.*/(health|sync/status|runtime/status),ok,' "${BASE_DIR}/events.csv" || true)

if [[ "$actual_capture_count" -lt "$expected_capture_count" ]]; then
  echo "insufficient successful captures: expected >= ${expected_capture_count}, got ${actual_capture_count}"
  exit 1
fi

echo "chaos evidence validation passed"
echo "  scenarios: ${scenario_rows}"
echo "  outcomes: ${outcome_rows}"
echo "  successful endpoint captures: ${actual_capture_count} (expected >= ${expected_capture_count})"
