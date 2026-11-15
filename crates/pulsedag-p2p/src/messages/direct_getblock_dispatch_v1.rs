//! Bounded binding between the actual libp2p OutboundRequestId and validated
//! direct GetBlock tickets. NOT installed in the production Swarm yet.
//!
//! A byte-level response's self-asserted request ID is NEVER used to find a
//! pending ticket. Only libp2p's locally minted OutboundRequestId is a key.
//! Returned blocks are untrusted until the daemon separately validates full
//! PoW, headers, ancestry, selected-parent and monetary state.

use std::collections::HashMap;
use std::time::Instant;

use libp2p::request_response::{self, OutboundRequestId};
use libp2p::PeerId;

use super::direct_getblock_codec_v1::DirectGetBlockRawCodecV1;
use super::direct_getblock_outbound_v1::{DirectGetBlockOutboundError, DirectGetBlockOutboundV1};
use super::direct_getblock_pending_v1::DIRECT_GETBLOCK_REQUEST_TTL;
use super::direct_getblock_v1::DirectGetBlockResponseV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockDispatchErrorV1 {
    Ticket(DirectGetBlockOutboundError),
    NoAuthenticatedConnection,
    UnknownOutboundRequestId,
    DuplicateOutboundRequestId,
    UnexpectedTransportPeer,
    DeadlineOverflow,
}

#[derive(Debug, Clone)]
struct Dispatched {
    authenticated_target: PeerId,
    ticket_id: String,
    deadline: Instant,
}

/// Owns exactly one libp2p Behaviour for its entire lifetime. Never swap
/// behaviours: libp2p OutboundRequestIds are unique per behaviour, not globally.
pub struct DirectGetBlockDispatchV1 {
    behaviour: request_response::Behaviour<DirectGetBlockRawCodecV1>,
    outbound: DirectGetBlockOutboundV1,
    by_transport_id: HashMap<OutboundRequestId, Dispatched>,
}

impl DirectGetBlockDispatchV1 {
    pub fn new(
        chain_id: &str,
        authenticated_local_peer: PeerId,
        behaviour: request_response::Behaviour<DirectGetBlockRawCodecV1>,
    ) -> Result<Self, DirectGetBlockDispatchErrorV1> {
        let outbound = DirectGetBlockOutboundV1::new(chain_id, authenticated_local_peer)
            .map_err(DirectGetBlockDispatchErrorV1::Ticket)?;
        Ok(Self {
            behaviour,
            outbound,
            by_transport_id: HashMap::new(),
        })
    }

    /// The live caller MUST supply connected=true only after checking the
    /// actual Noise Swarm's is_connected(peer) result. This method is dormant
    /// and does not change the current Gossipsub or daemon paths.
    pub fn issue(
        &mut self,
        authenticated_target: &PeerId,
        block_hash: &str,
        swarm_connected: bool,
        now: Instant,
    ) -> Result<OutboundRequestId, DirectGetBlockDispatchErrorV1> {
        // Keep the transport-ID map and the pending book synchronized even
        // if the periodic timeout tick has been delayed or never delivered.
        self.expire(now);
        if !swarm_connected {
            return Err(DirectGetBlockDispatchErrorV1::NoAuthenticatedConnection);
        }
        let deadline = now
            .checked_add(DIRECT_GETBLOCK_REQUEST_TTL)
            .ok_or(DirectGetBlockDispatchErrorV1::DeadlineOverflow)?;
        let prepared = self
            .outbound
            .prepare(authenticated_target, block_hash, now)
            .map_err(DirectGetBlockDispatchErrorV1::Ticket)?;

        // libp2p generates this identifier; the remote cannot choose it.
        // send_request returns the locally assigned ID; a later
        // OutboundFailure MUST cancel this exact mapping.
        let transport_id = self.behaviour.send_request(authenticated_target, prepared.wire);
        if self.by_transport_id.contains_key(&transport_id) {
            // Defensive invariant if libp2p ever violates uniqueness within
            // the owned behaviour. Never replace an older pending binding.
            self.outbound.cancel_request(&prepared.request.request_id, authenticated_target);
            return Err(DirectGetBlockDispatchErrorV1::DuplicateOutboundRequestId);
        }
        self.by_transport_id.insert(
            transport_id,
            Dispatched {
                authenticated_target: *authenticated_target,
                ticket_id: prepared.request.request_id,
                deadline,
            },
        );
        Ok(transport_id)
    }

