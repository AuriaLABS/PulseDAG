//! First-public-network native-v3 pull transport coordinator (DORMANT).
//!
//! This joins verified missing-parent scheduling to the actual libp2p
//! request_response::OutboundRequestId from a single authenticated Noise
//! Swarm. It intentionally has NO production Swarm registration, no accepted
//! native-v3 storage, no monetary consensus permission, and no legacy-block
//! fallback. The test-only sender may use synthetic transport IDs; the real
//! integration MUST use NativeV3RealNoisePullControllerV1.
//!
//! Critical contract: an authenticated response can stage a full native-v3
//! candidate, NEVER mark it economically accepted. Only an independent daemon
//! atomic consensus/storage commit can acknowledge it.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Instant;

use libp2p::{request_response, PeerId};
use pulsedag_core::block_envelope_v3::BlockEnvelopeV3;

use super::native_v3_getblock_correlation_v1::{
    NativeV3GetBlockCorrelationBookV1, NativeV3GetBlockCorrelationError,
};
use super::native_v3_sync_recovery_v1::{
    NativeV3SyncError, NativeV3SyncRecovery, NativeV3SyncReport,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeV3NoisePullErrorV1 {
    InvalidTransportId,
    WrongAuthenticatedPeer,
    Correlation(NativeV3GetBlockCorrelationError),
    Recovery(NativeV3SyncError),
}

#[derive(Debug)]
struct PendingNoisePull {
    hash: String,
    authenticated_peer: PeerId,
}

/// The ONLY permitted production transport ID type; the wrapper is
/// non-operational until a separate explicit review wires the real Swarm.
pub type NativeV3RealNoisePullControllerV1 =
    NativeV3NoisePullControllerV1<request_response::OutboundRequestId>;

#[derive(Debug)]
pub struct NativeV3PullDispatchV1<I> {
    pub transmitted: Vec<I>,
    pub rejected: Vec<(String, NativeV3GetBlockCorrelationError)>,
}

#[derive(Debug)]
pub struct NativeV3NoisePullControllerV1<I: Copy + Eq + Hash> {
    recovery: NativeV3SyncRecovery,
    correlation: NativeV3GetBlockCorrelationBookV1<I>,
    inflight: HashMap<I, PendingNoisePull>,
}

impl<I: Copy + Eq + Hash> NativeV3NoisePullControllerV1<I> {
    pub fn new(
        chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
        actual_local_noise_peer: PeerId,
    ) -> Result<Self, NativeV3NoisePullErrorV1> {
        let recovery =
            NativeV3SyncRecovery::new(chain_id, frozen_genesis_timestamp_ns, frozen_genesis_hash)
                .map_err(NativeV3NoisePullErrorV1::Recovery)?;
        let correlation = NativeV3GetBlockCorrelationBookV1::new(
            chain_id,
            actual_local_noise_peer,
        )
        .map_err(NativeV3NoisePullErrorV1::Correlation)?;
        Ok(Self {
            recovery,
            correlation,
            inflight: HashMap::new(),
        })
    }

    pub fn enqueue_missing(&mut self, hash: &str, now: Instant) -> Result<bool, NativeV3NoisePullErrorV1> {
        self.recovery
            .enqueue_missing(hash, now)
            .map_err(NativeV3NoisePullErrorV1::Recovery)
    }

    pub fn report(&self) -> NativeV3SyncReport {
        self.recovery.report()
    }

    pub fn transport_inflight_count(&self) -> usize {
        self.inflight.len()
    }

    /// Operate on exactly the SAME Noise-authenticated Swarm which will
    /// supply the matching response / OutboundFailure event. Never feed
    /// addresses or peer IDs claimed solely inside a JSON request.
    ///
    /// The caller supplies currently connected Noise peers and the ONLY
    /// callback permitted to create a transport ID is:
    /// |peer, raw| swarm.behaviour_mut().direct_getblock.send_request(peer, raw)
    ///
    /// This method merely assembles the requests. The production Swarm
    /// remains outbound-only and HARD OFF; no runtime calls it.
    pub fn drive<F>(
        &mut self,
        connected_noise_peers: &[PeerId],
        now: Instant,
        accepted_by_daemon: &impl Fn(&str) -> bool,
        mut send_on_same_noise_swarm: F,
    ) -> NativeV3PullDispatchV1<I>
    where
        F: FnMut(&PeerId, Vec<u8>) -> I,
    {
        self.expire(now);
        let assignments = self
            .recovery
            .plan_fetches(connected_noise_peers, now, accepted_by_daemon);
        let mut dispatched = NativeV3PullDispatchV1 {
            transmitted: Vec::new(),
            rejected: Vec::new(),
        };
        for assignment in assignments {
            let peer = assignment.authenticated_peer;
            match self.correlation.issue_with(
                &peer,
                &assignment.hash,
                connected_noise_peers.contains(&peer),
                now,
                |target, raw| send_on_same_noise_swarm(target, raw),
            ) {
                Ok(actual_transport_id) => {
                    // Collision is already checked in correlation::issue_with.
                    // It cannot overwrite a different local transport ticket.
                    self.inflight.insert(
                        actual_transport_id,
                        PendingNoisePull {
                            hash: assignment.hash,
                            authenticated_peer: peer,
                        },
                    );
                    dispatched.transmitted.push(actual_transport_id);
                }
                Err(error) => {
                    // The scheduler tentatively reserved a hash. A rejected
                    // local send MUST release that reservation, or the block
                    // can remain a permanently phantom in-flight request.
                    let _ = self.recovery.dispatch_failed(&assignment.hash, &peer, now);
                    dispatched.rejected.push((assignment.hash, error));
                }
            }
        }
        dispatched
    }

    /// An unsolicited/wrong-peer event must neither consume a valid ticket
    /// nor mutate the pending parent plan. A valid Noise target's response,
    /// even if its JSON/PoW is invalid, consumes exactly one local ticket
    /// and reschedules the missing hash for a fresh authenticated request.
    pub fn response_from_noise(
        &mut self,
        actual_transport_id: &I,
        authenticated_responder: &PeerId,
        raw: &[u8],
        now: Instant,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Result<(), NativeV3NoisePullErrorV1> {
        let slot = self
            .inflight
            .get(actual_transport_id)
            .ok_or(NativeV3NoisePullErrorV1::InvalidTransportId)?;
        if &slot.authenticated_peer != authenticated_responder {
            return Err(NativeV3NoisePullErrorV1::WrongAuthenticatedPeer);
        }
        let hash = slot.hash.clone();
        let checked = self.correlation.complete_from_noise(
            actual_transport_id,
            authenticated_responder,
            raw,
            now,
        );
        self.inflight.remove(actual_transport_id);
        match checked {
            Ok(response) => self
                .recovery
                .on_candidate(
                    &hash,
                    authenticated_responder,
                    response.block,
                    now,
                    accepted_by_daemon,
                )
                .map_err(NativeV3NoisePullErrorV1::Recovery),
            Err(error) => {
                let _ = self
                    .recovery
                    .dispatch_failed(&hash, authenticated_responder, now);
                Err(NativeV3NoisePullErrorV1::Correlation(error))
            }
        }
    }

    /// The *actual* local OutboundFailure ID and authenticated target are
    /// mandatory. A peer's self-reported request_id can never clear work.
    pub fn outbound_failure(
        &mut self,
        actual_transport_id: &I,
        authenticated_target: &PeerId,
        now: Instant,
    ) -> Result<(), NativeV3NoisePullErrorV1> {
        let slot = self
            .inflight
            .get(actual_transport_id)
            .ok_or(NativeV3NoisePullErrorV1::InvalidTransportId)?;
        if &slot.authenticated_peer != authenticated_target {
            return Err(NativeV3NoisePullErrorV1::WrongAuthenticatedPeer);
        }
        let hash = slot.hash.clone();
        self.correlation.cancel(actual_transport_id, authenticated_target);
        self.inflight.remove(actual_transport_id);
        self.recovery
            .dispatch_failed(&hash, authenticated_target, now)
            .map_err(NativeV3NoisePullErrorV1::Recovery)
    }

    pub fn noise_peer_disconnected(&mut self, authenticated_peer: &PeerId, now: Instant) {
        self.correlation.peer_disconnected(authenticated_peer);
        self.recovery.peer_disconnected(authenticated_peer, now);
        self.inflight
            .retain(|_, slot| &slot.authenticated_peer != authenticated_peer);
    }

    /// Exact-request TTL cleanup for both books at once. Without this
    /// joining step a stale correlation ID can diverge from the scheduler,
    /// producing the same persistent "GetBlock sent, no accepted data" stall.
    pub fn expire(&mut self, now: Instant) {
        self.correlation.expire(now);
        self.recovery.expire(now);
        let book = &self.correlation;
        self.inflight
            .retain(|id, _| book.expected_request(id).is_some());
    }

    /// Staging is a PoW/merkle transport boundary, NOT financial acceptance.
    /// Never call the commit acknowledgment from a peer event.
    pub fn ready_for_daemon_consensus(
        &self,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Vec<BlockEnvelopeV3> {
        self.recovery.ready_for_consensus(accepted_by_daemon)
    }

    /// This method requires a fresh query of the independently committed
    /// native-v3 GD-08/UTXO/reward accepted store after atomic persistence.
    /// No native monetary accepted-store implementation is supplied here.
    pub fn accepted_by_daemon_after_atomic_commit(
        &mut self,
        hash: &str,
        accepted_by_daemon: &impl Fn(&str) -> bool,
    ) -> Result<(), NativeV3NoisePullErrorV1> {
        self.recovery
            .consensus_committed(hash, accepted_by_daemon)
            .map_err(NativeV3NoisePullErrorV1::Recovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashMap};
    use libp2p::identity;
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        types::compute_merkle_root,
    };
    use crate::messages::{
        direct_getblock_v1::{decode_direct_getblock_request, DirectGetBlockRequestV1},
        native_v3_getblock_candidate_v1::{
            encode_native_v3_getblock_candidate_response, NativeV3GetBlockCandidateResponse,
            NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
        },
    };

    const CHAIN: &str = "first-native-v3-noise-pull-controller-test-only";
    const GENESIS_NS: u64 = 1_800_000_000_500_000_000;

    fn peer() -> PeerId {
        PeerId::from(identity::Keypair::generate_ed25519().public())
    }

    fn candidate(genesis_hash: &str) -> BlockEnvelopeV3 {
        let txs = vec![build_reward_claim_transaction_v3("pulse1noise-controller", 1, CHAIN).unwrap()];
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis_hash.to_string()],
            timestamp_ns: GENESIS_NS + 1_000_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: "33".repeat(32),
            blue_score: 1,
            height: 1,
        };
        for nonce in 0..4096 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                return build_block_envelope_v3(CHAIN, header, txs).unwrap();
            }
        }
        panic!("test requires easy native-v3 PoW");
    }

    fn response(
        request: &DirectGetBlockRequestV1,
        local_peer: &PeerId,
        remote_peer: &PeerId,
        block: Option<BlockEnvelopeV3>,
    ) -> Vec<u8> {
        let reply = NativeV3GetBlockCandidateResponse {
            version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
            chain_id: CHAIN.to_string(),
            request_id: request.request_id.clone(),
            responding_peer_id: remote_peer.to_string(),
            block_hash: request.block_hash.clone(),
            block,
        };
        encode_native_v3_getblock_candidate_response(
            &reply,
            request,
            CHAIN,
            &local_peer.to_string(),
            &remote_peer.to_string(),
        )
        .unwrap()
    }

    #[test]
    fn exact_transport_ids_bind_native_block_with_no_fake_acceptance() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let mined = candidate(&genesis.hash);
        let now = Instant::now();
        let mut controller =
            NativeV3NoisePullControllerV1::<u64>::new(CHAIN, GENESIS_NS, &genesis.hash, local)
                .unwrap();
        controller.enqueue_missing(&mined.hash, now).unwrap();
        let mut wire = HashMap::new();
        let sent = controller.drive(&[remote], now, &|_| false, |target, bytes| {
            assert_eq!(*target, remote);
            wire.insert(91_u64, bytes);
            91_u64
        });
        assert_eq!(sent.transmitted, vec![91]);
        assert!(sent.rejected.is_empty());
        let req =
            decode_direct_getblock_request(&wire[&91], CHAIN, &local.to_string(), &remote.to_string())
                .unwrap();
        assert_eq!(req.block_hash, mined.hash);
        let raw = response(&req, &local, &remote, Some(mined.clone()));

        assert_eq!(
            controller.response_from_noise(&91, &attacker, &raw, now, &|_| false),
            Err(NativeV3NoisePullErrorV1::WrongAuthenticatedPeer)
        );
        assert_eq!(controller.transport_inflight_count(), 1);
        assert_eq!(
            controller.response_from_noise(&999, &remote, &raw, now, &|_| false),
            Err(NativeV3NoisePullErrorV1::InvalidTransportId)
        );
        controller
            .response_from_noise(&91, &remote, &raw, now, &|_| false)
            .unwrap();
        assert_eq!(controller.transport_inflight_count(), 0);
        assert_eq!(controller.report().awaiting_consensus, 1);
        assert_eq!(controller.report().accepted, 0);
        assert_eq!(controller.ready_for_daemon_consensus(&|_| false).len(), 1);
        assert_eq!(
            controller.accepted_by_daemon_after_atomic_commit(&mined.hash, &|_| false),
            Err(NativeV3NoisePullErrorV1::Recovery(
                NativeV3SyncError::NotInAcceptedStore
            ))
        );
        let trusted = BTreeSet::from([mined.hash.clone()]);
        controller.accepted_by_daemon_after_atomic_commit(
            &mined.hash,
            &|hash| trusted.contains(hash),
        ).unwrap();
        assert!(controller.report().local_work_finished());
        assert_eq!(controller.report().accepted, 1);
        assert_eq!(
            controller.response_from_noise(&91, &remote, &raw, now, &|_| false),
            Err(NativeV3NoisePullErrorV1::InvalidTransportId)
        );
    }

    #[test]
    fn invalid_reply_and_real_outbound_failure_reschedule_different_noise_tickets() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let local = peer();
        let remote = peer();
        let other = peer();
        let attacker = peer();
        let mined = candidate(&genesis.hash);
        let now = Instant::now();
        let mut controller =
            NativeV3NoisePullControllerV1::<u64>::new(CHAIN, GENESIS_NS, &genesis.hash, local)
                .unwrap();
        controller.enqueue_missing(&mined.hash, now).unwrap();
        let mut bytes = Vec::new();
        let first = controller.drive(&[remote, other], now, &|_| false, |target, raw| {
            bytes.push((*target, raw));
            101
        });
        assert_eq!(first.transmitted, vec![101]);
        let first_peer = bytes[0].0;
        let first_req = decode_direct_getblock_request(
            &bytes[0].1, CHAIN, &local.to_string(), &first_peer.to_string(),
        ).unwrap();
        let mut forged = response(&first_req, &local, &first_peer, Some(mined.clone()));
        // An authenticated responder can still lie in the JSON body.
        forged[0] = b'!';
        assert!(matches!(
            controller.response_from_noise(&101, &first_peer, &forged, now, &|_| false),
            Err(NativeV3NoisePullErrorV1::Correlation(_))
        ));
        assert_eq!(controller.transport_inflight_count(), 0);
        assert_eq!(controller.report().queued, 1);
        let later = now + std::time::Duration::from_secs(2);
        let second = controller.drive(&[remote, other], later, &|_| false, |_, _| 102);
        assert_eq!(second.transmitted, vec![102]);
        let next_peer = if first_peer == remote {other} else {remote};
        assert_eq!(
            controller.outbound_failure(&102, &attacker, later),
            Err(NativeV3NoisePullErrorV1::WrongAuthenticatedPeer)
        );
        assert_eq!(controller.transport_inflight_count(), 1);
        controller.outbound_failure(&102, &next_peer, later).unwrap();
        assert_eq!(controller.transport_inflight_count(), 0);
        assert_eq!(controller.report().queued, 1);
        assert_eq!(controller.report().accepted, 0);
    }

    #[test]
    fn timeout_and_disconnect_drop_matching_transport_not_other_peer() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let local = peer();
        let target = peer();
        let unrelated = peer();
        let now = Instant::now();
        let mut controller =
            NativeV3NoisePullControllerV1::<u64>::new(CHAIN, GENESIS_NS, &genesis.hash, local)
                .unwrap();
        controller.enqueue_missing(&"a".repeat(64), now).unwrap();
        let sent = controller.drive(&[target], now, &|_| false, |_, _| 11);
        assert_eq!(sent.transmitted, vec![11]);
        controller.noise_peer_disconnected(&unrelated, now);
        assert_eq!(controller.transport_inflight_count(), 1);
        controller.noise_peer_disconnected(&target, now);
        assert_eq!(controller.transport_inflight_count(), 0);
        assert_eq!(controller.report().queued, 1);
        let later = now + std::time::Duration::from_secs(2);
        let second = controller.drive(&[target], later, &|_| false, |_, _| 12);
        assert_eq!(second.transmitted, vec![12]);
        let too_late = later + std::time::Duration::from_secs(10);
        controller.expire(too_late);
        assert_eq!(controller.transport_inflight_count(), 0);
        assert_eq!(controller.report().queued, 1);
        assert_eq!(
            controller.response_from_noise(&12, &target, b"{}", too_late, &|_| false),
            Err(NativeV3NoisePullErrorV1::InvalidTransportId)
        );
        assert_eq!(controller.report().accepted, 0);
    }

    #[test]
    fn real_libp2p_outbound_request_id_is_the_only_runtime_alias() {
        fn accepts_real_type(_value: Option<NativeV3RealNoisePullControllerV1>) {}
        accepts_real_type(None);
    }
}
