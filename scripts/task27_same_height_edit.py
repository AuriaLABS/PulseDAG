from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one exact match, found {count}\n--- needle ---\n{old}")
    p.write_text(text.replace(old, new, 1))


# Extend the existing bounded recovery tracker instead of creating a second runtime state machine.
path = "crates/pulsedag-p2p/src/messages/recovery_progress_v1.rs"
replace_once(
    path,
    "    pub network_selected_height: Option<u64>,\n    pub compatible_peer_available: bool,",
    "    pub network_selected_height: Option<u64>,\n    pub same_height_divergence: bool,\n    pub compatible_peer_available: bool,",
)
replace_once(
    path,
    "    PositiveGapIdle,\n    MissingParentCycleNoProgress,",
    "    PositiveGapIdle,\n    SameHeightDivergenceIdle,\n    MissingParentCycleNoProgress,",
)
replace_once(
    path,
    "        if gap == 0 {\n            self.stagnant_cycles = 0;",
    "        if gap == 0 && !current.same_height_divergence {\n            self.stagnant_cycles = 0;",
)
replace_once(
    path,
    "        let reason = if current.pending_missing_parents > 0 || current.orphan_count > 0 {\n            RecoveryProgressReasonV1::MissingParentCycleNoProgress\n        } else {\n            RecoveryProgressReasonV1::PositiveGapIdle\n        };",
    "        let reason = if current.pending_missing_parents > 0 || current.orphan_count > 0 {\n            RecoveryProgressReasonV1::MissingParentCycleNoProgress\n        } else if gap == 0 && current.same_height_divergence {\n            RecoveryProgressReasonV1::SameHeightDivergenceIdle\n        } else {\n            RecoveryProgressReasonV1::PositiveGapIdle\n        };",
)

# Regression coverage for bounded same-height recovery.
path = "crates/pulsedag-p2p/tests/task27_recovery_progress.rs"
replace_once(
    path,
    "        network_selected_height: Some(105),\n        compatible_peer_available: true,",
    "        network_selected_height: Some(105),\n        same_height_divergence: false,\n        compatible_peer_available: true,",
)
replace_once(
    path,
    "#[test]\nfn zero_gap_resets_without_claiming_finality() {",
    "#[test]\nfn same_height_divergence_is_bounded_recovery_not_a_fake_height_gap() {\n    let mut tracker = RecoveryProgressTrackerV1::new(2);\n    let mut current = observation();\n    current.network_selected_height = Some(current.local_selected_height);\n    current.same_height_divergence = true;\n\n    assert_eq!(\n        tracker.observe(current),\n        RecoveryProgressDecisionV1::ScheduleRecovery {\n            gap: 0,\n            stagnant_cycles: 1,\n        }\n    );\n    assert_eq!(\n        tracker.observe(current),\n        RecoveryProgressDecisionV1::Degraded {\n            gap: 0,\n            stagnant_cycles: 2,\n            reason: RecoveryProgressReasonV1::SameHeightDivergenceIdle,\n        }\n    );\n\n    current.same_height_divergence = false;\n    assert_eq!(\n        tracker.observe(current),\n        RecoveryProgressDecisionV1::NoPositiveGap\n    );\n    assert_eq!(tracker.stagnant_cycles(), 0);\n}\n\n#[test]\nfn zero_gap_resets_without_claiming_finality() {",
)

