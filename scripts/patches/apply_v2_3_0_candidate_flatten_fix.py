#!/usr/bin/env python3
"""Replace the shell candidate flattener with the deterministic Python helper."""

from pathlib import Path

path = Path(".github/workflows/v2_3_0_release_candidate.yml")
text = path.read_text(encoding="utf-8")
old = '''      - name: Flatten candidate assets
        shell: bash
        run: |
          set -euo pipefail
          rm -rf final
          mkdir -p final
          files=$(find artifacts -type f | wc -l)
          test "$files" -eq 18
          duplicate_names=$(find artifacts -type f -printf '%f\\n' | sort | uniq -d)
          if [ -n "$duplicate_names" ]; then
            echo "duplicate candidate asset names:" >&2
            echo "$duplicate_names" >&2
            exit 1
          fi
          find artifacts -type f -exec cp "{}" final/ \
;
'''
new = '''      - name: Flatten candidate assets
        shell: bash
        run: |
          set -euo pipefail
          python3 scripts/release/flatten_candidate_assets.py \
            --source artifacts \
            --dest final
'''
if old not in text:
    raise SystemExit("expected candidate flatten block was not found")
text = text.replace(old, new, 1)
path.write_text(text, encoding="utf-8")
