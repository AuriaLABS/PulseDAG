#!/usr/bin/env python3
import re
import sys
from pathlib import Path

node = Path(sys.argv[1]).read_text()
metrics = Path(sys.argv[2]).read_text()
checks = [
    (node, r"let\s+mut\s+selected_segment_completed\s*=\s*false", "completion flag"),
    (node, r"selected_segment_session\s*=\s*None", "session cleared"),
    (node, r"active_session_remaining_blocks\s*=\s*0", "remaining cleared"),
    (node, r"peer_addressed_getblock_sent_total\s*=\s*rt\s*\.peer_addressed_getblock_sent_total\s*\.saturating_add\(1\)", "peer addressed accounting"),
    (node, r"selected_segment_gap_blocks\s*=\s*remote_height\s*\.saturating_sub\(local_height\)", "selected gap"),
    (metrics, r"remote_sync_evidence_from_p2p_status\s*\(", "remote evidence"),
    (metrics, r"build_canonical_sync_state_with_remote_evidence\s*\(", "canonical builder"),
    (metrics, r"&remote_sync_evidence", "evidence supplied"),
]
missing = [label for source, pattern, label in checks if not re.search(pattern, source)]
if missing:
    raise SystemExit("missing safeguards: " + ", ".join(missing))
