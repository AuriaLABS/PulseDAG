#!/usr/bin/env python3
from pathlib import Path

path = Path(__file__).with_name("apply_v2_3_0_runtime_round5_patch.py")
text = path.read_text()
old_import = "from pathlib import Path\n"
if text.count(old_import) != 1:
    raise SystemExit(f"expected one pathlib import, found {text.count(old_import)}")
text = text.replace("from pathlib import Path\n", "import re\nfrom pathlib import Path\n", 1)
old = '''replace_once(
    node,
    "                .unwrap_or(32)\\n"
    "                .clamp(1, 128),\\n",
    "                .unwrap_or(128)\\n"
    "                .clamp(1, 128),\\n",
    "selected-segment closeout capacity",
)
'''
new = '''node_text = node.read_text()
node_text, capacity_count = re.subn(
    r"(?P<indent>\\s*)\\.unwrap_or\\(32\\)\\n(?P=indent)\\.clamp\\(1, 128\\),",
    lambda match: (
        f"{match.group('indent')}.unwrap_or(128)\\n"
        f"{match.group('indent')}.clamp(1, 128),"
    ),
    node_text,
    count=1,
)
if capacity_count != 1:
    raise SystemExit(
        f"{node}: expected one selected-segment closeout capacity regex match, found {capacity_count}"
    )
node.write_text(node_text)
'''
if text.count(old) != 1:
    raise SystemExit(f"expected one capacity patch block, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))
print("round-5 patcher capacity match updated")
