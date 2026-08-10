from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected 1 match, found {count}")
    return text.replace(old, new, 1)


# --- RPC: retain fresh connected remote selected-tip evidence in /sync/status. ---
rpc_path = Path("crates/pulsedag-rpc/src/handlers/sync.rs")
rpc = rpc_path.read_text()

rpc = replace_once(
    rpc,
    "use super::canonical_sync::build_canonical_sync_state;\n",
    "use super::canonical_sync::{\n"
    "    build_canonical_sync_state_with_remote_evidence, remote_sync_evidence_from_p2p_status,\n"
    "    CanonicalSyncState,\n"
    "};\n",
    "canonical sync imports",
)

needle = '''fn cache_sync_status_response(data: &SyncStatusData) {
    if let Ok(mut cache) = SYNC_STATUS_RESPONSE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        *cache = Some(data.clone());
    }
}
'''
helper = needle + '''
fn build_sync_status_canonical_state(
    chain: &pulsedag_core::ChainState,
    runtime: &crate::api::NodeRuntimeStats,
    persisted_block_count: usize,
    now_unix: u64,
    p2p_status: Option<&pulsedag_p2p::P2pStatus>,
) -> CanonicalSyncState {
    let remote_evidence = remote_sync_evidence_from_p2p_status(p2p_status, now_unix);
    build_canonical_sync_state_with_remote_evidence(
        chain,
        runtime,
        persisted_block_count,
        now_unix,
        p2p_status.and_then(|status| status.selected_sync_peer.clone()),
        &remote_evidence,
    )
}
'''
rpc = replace_once(rpc, needle, helper, "sync status canonical helper")

old_call = '''    let canonical_sync = build_canonical_sync_state(
        &chain,
        &runtime,
        persisted_block_count,
        now_unix,
        p2p_status
            .as_ref()
            .and_then(|snapshot| snapshot.status.selected_sync_peer.clone()),
    );
'''
new_call = '''    let canonical_sync = build_sync_status_canonical_state(
        &chain,
        &runtime,
        persisted_block_count,
        now_unix,
        p2p_status.as_ref().map(|snapshot| &snapshot.status),
    );
'''
rpc = replace_once(rpc, old_call, new_call, "sync status canonical call")

rpc = replace_once(
    rpc,
    "    use super::{get_sync_missing, get_sync_status};\n",
    "    use super::{build_sync_status_canonical_state, get_sync_missing, get_sync_status};\n",
    "test imports",
)

needle_test = '''    #[tokio::test]
    async fn sync_status_derives_catchup_stage_and_recovery_reason_coherently() {
'''
new_test = '''    #[test]
    fn sync_status_canonical_state_uses_fresh_remote_tip_inventory() {
        let chain = pulsedag_core::genesis::init_chain_state("testnet-dev".to_string());
        let runtime = crate::api::NodeRuntimeStats::default();
        let status = pulsedag_p2p::P2pStatus {
            chain_id: "testnet-dev".to_string(),
            connected_peers: vec!["peer-a".to_string()],
            direct_request_capable_peers: vec!["peer-a".to_string()],
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "peer-a".to_string(),
                chain_id: "testnet-dev".to_string(),
                selected_tip: Some("remote-3".to_string()),
                selected_height: 3,
                selected_blue_score: Some(3),
                ordered_dag_tip: Some("ordered-3".to_string()),
                state_root_digest: Some("root-3".to_string()),
                observed_at_unix: 1_000,
                inventory_generation: 1,
                age_secs: 0,
                direct_request_capable: true,
                connected: true,
                ..pulsedag_p2p::RemoteSelectedTipStatus::default()
            }],
            ..pulsedag_p2p::P2pStatus::default()
        };

        let canonical = build_sync_status_canonical_state(
            &chain,
            &runtime,
            chain.dag.blocks.len(),
            1_000,
            Some(&status),
        );

        assert_eq!(canonical.best_remote_selected_height, Some(3));
        assert_eq!(canonical.network_selected_height_gap, 3);
        assert_eq!(canonical.sync_state, "locating_common_ancestor");
        assert_eq!(canonical.catchup_stage, "discovering");
        assert_ne!(canonical.sync_state, "synced");
    }

''' + needle_test
rpc = replace_once(rpc, needle_test, new_test, "remote inventory wiring regression")
rpc_path.write_text(rpc)