# Reuse the already-shipped tip/order/state divergence signal for exact-compatible v2 peers.
path = "apps/pulsedagd/src/main.rs"
old_tests = '''#[cfg(test)]
mod task27_rejoin_runtime_tests {
    use super::*;
    use pulsedag_p2p::RemoteSelectedTipStatus;

    fn remote(peer_id: &str, selected_height: u64) -> RemoteSelectedTipStatus {
        RemoteSelectedTipStatus {
            peer_id: peer_id.to_string(),
            selected_height,
            connected: true,
            direct_request_capable: true,
            ..RemoteSelectedTipStatus::default()
        }
    }

    #[test]
    fn task27_rejoin_uses_only_exact_eligible_peer_with_highest_gap() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("legacy-high", 500),
                remote("v2-b", 220),
                remote("v2-a", 220),
                remote("v2-low", 210),
            ],
            ..P2pStatus::default()
        };
        let eligible = vec!["v2-a".to_string(), "v2-b".to_string(), "v2-low".to_string()];

        assert_eq!(
            task27_rejoin_peer_for_gap(&status, &eligible, 200),
            Some(("v2-a".to_string(), 220))
        );
        assert_eq!(task27_rejoin_peer_for_gap(&status, &eligible, 220), None);
    }
}
'''
new_tests = '''#[cfg(test)]
mod task27_rejoin_runtime_tests {
    use super::*;
    use pulsedag_p2p::RemoteSelectedTipStatus;

    fn remote(peer_id: &str, selected_height: u64) -> RemoteSelectedTipStatus {
        RemoteSelectedTipStatus {
            peer_id: peer_id.to_string(),
            selected_height,
            connected: true,
            direct_request_capable: true,
            ..RemoteSelectedTipStatus::default()
        }
    }

    fn local_inventory(selected_height: u64) -> TipInventoryStatus {
        TipInventoryStatus {
            selected_height: Some(selected_height),
            ..TipInventoryStatus::default()
        }
    }

    fn matching_remote(peer_id: &str, local: &TipInventoryStatus) -> RemoteSelectedTipStatus {
        let mut remote = remote(peer_id, local.selected_height.unwrap_or_default());
        remote.selected_tip = local.selected_tip.clone();
        remote.ordered_dag_tip = local.ordered_dag_tip.clone();
        remote.state_root_digest = local.state_root_digest.clone();
        remote
    }

    #[test]
    fn task27_rejoin_uses_only_exact_eligible_peer_with_highest_gap() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                remote("legacy-high", 500),
                remote("v2-b", 220),
                remote("v2-a", 220),
                remote("v2-low", 210),
            ],
            ..P2pStatus::default()
        };
        let eligible = vec!["v2-a".to_string(), "v2-b".to_string(), "v2-low".to_string()];

        assert_eq!(
            task27_rejoin_peer_for_reconcile(&status, &eligible, &local_inventory(200)),
            Some(("v2-a".to_string(), 220, false))
        );
        assert_eq!(
            task27_rejoin_peer_for_reconcile(&status, &eligible, &local_inventory(220)),
            None
        );
    }

    #[test]
    fn task27_rejoin_detects_same_height_tip_order_or_state_divergence() {
        let local = TipInventoryStatus {
            selected_tip: Some("local-tip".to_string()),
            selected_height: Some(220),
            ordered_dag_tip: Some("local-order".to_string()),
            state_root_digest: Some("local-root".to_string()),
            ..TipInventoryStatus::default()
        };
        let mut v2_b = matching_remote("v2-b", &local);
        v2_b.state_root_digest = Some("remote-root".to_string());
        let mut v2_a = matching_remote("v2-a", &local);
        v2_a.selected_tip = Some("remote-tip".to_string());
        let mut legacy = matching_remote("legacy", &local);
        legacy.ordered_dag_tip = Some("legacy-order".to_string());
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![
                legacy,
                matching_remote("v2-match", &local),
                v2_b,
                v2_a,
            ],
            ..P2pStatus::default()
        };
        let eligible = vec![
            "v2-a".to_string(),
            "v2-b".to_string(),
            "v2-match".to_string(),
        ];

        assert_eq!(
            task27_rejoin_peer_for_reconcile(&status, &eligible, &local),
            Some(("v2-a".to_string(), 220, true))
        );

        let matching_status = P2pStatus {
            remote_selected_tip_inventory: vec![
                matching_remote("v2-a", &local),
                matching_remote("v2-b", &local),
            ],
            ..P2pStatus::default()
        };
        assert_eq!(
            task27_rejoin_peer_for_reconcile(&matching_status, &eligible, &local),
            None
        );
    }
}
'''
replace_once(path, old_tests, new_tests)

