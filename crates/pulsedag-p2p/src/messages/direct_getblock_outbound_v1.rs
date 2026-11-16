//! Typed outbound-only GetBlock ticket adapter for future Noise-authenticated
//! libp2p request-response dispatch. Intentionally NOT attached to the live
//! Swarm, not a block/PoW/UTXO validator, and not a release switch.
//!
//! The runtime must bind every prepared ticket to its *actual* libp2p
//! OutboundRequestId and use that immutable identifier for complete_raw().
//! Never use a request ID asserted by the untrusted response as the lookup key.

use std::time::Instant;

use libp2p::{identity, PeerId};

use super::direct_getblock_pending_v1::{DirectGetBlockPendingBookV1, DirectGetBlockPendingError};
use super::direct_getblock_v1::{
    decode_direct_getblock_response, encode_direct_getblock_request, DirectGetBlockRequestV1,
    DirectGetBlockResponseV1, DirectGetBlockWireError, DIRECT_GETBLOCK_PROTOCOL_V1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockOutboundError {
    Pending(DirectGetBlockPendingError),
    Wire(DirectGetBlockWireError),
    SequenceOverflow,
    InvalidTarget,
    Unsolicited,
}

#[derive(Debug, Clone)]
pub struct PreparedDirectGetBlockV1 {
    pub request: DirectGetBlockRequestV1,
    pub wire: Vec<u8>,
}

#[derive(Debug)]
pub struct DirectGetBlockOutboundV1 {
    chain_id: String,
    local_peer: PeerId,
    // A fresh per-runtime entropy domain avoids replaying an identical ID
    // after a process restart with the same persistent Noise identity.
    ticket_epoch: String,
    book: DirectGetBlockPendingBookV1,
    next_sequence: u64,
}

impl DirectGetBlockOutboundV1 {
    /// Construct once per node runtime; preserve across per-peer disconnects.
    /// The local PeerId must originate from the actual Noise Swarm identity.
    pub fn new(
        chain_id: &str,
        authenticated_local_peer: PeerId,
    ) -> Result<Self, DirectGetBlockOutboundError> {
        let book =
            DirectGetBlockPendingBookV1::new(chain_id, &authenticated_local_peer.to_string())
                .map_err(DirectGetBlockOutboundError::Pending)?;
        Ok(Self {
            chain_id: chain_id.to_owned(),
            local_peer: authenticated_local_peer,
            ticket_epoch: PeerId::from(identity::Keypair::generate_ed25519().public()).to_string(),
            book,
            next_sequence: 0,
        })
    }

    /// Produce a validated, bounded envelope and register its in-flight budget.
    /// The caller must verify target_peer is currently connected to the Swarm
    /// and cancel this exact ticket if libp2p send_request cannot be issued.
    pub fn prepare(
        &mut self,
        authenticated_target_peer: &PeerId,
        block_hash: &str,
        now: Instant,
    ) -> Result<PreparedDirectGetBlockV1, DirectGetBlockOutboundError> {
        if authenticated_target_peer == &self.local_peer {
            return Err(DirectGetBlockOutboundError::InvalidTarget);
        }
        let sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(DirectGetBlockOutboundError::SequenceOverflow)?;
        let request = DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: self.chain_id.clone(),
            request_id: format!(
                "getblock:{}:{}:{block_hash}:{sequence}",
                self.local_peer, self.ticket_epoch
            ),
            requesting_peer_id: self.local_peer.to_string(),
            requested_peer_id: authenticated_target_peer.to_string(),
            block_hash: block_hash.to_owned(),
        };
        // Encode/validate BEFORE mutating pending state. Invalid hash, chain
        // or length cannot consume quota or advance the lifetime sequence.
        let wire = encode_direct_getblock_request(
            &request,
            &self.chain_id,
            &self.local_peer.to_string(),
            &authenticated_target_peer.to_string(),
        )
        .map_err(DirectGetBlockOutboundError::Wire)?;
        self.book
            .begin(request.clone(), &authenticated_target_peer.to_string(), now)
            .map_err(DirectGetBlockOutboundError::Pending)?;
        self.next_sequence = sequence;
        Ok(PreparedDirectGetBlockV1 { request, wire })
    }

    /// The request ID MUST come from the local OutboundRequestId correlation
    /// map. A forged request ID inside raw bytes never selects the slot.
    pub fn complete_raw(
        &mut self,
        expected_request_id: &str,
        authenticated_remote_peer: &PeerId,
        raw: &[u8],
        now: Instant,
    ) -> Result<DirectGetBlockResponseV1, DirectGetBlockOutboundError> {
        let response = {
            let expected = self
                .book
                .expected_request(expected_request_id)
                .ok_or(DirectGetBlockOutboundError::Unsolicited)?;
            decode_direct_getblock_response(raw, expected, &authenticated_remote_peer.to_string())
                .map_err(DirectGetBlockOutboundError::Wire)?
        };
        self.book
            .complete(response, &authenticated_remote_peer.to_string(), now)
            .map_err(DirectGetBlockOutboundError::Pending)
    }

    /// Cleanup after a failed libp2p outbound request or cancelled substream.
    /// Other peers cannot cancel a ticket they do not own.
    pub fn cancel_request(&mut self, request_id: &str, authenticated_target_peer: &PeerId) -> bool {
        self.book
            .cancel_request(request_id, &authenticated_target_peer.to_string())
    }

    pub fn cancel_peer(&mut self, authenticated_remote_peer: &PeerId) -> usize {
        self.book
            .cancel_peer(&authenticated_remote_peer.to_string())
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        self.book.expire(now)
    }

    pub fn len(&self) -> usize {
        self.book.len()
    }

    pub fn is_empty(&self) -> bool {
        self.book.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use super::super::direct_getblock_v1::encode_direct_getblock_response;
    use libp2p::identity;

    fn new_peer() -> PeerId {
        PeerId::from(identity::Keypair::generate_ed25519().public())
    }

    fn reply(prepared: &PreparedDirectGetBlockV1, responder: &PeerId) -> Vec<u8> {
        let response = DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: prepared.request.chain_id.clone(),
            request_id: prepared.request.request_id.clone(),
            responding_peer_id: responder.to_string(),
            block_hash: prepared.request.block_hash.clone(),
            block: None,
        };
        encode_direct_getblock_response(&response, &prepared.request, &responder.to_string())
            .unwrap()
    }

    #[test]
    fn immediate_same_hash_requests_are_distinct_and_complete_out_of_order() {
        let local = new_peer();
        let remote = new_peer();
        let mut outbound = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        let now = Instant::now();
        let hash = "a".repeat(64);
        let one = outbound.prepare(&remote, &hash, now).unwrap();
        let two = outbound.prepare(&remote, &hash, now).unwrap();
        assert_ne!(one.request.request_id, two.request.request_id);
        assert_eq!(outbound.len(), 2);
        let parsed: DirectGetBlockRequestV1 = serde_json::from_slice(&one.wire).unwrap();
        assert_eq!(parsed, one.request);
        let raw_two = reply(&two, &remote);
        let raw_one = reply(&one, &remote);
        outbound
            .complete_raw(&two.request.request_id, &remote, &raw_two, now)
            .unwrap();
        outbound
            .complete_raw(&one.request.request_id, &remote, &raw_one, now)
            .unwrap();
        assert!(outbound.is_empty());
    }

    #[test]
    fn independent_runtime_epochs_never_reissue_stale_request_ids() {
        let local = new_peer();
        let remote = new_peer();
        let now = Instant::now();
        let hash = "a".repeat(64);
        let mut before = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        let mut after = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        let old = before.prepare(&remote, &hash, now).unwrap();
        let fresh = after.prepare(&remote, &hash, now).unwrap();
        assert!(old.request.request_id.ends_with(":1"));
        assert!(fresh.request.request_id.ends_with(":1"));
        assert_ne!(old.request.request_id, fresh.request.request_id);
        assert!(matches!(
            after.complete_raw(
                &fresh.request.request_id,
                &remote,
                &reply(&old, &remote),
                now
            ),
            Err(DirectGetBlockOutboundError::Wire(_))
        ));
        assert_eq!(after.len(), 1);
    }

    #[test]
    fn forged_peer_foreign_id_and_hash_must_not_consume_valid_slot() {
        let local = new_peer();
        let remote = new_peer();
        let imposter = new_peer();
        let mut outbound = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        let now = Instant::now();
        let request = outbound.prepare(&remote, &"a".repeat(64), now).unwrap();
        let wire = reply(&request, &remote);
        assert!(matches!(
            outbound.complete_raw(&request.request.request_id, &imposter, &wire, now),
            Err(DirectGetBlockOutboundError::Wire(_))
        ));
        assert!(matches!(
            outbound.complete_raw("getblock:unknown", &remote, &wire, now),
            Err(DirectGetBlockOutboundError::Unsolicited)
        ));
        let mut tampered: serde_json::Value = serde_json::from_slice(&wire).unwrap();
        tampered["block_hash"] = serde_json::json!("b".repeat(64));
        let tampered = serde_json::to_vec(&tampered).unwrap();
        assert!(matches!(
            outbound.complete_raw(&request.request.request_id, &remote, &tampered, now),
            Err(DirectGetBlockOutboundError::Wire(_))
        ));
        assert_eq!(outbound.len(), 1);
        outbound
            .complete_raw(&request.request.request_id, &remote, &wire, now)
            .unwrap();
        assert!(outbound.is_empty());
    }

    #[test]
    fn bounded_quota_wrong_peer_cancellation_and_replay_rejection() {
        let local = new_peer();
        let remote = new_peer();
        let other = new_peer();
        let mut outbound = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        let now = Instant::now();
        let hash = "c".repeat(64);
        let requests: Vec<_> = (0..4)
            .map(|_| outbound.prepare(&remote, &hash, now).unwrap())
            .collect();
        assert!(matches!(
            outbound.prepare(&remote, &hash, now),
            Err(DirectGetBlockOutboundError::Pending(
                DirectGetBlockPendingError::LimitExceeded
            ))
        ));
        assert!(!outbound.cancel_request(&requests[0].request.request_id, &other));
        assert_eq!(outbound.len(), 4);
        assert!(outbound.cancel_request(&requests[0].request.request_id, &remote));
        let fresh = outbound.prepare(&remote, &hash, now).unwrap();
        assert_ne!(requests[0].request.request_id, fresh.request.request_id);
        let stale_raw = reply(&requests[0], &remote);
        assert!(matches!(
            outbound.complete_raw(&fresh.request.request_id, &remote, &stale_raw, now),
            Err(DirectGetBlockOutboundError::Wire(_))
        ));
        assert_eq!(outbound.len(), 4);
        assert_eq!(outbound.cancel_peer(&remote), 4);
        assert!(outbound.is_empty());
    }

    #[test]
    fn invalid_hash_does_not_advance_sequence_and_expiry_is_bounded() {
        let local = new_peer();
        let remote = new_peer();
        let mut outbound = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        let now = Instant::now();
        assert!(matches!(
            outbound.prepare(&remote, &"A".repeat(64), now),
            Err(DirectGetBlockOutboundError::Wire(_))
        ));
        assert_eq!(outbound.len(), 0);
        let valid = outbound.prepare(&remote, &"a".repeat(64), now).unwrap();
        assert!(valid.request.request_id.ends_with(":1"));
        assert_eq!(outbound.expire(now + Duration::from_secs(8)), 1);
        assert!(matches!(
            outbound.complete_raw(
                &valid.request.request_id,
                &remote,
                &reply(&valid, &remote),
                now + Duration::from_secs(8)
            ),
            Err(DirectGetBlockOutboundError::Unsolicited)
        ));
        assert!(outbound.is_empty());
    }

    #[test]
    fn checked_sequence_overflow_and_self_request_fail_closed() {
        let local = new_peer();
        let remote = new_peer();
        let mut outbound = DirectGetBlockOutboundV1::new("pulsedag-v3-test", local).unwrap();
        assert!(matches!(
            outbound.prepare(&local, &"a".repeat(64), Instant::now()),
            Err(DirectGetBlockOutboundError::InvalidTarget)
        ));
        outbound.next_sequence = u64::MAX;
        assert!(matches!(
            outbound.prepare(&remote, &"a".repeat(64), Instant::now()),
            Err(DirectGetBlockOutboundError::SequenceOverflow)
        ));
        assert!(outbound.is_empty());
    }
}
