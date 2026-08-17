from pathlib import Path

main_path = Path("apps/pulsedagd/src/main.rs")
main = main_path.read_text()
p2p_path = Path("crates/pulsedag-p2p/src/lib.rs")
p2p = p2p_path.read_text()


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


main = replace_once(
    main,
    """    messages::{
        build_dag_frontier_response_v1, plan_dag_frontier_reconciliation_v1, HeaderInventory,
        ProtocolSyncWireV1, TipInventoryStatus, MAX_DAG_FRONTIER_ENTRIES,
        MAX_DAG_FRONTIER_REQUIRED_CONTEXT, MAX_SELECTED_CHAIN_SUFFIX_HASHES,
    },""",
    """    messages::{
        build_dag_frontier_response_v1, build_selected_chain_locator_v1,
        plan_dag_frontier_reconciliation_v1, HeaderInventory, ProtocolSyncWireV1,
        RecoveryProgressDecisionV1, RecoveryProgressObservationV1, RecoveryProgressTrackerV1,
        TipInventoryStatus, MAX_DAG_FRONTIER_ENTRIES, MAX_DAG_FRONTIER_REQUIRED_CONTEXT,
        MAX_SELECTED_CHAIN_SUFFIX_HASHES,
    },""",
    "Task 27 rejoin imports",
)

main = replace_once(
    main,
    """fn selected_headers_own_broadcast_locator(
    session_active: bool,""",
    """fn task27_rejoin_peer_for_gap(
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

fn selected_headers_own_broadcast_locator(
    session_active: bool,""",
    "Task 27 rejoin peer selection helper",
)

main = replace_once(
    main,
    """const SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS: u64 = 64;
const SELECTED_LOCATOR_PRIORITY_GRACE_SECS: u64 = 60;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FinalQuiescenceCleanupResult {""",
    """const SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS: u64 = 64;
const SELECTED_LOCATOR_PRIORITY_GRACE_SECS: u64 = 60;
const TASK27_REJOIN_MAX_STAGNANT_CYCLES: u32 = 30;
const TASK27_LOCATOR_RESPONSE_TIMEOUT_SECS: u64 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingTask27Locator {
    peer_id: String,
    selected_tip: String,
    sent_at_unix: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FinalQuiescenceCleanupResult {""",
    "Task 27 rejoin constants and state",
)

main = replace_once(
    main,
    """            let mut frontier_fetch_scheduler =
                DependencyAwareFetchScheduler::with_limit(MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH);
            let mut pending_dag_frontier_peer: Option<String> = None;
            let mut final_quiescence_higher_tip_requests: HashSet<String> = HashSet::new();""",
    """            let mut frontier_fetch_scheduler =
                DependencyAwareFetchScheduler::with_limit(MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH);
            let mut pending_dag_frontier_peer: Option<String> = None;
            let mut pending_task27_locator: Option<PendingTask27Locator> = None;
            let mut task27_recovery_tracker =
                RecoveryProgressTrackerV1::new(TASK27_REJOIN_MAX_STAGNANT_CYCLES);
            let mut task27_last_eligible_v2_peers = Vec::<String>::new();
            let mut final_quiescence_higher_tip_requests: HashSet<String> = HashSet::new();""",
    "Task 27 recovery loop state",
)

old_pump = """                let selected_segment_priority = {
                    let guard = selected_segment_locator_state.lock().await;
                    selected_segment_recovery_has_priority(
                        selected_segment_session.is_some(),
                        guard
                            .pending_locator
                            .as_ref()
                            .map(|pending| pending.requested_at_unix),
                        now_unix(),
                    )
                };
                if !selected_segment_priority {"""
