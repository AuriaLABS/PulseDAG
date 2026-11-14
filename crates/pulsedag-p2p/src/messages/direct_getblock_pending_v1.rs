//! Bounded pending-request book for the future authenticated direct GetBlock
//! substream. Dormant: no Swarm hooks, consensus effects or block admission.
//! The authenticated peer id MUST originate from Noise/libp2p transport.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::direct_getblock_v1::{
    validate_direct_getblock_request, validate_direct_getblock_response, DirectGetBlockRequestV1,
    DirectGetBlockResponseV1, DirectGetBlockWireError,
};

pub const MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL: usize = 32;
pub const MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER: usize = 4;
pub const DIRECT_GETBLOCK_REQUEST_TTL: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockPendingError {
    Wire(DirectGetBlockWireError),
    InvalidIdentity,
    Duplicate,
    NonMonotonicRequestId,
    LimitExceeded,
    Unsolicited,
    Expired,
    DeadlineOverflow,
}

impl From<DirectGetBlockWireError> for DirectGetBlockPendingError {
    fn from(error: DirectGetBlockWireError) -> Self {
        Self::Wire(error)
    }
}

#[derive(Debug, Clone)]
struct Pending {
    request: DirectGetBlockRequestV1,
    target_peer_id: String,
    deadline: Instant,
}

/// Every in-flight response is tied to its exact authenticated request.
#[derive(Debug)]
pub struct DirectGetBlockPendingBookV1 {
    chain_id: String,
    local_peer_id: String,
    pending: BTreeMap<String, Pending>,
    // A lifetime-monotonic high-water mark prevents stale responses from a
    // completed, timed-out or cancelled request claiming a new request slot.
    // Unlike an unbounded used-ID set, this has constant memory.
    last_issued_sequence: u64,
}

impl DirectGetBlockPendingBookV1 {
    pub fn new(
        chain_id: &str,
        authenticated_local_peer_id: &str,
    ) -> Result<Self, DirectGetBlockPendingError> {
        if chain_id.is_empty()
            || chain_id.len() > 96
            || authenticated_local_peer_id.is_empty()
            || authenticated_local_peer_id.len() > 128
            || !authenticated_local_peer_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric())
        {
            return Err(DirectGetBlockPendingError::InvalidIdentity);
        }
        Ok(Self {
            chain_id: chain_id.to_owned(),
            local_peer_id: authenticated_local_peer_id.to_owned(),
            pending: BTreeMap::new(),
            last_issued_sequence: 0,
        })
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn begin(
        &mut self,
        request: DirectGetBlockRequestV1,
        authenticated_target_peer_id: &str,
        now: Instant,
    ) -> Result<(), DirectGetBlockPendingError> {
        validate_direct_getblock_request(
            &request,
            &self.chain_id,
            &self.local_peer_id,
            authenticated_target_peer_id,
        )?;
        if self.pending.contains_key(&request.request_id) {
            return Err(DirectGetBlockPendingError::Duplicate);
        }
        // Correlation IDs are minted by the originating authenticated handle
        // with a checked monotonic :u64 suffix. For this book every new request
        // must advance that suffix; old response IDs can never be reused.
        // This book must outlive individual direct-peer sessions; never reset
        // it on reconnect or reuse an ID from a previous transport instance.
        let sequence_text = request
            .request_id
            .rsplit_once(':')
            .map(|(_, suffix)| suffix)
            .ok_or(DirectGetBlockPendingError::NonMonotonicRequestId)?;
        let sequence: u64 = sequence_text
            .parse()
            .map_err(|_| DirectGetBlockPendingError::NonMonotonicRequestId)?;
        if sequence == 0
            || sequence.to_string() != sequence_text
            || sequence <= self.last_issued_sequence
        {
            return Err(DirectGetBlockPendingError::NonMonotonicRequestId);
        }
        self.expire(now);
        if self.pending.len() >= MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL
            || self
                .pending
                .values()
                .filter(|value| value.target_peer_id == authenticated_target_peer_id)
                .count()
                >= MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER
        {
            return Err(DirectGetBlockPendingError::LimitExceeded);
        }
        let deadline = now
            .checked_add(DIRECT_GETBLOCK_REQUEST_TTL)
            .ok_or(DirectGetBlockPendingError::DeadlineOverflow)?;
        self.pending.insert(
            request.request_id.clone(),
            Pending {
                request,
                target_peer_id: authenticated_target_peer_id.to_owned(),
                deadline,
            },
        );
        self.last_issued_sequence = sequence;
        Ok(())
    }

