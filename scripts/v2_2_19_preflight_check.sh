#!/usr/bin/env bash
set -euo pipefail

fail=0
checks=0
passes=0

ver="unknown"
cargo_ver="unknown"
ref="unknown"
commit="unknown"
claim_scan_summary="not-run"

check(){
  local msg="$1"; shift
  checks=$((checks+1))
  if "$@"; then
    passes=$((passes+1))
    echo "PASS: $msg"
  else
    echo "FAIL: $msg"
    fail=1
  fi
}

search_text(){
  local pattern="$1"; shift
  if command -v rg >/dev/null 2>&1; then
    rg -nE "$pattern" "$@" >/dev/null 2>&1
  else
    grep -En "$pattern" "$@" >/dev/null 2>&1
  fi
}

check_no_claim(){
  local pattern="$1"; shift
  if search_text "$pattern" "$@"; then
    return 1
  fi
  return 0
}

production_gpu_backend_ready(){
  local marker="docs/PRODUCTION_GPU_BACKEND_READY_V2_2_19.md"
  local required_test="scripts/tests/test_gpu_backend_production_readiness.sh"

  if [[ -f "$marker" ]] && [[ -f "$required_test" ]]; then
    if search_text "^production_gpu_backend_ready:[[:space:]]*yes$" "$marker" && search_text "PASS" "$marker"; then
      return 0
    fi
  fi
  return 1
}

write_evidence(){
  [[ -n "${OUT_DIR:-}" ]] || return 0

  mkdir -p "$OUT_DIR"
  local summary_result
  summary_result=$([[ $fail -eq 0 ]] && echo PASS || echo FAIL)

  cat > "$OUT_DIR/preflight-summary.md" <<SUM
# v2.2.19 preflight

- ref: $ref
- commit: $commit
- version: $ver
- cargo: $cargo_ver
- explicit_checks: $checks
- explicit_passes: $passes
- claim_scan_summary: $claim_scan_summary
- result: $summary_result
SUM
  printf "%s\n" "$ver" > "$OUT_DIR/version.txt"
  printf "%s\n" "$cargo_ver" > "$OUT_DIR/cargo-workspace-version.txt"
  printf "%s\n" "$ref" > "$OUT_DIR/git-ref.txt"
  printf "%s\n" "$commit" > "$OUT_DIR/git-commit.txt"
  printf "%s\n" "$claim_scan_summary" > "$OUT_DIR/claim-scan-summary.txt"
}

trap 'write_evidence' EXIT

ver=$(cat VERSION 2>/dev/null || echo "unknown")
cargo_ver=$(awk '/^version\s*=/{print $3; exit}' Cargo.toml 2>/dev/null | tr -d '"')
if [[ -z "$cargo_ver" ]]; then
  cargo_ver="unknown"
fi

if command -v git >/dev/null 2>&1; then
  if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    ref=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || git describe --all --always 2>/dev/null || echo "unknown")
    commit=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
  fi
fi

echo "Git ref: $ref"
echo "Git commit: $commit"

check "VERSION == v2.2.19" test "$ver" = "v2.2.19"
check "Cargo workspace version == 2.2.19" test "$cargo_ver" = "2.2.19"

required_docs=(
  "docs/VERSION_MATRIX.md"
  "docs/KNOWN_LIMITATIONS_V2_2_19.md"
  "docs/CLOSING_CHECKLIST_V2_2_19_FINAL.md"
)
required_scripts=(
  "scripts/v2_2_19_preflight_check.sh"
  "scripts/v2_2_19_local_3n_1m_smoke.sh"
  "scripts/v2_2_19_private_5n_4m_rehearsal.sh"
)
for f in "${required_docs[@]}"; do
  check "exists doc: $f" test -f "$f"
done
for f in "${required_scripts[@]}"; do
  check "exists script: $f" test -f "$f"
done

claim_files=(README.md docs/VERSION_MATRIX.md docs/KNOWN_LIMITATIONS_V2_2_19.md docs/CLOSING_CHECKLIST_V2_2_19_FINAL.md)

check "no v2.3.0 readiness claim detected" check_no_claim "(v2\\.3\\.0 is ready|ready for v2\\.3\\.0|v2\\.3\\.0 readiness:[[:space:]]*yes|v2\\.3\\.0 ready)" "${claim_files[@]}"

check "no v3.0 readiness claim detected" check_no_claim "(v3\\.0(\\.0)? is ready|ready for v3\\.0(\\.0)?|v3\\.0 readiness:[[:space:]]*yes|v3\\.0 ready)" "${claim_files[@]}"

check "no public testnet live/ready claim detected" check_no_claim "(public testnet is live|public testnet now live|public testnet readiness:[[:space:]]*yes|we are ready to launch public testnet|public testnet ready|public testnet live)" "${claim_files[@]}"

if production_gpu_backend_ready; then
  check "production GPU backend marker exists and tested (claims allowed)" true
else
  check "no production GPU mining claim unless real backend implemented+tested" check_no_claim "(production gpu mining (is )?(live|ready)|gpu mining ready for production|production-ready gpu mining|gpu miner production ready|gpu mining in production)" "${claim_files[@]}"
fi

claim_scan_summary=$([[ $fail -eq 0 ]] && echo "PASS: no forbidden readiness claims" || echo "FAIL: forbidden readiness claim detected")

summary_result=$([[ $fail -eq 0 ]] && echo PASS || echo FAIL)
echo "SUMMARY: ${summary_result} (${passes}/${checks} explicit checks passed)"

exit $fail
