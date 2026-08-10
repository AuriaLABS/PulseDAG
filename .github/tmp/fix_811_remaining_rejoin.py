from pathlib import Path

path = Path('apps/pulsedagd/src/main.rs')
s = path.read_text()


def replace_once(old, new, label):
    global s
    count = s.count(old)
    if count != 1:
        raise SystemExit(f'{label}: expected 1 match, found {count}')
    s = s.replace(old, new, 1)

old = '''fn pending_selected_locator_accepts_response_peer(
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
) -> bool {
    let (Some(pending), Some(response_peer)) = (pending, response_peer) else {
        return false;
    };
    pending.peer_id == "broadcast-observed-block-gap" || pending.peer_id == response_peer
}
'''
new = old + '''
fn selected_common_ancestor_for_headers(
    headers: &[HeaderInventory],
    known_blocks: &HashSet<String>,
    pending: Option<&PendingSelectedLocator>,
) -> Option<String> {
    let first = headers.first()?;
    if let Some(pending) = pending {
        if let Some(parent) = first.header.parents.iter().find(|parent| {
            known_blocks.contains(*parent)
                && pending.locator.iter().any(|locator_hash| locator_hash == *parent)
        }) {
            return Some(parent.clone());
        }
    }
    first
        .header
        .parents
        .iter()
        .find(|parent| known_blocks.contains(*parent))
        .cloned()
}

fn pending_selected_locator_should_rearm(
    pending: Option<&PendingSelectedLocator>,
    session_active: bool,
    pending_block_requests: usize,
    now: u64,
) -> bool {
    !session_active
        && pending_block_requests == 0
        && pending.is_some_and(|pending| {
            now.saturating_sub(pending.requested_at_unix)
                >= SELECTED_SEGMENT_NO_PROGRESS_REARM_SECS
        })
}
'''
replace_once(old, new, 'selected helper insertion')

old = '''    fn accept_header_page(
        &mut self,
        pending: &PendingSelectedLocator,
        headers: &[HeaderInventory],
        now: u64,
    ) {
        self.locator_request_id = pending.request_id;
        self.locator_fingerprint = pending.locator.join(",");
        for item in headers {
            if !self.expected_header_hashes.contains(&item.hash) {
                self.expected_header_hashes.push(item.hash.clone());
            }
            if item.header.height > self.remote_selected_height {
                self.remote_selected_tip = item.hash.clone();
                self.remote_selected_height = item.header.height;
            }
        }
        self.updated_at_unix = now;
    }
'''
new = '''    fn accept_header_page(
        &mut self,
        pending: &PendingSelectedLocator,
        headers: &[HeaderInventory],
        now: u64,
    ) -> bool {
        self.locator_request_id = pending.request_id;
        self.locator_fingerprint = pending.locator.join(",");
        let mut progressed = false;
        for item in headers {
            if !self.expected_header_hashes.contains(&item.hash) {
                self.expected_header_hashes.push(item.hash.clone());
                progressed = true;
            }
            if item.header.height > self.remote_selected_height {
                self.remote_selected_tip = item.hash.clone();
                self.remote_selected_height = item.header.height;
                progressed = true;
            }
        }
        if progressed {
            self.updated_at_unix = now;
        }
        progressed
    }
'''
replace_once(old, new, 'header page progress accounting')

old = '''fn selected_segment_request_candidates(
    headers: &[HeaderInventory],
    limits: SelectedSegmentLimits,
    accepted: &HashSet<String>,
    pending: &HashSet<String>,
) -> Vec<String> {
    selected_segment_request_order(headers, limits.headers_per_chunk)
        .into_iter()
        .filter(|hash| !accepted.contains(hash) && !pending.contains(hash))
        .take(limits.max_inflight_blocks_per_peer)
        .collect()
}
'''
new = '''fn selected_segment_request_candidates(
    headers: &[HeaderInventory],
    limits: SelectedSegmentLimits,
    accepted: &HashSet<String>,
    pending: &HashSet<String>,
) -> Vec<String> {
    let mut scheduler = DependencyAwareFetchScheduler::with_limit(
        limits
            .headers_per_chunk
            .saturating_mul(2)
            .max(limits.max_inflight_blocks_per_peer),
    );
    scheduler.queue_headers(headers.iter().map(|item| HeaderFetchCandidate {
        hash: item.hash.clone(),
        parents: item.header.parents.clone(),
        height: item.header.height,
    }));
    scheduler
        .next_requests(accepted, pending, limits.max_inflight_blocks_per_peer)
        .requests
}
'''
replace_once(old, new, 'selected dependency candidates')