    /// A forged response never consumes another authenticated peer's pending
    /// request. A valid response clears its slot for the next ancestor.
    /// The caller MUST separately authenticate block PoW, parents and state.
    pub fn complete(
        &mut self,
        response: DirectGetBlockResponseV1,
        authenticated_remote_peer_id: &str,
        now: Instant,
    ) -> Result<DirectGetBlockResponseV1, DirectGetBlockPendingError> {
        let pending = self
            .pending
            .get(&response.request_id)
            .ok_or(DirectGetBlockPendingError::Unsolicited)?;
        // Authenticate the remote transport identity and exact response
        // *before* mutating any pending slot, even if its TTL has elapsed.
        // A peer that only guesses a pending ID may not expire another peer's
        // request via complete(); timer housekeeping uses expire().
        if authenticated_remote_peer_id != pending.target_peer_id {
            return Err(DirectGetBlockPendingError::Wire(
                DirectGetBlockWireError::Invalid("unexpected transport responder"),
            ));
        }
        validate_direct_getblock_response(
            &response,
            &pending.request,
            authenticated_remote_peer_id,
        )?;
        if now >= pending.deadline {
            self.pending.remove(&response.request_id);
            return Err(DirectGetBlockPendingError::Expired);
        }
        self.pending.remove(&response.request_id);
        Ok(response)
    }

    pub fn expire(&mut self, now: Instant) -> usize {
        let before = self.pending.len();
        self.pending.retain(|_, request| now < request.deadline);
        before - self.pending.len()
    }

    /// Called on authenticated peer disconnect / revocation, not just when
    /// a gossip topic happens to lose mesh subscriptions.
    pub fn cancel_peer(&mut self, authenticated_remote_peer_id: &str) -> usize {
        let before = self.pending.len();
        self.pending
            .retain(|_, request| request.target_peer_id != authenticated_remote_peer_id);
        before - self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::super::direct_getblock_v1::DIRECT_GETBLOCK_PROTOCOL_V1;
    use super::*;

    fn request(index: usize, peer: &str) -> DirectGetBlockRequestV1 {
        DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: "pulsedag-testnet-v3".into(),
            request_id: format!("getblock:peerA:{}:42:{index}", "a".repeat(64)),
            requesting_peer_id: "peerA".into(),
            requested_peer_id: peer.into(),
            block_hash: "a".repeat(64),
        }
    }

