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
    QueueFull,
    StagingFull,
    UnknownRequest,
    WrongAuthenticatedPeer,
    NotAwaitingBody,
    WrongBodyHash,
    InvalidCandidate,
    MissingConsensusParents,
    NotStagedForConsensus,
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
    pub accepted: usize,
    pub exhausted: usize,
}

impl NativeV3SyncReport {
    /// ONLY completeness of this explicitly enqueued set; not network
    /// readiness, a validated selected tip, or an economic consensus result.
    pub fn local_work_finished(self) -> bool {
        self.queued == 0 && self.in_flight == 0 && self.awaiting_consensus == 0
            && self.exhausted == 0
    }
}

#[derive(Debug)]
enum Phase {
    Waiting { not_before: Instant },
    Fetching { peer: PeerId, deadline: Instant },
    Staged { envelope: BlockEnvelopeV3, bytes: usize },
    ConsensusAccepted,
    Exhausted,
}

#[derive(Debug)]
struct Item {
    phase: Phase,
    attempts: u8,
    last_peer: Option<PeerId>,
    /// Missing-parent work outranks descendant/frontier work.
    parent_priority: bool,
}

#[derive(Debug)]
pub struct NativeV3SyncRecovery {
    chain_id: String,
    trusted_genesis_hash: String,
    items: BTreeMap<String, Item>,
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
        trusted_genesis_hash: &str,
    ) -> Result<Self, NativeV3SyncError> {
        if chain_id.is_empty()
            || chain_id.len() > 96
            || !canonical_hash(trusted_genesis_hash)
        {
            return Err(NativeV3SyncError::InvalidIdentity);
        }
        Ok(Self {
            chain_id: chain_id.to_string(),
            trusted_genesis_hash: trusted_genesis_hash.to_string(),
            items: BTreeMap::new(),
        })
    }

    /// Untrusted tip inventories only enqueue a bounded candidate hash. They
    /// never change consensus, readiness, monetary supply or accepted state.
    pub fn enqueue_missing(
        &mut self,
        hash: &str,
        now: Instant,
    ) -> Result<bool, NativeV3SyncError> {
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
                parent_priority,
            },
        );
        Ok(true)
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
    ) -> Vec<NativeV3SyncAssignment> {
        self.expire(now);
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
                    Some((!item.parent_priority, hash.clone()))
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
        let item = self.items.get_mut(hash).ok_or(NativeV3SyncError::UnknownRequest)?;
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
    ) -> Result<(), NativeV3SyncError> {
        let item = self.items.get(requested_hash).ok_or(NativeV3SyncError::UnknownRequest)?;
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
            || envelope.header.parents.iter().any(|hash| hash == requested_hash)
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
        let (staged_count, staged_bytes) =
            self.items.values().fold((0usize, 0usize), |(n, total), item| {
                match &item.phase {
                    Phase::Staged { bytes, .. } => (n + 1, total + bytes),
                    _ => (n, total),
                }
            });
        if staged_count >= NATIVE_V3_SYNC_MAX_STAGED_BLOCKS
            || staged_bytes + bytes > NATIVE_V3_SYNC_MAX_STAGED_BYTES
        {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::StagingFull);
        }
        let new_parents = envelope
            .header
            .parents
            .iter()
            .filter(|hash| {
                hash.as_str() != self.trusted_genesis_hash && !self.items.contains_key(*hash)
            })
            .collect::<Vec<_>>();
        if self.items.len() + new_parents.len() > NATIVE_V3_SYNC_MAX_KNOWN_HASHES
            || new_parents.iter().any(|hash| !canonical_hash(hash))
        {
            let _ = self.dispatch_failed(requested_hash, actual_peer, now);
            return Err(NativeV3SyncError::QueueFull);
        }
        for parent in &envelope.header.parents {
            self.enqueue_inner(parent, now, true)?;
        }
        let item = self.items.get_mut(requested_hash).ok_or(NativeV3SyncError::UnknownRequest)?;
        item.phase = Phase::Staged { envelope, bytes };
        Ok(())
    }

    /// Bodies are exposed parent-first only when all parents have already
    /// been ACKed by the native-v3 consensus authority. `Staged` is NOT a
    /// positive monetary/UTXO/blue-work acceptance verdict.
    pub fn ready_for_consensus(&self) -> Vec<BlockEnvelopeV3> {
        let mut ready = self
            .items
            .values()
            .filter_map(|item| match &item.phase {
                Phase::Staged { envelope, .. }
                    if envelope
                        .header
                        .parents
                        .iter()
                        .all(|parent| self.parent_consensus_accepted(parent)) =>
                {
                    Some(envelope.clone())
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

    fn parent_consensus_accepted(&self, hash: &str) -> bool {
        hash == self.trusted_genesis_hash
            || self
                .items
                .get(hash)
                .is_some_and(|item| matches!(&item.phase, Phase::ConsensusAccepted))
    }

    /// Invoke ONLY from the independently verified native-v3 daemon consensus
    /// callback **after** committing GD-08/UTXO/rewards atomically. This API
    /// does not implement or assert those checks; external user input or P2P
    /// messages must NEVER call it.
    pub fn consensus_committed(&mut self, hash: &str) -> Result<(), NativeV3SyncError> {
        let item = self.items.get(hash).ok_or(NativeV3SyncError::NotStagedForConsensus)?;
        let Phase::Staged { envelope, .. } = &item.phase else {
            return Err(NativeV3SyncError::NotStagedForConsensus);
        };
        if !envelope
            .header
            .parents
            .iter()
            .all(|parent| self.parent_consensus_accepted(parent))
        {
            return Err(NativeV3SyncError::MissingConsensusParents);
        }
        self.items
            .get_mut(hash)
            .expect("staged candidate exists")
            .phase = Phase::ConsensusAccepted;
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
                Phase::ConsensusAccepted => report.accepted += 1,
                Phase::Exhausted => report.exhausted += 1,
            }
        }
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
        panic!("test PoW must be easy enough");
    }

    #[test]
    fn authentic_native_v3_thirty_two_candidate_recovery_tracks_missing_parents() {
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
            let b = mine(parents, height, height);
            spine = b.hash.clone();
            all.insert(b.hash.clone(), b);
            if height == 12 {
                let fork = mine(vec![old], height, 100 + height);
                side = Some(fork.hash.clone());
                all.insert(fork.hash.clone(), fork);
            }
        }
        assert_eq!(all.len(), 32);
        assert!(all.values().any(|b| b.header.parents.len() == 2));
        let mut sync = NativeV3SyncRecovery::new(CHAIN, &genesis.hash).unwrap();
        let now = Instant::now();
        sync.enqueue_missing(&spine, now).unwrap();
        let providers = vec![peer(), peer(), peer(), peer(), peer()];
        let mut ticks = 0u64;
        let mut had_timeout = false;

        while sync.report().accepted < 32 && ticks < 512 {
            let clock = now + Duration::from_secs(ticks);
            let assignments = sync.plan_fetches(&providers, clock);
            for assignment in assignments {
                if !had_timeout {
                    // Genuine timeout on the first provider, then peer
                    // alternation must preserve the missing hash.
                    had_timeout = true;
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
                )
                .unwrap();
            }
            loop {
                let ready = sync.ready_for_consensus();
                if ready.is_empty() {
                    break;
                }
                for b in ready {
                    // THIS IS ONLY A TEST ORACLE FOR THE SCHEDULER. Real
                    // daemon consensus GD-08 is deliberately NOT wired.
                    sync.consensus_committed(&b.hash).unwrap();
                }
            }
            ticks += 1;
        }
        assert!(had_timeout);
        assert_eq!(sync.report().accepted, 32);
        assert!(sync.report().local_work_finished());
        assert!(!sync.has_blockers());
    }

    #[test]
    fn forged_peer_and_misbound_hash_never_clear_an_inflight_candidate() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let candidate = mine(vec![genesis.hash.clone()], 1, 1);
        let now = Instant::now();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, &genesis.hash).unwrap();
        sync.enqueue_missing(&candidate.hash, now).unwrap();
        let honest = peer();
        let attacker = peer();
        let assigned = sync.plan_fetches(&[honest], now);
        assert_eq!(assigned.len(), 1);
        assert_eq!(
            sync.on_candidate(&candidate.hash, &attacker, Some(candidate.clone()), now),
            Err(NativeV3SyncError::WrongAuthenticatedPeer)
        );
        assert_eq!(sync.report().in_flight, 1);
        let mut wrong = candidate.clone();
        wrong.hash = "f".repeat(64);
        assert_eq!(
            sync.on_candidate(&candidate.hash, &honest, Some(wrong), now),
            Err(NativeV3SyncError::WrongBodyHash)
        );
        assert_eq!(sync.report().in_flight, 0);
        assert_eq!(sync.report().queued, 1);
        // Own authenticated malformed response consumed its wire ticket.
        // Request a fresh one instead of leaving a zombie in-flight request.
        let retry_clock = now + Duration::from_secs(1);
        assert_eq!(sync.plan_fetches(&[honest], retry_clock).len(), 1);
        sync.on_candidate(&candidate.hash, &honest, Some(candidate.clone()), retry_clock)
            .unwrap();
        assert_eq!(sync.report().awaiting_consensus, 1);
        assert_eq!(sync.report().accepted, 0);
        assert_eq!(sync.ready_for_consensus().len(), 1);
        assert!(sync.has_blockers());
        sync.consensus_committed(&candidate.hash).unwrap();
        assert!(sync.report().local_work_finished());
        assert_eq!(
            sync.consensus_committed(&candidate.hash),
            Err(NativeV3SyncError::NotStagedForConsensus)
        );
    }

    #[test]
    fn missing_parent_blocks_economic_readiness_until_independent_commit() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let parent = mine(vec![genesis.hash.clone()], 1, 10);
        let child = mine(vec![parent.hash.clone()], 2, 20);
        let mut sync = NativeV3SyncRecovery::new(CHAIN, &genesis.hash).unwrap();
        let now = Instant::now();
        let p = peer();
        sync.enqueue_missing(&child.hash, now).unwrap();
        let fetch = sync.plan_fetches(&[p], now);
        assert_eq!(fetch.len(), 1);
        sync.on_candidate(&child.hash, &p, Some(child.clone()), now).unwrap();
        assert!(sync.ready_for_consensus().is_empty());
        assert_eq!(
            sync.consensus_committed(&child.hash),
            Err(NativeV3SyncError::MissingConsensusParents)
        );
        let parent_work = sync.plan_fetches(&[p], now);
        assert_eq!(parent_work[0].hash, parent.hash);
        sync.on_candidate(&parent.hash, &p, Some(parent.clone()), now)
            .unwrap();
        assert_eq!(sync.ready_for_consensus()[0].hash, parent.hash);
        sync.consensus_committed(&parent.hash).unwrap();
        assert_eq!(sync.ready_for_consensus()[0].hash, child.hash);
        sync.consensus_committed(&child.hash).unwrap();
        assert!(!sync.has_blockers());
    }

    #[test]
    fn timeout_switches_peers_and_exhaustion_never_becomes_synced() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let now = Instant::now();
        let a = peer();
        let b = peer();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, &genesis.hash).unwrap();
        let hash = "a".repeat(64);
        sync.enqueue_missing(&hash, now).unwrap();
        let first = sync.plan_fetches(&[a, b], now);
        assert_eq!(first.len(), 1);
        sync.dispatch_failed(&hash, &first[0].authenticated_peer, now).unwrap();
        assert!(sync.plan_fetches(&[a, b], now).is_empty());
        let second_clock = now + Duration::from_secs(1);
        let second = sync.plan_fetches(&[a, b], second_clock);
        assert_eq!(second.len(), 1);
        assert_ne!(first[0].authenticated_peer, second[0].authenticated_peer);
        let mut current = second;
        for attempt in 2..=NATIVE_V3_SYNC_MAX_RETRIES {
            let time = now + Duration::from_secs(u64::from(attempt) * 12);
            assert_eq!(current.len(), 1);
            sync.dispatch_failed(&hash, &current[0].authenticated_peer, time)
                .unwrap();
            if attempt < NATIVE_V3_SYNC_MAX_RETRIES {
                current = sync.plan_fetches(&[a, b], time + Duration::from_secs(10));
            }
        }
        assert_eq!(sync.report().exhausted, 1);
        assert!(sync.has_blockers());
        assert!(sync.plan_fetches(&[a, b], now + Duration::from_secs(300)).is_empty());
    }

    #[test]
    fn peer_disconnect_and_bounded_queue_fail_closed() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut sync = NativeV3SyncRecovery::new(CHAIN, &genesis.hash).unwrap();
        let now = Instant::now();
        let first_peer = peer();
        let other = peer();
        sync.enqueue_missing(&"a".repeat(64), now).unwrap();
        let assignments = sync.plan_fetches(&[first_peer], now);
        assert_eq!(assignments.len(), 1);
        assert_eq!(sync.peer_disconnected(&other, now), 0);
        assert_eq!(sync.peer_disconnected(&first_peer, now), 1);
        assert!(sync.has_blockers());
        let again = sync.plan_fetches(&[other], now + Duration::from_secs(1));
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].authenticated_peer, other);
        assert_eq!(
            sync.enqueue_missing("not-a-hash", now),
            Err(NativeV3SyncError::InvalidHash)
        );
        for i in 0..NATIVE_V3_SYNC_MAX_KNOWN_HASHES {
            let h = format!("{i:064x}");
            sync.enqueue_missing(&h, now).ok();
        }
        assert_eq!(sync.items.len(), NATIVE_V3_SYNC_MAX_KNOWN_HASHES);
        assert_eq!(
            sync.enqueue_missing(&"f".repeat(64), now),
            Err(NativeV3SyncError::QueueFull)
        );
    }
}