old = '''        if item
            .header
            .parents
            .iter()
            .any(|parent| !staged.contains(parent))
        {
            return Err("unknown_or_unstaged_parent");
        }
'''
new = '''        if !item
            .header
            .parents
            .iter()
            .any(|parent| staged.contains(parent))
        {
            return Err("no_known_or_staged_parent");
        }
'''
replace_once(old, new, 'selected validation merge parents')

needle = '''                if selected_segment_session_should_rearm(
                    selected_segment_session.as_ref(),
                    block_requests.pending.len(),
                    now,
                ) {
'''
insert = '''                let stale_pending_locator = {
                    let mut guard = selected_segment_locator_state.lock().await;
                    if pending_selected_locator_should_rearm(
                        guard.pending_locator.as_ref(),
                        selected_segment_session.is_some(),
                        block_requests.pending.len(),
                        now,
                    ) {
                        guard.pending_locator.take()
                    } else {
                        None
                    }
                };
                if let Some(stale_locator) = stale_pending_locator {
                    {
                        let mut rt = runtime.write().await;
                        rt.selected_segment_restarts_total =
                            rt.selected_segment_restarts_total.saturating_add(1);
                        rt.selected_segment_gap_blocks = 0;
                        rt.final_quiescence_selected_sync_blocked_reason =
                            Some("selected_locator_no_progress_rearm".to_string());
                        rt.sync_state = "requesting_tips".to_string();
                    }
                    if let Some(ref p2p_handle) = p2p {
                        if let Err(e) = p2p_handle.request_tips() {
                            warn!(
                                error = %e,
                                request_id = stale_locator.request_id,
                                peer = %stale_locator.peer_id,
                                "failed requesting fresh tips after stale selected locator rearm"
                            );
                        } else {
                            let mut rt = runtime.write().await;
                            rt.tips_requested = rt.tips_requested.saturating_add(1);
                        }
                    }
                    warn!(
                        event = "selected_locator_no_progress_rearm",
                        request_id = stale_locator.request_id,
                        peer = %stale_locator.peer_id,
                        no_progress_secs = now.saturating_sub(stale_locator.requested_at_unix),
                        "expired stale selected locator and requested fresh tip reconciliation"
                    );
                }

'''+needle
replace_once(needle, insert, 'pending locator watchdog')

old = '''                    InboundEvent::Headers { peer_id, headers } => {
                        let selected_limits = SelectedSegmentLimits::default();
                        let (known_blocks_for_segment, common_ancestor, common_ancestor_height) = {
                            let guard = chain.read().await;
                            let known = known_hashes_for_scheduler(&guard);
                            let common = headers.first().and_then(|first| {
                                first
                                    .header
                                    .parents
                                    .iter()
                                    .find(|parent| known.contains(*parent))
                                    .cloned()
                            });
                            let height = common
                                .as_ref()
                                .and_then(|hash| guard.dag.blocks.get(hash))
                                .map(|block| block.header.height)
                                .unwrap_or(0);
                            (known, common, height)
                        };
                        let selected_segment_validation = common_ancestor.as_ref().map(|common| {
                            validate_selected_header_segment(
                                common,
                                &headers,
                                &known_blocks_for_segment,
                            )
                        });
                        let pending_selected_locator = {
                            let guard = selected_segment_locator_state.lock().await;
                            guard.pending_locator.clone()
                        };
'''
new = '''                    InboundEvent::Headers { peer_id, headers } => {
                        let selected_limits = SelectedSegmentLimits::default();
                        let pending_selected_locator = {
                            let guard = selected_segment_locator_state.lock().await;
                            guard.pending_locator.clone()
                        };
                        let (known_blocks_for_segment, common_ancestor, common_ancestor_height) = {
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
                        let selected_segment_validation = common_ancestor.as_ref().map(|common| {
                            validate_selected_header_segment(
                                common,
                                &headers,
                                &known_blocks_for_segment,
                            )
                        });
'''
replace_once(old, new, 'locator-aware common ancestor')

old = '''                                    let selected_locator_needed = selected_locator_peer.is_some();
                                    let selected_locator_request_id = {
                                        let guard = selected_segment_locator_state.lock().await;
                                        guard.next_request_id
                                    };
                                    let selected_locator_requested = selected_locator_peer
                                        .is_some()
                                        && p2p
                                            .request_headers_from(
'''
new = '''                                    let selected_locator_needed = selected_locator_peer.is_some();
                                    let active_session =
                                        runtime.read().await.active_session_id.is_some();
                                    let selected_locator_already_active = {
                                        let guard = selected_segment_locator_state.lock().await;
                                        selected_segment_recovery_has_priority(
                                            active_session,
                                            guard
                                                .pending_locator
                                                .as_ref()
                                                .map(|pending| pending.requested_at_unix),
                                            now,
                                        )
                                    };
                                    let selected_locator_request_id = {
                                        let guard = selected_segment_locator_state.lock().await;
                                        guard.next_request_id
                                    };
                                    let selected_locator_requested = selected_locator_peer
                                        .is_some()
                                        && !selected_locator_already_active
                                        && p2p
                                            .request_headers_from(
'''
replace_once(old, new, 'final quiescence locator refresh guard')

