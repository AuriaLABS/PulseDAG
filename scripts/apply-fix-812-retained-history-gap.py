from pathlib import Path
import re

path = Path('apps/pulsedagd/src/main.rs')
text = path.read_text()


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f'{label}: expected exactly 1 match, found {count}')
    text = text.replace(old, new, 1)


replace_once(
'''fn selected_locator_peer_for_priority_gap(
    status: &P2pStatus,
    local_height: u64,
    minimum_gap: u64,
) -> Option<(String, u64)> {
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| remote.selected_height.saturating_sub(local_height) >= minimum_gap)
        .max_by_key(|remote| remote.selected_height)
        .map(|remote| (remote.peer_id.clone(), remote.selected_height))
}
''',
'''fn selected_locator_peer_for_priority_gap(
    status: &P2pStatus,
    local_height: u64,
    minimum_gap: u64,
    excluded_peers: &HashSet<String>,
) -> Option<(String, u64)> {
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| !excluded_peers.contains(&remote.peer_id))
        .filter(|remote| remote.selected_height.saturating_sub(local_height) >= minimum_gap)
        .max_by_key(|remote| remote.selected_height)
        .map(|remote| (remote.peer_id.clone(), remote.selected_height))
}
''',
'priority peer selector')

replace_once(
'''fn selected_locator_peer_for_reconcile(
    status: &P2pStatus,
    local: &TipInventoryStatus,
) -> Option<String> {
    let local_height = local.selected_height.unwrap_or_default();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| {
''',
'''fn selected_locator_peer_for_reconcile(
    status: &P2pStatus,
    local: &TipInventoryStatus,
    excluded_peers: &HashSet<String>,
) -> Option<String> {
    let local_height = local.selected_height.unwrap_or_default();
    status
        .remote_selected_tip_inventory
        .iter()
        .filter(|remote| remote.connected && remote.direct_request_capable)
        .filter(|remote| !excluded_peers.contains(&remote.peer_id))
        .filter(|remote| {
''',
'reconcile peer selector')

replace_once(
'''fn pending_selected_locator_accepts_response_common_ancestor(
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
    common_ancestor: Option<&str>,
) -> bool {
    common_ancestor.is_some()
        && pending_selected_locator_accepts_response_peer(pending, response_peer)
}

fn selected_common_ancestor_for_headers(
''',
'''fn pending_selected_locator_accepts_response_common_ancestor(
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
    common_ancestor: Option<&str>,
) -> bool {
    common_ancestor.is_some()
        && pending_selected_locator_accepts_response_peer(pending, response_peer)
}

fn selected_headers_indicate_retained_history_gap(
    headers: &[HeaderInventory],
    local_selected_height: u64,
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
    common_ancestor: Option<&str>,
    session_active: bool,
) -> bool {
    if session_active || common_ancestor.is_some() {
        return false;
    }
    let (Some(first), Some(pending), Some(response_peer)) =
        (headers.first(), pending, response_peer)
    else {
        return false;
    };
    pending.peer_id == response_peer
        && first.header.height > local_selected_height.saturating_add(1)
}

fn selected_common_ancestor_for_headers(
''',
'gap detector helper')

replace_once(
'''    !session_active
        && pending_block_requests == 0
        && pending.is_some_and(|pending| {
            now.saturating_sub(pending.requested_at_unix) >= SELECTED_SEGMENT_NO_PROGRESS_REARM_SECS
        })
}
''',
'''    !session_active
        && pending_block_requests == 0
        && pending.is_some_and(|pending| {
            !pending.retained_history_gap
                && now.saturating_sub(pending.requested_at_unix)
                    >= SELECTED_SEGMENT_NO_PROGRESS_REARM_SECS
        })
}
''',
'pending rearm suppression')

