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
use crate::{InboundEvent, PeerAddressedBlockRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockSwarmBindingErrorV1 {
    Ticket(DirectGetBlockOutboundError),
    NotConnected,
    DeadlineOverflow,
    DuplicateTransportId,
    UnknownTransportId,
    WrongTransportPeer,
    WrongLocalRequest,
}

#[derive(Debug, Clone)]
struct BoundRequest {
    target: PeerId,
    ticket_id: String,
    requested_hash: String,
    local_daemon_request_id: Option<String>,
    // Retain the complete daemon-local correlation in the SAME bounded book.
    // Never reconstruct the original from untrusted wire reply fields.
    local_daemon_request: Option<PeerAddressedBlockRequest>,
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
                requested_hash: prepared.request.block_hash,
                local_daemon_request_id: None,
                local_daemon_request: None,
                deadline,
            },
        );
        Ok(transport_id)
    }

    /// Prevalidate a locally minted daemon request BEFORE calling the actual
    /// authenticated Swarm sender. The wire ticket, real OutboundRequestId and
    /// complete original daemon request are then bound in this SAME book.
    ///
    /// In particular, a duplicated daemon-local request ID cannot cause an
    /// additional wire send, even if the destination and hash are identical.
    /// This adapter does not itself turn on production networking: callers
    /// must supply the real Behaviour send closure when separately authorized.
    pub fn issue_daemon_request_with<F>(
        &mut self,
        original: &PeerAddressedBlockRequest,
        swarm_connected: bool,
        now: Instant,
        send_on_live_behaviour: F,
    ) -> Result<OutboundRequestId, DirectGetBlockSwarmBindingErrorV1>
    where
        F: FnOnce(&PeerId, Vec<u8>) -> OutboundRequestId,
    {
        self.expire(now);
        let target: PeerId = original
            .requested_peer_id
            .parse()
            .map_err(|_| DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)?;
        if original.request_id.len() < 10
            || original.request_id.len() > 256
            || !original.request_id.starts_with("getblock:")
            || !original.request_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-' | b'_' | b'/')
            })
            || self.by_transport_id.values().any(|bound| {
                bound.local_daemon_request_id.as_deref() == Some(original.request_id.as_str())
            })
        {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest);
        }
        let transport_id = self.issue_with(
            &target,
            &original.block_hash,
            swarm_connected,
            now,
            send_on_live_behaviour,
        )?;
        if let Err(error) = self.bind_local_daemon_request(&transport_id, original) {
            // A rejection must not leave an internally valid ticket or quota.
            // The actual Swarm sender, if invoked, cannot be unsent; this path
            // is only a defensive race/consistency fallback.
            let _ = self.outbound_failure(&transport_id, &target);
            return Err(error);
        }
        Ok(transport_id)
    }

    /// Bind the exact daemon-local request ID to the REAL outbound transport ID.
    /// This must run synchronously after issue_with in the Swarm event loop,
    /// before any response event can be handled. Same peer/hash concurrent
    /// requests must never allow swapping one daemon's request ID for another.
    pub fn bind_local_daemon_request(
        &mut self,
        transport_id: &OutboundRequestId,
        original: &PeerAddressedBlockRequest,
    ) -> Result<(), DirectGetBlockSwarmBindingErrorV1> {
        if self.by_transport_id.values().any(|bound| {
            bound.local_daemon_request_id.as_deref() == Some(original.request_id.as_str())
        }) {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest);
        }
        let bound = self
            .by_transport_id
            .get_mut(transport_id)
            .ok_or(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)?;
        if bound.local_daemon_request_id.is_some()
            || original.requested_peer_id != bound.target.to_string()
            || original.block_hash != bound.requested_hash
            || !original.request_id.starts_with("getblock:")
            || original.request_id.len() > 256
        {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest);
        }
        bound.local_daemon_request_id = Some(original.request_id.clone());
        bound.local_daemon_request = Some(original.clone());
        Ok(())
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

    /// Convert an authenticated, strictly correlated response to the EXISTING
    /// daemon ingress event. This deliberately does not validate or insert
    /// the block in consensus: InboundEvent::Block must traverse the daemon's
    /// normal full PoW/ancestry/monetary admission pipeline.
    ///
    /// The original legacy request is local-only state paired with the real
    /// OutboundRequestId by the runtime. It must NEVER be read from reply JSON.
    /// Verify its exact destination/hash BEFORE consuming the outbound ticket.
    /// Still dormant until the production Swarm sends requests and the daemon
    /// response path is connected.
    pub fn deliver_verified_response_to_daemon(
        &mut self,
        transport_id: &OutboundRequestId,
        authenticated_peer: &PeerId,
        raw: &[u8],
        original_local_request: &PeerAddressedBlockRequest,
        now: Instant,
    ) -> Result<InboundEvent, DirectGetBlockSwarmBindingErrorV1> {
        self.expire(now);
        let bound = self
            .by_transport_id
            .get(transport_id)
            .ok_or(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)?;
        if &bound.target != authenticated_peer {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer);
        }
        if original_local_request.requested_peer_id != authenticated_peer.to_string()
            || original_local_request.block_hash != bound.requested_hash
            || bound.local_daemon_request_id.as_deref()
                != Some(original_local_request.request_id.as_str())
        {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest);
        }
        // Checks response transport-ID/peer/chain domain/wire request-ID/hash
        // against the ticket prepared locally, then removes the one-shot slot.
        let response = self.accept_response(transport_id, authenticated_peer, raw, now)?;
        Ok(match response.block {
            Some(block) => InboundEvent::Block {
                peer_id: Some(authenticated_peer.to_string()),
                block,
            },
            None => InboundEvent::BlockDataMissing {
                hash: Some(response.block_hash),
                peer_id: Some(authenticated_peer.to_string()),
                request_id: Some(original_local_request.request_id.clone()),
            },
        })
    }

    /// Deliver from the SAME Swarm-owned request book, without a second runtime
    /// HashMap from OutboundRequestId to daemon-local request. The authenticated
    /// peer, wire ticket, exact hash and locally minted request ID are verified
    /// by deliver_verified_response_to_daemon before any daemon event escapes.
    /// The response is still subject to normal daemon consensus admission.
    /// A wrong peer or unbound ticket never consumes another request's slot.
    pub fn deliver_bound_response_to_daemon(
        &mut self,
        transport_id: &OutboundRequestId,
        authenticated_peer: &PeerId,
        raw: &[u8],
        now: Instant,
    ) -> Result<InboundEvent, DirectGetBlockSwarmBindingErrorV1> {
        self.expire(now);
        let bound = self
            .by_transport_id
            .get(transport_id)
            .ok_or(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)?;
        let original = bound
            .local_daemon_request
            .as_ref()
            .ok_or(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)?;
        if bound.local_daemon_request_id.as_deref() != Some(original.request_id.as_str()) {
            return Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest);
        }
        let original = original.clone();
        self.deliver_verified_response_to_daemon(
            transport_id,
            authenticated_peer,
            raw,
            &original,
            now,
        )
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
        self.outbound
            .cancel_request(&ticket_id, authenticated_target);
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
    use super::super::direct_getblock_codec_v1::{
        DirectGetBlockRawCodecV1, DIRECT_GETBLOCK_STREAM_PROTOCOL_V1,
    };
    use super::super::direct_getblock_v1::{
        encode_direct_getblock_response, DirectGetBlockRequestV1, DirectGetBlockResponseV1,
        DIRECT_GETBLOCK_PROTOCOL_V1,
    };
    use super::*;
    use libp2p::{identity, request_response, StreamProtocol};
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

    fn missing_reply(raw_request: &[u8], target: &PeerId) -> Vec<u8> {
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
        let one = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                outgoing.push(raw.clone());
                live.send_request(p, raw)
            })
            .unwrap();
        let two = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                outgoing.push(raw.clone());
                live.send_request(p, raw)
            })
            .unwrap();
        assert_ne!(one, two);
        assert_ne!(
            serde_json::from_slice::<DirectGetBlockRequestV1>(&outgoing[0])
                .unwrap()
                .request_id,
            serde_json::from_slice::<DirectGetBlockRequestV1>(&outgoing[1])
                .unwrap()
                .request_id
        );
        assert_eq!(tracker.len(), 2);
        tracker
            .accept_response(&two, &target, &missing_reply(&outgoing[1], &target), now)
            .unwrap();
        tracker
            .accept_response(&one, &target, &missing_reply(&outgoing[0], &target), now)
            .unwrap();
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
        let id = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                request = raw.clone();
                live.send_request(p, raw)
            })
            .unwrap();
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
            tracker.issue_with(&target, &hash, false, now, |p, raw| live
                .send_request(p, raw)),
            Err(DirectGetBlockSwarmBindingErrorV1::NotConnected)
        ));
        assert!(tracker.is_empty());
        let id = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                live.send_request(p, raw)
            })
            .unwrap();
        assert_eq!(tracker.len(), 1);
        assert!(tracker.outbound_failure(&id, &target).is_ok());
        assert!(tracker.is_empty());
        tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                live.send_request(p, raw)
            })
            .unwrap();
        assert_eq!(tracker.last_connection_closed(&target), 1);
        assert!(tracker.is_empty());
        tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                live.send_request(p, raw)
            })
            .unwrap();
        assert_eq!(
            tracker.expire(now + DIRECT_GETBLOCK_REQUEST_TTL + Duration::from_millis(1)),
            1
        );
        assert!(tracker.is_empty());
    }

    fn local_request(peer: &PeerId, hash: &str) -> PeerAddressedBlockRequest {
        PeerAddressedBlockRequest {
            request_id: "getblock:locally-allocated:1".to_string(),
            requested_peer_id: peer.to_string(),
            block_hash: hash.to_string(),
            sent_at: 0,
        }
    }

    #[test]
    fn authenticated_accepted_block_is_delivered_only_to_daemon_ingress() {
        let local = peer();
        let target = peer();
        let chain = pulsedag_core::genesis::init_chain_state("swarm-bindings-test".to_string());
        let hash = chain.dag.genesis_hash.clone();
        let block = chain.dag.blocks.get(&hash).unwrap().clone();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let now = Instant::now();
        let mut outgoing = Vec::new();
        let id = tracker
            .issue_with(&target, &hash, true, now, |peer, raw| {
                outgoing = raw.clone();
                live.send_request(peer, raw)
            })
            .unwrap();
        let request: DirectGetBlockRequestV1 = serde_json::from_slice(&outgoing).unwrap();
        let reply = DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: request.chain_id.clone(),
            request_id: request.request_id.clone(),
            responding_peer_id: target.to_string(),
            block_hash: hash.clone(),
            block: Some(block),
        };
        let raw = encode_direct_getblock_response(&reply, &request, &target.to_string()).unwrap();
        let local_request = local_request(&target, &hash);
        tracker
            .bind_local_daemon_request(&id, &local_request)
            .unwrap();
        let event = tracker
            .deliver_verified_response_to_daemon(&id, &target, &raw, &local_request, now)
            .unwrap();
        assert!(matches!(
            event,
            InboundEvent::Block { peer_id: Some(ref actual_peer), block }
                if actual_peer == &target.to_string() && block.hash == hash
        ));
        assert!(tracker.is_empty());
        assert!(matches!(
            tracker.deliver_verified_response_to_daemon(&id, &target, &raw, &local_request, now),
            Err(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)
        ));
    }

    #[test]
    fn authenticated_missing_result_uses_local_request_identity_not_remote_claim() {
        let local = peer();
        let target = peer();
        let impostor = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "e".repeat(64);
        let now = Instant::now();
        let mut outgoing = Vec::new();
        let id = tracker
            .issue_with(&target, &hash, true, now, |peer, raw| {
                outgoing = raw.clone();
                live.send_request(peer, raw)
            })
            .unwrap();
        let raw_reply = missing_reply(&outgoing, &target);
        let request = local_request(&target, &hash);
        tracker.bind_local_daemon_request(&id, &request).unwrap();
        let mut wrong_request = request.clone();
        wrong_request.block_hash = "f".repeat(64);
        assert!(matches!(
            tracker.deliver_verified_response_to_daemon(
                &id,
                &target,
                &raw_reply,
                &wrong_request,
                now
            ),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        assert!(matches!(
            tracker.deliver_verified_response_to_daemon(
                &id,
                &impostor,
                &raw_reply,
                &local_request(&target, &hash),
                now
            ),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer)
        ));
        assert_eq!(tracker.len(), 1);
        let event = tracker
            .deliver_verified_response_to_daemon(&id, &target, &raw_reply, &request, now)
            .unwrap();
        assert!(matches!(
            event,
            InboundEvent::BlockDataMissing {
                hash: Some(ref actual_hash),
                peer_id: Some(ref actual_peer),
                request_id: Some(ref actual_request),
            } if actual_hash == &hash
                && actual_peer == &target.to_string()
                && actual_request == &request.request_id
        ));
        assert!(tracker.is_empty());
    }

    #[test]
    fn malformed_authenticated_response_never_emits_daemon_event() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "a".repeat(64);
        let now = Instant::now();
        let mut outgoing = Vec::new();
        let id = tracker
            .issue_with(&target, &hash, true, now, |peer, raw| {
                outgoing = raw.clone();
                live.send_request(peer, raw)
            })
            .unwrap();
        let mut forged: serde_json::Value =
            serde_json::from_slice(&missing_reply(&outgoing, &target)).unwrap();
        forged["chain_id"] = serde_json::json!("foreign-chain");
        let request = local_request(&target, &hash);
        tracker.bind_local_daemon_request(&id, &request).unwrap();
        assert!(matches!(
            tracker.deliver_verified_response_to_daemon(
                &id,
                &target,
                &serde_json::to_vec(&forged).unwrap(),
                &request,
                now
            ),
            Err(DirectGetBlockSwarmBindingErrorV1::Ticket(_))
        ));
        assert!(tracker.is_empty());
    }

    #[test]
    fn simultaneous_same_peer_and_hash_cannot_swap_local_daemon_request_ids() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "c".repeat(64);
        let now = Instant::now();
        let mut first_wire = Vec::new();
        let first = tracker
            .issue_with(&target, &hash, true, now, |peer, raw| {
                first_wire = raw.clone();
                live.send_request(peer, raw)
            })
            .unwrap();
        let mut second_wire = Vec::new();
        let second = tracker
            .issue_with(&target, &hash, true, now, |peer, raw| {
                second_wire = raw.clone();
                live.send_request(peer, raw)
            })
            .unwrap();
        let first_local = local_request(&target, &hash);
        let mut second_local = local_request(&target, &hash);
        second_local.request_id = "getblock:locally-allocated:2".to_string();
        tracker
            .bind_local_daemon_request(&first, &first_local)
            .unwrap();
        assert!(matches!(
            tracker.bind_local_daemon_request(&second, &first_local),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        tracker
            .bind_local_daemon_request(&second, &second_local)
            .unwrap();
        assert!(matches!(
            tracker.bind_local_daemon_request(&first, &second_local),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        assert!(matches!(
            tracker.deliver_verified_response_to_daemon(
                &second,
                &target,
                &missing_reply(&second_wire, &target),
                &first_local,
                now,
            ),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        assert_eq!(tracker.len(), 2);
        let second_event = tracker
            .deliver_verified_response_to_daemon(
                &second,
                &target,
                &missing_reply(&second_wire, &target),
                &second_local,
                now,
            )
            .unwrap();
        assert!(matches!(
            second_event,
            InboundEvent::BlockDataMissing {
                request_id: Some(ref id),
                ..
            } if id == &second_local.request_id
        ));
        let first_event = tracker
            .deliver_verified_response_to_daemon(
                &first,
                &target,
                &missing_reply(&first_wire, &target),
                &first_local,
                now,
            )
            .unwrap();
        assert!(matches!(
            first_event,
            InboundEvent::BlockDataMissing {
                request_id: Some(ref id),
                ..
            } if id == &first_local.request_id
        ));
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
        let first = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                live.send_request(p, raw)
            })
            .unwrap();
        assert!(matches!(
            tracker.issue_with(&target, &hash, true, now, |_, _| first),
            Err(DirectGetBlockSwarmBindingErrorV1::DuplicateTransportId)
        ));
        assert_eq!(tracker.len(), 1);
        tracker.outbound_failure(&first, &target).unwrap();
        assert!(tracker.is_empty());
    }

    #[test]
    fn bound_delivery_rejects_unregistered_peer_and_replay_without_second_map() {
        let local = peer();
        let target = peer();
        let wrong_peer = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "e".repeat(64);
        let now = Instant::now();
        let mut first_wire = Vec::new();
        let first = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                first_wire = raw.clone();
                live.send_request(p, raw)
            })
            .unwrap();
        let mut second_wire = Vec::new();
        let second = tracker
            .issue_with(&target, &hash, true, now, |p, raw| {
                second_wire = raw.clone();
                live.send_request(p, raw)
            })
            .unwrap();
        let first_local = local_request(&target, &hash);
        let mut second_local = local_request(&target, &hash);
        second_local.request_id = "getblock:locally-allocated:2".to_string();
        let first_reply = missing_reply(&first_wire, &target);
        let second_reply = missing_reply(&second_wire, &target);
        assert!(matches!(
            tracker.deliver_bound_response_to_daemon(&first, &target, &first_reply, now),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        tracker
            .bind_local_daemon_request(&first, &first_local)
            .unwrap();
        tracker
            .bind_local_daemon_request(&second, &second_local)
            .unwrap();
        assert!(matches!(
            tracker.deliver_bound_response_to_daemon(&second, &wrong_peer, &second_reply, now),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongTransportPeer)
        ));
        assert_eq!(tracker.len(), 2);
        let second_event = tracker
            .deliver_bound_response_to_daemon(&second, &target, &second_reply, now)
            .unwrap();
        assert!(matches!(
            second_event,
            InboundEvent::BlockDataMissing {
                request_id: Some(ref id),
                ..
            } if id == &second_local.request_id
        ));
        let first_event = tracker
            .deliver_bound_response_to_daemon(&first, &target, &first_reply, now)
            .unwrap();
        assert!(matches!(
            first_event,
            InboundEvent::BlockDataMissing {
                request_id: Some(ref id),
                ..
            } if id == &first_local.request_id
        ));
        assert!(tracker.is_empty());
        assert!(matches!(
            tracker.deliver_bound_response_to_daemon(&first, &target, &first_reply, now),
            Err(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)
        ));
    }

    #[test]
    fn bound_delivery_slots_expire_and_disconnect_fail_closed() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let hash = "f".repeat(64);
        let now = Instant::now();
        let mut raw = Vec::new();
        let id = tracker
            .issue_with(&target, &hash, true, now, |p, request| {
                raw = request.clone();
                live.send_request(p, request)
            })
            .unwrap();
        tracker
            .bind_local_daemon_request(&id, &local_request(&target, &hash))
            .unwrap();
        let reply = missing_reply(&raw, &target);
        assert_eq!(tracker.last_connection_closed(&target), 1);
        assert!(matches!(
            tracker.deliver_bound_response_to_daemon(&id, &target, &reply, now),
            Err(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)
        ));
        let new_id = tracker
            .issue_with(&target, &hash, true, now, |p, request| {
                live.send_request(p, request)
            })
            .unwrap();
        tracker
            .bind_local_daemon_request(&new_id, &local_request(&target, &hash))
            .unwrap();
        assert_eq!(tracker.expire(now + DIRECT_GETBLOCK_REQUEST_TTL), 1);
        assert!(matches!(
            tracker.deliver_bound_response_to_daemon(
                &new_id,
                &target,
                &reply,
                now + DIRECT_GETBLOCK_REQUEST_TTL
            ),
            Err(DirectGetBlockSwarmBindingErrorV1::UnknownTransportId)
        ));
    }
    #[test]
    fn atomic_daemon_dispatch_validates_before_any_swarm_send() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let now = Instant::now();
        let hash = "a".repeat(64);
        let original = local_request(&target, &hash);
        let mut sent = 0_usize;
        let id = tracker
            .issue_daemon_request_with(&original, true, now, |p, raw| {
                sent += 1;
                live.send_request(p, raw)
            })
            .unwrap();
        assert_eq!(sent, 1);
        assert_eq!(tracker.len(), 1);
        assert!(matches!(
            tracker.issue_daemon_request_with(&original, true, now, |p, raw| {
                sent += 1;
                live.send_request(p, raw)
            }),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        assert_eq!(
            sent, 1,
            "duplicate local ID must never send a second request"
        );
        let mut malformed = original.clone();
        malformed.request_id = "getblock:bad id".to_string();
        assert!(matches!(
            tracker.issue_daemon_request_with(&malformed, true, now, |p, raw| {
                sent += 1;
                live.send_request(p, raw)
            }),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        let mut invalid_target = original.clone();
        invalid_target.requested_peer_id = "not:a:peer".to_string();
        assert!(matches!(
            tracker.issue_daemon_request_with(&invalid_target, true, now, |p, raw| {
                sent += 1;
                live.send_request(p, raw)
            }),
            Err(DirectGetBlockSwarmBindingErrorV1::WrongLocalRequest)
        ));
        assert_eq!(sent, 1);
        tracker.outbound_failure(&id, &target).unwrap();
        assert!(tracker.is_empty());
    }

    #[test]
    fn atomic_dispatch_binds_two_same_hash_requests_and_exact_replies() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let now = Instant::now();
        let hash = "b".repeat(64);
        let first_local = local_request(&target, &hash);
        let mut second_local = first_local.clone();
        second_local.request_id = "getblock:locally-allocated:2".into();
        let mut first_wire = Vec::new();
        let first = tracker
            .issue_daemon_request_with(&first_local, true, now, |p, raw| {
                first_wire = raw.clone();
                live.send_request(p, raw)
            })
            .unwrap();
        let mut second_wire = Vec::new();
        let second = tracker
            .issue_daemon_request_with(&second_local, true, now, |p, raw| {
                second_wire = raw.clone();
                live.send_request(p, raw)
            })
            .unwrap();
        assert_ne!(first, second);
        let second_reply = missing_reply(&second_wire, &target);
        let first_reply = missing_reply(&first_wire, &target);
        let second_event = tracker
            .deliver_bound_response_to_daemon(&second, &target, &second_reply, now)
            .unwrap();
        assert!(matches!(
            second_event,
            InboundEvent::BlockDataMissing {
                request_id: Some(ref id),
                ..
            } if id == &second_local.request_id
        ));
        let first_event = tracker
            .deliver_bound_response_to_daemon(&first, &target, &first_reply, now)
            .unwrap();
        assert!(matches!(
            first_event,
            InboundEvent::BlockDataMissing {
                request_id: Some(ref id),
                ..
            } if id == &first_local.request_id
        ));
        assert!(tracker.is_empty());
    }

    #[test]
    fn atomic_dispatch_disconnected_peer_does_not_issue_wire_request() {
        let local = peer();
        let target = peer();
        let mut live = behaviour();
        let mut tracker = DirectGetBlockSwarmBindingsV1::new("swarm-bindings-test", local).unwrap();
        let original = local_request(&target, &"c".repeat(64));
        let mut sent = 0_usize;
        assert!(matches!(
            tracker.issue_daemon_request_with(&original, false, Instant::now(), |p, raw| {
                sent += 1;
                live.send_request(p, raw)
            }),
            Err(DirectGetBlockSwarmBindingErrorV1::NotConnected)
        ));
        assert_eq!(sent, 0);
        assert!(tracker.is_empty());
    }
}