# --- Daemon: continue a bounded selected segment immediately and re-arm any residual ahead gap. ---
main_path = Path("apps/pulsedagd/src/main.rs")
main = main_path.read_text()

main = replace_once(
    main,
    "const SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS: u64 = 64;\n",
    "const SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS: u64 = 64;\n"
    "const SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS: u64 = 1;\n",
    "small gap rejoin constant",
)

rearm_helper = '''fn selected_segment_session_should_rearm(
    session: Option<&SelectedSegmentSession>,
    pending_block_requests: usize,
    now: u64,
) -> bool {
    pending_block_requests == 0
        && session.is_some_and(|session| {
            session.no_progress_expired(now, SELECTED_SEGMENT_NO_PROGRESS_REARM_SECS)
        })
}
'''
continuation_helper = rearm_helper + '''
fn selected_segment_continuation_needed(
    session: Option<&SelectedSegmentSession>,
    local_selected_height: u64,
    chunk_completed: bool,
) -> bool {
    chunk_completed
        && session.is_some_and(|session| {
            session.can_start_chunk() && local_selected_height < session.remote_selected_height
        })
}
'''
main = replace_once(
    main,
    rearm_helper,
    continuation_helper,
    "selected segment continuation helper",
)

# A Tips response after watchdog re-arm must recover even a residual 1..63 block gap.
tips_marker = "                    InboundEvent::Tips { tips } => {\n"
tips_index = main.find(tips_marker)
if tips_index < 0:
    raise SystemExit("tips handler marker not found")
priority_old = "                                    SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS,\n"
priority_index = main.find(priority_old, tips_index)
if priority_index < 0:
    raise SystemExit("tips priority threshold not found")
main = (
    main[:priority_index]
    + "                                    SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS,\n"
    + main[priority_index + len(priority_old):]
)

main = replace_once(
    main,
    "                            let mut selected_segment_completed = false;\n",
    "                            let mut selected_segment_completed = false;\n"
    "                            let mut selected_segment_chunk_completed = false;\n",
    "selected chunk completion flag",
)

old_chunk_complete = '''                                        if session.complete_current_chunk_if_applied() {
                                            rt.selected_segment_chunks_completed_total = rt
                                                .selected_segment_chunks_completed_total
                                                .saturating_add(1);
                                        }
'''
new_chunk_complete = '''                                        if session.complete_current_chunk_if_applied() {
                                            selected_segment_chunk_completed = true;
                                            rt.selected_segment_chunks_completed_total = rt
                                                .selected_segment_chunks_completed_total
                                                .saturating_add(1);
                                        }
'''
main = replace_once(
    main,
    old_chunk_complete,
    new_chunk_complete,
    "selected chunk completion tracking",
)

old_runtime_state = '''                                rt.sync_state = if selected_segment_completed {
                                    DagSyncStage::SelectedSegmentComplete.as_str().to_string()
                                } else if guard.orphan_blocks.is_empty() {
                                    "synced".to_string()
                                } else {
                                    "catching_up".to_string()
                                };
'''
new_runtime_state = '''                                rt.sync_state = if selected_segment_completed {
                                    DagSyncStage::SelectedSegmentComplete.as_str().to_string()
                                } else if selected_segment_session.is_some() {
                                    DagSyncStage::ApplyingSelectedSegment.as_str().to_string()
                                } else if guard.orphan_blocks.is_empty() {
                                    "synced".to_string()
                                } else {
                                    "catching_up".to_string()
                                };
'''
main = replace_once(
    main,
    old_runtime_state,
    new_runtime_state,
    "do not report runtime synced during active selected session",
)