replace_once(
'''fn selected_segment_recovery_has_priority(
    session_active: bool,
    pending_requested_at_unix: Option<u64>,
    now_unix: u64,
) -> bool {
    session_active
        || pending_requested_at_unix.is_some_and(|requested_at| {
            now_unix.saturating_sub(requested_at) <= SELECTED_LOCATOR_PRIORITY_GRACE_SECS
        })
}
''',
'''fn selected_segment_recovery_has_priority(
    session_active: bool,
    pending: Option<&PendingSelectedLocator>,
    now_unix: u64,
) -> bool {
    session_active
        || pending.is_some_and(|pending| {
            pending.retained_history_gap
                || now_unix.saturating_sub(pending.requested_at_unix)
                    <= SELECTED_LOCATOR_PRIORITY_GRACE_SECS
        })
}

fn selected_segment_request_already_active_for_peer(
    session_active: bool,
    pending: Option<&PendingSelectedLocator>,
    candidate_peer: Option<&str>,
    now_unix: u64,
) -> bool {
    if session_active {
        return true;
    }
    if pending.is_some_and(|pending| {
        pending.retained_history_gap
            && candidate_peer.is_some_and(|candidate| candidate != pending.peer_id)
    }) {
        return false;
    }
    selected_segment_recovery_has_priority(false, pending, now_unix)
}
''',
'priority helper')

replace_once(
'''struct PendingSelectedLocator {
    request_id: u64,
    peer_id: String,
    locator: Vec<String>,
    requested_at_unix: u64,
}

#[derive(Debug, Default)]
struct SelectedSegmentLocatorState {
    next_request_id: u64,
    pending_locator: Option<PendingSelectedLocator>,
}
''',
'''struct PendingSelectedLocator {
    request_id: u64,
    peer_id: String,
    locator: Vec<String>,
    requested_at_unix: u64,
    retained_history_gap: bool,
}

#[derive(Debug, Default)]
struct SelectedSegmentLocatorState {
    next_request_id: u64,
    pending_locator: Option<PendingSelectedLocator>,
    retained_history_gap_peers: HashSet<String>,
}
''',
'locator state')

# Add the new flag to every PendingSelectedLocator literal, excluding the struct field declaration.
pattern = re.compile(r'(requested_at_unix:\s*(?!u64\b)[^,\n]+,\n)(\s*)(})')
text, literal_count = pattern.subn(r'\1\2retained_history_gap: false,\n\2\3', text)
if literal_count < 10:
    raise SystemExit(f'pending locator literals: expected >=10 replacements, found {literal_count}')

# Runtime priority calls now pass the pending object, so a retained-history-gap locator keeps
# generic GetBlock/GetHeaders fallbacks suppressed without abusing timestamps.
pattern = re.compile(
    r'guard\s*\n\s*\.pending_locator\s*\n\s*\.as_ref\(\)\s*\n\s*\.map\(\|pending\| pending\.requested_at_unix\),'
)
text, priority_arg_count = pattern.subn('guard.pending_locator.as_ref(),', text)
if priority_arg_count < 7:
    raise SystemExit(f'priority pending args: expected >=7 replacements, found {priority_arg_count}')

# Directed requests may supersede a terminally-pruned peer with a different usable peer.
text = text.replace(
'''selected_segment_recovery_has_priority(
                                    selected_segment_session.is_some(),
                                    guard.pending_locator.as_ref(),
                                    now,
                                )''',
'''selected_segment_request_already_active_for_peer(
                                    selected_segment_session.is_some(),
                                    guard.pending_locator.as_ref(),
                                    Some(peer_id.as_str()),
                                    now,
                                )''',
1)
text = text.replace(
'''selected_segment_recovery_has_priority(
                            active_session,
                            guard.pending_locator.as_ref(),
                            now,
                        )''',
'''selected_segment_request_already_active_for_peer(
                            active_session,
                            guard.pending_locator.as_ref(),
                            Some(peer_id.as_str()),
                            now,
                        )''',
1)
text = text.replace(
'''selected_segment_recovery_has_priority(
                                            active_session,
                                            guard.pending_locator.as_ref(),
                                            now,
                                        )''',
'''selected_segment_request_already_active_for_peer(
                                            active_session,
                                            guard.pending_locator.as_ref(),
                                            selected_locator_peer.as_deref(),
                                            now,
                                        )''',
1)

