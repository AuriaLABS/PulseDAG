#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID=${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}
OUT_DIR_BASE="${OUT_DIR:-$ROOT_DIR/artifacts/v2_2_19/staged_convergence_gates/$RUN_ID}"
INTERMEDIATE_REQUIRED=${INTERMEDIATE_REQUIRED:-1}
STRESS_REQUIRED=${STRESS_REQUIRED:-0}
mkdir -p "$OUT_DIR_BASE"
exec > >(tee -a "$OUT_DIR_BASE/command-log.txt") 2>&1

run_stage(){
  local label="$1" miners="$2" required="$3" subdir="$4" rc=0
  echo "== Running $label (required=$required) =="
  MINER_COUNT="$miners" \
  STAGE_NAME="$label" \
  RUN_ID="$RUN_ID" \
  OUT_DIR="$OUT_DIR_BASE/$subdir" \
  "$ROOT_DIR/scripts/v2_2_19_private_5n_4m_rehearsal.sh" || rc=$?
  if (( rc != 0 && required == 1 )); then
    echo "STAGE_RESULT $label FAIL required rc=$rc"
    return "$rc"
  fi
  if (( rc != 0 )); then
    echo "STAGE_RESULT $label WARN diagnostic rc=$rc"
  else
    echo "STAGE_RESULT $label PASS rc=0"
  fi
  return 0
}

overall=0
run_stage "5N/1M baseline" 1 1 "baseline_5n_1m" || overall=$?
if (( overall == 0 )); then run_stage "5N/2M intermediate" 2 "$INTERMEDIATE_REQUIRED" "intermediate_5n_2m" || overall=$?; fi
if (( overall == 0 )); then run_stage "5N/4M stress" 4 "$STRESS_REQUIRED" "stress_5n_4m" || overall=$?; fi

{
  echo "# v2.2.19 staged convergence gates"
  echo "- run_id: $RUN_ID"
  echo "- 5N/1M baseline: mandatory readiness gate"
  echo "- 5N/2M intermediate: $([[ $INTERMEDIATE_REQUIRED == 1 ]] && echo mandatory || echo warning) gate"
  echo "- 5N/4M stress: $([[ $STRESS_REQUIRED == 1 ]] && echo mandatory || echo diagnostic) gate"
  echo "- public_testnet_ready: false (this script never promotes readiness)"
  echo
  echo "## Stage evidence"
  for d in baseline_5n_1m intermediate_5n_2m stress_5n_4m; do
    if [[ -f "$OUT_DIR_BASE/$d/evidence-summary.md" ]]; then
      echo "- $d: $OUT_DIR_BASE/$d/evidence-summary.md"
    else
      echo "- $d: not run"
    fi
  done
  echo
  echo "## Overall"
  echo "- exit_code: $overall"
} > "$OUT_DIR_BASE/staged-summary.md"

exit "$overall"
