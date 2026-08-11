from pathlib import Path

path = Path("apps/pulsedagd/src/main.rs")
text = path.read_text(encoding="utf-8")


def replace_once(old: str, new: str, label: str) -> None:
    global text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly 1 match, found {count}")
    text = text.replace(old, new, 1)


replace_once(
'''    if chain.dag.selected_chain.last() != Some(&selected_tip) {
        return true;
    }
''',
'''    if chain.dag.selected_chain.last() != Some(&selected_tip) {
        return true;
    }
    if chain.dag.blocks.contains_key(&chain.dag.genesis_hash)
        && chain.dag.selected_chain.first() != Some(&chain.dag.genesis_hash)
    {
        return true;
    }
''',
"detect truncated selected-chain prefix",
)

replace_once(
'''fn pending_selected_locator_accepts_response_peer(
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
) -> bool {
    let (Some(pending), Some(response_peer)) = (pending, response_peer) else {
        return false;
    };
    pending.peer_id == "broadcast-observed-block-gap" || pending.peer_id == response_peer
}

fn selected_common_ancestor_for_headers(
''',
'''fn pending_selected_locator_accepts_response_peer(
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
) -> bool {
    let (Some(pending), Some(response_peer)) = (pending, response_peer) else {
        return false;
    };
    pending.peer_id == "broadcast-observed-block-gap" || pending.peer_id == response_peer
}

fn pending_selected_locator_accepts_response_common_ancestor(
    pending: Option<&PendingSelectedLocator>,
    response_peer: Option<&str>,
    common_ancestor: Option<&str>,
) -> bool {
    common_ancestor.is_some()
        && pending_selected_locator_accepts_response_peer(pending, response_peer)
}

fn selected_common_ancestor_for_headers(
''',
"add fallback common-ancestor correlation helper",
)

replace_once(
'''            && common_ancestor.is_some_and(|hash| {
                hash == self.common_ancestor.as_str() || self.accepted_applied_hashes.contains(hash)
            })
            && self.can_start_chunk()
''',
'''            && common_ancestor.is_some_and(|hash| {
                hash == self.common_ancestor.as_str()
                    || self.accepted_applied_hashes.contains(hash)
                    || self.expected_header_hashes.iter().any(|known| known == hash)
            })
            && self.can_start_chunk()
''',
"correlate continuation through previously accepted header anchors",
)

replace_once(
'''        if progressed {
            self.updated_at_unix = now;
        }
        progressed
    }

    fn can_start_chunk(&self) -> bool {
''',
'''        if progressed {
            self.updated_at_unix = now;
        }
        progressed
    }

    fn update_remote_target(&mut self, remote_tip: Option<&str>, remote_height: u64) {
        if remote_height <= self.remote_selected_height {
            return;
        }
        self.remote_selected_height = remote_height;
        if let Some(remote_tip) = remote_tip {
            self.remote_selected_tip = remote_tip.to_string();
        }
    }

    fn can_start_chunk(&self) -> bool {
''',
"retain peer-advertised selected target",
)

replace_once(
'''fn selected_segment_continuation_needed(
    session: Option<&SelectedSegmentSession>,
    local_selected_height: u64,
    chunk_completed: bool,
) -> bool {
    chunk_completed
        && session.is_some_and(|session| {
            session.can_start_chunk() && local_selected_height < session.remote_selected_height
        })
}

impl Default for SelectedSegmentLimits {
''',
'''fn selected_segment_continuation_needed(
    session: Option<&SelectedSegmentSession>,
    local_selected_height: u64,
    chunk_completed: bool,
) -> bool {
    chunk_completed
        && session.is_some_and(|session| {
            session.can_start_chunk() && local_selected_height < session.remote_selected_height
        })
}

fn selected_header_discovery_continuation_anchor(
    session: Option<&SelectedSegmentSession>,
    headers: &[HeaderInventory],
    selected_requests: &[String],
) -> Option<String> {
    if !selected_requests.is_empty() {
        return None;
    }
    let session = session?;
    let furthest = headers.iter().max_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.hash.cmp(&b.hash))
    })?;
    (furthest.header.height < session.remote_selected_height).then(|| furthest.hash.clone())
}

impl Default for SelectedSegmentLimits {
''',
"add known-header discovery continuation helper",
)

replace_once(
'''                        let pending_locator = pending_selected_locator
                            .as_ref()
                            .map(|pending| pending.locator.clone())
                            .unwrap_or_default();
                        let session_correlated = selected_segment_session
''',
'''                        let pending_locator = pending_selected_locator
                            .as_ref()
                            .map(|pending| pending.locator.clone())
                            .unwrap_or_default();
                        let remote_selected_target = peer_id.as_deref().and_then(|response_peer| {
                            p2p.as_ref()
                                .and_then(|handle| handle.status().ok())
                                .and_then(|status| {
                                    status
                                        .remote_selected_tip_inventory
                                        .into_iter()
                                        .find(|remote| {
                                            remote.connected && remote.peer_id == response_peer
                                        })
                                        .map(|remote| (remote.selected_tip, remote.selected_height))
                                })
                        });
                        let session_correlated = selected_segment_session
''',
"capture peer-advertised selected target",
)