# Exclude peers already proven to have a retained-history gap from directed locator selection.
replace_once(
'''                        let immediate_selected_locator = p2p
                            .as_ref()
                            .and_then(|handle| handle.status().ok())
                            .and_then(|status| {
                                selected_locator_peer_for_priority_gap(
                                    &status,
                                    local_height,
                                    SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS,
                                )
                            });
''',
'''                        let retained_history_gap_peers = {
                            let guard = selected_segment_locator_state.lock().await;
                            guard.retained_history_gap_peers.clone()
                        };
                        let immediate_selected_locator = p2p
                            .as_ref()
                            .and_then(|handle| handle.status().ok())
                            .and_then(|status| {
                                selected_locator_peer_for_priority_gap(
                                    &status,
                                    local_height,
                                    SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS,
                                    &retained_history_gap_peers,
                                )
                            });
''',
'tips peer exclusion')

replace_once(
'''                let proactive_selected_locator = p2p_status.as_ref().and_then(|status| {
                    selected_locator_peer_for_priority_gap(
                        status,
                        best_height,
                        SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS,
                    )
                });
''',
'''                let retained_history_gap_peers = {
                    let guard = selected_segment_locator_state.lock().await;
                    guard.retained_history_gap_peers.clone()
                };
                let proactive_selected_locator = p2p_status.as_ref().and_then(|status| {
                    selected_locator_peer_for_priority_gap(
                        status,
                        best_height,
                        SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS,
                        &retained_history_gap_peers,
                    )
                });
''',
'proactive peer exclusion')

replace_once(
'''                                    let selected_limits = SelectedSegmentLimits::default();
                                    let selected_locator_peer = selected_locator_peer_for_reconcile(
                                        &status,
                                        &local_inventory,
                                    );
                                    let selected_locator_needed = selected_locator_peer.is_some();
''',
'''                                    let selected_limits = SelectedSegmentLimits::default();
                                    let retained_history_gap_peers = {
                                        let guard = selected_segment_locator_state.lock().await;
                                        guard.retained_history_gap_peers.clone()
                                    };
                                    let selected_locator_peer = selected_locator_peer_for_reconcile(
                                        &status,
                                        &local_inventory,
                                        &retained_history_gap_peers,
                                    );
                                    let selected_locator_blocked_by_retained_history_gap =
                                        selected_locator_peer.is_none()
                                            && !retained_history_gap_peers.is_empty();
                                    let selected_locator_needed = selected_locator_peer.is_some()
                                        || selected_locator_blocked_by_retained_history_gap;
''',
'final reconcile peer exclusion')

replace_once(
'''                                            rt.final_quiescence_selected_sync_blocked_reason =
                                                Some("selected_locator_request_failed".to_string());
''',
'''                                            rt.final_quiescence_selected_sync_blocked_reason =
                                                Some(if selected_locator_blocked_by_retained_history_gap {
                                                    "retained_history_gap".to_string()
                                                } else {
                                                    "selected_locator_request_failed".to_string()
                                                });
                                            if selected_locator_blocked_by_retained_history_gap {
                                                rt.sync_state = DagSyncStage::SelectedSegmentFailed
                                                    .as_str()
                                                    .to_string();
                                            }
''',
'final reconcile retained gap reason')

# Include local height in the inbound header analysis and detect a page that starts beyond the
# local attach point with no known ancestor from the exact requested peer.
replace_once(
'''                        let (known_blocks_for_segment, common_ancestor, common_ancestor_height) = {
                            let guard = chain.read().await;
                            let known = known_hashes_for_scheduler(&guard);
                            let common = selected_common_ancestor_for_headers(
                                &headers,
                                &known,
                                pending_selected_locator.as_ref(),
                            );
                            let height = common
                                .as_ref()
                                .and_then(|hash| guard.dag.blocks.get(hash))
                                .map(|block| block.header.height)
                                .unwrap_or(0);
                            (known, common, height)
                        };
''',
'''                        let (
                            known_blocks_for_segment,
                            common_ancestor,
                            common_ancestor_height,
                            local_selected_height,
                        ) = {
                            let guard = chain.read().await;
                            let known = known_hashes_for_scheduler(&guard);
                            let common = selected_common_ancestor_for_headers(
                                &headers,
                                &known,
                                pending_selected_locator.as_ref(),
                            );
                            let height = common
                                .as_ref()
                                .and_then(|hash| guard.dag.blocks.get(hash))
                                .map(|block| block.header.height)
                                .unwrap_or(0);
                            (known, common, height, guard.dag.best_height)
                        };
''',
'header local height')

