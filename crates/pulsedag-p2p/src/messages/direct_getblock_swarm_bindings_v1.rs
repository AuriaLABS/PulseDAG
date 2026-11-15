//! Bridge between one existing (Noise-authenticated) Swarm behaviour and the
//! bounded GetBlock ticket book. It does NOT construct or replace a Behaviour.
//!
//! The production event loop must pass *its existing*
//! swarm.behaviour_mut().direct_getblock.send_request(..) as the dispatch
//! function and later provide the REAL OutboundRequestId from the Swarm event.
//! The caller must calculate connected with Swarm::is_connected(peer).
//!
//! This adapter is dormant: it neither activates live sends/inbound service,
//! nor bypasses the daemon's independent full block-admission checks.

use std::collections::HashMap;
use std::time::Instant;

use libp2p::request_response::OutboundRequestId;
use libp2p::PeerId;

use super::direct_getblock_outbound_v1::{DirectGetBlockOutboundError, DirectGetBlockOutboundV1};
use super::direct_getblock_pending_v1::DIRECT_GETBLOCK_REQUEST_TTL;
use super::direct_getblock_v1::DirectGetBlockResponseV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockSwarmBindingErrorV1 {
    Ticket(DirectGetBlockOutboundError),
    NotConnected,
    DeadlineOverflow,
    DuplicateTransportId,
    UnknownTransportId,
    WrongTransportPeer,
}

#[derive(Debug, Clone)]
struct BoundRequest {
    target: PeerId,
    ticket_id: String,
    deadline: Instant,
}

/// Keep this object alive for the lifetime of a SINGLE actual Swarm Behaviour.
/// Never reuse IDs across Swarm restarts or swap in a different Behaviour.
#[derive(Debug)]
pub struct DirectGetBlockSwarmBindingsV1 {
    outbound: DirectGetBlockOutboundV1,
    by_transport_id: HashMap<OutboundRequestId, BoundRequest>,
}

impl DirectGetBlockSwarmBindingsV1 {
    pub fn new(
        chain_id: &str,
        local_noise_peer: PeerId,
    ) -> Result<Self, DirectGetBlockSwarmBindingErrorV1> {
        Ok(Self {
            outbound: DirectGetBlockOutboundV1::new(chain_id, local_noise_peer)
                .map_err(DirectGetBlockSwarmBindingErrorV1::Ticket)?,
            by_transport_id: HashMap::new(),
        })
    }

    /// The closure MUST perform exactly one send on the SAME real Swarm
    /// direct_getblock Behaviour that will emit the response/failure event.
    /// The requested peer must already be authenticated and connected.
    ///
    /// This API lets the actual Behaviour allocate the ID; no second offline
    /// Behaviour creates a different ID namespace.
    pub fn issue_with<F>(
        &mut self,
        authenticated_target: &PeerId,
        requested_hash: &str,
        swarm_connected: bool,
        now: Instant,
        send_on_live_behaviour: F,
    ) -> Result<OutboundRequestId, DirectGetBlockSwarmBindingErrorV1>
    where
        F: FnOnce(&PeerId, Vec<u8>) -> OutboundRequestId,
    {
        self.expire(now);
        if !swarm_connected {
            return Err(DirectGetBlockSwarmBindingErrorV1::NotConnected);
        }
        let deadline = now
            .checked_add(DIRECT_GETBLOCK_REQUEST_TTL)
            .ok_or(DirectGetBlockSwarmBindingErrorV1::DeadlineOverflow)?;
        let prepared = self
            .outbound
            .prepare(authenticated_target, requested_hash, now)
            .map_err(DirectGetBlockSwarmBindingErrorV1::Ticket)?;
        // The real Behaviour's returned ID is authoritative. Remote bytes
        // never supply or select the map key.
        let transport_id = send_on_live_behaviour(authenticated_target, prepared.wire);
        if self.by_transport_id.contains_key(&transport_id) {
            self.outbound
                .cancel_request(&prepared.request.request_id, authenticated_target);
            return Err(DirectGetBlockSwarmBindingErrorV1::DuplicateTransportId);
        }
        self.by_transport_id.insert(
            transport_id,
            BoundRequest {
                target: *authenticated_target,
                ticket_id: prepared.request.request_id,
                deadline,
            },
        );
        Ok(transport_id)
    }