replace_once(
'''                        let pending_locator_matches_common_ancestor =
                            pending_selected_locator_matches_common_ancestor(
                                pending_selected_locator.as_ref(),
                                common_ancestor.as_deref(),
                            );
                        let pending_locator_accepts_response_peer =
                            pending_selected_locator_accepts_response_peer(
                                pending_selected_locator.as_ref(),
                                peer_id.as_deref(),
                            );
                        let selected_session_owns_headers = selected_headers_own_broadcast_locator(
                            selected_segment_session.is_some(),
                            selected_locator_pending,
                            peer_id.as_deref(),
                            session_correlated,
                        ) && (session_correlated
                            || (pending_locator_matches_common_ancestor
                                && pending_locator_accepts_response_peer));
''',
'''                        let pending_locator_matches_common_ancestor =
                            pending_selected_locator_matches_common_ancestor(
                                pending_selected_locator.as_ref(),
                                common_ancestor.as_deref(),
                            );
                        let pending_locator_accepts_response_peer =
                            pending_selected_locator_accepts_response_peer(
                                pending_selected_locator.as_ref(),
                                peer_id.as_deref(),
                            );
                        let pending_locator_accepts_response_common_ancestor =
                            pending_selected_locator_accepts_response_common_ancestor(
                                pending_selected_locator.as_ref(),
                                peer_id.as_deref(),
                                common_ancestor.as_deref(),
                            );
                        let selected_session_owns_headers = selected_headers_own_broadcast_locator(
                            selected_segment_session.is_some(),
                            selected_locator_pending,
                            peer_id.as_deref(),
                            session_correlated,
                        ) && (session_correlated
                            || pending_locator_accepts_response_common_ancestor);
''',
"accept valid fallback common ancestor from requested peer",
)

replace_once(
'''                            if let Some(session) = selected_segment_session.as_mut() {
                                if !session.can_start_chunk() {
                                    Vec::new()
                                } else {
                                    if let Some(pending) = pending_selected_locator.as_ref() {
                                        session.accept_header_page(pending, &headers, now_unix());
                                    }
''',
'''                            if let Some(session) = selected_segment_session.as_mut() {
                                if let Some((remote_tip, remote_height)) =
                                    remote_selected_target.as_ref()
                                {
                                    session.update_remote_target(
                                        remote_tip.as_deref(),
                                        *remote_height,
                                    );
                                }
                                if !session.can_start_chunk() {
                                    Vec::new()
                                } else {
                                    if let Some(pending) = pending_selected_locator.as_ref() {
                                        session.accept_header_page(pending, &headers, now_unix());
                                    }
''',
"apply peer-advertised selected target to session",
)

replace_once(
'''                        let selected_request_hashes =
                            selected_requests.iter().cloned().collect::<HashSet<_>>();
                        let requests =
''',
'''                        let selected_header_continuation = if selected_session_owns_headers
                            && matches!(selected_segment_validation, Some(Ok(())))
                        {
                            selected_header_discovery_continuation_anchor(
                                selected_segment_session.as_ref(),
                                &headers,
                                &selected_requests,
                            )
                            .and_then(|anchor| {
                                selected_segment_session
                                    .as_ref()
                                    .map(|session| (session.peer_id.clone(), anchor))
                            })
                        } else {
                            None
                        };
                        let mut issued_selected_header_continuation = false;
                        if let (Some((continuation_peer, anchor)), Some(p2p_handle)) =
                            (selected_header_continuation, p2p.as_ref())
                        {
                            let continuation_locator = vec![anchor.clone()];
                            let request_id = {
                                let guard = selected_segment_locator_state.lock().await;
                                guard.next_request_id
                            };
                            if p2p_handle
                                .request_headers_from(
                                    &continuation_peer,
                                    &continuation_locator,
                                    None,
                                    selected_limits.headers_per_chunk,
                                )
                                .is_ok()
                            {
                                let requested_at = now_unix();
                                {
                                    let mut guard = selected_segment_locator_state.lock().await;
                                    guard.next_request_id = guard.next_request_id.saturating_add(1);
                                    guard.pending_locator = Some(PendingSelectedLocator {
                                        request_id,
                                        peer_id: continuation_peer.clone(),
                                        locator: continuation_locator,
                                        requested_at_unix: requested_at,
                                    });
                                }
                                if let Some(session) = selected_segment_session.as_mut() {
                                    session.state = SelectedSegmentSessionState::RequestingHeaders;
                                    session.updated_at_unix = requested_at;
                                }
                                issued_selected_header_continuation = true;
                                info!(
                                    event = "selected_header_discovery_continuation",
                                    peer = %continuation_peer,
                                    anchor = %anchor,
                                    request_id,
                                    "selected header page contained no unknown blocks; continued discovery from accepted page anchor"
                                );
                            }
                        }
                        let selected_request_hashes =
                            selected_requests.iter().cloned().collect::<HashSet<_>>();
                        let requests =
''',
"continue header discovery across already-known pages",
)