    fn response(request: &DirectGetBlockRequestV1) -> DirectGetBlockResponseV1 {
        DirectGetBlockResponseV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: request.chain_id.clone(),
            request_id: request.request_id.clone(),
            responding_peer_id: request.requested_peer_id.clone(),
            block_hash: request.block_hash.clone(),
            block: None,
        }
    }

    fn book() -> DirectGetBlockPendingBookV1 {
        DirectGetBlockPendingBookV1::new("pulsedag-testnet-v3", "peerA").unwrap()
    }

    #[test]
    fn authenticated_correlation_duplicate_and_unsolicited_rejected() {
        let now = Instant::now();
        let mut book = book();
        let req = request(1, "peerB");
        book.begin(req.clone(), "peerB", now).unwrap();
        assert_eq!(
            book.begin(req.clone(), "peerB", now),
            Err(DirectGetBlockPendingError::Duplicate)
        );
        let mut wrong = response(&req);
        wrong.block_hash = "b".repeat(64);
        assert!(book.complete(wrong, "peerB", now).is_err());
        assert_eq!(book.len(), 1, "forged response must not consume a slot");
        assert!(book.complete(response(&req), "peerEvil", now).is_err());
        assert_eq!(book.len(), 1);
        let reply = book.complete(response(&req), "peerB", now).unwrap();
        assert!(reply.block.is_none());
        assert!(book.is_empty());
        assert!(matches!(
            book.complete(response(&req), "peerB", now),
            Err(DirectGetBlockPendingError::Unsolicited)
        ));
    }

    #[test]
    fn chain_peer_and_global_per_peer_limits_fail_closed() {
        let now = Instant::now();
        let mut book = book();
        let mut wrong_chain = request(0, "peerB");
        wrong_chain.chain_id = "pulsedag-mainnet-v3".into();
        assert!(book.begin(wrong_chain, "peerB", now).is_err());
        assert!(book.begin(request(0, "peerB"), "peerEvil", now).is_err());
        assert!(book.is_empty());
        for i in 1..=MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER {
            book.begin(request(i, "peerB"), "peerB", now).unwrap();
        }
        assert_eq!(
            book.begin(request(5, "peerB"), "peerB", now),
            Err(DirectGetBlockPendingError::LimitExceeded)
        );
        for peer_idx in 0..7 {
            let name = format!("peer{peer_idx}");
            for i in 0..MAX_DIRECT_GETBLOCK_INFLIGHT_PER_PEER {
                book.begin(request(100 + peer_idx * 10 + i, &name), &name, now)
                    .unwrap();
            }
        }
        assert_eq!(book.len(), MAX_DIRECT_GETBLOCK_INFLIGHT_GLOBAL);
        assert_eq!(
            book.begin(request(1000, "peerX"), "peerX", now),
            Err(DirectGetBlockPendingError::LimitExceeded)
        );
    }

    #[test]
    fn old_request_id_cannot_be_reused_after_completion_expiry_or_disconnect() {
        let now = Instant::now();
        let mut book = book();
        let first = request(1, "peerB");
        book.begin(first.clone(), "peerB", now).unwrap();
        book.complete(response(&first), "peerB", now).unwrap();
        assert!(matches!(
            book.begin(first.clone(), "peerB", now),
            Err(DirectGetBlockPendingError::NonMonotonicRequestId)
        ));
        let next = request(2, "peerB");
        book.begin(next.clone(), "peerB", now).unwrap();
        book.cancel_peer("peerB");
        assert!(matches!(
            book.begin(next, "peerB", now),
            Err(DirectGetBlockPendingError::NonMonotonicRequestId)
        ));
        let expiring = request(3, "peerB");
        book.begin(expiring.clone(), "peerB", now).unwrap();
        book.expire(now + DIRECT_GETBLOCK_REQUEST_TTL);
        assert!(matches!(
            book.begin(expiring, "peerB", now + DIRECT_GETBLOCK_REQUEST_TTL),
            Err(DirectGetBlockPendingError::NonMonotonicRequestId)
        ));
        let mut leading_zero = request(4, "peerB");
        leading_zero.request_id = leading_zero.request_id.replace(":4", ":04");
        assert!(matches!(
            book.begin(leading_zero, "peerB", now),
            Err(DirectGetBlockPendingError::NonMonotonicRequestId)
        ));
        let fresh = request(4, "peerB");
        book.begin(fresh.clone(), "peerB", now).unwrap();
        let stale_response = response(&first);
        assert!(matches!(
            book.complete(stale_response, "peerB", now),
            Err(DirectGetBlockPendingError::Unsolicited)
        ));
        assert_eq!(book.len(), 1);
        book.complete(response(&fresh), "peerB", now).unwrap();
    }

    #[test]
    fn unauthenticated_expired_response_does_not_remove_another_peer_slot() {
        let now = Instant::now();
        let mut book = book();
        let req = request(1, "peerB");
        book.begin(req.clone(), "peerB", now).unwrap();
        let deadline = now + DIRECT_GETBLOCK_REQUEST_TTL;
        assert!(matches!(
            book.complete(response(&req), "peerEvil", deadline),
            Err(DirectGetBlockPendingError::Wire(_))
        ));
        assert_eq!(book.len(), 1);
        let mut wrong_hash = response(&req);
        wrong_hash.block_hash = "b".repeat(64);
        assert!(matches!(
            book.complete(wrong_hash, "peerB", deadline),
            Err(DirectGetBlockPendingError::Wire(_))
        ));
        assert_eq!(book.len(), 1);
        assert!(matches!(
            book.complete(response(&req), "peerB", deadline),
            Err(DirectGetBlockPendingError::Expired)
        ));
        assert!(book.is_empty());
    }

    #[test]
    fn expiry_and_disconnect_clear_only_authorized_peer_slots() {
        let now = Instant::now();
        let mut book = book();
        let a = request(1, "peerB");
        let b = request(2, "peerC");
        book.begin(a.clone(), "peerB", now).unwrap();
        book.begin(b.clone(), "peerC", now).unwrap();
        assert_eq!(book.cancel_peer("peerEvil"), 0);
        assert_eq!(book.cancel_peer("peerB"), 1);
        assert_eq!(book.len(), 1);
        let expired_at = now + DIRECT_GETBLOCK_REQUEST_TTL;
        assert!(matches!(
            book.complete(response(&b), "peerC", expired_at),
            Err(DirectGetBlockPendingError::Expired)
        ));
        assert!(book.is_empty());
        book.begin(a, "peerB", expired_at).unwrap();
        assert_eq!(book.expire(expired_at + DIRECT_GETBLOCK_REQUEST_TTL), 1);
        assert!(book.is_empty());
    }
}
