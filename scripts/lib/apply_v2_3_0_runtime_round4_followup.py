#!/usr/bin/env python3
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_once(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one {label} match, found {count}")
    path.write_text(text.replace(old, new, 1))


main = ROOT / "apps/pulsedagd/src/main.rs"
text = main.read_text()
pattern = re.compile(
    r"^(?P<i>[ \t]*)let canonical_gap = remote_height\.saturating_sub\(local_height\);\n"
    r"(?P=i)rt\.selected_segment_gap_blocks = canonical_gap;\n"
    r"(?P=i)rt\.network_selected_height_gap =\n"
    r"(?P=i)    rt\.network_selected_height_gap\.max\(canonical_gap\);\n",
    re.MULTILINE,
)
matches = list(pattern.finditer(text))
if len(matches) != 1:
    raise SystemExit(f"{main}: expected one invalid canonical gap block, found {len(matches)}")
indent = matches[0].group("i")
main.write_text(pattern.sub(
    f"{indent}rt.selected_segment_gap_blocks =\n"
    f"{indent}    remote_height.saturating_sub(local_height);\n",
    text,
    count=1,
))

metrics = ROOT / "crates/pulsedag-rpc/src/handlers/metrics.rs"
replace_once(
    metrics,
    "    handlers::canonical_sync::build_canonical_sync_state,\n",
    "    handlers::canonical_sync::{\n"
    "        build_canonical_sync_state_with_remote_evidence,\n"
    "        remote_sync_evidence_from_p2p_status,\n"
    "    },\n",
    "canonical sync imports",
)
replace_once(
    metrics,
    "    let canonical_sync = build_canonical_sync_state(\n"
    "        chain,\n"
    "        runtime,\n"
    "        chain.dag.blocks.len(),\n"
    "        now_unix,\n"
    "        p2p_status\n"
    "            .as_ref()\n"
    "            .and_then(|snapshot| snapshot.status.selected_sync_peer.clone()),\n"
    "    );\n",
    "    let remote_sync_evidence = remote_sync_evidence_from_p2p_status(\n"
    "        p2p_status.as_ref().map(|snapshot| &snapshot.status),\n"
    "        now_unix,\n"
    "    );\n"
    "    let canonical_sync = build_canonical_sync_state_with_remote_evidence(\n"
    "        chain,\n"
    "        runtime,\n"
    "        chain.dag.blocks.len(),\n"
    "        now_unix,\n"
    "        p2p_status\n"
    "            .as_ref()\n"
    "            .and_then(|snapshot| snapshot.status.selected_sync_peer.clone()),\n"
    "        &remote_sync_evidence,\n"
    "    );\n",
    "metrics canonical sync evidence wiring",
)

contract = ROOT / "scripts/tests/test_v2_3_0_lag_runtime_driver.sh"
replace_once(
    contract,
    'NODE_MAIN="apps/pulsedagd/src/main.rs"\n',
    'NODE_MAIN="apps/pulsedagd/src/main.rs"\nMETRICS="crates/pulsedag-rpc/src/handlers/metrics.rs"\n',
    "metrics source path",
)
replace_once(
    contract,
    "grep -Fq 'let mut selected_segment_completed = false;' \"$NODE_MAIN\"\n"
    "grep -Fq 'selected_segment_session = None;' \"$NODE_MAIN\"\n"
    "grep -Fq 'rt.active_session_remaining_blocks = 0;' \"$NODE_MAIN\"\n"
    "grep -Fq 'rt.peer_addressed_getblock_sent_total = rt' \"$NODE_MAIN\"\n"
    "grep -Fq 'rt.network_selected_height_gap.max(canonical_gap)' \"$NODE_MAIN\"\n",
    'python3 scripts/tests/test_v2_3_0_selected_segment_source_semantics.py "$NODE_MAIN" "$METRICS"\n',
    "selected segment source assertions",
)
replace_once(
    contract,
    'grep -Fq \'ss -K state established\' "$tmp/patched-harness.sh"\n',
    "",
    "obsolete socket assertion",
)

(ROOT / "scripts/tests/test_v2_3_0_selected_segment_source_semantics.py").write_text(
    '''#!/usr/bin/env python3
import re
import sys
from pathlib import Path

node = Path(sys.argv[1]).read_text()
metrics = Path(sys.argv[2]).read_text()
checks = [
    (node, r"let\\s+mut\\s+selected_segment_completed\\s*=\\s*false", "completion flag"),
    (node, r"selected_segment_session\\s*=\\s*None", "session cleared"),
    (node, r"active_session_remaining_blocks\\s*=\\s*0", "remaining cleared"),
    (node, r"peer_addressed_getblock_sent_total\\s*=\\s*rt\\s*\\.peer_addressed_getblock_sent_total\\s*\\.saturating_add\\(1\\)", "peer addressed accounting"),
    (node, r"selected_segment_gap_blocks\\s*=\\s*remote_height\\s*\\.saturating_sub\\(local_height\\)", "selected gap"),
    (metrics, r"remote_sync_evidence_from_p2p_status\\s*\\(", "remote evidence"),
    (metrics, r"build_canonical_sync_state_with_remote_evidence\\s*\\(", "canonical builder"),
    (metrics, r"&remote_sync_evidence", "evidence supplied"),
]
missing = [label for source, pattern, label in checks if not re.search(pattern, source)]
if missing:
    raise SystemExit("missing safeguards: " + ", ".join(missing))
'''
)

print("runtime round-4 follow-up patch applied")
