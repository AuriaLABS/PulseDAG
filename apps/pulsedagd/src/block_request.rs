use std::collections::HashMap;

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

#[cfg(test)]
mod tests {
    use super::BlockRequestTracker;

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
}
