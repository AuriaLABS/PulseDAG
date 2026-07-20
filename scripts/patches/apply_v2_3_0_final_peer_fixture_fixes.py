#!/usr/bin/env python3
"""Apply the two remaining direct-session fixture corrections."""

from pathlib import Path

path = Path("crates/pulsedag-p2p/src/lib.rs")
text = path.read_text(encoding="utf-8")


def replace_in_test(test_name: str, old: str, new: str) -> None:
    global text
    marker = f"    fn {test_name}() {{"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"test not found: {test_name}")
    end = text.find("\n    #[test]", start + len(marker))
    if end < 0:
        end = len(text)
    section = text[start:end]
    count = section.count(old)
    if count != 1:
        raise SystemExit(f"{test_name}: expected one match, found {count}")
    section = section.replace(old, new, 1)
    text = text[:start] + section + text[end:]


replace_in_test(
    "degraded_peers_are_cooled_down_without_starving_healthy_peers",
    """        state.active_connections.insert("peer-healthy".into(), 1);
        state
            .active_connections
            .insert("peer-degraded-a".into(), 1);
        state
            .active_connections
            .insert("peer-degraded-b".into(), 1);

""",
    """        state.active_connections.insert("peer-healthy".into(), 1);
        state
            .active_connections
            .insert("peer-degraded-a".into(), 1);

""",
)
replace_in_test(
    "connection_shaping_reduces_churn_loops_under_stress",
    '        state.connected_peers = vec!["peer-b".into()];\n',
    '        state.connected_peers = vec!["peer-a".into(), "peer-b".into()];\n',
)

path.write_text(text, encoding="utf-8")
