#!/usr/bin/env bash
set -euo pipefail

out_file=""
if [[ "${1:-}" == "--write-artifact" ]]; then
  out_file="artifacts/repository-hygiene/cleanup-candidates.txt"
  mkdir -p "$(dirname "$out_file")"
elif [[ -n "${1:-}" ]]; then
  echo "usage: $0 [--write-artifact]" >&2
  exit 2
fi

emit() { printf '%s\n' "$*"; }
search() {
  if command -v rg >/dev/null 2>&1; then
    rg "$@"
  else
    grep -RInE "$@"
  fi
}

report() {
  emit "== Repository cleanup candidate inventory =="
  emit "Generated at: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  emit "Commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)"
  emit ""

  emit "== Tracked generated/runtime/editor files =="
  git ls-files | grep -E '\.(log|tmp|bak|old|orig|swp|swo|zip|tar\.gz|pid|profraw|profdata)$|(^|/)(target|logs|run|ci-evidence|node_modules|__pycache__)/|\.DS_Store$|Thumbs\.db$|desktop\.ini$' || true
  emit ""

  emit "== Secret-like tracked filenames requiring review =="
  git ls-files | grep -Ei '(^|/)\.env($|\.)|(^|/)(id_rsa|id_ed25519)$|\.(pem|key|p12|pfx|private-key)$' | grep -Ev '(example|sample|fixture|testdata|/tests?/)' || true
  emit ""

  emit "== Version-specific documents in the active docs root =="
  find docs -maxdepth 1 -type f -printf '%p\n' | grep -E '/(ROADMAP|RELEASE_NOTES|CLOSING_CHECKLIST|SMOKE_TEST|PREFLIGHT|V2_[0-9_]+).*\.md$' | sort || true
  emit ""

  emit "== Version-specific scripts in the active scripts root =="
  find scripts -maxdepth 1 -type f -printf '%p\n' | grep -E '/v[0-9_]+.*\.(sh|py|ps1)$' | sort || true
  emit ""

  emit "== Files with legacy/temporary naming =="
  git ls-files | grep -Ei '(^|/)(old|legacy|backup|copy|tmp)[^/]*$|\.(old|bak|orig)$' || true
  emit ""

  emit "== Large source and script files (over 1,200 lines) =="
  python3 - <<'PY'
from pathlib import Path
import subprocess

tracked = subprocess.run(
    ["git", "ls-files", "-z", "--", "apps", "crates", "scripts"],
    check=True,
    capture_output=True,
).stdout.split(b"\0")
for raw in tracked:
    if not raw:
        continue
    path = Path(raw.decode("utf-8"))
    if path.suffix not in {".rs", ".py", ".sh"} or not path.exists():
        continue
    try:
        count = sum(1 for _ in path.open(encoding="utf-8", errors="ignore"))
    except OSError:
        continue
    if count > 1200:
        print(f"{count:6d} {path}")
PY
  emit ""

  emit "== TODO/FIXME/XXX maintenance markers =="
  if command -v rg >/dev/null 2>&1; then
    rg -n --hidden -g '!.git/**' -g '!docs/archive/**' '\b(TODO|FIXME|XXX|HACK)\b' apps crates scripts .github configs 2>/dev/null || true
  else
    grep -RInE '\b(TODO|FIXME|XXX|HACK)\b' apps crates scripts .github configs 2>/dev/null || true
  fi
  emit ""

  emit "== Active references to archived material =="
  if command -v rg >/dev/null 2>&1; then
    rg -n 'docs/archive/|scripts/archive/' README.md CONTRIBUTING.md docs .github --glob '!docs/archive/**' 2>/dev/null || true
  else
    grep -RInE 'docs/archive/|scripts/archive/' README.md CONTRIBUTING.md docs .github 2>/dev/null || true
  fi
  emit ""

  emit "== Non-English code-comment candidates =="
  python3 scripts/check_code_comment_language.py --report 2>&1 || true
  emit ""

  emit "== Hygiene summary =="
  bash scripts/repository_hygiene.sh --report || true
}

if [[ -n "$out_file" ]]; then
  report | tee "$out_file"
  emit "Wrote report to $out_file"
else
  report
fi
