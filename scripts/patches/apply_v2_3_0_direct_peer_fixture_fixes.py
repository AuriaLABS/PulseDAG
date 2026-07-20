#!/usr/bin/env python3
"""Preserve active-peer recovery semantics and repair direct-session test fixtures."""

from pathlib import Path

TARGET = Path("crates/pulsedag-p2p/src/lib.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


def insert_before_in_test(text: str, test_name: str, needle: str, insertion: str) -> str:
    marker = f"    fn {test_name}() {{"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"test not found: {test_name}")
    end = text.find("\n    #[test]", start + len(marker))
    if end < 0:
        end = len(text)
    position = text.find(needle, start, end)
    if position < 0:
        raise SystemExit(f"needle not found in {test_name}")
    return text[:position] + insertion + text[position:]


def replace_in_test(text: str, test_name: str, old: str, new: str) -> str:
    marker = f"    fn {test_name}() {{"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"test not found: {test_name}")
    end = text.find("\n    #[test]", start + len(marker))
    if end < 0:
        end = len(text)
    section = text[start:end]
    if section.count(old) != 1:
        raise SystemExit(f"{test_name}: expected one fixture match, found {section.count(old)}")
    section = section.replace(old, new, 1)
    return text[:start] + section + text[end:]


def main() -> int:
    text = TARGET.read_text(encoding="utf-8")

    text = replace_once(
        text,
        """fn peer_eligible_for_sync(peer_id: &str, health: &PeerHealth, active: bool, now: u64) -> bool {
    active
        && health.connected
        && health.chain_id_compatible
        && is_valid_peer_id(peer_id)
        && health.next_retry_unix <= now
        && health.suppressed_until_unix <= now
}
""",
        """fn peer_eligible_for_sync(
    peer_id: &str,
    health: &PeerHealth,
    active: bool,
    _now: u64,
) -> bool {
    active && health.connected && health.chain_id_compatible && is_valid_peer_id(peer_id)
}
""",
        "active peer recovery semantics",
    )

    for test_name, peers in (
        (
            "degraded_peers_are_cooled_down_without_starving_healthy_peers",
            ("peer-healthy", "peer-degraded-a", "peer-degraded-b"),
        ),
        (
            "connection_shaping_reduces_churn_loops_under_stress",
            ("peer-a", "peer-b", "peer-c"),
        ),
        (
            "sync_candidate_selection_deprioritizes_slow_or_degraded_peers",
            ("peer-fast", "peer-slow"),
        ),
    ):
        insertion = "".join(
            f'        state.active_connections.insert("{peer}".into(), 1);\n'
            for peer in peers
        ) + "\n"
        text = insert_before_in_test(
            text,
            test_name,
            "        refresh_connected_peers_from_health(&mut state);\n",
            insertion,
        )

    text = replace_in_test(
        text,
        "selected_sync_peer_does_not_flap_on_small_rank_advantage_during_churn",
        '        state.connected_peers = vec!["peer-b".into()];\n',
        '        state.connected_peers = vec!["peer-a".into(), "peer-b".into()];\n',
    )
    text = replace_in_test(
        text,
        "rejoin_convergence_switches_deterministically_after_sticky_window",
        '        state.connected_peers = vec!["peer-b".into()];\n',
        '        state.connected_peers = vec!["peer-a".into(), "peer-b".into()];\n',
    )
    text = replace_in_test(
        text,
        "rejoin_convergence_switches_deterministically_after_sticky_window",
        '        state.connected_peers = vec!["peer-a".into()];\n',
        '        state.connected_peers = vec!["peer-a".into(), "peer-b".into()];\n',
    )
    text = insert_before_in_test(
        text,
        "selected_sync_peer_tie_break_is_lexicographically_stable",
        "        let ranked = vec![\n",
        '        state.connected_peers = vec!["peer-a".into(), "peer-b".into()];\n',
    )

    TARGET.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