new_pump = """                let selected_segment_priority = {
                    let guard = selected_segment_locator_state.lock().await;
                    selected_segment_recovery_has_priority(
                        selected_segment_session.is_some(),
                        guard
                            .pending_locator
                            .as_ref()
                            .map(|pending| pending.requested_at_unix),
                        now,
                    )
                };
                let eligible_v2_peers = p2p
                    .as_ref()
                    .and_then(|handle| handle.protocol_sync_eligible_peers_v1().ok())
                    .unwrap_or_default();
                if eligible_v2_peers != task27_last_eligible_v2_peers {
                    task27_recovery_tracker =
                        RecoveryProgressTrackerV1::new(TASK27_REJOIN_MAX_STAGNANT_CYCLES);
                    task27_last_eligible_v2_peers = eligible_v2_peers.clone();
                    if pending_task27_locator.as_ref().is_some_and(|pending| {
                        !eligible_v2_peers.iter().any(|peer| peer == &pending.peer_id)
                    }) {
                        pending_task27_locator = None;
                    }
                }
                if let Some(pending) = pending_task27_locator.as_ref() {
                    if now.saturating_sub(pending.sent_at_unix)
                        >= TASK27_LOCATOR_RESPONSE_TIMEOUT_SECS
                    {
                        warn!(
                            peer = %pending.peer_id,
                            selected_tip = %pending.selected_tip,
                            "Task 27 locator response timed out; recovery may replan"
                        );
                        pending_task27_locator = None;
                    }
                }
                let p2p_status = p2p.as_ref().and_then(|handle| handle.status().ok());
                let (local_selected_height, pending_missing_parents, orphan_count) = {
                    let guard = chain.read().await;
                    let inventory = local_tip_inventory_status(&guard);
                    (
                        inventory.selected_height.unwrap_or(guard.dag.best_height),
                        pulsedag_core::pending_missing_parent_count(&guard),
                        guard.orphan_blocks.len(),
                    )
                };
                let task27_rejoin_candidate = p2p_status.as_ref().and_then(|status| {
                    task27_rejoin_peer_for_gap(
                        status,
                        &eligible_v2_peers,
                        local_selected_height,
                    )
                });
                let network_selected_height =
                    task27_rejoin_candidate.as_ref().map(|(_, height)| *height);
                let (missing_parent_responses, orphan_reprocess_attempts, orphan_reprocess_successes) = {
                    let rt = runtime.read().await;
                    (
                        rt.blockdata_not_found,
                        rt.orphan_reprocess_attempts,
                        rt.orphan_reprocess_success,
                    )
                };
                let task27_pending_work = block_requests
                    .pending
                    .len()
                    .saturating_add(fetch_scheduler.queue_depth())
                    .saturating_add(frontier_fetch_scheduler.queue_depth())
                    .saturating_add(usize::from(pending_dag_frontier_peer.is_some()))
                    .saturating_add(usize::from(pending_task27_locator.is_some()))
                    .saturating_add(usize::from(selected_segment_session.is_some()));
                let task27_recovery_decision = task27_recovery_tracker.observe(
                    RecoveryProgressObservationV1 {
                        local_selected_height,
                        network_selected_height,
                        compatible_peer_available: !eligible_v2_peers.is_empty(),
                        pending_requests: task27_pending_work,
                        inflight_requests: block_requests.pending.len(),
                        pending_missing_parents,
                        orphan_count,
                        missing_parent_responses,
                        orphan_reprocess_attempts,
                        orphan_reprocess_successes,
                    },
                );
                match task27_recovery_decision {
                    RecoveryProgressDecisionV1::NoPositiveGap => {
                        pending_task27_locator = None;
                    }
                    RecoveryProgressDecisionV1::ScheduleRecovery {
                        gap,
                        stagnant_cycles,
                    } if !selected_segment_priority
                        && pending_task27_locator.is_none()
                        && pending_dag_frontier_peer.is_none()
                        && frontier_fetch_scheduler.queue_depth() == 0 =>
                    {
                        if let (Some((peer_id, remote_height)), Some(p2p_handle)) =
                            (task27_rejoin_candidate.clone(), p2p.as_ref())
                        {
                            match p2p_handle.local_protocol_capabilities_v1() {
                                Ok(Some(local_capabilities)) => {
                                    let selected_chain = {
                                        let guard = chain.read().await;
                                        guard.dag.selected_chain.clone()
                                    };
                                    match build_selected_chain_locator_v1(
                                        local_capabilities.protocol_identity,
                                        &selected_chain,
                                    ) {
                                        Ok(locator) => {
                                            let selected_tip = locator.selected_tip.clone();
                                            match p2p_handle.send_protocol_sync_v1(
                                                &peer_id,
                                                &ProtocolSyncWireV1::SelectedChainLocator(locator),
                                            ) {
                                                Ok(()) => {
                                                    pending_task27_locator =
                                                        Some(PendingTask27Locator {
                                                            peer_id: peer_id.clone(),
                                                            selected_tip: selected_tip.clone(),
                                                            sent_at_unix: now,
                                                        });
                                                    let mut rt = runtime.write().await;
                                                    rt.selected_segment_gap_blocks = rt
                                                        .selected_segment_gap_blocks
                                                        .max(remote_height.saturating_sub(
                                                            local_selected_height,
                                                        ));
                                                    rt.dag_sync_selected_chain_locator_total = rt
                                                        .dag_sync_selected_chain_locator_total
                                                        .saturating_add(1);
                                                    rt.sync_state = DagSyncStage::SelectedChainLocator
                                                        .as_str()
                                                        .to_string();
                                                    info!(
                                                        peer = %peer_id,
                                                        selected_tip = %selected_tip,
                                                        local_selected_height,
                                                        remote_height,
                                                        gap,
                                                        stagnant_cycles,
                                                        "started Task 27 bounded rejoin locator recovery"
                                                    );
                                                }
                                                Err(error) => {
                                                    warn!(
                                                        peer = %peer_id,
                                                        error = %error,
                                                        "failed sending Task 27 rejoin locator"
                                                    );
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            warn!(
                                                peer = %peer_id,
                                                error = ?error,
                                                "failed building Task 27 rejoin locator"
                                            );
                                        }
                                    }
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    warn!(
                                        peer = %peer_id,
                                        error = %error,
                                        "failed reading local capabilities for Task 27 rejoin"
                                    );
                                }
                            }
                        }
                    }
                    RecoveryProgressDecisionV1::Degraded {
                        gap,
                        stagnant_cycles,
                        reason,
                    } if stagnant_cycles == TASK27_REJOIN_MAX_STAGNANT_CYCLES => {
                        let mut rt = runtime.write().await;
                        rt.sync_state = "degraded".to_string();
                        rt.sync_failures = rt.sync_failures.saturating_add(1);
                        warn!(
                            gap,
                            stagnant_cycles,
                            reason = ?reason,
                            "Task 27 bounded rejoin recovery became non-productive"
                        );
                    }
                    RecoveryProgressDecisionV1::AwaitCompatiblePeer { .. }
                    | RecoveryProgressDecisionV1::Productive { .. }
                    | RecoveryProgressDecisionV1::ContinueRecovery { .. }
                    | RecoveryProgressDecisionV1::ScheduleRecovery { .. }
                    | RecoveryProgressDecisionV1::Degraded { .. } => {}
                }
                if !selected_segment_priority {"""
