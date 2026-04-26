#!/usr/bin/env bash
set -euo pipefail

RUN_ID="${1:-}"
if [[ -z "$RUN_ID" ]]; then
  echo "usage: scripts/chaos/validate-evidence.sh <run_id>" >&2
  exit 1
fi

BASE_DIR="artifacts/release-evidence/${RUN_ID}/chaos-suite"
required=(
  "${BASE_DIR}/manifest.csv"
  "${BASE_DIR}/events.csv"
  "${BASE_DIR}/summary.md"
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
outcome_rows=$(grep -Ec '^\| .*\| (pass|fail) \|' "${BASE_DIR}/summary.md" || true)

if [[ "$scenario_rows" -eq 0 ]]; then
  echo "manifest has no scenarios"
  exit 1
fi

if [[ "$outcome_rows" -lt "$scenario_rows" ]]; then
  echo "summary is incomplete: expected at least ${scenario_rows} outcome rows, got ${outcome_rows}"
  exit 1
fi

echo "chaos evidence validation passed (${outcome_rows}/${scenario_rows} scenarios recorded)"
