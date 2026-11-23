//! First-network native-v3 **pull-based** missing-parent/block recovery plan.
//!
//! Bitcoin/Ethereum style separation: signed/verified header discovery is
//! independent of bounded direct block fetch, and economic acceptance is
//! independent of either. This planner NEVER sends gossip or decides GD-08,
//! rewards, spend validity, checkpoints or monetary activation.
//!
//! Only call `on_candidate` after the SAME Swarm's OutboundRequestId has been
//! authenticated and consumed by native_v3_getblock_correlation_v1. The
//! returned bodies are *candidates* until a trusted native-v3 daemon consensus
//! subsystem explicitly confirms them. Production does not instantiate this
//! planner. No legacy types::Block or protocol compatibility fallback.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use libp2p::PeerId;
use pulsedag_core::{
    block_envelope_v3::{validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    pow_v3::verify_v3_header_pow,
};

use super::direct_getblock_pending_v1::{
    DIRECT_GETBLOCK_REQUEST_TTL, MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL,
    MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER,
};

/// Resource limits for local sync scheduling, NOT native-v3 consensus rules.
pub const NATIVE_V3_SYNC_MAX_KNOWN_HASHES: usize = 256;
pub const NATIVE_V3_SYNC_MAX_STAGED_BLOCKS: usize = 64;
pub const NATIVE_V3_SYNC_MAX_STAGED_BYTES: usize = 32 * 1024 * 1024;
pub const NATIVE_V3_SYNC_MAX_RETRIES: u8 = 5;
const MAX_CANDIDATE_FRAME: usize = 2 * 1024 * 1024;
const RETRY_BASE_MILLIS: u64 = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeV3SyncError {
    InvalidIdentity,
    InvalidHash,
    InvalidHeaderDag,
    QueueFull,
    StagingFull,
    UnknownRequest,
    WrongAuthenticatedPeer,
    NotAwaitingBody,
    WrongBodyHash,
    InvalidCandidate,
    MissingConsensusParents,
    NotStagedForConsensus,
    NotInAcceptedStore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeV3SyncAssignment {
    /// Must be transported via exact Noise/libp2p GetBlock OutboundRequestId.
    pub hash: String,
    pub authenticated_peer: PeerId,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeV3SyncReport {
    pub queued: usize,
    pub in_flight: usize,
    pub awaiting_consensus: usize,
    pub deferred_for_ancestors: usize,
    pub accepted: usize,
    pub exhausted: usize,
}

impl NativeV3SyncReport {
    /// ONLY completeness of this explicitly enqueued set; not network
    /// readiness, a validated selected tip, or an economic consensus result.
    pub fn local_work_finished(self) -> bool {
        self.queued == 0
            && self.in_flight == 0
            && self.awaiting_consensus == 0
            && self.deferred_for_ancestors == 0
            && self.exhausted == 0
    }
}

#[derive(Debug)]
enum Phase {
    Waiting {
        not_before: Instant,
    },
    Fetching {
        peer: PeerId,
        deadline: Instant,
    },
    Staged {
        envelope: Box<BlockEnvelopeV3>,
        bytes: usize,
    },
    /// Body intentionally discarded under storage pressure; do not fetch
    /// again until independently accepted parent bodies unblock it.
    DeferredUntilParents { parents: Vec<String> },
    Exhausted,
}

#[derive(Debug)]
struct Item {
    phase: Phase,
    attempts: u8,
    last_peer: Option<PeerId>,
    /// Track providers per hash so an unhelpful A/B pair cannot exhaust all
    /// retries without trying C/D/E. Strictly bounded by MAX_RETRIES.
    attempted_peers: BTreeSet<String>,
    evictions: u8,
    /// Missing-parent work outranks descendant/frontier work.
    parent_priority: bool,
    verified_header_height: Option<u64>,
}

#[derive(Debug)]
pub struct NativeV3SyncRecovery {
    chain_id: String,
    trusted_genesis_hash: String,
    trusted_genesis_timestamp_ns: u64,
    items: BTreeMap<String, Item>,
    accepted_committed_total: usize,
}

fn canonical_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn cooldown(attempts: u8) -> Duration {
    // Bounded exponential backoff; attempts are capped by MAX_RETRIES.
    Duration::from_millis(RETRY_BASE_MILLIS * (1_u64 << attempts.min(5)))
}

impl NativeV3SyncRecovery {
    pub fn new(
        chain_id: &str,
        trusted_genesis_timestamp_ns: u64,
        trusted_genesis_hash: &str,
    ) -> Result<Self, NativeV3SyncError> {
        if chain_id.is_empty()
            || chain_id.len() > 96
            || !canonical_hash(trusted_genesis_hash)
            || dormant_native_genesis_envelope_v3_ns(chain_id, trusted_genesis_timestamp_ns)
                .map(|genesis| genesis.hash != trusted_genesis_hash)
                .unwrap_or(true)
        {
            return Err(NativeV3SyncError::InvalidIdentity);
        }
        Ok(Self {
            chain_id: chain_id.to_string(),
            trusted_genesis_hash: trusted_genesis_hash.to_string(),
            trusted_genesis_timestamp_ns,
            items: BTreeMap::new(),
            accepted_committed_total: 0,
        })
    }

    /// Untrusted tip inventories only enqueue a bounded candidate hash. They
    /// never change consensus, readiness, monetary supply or accepted state.
    pub fn enqueue_missing(&mut self, hash: &str, now: Instant) -> Result<bool, NativeV3SyncError> {
        self.enqueue_inner(hash, now, false)
    }

    fn enqueue_inner(
        &mut self,
        hash: &str,
        now: Instant,
        parent_priority: bool,
    ) -> Result<bool, NativeV3SyncError> {
        if !canonical_hash(hash) {
            return Err(NativeV3SyncError::InvalidHash);
        }
        if hash == self.trusted_genesis_hash {
            return Ok(false);
        }
        if let Some(existing) = self.items.get_mut(hash) {
            existing.parent_priority |= parent_priority;
            return Ok(false);
        }
        if self.items.len() >= NATIVE_V3_SYNC_MAX_KNOWN_HASHES {
            return Err(NativeV3SyncError::QueueFull);
        }
        self.items.insert(
            hash.to_string(),
            Item {
                phase: Phase::Waiting { not_before: now },
                attempts: 0,
                last_peer: None,
                attempted_peers: BTreeSet::new(),
                evictions: 0,
                parent_priority,
                verified_header_height: None,
            },
        );
        Ok(true)
    }

    /// Header-first fast path: independently authenticate the complete
    /// bounded PoW header DAG back to our frozen genesis, then schedule the
    /// full bodies in parent-before-child height order. A remote assertion of
    /// headers/tip alone NEVER implies selected-parent or monetary authority.
    ///
    /// Unlike recursive tip-only fetch, this enables concurrent body
    /// downloads as soon as verified native headers are available.
    pub fn enqueue_pow_verified_header_dag(
        &mut self,
        tip: &str,
        complete_headers: &BTreeMap<String, BlockHeaderV3>,
        now: Instant,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Result<usize, NativeV3SyncError> {
        let order = derive_dormant_native_v3_topological_evaluation(
            &self.chain_id,
            self.trusted_genesis_timestamp_ns,
            &self.trusted_genesis_hash,
            &[tip.to_owned()],
            complete_headers,
        )
        .map_err(|_| NativeV3SyncError::InvalidHeaderDag)?;
        let new_count = order
            .ordered_reachable_headers
            .iter()
            .filter(|hash| {
                hash.as_str() != self.trusted_genesis_hash
                    && !accepted_by_daemon(hash)
                    && !self.items.contains_key(*hash)
            })
            .count();
        if self.items.len() + new_count > NATIVE_V3_SYNC_MAX_KNOWN_HASHES {
            return Err(NativeV3SyncError::QueueFull);
        }
        let mut added = 0;
        for hash in order.ordered_reachable_headers.iter().skip(1) {
            if accepted_by_daemon(hash) {
                continue;
            }
            if self.enqueue_inner(hash, now, false)? {
                added += 1;
            }
            if let Some(item) = self.items.get_mut(hash) {
                item.verified_header_height = complete_headers.get(hash).map(|header| header.height);
            }
        }
        Ok(added)
    }

    /// Select which hash to request from which *already authenticated,
    /// currently connected* Noise peer. The transport caller must invoke the
    /// bounded correlation book and call `dispatch_failed` if send fails.
    /// This reserves each hash before returning: two ticks cannot emit the
    /// same in-flight block request.
    pub fn plan_fetches(
        &mut self,
        currently_connected_noise_peers: &[PeerId],
        now: Instant,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Vec<NativeV3SyncAssignment> {
        self.expire(now);
        // A deferred body must never refetch repeatedly while any ancestor
        // remains unaccepted. Release only once the trusted daemon store
        // reports all immediate parents committed (not just PoW staged).
        for item in self.items.values_mut() {
            if let Phase::DeferredUntilParents { parents } = &item.phase {
                if parents
                    .iter()
                    .all(|parent| parent == &self.trusted_genesis_hash || accepted_by_daemon(parent))
                {
                    item.phase = Phase::Waiting { not_before: now };
                }
            }
        }
        let mut peers = currently_connected_noise_peers.to_vec();
        peers.sort_unstable_by_key(ToString::to_string);
        peers.dedup();
        if peers.is_empty() {
            return Vec::new();
        }

        let mut per_peer = BTreeMap::<String, usize>::new();
        let mut active = 0usize;
        for item in self.items.values() {
            if let Phase::Fetching { peer, .. } = &item.phase {
                active += 1;
                *per_peer.entry(peer.to_string()).or_default() += 1;
            }
        }
        let mut ready = self
            .items
            .iter()
            .filter_map(|(hash, item)| match &item.phase {
                Phase::Waiting { not_before } if now >= *not_before => {
                    Some((!item.parent_priority, item.verified_header_height.unwrap_or(u64::MAX), hash.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        ready.sort_unstable();

        let mut planned = Vec::new();
        for (_, hash) in ready {
            if active >= MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL {
                break;
            }
            let Some(item) = self.items.get(&hash) else {
                continue;
            };
            let previous = item.last_peer;
            let mut ranked = peers
                .iter()
                .filter(|peer| {
                    per_peer.get(&peer.to_string()).copied().unwrap_or(0)
                        < MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER
                })
                .copied()
                .collect::<Vec<_>>();
            ranked.sort_by_key(|peer| {
                (
                    item.attempted_peers.contains(&peer.to_string()),
                    Some(*peer) == previous,
                    per_peer.get(&peer.to_string()).copied().unwrap_or(0),
                    peer.to_string(),
                )
            });
            let Some(peer) = ranked.first().copied() else {
                break;
            };
            let item = self.items.get_mut(&hash).expect("queued hash exists");
            item.attempts = item.attempts.saturating_add(1);
            item.last_peer = Some(peer);
            item.attempted_peers.insert(peer.to_string());
            item.phase = Phase::Fetching {
                peer,
                deadline: now + DIRECT_GETBLOCK_REQUEST_TTL,
            };
            *per_peer.entry(peer.to_string()).or_default() += 1;
            active += 1;
            planned.push(NativeV3SyncAssignment {
                hash,
                authenticated_peer: peer,
            });
        }
        planned
    }

    /// A deep tip-only chain can fill all 64 staged slots before its oldest
    /// missing ancestor arrives. Evict only an *unready, higher* descendant
    /// and retain a lightweight parent dependency until trusted consensus
    /// commits its parents. Never use an unbounded cache or keep repeating
    /// poisoned/rejected bodies as an alleged recovery success.
    fn reclaim_staging_for_ancestor(
        &mut self,
        incoming_height: u64,
        incoming_bytes: usize,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Result<(), NativeV3SyncError> {
        let (mut staged_count, mut staged_bytes) =
            self.items
                .values()
                .fold((0usize, 0usize), |(n, total), item| match &item.phase {
                    Phase::Staged { bytes, .. } => (n + 1, total + bytes),
                    _ => (n, total),
                });

        while staged_count >= NATIVE_V3_SYNC_MAX_STAGED_BLOCKS
            || staged_bytes + incoming_bytes > NATIVE_V3_SYNC_MAX_STAGED_BYTES
        {
            let victim = self
                .items
                .iter()
                .filter_map(|(hash, item)| {
                    if let Phase::Staged { envelope, bytes } = &item.phase {
                        if envelope.header.height > incoming_height
                            && envelope.header.parents.iter().any(|parent| {
                                parent != &self.trusted_genesis_hash && !accepted_by_daemon(parent)
                            })
                        {
                            return Some((
                                envelope.header.height,
                                hash.clone(),
                                *bytes,
                                envelope.header.parents.clone(),
                            ));
                        }
                    }
                    None
                })
                .max_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
            let Some((_, hash, released_bytes, parents)) = victim else {
                return Err(NativeV3SyncError::StagingFull);
            };
            let item = self
                .items
                .get_mut(&hash)
                .expect("verified victim is still staged");
            item.evictions = item.evictions.saturating_add(1);
            item.attempts = 0;
            item.parent_priority = false;
            item.phase = if item.evictions >= 4 {
                Phase::Exhausted
            } else {
                Phase::DeferredUntilParents { parents }
            };
            staged_count -= 1;
            staged_bytes -= released_bytes;
        }
        Ok(())
    }

    fn reschedule(item: &mut Item, now: Instant) {
        if item.attempts >= NATIVE_V3_SYNC_MAX_RETRIES {
            item.phase = Phase::Exhausted;
        } else {
            item.phase = Phase::Waiting {
                not_before: now + cooldown(item.attempts),
            };
        }
    }

    /// Called by the authenticated Noise transport on a timeout, explicit
    /// Missing response, or failed outbound send. A remote peer never gets
    /// authority to mark a block absent across other possible providers.
    pub fn dispatch_failed(
        &mut self,
        hash: &str,
        actual_peer: &PeerId,
        now: Instant,
    ) -> Result<(), NativeV3SyncError> {
        let item = self
            .items
            .get_mut(hash)
            .ok_or(NativeV3SyncError::UnknownRequest)?;
        match &item.phase {
            Phase::Fetching { peer, .. } if peer == actual_peer => {
                Self::reschedule(item, now);
                Ok(())
            }
            Phase::Fetching { .. } => Err(NativeV3SyncError::WrongAuthenticatedPeer),
            _ => Err(NativeV3SyncError::NotAwaitingBody),
        }
    }

    /// Called ONLY after the real Noise correlation book verifies the exact
    /// OutboundRequestId, authenticated responder, request id and hash.
    /// Rechecks the typed candidate, but DOES NOT give it consensus authority.
    pub fn on_candidate(
        &mut self,
        requested_hash: &str,
        actual_peer: &PeerId,
        candidate: Option<BlockEnvelopeV3>,
        now: Instant,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Result<(), NativeV3SyncError> {
        let item = self
            .items
            .get(requested_hash)
            .ok_or(NativeV3SyncError::UnknownRequest)?;
        match &item.phase {
            Phase::Fetching { peer, deadline } if peer == actual_peer && now < *deadline => {}
            Phase::Fetching { peer, .. } if peer != actual_peer => {
                return Err(NativeV3SyncError::WrongAuthenticatedPeer)
            }
            _ => return Err(NativeV3SyncError::NotAwaitingBody),
        }
        let Some(envelope) = candidate else {
            return self.dispatch_failed(requested_hash, actual_peer, now);
        };
        if envelope.hash != requested_hash || envelope.chain_id != self.chain_id {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::WrongBodyHash);
        }
        if envelope.header.height == 0
            || envelope.header.parents.is_empty()
            || envelope.header.parents.len() > 64
            || envelope
                .header
                .parents
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || envelope
                .header
                .parents
                .iter()
                .any(|hash| hash == requested_hash)
        {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::InvalidCandidate);
        }
        if validate_dormant_native_v3_monetary_body_preflight(&envelope, &self.chain_id).is_err()
            || verify_v3_header_pow(&envelope.header, &self.chain_id).is_err()
        {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::InvalidCandidate);
        }
        let bytes = match serde_json::to_vec(&envelope) {
            Ok(raw) => raw.len(),
            Err(_) => {
                let _ = self.dispatch_failed(requested_hash, actual_peer, now);
                return Err(NativeV3SyncError::InvalidCandidate);
            }
        };
        if bytes > MAX_CANDIDATE_FRAME {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::StagingFull);
        }
        // Authenticate the complete referenced parent set before any cache
        // mutation. Never let a malicious candidate allocate beyond the
        // bounded queue, even if a victim would otherwise be evicted.
        let new_parents = envelope
            .header
            .parents
            .iter()
            .filter(|hash| {
                hash.as_str() != self.trusted_genesis_hash
                    && !accepted_by_daemon(hash)
                    && !self.items.contains_key(*hash)
            })
            .collect::<Vec<_>>();
        if self.items.len() + new_parents.len() > NATIVE_V3_SYNC_MAX_KNOWN_HASHES
            || new_parents.iter().any(|hash| !canonical_hash(hash))
        {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::QueueFull);
        }
        for parent in &envelope.header.parents {
            if !accepted_by_daemon(parent) {
                self.enqueue_inner(parent, now, true)?;
            }
        }

        if self
            .reclaim_staging_for_ancestor(envelope.header.height, bytes, accepted_by_daemon)
            .is_err()
        {
            // Staging pressure is NOT evidence that the authenticated peer
            // failed. Drop this valid body, retain only its parent dependency
            // and refetch with a fresh wire ticket once the real daemon makes
            // progress. Do not burn the network retry budget on RAM pressure.
            let parents = envelope.header.parents.clone();
            let all_accepted = parents
                .iter()
                .all(|parent| self.parent_consensus_accepted(parent, accepted_by_daemon));
            let item = self
                .items
                .get_mut(requested_hash)
                .ok_or(NativeV3SyncError::UnknownRequest)?;
            item.attempts = item.attempts.saturating_sub(1);
            item.phase = if all_accepted {
                Phase::Waiting {
                    not_before: now + Duration::from_secs(2),
                }
            } else {
                Phase::DeferredUntilParents { parents }
            };
            return Ok(());
        }

        let item = self
            .items
            .get_mut(requested_hash)
            .ok_or(NativeV3SyncError::UnknownRequest)?;
        item.phase = Phase::Staged {
            envelope: Box::new(envelope),
            bytes,
        };
        Ok(())
    }

    /// Bodies are exposed parent-first only when all parents have already
    /// been ACKed by the native-v3 consensus authority. `Staged` is NOT a
    /// positive monetary/UTXO/blue-work acceptance verdict.
    pub fn ready_for_consensus(
        &self,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Vec<BlockEnvelopeV3> {
        let mut ready = self
            .items
            .values()
            .filter_map(|item| match &item.phase {
                Phase::Staged { envelope, .. }
                    if envelope
                        .header
                        .parents
                        .iter()
                        .all(|parent| self.parent_consensus_accepted(parent, accepted_by_daemon)) =>
                {
                    Some((**envelope).clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        ready.sort_unstable_by(|a, b| {
            a.header
                .height
                .cmp(&b.header.height)
                .then_with(|| a.hash.cmp(&b.hash))
        });
        ready
    }

    fn parent_consensus_accepted(
        &self,
        hash: &str,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> bool {
        hash == self.trusted_genesis_hash || accepted_by_daemon(hash)
    }

    /// Only after the INDEPENDENT native-v3 monetary consensus and database
    /// transaction have atomically committed this hash AND its parents.
    /// The authoritative store lookup is a separate requirement, not a
    /// remote self-asserted peer, PoW-only candidate or local queued flag.
    ///
    /// Accepted entries are retired. Their parent membership must thereafter
    /// be checked against the trusted daemon store, so >256 successful
    /// blocks cannot deadlock the bounded pending queue.
    pub fn consensus_committed(
        &mut self,
        hash: &str,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Result<(), NativeV3SyncError> {
        let item = self
            .items
            .get(hash)
            .ok_or(NativeV3SyncError::NotStagedForConsensus)?;
        let Phase::Staged { envelope, .. } = &item.phase else {
            return Err(NativeV3SyncError::NotStagedForConsensus);
        };
        if !accepted_by_daemon(hash) {
            return Err(NativeV3SyncError::NotInAcceptedStore);
        }
        if !envelope
            .header
            .parents
            .iter()
            .all(|parent| self.parent_consensus_accepted(parent, accepted_by_daemon))
        {
            return Err(NativeV3SyncError::MissingConsensusParents);
        }
        self.items.remove(hash);
        self.accepted_committed_total = self.accepted_committed_total.saturating_add(1);
        Ok(())
    }

    pub fn peer_disconnected(&mut self, authenticated_peer: &PeerId, now: Instant) -> usize {
        let mut count = 0;
        for item in self.items.values_mut() {
            if let Phase::Fetching { peer, .. } = &item.phase {
                if peer == authenticated_peer {
                    Self::reschedule(item, now);
                    count += 1;
                }
            }
        }
        count
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        let mut expired = 0;
        for item in self.items.values_mut() {
            if let Phase::Fetching { deadline, .. } = &item.phase {
                if *deadline <= now {
                    Self::reschedule(item, now);
                    expired += 1;
                }
            }
        }
        expired
    }

    pub fn report(&self) -> NativeV3SyncReport {
        let mut report = NativeV3SyncReport::default();
        for item in self.items.values() {
            match &item.phase {
                Phase::Waiting { .. } => report.queued += 1,
                Phase::Fetching { .. } => report.in_flight += 1,
                Phase::Staged { .. } => report.awaiting_consensus += 1,
                Phase::DeferredUntilParents { .. } => report.deferred_for_ancestors += 1,
                Phase::Exhausted => report.exhausted += 1,
            }
        }
        report.accepted = self.accepted_committed_total;
        report
    }

    /// No silently green state on exhausted work, even when all peers dropped
    /// and no messages remain in flight.
    pub fn has_blockers(&self) -> bool {
        !self.report().local_work_finished()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "native-first-public-network-sync-planner-test-only";
    const GENESIS_NS: u64 = 1_800_000_000_500_000_000;

    fn peer() -> PeerId {
        PeerId::from(libp2p::identity::Keypair::generate_ed25519().public())
    }

    fn mine(parents: Vec<String>, height: u64, salt: u64) -> BlockEnvelopeV3 {
        let txs = vec![
            build_reward_claim_transaction_v3("pulse1recovery-test", salt, CHAIN).unwrap(),
        ];
        let mut parents = parents;
        parents.sort();
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns: GENESIS_NS + height * 1_000_000_000 + salt,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: format!("{:064x}", salt + 1),
            blue_score: height,
            height,
        };
        for nonce in 0..4096 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                return build_block_envelope_v3(CHAIN, header, txs).unwrap();
            }
        }
        panic!("easy-target KHeavyHash test fixture must find PoW");
    }

    // Simulated trusted store ONLY in tests. The production daemon must
    // independently perform atomic GD-08/UTXO/reward validation and persistence.
    fn test_oracle_commit_ready(
        sync: &mut NativeV3SyncRecovery,
        accepted: &mut BTreeSet<String>,
    ) {
        loop {
            let ready = sync.ready_for_consensus(&|hash| accepted.contains(hash));
            if ready.is_empty() {
                break;
            }
            for block in ready {
                accepted.insert(block.hash.clone());
                sync.consensus_committed(&block.hash, &|hash| accepted.contains(hash))
                    .unwrap();
            }
        }
    }

    #[test]
    fn headers_first_thirty_two_real_pow_bodies_use_twenty_parallel_peer_slots() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut all = BTreeMap::new();
        let mut spine = genesis.hash.clone();
        let mut side = None;
        for height in 1..=31_u64 {
            let old = spine.clone();
            let mut parents = vec![old.clone()];
            if height == 13 {
                parents.push(side.clone().unwrap());
            }
            let block = mine(parents, height, height);
            spine = block.hash.clone();
            all.insert(block.hash.clone(), block);
            if height == 12 {
                let fork = mine(vec![old], height, 100 + height);
                side = Some(fork.hash.clone());
                all.insert(fork.hash.clone(), fork);
            }
        }
        assert_eq!(all.len(), 32);
        assert!(all.values().any(|b| b.header.parents.len() == 2));
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let mut accepted = BTreeSet::new();
        let now = Instant::now();
        let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header)]);
        for (hash, block) in &all {
            headers.insert(hash.clone(), block.header.clone());
        }
        assert_eq!(
            sync.enqueue_pow_verified_header_dag(
                &spine, &headers, now, &|hash| accepted.contains(hash)
            ),
            Ok(32),
            "native KHeavyHash/ancestor preflight should schedule all 32 bodies"
        );
        let providers = vec![peer(), peer(), peer(), peer(), peer()];
        let mut ticks = 0u64;
        let mut injected_timeout = false;
        let mut peak_parallel = 0usize;

        while sync.report().accepted < 32 && ticks < 512 {
            let clock = now + Duration::from_secs(ticks);
            let assignments =
                sync.plan_fetches(&providers, clock, &|hash| accepted.contains(hash));
            peak_parallel = peak_parallel.max(assignments.len());
            for assignment in assignments {
                if !injected_timeout {
                    injected_timeout = true;
                    sync.dispatch_failed(
                        &assignment.hash,
                        &assignment.authenticated_peer,
                        clock,
                    )
                    .unwrap();
                    continue;
                }
                sync.on_candidate(
                    &assignment.hash,
                    &assignment.authenticated_peer,
                    Some(all[&assignment.hash].clone()),
                    clock,
                    &|hash| accepted.contains(hash),
                )
                .unwrap();
            }
            test_oracle_commit_ready(&mut sync, &mut accepted);
            ticks += 1;
        }
        assert!(injected_timeout);
        assert_eq!(peak_parallel, 20, "four slots each across five peers");
        assert_eq!(sync.report().accepted, 32);
        assert_eq!(accepted.len(), 32);
        assert!(sync.report().local_work_finished());
        assert!(!sync.has_blockers());
    }

    #[test]
    fn foreign_missing_or_corrupt_pow_header_dag_cannot_enqueue_bodies() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let block = mine(vec![genesis.hash.clone()], 1, 1);
        let now = Instant::now();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let mut headers = BTreeMap::from([
            (genesis.hash.clone(), genesis.header.clone()),
            (block.hash.clone(), block.header.clone()),
        ]);
        assert_eq!(
            sync.enqueue_pow_verified_header_dag(&block.hash, &headers, now, &|_| false),
            Ok(1)
        );
        assert!(matches!(
            NativeV3SyncRecovery::new(CHAIN, GENESIS_NS + 1, &genesis.hash),
            Err(NativeV3SyncError::InvalidIdentity)
        ));
        let mut other = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        headers.remove(&genesis.hash);
        assert_eq!(
            other.enqueue_pow_verified_header_dag(&block.hash, &headers, now, &|_| false),
            Err(NativeV3SyncError::InvalidHeaderDag)
        );
        assert_eq!(other.report(), NativeV3SyncReport::default());
        headers.insert(genesis.hash.clone(), genesis.header);
        headers.get_mut(&block.hash).unwrap().nonce ^= 1;
        assert_eq!(
            other.enqueue_pow_verified_header_dag(&block.hash, &headers, now, &|_| false),
            Err(NativeV3SyncError::InvalidHeaderDag)
        );
        assert_eq!(other.report(), NativeV3SyncReport::default());
    }

    #[test]
    fn spoofed_peer_or_hash_never_promotes_candidate_without_store_acceptance() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let block = mine(vec![genesis.hash.clone()], 1, 1);
        let now = Instant::now();
        let honest = peer();
        let attacker = peer();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        sync.enqueue_missing(&block.hash, now).unwrap();
        assert_eq!(sync.plan_fetches(&[honest], now, &|_| false).len(), 1);
        assert_eq!(
            sync.on_candidate(&block.hash, &attacker, Some(block.clone()), now, &|_| false),
            Err(NativeV3SyncError::WrongAuthenticatedPeer)
        );
        assert_eq!(sync.report().in_flight, 1);
        let mut bad = block.clone();
        bad.hash = "f".repeat(64);
        assert_eq!(
            sync.on_candidate(&block.hash, &honest, Some(bad), now, &|_| false),
            Err(NativeV3SyncError::WrongBodyHash)
        );
        assert_eq!(sync.report().in_flight, 0);
        let later = now + Duration::from_secs(2);
        assert_eq!(sync.plan_fetches(&[honest], later, &|_| false).len(), 1);
        sync.on_candidate(&block.hash, &honest, Some(block.clone()), later, &|_| false)
            .unwrap();
        assert_eq!(sync.ready_for_consensus(&|_| false).len(), 1);
        assert_eq!(
            sync.consensus_committed(&block.hash, &|_| false),
            Err(NativeV3SyncError::NotInAcceptedStore)
        );
        let accepted = BTreeSet::from([block.hash.clone()]);
        sync.consensus_committed(&block.hash, &|hash| accepted.contains(hash))
            .unwrap();
        assert_eq!(sync.report().accepted, 1);
        assert!(sync.report().local_work_finished());
    }

    #[test]
    fn missing_parent_is_not_conflated_with_accepted_state() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let parent = mine(vec![genesis.hash.clone()], 1, 10);
        let child = mine(vec![parent.hash.clone()], 2, 20);
        let mut accepted = BTreeSet::new();
        let now = Instant::now();
        let provider = peer();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        sync.enqueue_missing(&child.hash, now).unwrap();
        assert_eq!(sync.plan_fetches(&[provider], now, &|_| false).len(), 1);
        sync.on_candidate(
            &child.hash,
            &provider,
            Some(child.clone()),
            now,
            &|hash| accepted.contains(hash),
        )
        .unwrap();
        assert!(sync.ready_for_consensus(&|hash| accepted.contains(hash)).is_empty());
        // Even an incorrectly asserted accepted child cannot bypass its parent.
        assert_eq!(
            sync.consensus_committed(&child.hash, &|hash| hash == child.hash),
            Err(NativeV3SyncError::MissingConsensusParents)
        );
        assert_eq!(
            sync.plan_fetches(&[provider], now, &|_| false)[0].hash,
            parent.hash
        );
        sync.on_candidate(
            &parent.hash,
            &provider,
            Some(parent.clone()),
            now,
            &|hash| accepted.contains(hash),
        )
        .unwrap();
        assert_eq!(
            sync.ready_for_consensus(&|hash| accepted.contains(hash))[0].hash,
            parent.hash
        );
        test_oracle_commit_ready(&mut sync, &mut accepted);
        assert_eq!(accepted.len(), 2);
        assert!(!sync.has_blockers());
    }

    #[test]
    fn two_unhelpful_providers_cannot_prevent_the_third_from_serving() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let block = mine(vec![genesis.hash.clone()], 1, 4);
        let mut providers = vec![peer(), peer(), peer(), peer(), peer()];
        providers.sort_unstable_by_key(ToString::to_string);
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let now = Instant::now();
        sync.enqueue_missing(&block.hash, now).unwrap();
        let mut observed = Vec::new();
        for attempt in 0..3u64 {
            let clock = now + Duration::from_secs(attempt * 3);
            let assignments = sync.plan_fetches(&providers, clock, &|_| false);
            assert_eq!(assignments.len(), 1);
            let provider = assignments[0].authenticated_peer;
            observed.push(provider);
            if attempt < 2 {
                sync.on_candidate(&block.hash, &provider, None, clock, &|_| false)
                    .unwrap();
            } else {
                sync.on_candidate(&block.hash, &provider, Some(block.clone()), clock, &|_| false)
                    .unwrap();
            }
        }
        assert_eq!(observed, providers[..3]);
        assert_eq!(sync.ready_for_consensus(&|_| false).len(), 1);
        let accepted = BTreeSet::from([block.hash.clone()]);
        sync.consensus_committed(&block.hash, &|hash| accepted.contains(hash))
            .unwrap();
        assert!(!sync.has_blockers());
    }

    #[test]
    fn sixty_five_tip_only_blocks_reclaim_staging_for_oldest_ancestor() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut all = BTreeMap::new();
        let mut tip = genesis.hash.clone();
        for height in 1..=65u64 {
            let block = mine(vec![tip.clone()], height, height);
            tip = block.hash.clone();
            all.insert(block.hash.clone(), block);
        }
        let mut accepted = BTreeSet::new();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let now = Instant::now();
        let provider = peer();
        sync.enqueue_missing(&tip, now).unwrap();
        let mut witnessed_deferred = false;
        for t in 0..512u64 {
            let clock = now + Duration::from_secs(t);
            let work = sync.plan_fetches(&[provider], clock, &|hash| accepted.contains(hash));
            for request in work {
                sync.on_candidate(
                    &request.hash,
                    &request.authenticated_peer,
                    Some(all[&request.hash].clone()),
                    clock,
                    &|hash| accepted.contains(hash),
                )
                .unwrap();
            }
            witnessed_deferred |= sync.report().deferred_for_ancestors > 0;
            test_oracle_commit_ready(&mut sync, &mut accepted);
            if sync.report().accepted == 65 {
                break;
            }
        }
        assert!(witnessed_deferred, "tip-only deep recovery must exercise eviction");
        assert_eq!(accepted.len(), 65);
        assert_eq!(sync.report().accepted, 65);
        assert!(!sync.has_blockers());
    }

    #[test]
    fn more_than_256_sequential_commits_do_not_fill_the_pending_queue() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut tip = genesis.hash.clone();
        let mut accepted = BTreeSet::new();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let now = Instant::now();
        let provider = peer();
        for height in 1..=300u64 {
            let block = mine(vec![tip.clone()], height, height);
            tip = block.hash.clone();
            let clock = now + Duration::from_secs(height);
            sync.enqueue_missing(&block.hash, clock).unwrap();
            assert_eq!(
                sync.plan_fetches(&[provider], clock, &|hash| accepted.contains(hash))
                    .len(),
                1
            );
            sync.on_candidate(
                &block.hash,
                &provider,
                Some(block.clone()),
                clock,
                &|hash| accepted.contains(hash),
            )
            .unwrap();
            test_oracle_commit_ready(&mut sync, &mut accepted);
            assert_eq!(sync.items.len(), 0, "accepted bodies retire from the queue");
        }
        assert_eq!(accepted.len(), 300);
        assert_eq!(sync.report().accepted, 300);
        assert!(sync.report().local_work_finished());
    }

    #[test]
    fn exhausted_fetch_timeout_never_declares_network_ready() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let now = Instant::now();
        let a = peer();
        let b = peer();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let hash = "a".repeat(64);
        sync.enqueue_missing(&hash, now).unwrap();
        let first = sync.plan_fetches(&[a, b], now, &|_| false);
        assert_eq!(first.len(), 1);
        sync.dispatch_failed(&hash, &first[0].authenticated_peer, now)
            .unwrap();
        assert!(sync.plan_fetches(&[a, b], now, &|_| false).is_empty());
        let second = sync.plan_fetches(&[a, b], now + Duration::from_secs(1), &|_| false);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].authenticated_peer, second[0].authenticated_peer);
        let mut current = second;
        for attempt in 2..=NATIVE_V3_SYNC_MAX_RETRIES {
            let clock = now + Duration::from_secs(u64::from(attempt) * 12);
            sync.dispatch_failed(&hash, &current[0].authenticated_peer, clock)
                .unwrap();
            if attempt < NATIVE_V3_SYNC_MAX_RETRIES {
                current = sync.plan_fetches(
                    &[a, b],
                    clock + Duration::from_secs(10),
                    &|_| false,
                );
                assert_eq!(current.len(), 1);
            }
        }
        assert_eq!(sync.report().exhausted, 1);
        assert!(sync.has_blockers());
        assert!(sync
            .plan_fetches(&[a, b], now + Duration::from_secs(300), &|_| false)
            .is_empty());
    }

    #[test]
    fn peer_disconnect_and_queue_caps_fail_closed() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, GENESIS_NS, &genesis.hash).unwrap();
        let now = Instant::now();
        let first = peer();
        let other = peer();
        sync.enqueue_missing(&"a".repeat(64), now).unwrap();
        assert_eq!(sync.plan_fetches(&[first], now, &|_| false).len(), 1);
        assert_eq!(sync.peer_disconnected(&other, now), 0);
        assert_eq!(sync.peer_disconnected(&first, now), 1);
        assert!(sync.has_blockers());
        let again = sync.plan_fetches(
            &[other],
            now + Duration::from_secs(1),
            &|_| false,
        );
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].authenticated_peer, other);
        assert_eq!(
            sync.enqueue_missing("not-a-hash", now),
            Err(NativeV3SyncError::InvalidHash)
        );
        for i in 0..NATIVE_V3_SYNC_MAX_KNOWN_HASHES {
            let hash = format!("{i:064x}");
            let _ = sync.enqueue_missing(&hash, now);
        }
        assert_eq!(sync.items.len(), NATIVE_V3_SYNC_MAX_KNOWN_HASHES);
        assert_eq!(
            sync.enqueue_missing(&"f".repeat(64), now),
            Err(NativeV3SyncError::QueueFull)
        );
    }
}