replace_once(
'''                        let selected_segment_validation = common_ancestor.as_ref().map(|common| {
                            validate_selected_header_segment(
                                common,
                                &headers,
                                &known_blocks_for_segment,
                            )
                        });
''',
'''                        let selected_segment_validation = common_ancestor.as_ref().map(|common| {
                            validate_selected_header_segment(
                                common,
                                &headers,
                                &known_blocks_for_segment,
                            )
                        });
                        let retained_history_gap = selected_headers_indicate_retained_history_gap(
                            &headers,
                            local_selected_height,
                            pending_selected_locator.as_ref(),
                            peer_id.as_deref(),
                            common_ancestor.as_deref(),
                            selected_segment_session.is_some(),
                        );
                        let first_remote_header_height =
                            headers.first().map(|item| item.header.height).unwrap_or(0);
                        let retained_history_gap_newly_detected = if retained_history_gap {
                            if let Some(response_peer) = peer_id.as_ref() {
                                let mut guard = selected_segment_locator_state.lock().await;
                                let inserted = guard
                                    .retained_history_gap_peers
                                    .insert(response_peer.clone());
                                if let Some(pending) = guard
                                    .pending_locator
                                    .as_mut()
                                    .filter(|pending| pending.peer_id == *response_peer)
                                {
                                    pending.retained_history_gap = true;
                                }
                                inserted
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if retained_history_gap_newly_detected {
                            let response_peer = peer_id.as_deref().unwrap_or("unknown");
                            warn!(
                                event = "selected_retained_history_gap_detected",
                                peer = %response_peer,
                                local_selected_height,
                                first_remote_header_height,
                                "requested peer's retained header page begins above the local attach point; header sync cannot bridge pruned history"
                            );
                            let _ = storage.append_runtime_event(
                                "warn",
                                "selected_retained_history_gap_detected",
                                &format!(
                                    "peer={} local_selected_height={} first_remote_header_height={}",
                                    response_peer, local_selected_height, first_remote_header_height
                                ),
                            );
                        }
''',
'header gap detection')

replace_once(
'''                        fetch_scheduler.queue_headers(candidates);
''',
'''                        if !retained_history_gap {
                            fetch_scheduler.queue_headers(candidates);
                        }
''',
'skip impossible header queue')

replace_once(
'''                        } else {
                            rt.selected_segment_uncorrelated_headers_total = rt
                                .selected_segment_uncorrelated_headers_total
                                .saturating_add(headers.len() as u64);
                        }
                        if !headers_correlated
                            && final_quiescence_reconcile_pending(
''',
'''                        } else {
                            rt.selected_segment_uncorrelated_headers_total = rt
                                .selected_segment_uncorrelated_headers_total
                                .saturating_add(headers.len() as u64);
                            if retained_history_gap {
                                rt.sync_state =
                                    DagSyncStage::SelectedSegmentFailed.as_str().to_string();
                                rt.final_quiescence_selected_sync_blocked_reason =
                                    Some("retained_history_gap".to_string());
                                if retained_history_gap_newly_detected {
                                    let entry = rt
                                        .selected_segment_failure_total
                                        .entry("retained_history_gap".to_string())
                                        .or_insert(0);
                                    *entry = entry.saturating_add(1);
                                }
                            }
                        }
                        if !headers_correlated
                            && !retained_history_gap
                            && final_quiescence_reconcile_pending(
''',
'gap runtime classification')

# Keep failed state stable when a Tips response arrives while all relevant peers are known to be
# too pruned; generic tip fetches remain suppressed by the blocked pending locator.
replace_once(
'''                        let unknown_tips = {
                            let guard = chain.read().await;
                            tips.into_iter()
                                .filter(|tip| !guard.dag.blocks.contains_key(tip))
                                .collect::<Vec<_>>()
                        };
''',
'''                        let retained_history_gap_active = {
                            let guard = selected_segment_locator_state.lock().await;
                            !guard.retained_history_gap_peers.is_empty()
                        };
                        let unknown_tips = {
                            let guard = chain.read().await;
                            tips.into_iter()
                                .filter(|tip| !guard.dag.blocks.contains_key(tip))
                                .collect::<Vec<_>>()
                        };
''',
'tips gap active')

