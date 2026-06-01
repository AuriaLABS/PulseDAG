use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use pulsedag_p2p::messages::BlockHeaderAnnouncement;

#[derive(Debug, Clone)]
pub struct PendingBlockRequest {
    pub last_requested_at_unix: u64,
    pub retry_count: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockRequestTimeouts {
    pub retryable: Vec<String>,
    pub expired: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchSchedule {
    pub ready: Vec<String>,
    pub deferred: Vec<String>,
    pub duplicate_suppressed: usize,
}

#[derive(Debug, Clone)]
pub struct BlockRequestTracker {
    pub pending: HashMap<String, PendingBlockRequest>,
    pub timeout_secs: u64,
    pub retry_limit: u8,
    known_headers: BTreeMap<String, BlockHeaderAnnouncement>,
    deferred_by_missing_parent: HashMap<String, HashSet<String>>,
}

impl BlockRequestTracker {
    pub fn new(timeout_secs: u64, retry_limit: u8) -> Self {
        Self {
            pending: HashMap::new(),
            timeout_secs,
            retry_limit,
            known_headers: BTreeMap::new(),
            deferred_by_missing_parent: HashMap::new(),
        }
    }

    pub fn should_issue_getblock(&mut self, hash: &str, now_unix: u64) -> bool {
        match self.pending.get_mut(hash) {
            Some(req)
                if now_unix.saturating_sub(req.last_requested_at_unix) < self.timeout_secs =>
            {
                false
            }
            Some(req) => {
                if req.retry_count >= self.retry_limit {
                    return false;
                }
                req.retry_count = req.retry_count.saturating_add(1);
                req.last_requested_at_unix = now_unix;
                true
            }
            None => {
                self.pending.insert(
                    hash.to_string(),
                    PendingBlockRequest {
                        last_requested_at_unix: now_unix,
                        retry_count: 0,
                    },
                );
                true
            }
        }
    }

    pub fn note_headers(&mut self, headers: &[BlockHeaderAnnouncement]) {
        for header in headers {
            self.known_headers
                .insert(header.hash.clone(), header.clone());
        }
    }

    fn missing_parents(
        header: &BlockHeaderAnnouncement,
        known_blocks: &HashSet<String>,
    ) -> Vec<String> {
        header
            .header
            .parents
            .iter()
            .filter(|parent| !known_blocks.contains(*parent))
            .cloned()
            .collect()
    }

    fn remove_deferred_child(&mut self, child: &str) {
        self.deferred_by_missing_parent.retain(|_, children| {
            children.remove(child);
            !children.is_empty()
        });
    }

    fn track_deferred_child(&mut self, child: &str, missing_parents: &[String]) {
        self.remove_deferred_child(child);
        for parent in missing_parents {
            self.deferred_by_missing_parent
                .entry(parent.clone())
                .or_default()
                .insert(child.to_string());
        }
    }

    pub fn schedule_header_fetches(
        &mut self,
        headers: &[BlockHeaderAnnouncement],
        known_blocks: &HashSet<String>,
        now_unix: u64,
    ) -> FetchSchedule {
        self.note_headers(headers);
        let mut schedule = FetchSchedule::default();
        for header in headers {
            if known_blocks.contains(&header.hash) {
                schedule.duplicate_suppressed = schedule.duplicate_suppressed.saturating_add(1);
                continue;
            }
            let missing_parents = Self::missing_parents(header, known_blocks);
            if !missing_parents.is_empty() {
                self.track_deferred_child(&header.hash, &missing_parents);
                schedule.deferred.push(header.hash.clone());
                for parent in missing_parents {
                    if self.known_headers.contains_key(&parent) {
                        continue;
                    }
                    if self.should_issue_getblock(&parent, now_unix) {
                        schedule.ready.push(parent);
                    } else {
                        schedule.duplicate_suppressed =
                            schedule.duplicate_suppressed.saturating_add(1);
                    }
                }
                continue;
            }
            self.remove_deferred_child(&header.hash);
            if self.should_issue_getblock(&header.hash, now_unix) {
                schedule.ready.push(header.hash.clone());
            } else {
                schedule.duplicate_suppressed = schedule.duplicate_suppressed.saturating_add(1);
            }
        }
        schedule
    }

    pub fn unblock_after_resolve(
        &mut self,
        hash: &str,
        known_blocks: &HashSet<String>,
        now_unix: u64,
    ) -> FetchSchedule {
        let mut schedule = FetchSchedule::default();
        let children = self
            .deferred_by_missing_parent
            .remove(hash)
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>();
        for child in children {
            let Some(header) = self.known_headers.get(&child).cloned() else {
                continue;
            };
            let missing_parents = Self::missing_parents(&header, known_blocks);
            if !missing_parents.is_empty() {
                self.track_deferred_child(&child, &missing_parents);
                schedule.deferred.push(child);
            } else if self.should_issue_getblock(&child, now_unix) {
                self.remove_deferred_child(&child);
                schedule.ready.push(child);
            } else {
                self.remove_deferred_child(&child);
                schedule.duplicate_suppressed = schedule.duplicate_suppressed.saturating_add(1);
            }
        }
        schedule
    }

    pub fn resolve(&mut self, hash: &str) {
        self.pending.remove(hash);
    }

    pub fn pending_hashes(&self) -> Vec<String> {
        let mut hashes = self.pending.keys().cloned().collect::<Vec<_>>();
        hashes.sort();
        hashes
    }

    pub fn collect_timeouts(&self, now_unix: u64) -> Vec<String> {
        self.pending
            .iter()
            .filter(|(_, req)| {
                now_unix.saturating_sub(req.last_requested_at_unix) >= self.timeout_secs
            })
            .map(|(hash, _)| hash.clone())
            .collect()
    }

    pub fn drain_timeouts(&mut self, now_unix: u64) -> BlockRequestTimeouts {
        let timed_out = self.collect_timeouts(now_unix);
        let mut result = BlockRequestTimeouts::default();
        for hash in timed_out {
            let Some(req) = self.pending.get_mut(&hash) else {
                continue;
            };
            if req.retry_count >= self.retry_limit {
                self.pending.remove(&hash);
                result.expired.push(hash);
            } else {
                req.retry_count = req.retry_count.saturating_add(1);
                req.last_requested_at_unix = now_unix;
                result.retryable.push(hash);
            }
        }
        result.retryable.sort();
        result.expired.sort();
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderFetchCandidate {
    pub hash: String,
    pub parents: Vec<String>,
    pub height: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DependencyFetchPlan {
    pub requests: Vec<String>,
    pub deferred: Vec<String>,
    pub parent_first_requests: usize,
}

#[derive(Debug, Clone, Default)]
pub struct DependencyAwareFetchScheduler {
    candidates: HashMap<String, HeaderFetchCandidate>,
    inventory: VecDeque<String>,
    queued: HashSet<String>,
}

impl DependencyAwareFetchScheduler {
    pub fn queue_inventory<I, S>(&mut self, hashes: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut added = 0usize;
        for hash in hashes {
            let hash = hash.into();
            if self.queued.insert(hash.clone()) {
                self.inventory.push_back(hash);
                added = added.saturating_add(1);
            }
        }
        added
    }

    pub fn queue_headers<I>(&mut self, headers: I) -> usize
    where
        I: IntoIterator<Item = HeaderFetchCandidate>,
    {
        let mut added = 0usize;
        let mut headers = headers.into_iter().collect::<Vec<_>>();
        headers.sort_by(|a, b| a.height.cmp(&b.height).then_with(|| a.hash.cmp(&b.hash)));
        for candidate in headers {
            let hash = candidate.hash.clone();
            if self.queued.insert(hash.clone()) {
                self.inventory.push_back(hash.clone());
                added = added.saturating_add(1);
            }
            self.candidates.insert(hash, candidate);
        }
        added
    }

    pub fn next_requests(
        &mut self,
        known_blocks: &HashSet<String>,
        pending_blocks: &HashSet<String>,
        max: usize,
    ) -> DependencyFetchPlan {
        let mut plan = DependencyFetchPlan::default();
        if max == 0 {
            return plan;
        }
        let mut deferred = VecDeque::new();
        while let Some(hash) = self.inventory.pop_front() {
            if plan.requests.len() >= max {
                deferred.push_back(hash);
                continue;
            }
            if known_blocks.contains(&hash) || pending_blocks.contains(&hash) {
                self.queued.remove(&hash);
                continue;
            }
            let missing_parents = self
                .candidates
                .get(&hash)
                .map(|candidate| {
                    candidate
                        .parents
                        .iter()
                        .filter(|parent| {
                            !known_blocks.contains(*parent) && !pending_blocks.contains(*parent)
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if missing_parents.is_empty() {
                self.queued.remove(&hash);
                plan.requests.push(hash);
                continue;
            }
            for parent in missing_parents {
                if plan.requests.len() >= max {
                    break;
                }
                if self.queued.insert(parent.clone()) {
                    plan.parent_first_requests = plan.parent_first_requests.saturating_add(1);
                    plan.requests.push(parent);
                }
            }
            plan.deferred.push(hash.clone());
            deferred.push_back(hash);
        }
        self.inventory = deferred;
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockRequestTracker, DependencyAwareFetchScheduler, HeaderFetchCandidate};
    use pulsedag_core::types::BlockHeader;
    use pulsedag_p2p::messages::BlockHeaderAnnouncement;
    use std::collections::HashSet;

    fn header(hash: &str, parents: &[&str], height: u64) -> BlockHeaderAnnouncement {
        BlockHeaderAnnouncement {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: parents.iter().map(|parent| parent.to_string()).collect(),
                timestamp: 0,
                difficulty: 1,
                nonce: 0,
                merkle_root: "merkle".into(),
                state_root: "state".into(),
                blue_score: height,
                height,
            },
        }
    }

    #[test]
    fn dedupes_request_within_timeout() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        assert!(tracker.should_issue_getblock("h1", 100));
        assert!(!tracker.should_issue_getblock("h1", 101));
    }

    #[test]
    fn retries_after_timeout_until_limit() {
        let mut tracker = BlockRequestTracker::new(5, 2);
        assert!(tracker.should_issue_getblock("h1", 10));
        assert!(tracker.should_issue_getblock("h1", 20));
        assert!(tracker.should_issue_getblock("h1", 30));
        assert!(!tracker.should_issue_getblock("h1", 40));
    }

    #[test]
    fn drains_timeouts_with_retry_then_expiry() {
        let mut tracker = BlockRequestTracker::new(5, 1);
        assert!(tracker.should_issue_getblock("h1", 10));

        let first = tracker.drain_timeouts(16);
        assert_eq!(first.retryable, vec!["h1"]);
        assert!(first.expired.is_empty());
        assert_eq!(tracker.pending_hashes(), vec!["h1"]);

        let second = tracker.drain_timeouts(22);
        assert!(second.retryable.is_empty());
        assert_eq!(second.expired, vec!["h1"]);
        assert!(tracker.pending_hashes().is_empty());
    }

    #[test]
    fn defers_child_while_parent_request_is_in_flight() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        let known = HashSet::new();
        let schedule = tracker.schedule_header_fetches(
            &[header("parent", &[], 1), header("child", &["parent"], 2)],
            &known,
            100,
        );

        assert_eq!(schedule.ready, vec!["parent"]);
        assert_eq!(schedule.deferred, vec!["child"]);
        assert_eq!(tracker.pending_hashes(), vec!["parent"]);
    }

    #[test]
    fn requests_unknown_missing_parent_before_child() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        let known = HashSet::new();
        let schedule =
            tracker.schedule_header_fetches(&[header("child", &["parent"], 2)], &known, 100);

        assert_eq!(schedule.ready, vec!["parent"]);
        assert_eq!(schedule.deferred, vec!["child"]);
        assert_eq!(tracker.pending_hashes(), vec!["parent"]);
    }

    #[test]
    fn re_tracks_deferred_child_until_all_missing_parents_resolve() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        let known = HashSet::new();
        let schedule = tracker.schedule_header_fetches(
            &[header("child", &["parent-a", "parent-b"], 2)],
            &known,
            100,
        );
        assert_eq!(schedule.ready, vec!["parent-a", "parent-b"]);
        assert_eq!(schedule.deferred, vec!["child"]);

        tracker.resolve("parent-a");
        let known = HashSet::from(["parent-a".to_string()]);
        let still_deferred = tracker.unblock_after_resolve("parent-a", &known, 110);
        assert!(still_deferred.ready.is_empty());
        assert_eq!(still_deferred.deferred, vec!["child"]);

        tracker.resolve("parent-b");
        let known = HashSet::from(["parent-a".to_string(), "parent-b".to_string()]);
        let unblocked = tracker.unblock_after_resolve("parent-b", &known, 120);
        assert_eq!(unblocked.ready, vec!["child"]);
        assert!(unblocked.deferred.is_empty());
    }

    #[test]
    fn schedules_parents_before_children_from_headers() {
        let mut scheduler = DependencyAwareFetchScheduler::default();
        scheduler.queue_headers([HeaderFetchCandidate {
            hash: "child".into(),
            parents: vec!["parent".into()],
            height: 2,
        }]);
        let known = HashSet::new();
        let pending = HashSet::new();
        let plan = scheduler.next_requests(&known, &pending, 4);
        assert_eq!(plan.requests, vec!["parent"]);
        assert_eq!(plan.deferred, vec!["child"]);
        assert_eq!(plan.parent_first_requests, 1);
    }

    #[test]
    fn requests_child_after_parent_is_known() {
        let mut scheduler = DependencyAwareFetchScheduler::default();
        scheduler.queue_headers([HeaderFetchCandidate {
            hash: "child".into(),
            parents: vec!["parent".into()],
            height: 2,
        }]);
        let known = HashSet::from(["parent".to_string()]);
        let pending = HashSet::new();
        let plan = scheduler.next_requests(&known, &pending, 4);
        assert_eq!(plan.requests, vec!["child"]);
        assert!(plan.deferred.is_empty());
    }
}
