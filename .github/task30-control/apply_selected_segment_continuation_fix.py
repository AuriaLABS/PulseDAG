#!/usr/bin/env python3
"""Apply the narrowly-scoped Task30 #995 selected-segment continuation fix.

This is temporary control-plane scaffolding.  It must delete itself before the
product commit is pushed so the final PR tree contains only the source fix.
"""
from __future__ import annotations

import sys
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one patch target, found {count}")
    return text.replace(old, new, 1)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: apply_selected_segment_continuation_fix.py PATH", file=sys.stderr)
        return 64

    path = Path(sys.argv[1])
    text = path.read_text()

    old = '''    fn complete_current_chunk_if_applied(&mut self) -> bool {
        if self.current_chunk.is_empty()
            || !self
                .current_chunk
                .iter()
                .all(|hash| self.accepted_applied_hashes.contains(hash))
        {
            return false;
        }
        self.current_chunk.clear();
        true
    }
'''
    new = old + '''
    fn continuation_hashes(&self, limit: usize) -> Vec<String> {
        self.missing_hashes
            .iter()
            .filter(|hash| {
                !self.requested_hashes.contains(*hash)
                    && !self.accepted_applied_hashes.contains(*hash)
            })
            .take(limit.max(1))
            .cloned()
            .collect()
    }
'''
    text = replace_once(text, old, new, "selected-segment continuation helper")

    old = '''                            let mut selected_segment_completed = false;
                            {
'''
    new = '''                            let mut selected_segment_completed = false;
                            let mut selected_segment_continuation = None;
                            {
'''
    text = replace_once(text, old, new, "selected-segment continuation capture")

    old = '''                                        if session.complete_current_chunk_if_applied() {
                                            rt.selected_segment_chunks_completed_total = rt
                                                .selected_segment_chunks_completed_total
                                                .saturating_add(1);
                                        }
'''
    new = '''                                        if session.complete_current_chunk_if_applied() {
                                            rt.selected_segment_chunks_completed_total = rt
                                                .selected_segment_chunks_completed_total
                                                .saturating_add(1);
                                            let continuation = session
                                                .continuation_hashes(MAX_INFLIGHT_BLOCK_REQUESTS);
                                            if !continuation.is_empty() {
                                                selected_segment_continuation = Some((
                                                    session.session_id,
                                                    session.peer_id.clone(),
                                                    continuation,
                                                ));
                                            }
                                        }
'''
    text = replace_once(text, old, new, "selected-segment continuation scheduling trigger")

    marker = '''                            if selected_segment_completed {
                                selected_segment_session = None;
'''
    insertion = '''                            if !selected_segment_completed {
                                if let Some((session_id, peer_id, candidates)) =
                                    selected_segment_continuation.take()
                                {
                                    let mut issued_hashes = Vec::new();
                                    for hash in candidates {
                                        if !block_requests.should_issue_getblock_for_peers(
                                            &hash,
                                            now_unix(),
                                            active_peer_ids(&p2p),
                                        ) {
                                            continue;
                                        }
                                        let request_succeeded = if let Some(ref p2p_handle) = p2p {
                                            match p2p_handle.request_block_from(&peer_id, &hash) {
                                                Ok(()) => true,
                                                Err(e) => {
                                                    block_requests.resolve(&hash);
                                                    warn!(
                                                        error = %e,
                                                        block_hash = %hash,
                                                        session_id,
                                                        peer = %peer_id,
                                                        "failed issuing selected-segment continuation GetBlock request"
                                                    );
                                                    false
                                                }
                                            }
                                        } else {
                                            block_requests.resolve(&hash);
                                            false
                                        };
                                        if request_succeeded {
                                            issued_hashes.push(hash);
                                        }
                                    }

                                    if !issued_hashes.is_empty() {
                                        let issued_at = now_unix();
                                        let mut chunk_started = false;
                                        if let Some(session) =
                                            selected_segment_session.as_mut().filter(|session| {
                                                session.session_id == session_id
                                                    && session.peer_id == peer_id
                                            })
                                        {
                                            for hash in &issued_hashes {
                                                session.requested_hashes.insert(hash.clone());
                                            }
                                            chunk_started = session
                                                .start_chunk(issued_hashes.clone(), issued_at);
                                        }

                                        if chunk_started {
                                            let issued_count = issued_hashes.len() as u64;
                                            let mut rt = runtime.write().await;
                                            rt.getblock_sent =
                                                rt.getblock_sent.saturating_add(issued_count);
                                            rt.peer_addressed_getblock_sent_total = rt
                                                .peer_addressed_getblock_sent_total
                                                .saturating_add(issued_count);
                                            rt.selected_segment_block_requests_total = rt
                                                .selected_segment_block_requests_total
                                                .saturating_add(issued_count);
                                            rt.active_session_requested_blocks = rt
                                                .active_session_requested_blocks
                                                .saturating_add(issued_count);
                                            rt.final_quiescence_missing_segment_request_total = rt
                                                .final_quiescence_missing_segment_request_total
                                                .saturating_add(issued_count);
                                            rt.pending_block_requests =
                                                block_requests.pending.len();
                                            rt.inflight_block_requests =
                                                block_requests.pending.len();
                                            rt.pending_block_request_hashes =
                                                block_requests.pending_hashes();
                                            rt.sync_state = DagSyncStage::RequestingSelectedBlocks
                                                .as_str()
                                                .to_string();
                                            info!(
                                                event = "selected_segment_chunk_continued",
                                                session_id,
                                                peer = %peer_id,
                                                issued_count,
                                                "scheduled next peer-addressed selected-segment chunk from already-correlated headers"
                                            );
                                        } else {
                                            for hash in &issued_hashes {
                                                block_requests.resolve(hash);
                                            }
                                            warn!(
                                                session_id,
                                                peer = %peer_id,
                                                issued_count = issued_hashes.len(),
                                                "selected-segment continuation could not start a new chunk; rolled back request tracking"
                                            );
                                        }
                                    }
                                }
                            }

'''
    text = replace_once(
        text,
        marker,
        insertion + marker,
        "selected-segment continuation request emission",
    )

    marker = '''    #[test]
    fn lag_injection_evidence_requires_correlated_selected_segment_recovery() {
'''
    test = '''    #[test]
    fn selected_segment_single_header_page_continues_after_global_64_request_cap() {
        let mut headers = Vec::new();
        let mut parent = "common".to_string();
        for height in 1..=80 {
            let hash = format!("selected-{height:03}");
            headers.push(selected_test_header(&hash, &parent, height));
            parent = hash;
        }
        let locator = vec!["common".to_string()];
        let mut session = SelectedSegmentSession::new(
            10,
            "peer-a".to_string(),
            "common".to_string(),
            0,
            &headers,
            &locator,
            19,
            1_000,
        )
        .expect("session");
        let limits = SelectedSegmentLimits {
            headers_per_chunk: 128,
            max_inflight_blocks_per_peer: 128,
            max_segment_bytes: 4 * 1024 * 1024,
        };
        let candidates = selected_segment_request_candidates(
            &headers,
            limits,
            &HashSet::from(["common".to_string()]),
            &HashSet::new(),
        );
        assert_eq!(candidates.len(), 80);
        session.missing_hashes = candidates.clone();

        let first = candidates[..MAX_INFLIGHT_BLOCK_REQUESTS].to_vec();
        assert_eq!(first.len(), 64);
        for hash in &first {
            session.requested_hashes.insert(hash.clone());
        }
        assert!(session.start_chunk(first.clone(), 1_001));
        for hash in &first {
            assert!(session.mark_applied(hash, 2_000));
        }
        assert!(session.complete_current_chunk_if_applied());

        let continuation = session.continuation_hashes(MAX_INFLIGHT_BLOCK_REQUESTS);
        assert_eq!(continuation.len(), 16);
        assert_eq!(continuation.first(), Some(&"selected-065".to_string()));
        assert_eq!(continuation.last(), Some(&"selected-080".to_string()));
        for hash in &continuation {
            session.requested_hashes.insert(hash.clone());
        }
        assert!(session.start_chunk(continuation.clone(), 2_001));
        for hash in &continuation {
            assert!(session.mark_applied(hash, 3_000));
        }
        assert!(session.complete_current_chunk_if_applied());
        assert!(session
            .continuation_hashes(MAX_INFLIGHT_BLOCK_REQUESTS)
            .is_empty());
        assert_eq!(session.accepted_applied_hashes.len(), 80);
        assert_eq!(session.remote_selected_tip, "selected-080");
        assert_eq!(session.remote_selected_height, 80);
    }

'''
    text = replace_once(
        text,
        marker,
        test + marker,
        "selected-segment single-page continuation regression",
    )

    path.write_text(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