replace_once(
'''                            rt.sync_state = if unknown_tips.is_empty() {
                                "synced"
                            } else {
                                "requesting_blocks"
                            }
                            .to_string();
''',
'''                            rt.sync_state = if retained_history_gap_active {
                                DagSyncStage::SelectedSegmentFailed.as_str()
                            } else if unknown_tips.is_empty() {
                                "synced"
                            } else {
                                "requesting_blocks"
                            }
                            .to_string();
''',
'tips preserve failed state')

replace_once(
'''                                rt.final_quiescence_selected_sync_blocked_reason =
                                    Some("selected_locator_priority".to_string());
                                continue;
''',
'''                                rt.final_quiescence_selected_sync_blocked_reason = Some(
                                    if retained_history_gap_active {
                                        "retained_history_gap"
                                    } else {
                                        "selected_locator_priority"
                                    }
                                    .to_string(),
                                );
                                continue;
''',
'tips preserve blocked reason')

# Successful convergence resets peer exclusions for future independent sync cycles.
replace_once(
'''                            if selected_segment_completed {
                                selected_segment_session = None;
                                selected_segment_locator_state.lock().await.pending_locator = None;
                                if let Some(ref p2p_handle) = p2p {
''',
'''                            if selected_segment_completed {
                                selected_segment_session = None;
                                {
                                    let mut locator_state =
                                        selected_segment_locator_state.lock().await;
                                    locator_state.pending_locator = None;
                                    locator_state.retained_history_gap_peers.clear();
                                }
                                if let Some(ref p2p_handle) = p2p {
''',
'clear exclusions after success')

# Update selector tests for the exclusion parameter and prove peer rotation.
text = text.replace(
    'selected_locator_peer_for_priority_gap(&status, 120, 64)',
    'selected_locator_peer_for_priority_gap(&status, 120, 64, &HashSet::new())'
)
text = text.replace(
    'selected_locator_peer_for_priority_gap(&status, 190, 64)',
    'selected_locator_peer_for_priority_gap(&status, 190, 64, &HashSet::new())'
)
replace_once(
'''        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 190, 64, &HashSet::new()),
            None
        );
''',
'''        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 190, 64, &HashSet::new()),
            None
        );
        let excluded = HashSet::from(["peer-large-gap".to_string()]);
        assert_eq!(
            selected_locator_peer_for_priority_gap(&status, 120, 20, &excluded),
            Some(("peer-small-gap".to_string(), 150))
        );
''',
'priority peer rotation test')

text = text.replace(
    'selected_locator_peer_for_reconcile(&status, &local)',
    'selected_locator_peer_for_reconcile(&status, &local, &HashSet::new())'
)

# Multiline runtime priority selector call not covered above.
text = text.replace(
'''selected_locator_peer_for_priority_gap(
                        status,
                        best_height,
                        SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS,
                        &retained_history_gap_peers,
                    )''',
'''selected_locator_peer_for_priority_gap(
                        status,
                        best_height,
                        SELECTED_SEGMENT_PRIORITY_GAP_BLOCKS,
                        &retained_history_gap_peers,
                    )''')

# Update the multiline test call for immediate rejoin priority if present.
text = text.replace(
'''selected_locator_peer_for_priority_gap(
                &status,
                1303,
                SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS,
            )''',
'''selected_locator_peer_for_priority_gap(
                &status,
                1303,
                SELECTED_REJOIN_REARM_MIN_GAP_BLOCKS,
                &HashSet::new(),
            )''')