    /// Called ONLY after the Noise-authenticated Swarm dispatches an actual
    /// request_response::Message::Response with its own OutboundRequestId.
    pub fn accept_response(
        &mut self,
        transport_id: &OutboundRequestId,
        authenticated_remote: &PeerId,
        raw: &[u8],
        now: Instant,
    ) -> Result<DirectGetBlockResponseV1, DirectGetBlockDispatchErrorV1> {
        let dispatched = self
            .by_transport_id
            .get(transport_id)
            .ok_or(DirectGetBlockDispatchErrorV1::UnknownOutboundRequestId)?;
        if authenticated_remote != &dispatched.authenticated_target {
            return Err(DirectGetBlockDispatchErrorV1::UnexpectedTransportPeer);
        }
        let ticket_id = dispatched.ticket_id.clone();
        let result = self
            .outbound
            .complete_raw(&ticket_id, authenticated_remote, raw, now);
        // The same authenticated request-response substream gets one response.
        // On either a good or a bad response, release this peer's slot. A
        // forged response FROM A DIFFERENT PEER never mutates any slot.
        self.by_transport_id.remove(transport_id);
        self.outbound
            .cancel_request(&ticket_id, authenticated_remote);
        result.map_err(DirectGetBlockDispatchErrorV1::Ticket)
    }

    /// Handle libp2p OutboundFailure with the *actual* affected target peer.
    /// An unrelated peer can never cancel another peer's transport ticket.
    pub fn outbound_failure(
        &mut self,
        transport_id: &OutboundRequestId,
        authenticated_target: &PeerId,
    ) -> Result<(), DirectGetBlockDispatchErrorV1> {
        let dispatched = self
            .by_transport_id
            .get(transport_id)
            .ok_or(DirectGetBlockDispatchErrorV1::UnknownOutboundRequestId)?;
        if authenticated_target != &dispatched.authenticated_target {
            return Err(DirectGetBlockDispatchErrorV1::UnexpectedTransportPeer);
        }
        let ticket_id = dispatched.ticket_id.clone();
        self.by_transport_id.remove(transport_id);
        self.outbound
            .cancel_request(&ticket_id, authenticated_target);
        Ok(())
    }

    /// Only call after all authenticated connections to this peer are gone.
    pub fn peer_disconnected(&mut self, authenticated_peer: &PeerId) -> usize {
        let before = self.by_transport_id.len();
        self.by_transport_id
            .retain(|_, request| &request.authenticated_target != authenticated_peer);
        self.outbound.cancel_peer(authenticated_peer);
        before - self.by_transport_id.len()
    }

