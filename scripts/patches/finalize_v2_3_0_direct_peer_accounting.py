#!/usr/bin/env python3
"""Finalize direct-session observability assertions after the primary source rewrite."""

from pathlib import Path

path = Path("crates/pulsedag-p2p/src/lib.rs")
text = path.read_text(encoding="utf-8")

replacements = [
    (
        """    if health.connected {
        states.push("connected".to_string());
    }
""",
        """    if active && health.connected {
        states.push("connected".to_string());
    }
""",
        "health-state connected flag",
    ),
    (
        """        assert!(state.peer_book.contains_key(peer));
        assert!(state.connected_peers.is_empty());
        assert!(sync_candidates_snapshot(&state)
            .iter()
            .all(|candidate| candidate.peer_id != peer));
        assert!(recovery
""",
        """        assert!(state
            .peer_book
            .get(peer)
            .is_some_and(|health| !health.connected));
        assert!(state.connected_peers.is_empty());
        assert!(recovery
""",
        "indirect gossip regression assertion",
    ),
]

for old, new, label in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
