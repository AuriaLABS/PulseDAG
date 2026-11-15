//! Strict authenticated inbound GetBlock responder, restricted to blocks
//! already accepted by the local authoritative DAG. The daemon's historical
//! storage responder is a separate future integration: *never* serve staging,
//! unverified gossip or an arbitrary block supplied by an untrusted caller.
//!
//! This API is deliberately dormant: no inbound libp2p protocol is advertised,
//! no transport ResponseChannel is connected, and no block is admitted.

use std::collections::HashMap;
use std::time::Instant;

use libp2p::PeerId;
use pulsedag_core::state::ChainState;

use super::direct_getblock_pending_v1::{
    DIRECT_GETBLOCK_REQUEST_TTL, MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL,
    MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER,
};
use super::direct_getblock_v1::{
    decode_direct_getblock_request, encode_direct_getblock_response, DirectGetBlockRequestV1,
    DirectGetBlockResponseV1, DirectGetBlockWireError, DIRECT_GETBLOCK_PROTOCOL_V1,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockInboundErrorV1 {
    Wire(DirectGetBlockWireError),
    SelfRequest,
    UnknownLocalToken,
    WrongAuthenticatedPeer,
    DuplicateRequest,
    GlobalLimit,
    PeerLimit,
    TokenOverflow,
    DeadlineOverflow,
}

#[derive(Debug, Clone)]
struct PendingInbound {
    request: DirectGetBlockRequestV1,
    authenticated_remote: PeerId,
    deadline: Instant,
}

/// A locally allocated receipt for a *real* libp2p inbound response channel.
/// The future Swarm integration must map this token to precisely ONE
/// ResponseChannel, never to a remote-supplied request ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectGetBlockInboundTicketV1 {
    pub local_token: u64,
    pub block_hash: String,
    pub authenticated_remote: PeerId,
}

#[derive(Debug)]
pub struct DirectGetBlockInboundV1 {
    chain_id: String,
    authenticated_local: PeerId,
    next_token: u64,
    pending: HashMap<u64, PendingInbound>,
}

impl DirectGetBlockInboundV1 {
    pub fn new(
        chain_id: &str,
        authenticated_local: PeerId,
    ) -> Result<Self, DirectGetBlockInboundErrorV1> {
        // Validate the chain domain early; an empty or invalid local domain
        // cannot become a listener that silently treats all requests as valid.
        if chain_id.is_empty() || chain_id.len() > 96 {
            return Err(DirectGetBlockInboundErrorV1::Wire(
                DirectGetBlockWireError::Invalid("invalid local chain domain"),
            ));
        }
        Ok(Self {
            chain_id: chain_id.to_owned(),
            authenticated_local,
            next_token: 0,
            pending: HashMap::new(),
        })
    }

    /// Call only with the actual remote PeerId from Noise libp2p Swarm events.
    /// The JSON's requesting_peer_id is checked against that identity and
    /// cannot grant itself authority. The caller must retain the matching
    /// ResponseChannel separately under the returned *local* token.
    pub fn accept(
        &mut self,
        raw: &[u8],
        authenticated_remote: &PeerId,
        now: Instant,
    ) -> Result<DirectGetBlockInboundTicketV1, DirectGetBlockInboundErrorV1> {
        self.expire(now);
        if authenticated_remote == &self.authenticated_local {
            return Err(DirectGetBlockInboundErrorV1::SelfRequest);
        }
        let request = decode_direct_getblock_request(
            raw,
            &self.chain_id,
            &authenticated_remote.to_string(),
            &self.authenticated_local.to_string(),
        )
        .map_err(DirectGetBlockInboundErrorV1::Wire)?;

        if self.pending.len() >= MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL {
            return Err(DirectGetBlockInboundErrorV1::GlobalLimit);
        }
        let same_peer = self
            .pending
            .values()
            .filter(|ticket| &ticket.authenticated_remote == authenticated_remote)
            .collect::<Vec<_>>();
        if same_peer.iter().any(|ticket| ticket.request.request_id == request.request_id) {
            return Err(DirectGetBlockInboundErrorV1::DuplicateRequest);
        }
        if same_peer.len() >= MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER {
            return Err(DirectGetBlockInboundErrorV1::PeerLimit);
        }
        let token = self
            .next_token
            .checked_add(1)
            .ok_or(DirectGetBlockInboundErrorV1::TokenOverflow)?;
        let deadline = now
            .checked_add(DIRECT_GETBLOCK_REQUEST_TTL)
            .ok_or(DirectGetBlockInboundErrorV1::DeadlineOverflow)?;
        self.pending.insert(
            token,
            PendingInbound {
                request: request.clone(),
                authenticated_remote: *authenticated_remote,
                deadline,
            },
        );
        self.next_token = token;
        Ok(DirectGetBlockInboundTicketV1 {
            local_token: token,
            block_hash: request.block_hash,
            authenticated_remote: *authenticated_remote,
        })
    }