    /// Finite capacity even if an upstream event is never delivered.
    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.by_transport_id.len();
        self.by_transport_id
            .retain(|_, request| now < request.deadline);
        self.outbound.expire(now);
        before - self.by_transport_id.len()
    }

    pub fn len(&self) -> usize {
        self.by_transport_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_transport_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::super::direct_getblock_codec_v1::DIRECT_GETBLOCK_STREAM_PROTOCOL_V1;
    use super::super::direct_getblock_v1::{
        encode_direct_getblock_response, DirectGetBlockResponseV1, DIRECT_GETBLOCK_PROTOCOL_V1,
    };
    use super::*;
    use libp2p::{identity, StreamProtocol};
    use std::time::Duration;

    fn peer() -> PeerId {
        PeerId::from(identity::Keypair::generate_ed25519().public())
    }

    fn behaviour() -> request_response::Behaviour<DirectGetBlockRawCodecV1> {
        request_response::Behaviour::with_codec(
            DirectGetBlockRawCodecV1,
            [(
                StreamProtocol::new(DIRECT_GETBLOCK_STREAM_PROTOCOL_V1),
                request_response::ProtocolSupport::Outbound,
            )],
            request_response::Config::default(),
        )
    }

    fn missing_response(
        chain_id: &str,
        ticket_id: &str,
        hash: &str,
        target: &PeerId,
        source: &PeerId,
    ) -> Vec<u8> {
        let original = super::super::direct_getblock_v1::DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: chain_id.to_owned(),
            request_id: ticket_id.to_owned(),
            requesting_peer_id: source.to_string(),
            requested_peer_id: target.to_string(),
            block_hash: hash.to_owned(),
        };
        let response = DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: chain_id.to_owned(),
            request_id: ticket_id.to_owned(),
            responding_peer_id: target.to_string(),
            block_hash: hash.to_owned(),
            block: None,
        };
        encode_direct_getblock_response(&response, &original, &target.to_string()).unwrap()
    }

    #[test]
    fn actual_libp2p_ids_bind_retries_without_network_or_daemon_admission() {
        let local = peer();
        let remote = peer();
        let mut dispatch = DirectGetBlockDispatchV1::new("ticket-testnet", local, behaviour()).unwrap();
        let now = Instant::now();
        let hash = "a".repeat(64);
        assert!(matches!(
            dispatch.issue(&remote, &hash, false, now),
            Err(DirectGetBlockDispatchErrorV1::NoAuthenticatedConnection)
        ));
        assert!(dispatch.is_empty());
        let first = dispatch
            .issue(&remote, &hash, true, now)
            .unwrap();
        let second = dispatch
            .issue(&remote, &hash, true, now)
            .unwrap();
        assert_ne!(first, second);
        assert_eq!(dispatch.len(), 2);

        // The remote cannot supply a transport ID, and a request ID inside
        // its body is not used as a lookup key.
        let second_ticket = dispatch
            .by_transport_id
            .get(&second)
            .unwrap()
            .ticket_id
            .clone();
        let first_ticket = dispatch
            .by_transport_id
            .get(&first)
            .unwrap()
            .ticket_id
            .clone();
        let second_wire =
            missing_response("ticket-testnet", &second_ticket, &hash, &remote, &local);
        let first_wire = missing_response("ticket-testnet", &first_ticket, &hash, &remote, &local);
        dispatch
            .accept_response(&second, &remote, &second_wire, now)
            .unwrap();
        dispatch
            .accept_response(&first, &remote, &first_wire, now)
            .unwrap();
        assert!(dispatch.is_empty());
    }

    #[test]
    fn wrong_transport_peer_and_cross_ticket_payload_fail_closed() {
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let mut dispatch = DirectGetBlockDispatchV1::new("ticket-testnet", local, behaviour()).unwrap();
        let now = Instant::now();
        let hash = "b".repeat(64);
        let a = dispatch
            .issue(&remote, &hash, true, now)
            .unwrap();
        let b = dispatch
            .issue(&remote, &hash, true, now)
            .unwrap();
        let a_ticket = dispatch.by_transport_id.get(&a).unwrap().ticket_id.clone();
        let b_ticket = dispatch.by_transport_id.get(&b).unwrap().ticket_id.clone();
        let a_wire = missing_response("ticket-testnet", &a_ticket, &hash, &remote, &local);
        let b_wire = missing_response("ticket-testnet", &b_ticket, &hash, &remote, &local);

        assert!(matches!(
            dispatch.accept_response(&a, &attacker, &a_wire, now),
            Err(DirectGetBlockDispatchErrorV1::UnexpectedTransportPeer)
        ));
        assert_eq!(dispatch.len(), 2);
        assert!(matches!(
            dispatch.accept_response(&a, &remote, &b_wire, now),
            Err(DirectGetBlockDispatchErrorV1::Ticket(_))
        ));
        assert_eq!(dispatch.len(), 1);
        assert!(matches!(
            dispatch.accept_response(&a, &remote, &a_wire, now),
            Err(DirectGetBlockDispatchErrorV1::UnknownOutboundRequestId)
        ));
        dispatch.accept_response(&b, &remote, &b_wire, now).unwrap();
        assert!(dispatch.is_empty());
    }

    #[test]
    fn expiry_is_enforced_by_issue_without_periodic_tick() {
        let local = peer();
        let remote = peer();
        let now = Instant::now();
        let hash = "d".repeat(64);
        let mut dispatch =
            DirectGetBlockDispatchV1::new("ticket-testnet", local, behaviour()).unwrap();
        for round in 0..8 {
            let instant = now + Duration::from_secs(8 * round);
            for _ in 0..4 {
                dispatch.issue(&remote, &hash, true, instant).unwrap();
            }
            assert_eq!(dispatch.len(), 4, "old transport IDs must expire before reissue");
            assert!(matches!(
                dispatch.issue(&remote, &hash, true, instant),
                Err(DirectGetBlockDispatchErrorV1::Ticket(
                    DirectGetBlockOutboundError::Pending(_)
                ))
            ));
        }
        assert_eq!(dispatch.expire(now + Duration::from_secs(64)), 4);
        assert!(dispatch.is_empty());
    }

    #[test]
    fn failure_ttl_disconnect_and_quota_bound_transport_ids() {
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let mut dispatch = DirectGetBlockDispatchV1::new("ticket-testnet", local, behaviour()).unwrap();
        let now = Instant::now();
        let hash = "c".repeat(64);
        let ids: Vec<_> = (0..4)
            .map(|_| {
                dispatch
                    .issue(&remote, &hash, true, now)
                    .unwrap()
            })
            .collect();
        assert!(matches!(
            dispatch.issue(&remote, &hash, true, now),
            Err(DirectGetBlockDispatchErrorV1::Ticket(
                DirectGetBlockOutboundError::Pending(_)
            ))
        ));
        assert!(matches!(
            dispatch.outbound_failure(&ids[0], &attacker),
            Err(DirectGetBlockDispatchErrorV1::UnexpectedTransportPeer)
        ));
        assert_eq!(dispatch.len(), 4);
        dispatch.outbound_failure(&ids[0], &remote).unwrap();
        assert_eq!(dispatch.len(), 3);
        assert_eq!(dispatch.peer_disconnected(&attacker), 0);
        let fresh = dispatch
            .issue(&remote, &hash, true, now)
            .unwrap();
        assert_ne!(ids[0], fresh);
        assert_eq!(dispatch.expire(now + Duration::from_secs(8)), 4);
        assert!(dispatch.is_empty());
    }
}
