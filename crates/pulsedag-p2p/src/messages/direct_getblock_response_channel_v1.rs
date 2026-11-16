//! One-shot, authenticated binding of locally minted inbound tokens to
//! their corresponding libp2p response channels. Dormant: the real Swarm
//! does not yet advertise direct GetBlock or route ResponseChannel events.
//!
//! The generic channel carrier is intentionally move-only. The alias below
//! uses libp2p's concrete ResponseChannel<Vec<u8>>; tests use opaque local
//! handles without constructing a network or accepting unverified blocks.

use std::collections::hash_map::{Entry, HashMap};
use std::time::Instant;

use libp2p::request_response;
use libp2p::PeerId;
use pulsedag_core::state::ChainState;

use super::direct_getblock_accepted_snapshot_v1::DirectGetBlockAcceptedSnapshotV1;
use super::direct_getblock_inbound_v1::{
    DirectGetBlockInboundErrorV1, DirectGetBlockInboundTicketV1, DirectGetBlockInboundV1,
};
use super::direct_getblock_pending_v1::DIRECT_GETBLOCK_REQUEST_TTL;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectGetBlockChannelErrorV1 {
    Inbound(DirectGetBlockInboundErrorV1),
    DuplicateLocalToken,
    DeadlineOverflow,
    UnknownLocalToken,
    WrongAuthenticatedPeer,
    SendFailed,
}

#[derive(Debug)]
struct LocalResponseChannel<C> {
    authenticated_remote: PeerId,
    deadline: Instant,
    channel: C,
}

/// One manager must be retained for the entire authenticated libp2p runtime.
/// The actual channel is consumed at most once and is never chosen by a
/// peer-controlled JSON request identifier.
#[derive(Debug)]
pub struct DirectGetBlockResponseChannelsV1<C> {
    inbound: DirectGetBlockInboundV1,
    channels: HashMap<u64, LocalResponseChannel<C>>,
}

/// Instantiate with the concrete libp2p response-channel type at a future
/// Noise Swarm inbound event. The channel is NEVER serialized into JSON.
pub type DirectGetBlockLibp2pResponseChannelsV1 =
    DirectGetBlockResponseChannelsV1<request_response::ResponseChannel<Vec<u8>>>;

impl<C> DirectGetBlockResponseChannelsV1<C> {
    pub fn new(
        chain_id: &str,
        authenticated_local: PeerId,
    ) -> Result<Self, DirectGetBlockChannelErrorV1> {
        Ok(Self {
            inbound: DirectGetBlockInboundV1::new(chain_id, authenticated_local)
                .map_err(DirectGetBlockChannelErrorV1::Inbound)?,
            channels: HashMap::new(),
        })
    }

    /// Accept a real Noise peer and its move-only libp2p ResponseChannel.
    /// A failed authentication/quota/duplicate check drops the unsent channel;
    /// no channel is registered until its request envelope is accepted.
    pub fn accept(
        &mut self,
        raw: &[u8],
        authenticated_remote: &PeerId,
        response_channel: C,
        now: Instant,
    ) -> Result<DirectGetBlockInboundTicketV1, DirectGetBlockChannelErrorV1> {
        self.expire(now);
        let deadline = now
            .checked_add(DIRECT_GETBLOCK_REQUEST_TTL)
            .ok_or(DirectGetBlockChannelErrorV1::DeadlineOverflow)?;
        let ticket = self
            .inbound
            .accept(raw, authenticated_remote, now)
            .map_err(DirectGetBlockChannelErrorV1::Inbound)?;
        let local_token = ticket.local_token;
        let channel = LocalResponseChannel {
            authenticated_remote: *authenticated_remote,
            deadline,
            channel: response_channel,
        };
        match self.channels.entry(local_token) {
            Entry::Vacant(slot) => {
                slot.insert(channel);
            }
            Entry::Occupied(_) => {
                // A u64 collision is impossible before lifetime token
                // exhaustion, but must never overwrite an older channel.
                return Err(DirectGetBlockChannelErrorV1::DuplicateLocalToken);
            }
        }
        Ok(ticket)
    }

