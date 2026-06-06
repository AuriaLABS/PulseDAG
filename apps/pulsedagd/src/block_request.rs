use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use pulsedag_p2p::messages::BlockHeaderAnnouncement;

#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_MAX_PENDING_BLOCK_REQUESTS: usize = 128;
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_MAX_PENDING_BLOCK_REQUESTS_PER_PEER: usize = 16;
pub const DEFAULT_BLOCK_REQUEST_BACKOFF_SECS: u64 = 8;
pub const MAX_BLOCK_REQUEST_BACKOFF_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub struct PendingBlockRequest {
    pub first_requested_at_unix: u64,
    pub last_requested_at_unix: u64,
    pub retry_count: u8,
    pub peer: Option<String>,
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
    pub queued: usize,
    pub dropped: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchBackpressureCounters {
    pub suppressed: u64,
    pub queued: u64,
    pub dropped: u64,
}

#[derive(Debug, Clone)]
struct RequestBackoff {
    retry_after_unix: u64,
    failures: u8,
}

#[derive(Debug, Clone)]
pub struct BlockRequestTracker {
    pub pending: HashMap<String, PendingBlockRequest>,
    pub timeout_secs: u64,
    pub retry_limit: u8,
    pub max_pending: usize,
    pub max_pending_per_peer: usize,
    known_headers: BTreeMap<String, BlockHeaderAnnouncement>,
    deferred_by_missing_parent: HashMap<String, HashSet<String>>,
    backpressure_suppressed: u64,
    fetch_queued: u64,
    fetch_dropped: u64,
    backoff_by_hash: HashMap<String, RequestBackoff>,
    next_peer_index: usize,
}

impl BlockRequestTracker {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(timeout_secs: u64, retry_limit: u8) -> Self {
        Self::with_limit(
            timeout_secs,
            retry_limit,
            DEFAULT_MAX_PENDING_BLOCK_REQUESTS,
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_limit(timeout_secs: u64, retry_limit: u8, max_pending: usize) -> Self {
        Self::with_limits(
            timeout_secs,
            retry_limit,
            max_pending,
            DEFAULT_MAX_PENDING_BLOCK_REQUESTS_PER_PEER,
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_max_pending(timeout_secs: u64, retry_limit: u8, max_pending: usize) -> Self {
        Self::with_limit(timeout_secs, retry_limit, max_pending)
    }

    pub fn with_limits(
        timeout_secs: u64,
        retry_limit: u8,
        max_pending: usize,
        max_pending_per_peer: usize,
    ) -> Self {
        Self {
            pending: HashMap::new(),
            timeout_secs,
            retry_limit,
            max_pending: max_pending.max(1),
            max_pending_per_peer: max_pending_per_peer.max(1),
            known_headers: BTreeMap::new(),
            deferred_by_missing_parent: HashMap::new(),
            backpressure_suppressed: 0,
            fetch_queued: 0,
            fetch_dropped: 0,
            backoff_by_hash: HashMap::new(),
            next_peer_index: 0,
        }
    }

    pub fn max_pending(&self) -> usize {
        self.max_pending
    }

    pub fn max_pending_per_peer(&self) -> usize {
        self.max_pending_per_peer
    }

    pub fn take_fetch_counters(&mut self) -> FetchBackpressureCounters {
        let counters = FetchBackpressureCounters {
            suppressed: self.backpressure_suppressed,
            queued: self.fetch_queued,
            dropped: self.fetch_dropped,
        };
        self.backpressure_suppressed = 0;
        self.fetch_queued = 0;
        self.fetch_dropped = 0;
        counters
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn take_backpressure_suppressed(&mut self) -> u64 {
        let suppressed = self.backpressure_suppressed;
        self.backpressure_suppressed = 0;
        suppressed
    }

    pub fn should_issue_getblock(&mut self, hash: &str, now_unix: u64) -> bool {
        self.should_issue_getblock_for_peers(hash, now_unix, std::iter::empty::<String>())
    }

    pub fn should_issue_getblock_for_peers<I, S>(
        &mut self,
        hash: &str,
        now_unix: u64,
        peers: I,
    ) -> bool
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let peers = peers.into_iter().map(Into::into).collect::<Vec<_>>();
        if self.is_backing_off(hash, now_unix) {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }

        if let Some(req) = self.pending.get_mut(hash) {
            if now_unix.saturating_sub(req.last_requested_at_unix)
                < Self::retry_backoff_secs(self.timeout_secs, req.retry_count)
            {
                self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
                return false;
            }
            if req.retry_count >= self.retry_limit {
                self.note_request_dropped(hash, now_unix);
                return false;
            }
            req.retry_count = req.retry_count.saturating_add(1);
            req.last_requested_at_unix = now_unix;
            self.fetch_queued = self.fetch_queued.saturating_add(1);
            return true;
        }

        if self.pending.len() >= self.max_pending {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }

        let peer = self.select_available_peer(&peers);
        if !peers.is_empty() && peer.is_none() {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }
        self.pending.insert(
            hash.to_string(),
            PendingBlockRequest {
                first_requested_at_unix: now_unix,
                last_requested_at_unix: now_unix,
                retry_count: 0,
                peer,
            },
        );
        self.backoff_by_hash.remove(hash);
        self.fetch_queued = self.fetch_queued.saturating_add(1);
        true
    }

    fn retry_backoff_secs(timeout_secs: u64, retry_count: u8) -> u64 {
        let shift = retry_count.min(6) as u32;
        timeout_secs
            .saturating_mul(1u64 << shift)
            .clamp(timeout_secs.max(1), MAX_BLOCK_REQUEST_BACKOFF_SECS)
    }

    fn is_backing_off(&mut self, hash: &str, now_unix: u64) -> bool {
        match self.backoff_by_hash.get(hash) {
            Some(backoff) if now_unix < backoff.retry_after_unix => true,
            Some(_) => {
                self.backoff_by_hash.remove(hash);
                false
            }
            None => false,
        }
    }

    fn select_available_peer(&mut self, peers: &[String]) -> Option<String> {
        if peers.is_empty() {
            return None;
        }
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for req in self.pending.values() {
            if let Some(peer) = req.peer.as_deref() {
                *counts.entry(peer).or_default() += 1;
            }
        }
        for offset in 0..peers.len() {
            let idx = (self.next_peer_index + offset) % peers.len();
            let peer = &peers[idx];
            if counts.get(peer.as_str()).copied().unwrap_or(0) < self.max_pending_per_peer {
                self.next_peer_index = (idx + 1) % peers.len();
                return Some(peer.clone());
            }
        }
        None
    }

    fn note_request_dropped(&mut self, hash: &str, now_unix: u64) {
        let failures = self
            .backoff_by_hash
            .get(hash)
            .map(|backoff| backoff.failures.saturating_add(1))
            .unwrap_or(1);
        let delay = DEFAULT_BLOCK_REQUEST_BACKOFF_SECS
            .saturating_mul(1u64 << failures.min(6))
            .min(MAX_BLOCK_REQUEST_BACKOFF_SECS);
        self.backoff_by_hash.insert(
            hash.to_string(),
            RequestBackoff {
                retry_after_unix: now_unix.saturating_add(delay),
                failures,
            },
        );
        self.fetch_dropped = self.fetch_dropped.saturating_add(1);
        self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
    }

    pub fn note_not_found(&mut self, hash: &str, now_unix: u64) {
        self.pending.remove(hash);
        self.note_request_dropped(hash, now_unix);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn pending_capacity_remaining(&self) -> usize {
        self.max_pending.saturating_sub(self.pending.len())
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
        self.backoff_by_hash.remove(hash);
    }

    pub fn pending_hashes(&self) -> Vec<String> {
        let mut hashes = self.pending.keys().cloned().collect::<Vec<_>>();
        hashes.sort();
        hashes
    }

    pub fn inflight_by_peer(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for req in self.pending.values() {
            if let Some(peer) = req.peer.as_deref() {
                *counts.entry(peer.to_string()).or_default() += 1;
            }
        }
        counts
    }

    pub fn oldest_pending_age_secs(&self, now_unix: u64) -> u64 {
        self.pending
            .values()
            .map(|req| now_unix.saturating_sub(req.first_requested_at_unix))
            .max()
            .unwrap_or(0)
    }

    pub fn collect_timeouts(&self, now_unix: u64) -> Vec<String> {
        self.pending
            .iter()
            .filter(|(_, req)| {
                now_unix.saturating_sub(req.last_requested_at_unix)
                    >= Self::retry_backoff_secs(self.timeout_secs, req.retry_count)
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

#[derive(Debug, Clone)]
pub struct DependencyAwareFetchScheduler {
    candidates: HashMap<String, HeaderFetchCandidate>,
    inventory: VecDeque<String>,
    queued: HashSet<String>,
    max_queue_depth: usize,
}

impl Default for DependencyAwareFetchScheduler {
    fn default() -> Self {
        Self::with_limit(512)
    }
}

impl DependencyAwareFetchScheduler {
    pub fn with_limit(max_queue_depth: usize) -> Self {
        Self {
            candidates: HashMap::new(),
            inventory: VecDeque::new(),
            queued: HashSet::new(),
            max_queue_depth: max_queue_depth.max(1),
        }
    }

    pub fn note_header(&mut self, candidate: HeaderFetchCandidate) {
        if self.queued.contains(&candidate.hash) {
            self.candidates.insert(candidate.hash.clone(), candidate);
            return;
        }
        if self.inventory.len() >= self.max_queue_depth {
            return;
        }
        self.queued.insert(candidate.hash.clone());
        self.inventory.push_back(candidate.hash.clone());
        self.candidates.insert(candidate.hash.clone(), candidate);
    }

    pub fn next_plan(&mut self, known_blocks: &HashSet<String>, limit: usize) -> DependencyFetchPlan {
        let mut plan = DependencyFetchPlan::default();
        let mut remaining = self.inventory.len();
        while remaining > 0 && plan.requests.len() < limit {
            remaining -= 1;
            let Some(hash) = self.inventory.pop_front() else {
                break;
            };
            self.queued.remove(&hash);
            let Some(candidate) = self.candidates.get(&hash).cloned() else {
                continue;
            };
            if known_blocks.contains(&candidate.hash) {
                self.candidates.remove(&candidate.hash);
                continue;
            }
            let missing = candidate
                .parents
                .iter()
                .filter(|parent| !known_blocks.contains(*parent))
                .cloned()
                .collect::<Vec<_>>();
            if let Some(parent) = missing.first() {
                if !self.candidates.contains_key(parent) && !known_blocks.contains(parent) {
                    plan.requests.push(parent.clone());
                    plan.parent_first_requests = plan.parent_first_requests.saturating_add(1);
                }
                plan.deferred.push(candidate.hash.clone());
                self.note_header(candidate);
                continue;
            }
            plan.requests.push(candidate.hash.clone());
            self.candidates.remove(&candidate.hash);
        }
        plan
    }

    pub fn queue_depth(&self) -> usize {
        self.inventory.len()
    }
}
