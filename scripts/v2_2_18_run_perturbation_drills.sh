#!/usr/bin/env bash
set -euo pipefail

# Controlled perturbation drills for PulseDAG v2.2.18 private RC.
# This script does not alter consensus or P2P protocol behavior.
# Optional node isolation is implemented only when ISOLATION_CMD/RESTORE_CMD are supplied.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACTS_DIR="${REPO_ROOT}/artifacts/perturbation_drills_v2_2_18"
UTC_TS="$(date -u +"%Y%m%dT%H%M%SZ")"
RUN_DIR="${ARTIFACTS_DIR}/${UTC_TS}"

mkdir -p "${RUN_DIR}" "${RUN_DIR}/evidence"

# ---- Required runtime parameters ----
SEED_NODE_SERVICE="${SEED_NODE_SERVICE:-pulsedagd-seed-1}"
NON_SEED_NODE_SERVICE="${NON_SEED_NODE_SERVICE:-pulsedagd-node-2}"
MINER_SERVICE="${MINER_SERVICE:-pulsedag-miner-1}"
MINER_GROUP_SERVICES="${MINER_GROUP_SERVICES:-pulsedag-miner-1 pulsedag-miner-2 pulsedag-miner-3 pulsedag-miner-4}"
ISOLATION_TARGET="${ISOLATION_TARGET:-pulsedagd-node-3}"

# ---- Optional isolation hooks (must be explicitly provided) ----
ISOLATION_CMD="${ISOLATION_CMD:-}"
RESTORE_CMD="${RESTORE_CMD:-}"

# ---- Timing and thresholds aligned with readiness criteria ----
MINER_STOP_SECONDS="${MINER_STOP_SECONDS:-900}"    # 15 minutes default
SYNC_RECONVERGENCE_WINDOW_SECONDS="${SYNC_RECONVERGENCE_WINDOW_SECONDS:-600}"
MINER_SUBMIT_RECOVERY_WINDOW_SECONDS="${MINER_SUBMIT_RECOVERY_WINDOW_SECONDS:-300}"

DRILL_LOG="${RUN_DIR}/perturbation_drills.log"
SUMMARY_CSV="${RUN_DIR}/drill_summary.csv"

log() {
  printf '%s %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" "$*" | tee -a "${DRILL_LOG}"
}

run_cmd() {
  local cmd="$1"
  log "cmd> ${cmd}"
  bash -lc "${cmd}" >>"${DRILL_LOG}" 2>&1
}

append_summary_row() {
  local drill_id="$1"
  local utc_start="$2"
  local utc_end="$3"
  local affected_process="$4"
  local expected_result="$5"
  local observed_result="$6"
  local recovery_time="$7"
  local pass_fail="$8"
  local evidence_path="$9"

  printf '"%s","%s","%s","%s","%s","%s","%s","%s","%s"\n' \
    "${drill_id}" "${utc_start}" "${utc_end}" "${affected_process}" \
    "${expected_result}" "${observed_result}" "${recovery_time}" "${pass_fail}" "${evidence_path}" \
    >> "${SUMMARY_CSV}"
}

wait_for_marker() {
  local marker_cmd="$1"
  local timeout="$2"
  local started_at
  started_at="$(date +%s)"

  while true; do
    if bash -lc "${marker_cmd}" >>"${DRILL_LOG}" 2>&1; then
      echo "$(( $(date +%s) - started_at ))"
      return 0
    fi

    if (( $(date +%s) - started_at > timeout )); then
      return 1
    fi

    sleep 5
  done
}

run_drill() {
  local drill_id="$1"
  local affected_process="$2"
  local expected_result="$3"
  local perturb_cmd="$4"
  local recover_cmd="$5"
  local marker_cmd="$6"
  local marker_timeout="$7"

  local evidence_file="${RUN_DIR}/evidence/${drill_id}.md"
  local utc_start utc_end recovery_secs observed_result pass_fail

  utc_start="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  log "[${drill_id}] starting (${affected_process})"

  run_cmd "${perturb_cmd}"
  run_cmd "${recover_cmd}"

  if recovery_secs="$(wait_for_marker "${marker_cmd}" "${marker_timeout}")"; then
    observed_result="Recovered within ${recovery_secs}s"
    pass_fail="PASS"
  else
    recovery_secs="N/A"
    observed_result="Did not recover within ${marker_timeout}s"
    pass_fail="FAIL"
  fi

  utc_end="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  cat > "${evidence_file}" <<EOD
# ${drill_id}

- UTC start: ${utc_start}
- UTC end: ${utc_end}
- Affected process: ${affected_process}
- Expected result: ${expected_result}
- Observed result: ${observed_result}
- Recovery time: ${recovery_secs}
- Pass/fail: ${pass_fail}
- Evidence path: ${evidence_file}

## Readiness threshold mapping
- Sync reconvergence window target: ${SYNC_RECONVERGENCE_WINDOW_SECONDS}s
- Miner submit recovery window target: ${MINER_SUBMIT_RECOVERY_WINDOW_SECONDS}s
- No persistent fork/divergence: verify chain head/hash parity in node logs and health checks.
- No unresolved Sev-1 consensus/sync issue: verify pager/incident status is clear for the drill window.

## Raw execution log pointer
- ${DRILL_LOG}
EOD

  append_summary_row "${drill_id}" "${utc_start}" "${utc_end}" "${affected_process}" \
    "${expected_result}" "${observed_result}" "${recovery_secs}" "${pass_fail}" "${evidence_file}"

  log "[${drill_id}] completed with status=${pass_fail}"
}

