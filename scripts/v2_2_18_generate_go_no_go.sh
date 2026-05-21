#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE
Usage:
  $0 \
    --evidence-dir <dir> \
    --topology-manifest <file> \
    --sync-summary <file> \
    --miner-telemetry-summary <file> \
    --perturbation-summary <file> \
    --restore-summary <file> \
    --command-check-outputs <file> \
    [--output <file>]

Output defaults to: <evidence-dir>/go-no-go.md
USAGE
}

need_arg() {
  local name="$1"
  local value="$2"
  if [[ -z "${value}" ]]; then
    echo "error: missing required argument: ${name}" >&2
    usage
    exit 1
  fi
}

is_missing_or_empty() {
  local path="$1"
  [[ ! -f "${path}" || ! -s "${path}" ]]
}

contains_any() {
  local path="$1"
  shift
  local pattern
  for pattern in "$@"; do
    if rg -qi -- "${pattern}" "${path}"; then
      return 0
    fi
  done
  return 1
}

EVIDENCE_DIR=""
TOPOLOGY_MANIFEST=""
SYNC_SUMMARY=""
MINER_TELEMETRY_SUMMARY=""
PERTURBATION_SUMMARY=""
RESTORE_SUMMARY=""
COMMAND_CHECK_OUTPUTS=""
OUTPUT_PATH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --evidence-dir)
      EVIDENCE_DIR="${2:-}"
      shift 2
      ;;
    --topology-manifest)
      TOPOLOGY_MANIFEST="${2:-}"
      shift 2
      ;;
    --sync-summary)
      SYNC_SUMMARY="${2:-}"
      shift 2
      ;;
    --miner-telemetry-summary)
      MINER_TELEMETRY_SUMMARY="${2:-}"
      shift 2
      ;;
    --perturbation-summary)
      PERTURBATION_SUMMARY="${2:-}"
      shift 2
      ;;
    --restore-summary)
      RESTORE_SUMMARY="${2:-}"
      shift 2
      ;;
    --command-check-outputs)
      COMMAND_CHECK_OUTPUTS="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT_PATH="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

need_arg "--evidence-dir" "${EVIDENCE_DIR}"
need_arg "--topology-manifest" "${TOPOLOGY_MANIFEST}"
need_arg "--sync-summary" "${SYNC_SUMMARY}"
need_arg "--miner-telemetry-summary" "${MINER_TELEMETRY_SUMMARY}"
need_arg "--perturbation-summary" "${PERTURBATION_SUMMARY}"
need_arg "--restore-summary" "${RESTORE_SUMMARY}"
need_arg "--command-check-outputs" "${COMMAND_CHECK_OUTPUTS}"

if [[ -z "${OUTPUT_PATH}" ]]; then
  OUTPUT_PATH="${EVIDENCE_DIR}/go-no-go.md"
fi

mkdir -p "${EVIDENCE_DIR}"

hard_no_go_reasons=()
pending_evidence_reasons=()
conditional_reasons=()

if [[ ! -d "${EVIDENCE_DIR}" ]]; then
  pending_evidence_reasons+=("evidence bundle missing: ${EVIDENCE_DIR}")
fi

required_files=(
  "${TOPOLOGY_MANIFEST}"
  "${SYNC_SUMMARY}"
  "${MINER_TELEMETRY_SUMMARY}"
  "${PERTURBATION_SUMMARY}"
  "${RESTORE_SUMMARY}"
  "${COMMAND_CHECK_OUTPUTS}"
)

for file in "${required_files[@]}"; do
  if is_missing_or_empty "${file}"; then
    pending_evidence_reasons+=("required evidence missing or empty: ${file}")
  fi
done

if ! is_missing_or_empty "${COMMAND_CHECK_OUTPUTS}"; then
  if ! contains_any "${COMMAND_CHECK_OUTPUTS}" "cargo fmt"; then
    hard_no_go_reasons+=("cargo fmt evidence missing")
  fi
  if ! contains_any "${COMMAND_CHECK_OUTPUTS}" "cargo test"; then
    hard_no_go_reasons+=("cargo test evidence missing")
  fi
  if ! contains_any "${COMMAND_CHECK_OUTPUTS}" "cargo build"; then
    hard_no_go_reasons+=("cargo build evidence missing")
  fi