old_helpers = '''fn selected_locator_peer_for_reconcile(
    status: &P2pStatus,
    local: &TipInventoryStatus,
) -> Option<String> {
    let local_height = local.selected_height.unwrap_or_default();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| {
            remote.selected_height > local_height
                || (remote.selected_height == local_height
                    && (remote.selected_tip != local.selected_tip
                        || remote.ordered_dag_tip != local.ordered_dag_tip
                        || remote.state_root_digest != local.state_root_digest))
        })
        .max_by_key(|remote| remote.selected_height)
        .map(|remote| remote.peer_id.clone())
}

fn task27_rejoin_peer_for_gap(
    status: &P2pStatus,
    eligible_v2_peers: &[String],
    local_height: u64,
) -> Option<(String, u64)> {
    let eligible = eligible_v2_peers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| eligible.contains(remote.peer_id.as_str()))
        .filter(|remote| remote.selected_height > local_height)
        .map(|remote| (remote.peer_id.clone(), remote.selected_height))
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
}
'''
new_helpers = '''fn remote_same_height_divergence(
    remote: &pulsedag_p2p::RemoteSelectedTipStatus,
    local: &TipInventoryStatus,
) -> bool {
    remote.selected_height == local.selected_height.unwrap_or_default()
        && (remote.selected_tip != local.selected_tip
            || remote.ordered_dag_tip != local.ordered_dag_tip
            || remote.state_root_digest != local.state_root_digest)
}

fn selected_locator_peer_for_reconcile(
    status: &P2pStatus,
    local: &TipInventoryStatus,
) -> Option<String> {
    let local_height = local.selected_height.unwrap_or_default();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| {
            remote.selected_height > local_height || remote_same_height_divergence(remote, local)
        })
        .max_by_key(|remote| remote.selected_height)
        .map(|remote| remote.peer_id.clone())
}

fn task27_rejoin_peer_for_reconcile(
    status: &P2pStatus,
    eligible_v2_peers: &[String],
    local: &TipInventoryStatus,
) -> Option<(String, u64, bool)> {
    let local_height = local.selected_height.unwrap_or_default();
    let eligible = eligible_v2_peers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| eligible.contains(remote.peer_id.as_str()))
        .filter_map(|remote| {
            let same_height_divergence = remote_same_height_divergence(remote, local);
            (remote.selected_height > local_height || same_height_divergence).then(|| {
                (
                    remote.peer_id.clone(),
                    remote.selected_height,
                    same_height_divergence,
                )
            })
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
}
'''
replace_once(path, old_helpers, new_helpers)

old_runtime = '''                let (local_selected_height, pending_missing_parents, orphan_count) = {
                    let guard = chain.read().await;
                    let inventory = local_tip_inventory_status(&guard);
                    (
                        inventory.selected_height.unwrap_or(guard.dag.best_height),
                        pulsedag_core::pending_missing_parent_count(&guard),
                        guard.orphan_blocks.len(),
                    )
                };
                let task27_rejoin_candidate = p2p_status.as_ref().and_then(|status| {
                    task27_rejoin_peer_for_gap(status, &eligible_v2_peers, local_selected_height)
                });
                let network_selected_height =
                    task27_rejoin_candidate.as_ref().map(|(_, height)| *height);
'''
new_runtime = '''                let (
                    local_inventory,
                    local_selected_height,
                    pending_missing_parents,
                    orphan_count,
                ) = {
                    let guard = chain.read().await;
                    let inventory = local_tip_inventory_status(&guard);
                    let local_selected_height =
                        inventory.selected_height.unwrap_or(guard.dag.best_height);
                    (
                        inventory,
                        local_selected_height,
                        pulsedag_core::pending_missing_parent_count(&guard),
                        guard.orphan_blocks.len(),
                    )
                };
                let task27_rejoin_candidate = p2p_status.as_ref().and_then(|status| {
                    task27_rejoin_peer_for_reconcile(status, &eligible_v2_peers, &local_inventory)
                });
                let network_selected_height = task27_rejoin_candidate
                    .as_ref()
                    .map(|(_, height, _)| *height);
                let same_height_divergence = task27_rejoin_candidate
                    .as_ref()
                    .is_some_and(|(_, _, divergence)| *divergence);
'''
replace_once(path, old_runtime, new_runtime)

replace_once(
    path,
    "                        network_selected_height,\n                        compatible_peer_available: !eligible_v2_peers.is_empty(),",
    "                        network_selected_height,\n                        same_height_divergence,\n                        compatible_peer_available: !eligible_v2_peers.is_empty(),",
)
replace_once(
    path,
    "                        if let (Some((peer_id, remote_height)), Some(p2p_handle)) =\n                            (task27_rejoin_candidate.clone(), p2p.as_ref())",
    "                        if let (\n                            Some((peer_id, remote_height, same_height_divergence)),\n                            Some(p2p_handle),\n                        ) = (task27_rejoin_candidate.clone(), p2p.as_ref())",
)
replace_once(
    path,
    "                                                            gap,\n                                                            stagnant_cycles,\n                                                            \"started Task 27 bounded rejoin locator recovery\"",
    "                                                            gap,\n                                                            stagnant_cycles,\n                                                            same_height_divergence,\n                                                            \"started Task 27 bounded reconcile locator recovery\"",
)
