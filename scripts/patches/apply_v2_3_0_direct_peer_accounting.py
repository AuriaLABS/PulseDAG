#!/usr/bin/env python3
"""Apply the reviewed Task 12 direct-transport peer-accounting correction."""

from __future__ import annotations

from pathlib import Path

TARGET = Path("crates/pulsedag-p2p/src/lib.rs")


def replace_exact(text: str, old: str, new: str, label: str) -> str:
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
        raise SystemExit(f"needle not found in test {test_name}: {needle!r}")
    return text[:position] + insertion + text[position:]


def main() -> int:
    text = TARGET.read_text(encoding="utf-8")

    text = replace_exact(
        text,
        """            score: 100,
            fail_streak: 0,
            next_retry_unix: 0,
            connected: true,
""",
        """            score: 100,
            fail_streak: 0,
            next_retry_unix: 0,
            connected: false,
""",
        "peer health defaults disconnected",
    )

    text = replace_exact(
        text,
        """fn peer_eligible_for_sync(peer_id: &str, health: &PeerHealth, active: bool, now: u64) -> bool {
    health.connected
        && health.chain_id_compatible
        && is_valid_peer_id(peer_id)
        && (active || (health.next_retry_unix <= now && health.suppressed_until_unix <= now))
}
""",
        """fn peer_eligible_for_sync(peer_id: &str, health: &PeerHealth, active: bool, now: u64) -> bool {
    active
        && health.connected
        && health.chain_id_compatible
        && is_valid_peer_id(peer_id)
        && health.next_retry_unix <= now
        && health.suppressed_until_unix <= now
}
""",
        "sync eligibility requires direct transport",
    )

    text = replace_exact(
        text,
        """        .map(|(peer_id, health)| {
            let active = state.active_connections.get(peer_id).copied().unwrap_or(0) > 0;
            let eligible_for_sync = peer_eligible_for_sync(peer_id, health, active, now);
""",
        """        .map(|(peer_id, health)| {
            let direct_active = state.active_connections.get(peer_id).copied().unwrap_or(0) > 0;
            let transport_active = if mode_connected_peers_are_real_network(&state.mode) {
                direct_active
            } else {
                health.connected
            };
            let eligible_for_sync =
                peer_eligible_for_sync(peer_id, health, transport_active, now);
""",
        "peer recovery derives transport activity",
    )
    text = replace_exact(
        text,
        """                connected: health.connected,
                last_seen_unix: health.last_seen_unix,
""",
        """                connected: transport_active && health.connected,
                last_seen_unix: health.last_seen_unix,
""",
        "peer recovery connected surface",
    )
    text = replace_exact(
        text,
        """                health_states: peer_health_states(peer_id, health, active, now),
""",
        """                health_states: peer_health_states(peer_id, health, transport_active, now),
""",
        "peer recovery health states",
    )

    text = replace_exact(
        text,
        """        .map(|(peer_id, health)| SyncPeerCandidate {
            peer_id: peer_id.clone(),
            score: health.score,
            fail_streak: health.fail_streak,
            connected: health.connected,
""",
        """        .map(|(peer_id, health)| SyncPeerCandidate {
            peer_id: peer_id.clone(),
            score: health.score,
            fail_streak: health.fail_streak,
            connected: health.connected
                && (!mode_connected_peers_are_real_network(&state.mode)
                    || state.active_connections.get(peer_id).copied().unwrap_or(0) > 0),
""",
        "ranked candidates use direct transport",
    )

    text = replace_exact(
        text,
        """                    health.connected
                        && health.chain_id_compatible
                        && (peer.excluded_until_unix.is_none()
                            || active_peer_ids.contains(&peer.peer_id))
""",
        """                    active_peer_ids.contains(&peer.peer_id)
                        && health.connected
                        && health.chain_id_compatible
                        && (peer.excluded_until_unix.is_none()
                            || active_peer_ids.contains(&peer.peer_id))
""",
        "connected peers require active session",
    )

    text = replace_exact(
        text,
        """    let preferred = sync_candidates
        .iter()
        .filter(|candidate| is_valid_peer_id(&candidate.peer_id))
        .filter(|candidate| candidate.excluded_until_unix.is_none())
""",
        """    let direct_transport_required = mode_connected_peers_are_real_network(&state.mode);
    let preferred = sync_candidates
        .iter()
        .filter(|candidate| is_valid_peer_id(&candidate.peer_id))
        .filter(|candidate| candidate.excluded_until_unix.is_none())
        .filter(|candidate| {
            !direct_transport_required || state.connected_peers.contains(&candidate.peer_id)
        })
""",
        "selected sync peer filters direct sessions",
    )
    text = replace_exact(
        text,
        """        .map(|peer| {
            state.connected_peers.contains(peer)
                || sync_candidates.iter().any(|candidate| {
                    candidate.peer_id == *peer && candidate.excluded_until_unix.is_none()
                })
        })
""",
        """        .map(|peer| {
            state.connected_peers.contains(peer)
                || (!direct_transport_required
                    && sync_candidates.iter().any(|candidate| {
                        candidate.peer_id == *peer && candidate.excluded_until_unix.is_none()
                    }))
        })
""",
        "sticky sync peer requires direct session",
    )

    text = insert_before_in_test(
        text,
        "compatible_connected_peers_exclude_chain_mismatch_peers",
        "        refresh_connected_peers_from_health(&mut state);\n",
        """        state
            .active_connections
            .insert("peer-compatible".into(), 1);
        state
            .active_connections
            .insert("peer-wrong-chain".into(), 1);

""",
    )
    text = insert_before_in_test(
        text,
        "connection_slot_budget_snapshot_is_deterministic",
        "        refresh_connected_peers_from_health(&mut state);\n",
        """        state.active_connections.insert("peer-a".into(), 1);
        state.active_connections.insert("peer-b".into(), 1);

""",
    )
    for test_name, insertion in (
        (
            "refresh_connected_peers_excludes_disconnected_ranked_candidates",
            "        state.active_connections.insert(\"peer-live\".into(), 1);\n\n",
        ),
        (
            "connection_budget_caps_connected_peer_surface",
            "        state.active_connections.insert(\"peer-a\".into(), 1);\n        state.active_connections.insert(\"peer-b\".into(), 1);\n\n",
        ),
        (
            "topology_aware_shaping_still_respects_health_and_budget_constraints",
            "        state.active_connections.insert(\"healthy-a\".into(), 1);\n        state.active_connections.insert(\"healthy-b\".into(), 1);\n\n",
        ),
        (
            "constrained_slots_keep_selection_coherent_with_connected_set",
            "        state.active_connections.insert(\"peer-primary\".into(), 1);\n\n",
        ),
        (
            "selection_respects_health_and_budget_constraints_under_hysteresis",
            "        state.active_connections.insert(\"peer-healthy\".into(), 1);\n\n",
        ),
    ):
        text = insert_before_in_test(
            text,
            test_name,
            "        refresh_connected_peers_from_health(&mut state);\n",
            insertion,
        )

    text = insert_before_in_test(
        text,
        "topology_diversity_prevents_slot_collapse_when_alternatives_exist",
        "        refresh_connected_peers_from_health(&mut state);\n",
        """        for peer in peers_bucket_0
            .iter()
            .take(2)
            .chain(peers_bucket_1.iter().take(2))
        {
            state.active_connections.insert(peer.clone(), 1);
        }

""",
    )

    regression_anchor = """    #[test]
    fn per_peer_inbound_message_budget_rate_limits_noisy_peer() {
"""
    regression = """    #[test]
    fn indirect_gossip_author_does_not_become_a_connected_transport_peer() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            ..InnerState::default()
        };
        let peer = "peer-indirect-gossip";
        score_peer_message_outcome(
            &mut state,
            peer,
            PeerMessageOutcome::ValidRelay,
            now_unix(),
        );

        refresh_connected_peers_from_health(&mut state);
        let recovery = peer_recovery_snapshot(&state).13;

        assert!(state.peer_book.contains_key(peer));
        assert!(state.connected_peers.is_empty());
        assert!(sync_candidates_snapshot(&state)
            .iter()
            .all(|candidate| candidate.peer_id != peer));
        assert!(recovery
            .iter()
            .any(|entry| entry.peer_id == peer && !entry.connected && !entry.eligible_for_sync));
    }

    #[test]
    fn real_transport_session_controls_connected_and_selected_sync_surfaces() {
        let mut state = InnerState {
            mode: P2P_MODE_LIBP2P_REAL.into(),
            sync_selection_stickiness_secs: 30,
            ..InnerState::default()
        };
        let peer = "peer-direct-session";
        state.peer_book.insert(
            peer.into(),
            PeerHealth {
                connected: true,
                chain_id_compatible: true,
                ..PeerHealth::default()
            },
        );
        state.active_connections.insert(peer.into(), 1);

        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);
        assert_eq!(state.connected_peers, vec![peer.to_string()]);
        assert_eq!(
            update_selected_sync_peer(&mut state, &ranked, now_unix()).as_deref(),
            Some(peer)
        );

        state.active_connections.clear();
        refresh_connected_peers_from_health(&mut state);
        let ranked = sync_candidates_snapshot(&state);
        assert!(state.connected_peers.is_empty());
        assert_eq!(update_selected_sync_peer(&mut state, &ranked, now_unix()), None);
    }

""" + regression_anchor
    text = replace_exact(
        text,
        regression_anchor,
        regression,
        "insert direct transport regressions",
    )

    TARGET.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
