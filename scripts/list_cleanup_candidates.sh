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

emit "\n== Old version docs in docs/ root (v2.2.x + v2.3/v3 readiness) =="
find docs -maxdepth 1 -type f | rg 'V2_2_|V2_3|V3_READINESS' || true

emit "\n== Candidate stale scripts =="
find scripts -maxdepth 1 -type f | rg '(old|legacy|backup|copy|tmp|smoke_v2_2_7|v2_2_9_)' || true

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

emit "\n== Keyword hits (obsolete/deprecated/legacy/stale/old/backup/copy) =="
rg -n -i 'obsolete|deprecated|legacy|stale|\bold\b|backup|\bcopy\b' docs scripts .github README.md || true
} | {
  if [[ -n "$out_file" ]]; then tee "$out_file"; else cat; fi
}

if [[ -n "$out_file" ]]; then
  echo "Wrote report to $out_file"
fi