    /// Encode exclusively from an accepted, same-chain ChainState, THEN move
    /// out the exact response channel and attempt a one-shot send.
    ///
    /// The closure can later be:
    /// |channel, bytes| behaviour.send_response(channel, bytes).is_ok()
    ///
    /// On wrong peer or chain domain, preserve both the original ticket and
    /// its channel for a valid retry. On an actual send error, never replay
    /// the closed or already-consumed response channel.
    pub fn respond_from_accepted_dag<F>(
        &mut self,
        local_token: u64,
        authenticated_remote: &PeerId,
        accepted_chain: &ChainState,
        now: Instant,
        send: F,
    ) -> Result<(), DirectGetBlockChannelErrorV1>
    where
        F: FnOnce(C, Vec<u8>) -> bool,
    {
        self.expire(now);
        let selected = self
            .channels
            .get(&local_token)
            .ok_or(DirectGetBlockChannelErrorV1::UnknownLocalToken)?;
        if &selected.authenticated_remote != authenticated_remote {
            return Err(DirectGetBlockChannelErrorV1::WrongAuthenticatedPeer);
        }

        let wire = match self.inbound.reply_from_accepted_dag(
            local_token,
            authenticated_remote,
            accepted_chain,
            now,
        ) {
            Ok(wire) => wire,
            Err(error) => {
                if !matches!(
                    error,
                    DirectGetBlockInboundErrorV1::WrongAuthenticatedPeer
                        | DirectGetBlockInboundErrorV1::ChainDomainMismatch
                ) {
                    // An expired/invalid/oversize ticket must not retain its
                    // channel until the next periodic maintenance tick.
                    self.channels.remove(&local_token);
                }
                return Err(DirectGetBlockChannelErrorV1::Inbound(error));
            }
        };
        let response = self
            .channels
            .remove(&local_token)
            .ok_or(DirectGetBlockChannelErrorV1::UnknownLocalToken)?;
        if !send(response.channel, wire) {
            return Err(DirectGetBlockChannelErrorV1::SendFailed);
        }
        Ok(())
    }

    /// Send a daemon-created compact accepted-DAG snapshot over the unique
    /// one-shot channel minted by the actual authenticated Noise Swarm.
    /// A wrong chain/hash/peer preserves the channel for the correct reply.
    /// The API is intentionally dormant until the daemon response bridge is
    /// wired and production ingress is separately authorized.
    pub fn respond_from_accepted_snapshot<F>(
        &mut self,
        local_token: u64,
        authenticated_remote: &PeerId,
        snapshot: &DirectGetBlockAcceptedSnapshotV1,
        now: Instant,
        send: F,
    ) -> Result<(), DirectGetBlockChannelErrorV1>
    where
        F: FnOnce(C, Vec<u8>) -> bool,
    {
        self.expire(now);
        let selected = self
            .channels
            .get(&local_token)
            .ok_or(DirectGetBlockChannelErrorV1::UnknownLocalToken)?;
        if &selected.authenticated_remote != authenticated_remote {
            return Err(DirectGetBlockChannelErrorV1::WrongAuthenticatedPeer);
        }

        let wire = match self.inbound.reply_from_accepted_snapshot(
            local_token,
            authenticated_remote,
            snapshot,
            now,
        ) {
            Ok(wire) => wire,
            Err(error) => {
                if !matches!(
                    error,
                    DirectGetBlockInboundErrorV1::WrongAuthenticatedPeer
                        | DirectGetBlockInboundErrorV1::ChainDomainMismatch
                        | DirectGetBlockInboundErrorV1::SnapshotHashMismatch
                ) {
                    self.channels.remove(&local_token);
                }
                return Err(DirectGetBlockChannelErrorV1::Inbound(error));
            }
        };
        let response = self
            .channels
            .remove(&local_token)
            .ok_or(DirectGetBlockChannelErrorV1::UnknownLocalToken)?;
        if !send(response.channel, wire) {
            return Err(DirectGetBlockChannelErrorV1::SendFailed);
        }
        Ok(())
    }

