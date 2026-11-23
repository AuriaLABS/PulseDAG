//! Dormant native-v3 GetBlock outbound correlation book.
//!
//! A response can only be selected by the *actual libp2p OutboundRequestId*
//! returned by the Noise-authenticated Swarm, never by JSON supplied by a peer.
//! It verifies the requested native BlockEnvelopeV3 candidate and consumes a
//! matched response exactly once. No legacy types::Block or daemon InboundEvent
//! is constructed. Nothing here proves monetary acceptance, derives GD-08,
//! authorizes block storage, serves from a native accepted DAG, or enables a
//! production request-response substream. Public testnet/mainnet remain NO-GO.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

use libp2p::{identity, PeerId};

use super::direct_getblock_pending_v1::{
    DIRECT_GETBLOCK_REQUEST_TTL, MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL,
    MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER,
};
use super::direct_getblock_v1::{
    encode_direct_getblock_request, DirectGetBlockRequestV1, DirectGetBlockWireError,
    DIRECT_GETBLOCK_PROTOCOL_V1,
};
use super::native_v3_getblock_candidate_v1::{
    decode_native_v3_getblock_candidate_response, NativeV3GetBlockCandidateResponse,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeV3GetBlockCorrelationError {
    InvalidChainIdentity,
    NotConnected,
    InvalidTarget,
    SequenceOverflow,
    NoDeadline,
    BudgetExceeded,
    DuplicateTransportId,
    UnknownTransportId,
    WrongAuthenticatedPeer,
    Expired,
    Wire(DirectGetBlockWireError),
}

#[derive(Debug)]
struct BoundNativeV3Request {
    request: DirectGetBlockRequestV1,
    target: PeerId,
    deadline: Instant,
}

/// Use the REAL `libp2p::request_response::OutboundRequestId` as `I` in a
/// future native-v3 Swarm. A generic ID only permits deterministic unit tests.
/// There is NO production instantiation, protocol switch or consensus ingress.
#[derive(Debug)]
pub struct NativeV3GetBlockCorrelationBookV1<I: Copy + Eq + Hash> {
    chain_id: String,
    local_peer: PeerId,
    epoch: String,
    sequence: u64,
    pending: HashMap<I, BoundNativeV3Request>,
}

impl<I: Copy + Eq + Hash> NativeV3GetBlockCorrelationBookV1<I> {
    pub fn new(
        chain_id: &str,
        local_noise_peer: PeerId,
    ) -> Result<Self, NativeV3GetBlockCorrelationError> {
        if chain_id.is_empty() || chain_id.len() > 96 {
            return Err(NativeV3GetBlockCorrelationError::InvalidChainIdentity);
        }
        Ok(Self {
            chain_id: chain_id.to_owned(),
            local_peer: local_noise_peer,
            // New random domain on each instance/process restart; reconnection
            // within the runtime never resets the sequence or reuses tickets.
            epoch: PeerId::from(identity::Keypair::generate_ed25519().public()).to_string(),
            sequence: 0,
            pending: HashMap::new(),
        })
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// For diagnostics only: a peer's wire request_id may NEVER be used as
    /// the lookup key; callers must hold the real local OutboundRequestId.
    pub fn expected_request(&self, transport_id: &I) -> Option<&DirectGetBlockRequestV1> {
        self.pending.get(transport_id).map(|slot| &slot.request)
    }

    /// Call `send_on_authenticated_swarm` on the SAME Swarm that will emit
    /// the corresponding response/failure event. This does NOT send itself.
    /// The existing production P2P module never calls this book.
    pub fn issue_with<F>(
        &mut self,
        target: &PeerId,
        requested_hash: &str,
        swarm_connected: bool,
        now: Instant,
        send_on_authenticated_swarm: F,
    ) -> Result<I, NativeV3GetBlockCorrelationError>
    where
        F: FnOnce(&PeerId, Vec<u8>) -> I,
    {
        self.expire(now);
        if !swarm_connected {
            return Err(NativeV3GetBlockCorrelationError::NotConnected);
        }
        if target == &self.local_peer {
            return Err(NativeV3GetBlockCorrelationError::InvalidTarget);
        }
        if self.pending.len() >= MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL
            || self.pending.values().filter(|slot| slot.target == *target).count()
                >= MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER
        {
            return Err(NativeV3GetBlockCorrelationError::BudgetExceeded);
        }
        let next = self
            .sequence
            .checked_add(1)
            .ok_or(NativeV3GetBlockCorrelationError::SequenceOverflow)?;
        let request = DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: self.chain_id.clone(),
            request_id: format!(
                "getblock:{}:{}:{requested_hash}:{next}",
                self.local_peer, self.epoch
            ),
            requesting_peer_id: self.local_peer.to_string(),
            requested_peer_id: target.to_string(),
            block_hash: requested_hash.to_owned(),
        };
        let wire = encode_direct_getblock_request(
            &request,
            &self.chain_id,
            &self.local_peer.to_string(),
            &target.to_string(),
        )
        .map_err(NativeV3GetBlockCorrelationError::Wire)?;
        let deadline = now
            .checked_add(DIRECT_GETBLOCK_REQUEST_TTL)
            .ok_or(NativeV3GetBlockCorrelationError::NoDeadline)?;
        // Burn the sequence before the real send. Even a failed/colliding
        // transport ID can never authorize reuse of the same wire ticket.
        self.sequence = next;
        let actual_transport_id = send_on_authenticated_swarm(target, wire);
        if self.pending.contains_key(&actual_transport_id) {
            return Err(NativeV3GetBlockCorrelationError::DuplicateTransportId);
        }
        self.pending.insert(
            actual_transport_id,
            BoundNativeV3Request {
                request,
                target: *target,
                deadline,
            },
        );
        Ok(actual_transport_id)
    }

    /// A forged peer cannot consume another peer's request, even if the
    /// request has expired. A response from the actual authenticated target
    /// consumes its own one-shot ticket BEFORE decoding untrusted bytes.
    /// Decoding yields only a candidate: never an accepted monetary block.
    pub fn complete_from_noise(
        &mut self,
        actual_transport_id: &I,
        authenticated_responder: &PeerId,
        raw: &[u8],
        now: Instant,
    ) -> Result<NativeV3GetBlockCandidateResponse, NativeV3GetBlockCorrelationError> {
        let slot = self
            .pending
            .get(actual_transport_id)
            .ok_or(NativeV3GetBlockCorrelationError::UnknownTransportId)?;
        if &slot.target != authenticated_responder {
            return Err(NativeV3GetBlockCorrelationError::WrongAuthenticatedPeer);
        }
        if now >= slot.deadline {
            self.pending.remove(actual_transport_id);
            return Err(NativeV3GetBlockCorrelationError::Expired);
        }
        let slot = self
            .pending
            .remove(actual_transport_id)
            .ok_or(NativeV3GetBlockCorrelationError::UnknownTransportId)?;
        decode_native_v3_getblock_candidate_response(
            raw,
            &slot.request,
            &self.chain_id,
            &self.local_peer.to_string(),
            &authenticated_responder.to_string(),
        )
        .map_err(NativeV3GetBlockCorrelationError::Wire)
    }

    /// An OutboundFailure can clear only this exact authenticated peer's ID.
    pub fn cancel(&mut self, transport_id: &I, authenticated_target: &PeerId) -> bool {
        if self
            .pending
            .get(transport_id)
            .is_some_and(|slot| slot.target == *authenticated_target)
        {
            self.pending.remove(transport_id);
            true
        } else {
            false
        }
    }

    pub fn peer_disconnected(&mut self, authenticated_peer: &PeerId) -> usize {
        let before = self.pending.len();
        self.pending
            .retain(|_, slot| &slot.target != authenticated_peer);
        before - self.pending.len()
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, slot| now < slot.deadline);
        before - self.pending.len()
    }

    /// Explicit bounded request lifetime is shared with the transport codec.
    pub fn lifetime(&self) -> Duration {
        DIRECT_GETBLOCK_REQUEST_TTL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::native_v3_getblock_candidate_v1::NATIVE_V3_GETBLOCK_CANDIDATE_VERSION;
    use std::time::Duration;

    fn peer() -> PeerId {
        PeerId::from(identity::Keypair::generate_ed25519().public())
    }

    fn missing(request: &DirectGetBlockRequestV1, responder: &PeerId) -> Vec<u8> {
        serde_json::to_vec(&NativeV3GetBlockCandidateResponse {
            version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
            chain_id: request.chain_id.clone(),
            request_id: request.request_id.clone(),
            responding_peer_id: responder.to_string(),
            block_hash: request.block_hash.clone(),
            block: None,
        })
        .unwrap()
    }

    #[test]
    fn response_must_match_local_transport_id_and_real_authenticated_peer() {
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let mut book = NativeV3GetBlockCorrelationBookV1::<u64>::new("first-native-v3", local)
            .unwrap();
        let now = Instant::now();
        let hash = "a".repeat(64);
        let first = book
            .issue_with(&remote, &hash, true, now, |actual, raw| {
                assert_eq!(*actual, remote);
                assert!(!raw.is_empty());
                101
            })
            .unwrap();
        let second = book
            .issue_with(&remote, &hash, true, now, |_, _| 102)
            .unwrap();
        assert_eq!(book.len(), 2);
        assert_ne!(
            book.expected_request(&first).unwrap().request_id,
            book.expected_request(&second).unwrap().request_id
        );
        let valid = missing(book.expected_request(&first).unwrap(), &remote);
        assert_eq!(
            book.complete_from_noise(&999, &remote, &valid, now).unwrap_err(),
            NativeV3GetBlockCorrelationError::UnknownTransportId
        );
        assert_eq!(
            book.complete_from_noise(&first, &attacker, &valid, now).unwrap_err(),
            NativeV3GetBlockCorrelationError::WrongAuthenticatedPeer
        );
        assert_eq!(book.len(), 2, "attacker cannot cancel a pending request");
        let verified = book
            .complete_from_noise(&first, &remote, &valid, now)
            .unwrap();
        assert_eq!(verified.block_hash, hash);
        assert!(verified.block.is_none(), "authenticated miss is not a block");
        assert_eq!(book.len(), 1);
        assert_eq!(
            book.complete_from_noise(&first, &remote, &valid, now).unwrap_err(),
            NativeV3GetBlockCorrelationError::UnknownTransportId
        );
        assert!(!book.cancel(&second, &attacker));
        assert!(book.cancel(&second, &remote));
        assert!(book.is_empty());
    }

    #[test]
    fn malicious_authenticated_response_is_consumed_without_admission() {
        let local = peer();
        let remote = peer();
        let mut book = NativeV3GetBlockCorrelationBookV1::<u64>::new("first-native-v3", local)
            .unwrap();
        let now = Instant::now();
        let id = book
            .issue_with(&remote, &"b".repeat(64), true, now, |_, _| 7)
            .unwrap();
        let original = book.expected_request(&id).unwrap().clone();
        let mut forged: NativeV3GetBlockCandidateResponse =
            serde_json::from_slice(&missing(&original, &remote)).unwrap();
        forged.block_hash = "c".repeat(64);
        let raw = serde_json::to_vec(&forged).unwrap();
        assert!(matches!(
            book.complete_from_noise(&id, &remote, &raw, now),
            Err(NativeV3GetBlockCorrelationError::Wire(_))
        ));
        assert!(book.is_empty(), "invalid response cannot be replayed");
        assert_eq!(
            book.complete_from_noise(&id, &remote, &missing(&original, &remote), now).unwrap_err(),
            NativeV3GetBlockCorrelationError::UnknownTransportId
        );
    }

    #[test]
    fn quota_disconnect_expiry_and_wrong_peer_cannot_extend_lifetime() {
        let local = peer();
        let remote = peer();
        let impostor = peer();
        let mut book = NativeV3GetBlockCorrelationBookV1::<u64>::new("first-native-v3", local)
            .unwrap();
        let now = Instant::now();
        assert_eq!(
            book.issue_with(&remote, &"x".repeat(64), true, now, |_, _| 0),
            Err(NativeV3GetBlockCorrelationError::Wire(
                DirectGetBlockWireError::Invalid("noncanonical requested block hash")
            ))
        );
        assert_eq!(
            book.issue_with(&remote, &"a".repeat(64), false, now, |_, _| 0),
            Err(NativeV3GetBlockCorrelationError::NotConnected)
        );
        assert_eq!(
            book.issue_with(&local, &"a".repeat(64), true, now, |_, _| 0),
            Err(NativeV3GetBlockCorrelationError::InvalidTarget)
        );
        for i in 0..4_u64 {
            book.issue_with(&remote, &"a".repeat(64), true, now, |_, _| i)
                .unwrap();
        }
        assert_eq!(
            book.issue_with(&remote, &"a".repeat(64), true, now, |_, _| 55),
            Err(NativeV3GetBlockCorrelationError::BudgetExceeded)
        );
        let first = book.expected_request(&0).unwrap().clone();
        assert_eq!(
            book.complete_from_noise(
                &0, &impostor, &missing(&first, &remote),
                now + Duration::from_secs(30)
            ).unwrap_err(),
            NativeV3GetBlockCorrelationError::WrongAuthenticatedPeer
        );
        assert_eq!(book.len(), 4);
        assert_eq!(book.expire(now + book.lifetime()), 4);
        assert!(book.is_empty());
        let id = book
            .issue_with(
                &remote,
                &"a".repeat(64),
                true,
                now + Duration::from_secs(30),
                |_, _| 20,
            )
            .unwrap();
        assert_eq!(id, 20);
        assert_eq!(book.peer_disconnected(&impostor), 0);
        assert_eq!(book.peer_disconnected(&remote), 1);
        assert!(book.is_empty());
    }

    #[test]
    fn collision_cannot_replace_existing_local_transport_ticket() {
        let local = peer();
        let remote = peer();
        let mut book = NativeV3GetBlockCorrelationBookV1::<u64>::new("first-native-v3", local)
            .unwrap();
        let now = Instant::now();
        book.issue_with(&remote, &"a".repeat(64), true, now, |_, _| 1)
            .unwrap();
        let prior = book.expected_request(&1).unwrap().request_id.clone();
        assert_eq!(
            book.issue_with(&remote, &"b".repeat(64), true, now, |_, _| 1),
            Err(NativeV3GetBlockCorrelationError::DuplicateTransportId)
        );
        assert_eq!(book.len(), 1);
        assert_eq!(book.expected_request(&1).unwrap().request_id, prior);
    }
}
