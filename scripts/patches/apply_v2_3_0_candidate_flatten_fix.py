#!/usr/bin/env python3
"""Replace the shell candidate flattener with the deterministic Python helper."""

from pathlib import Path

path = Path(".github/workflows/v2_3_0_release_candidate.yml")
text = path.read_text(encoding="utf-8")
start_marker = "      - name: Flatten candidate assets\n"
end_marker = "      - name: Verify checksums and manifests\n"
start = text.find(start_marker)
end = text.find(end_marker, start + len(start_marker))
if start < 0 or end < 0 or end <= start:
    raise SystemExit("candidate flatten workflow block was not found")
replacement = '''      - name: Flatten candidate assets
        shell: bash
        run: |
          set -euo pipefail
          python3 scripts/release/flatten_candidate_assets.py \
            --source artifacts \
            --dest final

'''
text = text[:start] + replacement + text[end:]
path.write_text(text, encoding="utf-8")
