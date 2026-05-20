#!/usr/bin/env bash
set -euo pipefail

out_file=""
if [[ "${1:-}" == "--write-artifact" ]]; then
  out_file="artifacts/cleanup_candidates_report.txt"
  mkdir -p artifacts
fi

emit() { echo "$*"; }

{
emit "== Tracked generated/runtime junk =="
git ls-files | rg '\.(log|tmp|bak|old|orig|swp|swo|zip|tar\.gz)$|(^|/)(target|logs|run|artifacts)/|\.DS_Store|Thumbs\.db|desktop\.ini' || true

emit "\n== Old docs still in docs/ root =="
find docs -maxdepth 1 -type f | rg 'RELEASE_NOTES_V2_2_([0-9]|1[0-6])\.md|CLOSING_CHECKLIST_V2_2_([0-9]|1[0-6])\.md|ROADMAP_V2_2_([0-9]|1[0-6])\.md|SMOKE_TEST_V2_2_([0-9]|1[0-2])\.md|ROADMAP_V2_3_0\.md|V3_READINESS\.md' || true

emit "\n== Candidate stale scripts =="
find scripts -maxdepth 1 -type f | rg '(old|legacy|backup|copy|tmp|smoke_v2_2_7|v2_2_9_)' || true

emit "\n== PowerShell scripts classification hint =="
if [[ -f docs/CLEANUP_AUDIT_V2_2_18_FINAL.md ]]; then
  while IFS= read -r ps1; do
    [[ -z "$ps1" ]] && continue
    if rg -q "\| ${ps1//./\.} \| KEEP_CURRENT \|" docs/CLEANUP_AUDIT_V2_2_18_FINAL.md; then
      echo "KEEP_CURRENT: $ps1"
    elif [[ "$ps1" == scripts/archive/* ]]; then
      echo "MOVE_ARCHIVE: $ps1"
    else
      echo "UNCLASSIFIED: $ps1"
    fi
  done < <(git ls-files '*.ps1' || true)
else
  echo "Final audit missing: docs/CLEANUP_AUDIT_V2_2_18_FINAL.md"
  git ls-files '*.ps1' || true
fi

emit "\n== Broken markdown local links (quick pass) =="
python3 - <<'PY'
from pathlib import Path
import re
root=Path('.')
link_re=re.compile(r'\[[^\]]+\]\(([^)]+)\)')
for md in root.rglob('*.md'):
    if '.git' in md.parts: continue
    text=md.read_text(encoding='utf-8',errors='ignore')
    for t in link_re.findall(text):
        t=t.strip()
        if not t or t.startswith('#') or '://' in t or t.startswith('mailto:'): continue
        p=(md.parent/t.split('#',1)[0]).resolve()
        try: p.relative_to(root.resolve())
        except Exception: continue
        if not p.exists(): print(f"{md}: {t}")
PY

emit "\n== Stale references to archived files/scripts from current docs/workflows =="
rg -n 'docs/archive/|scripts/archive/' docs .github README.md --glob '!docs/archive/**' || true

emit "\n== Forbidden readiness claims in root docs =="
rg -n -i 'ready for v2\.3\.0|ready for v3\.0|v2\.3\.0 is current|v3\.0 is current' README.md docs/VERSION_MATRIX.md docs/RELEASE_EVIDENCE.md || true
} | {
  if [[ -n "$out_file" ]]; then tee "$out_file"; else cat; fi
}

if [[ -n "$out_file" ]]; then
  echo "Wrote report to $out_file"
fi
