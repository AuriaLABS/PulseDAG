from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text()


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    text = text.replace(old, new, 1)


replace_once(
    """    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},""",
    """    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},""",
    "atomic ownership import",
)

replace_once(
    """    let selected_segment_locator_state = Arc::new(Mutex::new(SelectedSegmentLocatorState {
        next_request_id: 1,
        pending_locator: None,
    }));

    if let Some(mut rx) = inbound_rx {""",
    """    let selected_segment_locator_state = Arc::new(Mutex::new(SelectedSegmentLocatorState {
        next_request_id: 1,
        pending_locator: None,
    }));
    let task27_recovery_active = Arc::new(AtomicBool::new(false));

    if let Some(mut rx) = inbound_rx {""",
    "shared Task 27 ownership state",
)

replace_once(
    """        let p2p = app_state.p2p.clone();
        let selected_segment_locator_state = selected_segment_locator_state.clone();
        let max_orphan_count = cfg.max_orphan_count;""",
    """        let p2p = app_state.p2p.clone();
        let selected_segment_locator_state = selected_segment_locator_state.clone();
        let task27_recovery_active = task27_recovery_active.clone();
        let max_orphan_count = cfg.max_orphan_count;""",
    "clone Task 27 ownership into inbound loop",
)

replace_once(
    """                    RecoveryProgressDecisionV1::NoPositiveGap => {
                        pending_task27_locator = None;
                    }""",
    """                    RecoveryProgressDecisionV1::NoPositiveGap => {
                        pending_task27_locator = None;
                        task27_recovery_active.store(false, Ordering::Relaxed);
                    }""",
    "release Task 27 ownership on convergence",
)

replace_once(
    """                                                    pending_task27_locator =
                                                        Some(PendingTask27Locator {
                                                            peer_id: peer_id.clone(),
                                                            selected_tip: selected_tip.clone(),
                                                            sent_at_unix: now,
                                                        });
                                                    let mut rt = runtime.write().await;""",
    """                                                    pending_task27_locator =
                                                        Some(PendingTask27Locator {
                                                            peer_id: peer_id.clone(),
                                                            selected_tip: selected_tip.clone(),
                                                            sent_at_unix: now,
                                                        });
                                                    task27_recovery_active
                                                        .store(true, Ordering::Relaxed);
                                                    let mut rt = runtime.write().await;""",
    "acquire Task 27 ownership on locator send",
)

replace_once(
    """                    } if stagnant_cycles == TASK27_REJOIN_MAX_STAGNANT_CYCLES => {
                        let mut rt = runtime.write().await;
                        rt.sync_state = "degraded".to_string();""",
    """                    } if stagnant_cycles == TASK27_REJOIN_MAX_STAGNANT_CYCLES => {
                        task27_recovery_active.store(false, Ordering::Relaxed);
                        let mut rt = runtime.write().await;
                        rt.sync_state = "degraded".to_string();""",
    "release Task 27 ownership on bounded degradation",
)

replace_once(
    """                            let task27_authoritative_recovery_active =
                                pending_task27_locator.is_some()
                                    || pending_dag_frontier_peer.is_some()
                                    || frontier_fetch_scheduler.queue_depth() > 0;
                            if !priority_already_active && !task27_authoritative_recovery_active {""",
    """                            let task27_authoritative_recovery_active =
                                task27_recovery_active.load(Ordering::Relaxed);
                            if !priority_already_active && !task27_authoritative_recovery_active {""",
    "legacy immediate locator checks shared Task 27 ownership",
)

replace_once(
    """                            ) || pending_task27_locator.is_some()
                                || pending_dag_frontier_peer.is_some()
                                || frontier_fetch_scheduler.queue_depth() > 0
                        };""",
    """                            ) || task27_recovery_active.load(Ordering::Relaxed)
                        };""",
    "generic tip fetch checks shared Task 27 ownership",
)

replace_once(
    """                                                } else {
                                                    pending_dag_frontier_peer = Some(peer_id.clone());
                                                    let mut rt = runtime.write().await;""",
    """                                                } else {
                                                    pending_dag_frontier_peer = Some(peer_id.clone());
                                                    task27_recovery_active
                                                        .store(true, Ordering::Relaxed);
                                                    let mut rt = runtime.write().await;""",
    "frontier plan acquires Task 27 ownership",
)

replace_once(
    """        let p2p = app_state.p2p.clone();
        let selected_segment_locator_state = selected_segment_locator_state.clone();
        tokio::spawn(async move {""",
    """        let p2p = app_state.p2p.clone();
        let selected_segment_locator_state = selected_segment_locator_state.clone();
        let task27_recovery_active = task27_recovery_active.clone();
        tokio::spawn(async move {""",
    "clone Task 27 ownership into audit loop",
)

replace_once(
    """                    if !priority_already_active {
                        let selected_locator = {""",
    """                    if !priority_already_active
                        && !task27_recovery_active.load(Ordering::Relaxed)
                    {
                        let selected_locator = {""",
    "proactive legacy locator respects Task 27 ownership",
)

replace_once(
    """                    if cleanup_complete && selected_chain_gate.allows_selected_chain_sync() {
                        if let Some(ref p2p) = p2p {""",
    """                    if cleanup_complete
                        && selected_chain_gate.allows_selected_chain_sync()
                        && !task27_recovery_active.load(Ordering::Relaxed)
                    {
                        if let Some(ref p2p) = p2p {""",
    "final quiescence respects Task 27 ownership",
)

path.write_text(text)