# Add focused regressions immediately before the existing stale session watchdog test.
needle = '''    #[test]
    fn stale_selected_segment_rearms_only_after_no_progress_and_no_inflight_requests() {
'''
tests = r'''    #[test]
    fn selected_segment_common_ancestor_prefers_pending_locator_parent() {
        let pending = PendingSelectedLocator {
            request_id: 9,
            peer_id: "peer-a".to_string(),
            locator: vec!["selected-1298".to_string()],
            requested_at_unix: 100,
        };
        let mut header = selected_test_header("selected-1299", "merge-known", 1299);
        header.header.parents.push("selected-1298".to_string());
        let known = HashSet::from([
            "merge-known".to_string(),
            "selected-1298".to_string(),
        ]);
        assert_eq!(
            selected_common_ancestor_for_headers(&[header], &known, Some(&pending)).as_deref(),
            Some("selected-1298")
        );
    }

    #[test]
    fn selected_segment_fetches_unknown_merge_parent_before_selected_child() {
        let mut header = selected_test_header("selected-1299", "selected-1298", 1299);
        header.header.parents.push("merge-parent".to_string());
        let headers = vec![header];
        let known = HashSet::from(["selected-1298".to_string()]);
        assert_eq!(
            validate_selected_header_segment("selected-1298", &headers, &known),
            Ok(())
        );
        let limits = SelectedSegmentLimits {
            headers_per_chunk: 128,
            max_inflight_blocks_per_peer: 128,
            max_segment_bytes: 4 * 1024 * 1024,
        };
        assert_eq!(
            selected_segment_request_candidates(&headers, limits, &known, &HashSet::new()),
            vec!["merge-parent".to_string(), "selected-1299".to_string()]
        );
    }

    #[test]
    fn selected_segment_duplicate_header_page_does_not_refresh_progress_clock() {
        let headers = vec![selected_test_header("b1", "common", 1)];
        let locator = vec!["common".to_string()];
        let pending = PendingSelectedLocator {
            request_id: 1,
            peer_id: "peer-a".to_string(),
            locator: locator.clone(),
            requested_at_unix: 100,
        };
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
        assert!(!session.accept_header_page(&pending, &headers, 200));
        assert_eq!(session.updated_at_unix, 100);
        let next = vec![selected_test_header("b2", "b1", 2)];
        assert!(session.accept_header_page(&pending, &next, 201));
        assert_eq!(session.updated_at_unix, 201);
    }

    #[test]
    fn selected_segment_stale_pending_locator_rearms_without_session() {
        let pending = PendingSelectedLocator {
            request_id: 11,
            peer_id: "peer-a".to_string(),
            locator: vec!["local".to_string()],
            requested_at_unix: 100,
        };
        assert!(!pending_selected_locator_should_rearm(
            Some(&pending),
            false,
            0,
            129
        ));
        assert!(pending_selected_locator_should_rearm(
            Some(&pending),
            false,
            0,
            130
        ));
        assert!(!pending_selected_locator_should_rearm(
            Some(&pending),
            true,
            0,
            200
        ));
        assert!(!pending_selected_locator_should_rearm(
            Some(&pending),
            false,
            1,
            200
        ));
    }

'''+needle
replace_once(needle, tests, 'focused tests')

# Extend invalid validation coverage for a disconnected later selected header.
old = '''        assert_eq!(
            validate_selected_header_segment("common", &dup, &known),
            Err("duplicate_hash")
        );
    }
'''
new = '''        assert_eq!(
            validate_selected_header_segment("common", &dup, &known),
            Err("duplicate_hash")
        );
        let disconnected = vec![
            selected_test_header("b1", "common", 514),
            selected_test_header("b2", "unknown", 515),
        ];
        assert_eq!(
            validate_selected_header_segment("common", &disconnected, &known),
            Err("no_known_or_staged_parent")
        );
    }
'''
replace_once(old, new, 'disconnected validation test')

path.write_text(s)
print('patched apps/pulsedagd/src/main.rs')