    /// Return exactly the requested block ONLY if it is already present in
    /// ChainState's accepted DAG. Staged, pending or unverified blocks cannot
    /// enter this response through a caller-supplied Block parameter.
    ///
    /// A None response is explicitly authenticated to the original request.
    /// A separate verified historical-store adapter is needed after pruning.
    /// Output is raw bytes for the future matching ResponseChannel, NOT a
    /// consensus/monetary block-admission decision.
    pub fn reply_from_accepted_dag(
        &mut self,
        local_token: u64,
        authenticated_remote: &PeerId,
        accepted_chain: &ChainState,
        now: Instant,
    ) -> Result<Vec<u8>, DirectGetBlockInboundErrorV1> {
        self.expire(now);
        let pending = self
            .pending
            .get(&local_token)
            .ok_or(DirectGetBlockInboundErrorV1::UnknownLocalToken)?;
        if &pending.authenticated_remote != authenticated_remote {
            return Err(DirectGetBlockInboundErrorV1::WrongAuthenticatedPeer);
        }
        let request = pending.request.clone();
        let response = DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: request.chain_id.clone(),
            request_id: request.request_id.clone(),
            responding_peer_id: self.authenticated_local.to_string(),
            block_hash: request.block_hash.clone(),
            block: accepted_chain.dag.blocks.get(&request.block_hash).cloned(),
        };
        let result = encode_direct_getblock_response(
            &response,
            &request,
            &self.authenticated_local.to_string(),
        )
        .map_err(DirectGetBlockInboundErrorV1::Wire);
        // The local response channel is one shot even on encoding error.
        self.pending.remove(&local_token);
        result
    }

    pub fn cancel_peer(&mut self, authenticated_remote: &PeerId) -> usize {
        let before = self.pending.len();
        self.pending
            .retain(|_, ticket| &ticket.authenticated_remote != authenticated_remote);
        before - self.pending.len()
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, ticket| now < ticket.deadline);
        before - self.pending.len()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity;
    use pulsedag_core::genesis::init_chain_state;
    use std::time::Duration;

    fn peer() -> PeerId {
        PeerId::from(identity::Keypair::generate_ed25519().public())
    }

    fn raw(
        chain: &str,
        remote: &PeerId,
        local: &PeerId,
        hash: &str,
        suffix: u64,
    ) -> Vec<u8> {
        let request = DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: chain.to_owned(),
            request_id: format!("getblock:{remote}:{hash}:{suffix}"),
            requesting_peer_id: remote.to_string(),
            requested_peer_id: local.to_string(),
            block_hash: hash.to_owned(),
        };
        super::super::direct_getblock_v1::encode_direct_getblock_request(
            &request,
            chain,
            &remote.to_string(),
            &local.to_string(),
        )
        .unwrap()
    }

    #[test]
    fn authenticated_remote_identity_and_replay_fail_closed() {
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let now = Instant::now();
        let hash = "a".repeat(64);
        let wire = raw("inbound-test", &remote, &local, &hash, 1);
        let mut responder = DirectGetBlockInboundV1::new("inbound-test", local).unwrap();
        assert!(matches!(
            responder.accept(&wire, &attacker, now),
            Err(DirectGetBlockInboundErrorV1::Wire(_))
        ));
        assert!(matches!(
            responder.accept(&wire, &local, now),
            Err(DirectGetBlockInboundErrorV1::SelfRequest)
        ));
        let ticket = responder.accept(&wire, &remote, now).unwrap();
        assert_eq!(ticket.block_hash, hash);
        assert!(matches!(
            responder.accept(&wire, &remote, now),
            Err(DirectGetBlockInboundErrorV1::DuplicateRequest)
        ));
        assert_eq!(responder.len(), 1);
        let accepted = init_chain_state("inbound-test".into());
        assert!(matches!(
            responder.reply_from_accepted_dag(ticket.local_token, &attacker, &accepted, now),
            Err(DirectGetBlockInboundErrorV1::WrongAuthenticatedPeer)
        ));
        assert_eq!(responder.len(), 1);
        let response = responder
            .reply_from_accepted_dag(ticket.local_token, &remote, &accepted, now)
            .unwrap();
        let request = decode_direct_getblock_request(
            &wire, "inbound-test", &remote.to_string(), &local.to_string()
        ).unwrap();
        let decoded = super::super::direct_getblock_v1::decode_direct_getblock_response(
            &response, &request, &local.to_string()
        ).unwrap();
        assert!(decoded.block.is_none());
        assert_eq!(responder.len(), 0);
        assert!(matches!(
            responder.reply_from_accepted_dag(ticket.local_token, &remote, &accepted, now),
            Err(DirectGetBlockInboundErrorV1::UnknownLocalToken)
        ));
    }

    #[test]
    fn only_chain_accepted_blocks_are_served_not_arbitrary_staging() {
        let local = peer();
        let remote = peer();
        let now = Instant::now();
        let chain = init_chain_state("inbound-test".into());
        let canonical_hash = chain.dag.genesis_hash.clone();
        let mut responder = DirectGetBlockInboundV1::new("inbound-test", local).unwrap();
        let canonical_wire = raw("inbound-test", &remote, &local, &canonical_hash, 1);
        let canonical_ticket = responder.accept(&canonical_wire, &remote, now).unwrap();
        let bytes = responder
            .reply_from_accepted_dag(canonical_ticket.local_token, &remote, &chain, now)
            .unwrap();
        let original = decode_direct_getblock_request(
            &canonical_wire, "inbound-test", &remote.to_string(), &local.to_string()
        ).unwrap();
        let response = super::super::direct_getblock_v1::decode_direct_getblock_response(
            &bytes, &original, &local.to_string()
        ).unwrap();
        assert_eq!(response.block.as_ref().map(|b| b.hash.as_str()), Some(canonical_hash.as_str()));

        let staged_hash = "b".repeat(64);
        let mut fake_staged = chain.dag.blocks.get(&canonical_hash).unwrap().clone();
        fake_staged.hash = staged_hash.clone();
        assert!(!chain.dag.blocks.contains_key(&fake_staged.hash));
        let staged_wire = raw("inbound-test", &remote, &local, &staged_hash, 2);
        let staged = responder.accept(&staged_wire, &remote, now).unwrap();
        let bytes = responder
            .reply_from_accepted_dag(staged.local_token, &remote, &chain, now)
            .unwrap();
        let request = decode_direct_getblock_request(
            &staged_wire, "inbound-test", &remote.to_string(), &local.to_string()
        ).unwrap();
        let response = super::super::direct_getblock_v1::decode_direct_getblock_response(
            &bytes, &request, &local.to_string()
        ).unwrap();
        assert!(response.block.is_none(), "arbitrary staged data must never be served");
    }

    #[test]
    fn inbound_quota_expiry_and_disconnect_never_leak_tickets() {
        let local = peer();
        let remote = peer();
        let other = peer();
        let hash = "a".repeat(64);
        let now = Instant::now();
        let mut responder = DirectGetBlockInboundV1::new("inbound-test", local).unwrap();
        for i in 0..4 {
            let wire = raw("inbound-test", &remote, &local, &hash, i);
            responder.accept(&wire, &remote, now).unwrap();
        }
        assert_eq!(responder.len(), 4);
        assert!(matches!(
            responder.accept(&raw("inbound-test", &remote, &local, &hash, 5), &remote, now),
            Err(DirectGetBlockInboundErrorV1::PeerLimit)
        ));
        let other_ticket = responder
            .accept(&raw("inbound-test", &other, &local, &hash, 1), &other, now)
            .unwrap();
        assert_eq!(responder.cancel_peer(&remote), 4);
        assert_eq!(responder.len(), 1);
        assert_eq!(responder.cancel_peer(&remote), 0);
        assert_eq!(responder.expire(now + Duration::from_secs(8)), 1);
        assert_eq!(responder.len(), 0);
        let chain = init_chain_state("inbound-test".into());
        assert!(matches!(
            responder.reply_from_accepted_dag(
                other_ticket.local_token, &other, &chain, now + Duration::from_secs(8)
            ),
            Err(DirectGetBlockInboundErrorV1::UnknownLocalToken)
        ));
    }

    #[test]
    fn global_cap_and_token_overflow_fail_closed() {
        let local = peer();
        let remote = peer();
        let now = Instant::now();
        let mut responder = DirectGetBlockInboundV1::new("inbound-test", local).unwrap();
        responder.next_token = u64::MAX;
        assert!(matches!(
            responder.accept(&raw("inbound-test", &remote, &local, &"a".repeat(64), 1), &remote, now),
            Err(DirectGetBlockInboundErrorV1::TokenOverflow)
        ));
        assert_eq!(responder.len(), 0);
        responder.next_token = 0;
        let peers: Vec<_> = (0..8).map(|_| peer()).collect();
        for (i, remote) in peers.iter().enumerate() {
            for j in 0..4 {
                responder.accept(
                    &raw("inbound-test", remote, &local, &"a".repeat(64), (i * 4 + j) as u64),
                    remote, now
                ).unwrap();
            }
        }
        assert_eq!(responder.len(), MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL);
        assert!(matches!(
            responder.accept(
                &raw("inbound-test", &remote, &local, &"a".repeat(64), 1), &remote, now
            ),
            Err(DirectGetBlockInboundErrorV1::GlobalLimit)
        ));
    }
}