    /// Only a real Swarm event's OutboundRequestId may select a slot.
    /// A response from another peer NEVER consumes a valid peer's ticket.
    pub fn accept_response(
        &mut self,
        transport_id: &OutboundRequestId,
        authenticated_peer: &PeerId,
        wire: &[u8],
        now: Instant,
    ) -> Result<DirectGetBlockResponseV1, DirectGetBlockSwarmBindingErrorV1> {
        self.expire(now);
        let bound = self
            .by_transport_id
            .get(transport_id)
            .ok_or(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)?;
        if &bound.target != authenticated_peer {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer);
        }
        let ticket_id = bound.ticket_id.clone();
        let outcome = self
            .outbound
            .complete_raw(&ticket_id, authenticated_peer, wire, now);
        // An authenticated target only has one response attempt. On invalid
        // bytes discard its slot; do not keep it alive for a replay attack.
        self.by_transport_id.remove(transport_id);
        self.outbound.cancel_request(&ticket_id, authenticated_peer);
        outcome.map_err(DirectGetBlockSwarmBindingErrorV1::Ticket)
    }

    /// OutboundFailure must use the peer identified by the actual Swarm event.
    pub fn outbound_failure(
        &mut self,
        transport_id: &OutboundRequestId,
        authenticated_target: &PeerId,
    ) -> Result<(), DirectGetBlockSwarmBindingErrorV1> {
        let bound = self
            .by_transport_id
            .get(transport_id)
            .ok_or(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)?;
        if &bound.target != authenticated_target {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer);
        }
        let ticket_id = bound.ticket_id.clone();
        self.by_transport_id.remove(transport_id);
        self.outbound.cancel_request(&ticket_id, authenticated_target);
        Ok(())
    }

    /// Call only when the Swarm reports the LAST authenticated connection to
    /// the peer closed (never after merely losing one parallel connection).
    pub fn last_connection_closed(&mut self, authenticated_peer: &PeerId) -> usize {
        let before = self.by_transport_id.len();
        self.by_transport_id
            .retain(|_, bound| &bound.target != authenticated_peer);
        self.outbound.cancel_peer(authenticated_peer);
        before - self.by_transport_id.len()
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.by_transport_id.len();
        self.by_transport_id.retain(|_, bound| now < bound.deadline);
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
    use super::*;
    use libp2p::{identity, request_response, StreamProtocol};
    use super::super::direct_getblock_codec_v1::{
        DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1,
    };
    use super::super::direct_getblock_v1::{
        encode_direct_getblock_response, DirectGetBlockRequestV1,
        DirectGetBlockResponseV1, DIRECT_GETBLOCK_PROTOCOL_V1,
    };
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

    fn missing_reply(
        raw_request: &[u8],
        target: &PeerId,
    ) -> Vec<u8> {
        let req: DirectGetBlockRequestV1 = serde_json::from_slice(raw_request).unwrap();
        let reply = DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: req.chain_id.clone(),
            request_id: req.request_id.clone(),
            responding_peer_id: target.to_string(),
            block_hash: req.block_hash.clone(),
            block: None,
        };
        encode_direct_getblock_response(&reply, &req, &target.to_string()).unwrap()
    }

    #[test]
    fn two_same_hash_requests_bind_to_actual_ids_from_single_behaviour() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "a".repeat(64);
        let now = Instant::now();
        let mut outgoing = Vec::<Vec<u8>>::new();
        let one = tracker.issue_with(&target, &hash, true, now, |p, raw| {
            outgoing.push(raw.clone());
            live.send_request(p, raw)
        }).unwrap();
        let two = tracker.issue_with(&target, &hash, true, now, |p, raw| {
            outgoing.push(raw.clone());
            live.send_request(p, raw)
        }).unwrap();
        assert_ne!(one, two);
        assert_ne!(
            serde_json::from_slice::<DirectGetBlockRequestV1>(&outgoing[0]).unwrap().request_id,
            serde_json::from_slice::<DirectGetBlockRequestV1>(&outgoing[1]).unwrap().request_id
        );
        assert_eq!(tracker.len(), 2);
        tracker.accept_response(&two, &target, &missing_reply(&outgoing[1], &target), now).unwrap();
        tracker.accept_response(&one, &target, &missing_reply(&outgoing[0], &target), now).unwrap();
        assert!(tracker.is_empty());
        assert!(matches!(
            tracker.accept_response(&one, &target, &missing_reply(&outgoing[0], &target), now),
            Err(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)
        ));
    }

    #[test]
    fn wrong_noise_peer_and_forged_ticket_never_consume_another_peer_slot() {
        let local = peer();
        let target = peer();
        let imposter = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "b".repeat(64);
        let now = Instant::now();
        let mut request = Vec::new();
        let id = tracker.issue_with(&target, &hash, true, now, |p, raw| {
            request = raw.clone();
            live.send_request(p, raw)
        }).unwrap();
        let valid = missing_reply(&request, &target);
        assert!(matches!(
            tracker.accept_response(&id, &imposter, &valid, now),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer)
        ));
        assert!(matches!(
            tracker.outbound_failure(&id, &imposter),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer)
        ));
        assert_eq!(tracker.len(), 1);
        let mut forged: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        forged["request_id"] = serde_json::json!("getblock:forged");
        assert!(matches!(
            tracker.accept_response(&id, &target, &serde_json::to_vec(&forged).unwrap(), now),
            Err(DirectGetBlockSwarmBindingErrorV1::Ticket(_))
        ));
        assert!(tracker.is_empty());
    }

    #[test]
    fn disconnect_expiry_and_no_connection_cleanup_are_bounded() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "c".repeat(64);
        let now = Instant::now();
        assert!(matches!(
            tracker.issue_with(&target, &hash, false, now, |p, raw| live.send_request(p, raw)),
            Err(DirectGetBlockSwarmBindingErrorV1::NotConnected)
        ));
        assert!(tracker.is_empty());
        let id = tracker.issue_with(&target, &hash, true, now, |p, raw| live.send_request(p, raw)).unwrap();
        assert_eq!(tracker.len(), 1);
        assert!(tracker.outbound_failure(&id, &target).is_ok());
        assert!(tracker.is_empty());
        tracker.issue_with(&target, &hash, true, now, |p, raw| live.send_request(p, raw)).unwrap();
        assert_eq!(tracker.last_connection_closed(&target), 1);
        assert!(tracker.is_empty());
        tracker.issue_with(&target, &hash, true, now, |p, raw| live.send_request(p, raw)).unwrap();
        assert_eq!(tracker.expire(now + DIRECT_GETBLOCK_REQUEST_TTL + Duration::from_millis(1)), 1);
        assert!(tracker.is_empty());
    }

    #[test]
    fn duplicate_transport_id_is_rejected_without_replacing_existing_binding() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "d".repeat(64);
        let now = Instant::now();
        let first = tracker.issue_with(&target, &hash, true, now, |p, raw| live.send_request(p, raw)).unwrap();
        assert!(matches!(
            tracker.issue_with(&target, &hash, true, now, |_, _| first),
            Err(DirectGetBlockSwarmBindingErrorV1::DuplicateTransportId)
        ));
        assert_eq!(tracker.len(), 1);
        tracker.outbound_failure(&first, &target).unwrap();
        assert!(tracker.is_empty());
    }
}
