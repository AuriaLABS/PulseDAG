#!/usr/bin/env bash
set -u

failures=0

run_check() {
  local name="$1"
  shift
  echo
  echo "========== ${name} =========="
  echo "+ $*"
  if "$@"; then
    echo "PASS: ${name}"
  else
    local status=$?
    echo "FAIL: ${name} (exit ${status})"
    failures=$((failures + 1))
  fi
}

run_check "cargo fmt --check" cargo fmt --check
run_check "cargo test -p pulsedag-core" cargo test -p pulsedag-core
run_check "cargo test -p pulsedag-storage" cargo test -p pulsedag-storage
run_check "cargo test --workspace" cargo test --workspace
run_check "cargo build --workspace" cargo build --workspace

echo
if [[ "${failures}" -eq 0 ]]; then
  echo "========== v2.2.14 RELEASE EVIDENCE: PASS =========="
  exit 0
fi

echo "========== v2.2.14 RELEASE EVIDENCE: FAIL (${failures} failing section(s)) =========="
exit 1