    /// Invoke only after the final authenticated Swarm connection to a peer
    /// closes. This drops its owned response channels without touching others.
    pub fn peer_disconnected(&mut self, authenticated_remote: &PeerId) -> usize {
        self.inbound.cancel_peer(authenticated_remote);
        let before = self.channels.len();
        self.channels
            .retain(|_, slot| &slot.authenticated_remote != authenticated_remote);
        before - self.channels.len()
    }

    /// Run on the timer AND before every inbound admission and response.
    /// The accepted ticket book and the concrete response-channel map expire
    /// together, even when a periodic timer is delayed.
    pub fn expire(&mut self, now: Instant) -> usize {
        self.inbound.expire(now);
        let before = self.channels.len();
        self.channels.retain(|_, slot| now < slot.deadline);
        before - self.channels.len()
    }

    pub fn len(&self) -> usize {
        self.channels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.channels.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::identity;
    use pulsedag_core::genesis::init_chain_state;
    use std::time::Duration;

    use super::super::direct_getblock_v1::{
        decode_direct_getblock_request, decode_direct_getblock_response,
        encode_direct_getblock_request, DirectGetBlockRequestV1, DIRECT_GETBLOCK_PROTOCOL_V1,
    };

    fn peer() -> PeerId {
        PeerId::from(identity::Keypair::generate_ed25519().public())
    }

    fn request(local: &PeerId, remote: &PeerId, hash: &str, seq: u64) -> Vec<u8> {
        let request = DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: "response-channel-test".to_owned(),
            request_id: format!("getblock:{remote}:{hash}:{seq}"),
            requesting_peer_id: remote.to_string(),
            requested_peer_id: local.to_string(),
            block_hash: hash.to_owned(),
        };
        encode_direct_getblock_request(
            &request,
            "response-channel-test",
            &remote.to_string(),
            &local.to_string(),
        )
        .unwrap()
    }

    #[test]
    fn actual_accepted_genesis_responds_on_exact_one_shot_local_channel() {
        let local = peer();
        let remote = peer();
        let chain = init_chain_state("response-channel-test".into());
        let raw = request(&local, &remote, &chain.dag.genesis_hash, 1);
        let now = Instant::now();
        let mut book =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        let ticket = book.accept(&raw, &remote, 991_u64, now).unwrap();
        let expected = decode_direct_getblock_request(
            &raw,
            "response-channel-test",
            &remote.to_string(),
            &local.to_string(),
        )
        .unwrap();
        let mut sent = 0;
        book.respond_from_accepted_dag(
            ticket.local_token,
            &remote,
            &chain,
            now,
            |channel, bytes| {
                assert_eq!(channel, 991);
                let response =
                    decode_direct_getblock_response(&bytes, &expected, &local.to_string()).unwrap();
                assert_eq!(
                    response.block.as_ref().map(|block| block.hash.as_str()),
                    Some(chain.dag.genesis_hash.as_str())
                );
                sent += 1;
                true
            },
        )
        .unwrap();
        assert_eq!(sent, 1);
        assert!(book.is_empty());
        assert!(matches!(
            book.respond_from_accepted_dag(
                ticket.local_token,
                &remote,
                &chain,
                now,
                |_channel, _bytes| true
            ),
            Err(DirectGetBlockChannelErrorV1::UnknownLocalToken)
        ));
    }

