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
    "    collections::{BTreeMap, HashSet},",
    "    collections::{BTreeMap, BTreeSet, HashSet},",
    "collections import",
)

replace_once(
    """    messages::{
        build_dag_frontier_response_v1, HeaderInventory, ProtocolSyncWireV1, TipInventoryStatus,
    },""",
    """    messages::{
        build_dag_frontier_response_v1, plan_dag_frontier_reconciliation_v1, HeaderInventory,
        ProtocolSyncWireV1, TipInventoryStatus, MAX_DAG_FRONTIER_ENTRIES,
        MAX_DAG_FRONTIER_REQUIRED_CONTEXT, MAX_SELECTED_CHAIN_SUFFIX_HASHES,
    },""",
    "Task 27 message imports",
)

replace_once(
    """const MAX_FETCH_SCHEDULER_QUEUE_DEPTH: usize = 512;

const ORPHAN_RECOVERY_ROOT_REQUEST_LIMIT: usize = 16;""",
    """const MAX_FETCH_SCHEDULER_QUEUE_DEPTH: usize = 512;
const DAG_FRONTIER_FETCH_BATCH: usize = 8;
const MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH: usize = MAX_DAG_FRONTIER_REQUIRED_CONTEXT
    + MAX_SELECTED_CHAIN_SUFFIX_HASHES
    + MAX_DAG_FRONTIER_ENTRIES;

const ORPHAN_RECOVERY_ROOT_REQUEST_LIMIT: usize = 16;""",
    "frontier queue constants",
)

replace_once(
    """            let mut fetch_scheduler =
                DependencyAwareFetchScheduler::with_limit(MAX_FETCH_SCHEDULER_QUEUE_DEPTH);
            let mut final_quiescence_higher_tip_requests: HashSet<String> = HashSet::new();""",
    """            let mut fetch_scheduler =
                DependencyAwareFetchScheduler::with_limit(MAX_FETCH_SCHEDULER_QUEUE_DEPTH);
            let mut frontier_fetch_scheduler = DependencyAwareFetchScheduler::with_limit(
                MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH,
            );
            let mut pending_dag_frontier_peer: Option<String> = None;
            let mut final_quiescence_higher_tip_requests: HashSet<String> = HashSet::new();""",
    "frontier scheduler state",
)

replace_once(
    "                let Some(event) = maybe_event else {",
    """                let selected_segment_priority = {
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
                if !selected_segment_priority {
                    if let Some(frontier_peer) = pending_dag_frontier_peer.clone() {
                        let (known, pending) = {
                            let guard = chain.read().await;
                            (
                                known_hashes_for_scheduler(&guard),
                                pending_hashes_for_scheduler(&block_requests),
                            )
                        };
                        let request_capacity = block_requests
                            .pending_capacity_remaining()
                            .min(DAG_FRONTIER_FETCH_BATCH);
                        if request_capacity > 0 {
                            let plan = frontier_fetch_scheduler.next_requests(
                                &known,
                                &pending,
                                request_capacity,
                            );
                            let parent_first_requests = plan.parent_first_requests;
                            let mut requeue_hashes = Vec::new();
                            let mut issued_requests = 0u64;
                            for hash in plan.requests {
                                if !block_requests.should_issue_getblock_for_peers(
                                    &hash,
                                    now_unix(),
                                    [frontier_peer.clone()],
                                ) {
                                    requeue_hashes.push(hash);
                                    continue;
                                }
                                let request_sent = if let Some(ref p2p_handle) = p2p {
                                    match p2p_handle
                                        .request_block_from(&frontier_peer, &hash)
                                        .map(|_| ())
                                    {
                                        Ok(()) => true,
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                peer = %frontier_peer,
                                                block_hash = %hash,
                                                "failed issuing Task 27 frontier GetBlock request"
                                            );
                                            false
                                        }
                                    }
                                } else {
                                    false
                                };
                                if request_sent {
                                    issued_requests = issued_requests.saturating_add(1);
                                } else {
                                    block_requests.resolve(&hash);
                                    requeue_hashes.push(hash);
                                }
                            }
                            if !requeue_hashes.is_empty() {
                                let requeue_count = requeue_hashes.len();
                                let requeued =
                                    frontier_fetch_scheduler.queue_inventory(requeue_hashes);
                                if requeued != requeue_count {
                                    warn!(
                                        peer = %frontier_peer,
                                        expected = requeue_count,
                                        requeued,
                                        "Task 27 frontier retry queue could not retain every hash"
                                    );
                                }
                            }
                            if issued_requests > 0 {
                                let mut rt = runtime.write().await;
                                rt.sync_state = "requesting_blocks".to_string();
                                rt.getblock_sent =
                                    rt.getblock_sent.saturating_add(issued_requests);
                                rt.peer_addressed_getblock_sent_total = rt
                                    .peer_addressed_getblock_sent_total
                                    .saturating_add(issued_requests);
                                rt.dependency_fetches_scheduled = rt
                                    .dependency_fetches_scheduled
                                    .saturating_add(issued_requests);
                                rt.parent_first_fetches = rt
                                    .parent_first_fetches
                                    .saturating_add(parent_first_requests as u64);
                                rt.pending_block_requests = block_requests.pending.len();
                                rt.inflight_block_requests = block_requests.pending.len();
                                rt.pending_block_request_hashes = block_requests.pending_hashes();
                                rt.block_fetch_scheduler_queue_depth = fetch_scheduler
                                    .queue_depth()
                                    .saturating_add(frontier_fetch_scheduler.queue_depth());
                                rt.block_fetch_scheduler_inflight_by_peer =
                                    block_requests.inflight_by_peer();
                                info!(
                                    peer = %frontier_peer,
                                    issued_requests,
                                    remaining_frontier_hashes = frontier_fetch_scheduler.queue_depth(),
                                    "issued bounded Task 27 DAG frontier fetch batch"
                                );
                            }
                        }
                        if frontier_fetch_scheduler.queue_depth() == 0 {
                            pending_dag_frontier_peer = None;
                        }
                    }
                }

                let Some(event) = maybe_event else {""",
    "frontier scheduler pump",
)