printf '"drill_id","utc_start","utc_end","affected_process","expected_result","observed_result","recovery_time","pass_fail","evidence_path"\n' > "${SUMMARY_CSV}"

log "Artifacts directory: ${RUN_DIR}"
log "Starting perturbation drill suite for v2.2.18 private RC"

run_drill \
  "drill_1_restart_seed_node" \
  "${SEED_NODE_SERVICE}" \
  "Seed node restarts cleanly; cluster reconverges without persistent divergence within ${SYNC_RECONVERGENCE_WINDOW_SECONDS}s." \
  "systemctl restart ${SEED_NODE_SERVICE}" \
  "sleep 5" \
  "journalctl -u ${SEED_NODE_SERVICE} -n 200 | rg -q 'listening|ready|synced'" \
  "${SYNC_RECONVERGENCE_WINDOW_SECONDS}"

run_drill \
  "drill_2_restart_non_seed_node" \
  "${NON_SEED_NODE_SERVICE}" \
  "Non-seed node restarts and resynchronizes within ${SYNC_RECONVERGENCE_WINDOW_SECONDS}s with no fork/divergence." \
  "systemctl restart ${NON_SEED_NODE_SERVICE}" \
  "sleep 5" \
  "journalctl -u ${NON_SEED_NODE_SERVICE} -n 200 | rg -q 'listening|ready|synced'" \
  "${SYNC_RECONVERGENCE_WINDOW_SECONDS}"

run_drill \
  "drill_3_stop_single_miner_15m" \
  "${MINER_SERVICE}" \
  "Miner can be stopped for ${MINER_STOP_SECONDS}s and returns to submit/accepted state within ${MINER_SUBMIT_RECOVERY_WINDOW_SECONDS}s after restart." \
  "systemctl stop ${MINER_SERVICE} && sleep ${MINER_STOP_SECONDS}" \
  "systemctl start ${MINER_SERVICE}" \
  "journalctl -u ${MINER_SERVICE} -n 200 | rg -q 'submit|accepted|share'" \
  "${MINER_SUBMIT_RECOVERY_WINDOW_SECONDS}"

if [[ -n "${ISOLATION_CMD}" && -n "${RESTORE_CMD}" ]]; then
  run_drill \
    "drill_4_temporary_node_isolation" \
    "${ISOLATION_TARGET}" \
    "Node is isolated temporarily and rejoins consensus/sync domain within ${SYNC_RECONVERGENCE_WINDOW_SECONDS}s after restore." \
    "${ISOLATION_CMD}" \
    "${RESTORE_CMD}" \
    "journalctl -u ${ISOLATION_TARGET} -n 300 | rg -q 'peer|sync|reconnected|synced'" \
    "${SYNC_RECONVERGENCE_WINDOW_SECONDS}"
else
  log "[drill_4_temporary_node_isolation] SKIPPED (optional): provide ISOLATION_CMD and RESTORE_CMD to execute safely."
  append_summary_row \
    "drill_4_temporary_node_isolation" \
    "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    "$(date -u +"%Y-%m-%dT%H:%M:%SZ")" \
    "${ISOLATION_TARGET}" \
    "Temporary isolation and clean reconvergence without fake evidence." \
    "Skipped by design: no privileged isolation hooks configured." \
    "N/A" \
    "SKIP" \
    "${RUN_DIR}/evidence/drill_4_temporary_node_isolation.md"
fi

run_drill \
  "drill_5_restart_all_miners" \
  "${MINER_GROUP_SERVICES}" \
  "All miners restart and recover submit flow with no unresolved Sev-1 consensus/sync issues." \
  "for svc in ${MINER_GROUP_SERVICES}; do systemctl restart \"$svc\"; done" \
  "sleep 5" \
  "for svc in ${MINER_GROUP_SERVICES}; do journalctl -u \"$svc\" -n 200 | rg -q 'submit|accepted|share'; done" \
  "${MINER_SUBMIT_RECOVERY_WINDOW_SECONDS}"

log "Completed perturbation drills. Summary: ${SUMMARY_CSV}"