fi

if ! is_missing_or_empty "${SYNC_SUMMARY}"; then
  if contains_any "${SYNC_SUMMARY}" "sev-1" "sev1" "severity 1"; then
    hard_no_go_reasons+=("unresolved Sev-1 consensus/sync issue detected")
  fi
  if contains_any "${SYNC_SUMMARY}" "does not converge" "not converged" "convergence failed" "sync divergence"; then
    hard_no_go_reasons+=("nodes do not converge")
  fi
fi

if ! is_missing_or_empty "${MINER_TELEMETRY_SUMMARY}"; then
  if contains_any "${MINER_TELEMETRY_SUMMARY}" "submit path fails completely" "submit failed completely" "no successful submits"; then
    hard_no_go_reasons+=("miner submit path fails completely")
  elif contains_any "${MINER_TELEMETRY_SUMMARY}" "intermittent submit failure" "degraded submit"; then
    conditional_reasons+=("miner submit path degraded; waiver/retest required")
  fi
fi

if ! is_missing_or_empty "${RESTORE_SUMMARY}"; then
  if contains_any "${RESTORE_SUMMARY}" "drill fails" "restore failed" "rebuild failed"; then
    hard_no_go_reasons+=("restore/rebuild drill fails without retest")
  elif contains_any "${RESTORE_SUMMARY}" "retest required" "pending retest"; then
    conditional_reasons+=("restore/rebuild requires retest before full GO")
  fi
fi

if ! is_missing_or_empty "${TOPOLOGY_MANIFEST}"; then
  if contains_any "${TOPOLOGY_MANIFEST}" "admin rpc exposed" "0.0.0.0" "public admin rpc"; then
    hard_no_go_reasons+=("admin RPC exposed unsafely")
  fi
fi

decision="GO"
if (( ${#hard_no_go_reasons[@]} > 0 )); then
  decision="NO_GO"
elif (( ${#pending_evidence_reasons[@]} > 0 )); then
  decision="PENDING_EVIDENCE"
elif (( ${#conditional_reasons[@]} > 0 )); then
  decision="CONDITIONAL_GO"
fi

run_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

{
  echo "# Go/No-Go Report v2.2.18"
  echo
  echo "- generated_utc: ${run_ts}"
  echo "- decision: ${decision}"
  echo "- release: v2.2.18"
  echo
  echo "## Inputs"
  echo "- evidence_dir: ${EVIDENCE_DIR}"
  echo "- topology_manifest: ${TOPOLOGY_MANIFEST}"
  echo "- sync_summary: ${SYNC_SUMMARY}"
  echo "- miner_telemetry_summary: ${MINER_TELEMETRY_SUMMARY}"
  echo "- perturbation_summary: ${PERTURBATION_SUMMARY}"
  echo "- restore_summary: ${RESTORE_SUMMARY}"
  echo "- command_check_outputs: ${COMMAND_CHECK_OUTPUTS}"
  echo

  echo "## Hard NO-GO checks"
  if (( ${#hard_no_go_reasons[@]} == 0 )); then
    echo "- none triggered"
  else
    for reason in "${hard_no_go_reasons[@]}"; do
      echo "- ${reason}"
    done
  fi
  echo

  echo "## Missing / pending evidence"
  if (( ${#pending_evidence_reasons[@]} == 0 )); then
    echo "- none"
  else
    for reason in "${pending_evidence_reasons[@]}"; do
      echo "- ${reason}"
    done
  fi
  echo

  echo "## Conditional findings"
  if (( ${#conditional_reasons[@]} == 0 )); then
    echo "- none"
  else
    for reason in "${conditional_reasons[@]}"; do
      echo "- ${reason}"
    done
  fi
  echo

  echo "## Decision values"
  echo "- GO"
  echo "- CONDITIONAL_GO"
  echo "- NO_GO"
  echo "- PENDING_EVIDENCE"
  echo

  echo "## Policy reminders"
  echo "- Do not automatically mark v2.3.0 ready."
  echo "- Do not hide missing evidence."
  echo "- Do not convert failures to warnings without waiver."
} > "${OUTPUT_PATH}"

echo "Generated ${OUTPUT_PATH}"
