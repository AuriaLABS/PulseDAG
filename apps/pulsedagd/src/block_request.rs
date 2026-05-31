use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct PendingBlockRequest {
    pub last_requested_at_unix: u64,
    pub retry_count: u8,
}

#[derive(Debug, Clone)]
pub struct BlockRequestTracker {
    pub pending: HashMap<String, PendingBlockRequest>,
    pub timeout_secs: u64,
    pub retry_limit: u8,
}

impl BlockRequestTracker {
    pub fn new(timeout_secs: u64, retry_limit: u8) -> Self {
        Self {
            pending: HashMap::new(),
            timeout_secs,
            retry_limit,
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
    use std::collections::HashSet;

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
