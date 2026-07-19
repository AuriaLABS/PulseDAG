#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

bash -n scripts/repository_hygiene.sh
bash -n scripts/list_cleanup_candidates.sh
bash -n scripts/validate_repo_cleanup.sh
python3 -m py_compile \
  scripts/check_code_comment_language.py \
  scripts/repository_hygiene.py

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

cat > "$tmp_dir/good.rs" <<'RS'
// Preserve the request identifier so continuation pages remain correlated.
fn main() {}
RS
python3 scripts/check_code_comment_language.py "$tmp_dir/good.rs"

cat > "$tmp_dir/bad.rs" <<'RS'
// Comprueba que el nodo conserva la identidad.
fn main() {}
RS
if python3 scripts/check_code_comment_language.py "$tmp_dir/bad.rs" >/dev/null 2>&1; then
  echo "expected Spanish comment detection to fail" >&2
  exit 1
fi
python3 scripts/check_code_comment_language.py --report "$tmp_dir/bad.rs" >/dev/null 2>&1

cat > "$tmp_dir/localized.py" <<'PY'
# language-check: allow — localized user-facing fixture: operación completada
print("ok")
PY
python3 scripts/check_code_comment_language.py "$tmp_dir/localized.py"

if grep -Eq 'v2_2_17|V2_2_17|v2_2_18|V2_2_18|old doc still in docs root:.*ROADMAP_V2_3_0' \
  scripts/validate_repo_cleanup.sh \
  scripts/list_cleanup_candidates.sh; then
  echo "stale release-pinned cleanup policy detected" >&2
  exit 1
fi

OUT_DIR="$tmp_dir/evidence" bash scripts/repository_hygiene.sh --strict
python3 - "$tmp_dir/evidence/repository-hygiene.json" <<'PY'
import json
import sys
from pathlib import Path

manifest = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert manifest["gate"] == "repository-hygiene"
assert manifest["result"] == "PASS"
assert manifest["failure_count"] == 0
assert manifest["public_testnet_ready"] is False
assert manifest["thirty_day_public_testnet_clock_started"] is False
PY

echo "PASS: repository hygiene contract regression"