    #[test]
    fn forged_peer_and_foreign_chain_cannot_consume_response_channel() {
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let chain = init_chain_state("response-channel-test".into());
        let foreign = init_chain_state("other-chain".into());
        let now = Instant::now();
        let raw = request(&local, &remote, &chain.dag.genesis_hash, 1);
        let mut book =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        let ticket = book.accept(&raw, &remote, 7_u64, now).unwrap();
        assert!(matches!(
            book.respond_from_accepted_dag(
                ticket.local_token,
                &attacker,
                &chain,
                now,
                |_channel, _bytes| true
            ),
            Err(DirectGetBlockChannelErrorV1::WrongAuthenticatedPeer)
        ));
        assert!(matches!(
            book.respond_from_accepted_dag(
                ticket.local_token,
                &remote,
                &foreign,
                now,
                |_channel, _bytes| true
            ),
            Err(DirectGetBlockChannelErrorV1::Inbound(
                DirectGetBlockInboundErrorV1::ChainDomainMismatch
            ))
        ));
        assert_eq!(book.len(), 1);
        let mut target = None;
        book.respond_from_accepted_dag(
            ticket.local_token,
            &remote,
            &chain,
            now,
            |channel, _bytes| {
                target = Some(channel);
                true
            },
        )
        .unwrap();
        assert_eq!(target, Some(7));
        assert!(book.is_empty());
    }

    #[test]
    fn disconnected_peer_and_expired_channels_do_not_affect_others() {
        let local = peer();
        let remote = peer();
        let other = peer();
        let now = Instant::now();
        let hash = "b".repeat(64);
        let mut book =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        for i in 0..4_u64 {
            book.accept(&request(&local, &remote, &hash, i), &remote, i, now)
                .unwrap();
        }
        assert!(matches!(
            book.accept(&request(&local, &remote, &hash, 5), &remote, 5, now),
            Err(DirectGetBlockChannelErrorV1::Inbound(
                DirectGetBlockInboundErrorV1::PeerLimit
            ))
        ));
        let other_ticket = book
            .accept(&request(&local, &other, &hash, 6), &other, 6, now)
            .unwrap();
        assert_eq!(book.peer_disconnected(&remote), 4);
        assert_eq!(book.len(), 1);
        assert_eq!(book.expire(now + Duration::from_secs(8)), 1);
        assert!(book.is_empty());
        let chain = init_chain_state("response-channel-test".into());
        assert!(matches!(
            book.respond_from_accepted_dag(
                other_ticket.local_token,
                &other,
                &chain,
                now + Duration::from_secs(8),
                |_channel, _bytes| true,
            ),
            Err(DirectGetBlockChannelErrorV1::UnknownLocalToken)
        ));
    }

    #[test]
    fn send_failure_is_one_shot_never_reuses_closed_channel() {
        let local = peer();
        let remote = peer();
        let chain = init_chain_state("response-channel-test".into());
        let now = Instant::now();
        let mut book =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        let raw = request(&local, &remote, &chain.dag.genesis_hash, 1);
        let ticket = book.accept(&raw, &remote, 23_u64, now).unwrap();
        assert!(matches!(
            book.respond_from_accepted_dag(
                ticket.local_token,
                &remote,
                &chain,
                now,
                |channel, _bytes| {
                    assert_eq!(channel, 23);
                    false
                }
            ),
            Err(DirectGetBlockChannelErrorV1::SendFailed)
        ));
        assert!(book.is_empty());
    }

