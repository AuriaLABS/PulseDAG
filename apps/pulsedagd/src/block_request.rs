use std::collections::{BTreeMap, HashMap, HashSet};

use pulsedag_p2p::messages::BlockHeaderAnnouncement;

#[derive(Debug, Clone)]
pub struct PendingBlockRequest {
    pub last_requested_at_unix: u64,
    pub retry_count: u8,
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
            let missing_parents = header
                .header
                .parents
                .iter()
                .filter(|parent| !known_blocks.contains(*parent))
                .filter(|parent| !self.pending.contains_key(*parent))
                .filter(|parent| self.known_headers.contains_key(*parent))
                .cloned()
                .collect::<Vec<_>>();
            if let Some(parent) = missing_parents.first() {
                self.deferred_by_missing_parent
                    .entry(parent.clone())
                    .or_default()
                    .insert(header.hash.clone());
                schedule.deferred.push(header.hash.clone());
                continue;
            }
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
            let Some(header) = self.known_headers.get(&child) else {
                continue;
            };
            let missing_parent = header
                .header
                .parents
                .iter()
                .any(|parent| !known_blocks.contains(parent));
            if missing_parent {
                schedule.deferred.push(child);
            } else if self.should_issue_getblock(&child, now_unix) {
                schedule.ready.push(child);
            } else {
                schedule.duplicate_suppressed = schedule.duplicate_suppressed.saturating_add(1);
            }
        }
        schedule
    }

    pub fn resolve(&mut self, hash: &str) {
        self.pending.remove(hash);
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
}

#[cfg(test)]
mod tests {
    use super::BlockRequestTracker;
    use pulsedag_core::types::BlockHeader;
    use pulsedag_p2p::messages::BlockHeaderAnnouncement;
    use std::collections::HashSet;

    fn header(hash: &str, parents: &[&str], height: u64) -> BlockHeaderAnnouncement {
        BlockHeaderAnnouncement {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: parents.iter().map(|parent| parent.to_string()).collect(),
                timestamp: height,
                difficulty: 1,
                nonce: 1,
                merkle_root: format!("mr-{hash}"),
                state_root: format!("sr-{hash}"),
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
    fn schedules_parents_before_children_and_suppresses_inflight_duplicates() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        let known = HashSet::from(["genesis".to_string()]);
        let headers = vec![
            header("child", &["parent"], 2),
            header("parent", &["genesis"], 1),
        ];

        let schedule = tracker.schedule_header_fetches(&headers, &known, 100);
        assert_eq!(schedule.ready, vec!["parent".to_string()]);
        assert_eq!(schedule.deferred, vec!["child".to_string()]);

        let duplicate = tracker.schedule_header_fetches(&headers[1..], &known, 101);
        assert_eq!(duplicate.duplicate_suppressed, 1);
    }

    #[test]
    fn unblocks_children_after_parent_resolves() {
        let mut tracker = BlockRequestTracker::new(10, 2);
        let known = HashSet::from(["genesis".to_string()]);
        let headers = vec![
            header("child", &["parent"], 2),
            header("parent", &["genesis"], 1),
        ];
        let _ = tracker.schedule_header_fetches(&headers, &known, 100);

        let known = HashSet::from(["genesis".to_string(), "parent".to_string()]);
        tracker.resolve("parent");
        let schedule = tracker.unblock_after_resolve("parent", &known, 110);
        assert_eq!(schedule.ready, vec!["child".to_string()]);
    }
}