# Replace the old priority test with one that proves retained gaps are terminal for generic
# recovery yet may be superseded by a different directed peer.
old_priority_test = '''    #[test]
    fn selected_segment_priority_is_bounded_and_closeout_capacity_covers_gap() {
        assert!(selected_segment_recovery_has_priority(true, None, 100));
        assert!(selected_segment_recovery_has_priority(false, Some(80), 100));
        assert!(!selected_segment_recovery_has_priority(false, Some(1), 100));
        let limits = SelectedSegmentLimits::default();
        assert!(limits.max_inflight_blocks_per_peer >= 96);
        assert!(limits.max_inflight_blocks_per_peer <= 128);
    }
'''
new_priority_test = '''    #[test]
    fn selected_segment_priority_is_bounded_and_retained_gap_is_terminal_for_same_peer() {
        let fresh = PendingSelectedLocator {
            request_id: 1,
            peer_id: "peer-a".to_string(),
            locator: vec!["local".to_string()],
            requested_at_unix: 80,
            retained_history_gap: false,
        };
        let stale = PendingSelectedLocator {
            requested_at_unix: 1,
            ..fresh.clone()
        };
        let retained_gap = PendingSelectedLocator {
            retained_history_gap: true,
            ..stale.clone()
        };
        assert!(selected_segment_recovery_has_priority(true, None, 100));
        assert!(selected_segment_recovery_has_priority(false, Some(&fresh), 100));
        assert!(!selected_segment_recovery_has_priority(false, Some(&stale), 100));
        assert!(selected_segment_recovery_has_priority(
            false,
            Some(&retained_gap),
            10_000
        ));
        assert!(selected_segment_request_already_active_for_peer(
            false,
            Some(&retained_gap),
            Some("peer-a"),
            10_000,
        ));
        assert!(!selected_segment_request_already_active_for_peer(
            false,
            Some(&retained_gap),
            Some("peer-b"),
            10_000,
        ));
        let limits = SelectedSegmentLimits::default();
        assert!(limits.max_inflight_blocks_per_peer >= 96);
        assert!(limits.max_inflight_blocks_per_peer <= 128);
    }
'''
replace_once(old_priority_test, new_priority_test, 'priority unit test')

# Add the concrete 611 -> 783 regression immediately before the fallback-correlation test.
marker = '''    #[test]
    fn requested_peer_fallback_common_ancestor_can_own_selected_headers() {
'''
if text.count(marker) != 1:
    raise SystemExit('gap regression marker missing or duplicated')
gap_tests = '''    #[test]
    fn retained_history_gap_is_detected_only_for_exact_requested_peer_without_attach_point() {
        let pending = PendingSelectedLocator {
            request_id: 812,
            peer_id: "peer-a".to_string(),
            locator: vec!["local-611".to_string()],
            requested_at_unix: 100,
            retained_history_gap: false,
        };
        let headers = vec![selected_test_header("remote-783", "pruned-782", 783)];
        assert!(selected_headers_indicate_retained_history_gap(
            &headers,
            611,
            Some(&pending),
            Some("peer-a"),
            None,
            false,
        ));
        assert!(!selected_headers_indicate_retained_history_gap(
            &headers,
            611,
            Some(&pending),
            Some("peer-b"),
            None,
            false,
        ));
        assert!(!selected_headers_indicate_retained_history_gap(
            &headers,
            611,
            Some(&pending),
            Some("peer-a"),
            Some("local-611"),
            false,
        ));
        assert!(!selected_headers_indicate_retained_history_gap(
            &[selected_test_header("remote-612", "local-611", 612)],
            611,
            Some(&pending),
            Some("peer-a"),
            None,
            false,
        ));
        assert!(!selected_headers_indicate_retained_history_gap(
            &headers,
            611,
            Some(&pending),
            Some("peer-a"),
            None,
            true,
        ));
    }

    #[test]
    fn retained_history_gap_locator_does_not_rearm_after_timeout() {
        let pending = PendingSelectedLocator {
            request_id: 813,
            peer_id: "peer-a".to_string(),
            locator: vec!["local-611".to_string()],
            requested_at_unix: 100,
            retained_history_gap: true,
        };
        assert!(!pending_selected_locator_should_rearm(
            Some(&pending),
            false,
            0,
            10_000,
        ));
    }

'''
text = text.replace(marker, gap_tests + marker, 1)

# Ensure every remaining selector call has the exclusion argument. These counts include definitions.
if text.count('selected_locator_peer_for_priority_gap(') < 5:
    raise SystemExit('unexpected priority selector call count')
if text.count('selected_locator_peer_for_reconcile(') < 4:
    raise SystemExit('unexpected reconcile selector call count')

path.write_text(text)
print('applied #812 retained-history-gap fail-fast patch')
print('pending locator literals updated:', literal_count)
print('runtime priority args updated:', priority_arg_count)