completion_marker = '''                            if selected_segment_completed {
                                selected_segment_session = None;
'''
continuation_block = '''                            if selected_segment_continuation_needed(
                                selected_segment_session.as_ref(),
                                guard.dag.best_height,
                                selected_segment_chunk_completed,
                            ) {
                                let (peer_id, remote_height) = selected_segment_session
                                    .as_ref()
                                    .map(|session| {
                                        (session.peer_id.clone(), session.remote_selected_height)
                                    })
                                    .expect("continuation requires active selected session");
                                let selected_locator = guard
                                    .dag
                                    .selected_chain
                                    .iter()
                                    .rev()
                                    .take(32)
                                    .cloned()
                                    .collect::<Vec<_>>();
                                let selected_limits = SelectedSegmentLimits::default();
                                let request_id = {
                                    let locator_state = selected_segment_locator_state.lock().await;
                                    locator_state.next_request_id
                                };
                                if let Some(ref p2p_handle) = p2p {
                                    match p2p_handle.request_headers_from(
                                        &peer_id,
                                        &selected_locator,
                                        None,
                                        selected_limits.headers_per_chunk,
                                    ) {
                                        Ok(()) => {
                                            let requested_at = now_unix();
                                            {
                                                let mut locator_state =
                                                    selected_segment_locator_state.lock().await;
                                                locator_state.next_request_id =
                                                    locator_state.next_request_id.saturating_add(1);
                                                locator_state.pending_locator =
                                                    Some(PendingSelectedLocator {
                                                        request_id,
                                                        peer_id: peer_id.clone(),
                                                        locator: selected_locator,
                                                        requested_at_unix: requested_at,
                                                    });
                                            }
                                            if let Some(session) =
                                                selected_segment_session.as_mut()
                                            {
                                                session.state =
                                                    SelectedSegmentSessionState::RequestingHeaders;
                                                session.updated_at_unix = requested_at;
                                            }
                                            let mut rt = runtime.write().await;
                                            rt.selected_segment_header_requests_total = rt
                                                .selected_segment_header_requests_total
                                                .saturating_add(1);
                                            rt.header_requests_sent =
                                                rt.header_requests_sent.saturating_add(1);
                                            rt.sync_state = DagSyncStage::RequestingSelectedHeaders
                                                .as_str()
                                                .to_string();
                                            info!(
                                                event = "selected_segment_chunk_continuation",
                                                peer = %peer_id,
                                                local_height = guard.dag.best_height,
                                                remote_height,
                                                request_id,
                                                "selected chunk completed below remote tip; requested next selected header page"
                                            );
                                        }
                                        Err(e) => {
                                            warn!(
                                                error = %e,
                                                peer = %peer_id,
                                                local_height = guard.dag.best_height,
                                                remote_height,
                                                "failed requesting selected header continuation after chunk completion"
                                            );
                                        }
                                    }
                                }
                            }

''' + completion_marker
main = replace_once(
    main,
    completion_marker,
    continuation_block,
    "selected chunk continuation request",
)

# Focused regressions for the exact residual-gap/rejoin failure mode.
test_marker = '''    #[test]
    fn selected_segment_stale_pending_locator_rearms_without_session() {
'''
new_tests = '''    #[test]
    fn small_rejoin_gap_reactivates_selected_locator_after_rearm() {
        let status = P2pStatus {
            remote_selected_tip_inventory: vec![pulsedag_p2p::RemoteSelectedTipStatus {
                peer_id: "peer-a".to_string(),
                selected_height: 1306,
                connected: true,
                direct_request_capable: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        assert_eq!(
            selected_locator_peer_for_priority_gap(
                &status,
                1303,
                SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS,
            ),
            Some(("peer-a".to_string(), 1306))
        );
    }

    #[test]
    fn selected_segment_completed_chunk_below_remote_tip_requires_continuation() {
        let headers = vec![
            selected_test_header("b1", "common", 1),
            selected_test_header("b2", "b1", 2),
            selected_test_header("b3", "b2", 3),
        ];
        let locator = vec!["common".to_string()];
        let mut session = SelectedSegmentSession::new(
            1,
            "peer-a".to_string(),
            "common".to_string(),
            0,
            &headers,
            &locator,
            1,
            100,
        )
        .expect("session");
        assert!(session.start_chunk(vec!["b1".to_string(), "b2".to_string()], 101));
        session.mark_applied("b1", 102);
        session.mark_applied("b2", 103);
        assert!(session.complete_current_chunk_if_applied());

        assert!(selected_segment_continuation_needed(Some(&session), 2, true));
        assert!(!selected_segment_continuation_needed(Some(&session), 3, true));
        assert!(!selected_segment_continuation_needed(Some(&session), 2, false));
    }

''' + test_marker
main = replace_once(main, test_marker, new_tests, "small rejoin regressions")
main_path.write_text(main)
