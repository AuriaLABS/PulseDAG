#!/usr/bin/env python3
"""Replace the historical packaged install guide with the v2.3.0 candidate guide."""

from pathlib import Path

path = Path(".github/workflows/release-binaries.yml")
text = path.read_text(encoding="utf-8")
old = "docs/INSTALL_BINARIES_V2_2_19.md"
new = "docs/INSTALL_BINARIES_V2_3_0.md"
count = text.count(old)
if count != 2:
    raise SystemExit(f"expected two historical install-guide references, found {count}")
text = text.replace(old, new)
if text.count(new) != 2:
    raise SystemExit("v2.3.0 install-guide replacement count is not two")
path.write_text(text, encoding="utf-8")