replace_once(
'''                        let mut rt = runtime.write().await;
                        let headers_correlated =
                            selected_session_owns_headers && !headers.is_empty();
''',
'''                        let mut rt = runtime.write().await;
                        if issued_selected_header_continuation {
                            rt.selected_segment_header_requests_total = rt
                                .selected_segment_header_requests_total
                                .saturating_add(1);
                            rt.header_requests_sent = rt.header_requests_sent.saturating_add(1);
                            rt.final_quiescence_selected_sync_blocked_reason =
                                Some("selected_header_discovery_continuation".to_string());
                            rt.sync_state =
                                DagSyncStage::RequestingSelectedHeaders.as_str().to_string();
                        }
                        let headers_correlated =
                            selected_session_owns_headers && !headers.is_empty();
''',
"account immediate header discovery continuation",
)

replace_once(
'''                            "requesting_selected_blocks"
                                | "selected_segment_complete"
                                | "selected_segment_failed"
''',
'''                            "requesting_selected_headers"
                                | "requesting_selected_blocks"
                                | "selected_segment_complete"
                                | "selected_segment_failed"
''',
"preserve selected-header continuation sync state",
)

metadata_test_marker = '''    #[test]
    fn startup_selected_chain_metadata_repair_restores_locator_from_empty_snapshot_metadata() {
'''
metadata_test = '''    #[test]
    fn selected_chain_metadata_repair_detects_truncated_selected_prefix() {
        let mut chain = build_test_chain("selected-metadata-truncated-prefix", 10);
        assert_eq!(chain.dag.selected_chain.first(), Some(&chain.dag.genesis_hash));
        assert!(chain.dag.selected_chain.len() > 4);

        let retained_tail = chain.dag.selected_chain[3..].to_vec();
        chain.dag.selected_chain = retained_tail;

        assert!(selected_chain_metadata_needs_repair(&chain));
        assert!(repair_selected_chain_metadata_if_needed(&mut chain));
        assert_eq!(chain.dag.selected_chain.first(), Some(&chain.dag.genesis_hash));
        assert_eq!(
            chain.dag.selected_chain.last(),
            pulsedag_core::preferred_tip_hash(&chain).as_ref()
        );
    }

'''
replace_once(metadata_test_marker, metadata_test + metadata_test_marker, "add truncated metadata regression")

correlation_test_marker = '''    #[test]
    fn unrelated_header_page_cannot_hijack_pending_selected_locator() {
'''
correlation_tests = '''    #[test]
    fn requested_peer_fallback_common_ancestor_can_own_selected_headers() {
        let pending = PendingSelectedLocator {
            request_id: 6,
            peer_id: "peer-a".to_string(),
            locator: vec!["local-611".to_string(), "local-300".to_string()],
            requested_at_unix: 100,
        };
        assert!(!pending_selected_locator_matches_common_ancestor(
            Some(&pending),
            Some("genesis")
        ));
        assert!(pending_selected_locator_accepts_response_common_ancestor(
            Some(&pending),
            Some("peer-a"),
            Some("genesis")
        ));
        assert!(!pending_selected_locator_accepts_response_common_ancestor(
            Some(&pending),
            Some("peer-b"),
            Some("genesis")
        ));
        assert!(!pending_selected_locator_accepts_response_common_ancestor(
            Some(&pending),
            Some("peer-a"),
            None
        ));
    }

    #[test]
    fn selected_segment_known_header_page_continues_toward_advertised_tip() {
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
        session.update_remote_target(Some("remote-700"), 700);

        assert_eq!(
            selected_header_discovery_continuation_anchor(Some(&session), &headers, &[]),
            Some("b3".to_string())
        );
        assert_eq!(session.remote_selected_height, 700);
        assert_eq!(session.remote_selected_tip, "remote-700");

        let pending = PendingSelectedLocator {
            request_id: 2,
            peer_id: "peer-a".to_string(),
            locator: vec!["b3".to_string()],
            requested_at_unix: 101,
        };
        assert!(session.correlates_pending_header_page(
            Some("peer-a"),
            Some(&pending),
            Some("b3")
        ));
        assert!(!session.correlates_pending_header_page(
            Some("peer-b"),
            Some(&pending),
            Some("b3")
        ));
    }

'''
replace_once(correlation_test_marker, correlation_tests + correlation_test_marker, "add correlation regressions")

path.write_text(text, encoding="utf-8")
print("applied #812 selected-header correlation and discovery continuation patch")