main = replace_once(main, old_pump, new_pump, "Task 27 recovery pump")

main = replace_once(
    main,
    """                            if !priority_already_active {
                                let selected_locator = {""",
    """                            let task27_authoritative_recovery_active =
                                pending_task27_locator.is_some()
                                    || pending_dag_frontier_peer.is_some()
                                    || frontier_fetch_scheduler.queue_depth() > 0;
                            if !priority_already_active && !task27_authoritative_recovery_active {
                                let selected_locator = {""",
    "suppress legacy locator while Task 27 owns rejoin",
)

main = replace_once(
    main,
    """                        let selected_segment_priority = {
                            let guard = selected_segment_locator_state.lock().await;
                            selected_segment_recovery_has_priority(
                                selected_segment_session.is_some(),
                                guard
                                    .pending_locator
                                    .as_ref()
                                    .map(|pending| pending.requested_at_unix),
                                now_unix(),
                            )
                        };
                        for tip in unknown_tips {""",
    """                        let selected_segment_priority = {
                            let guard = selected_segment_locator_state.lock().await;
                            selected_segment_recovery_has_priority(
                                selected_segment_session.is_some(),
                                guard
                                    .pending_locator
                                    .as_ref()
                                    .map(|pending| pending.requested_at_unix),
                                now_unix(),
                            ) || pending_task27_locator.is_some()
                                || pending_dag_frontier_peer.is_some()
                                || frontier_fetch_scheduler.queue_depth() > 0
                        };
                        for tip in unknown_tips {""",
    "suppress generic tip fetch while Task 27 owns rejoin",
)

