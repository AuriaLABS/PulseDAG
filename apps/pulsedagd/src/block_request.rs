use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use pulsedag_p2p::messages::BlockHeaderAnnouncement;

#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_MAX_PENDING_BLOCK_REQUESTS: usize = 512;
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_MAX_PENDING_BLOCK_REQUESTS_PER_PEER: usize = 128;
pub const DEFAULT_BLOCK_REQUEST_BACKOFF_SECS: u64 = 8;
pub const MAX_BLOCK_REQUEST_BACKOFF_SECS: u64 = 120;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingParentRequestState {
    pub requested_from_peers: Vec<String>,
    pub not_found_from_peers: Vec<String>,
    pub contacted_peers: Vec<String>,
    pub requests: Vec<MissingParentRequestRecord>,
    pub next_eligible_direct_peer: Option<String>,
    pub terminal_reason: Option<String>,
    pub last_request_unix: Option<u64>,
    pub retry_count: u64,
    pub terminal_unavailable_after_all_peers: bool,
    pub terminal_generation: u64,
    pub terminal_at_unix: Option<u64>,
    pub terminal_peer_set_digest: Option<String>,
    pub reopened_total: u64,
    pub reopen_reason: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MissingParentRequestRecord {
    pub request_id: String,
    pub requested_peer_id: String,
    pub block_hash: String,
    pub sent_at: u64,
    pub response_at: Option<u64>,
    pub response_kind: Option<String>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct PeerMissingParentHints {
    inventory: HashSet<String>,
    tip_ancestry: HashSet<String>,
    selected_tip: Option<String>,
    best_height: Option<u64>,
    blue_score: Option<u64>,
    successful_blockdata_responses: u64,
    not_found_responses: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerSyncHintsSnapshot {
    pub peer: String,
    pub best_height: Option<u64>,
    pub selected_tip: Option<String>,
    pub blue_score: Option<u64>,
    pub known_inventory: usize,
    pub missing_parent_usefulness: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockRequestPeerRetryOutcome {
    pub retry: bool,
    pub peer: Option<String>,
    pub all_peers_exhausted: bool,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GetBlockRequestReadiness {
    Requestable,
    AlreadyPending,
    RateLimited,
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
    not_found_by_hash: HashMap<String, HashSet<String>>,
    timed_out_by_hash: HashMap<String, HashSet<String>>,
    requested_by_hash: HashMap<String, HashSet<String>>,
    request_state_by_hash: HashMap<String, MissingParentRequestState>,
    peer_hints: HashMap<String, PeerMissingParentHints>,
    exhausted_hashes: HashSet<String>,
    next_peer_index: usize,
    request_sequence: u64,
    request_records_by_hash: HashMap<String, Vec<MissingParentRequestRecord>>,
    peer_inventory_generation: u64,
    exhausted_generation_by_hash: HashMap<String, u64>,
    reopened_total_by_hash: HashMap<String, u64>,
    reopen_reason_by_hash: HashMap<String, String>,
    terminal_at_unix_by_hash: HashMap<String, u64>,
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
            not_found_by_hash: HashMap::new(),
            timed_out_by_hash: HashMap::new(),
            requested_by_hash: HashMap::new(),
            request_state_by_hash: HashMap::new(),
            peer_hints: HashMap::new(),
            exhausted_hashes: HashSet::new(),
            next_peer_index: 0,
            request_sequence: 0,
            request_records_by_hash: HashMap::new(),
            peer_inventory_generation: 0,
            exhausted_generation_by_hash: HashMap::new(),
            reopened_total_by_hash: HashMap::new(),
            reopen_reason_by_hash: HashMap::new(),
            terminal_at_unix_by_hash: HashMap::new(),
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

    pub fn classify_getblock_for_peers<I, S>(
        &self,
        hash: &str,
        now_unix: u64,
        _peers: I,
    ) -> GetBlockRequestReadiness
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if self.pending.contains_key(hash) {
            return GetBlockRequestReadiness::AlreadyPending;
        }
        if self
            .backoff_by_hash
            .get(hash)
            .map(|backoff| now_unix < backoff.retry_after_unix)
            .unwrap_or(false)
        {
            return GetBlockRequestReadiness::RateLimited;
        }
        if self.pending.len() >= self.max_pending {
            return GetBlockRequestReadiness::RateLimited;
        }
        GetBlockRequestReadiness::Requestable
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
        if self.exhausted_hashes.contains(hash) {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return false;
        }

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

        let peer = self.select_available_peer_for_hash(hash, &peers);
        if !peers.is_empty() && peer.is_none() {
            // The request_block API is not peer-addressed at this layer. When all
            // visible peers have reached their accounting cap but global capacity
            // remains, keep orphan-root recovery moving with an unassigned request
            // instead of suppressing the fetch entirely. The global max_pending cap
            // still bounds total in-flight work.
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
        }
        self.record_missing_parent_request(hash, peer.as_deref(), now_unix);
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

    fn record_missing_parent_request(&mut self, hash: &str, peer: Option<&str>, now_unix: u64) {
        let state = self
            .request_state_by_hash
            .entry(hash.to_string())
            .or_default();
        state.last_request_unix = Some(now_unix);
        state.retry_count = state.retry_count.saturating_add(1);
        state.terminal_unavailable_after_all_peers = false;
        if let Some(peer) = peer {
            self.requested_by_hash
                .entry(hash.to_string())
                .or_default()
                .insert(peer.to_string());
            self.request_sequence = self.request_sequence.saturating_add(1);
            self.request_records_by_hash
                .entry(hash.to_string())
                .or_default()
                .push(MissingParentRequestRecord {
                    request_id: format!(
                        "missing-parent:{}:{}:{}",
                        hash, peer, self.request_sequence
                    ),
                    requested_peer_id: peer.to_string(),
                    block_hash: hash.to_string(),
                    sent_at: now_unix,
                    response_at: None,
                    response_kind: None,
                });
            if !state.requested_from_peers.iter().any(|p| p == peer) {
                state.requested_from_peers.push(peer.to_string());
                state.requested_from_peers.sort();
            }
        }
    }

    #[allow(dead_code)]
    pub fn missing_parent_request_state(&self, hash: &str) -> MissingParentRequestState {
        let mut state = self
            .request_state_by_hash
            .get(hash)
            .cloned()
            .unwrap_or_default();
        state.requested_from_peers = self
            .requested_by_hash
            .get(hash)
            .map(|p| {
                let mut v = p.iter().cloned().collect::<Vec<_>>();
                v.sort();
                v
            })
            .unwrap_or(state.requested_from_peers);
        state.not_found_from_peers = self.not_found_peers(hash);
        state.contacted_peers = state.requested_from_peers.clone();
        state.requests = self
            .request_records_by_hash
            .get(hash)
            .cloned()
            .unwrap_or_default();
        state.terminal_unavailable_after_all_peers = self.exhausted_hashes.contains(hash);
        state.terminal_generation = self
            .exhausted_generation_by_hash
            .get(hash)
            .copied()
            .unwrap_or_default();
        state.terminal_at_unix = self.terminal_at_unix_by_hash.get(hash).copied();
        state.terminal_peer_set_digest = state
            .terminal_generation
            .ne(&0)
            .then(|| format!("peer_inventory_generation:{}", state.terminal_generation));
        state.reopened_total = self
            .reopened_total_by_hash
            .get(hash)
            .copied()
            .unwrap_or_default();
        state.reopen_reason = self.reopen_reason_by_hash.get(hash).cloned();
        state.terminal_reason = state
            .terminal_unavailable_after_all_peers
            .then(|| "all_direct_request_capable_peers_exhausted".to_string());
        state
    }

    #[allow(dead_code)]
    pub fn note_peer_inventory<I, S>(&mut self, peer: impl Into<String>, hashes: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let hints = self.peer_hints.entry(peer.into()).or_default();
        let mut changed = false;
        let hashes = hashes.into_iter().map(Into::into).collect::<Vec<String>>();
        for hash in &hashes {
            changed |= hints.inventory.insert(hash.clone());
        }
        if changed {
            self.peer_inventory_generation = self.peer_inventory_generation.saturating_add(1);
        }
        for hash in hashes {
            self.reopen_exhausted_hash(&hash, "peer_inventory_advertised_hash");
        }
    }

    #[allow(dead_code)]
    pub fn note_peer_tip_ancestry<I, S>(
        &mut self,
        peer: impl Into<String>,
        selected_tip: Option<String>,
        best_height: u64,
        ancestry: I,
    ) where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let hints = self.peer_hints.entry(peer.into()).or_default();
        hints.selected_tip = selected_tip;
        hints.best_height = Some(best_height);
        hints
            .tip_ancestry
            .extend(ancestry.into_iter().map(Into::into));
        let ancestry = hints.tip_ancestry.clone();
        for hash in ancestry {
            self.reopen_exhausted_hash(&hash, "peer_tip_ancestry_references_hash");
        }
    }

    #[allow(dead_code)]
    pub fn note_peer_sync_state(
        &mut self,
        peer: impl Into<String>,
        best_height: u64,
        selected_tip: Option<String>,
        blue_score: u64,
    ) {
        let hints = self.peer_hints.entry(peer.into()).or_default();
        let old_tip = hints.selected_tip.clone();
        hints.best_height = Some(best_height);
        hints.selected_tip = selected_tip;
        hints.blue_score = Some(blue_score);
        if old_tip != hints.selected_tip {
            self.peer_inventory_generation = self.peer_inventory_generation.saturating_add(1);
            if let Some(tip) = hints.selected_tip.clone() {
                self.reopen_exhausted_hash(&tip, "selected_chain_frontier_references_hash");
            }
        }
    }

    fn reopen_exhausted_hash(&mut self, hash: &str, reason: &str) -> bool {
        if !self.exhausted_hashes.remove(hash) {
            return false;
        }
        self.backoff_by_hash.remove(hash);
        self.not_found_by_hash.remove(hash);
        self.timed_out_by_hash.remove(hash);
        self.exhausted_generation_by_hash.remove(hash);
        let total = self
            .reopened_total_by_hash
            .entry(hash.to_string())
            .or_default();
        *total = total.saturating_add(1);
        self.reopen_reason_by_hash
            .insert(hash.to_string(), reason.to_string());
        if let Some(state) = self.request_state_by_hash.get_mut(hash) {
            state.terminal_unavailable_after_all_peers = false;
            state.reopened_total = *total;
            state.reopen_reason = Some(reason.to_string());
        }
        true
    }

    #[allow(dead_code)]
    pub fn peer_sync_hints(&self) -> Vec<PeerSyncHintsSnapshot> {
        let mut snapshots = self
            .peer_hints
            .iter()
            .map(|(peer, hints)| PeerSyncHintsSnapshot {
                peer: peer.clone(),
                best_height: hints.best_height,
                selected_tip: hints.selected_tip.clone(),
                blue_score: hints.blue_score,
                known_inventory: hints.inventory.len(),
                missing_parent_usefulness: (hints.successful_blockdata_responses as i64 * 10)
                    - (hints.not_found_responses as i64 * 5),
            })
            .collect::<Vec<_>>();
        snapshots.sort_by(|a, b| a.peer.cmp(&b.peer));
        snapshots
    }

    #[allow(dead_code)]
    pub fn note_successful_blockdata_response(&mut self, peer: impl Into<String>, hash: &str) {
        let hints = self.peer_hints.entry(peer.into()).or_default();
        hints.successful_blockdata_responses =
            hints.successful_blockdata_responses.saturating_add(1);
        hints.inventory.insert(hash.to_string());
        self.resolve(hash);
    }

    fn eligible_peers_for_hash(&self, hash: &str, peers: &[String]) -> Vec<String> {
        let mut scored = peers
            .iter()
            .map(|peer| {
                let score = self
                    .peer_hints
                    .get(peer)
                    .map(|h| {
                        let mut score = 0i64;
                        if h.inventory.contains(hash) {
                            score += 100;
                        }
                        if h.tip_ancestry.contains(hash) {
                            score += 80;
                        }
                        if h.best_height.unwrap_or(0) > 0 {
                            score += 10;
                        }
                        score += (h.blue_score.unwrap_or(0) as i64).min(20);
                        score += (h.successful_blockdata_responses as i64).min(20);
                        score -= ((h.not_found_responses as i64) * 5).min(40);
                        score
                    })
                    .unwrap_or(0);
                (peer.clone(), score)
            })
            .collect::<Vec<_>>();
        let has_positive = scored.iter().any(|(_, score)| *score > 0);
        if has_positive {
            scored.retain(|(_, score)| *score > 0);
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.into_iter().map(|(peer, _)| peer).collect()
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

    fn select_available_peer_for_hash(&mut self, hash: &str, peers: &[String]) -> Option<String> {
        if peers.is_empty() {
            return None;
        }
        let peers = self.eligible_peers_for_hash(hash, peers);
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for req in self.pending.values() {
            if let Some(peer) = req.peer.as_deref() {
                *counts.entry(peer).or_default() += 1;
            }
        }
        let not_found = self.not_found_by_hash.get(hash);
        let timed_out = self.timed_out_by_hash.get(hash);
        for offset in 0..peers.len() {
            let idx = (self.next_peer_index + offset) % peers.len();
            let peer = &peers[idx];
            if not_found.is_some_and(|failed| failed.contains(peer))
                || timed_out.is_some_and(|failed| failed.contains(peer))
            {
                continue;
            }
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

    pub fn note_not_found<I, S>(
        &mut self,
        hash: &str,
        now_unix: u64,
        peers: I,
    ) -> BlockRequestPeerRetryOutcome
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let previous_peer = self.pending.remove(hash).and_then(|req| req.peer);
        if let Some(peer) = previous_peer {
            let hints = self.peer_hints.entry(peer.clone()).or_default();
            hints.not_found_responses = hints.not_found_responses.saturating_add(1);
            self.not_found_by_hash
                .entry(hash.to_string())
                .or_default()
                .insert(peer.clone());
            if let Some(record) = self
                .request_records_by_hash
                .get_mut(hash)
                .and_then(|records| {
                    records.iter_mut().rev().find(|record| {
                        record.requested_peer_id == peer && record.response_kind.is_none()
                    })
                })
            {
                record.response_at = Some(now_unix);
                record.response_kind = Some("not_found".to_string());
            }
            let state = self
                .request_state_by_hash
                .entry(hash.to_string())
                .or_default();
            if !state.not_found_from_peers.iter().any(|p| p == &peer) {
                state.not_found_from_peers.push(peer);
                state.not_found_from_peers.sort();
            }
        }
        self.retry_after_peer_failure(hash, now_unix, peers)
    }

    pub fn retry_after_timeout<I, S>(
        &mut self,
        hash: &str,
        now_unix: u64,
        peers: I,
    ) -> BlockRequestPeerRetryOutcome
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        if let Some(peer) = self.pending.get(hash).and_then(|req| req.peer.clone()) {
            self.timed_out_by_hash
                .entry(hash.to_string())
                .or_default()
                .insert(peer.clone());
            if let Some(record) = self
                .request_records_by_hash
                .get_mut(hash)
                .and_then(|records| {
                    records.iter_mut().rev().find(|record| {
                        record.requested_peer_id == peer && record.response_kind.is_none()
                    })
                })
            {
                record.response_at = Some(now_unix);
                record.response_kind = Some("timeout".to_string());
            }
        }
        self.pending.remove(hash);
        self.retry_after_peer_failure(hash, now_unix, peers)
    }

    fn retry_after_peer_failure<I, S>(
        &mut self,
        hash: &str,
        now_unix: u64,
        peers: I,
    ) -> BlockRequestPeerRetryOutcome
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let peers = peers.into_iter().map(Into::into).collect::<Vec<_>>();
        if peers.is_empty() {
            self.note_request_dropped(hash, now_unix);
            return BlockRequestPeerRetryOutcome {
                retry: false,
                peer: None,
                all_peers_exhausted: false,
            };
        }
        if self.pending.len() >= self.max_pending {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
            return BlockRequestPeerRetryOutcome {
                retry: false,
                peer: None,
                all_peers_exhausted: false,
            };
        }
        let peer = self.select_available_peer_for_hash(hash, &peers);
        if let Some(peer) = peer {
            self.record_missing_parent_request(hash, Some(&peer), now_unix);
            self.pending.insert(
                hash.to_string(),
                PendingBlockRequest {
                    first_requested_at_unix: now_unix,
                    last_requested_at_unix: now_unix,
                    retry_count: 0,
                    peer: Some(peer.clone()),
                },
            );
            self.backoff_by_hash.remove(hash);
            self.fetch_queued = self.fetch_queued.saturating_add(1);
            return BlockRequestPeerRetryOutcome {
                retry: true,
                peer: Some(peer),
                all_peers_exhausted: false,
            };
        }
        let newly_exhausted = self.exhausted_hashes.insert(hash.to_string());
        self.exhausted_generation_by_hash
            .insert(hash.to_string(), self.peer_inventory_generation);
        self.terminal_at_unix_by_hash
            .insert(hash.to_string(), now_unix);
        self.request_state_by_hash
            .entry(hash.to_string())
            .or_default()
            .terminal_unavailable_after_all_peers = true;
        if newly_exhausted {
            self.note_request_dropped(hash, now_unix);
        } else {
            self.backpressure_suppressed = self.backpressure_suppressed.saturating_add(1);
        }
        BlockRequestPeerRetryOutcome {
            retry: false,
            peer: None,
            all_peers_exhausted: true,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_all_peers_exhausted(&self, hash: &str) -> bool {
        self.exhausted_hashes.contains(hash)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn not_found_peers(&self, hash: &str) -> Vec<String> {
        let mut peers = self
            .not_found_by_hash
            .get(hash)
            .map(|peers| peers.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        peers.sort();
        peers
    }

    pub fn has_not_found_peer(&self, hash: &str) -> bool {
        self.not_found_by_hash
            .get(hash)
            .map(|peers| !peers.is_empty())
            .unwrap_or(false)
    }

    pub fn has_timed_out_peer(&self, hash: &str) -> bool {
        self.timed_out_by_hash
            .get(hash)
            .map(|peers| !peers.is_empty())
            .unwrap_or(false)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn reset_backoff(&mut self, hash: &str) -> bool {
        self.backoff_by_hash.remove(hash).is_some()
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
        self.not_found_by_hash.remove(hash);
        self.timed_out_by_hash.remove(hash);
        self.requested_by_hash.remove(hash);
        self.request_state_by_hash.remove(hash);
        self.exhausted_hashes.remove(hash);
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

    pub fn queue_depth(&self) -> usize {
        self.inventory.len()
    }

    pub fn queue_inventory<I, S>(&mut self, hashes: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut added = 0usize;
        for hash in hashes {
            let hash = hash.into();
            if self.inventory.len() >= self.max_queue_depth {
                break;
            }
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
            if self.inventory.len() < self.max_queue_depth && self.queued.insert(hash.clone()) {
                self.inventory.push_back(hash.clone());
                added = added.saturating_add(1);
                self.candidates.insert(hash, candidate);
            }
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
#[allow(dead_code, unused_variables)]
mod tests {
    use super::{BlockRequestTracker, DependencyAwareFetchScheduler, HeaderFetchCandidate};
    use crate::GetBlockRequestReadiness;
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

        let second = tracker.drain_timeouts(27);
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
    fn caps_pending_requests_and_reports_backpressure() {
        let mut tracker = BlockRequestTracker::with_max_pending(10, 2, 2);

        assert!(tracker.should_issue_getblock("h1", 100));
        assert!(tracker.should_issue_getblock("h2", 100));
        assert!(!tracker.should_issue_getblock("h3", 100));

        assert_eq!(tracker.pending_hashes(), vec!["h1", "h2"]);
        assert_eq!(tracker.take_backpressure_suppressed(), 1);
        assert_eq!(tracker.take_backpressure_suppressed(), 0);
    }

    #[test]
    fn peer_saturation_falls_back_to_unassigned_request() {
        let mut tracker = BlockRequestTracker::with_limits(10, 2, 10, 1);
        assert!(tracker.should_issue_getblock_for_peers("h1", 100, ["peer-a"]));
        assert!(tracker.should_issue_getblock_for_peers("h2", 100, ["peer-b"]));
        assert!(tracker.should_issue_getblock_for_peers("h3", 100, ["peer-a", "peer-b"]));

        let by_peer = tracker.inflight_by_peer();
        assert_eq!(by_peer.get("peer-a"), Some(&1));
        assert_eq!(by_peer.get("peer-b"), Some(&1));
        assert_eq!(
            tracker
                .pending
                .get("h3")
                .and_then(|req| req.peer.as_deref()),
            None
        );
        assert_eq!(tracker.take_fetch_counters().suppressed, 1);
    }

    #[test]
    fn missing_parent_request_retries_next_peer_after_not_found() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a", "peer-b"]));
        assert_eq!(
            tracker
                .pending
                .get("parent")
                .and_then(|req| req.peer.as_deref()),
            Some("peer-a")
        );

        let outcome = tracker.note_not_found("parent", 101, ["peer-a", "peer-b"]);
        assert!(outcome.retry);
        assert_eq!(outcome.peer.as_deref(), Some("peer-b"));
        assert!(!outcome.all_peers_exhausted);
        assert_eq!(tracker.not_found_peers("parent"), vec!["peer-a"]);
        assert_eq!(
            tracker
                .pending
                .get("parent")
                .and_then(|req| req.peer.as_deref()),
            Some("peer-b")
        );
    }

    #[test]
    fn missing_parent_request_marks_peer_not_found_and_does_not_hammer_same_peer() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a"]));

        let outcome = tracker.note_not_found("parent", 101, ["peer-a"]);
        assert!(!outcome.retry);
        assert!(outcome.all_peers_exhausted);
        assert_eq!(tracker.not_found_peers("parent"), vec!["peer-a"]);
        assert!(!tracker.pending.contains_key("parent"));
        assert!(tracker.is_all_peers_exhausted("parent"));
        assert!(!tracker.should_issue_getblock_for_peers("parent", 102, ["peer-a"]));
    }

    #[test]
    fn reset_backoff_allows_bounded_recovery_reissue() {
        let mut tracker = BlockRequestTracker::with_limit(5, 1, 2);
        assert!(tracker.should_issue_getblock("parent", 100));
        let outcome = tracker.note_not_found("parent", 101, std::iter::empty::<String>());
        assert!(!outcome.retry);
        assert!(!tracker.should_issue_getblock("parent", 102));

        assert!(tracker.reset_backoff("parent"));
        assert!(tracker.should_issue_getblock("parent", 103));
        assert!(!tracker.reset_backoff("parent"));
    }

    #[test]
    fn successful_blockdata_response_resolves_orphan_request_state() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a", "peer-b"]));
        let outcome = tracker.note_not_found("parent", 101, ["peer-a", "peer-b"]);
        assert!(outcome.retry);
        assert_eq!(tracker.not_found_peers("parent"), vec!["peer-a"]);

        tracker.resolve("parent");

        assert!(tracker.pending_hashes().is_empty());
        assert!(tracker.not_found_peers("parent").is_empty());
        assert!(!tracker.is_all_peers_exhausted("parent"));
        assert!(tracker.should_issue_getblock_for_peers("parent", 102, ["peer-a"]));
    }

    #[test]
    fn already_pending_request_times_out_and_retries_another_peer() {
        let mut tracker = BlockRequestTracker::with_limits(5, 2, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a", "peer-b"]));
        assert_eq!(
            tracker.classify_getblock_for_peers("parent", 101, ["peer-a", "peer-b"]),
            GetBlockRequestReadiness::AlreadyPending
        );

        let timed_out = tracker.drain_timeouts(105);
        assert_eq!(timed_out.retryable, vec!["parent"]);
        let outcome = tracker.retry_after_timeout("parent", 105, ["peer-a", "peer-b"]);

        assert!(outcome.retry);
        assert_eq!(outcome.peer.as_deref(), Some("peer-b"));
        assert_eq!(tracker.pending_hashes(), vec!["parent"]);
        assert!(tracker.has_timed_out_peer("parent"));
    }

    #[test]
    fn peer_not_found_rotates_peers_then_marks_exhausted() {
        let mut tracker = BlockRequestTracker::with_limits(5, 2, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a", "peer-b"]));

        let first = tracker.note_not_found("parent", 101, ["peer-a", "peer-b"]);
        assert!(first.retry);
        assert_eq!(first.peer.as_deref(), Some("peer-b"));
        assert!(!first.all_peers_exhausted);

        let second = tracker.note_not_found("parent", 102, ["peer-a", "peer-b"]);
        assert!(!second.retry);
        assert!(second.all_peers_exhausted);
        assert_eq!(
            tracker.not_found_peers("parent"),
            vec!["peer-a".to_string(), "peer-b".to_string()]
        );
        assert!(tracker.is_all_peers_exhausted("parent"));
    }

    #[test]
    fn inflight_requests_drain_after_timeout_when_peers_exhausted() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a"]));
        let timed_out = tracker.drain_timeouts(105);
        assert_eq!(timed_out.retryable, vec!["parent"]);

        let outcome = tracker.retry_after_timeout("parent", 105, ["peer-a"]);
        assert!(!outcome.retry);
        assert!(outcome.all_peers_exhausted);
        assert!(tracker.pending_hashes().is_empty());
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
    fn pending_limit_prevents_unbounded_inflight_growth() {
        let mut tracker = BlockRequestTracker::with_limit(10, 2, 2);

        assert!(tracker.should_issue_getblock("h1", 100));
        assert!(tracker.should_issue_getblock("h2", 100));
        assert!(!tracker.should_issue_getblock("h3", 100));
        assert_eq!(tracker.pending_capacity_remaining(), 0);
    }

    #[test]
    fn scheduler_queue_limit_bounds_inventory_backlog() {
        let mut scheduler = DependencyAwareFetchScheduler::with_limit(2);
        let added = scheduler.queue_inventory(["a", "b", "c"]);

        assert_eq!(added, 2);
        assert_eq!(scheduler.queue_depth(), 2);
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

    #[test]
    fn rate_limited_root_is_classified_not_hidden() {
        let mut tracker = BlockRequestTracker::with_limit(10, 2, 1);
        assert!(tracker.should_issue_getblock("root-a", 100));

        assert_eq!(
            tracker.classify_getblock_for_peers("root-a", 101, ["peer-a"]),
            super::GetBlockRequestReadiness::AlreadyPending
        );
        assert_eq!(
            tracker.classify_getblock_for_peers("root-b", 101, ["peer-a"]),
            super::GetBlockRequestReadiness::RateLimited
        );
    }

    #[test]
    fn missing_parent_requested_from_peer_with_inventory_hint() {
        let mut tracker = BlockRequestTracker::with_limits(5, 2, 10, 10);
        tracker.note_peer_inventory("peer-b", ["parent"]);
        tracker.note_peer_tip_ancestry("peer-c", Some("tip-c".to_string()), 7, ["other"]);
        tracker.note_peer_sync_state("peer-b", 8, Some("tip-b".to_string()), 11);

        assert!(tracker.should_issue_getblock_for_peers(
            "parent",
            100,
            ["peer-a", "peer-b", "peer-c"]
        ));

        let state = tracker.missing_parent_request_state("parent");
        assert_eq!(state.requested_from_peers, vec!["peer-b"]);
        assert_eq!(state.last_request_unix, Some(100));
        assert_eq!(state.retry_count, 1);
        assert_eq!(
            tracker
                .pending
                .get("parent")
                .and_then(|req| req.peer.as_deref()),
            Some("peer-b")
        );
        let peer_b = tracker
            .peer_sync_hints()
            .into_iter()
            .find(|peer| peer.peer == "peer-b")
            .expect("peer-b snapshot");
        assert_eq!(peer_b.best_height, Some(8));
        assert_eq!(peer_b.selected_tip.as_deref(), Some("tip-b"));
        assert_eq!(peer_b.blue_score, Some(11));
        assert_eq!(peer_b.known_inventory, 1);
        assert!(peer_b.missing_parent_usefulness >= 0);
    }

    #[test]
    fn missing_parent_request_state_tracks_rotation_until_terminal() {
        let mut tracker = BlockRequestTracker::with_limits(5, 2, 10, 10);
        tracker.note_peer_inventory("peer-a", ["parent"]);
        tracker.note_peer_tip_ancestry("peer-b", Some("tip-b".to_string()), 9, ["parent"]);

        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a", "peer-b"]));
        let first = tracker.note_not_found("parent", 101, ["peer-a", "peer-b"]);
        assert!(first.retry);
        assert_eq!(first.peer.as_deref(), Some("peer-b"));
        let state = tracker.missing_parent_request_state("parent");
        assert_eq!(state.requested_from_peers, vec!["peer-a", "peer-b"]);
        assert_eq!(state.not_found_from_peers, vec!["peer-a"]);
        assert!(!state.terminal_unavailable_after_all_peers);

        let second = tracker.note_not_found("parent", 102, ["peer-a", "peer-b"]);
        assert!(!second.retry);
        assert!(second.all_peers_exhausted);
        let terminal = tracker.missing_parent_request_state("parent");
        assert_eq!(terminal.not_found_from_peers, vec!["peer-a", "peer-b"]);
        assert!(terminal.terminal_unavailable_after_all_peers);
        assert_eq!(terminal.retry_count, 2);
    }

    #[test]
    fn repeated_missing_parent_reconciliation_is_idempotent_after_exhaustion() {
        let mut tracker = BlockRequestTracker::with_limits(5, 2, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a"]));
        assert!(
            tracker
                .note_not_found("parent", 101, ["peer-a"])
                .all_peers_exhausted
        );
        let before = tracker.missing_parent_request_state("parent");

        assert!(!tracker.should_issue_getblock_for_peers("parent", 200, ["peer-a"]));
        assert!(!tracker.should_issue_getblock_for_peers("parent", 300, ["peer-a"]));

        assert_eq!(tracker.missing_parent_request_state("parent"), before);
    }

    #[test]
    fn exhausted_missing_parent_suppresses_future_retries() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a"]));
        let first = tracker.note_not_found("parent", 101, ["peer-a"]);
        assert!(first.all_peers_exhausted);
        assert!(!tracker.should_issue_getblock_for_peers("parent", 200, ["peer-a"]));
        assert!(tracker.is_all_peers_exhausted("parent"));
    }

    #[test]
    fn terminal_parent_reopens_when_peer_inventory_later_advertises_it() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a"]));
        assert!(
            tracker
                .note_not_found("parent", 101, ["peer-a"])
                .all_peers_exhausted
        );

        tracker.note_peer_inventory("peer-a", ["parent"]);

        assert!(!tracker.is_all_peers_exhausted("parent"));
        let state = tracker.missing_parent_request_state("parent");
        assert_eq!(state.reopened_total, 1);
        assert_eq!(
            state.reopen_reason.as_deref(),
            Some("peer_inventory_advertised_hash")
        );
        assert!(tracker.should_issue_getblock_for_peers("parent", 200, ["peer-a"]));
    }

    #[test]
    fn unchanged_peer_inventory_generation_does_not_retry_terminal_parent() {
        let mut tracker = BlockRequestTracker::with_limits(5, 1, 10, 10);
        assert!(tracker.should_issue_getblock_for_peers("parent", 100, ["peer-a"]));
        assert!(
            tracker
                .note_not_found("parent", 101, ["peer-a"])
                .all_peers_exhausted
        );

        assert!(!tracker.should_issue_getblock_for_peers("parent", 200, ["peer-a"]));
        assert!(!tracker.should_issue_getblock_for_peers("parent", 300, ["peer-a"]));
        let state = tracker.missing_parent_request_state("parent");
        assert!(state.terminal_unavailable_after_all_peers);
        assert_eq!(state.reopened_total, 0);
    }
}
