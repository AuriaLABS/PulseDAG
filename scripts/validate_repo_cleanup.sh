#!/usr/bin/env bash
set -euo pipefail

strict=0
for arg in "$@"; do
  case "$arg" in
    --strict) strict=1 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

fail(){ echo "[FAIL] $1" >&2; exit 1; }
pass(){ echo "[PASS] $1"; }

version_raw="$(tr -d '[:space:]' < VERSION)"
version_no_v="${version_raw#v}"
cargo_version="$(sed -n 's/^version = "\([0-9]\+\.[0-9]\+\.[0-9]\+\)"$/\1/p' Cargo.toml | head -n1)"
[[ "$cargo_version" == "$version_no_v" ]] || fail "VERSION/Cargo mismatch"
rg -q "$version_raw|$version_no_v" README.md || fail "README missing current version"
rg -q "$version_raw|$version_no_v" docs/VERSION_MATRIX.md || fail "VERSION_MATRIX missing current version"
pass "README/VERSION/Cargo/VERSION_MATRIX are consistent"

tracked="$(mktemp)"; git ls-files > "$tracked"
rg -n '\.(log|tmp|bak|old|orig|swp|swo|zip|tar\.gz)$|(^|/)(target|logs|run|artifacts)/|\.DS_Store|Thumbs\.db|desktop\.ini' "$tracked" && fail "tracked generated/temp files present" || pass "no tracked generated/temp/archive bundles"

required_scripts=(scripts/v2_2_17_rpc_security_smoke.sh scripts/v2_2_17_collect_api_security_evidence.sh scripts/validate_repo_cleanup.sh scripts/list_cleanup_candidates.sh)
for f in "${required_scripts[@]}"; do [[ -f "$f" ]] || fail "missing required script: $f"; done
required_docs=(docs/RELEASE_NOTES_V2_2_17.md docs/CLOSING_CHECKLIST_V2_2_17.md docs/RPC_ENDPOINT_INVENTORY_V2_2_17.md docs/OPERATOR_SECURITY_RUNBOOK_V2_2_17.md docs/RELEASE_EVIDENCE.md docs/VERSION_MATRIX.md docs/archive/README.md docs/archive/v2_2_history/README.md docs/CLEANUP_AUDIT_V2_2_18_FINAL.md)
for f in "${required_docs[@]}"; do [[ -f "$f" ]] || fail "missing required doc: $f"; done
pass "required current docs/scripts exist"

if [[ ! -f scripts/rpc_security_smoke.sh ]]; then
  if rg -n 'scripts/rpc_security_smoke\.sh' README.md docs .github scripts --glob '!scripts/validate_repo_cleanup.sh' --glob '!docs/CLEANUP_AUDIT_V2_2_18*.md' >/dev/null; then fail "stale reference to scripts/rpc_security_smoke.sh exists"; fi
fi
pass "no stale rpc_security_smoke.sh references"

python3 - <<'PY'
from pathlib import Path
import re
root=Path('.')
bad=[]
for md in [Path('docs/archive/README.md'), Path('docs/archive/v2_2_history/README.md')]:
    txt=md.read_text(encoding='utf-8')
    for t in re.findall(r'\[[^\]]+\]\(([^)]+)\)', txt):
        if t.startswith(('http','mailto:','#')): continue
        p=(md.parent/t.split('#',1)[0]).resolve()
        if not p.exists(): bad.append(f"{md}: {t}")
if bad: raise SystemExit('\n'.join(bad))
PY
pass "docs archive links valid"

if rg -n -i 'ready for v2\.3\.0|ready for v3\.0|v2\.3\.0 is current|v3\.0 is current' README.md docs/VERSION_MATRIX.md docs/RELEASE_EVIDENCE.md >/dev/null; then
  fail "found forbidden readiness claim"
fi
pass "no v2.3.0/v3.0 readiness claims in root docs"

if [[ $strict -eq 1 ]]; then
  if rg -n -i 'needs maintainer review' docs/CLEANUP_CANDIDATES_V2_2_18.md >/dev/null; then
    rg -n -A2 -i 'needs maintainer review' docs/CLEANUP_CANDIDATES_V2_2_18.md | rg -q -- '- None\.' || fail "strict: cleanup candidates still contain needs maintainer review"
  fi

  if rg -n 'PENDING_REVIEW' docs/CLEANUP_AUDIT_V2_2_18_PASS2.md >/dev/null; then
    rg -n 'CLEANUP_AUDIT_V2_2_18_FINAL\.md' docs/CLEANUP_AUDIT_V2_2_18_PASS2.md >/dev/null || fail "strict: pass2 has unresolved PENDING_REVIEW without final audit reference"
  fi

  ps1_files="$(git ls-files '*.ps1' || true)"
  if [[ -n "$ps1_files" ]]; then
    while IFS= read -r ps1; do
      [[ -z "$ps1" ]] && continue
      if [[ "$ps1" == scripts/archive/* ]]; then
        rg -n "\| ${ps1//./\.} \| MOVE_ARCHIVE \|" docs/CLEANUP_AUDIT_V2_2_18_FINAL.md >/dev/null || fail "strict: archived PowerShell script missing MOVE_ARCHIVE audit row: $ps1"
        rg -n "$ps1" docs README.md .github --glob '!docs/archive/**' --glob '!docs/CLEANUP_AUDIT_V2_2_18*.md' >/dev/null && fail "strict: archived script referenced as active: $ps1"
      else
        rg -n "\| ${ps1//./\.} \| KEEP_CURRENT \|" docs/CLEANUP_AUDIT_V2_2_18_FINAL.md >/dev/null || fail "strict: remaining PowerShell script not marked KEEP_CURRENT in final audit: $ps1"
      fi
    done <<< "$ps1_files"
  fi

  # current docs must not point to missing moved/deleted script paths
  while IFS= read -r ref; do
    path="$(echo "$ref" | cut -d: -f3)"
    [[ -z "$path" ]] && continue
    [[ -e "$path" ]] || fail "strict: current doc references missing script path: $path"
  done < <(python3 - <<'PY'
from pathlib import Path
import re
for md in Path('docs').rglob('*.md'):
    if 'docs/archive' in str(md):
        continue
    txt=md.read_text(encoding='utf-8',errors='ignore')
    for m in re.findall(r'(scripts/[A-Za-z0-9_\-./]+)', txt):
        print(f"{md}:{m}")
PY
)

  old_set=(docs/ROADMAP_V2_3_0.md docs/V3_READINESS.md)
  for f in "${old_set[@]}"; do
    [[ -f "$f" ]] && fail "strict: old doc still in docs root: $f"
  done

  pass "strict checks passed"
fi
rm -f "$tracked"
pass "repository cleanup validation completed"