replace_once(
    """                        ProtocolSyncWireV1::DagFrontier(frontier) => {
                            // Frontier reconciliation/fetch scheduling is the next Task 27 slice.
                            let _ = frontier;
                        }""",
    """                        ProtocolSyncWireV1::DagFrontier(frontier) => {
                            if let Some(ref p2p_handle) = p2p {
                                match p2p_handle.local_protocol_capabilities_v1() {
                                    Ok(Some(local_capabilities)) => {
                                        let known_hashes = {
                                            let guard = chain.read().await;
                                            guard
                                                .dag
                                                .blocks
                                                .keys()
                                                .cloned()
                                                .collect::<BTreeSet<_>>()
                                        };
                                        match plan_dag_frontier_reconciliation_v1(
                                            &local_capabilities.protocol_identity,
                                            &frontier,
                                            &known_hashes,
                                        ) {
                                            Ok(plan) if plan.is_complete() => {
                                                if pending_dag_frontier_peer.as_deref()
                                                    == Some(peer_id.as_str())
                                                {
                                                    frontier_fetch_scheduler =
                                                        DependencyAwareFetchScheduler::with_limit(
                                                            MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH,
                                                        );
                                                    pending_dag_frontier_peer = None;
                                                }
                                                info!(
                                                    peer = %peer_id,
                                                    selected_tip = %plan.selected_tip,
                                                    "received complete Task 27 DAG frontier; no block fetches required"
                                                );
                                            }
                                            Ok(plan) => {
                                                let selected_tip = plan.selected_tip.clone();
                                                let missing_required_context =
                                                    plan.missing_required_context.len();
                                                let missing_selected_chain =
                                                    plan.missing_selected_chain.len();
                                                let missing_frontier = plan.missing_frontier.len();
                                                let request_count = plan.request_hashes.len();
                                                frontier_fetch_scheduler =
                                                    DependencyAwareFetchScheduler::with_limit(
                                                        MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH,
                                                    );
                                                let staged = frontier_fetch_scheduler
                                                    .queue_inventory(plan.request_hashes);
                                                if staged != request_count {
                                                    warn!(
                                                        peer = %peer_id,
                                                        selected_tip = %selected_tip,
                                                        expected = request_count,
                                                        staged,
                                                        "failed to retain the complete Task 27 frontier reconciliation plan"
                                                    );
                                                    frontier_fetch_scheduler =
                                                        DependencyAwareFetchScheduler::with_limit(
                                                            MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH,
                                                        );
                                                    pending_dag_frontier_peer = None;
                                                } else {
                                                    pending_dag_frontier_peer =
                                                        Some(peer_id.clone());
                                                    let mut rt = runtime.write().await;
                                                    rt.sync_state =
                                                        "requesting_blocks".to_string();
                                                    rt.block_fetch_scheduler_queue_depth =
                                                        fetch_scheduler
                                                            .queue_depth()
                                                            .saturating_add(
                                                                frontier_fetch_scheduler
                                                                    .queue_depth(),
                                                            );
                                                    info!(
                                                        peer = %peer_id,
                                                        selected_tip = %selected_tip,
                                                        missing_required_context,
                                                        missing_selected_chain,
                                                        missing_frontier,
                                                        request_count,
                                                        "accepted Task 27 DAG frontier reconciliation plan"
                                                    );
                                                }
                                            }
                                            Err(error) => {
                                                warn!(
                                                    peer = %peer_id,
                                                    error = ?error,
                                                    "rejected Task 27 DAG frontier reconciliation plan"
                                                );
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        warn!(
                                            peer = %peer_id,
                                            "ignored Task 27 DAG frontier because local activated capabilities are not configured"
                                        );
                                    }
                                    Err(error) => {
                                        warn!(
                                            peer = %peer_id,
                                            error = %error,
                                            "failed reading local protocol-v2 capabilities for DAG frontier reconciliation"
                                        );
                                    }
                                }
                            } else {
                                warn!(
                                    peer = %peer_id,
                                    "ignored Task 27 DAG frontier because p2p is unavailable"
                                );
                            }
                        }""",
    "DagFrontier handler",
)

replace_once(
    """    #[test]
    fn observed_block_gap_activates_selected_locator_before_getblock_fallback() {""",
    """    #[test]
    fn task27_frontier_fetch_queue_covers_protocol_contract_maximum() {
        let mut scheduler = DependencyAwareFetchScheduler::with_limit(
            MAX_DAG_FRONTIER_FETCH_QUEUE_DEPTH,
        );
        let expected = MAX_DAG_FRONTIER_REQUIRED_CONTEXT
            + MAX_SELECTED_CHAIN_SUFFIX_HASHES
            + MAX_DAG_FRONTIER_ENTRIES;
        let hashes = (0..expected)
            .map(|index| format!("frontier-{index:08x}"))
            .collect::<Vec<_>>();

        assert_eq!(scheduler.queue_inventory(hashes), expected);
        assert_eq!(scheduler.queue_depth(), expected);
    }

    #[test]
    fn observed_block_gap_activates_selected_locator_before_getblock_fallback() {""",
    "frontier queue regression",
)

path.write_text(text)
