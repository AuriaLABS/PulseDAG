#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# 1) Ensure index links point to existing runbooks.
INDEX="docs/runbooks/INDEX.md"
missing=0
while IFS= read -r doc; do
  if [[ ! -f "$doc" ]]; then
    echo "missing linked doc: $doc"
    missing=1
  fi
done < <(grep -oE 'docs/runbooks/[A-Z0-9_.-]+\.md' "$INDEX" | sort -u)

# 2) Ensure key endpoint references in v2.2 runbooks exist in rpc routes.
ROUTES="crates/pulsedag-rpc/src/routes.rs"
for endpoint in \
  /health /readiness /status /sync/status /sync/verify /runtime /runtime/events /runtime/events/summary \
  /p2p/status /p2p/topology /snapshot /snapshot/create /sync/replay-plan /sync/rebuild-preview /prune /sync/rebuild \
  /checks /maintenance/report; do
  if ! rg -q ""${endpoint}"" "$ROUTES"; then
    echo "missing endpoint in routes: ${endpoint}"
    missing=1
  fi
done

# 3) Ensure scripts referenced by runbooks exist.
for script in scripts/restore-drill-evidence.sh scripts/smoke.ps1 scripts/dev-smoke.ps1 scripts/recovery-smoke.ps1 scripts/staging/validate_upgrade_rollback.sh scripts/chaos/run-validation-suite.sh scripts/chaos/validate-evidence.sh; do
  if [[ ! -f "$script" ]]; then
    echo "missing script referenced by runbooks: $script"
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  echo "runbook validation failed"
  exit 1
fi

echo "runbook validation passed"
