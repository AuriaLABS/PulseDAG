from pathlib import Path

main_path = Path("apps/pulsedagd/src/main.rs")
main = main_path.read_text()
tracker_path = Path("apps/pulsedagd/src/block_request.rs")
tracker = tracker_path.read_text()


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


main = replace_once(
    main,
    """                        let request_capacity = block_requests
                            .pending_capacity_remaining()
                            .min(DAG_FRONTIER_FETCH_BATCH);""",
    """                        let frontier_peer_inflight = block_requests
                            .inflight_by_peer()
                            .get(&frontier_peer)
                            .copied()
                            .unwrap_or_default();
                        let frontier_peer_capacity = block_requests
                            .max_pending_per_peer()
                            .saturating_sub(frontier_peer_inflight);
                        let request_capacity = block_requests
                            .pending_capacity_remaining()
                            .min(frontier_peer_capacity)
                            .min(DAG_FRONTIER_FETCH_BATCH);""",
    "frontier per-peer capacity",
)

main = replace_once(
    main,
    """                                if !block_requests.should_issue_getblock_for_peers(
                                    &hash,
                                    now_unix(),
                                    [frontier_peer.clone()],
                                ) {""",
    """                                if !block_requests.should_issue_getblock_from_peer(
                                    &hash,
                                    now_unix(),
                                    &frontier_peer,
                                ) {""",
    "strict frontier peer reservation",
)

tracker = replace_once(
    tracker,
    """    pub fn should_issue_getblock(&mut self, hash: &str, now_unix: u64) -> bool {
        self.should_issue_getblock_for_peers(hash, now_unix, std::iter::empty::<String>())
    }

    pub fn should_issue_getblock_for_peers<I, S>(""",
    """    pub fn should_issue_getblock(&mut self, hash: &str, now_unix: u64) -> bool {
        self.should_issue_getblock_for_peers(hash, now_unix, std::iter::empty::<String>())
    }

    pub fn should_issue_getblock_from_peer(
        &mut self,
        hash: &str,
        now_unix: u64,
        peer: &str,
    ) -> bool {
        if self.exhausted_hashes.contains(hash) {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }
        if self.is_backing_off(hash, now_unix) {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }
        if self.pending.contains_key(hash) || self.pending.len() >= self.max_pending {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }
        if self
            .not_found_by_hash
            .get(hash)
            .is_some_and(|failed| failed.contains(peer))
            || self
                .timed_out_by_hash
                .get(hash)
                .is_some_and(|failed| failed.contains(peer))
        {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }
        let peer_inflight = self
            .pending
            .values()
            .filter(|request| request.peer.as_deref() == Some(peer))
            .count();
        if peer_inflight >= self.max_pending_per_peer {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }

        self.record_missing_parent_request(hash, Some(peer), now_unix);
        self.pending.insert(
            hash.to_string(),
            PendingBlockRequest {
                first_requested_at_unix: now_unix,
                last_requested_at_unix: now_unix,
                retry_count: 0,
                peer: Some(peer.to_string()),
            },
        );
        self.backoff_by_hash.remove(hash);
        self.fetch_queued = self.fetch_queued.saturating_add(1);
        true
    }

    pub fn should_issue_getblock_for_peers<I, S>(""",
    "strict tracker method",
)

tracker = replace_once(
    tracker,
    """    #[test]
    fn dedupes_request_within_timeout() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        assert!(tracker.should_issue_getblock("h1", 100));
        assert!(!tracker.should_issue_getblock("h1", 101));
    }

    #[test]
    fn retries_after_timeout_until_limit() {""",
    """    #[test]
    fn dedupes_request_within_timeout() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        assert!(tracker.should_issue_getblock("h1", 100));
        assert!(!tracker.should_issue_getblock("h1", 101));
    }

    #[test]
    fn strict_peer_request_respects_per_peer_capacity_without_unassigned_fallback() {
        let mut tracker = BlockRequestTracker::with_limits(10, 2, 4, 1);
        assert!(tracker.should_issue_getblock_from_peer("h1", 100, "peer-a"));
        assert!(!tracker.should_issue_getblock_from_peer("h2", 100, "peer-a"));
        assert_eq!(tracker.pending.len(), 1);
        assert_eq!(
            tracker.pending.get("h1").and_then(|request| request.peer.as_deref()),
            Some("peer-a")
        );
        assert!(!tracker.pending.contains_key("h2"));
    }

    #[test]
    fn retries_after_timeout_until_limit() {""",
    "strict tracker regression",
)

main_path.write_text(main)
tracker_path.write_text(tracker)
