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
    (node, r"selected_segment_request_candidates\s*\(", "selected request candidates"),
    (node, r"block_requests\.resolve\(&hash\);\s*warn!", "failed selected request released"),
    (node, r"for\s+adopted_hash\s+in\s+&adopted_hashes", "adopted selected blocks accounted"),
    (node, r"selected_segment_gap_blocks\s*=\s*remote_height\s*\.saturating_sub\(local_height\)", "selected gap"),
    (node, r"correlates_pending_header_page\s*\(", "continuation page correlation"),
    (node, r"accept_header_page\s*\(", "continuation locator rotation"),
    (node, r"headers_correlated\s*=\s*selected_session_owns_headers", "shared header ownership semantics"),
    (node, r"persist_blocks_and_chain_state\s*\(", "adopted block durability batch"),
    (metrics, r"remote_sync_evidence_from_p2p_status\s*\(", "remote evidence"),
    (metrics, r"build_canonical_sync_state_with_remote_evidence\s*\(", "canonical builder"),
    (metrics, r"&remote_sync_evidence", "evidence supplied"),
]
missing = [label for source, pattern, label in checks if not re.search(pattern, source)]
if "!staged.contains(hash)" in node:
    missing.append("generic scheduler staging still suppresses selected requests")
if re.search(r"for\s+hash\s+in\s+&candidates\s*\{\s*session\.requested_hashes\.insert", node):
    missing.append("selected hashes marked requested before transport success")
if "storage.persist_chain_state(&adopted_guard)" in node:
    missing.append("adopted orphans still snapshot-only persisted")
if missing:
    raise SystemExit("missing safeguards: " + ", ".join(missing))