main = replace_once(
    main,
    """                        ProtocolSyncWireV1::DagFrontier(frontier) => {
                            if let Some(ref p2p_handle) = p2p {""",
    """                        ProtocolSyncWireV1::DagFrontier(frontier) => {
                            if pending_task27_locator
                                .as_ref()
                                .is_some_and(|pending| pending.peer_id == peer_id)
                            {
                                pending_task27_locator = None;
                            }
                            if let Some(ref p2p_handle) = p2p {""",
    "clear pending Task 27 locator on frontier response",
)

main = replace_once(
    main,
    """#[cfg(test)]
mod protocol_restore_startup_tests {""",
    """#[cfg(test)]
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
        let mut status = P2pStatus::default();
        status.remote_selected_tip_inventory = vec![
            remote("legacy-high", 500),
            remote("v2-b", 220),
            remote("v2-a", 220),
            remote("v2-low", 210),
        ];
        let eligible = vec![
            "v2-a".to_string(),
            "v2-b".to_string(),
            "v2-low".to_string(),
        ];

        assert_eq!(
            task27_rejoin_peer_for_gap(&status, &eligible, 200),
            Some(("v2-a".to_string(), 220))
        );
        assert_eq!(task27_rejoin_peer_for_gap(&status, &eligible, 220), None);
    }
}

#[cfg(test)]
mod protocol_restore_startup_tests {""",
    "Task 27 runtime peer-selection regression",
)

p2p = replace_once(
    p2p,
    """    fn local_protocol_capabilities_v1(&self) -> Result<Option<ProtocolCapabilitiesV1>, PulseError> {
        Ok(None)
    }
    fn send_protocol_sync_v1(""",
    """    fn local_protocol_capabilities_v1(&self) -> Result<Option<ProtocolCapabilitiesV1>, PulseError> {
        Ok(None)
    }
    fn protocol_sync_eligible_peers_v1(&self) -> Result<Vec<String>, PulseError> {
        Ok(Vec::new())
    }
    fn send_protocol_sync_v1(""",
    "P2pHandle eligible peer API",
)

memory_marker = """    fn local_protocol_capabilities_v1(&self) -> Result<Option<ProtocolCapabilitiesV1>, PulseError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        Ok(inner
            .protocol_capability_transport
            .local_capabilities()
            .cloned())
    }

    fn update_tip_inventory(&self, inventory: TipInventoryStatus) -> Result<(), PulseError> {"""
memory_replacement = """    fn local_protocol_capabilities_v1(&self) -> Result<Option<ProtocolCapabilitiesV1>, PulseError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        Ok(inner
            .protocol_capability_transport
            .local_capabilities()
            .cloned())
    }

    fn protocol_sync_eligible_peers_v1(&self) -> Result<Vec<String>, PulseError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| PulseError::Internal("p2p lock poisoned".into()))?;
        Ok(inner.protocol_capability_transport.eligible_v2_peers())
    }

    fn update_tip_inventory(&self, inventory: TipInventoryStatus) -> Result<(), PulseError> {"""
if p2p.count(memory_marker) != 2:
    raise SystemExit(f"eligible peer implementation: expected two handle matches, found {p2p.count(memory_marker)}")
p2p = p2p.replace(memory_marker, memory_replacement, 2)

p2p = replace_once(
    p2p,
    """        assert_eq!(stack.handle.local_protocol_capabilities_v1().unwrap(), None);

        let state = pulsedag_core::genesis::init_chain_state(chain_id.clone());""",
    """        assert_eq!(stack.handle.local_protocol_capabilities_v1().unwrap(), None);
        assert!(stack
            .handle
            .protocol_sync_eligible_peers_v1()
            .unwrap()
            .is_empty());

        let state = pulsedag_core::genesis::init_chain_state(chain_id.clone());""",
    "unconfigured eligible peer regression",
)

main_path.write_text(main)
p2p_path.write_text(p2p)
