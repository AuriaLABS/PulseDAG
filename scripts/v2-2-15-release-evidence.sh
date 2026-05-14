#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVIDENCE_DIR="${PULSEDAG_V2_2_15_EVIDENCE_DIR:-$ROOT_DIR/evidence/v2.2.15}"
SUMMARY="$EVIDENCE_DIR/summary.md"
FAILURES=0
mkdir -p "$EVIDENCE_DIR/logs"

run_section() {
  local name="$1"; shift
  local safe="${name//[^A-Za-z0-9_.-]/_}"
  local log="$EVIDENCE_DIR/logs/$safe.log"
  echo
  echo "========== $name =========="
  echo "[info] command: $*"
  if (cd "$ROOT_DIR" && "$@") >"$log" 2>&1; then
    echo "PASS: $name"
    RESULTS+=("PASS|$name|$log|$*")
  else
    echo "FAIL: $name (see $log)" >&2
    tail -80 "$log" >&2 || true
    RESULTS+=("FAIL|$name|$log|$*")
    FAILURES=$((FAILURES + 1))
  fi
}

run_optional_script() {
  local script="$1" name="$2"
  if [[ -f "$ROOT_DIR/$script" ]]; then
    run_section "$name" bash "$script"
  else
    echo "SKIP: $name ($script not found)"
    RESULTS+=("SKIP|$name||$script not found")
  fi
}

RESULTS=()
run_section "cargo fmt" cargo fmt --all -- --check
run_section "cargo test" cargo test --workspace
run_section "cargo build" cargo build --workspace
run_optional_script "scripts/v2-2-15-p2p-3node-rehearsal.sh" "v2.2.15 3-node rehearsal"
run_optional_script "scripts/v2-2-15-p2p-churn-rejoin-evidence.sh" "v2.2.15 churn/rejoin rehearsal"
run_optional_script "scripts/v2-2-15-p2p-lag-recovery-evidence.sh" "v2.2.15 lag recovery rehearsal"
run_optional_script "scripts/v2-2-15-chain-id-isolation-evidence.sh" "v2.2.15 chain-id isolation"

{
  echo "# PulseDAG v2.2.15 release evidence summary"
  echo
  echo "- Date (UTC): $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "- Commit SHA: $(cd "$ROOT_DIR" && git rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "- Overall status: $([[ $FAILURES -eq 0 ]] && echo PASS || echo FAIL)"
  echo "- Evidence directory: $EVIDENCE_DIR"
  echo
  echo "| Status | Section | Command | Log |"
  echo "| --- | --- | --- | --- |"
  for row in "${RESULTS[@]}"; do
    IFS='|' read -r status name log cmd <<<"$row"
    if [[ -n "$log" ]]; then rel="${log#$ROOT_DIR/}"; else rel=""; fi
    echo "| $status | $name | \`$cmd\` | $rel |"
  done
  echo
  echo "## Known limitations"
  echo
  echo "- v2.2.15 is a P2P rehearsal/hardening evidence gate, not v2.3.0 readiness."
  echo "- Any SKIP, FAIL, or environment-specific rehearsal limitation must be triaged in the v2.2.15 closeout checklist before moving to v2.2.16."
} >"$SUMMARY"

echo
cat "$SUMMARY"
exit "$FAILURES"