    #[test]
    fn compact_accepted_snapshot_matches_exact_peer_hash_and_one_shot_channel() {
        let local = peer();
        let remote = peer();
        let attacker = peer();
        let chain = init_chain_state("response-channel-test".into());
        let genesis = chain.dag.genesis_hash.clone();
        let accepted = DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(
            &chain,
            "response-channel-test",
            &genesis,
        )
        .unwrap();
        let missing = DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(
            &chain,
            "response-channel-test",
            &"a".repeat(64),
        )
        .unwrap();
        let now = Instant::now();
        let raw = request(&local, &remote, &genesis, 100);
        let expected = decode_direct_getblock_request(
            &raw,
            "response-channel-test",
            &remote.to_string(),
            &local.to_string(),
        )
        .unwrap();
        let mut channels =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        let ticket = channels.accept(&raw, &remote, 100_u64, now).unwrap();
        assert!(matches!(
            channels.respond_from_accepted_snapshot(
                ticket.local_token,
                &attacker,
                &accepted,
                now,
                |_, _| true,
            ),
            Err(DirectGetBlockChannelErrorV1::WrongAuthenticatedPeer)
        ));
        assert!(matches!(
            channels.respond_from_accepted_snapshot(
                ticket.local_token,
                &remote,
                &missing,
                now,
                |_, _| true,
            ),
            Err(DirectGetBlockChannelErrorV1::Inbound(
                DirectGetBlockInboundErrorV1::SnapshotHashMismatch
            ))
        ));
        assert_eq!(channels.len(), 1);
        let mut delivered = 0_usize;
        channels
            .respond_from_accepted_snapshot(
                ticket.local_token,
                &remote,
                &accepted,
                now,
                |channel, wire| {
                    assert_eq!(channel, 100);
                    let response =
                        decode_direct_getblock_response(&wire, &expected, &local.to_string())
                            .unwrap();
                    assert_eq!(
                        response.block.as_ref().map(|block| block.hash.as_str()),
                        Some(genesis.as_str())
                    );
                    delivered += 1;
                    true
                },
            )
            .unwrap();
        assert_eq!(delivered, 1);
        assert!(channels.is_empty());
        assert!(matches!(
            channels.respond_from_accepted_snapshot(
                ticket.local_token,
                &remote,
                &accepted,
                now,
                |_, _| true,
            ),
            Err(DirectGetBlockChannelErrorV1::UnknownLocalToken)
        ));
    }

    #[test]
    fn foreign_snapshot_rejected_and_accepted_missing_is_authenticated() {
        let local = peer();
        let remote = peer();
        let chain = init_chain_state("response-channel-test".into());
        let foreign = init_chain_state("foreign-testnet".into());
        let hash = "b".repeat(64);
        let approved_missing = DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(
            &chain,
            "response-channel-test",
            &hash,
        )
        .unwrap();
        let foreign_snapshot = DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(
            &foreign,
            "foreign-testnet",
            &hash,
        )
        .unwrap();
        let now = Instant::now();
        let raw = request(&local, &remote, &hash, 101);
        let expected = decode_direct_getblock_request(
            &raw,
            "response-channel-test",
            &remote.to_string(),
            &local.to_string(),
        )
        .unwrap();
        let mut channels =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        let ticket = channels.accept(&raw, &remote, 101_u64, now).unwrap();
        assert!(matches!(
            channels.respond_from_accepted_snapshot(
                ticket.local_token,
                &remote,
                &foreign_snapshot,
                now,
                |_, _| true,
            ),
            Err(DirectGetBlockChannelErrorV1::Inbound(
                DirectGetBlockInboundErrorV1::ChainDomainMismatch
            ))
        ));
        assert_eq!(channels.len(), 1);
        channels
            .respond_from_accepted_snapshot(
                ticket.local_token,
                &remote,
                &approved_missing,
                now,
                |channel, wire| {
                    assert_eq!(channel, 101);
                    let response =
                        decode_direct_getblock_response(&wire, &expected, &local.to_string())
                            .unwrap();
                    assert!(response.block.is_none());
                    true
                },
            )
            .unwrap();
        assert!(channels.is_empty());
    }

    #[test]
    fn accepted_snapshot_is_sendable_without_cloning_full_chainstate() {
        fn assert_send<T: Send>() {}
        assert_send::<DirectGetBlockAcceptedSnapshotV1>();
    }

    #[test]
    fn repeated_timeout_without_timer_never_grows_channel_map() {
        let local = peer();
        let remote = peer();
        let hash = "a".repeat(64);
        let now = Instant::now();
        let mut book =
            DirectGetBlockResponseChannelsV1::new("response-channel-test", local).unwrap();
        for round in 0..8_u64 {
            let instant = now + Duration::from_secs(round * 8);
            for i in 0..4_u64 {
                let raw = request(&local, &remote, &hash, round * 4 + i);
                book.accept(&raw, &remote, round * 4 + i, instant).unwrap();
            }
            assert_eq!(book.len(), 4);
        }
        assert_eq!(book.expire(now + Duration::from_secs(64)), 4);
        assert!(book.is_empty());
    }
}
