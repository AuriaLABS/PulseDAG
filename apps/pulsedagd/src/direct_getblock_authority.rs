//! Authority boundary for a future authenticated direct GetBlock daemon service.
//!
//! The current legacy Gossipsub GetBlock responder has different compatibility
//! semantics (including retained blocks). NEVER reuse that responder for the
//! dedicated Noise-authenticated request-response substream.
//!
//! This is an intentionally dormant authority selector until the live Swarm
//! request/response channel is joined to the daemon. No network activation,
//! checkpoint authority, historical-pruned fallback or consensus admission is
//! implied by this module.

use pulsedag_core::state::ChainState;
use pulsedag_core::types::Block;
use pulsedag_p2p::messages::direct_getblock_accepted_snapshot_v1::{
    DirectGetBlockAcceptedSnapshotErrorV1, DirectGetBlockAcceptedSnapshotV1,
};
use pulsedag_p2p::messages::direct_getblock_inbound_v1::DirectGetBlockInboundTicketV1;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DirectGetBlockDaemonSnapshotErrorV1 {
    InvalidLocalTicket,
    RejectChainDomain,
    RejectNoncanonicalHash,
    AuthorityMismatch,
    Snapshot(DirectGetBlockAcceptedSnapshotErrorV1),
}

/// A wrong chain is a rejection, NOT an authenticated "Missing" result.
/// Otherwise a caller could confuse a foreign-chain lookup with an absent hash.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum DirectGetBlockAuthorityDecisionV1<'a> {
    Accepted(&'a Block),
    Missing,
    RejectChainDomain,
    RejectNoncanonicalHash,
}

#[cfg_attr(not(test), allow(dead_code))]
fn canonical_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Only an exact block in the currently accepted, same-chain DAG is eligible.
/// The authenticated request decoder must already have checked the remote
/// Noise PeerId and request envelope before calling this local authority.
///
/// In particular do NOT fetch from staging, a pending queue, orphan storage,
/// an unverified disk record or a foreign-chain snapshot. A pruned historical
/// hash returns Missing until a separate checkpoint-verified storage authority
/// is implemented and reviewed.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn lookup_accepted_direct_getblock_v1<'a>(
    accepted_chain: &'a ChainState,
    configured_chain_id: &str,
    request_chain_id: &str,
    requested_hash: &str,
) -> DirectGetBlockAuthorityDecisionV1<'a> {
    if configured_chain_id.is_empty()
        || configured_chain_id.len() > 96
        || configured_chain_id != request_chain_id
        || accepted_chain.chain_id != configured_chain_id
    {
        return DirectGetBlockAuthorityDecisionV1::RejectChainDomain;
    }
    if !canonical_hash(requested_hash) {
        return DirectGetBlockAuthorityDecisionV1::RejectNoncanonicalHash;
    }
    match accepted_chain.dag.blocks.get(requested_hash) {
        Some(block) if block.hash == requested_hash => {
            DirectGetBlockAuthorityDecisionV1::Accepted(block)
        }
        _ => DirectGetBlockAuthorityDecisionV1::Missing,
    }
}

/// Handoff into the future daemon→Noise-Swarm response channel.
///
/// The ticket MUST originate from `DirectGetBlockLibp2pResponseChannelsV1::accept`
/// on the real authenticated Noise Swarm, not remote JSON or a caller-made ID.
/// Its local token is therefore the only reply-channel selector. The decoder
/// has already verified the requesting peer and same-chain request envelope.
///
/// No staging, pending, orphan, historical unverified storage or arbitrary
/// Block may be passed to this function: the only owned response source is
/// the immutable accepted-DAG snapshot factory.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn prepare_authenticated_direct_getblock_snapshot_v1(
    accepted_chain: &ChainState,
    configured_chain_id: &str,
    request_chain_id: &str,
    authenticated_ticket: &DirectGetBlockInboundTicketV1,
) -> Result<DirectGetBlockAcceptedSnapshotV1, DirectGetBlockDaemonSnapshotErrorV1> {
    if authenticated_ticket.local_token == 0 {
        return Err(DirectGetBlockDaemonSnapshotErrorV1::InvalidLocalTicket);
    }
    let accepted = match lookup_accepted_direct_getblock_v1(
        accepted_chain,
        configured_chain_id,
        request_chain_id,
        &authenticated_ticket.block_hash,
    ) {
        DirectGetBlockAuthorityDecisionV1::Accepted(_) => true,
        DirectGetBlockAuthorityDecisionV1::Missing => false,
        DirectGetBlockAuthorityDecisionV1::RejectChainDomain => {
            return Err(DirectGetBlockDaemonSnapshotErrorV1::RejectChainDomain);
        }
        DirectGetBlockAuthorityDecisionV1::RejectNoncanonicalHash => {
            return Err(DirectGetBlockDaemonSnapshotErrorV1::RejectNoncanonicalHash);
        }
    };
    let snapshot = DirectGetBlockAcceptedSnapshotV1::from_accepted_chain(
        accepted_chain,
        configured_chain_id,
        &authenticated_ticket.block_hash,
    )
    .map_err(DirectGetBlockDaemonSnapshotErrorV1::Snapshot)?;
    // Fail closed if the independent daemon authority and immutable transport
    // factory ever diverge. Never launder an accepted block into a false miss.
    if snapshot.block().is_some() != accepted {
        return Err(DirectGetBlockDaemonSnapshotErrorV1::AuthorityMismatch);
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::genesis::init_chain_state;

    const CHAIN_ID: &str = "direct-getblock-authority-test";

    fn authenticated_ticket(hash: &str) -> DirectGetBlockInboundTicketV1 {
        DirectGetBlockInboundTicketV1 {
            local_token: 1,
            block_hash: hash.to_owned(),
            // Known-valid ephemeral Ed25519 libp2p PeerId; ticket fields in
            // production originate exclusively from Noise-backed accept().
            authenticated_remote:
                "12D3KooWMuqx3a5cGJ9yj1T6svozkH79ekQArKR38D6XXSuy3Rqi"
                    .parse()
                    .unwrap(),
        }
    }

    #[test]
    fn authenticated_ticket_prepares_only_accepted_genesis_snapshot() {
        let chain = init_chain_state(CHAIN_ID.to_string());
        let genesis = chain.dag.genesis_hash.clone();
        let ticket = authenticated_ticket(&genesis);
        let snapshot = prepare_authenticated_direct_getblock_snapshot_v1(
            &chain, CHAIN_ID, CHAIN_ID, &ticket,
        )
        .unwrap();
        assert_eq!(snapshot.chain_id(), CHAIN_ID);
        assert_eq!(snapshot.requested_hash(), genesis);
        assert_eq!(
            snapshot.block().map(|block| block.hash.as_str()),
            Some(genesis.as_str())
        );
    }

    #[test]
    fn authenticated_ticket_missing_orphan_never_reads_pending_storage() {
        let mut chain = init_chain_state(CHAIN_ID.to_string());
        let mut unaccepted = chain
            .dag
            .blocks
            .get(&chain.dag.genesis_hash)
            .unwrap()
            .clone();
        unaccepted.hash = "b".repeat(64);
        chain
            .orphan_blocks
            .insert(unaccepted.hash.clone(), unaccepted.clone());
        let snapshot = prepare_authenticated_direct_getblock_snapshot_v1(
            &chain,
            CHAIN_ID,
            CHAIN_ID,
            &authenticated_ticket(&unaccepted.hash),
        )
        .unwrap();
        assert!(snapshot.block().is_none());
    }

    #[test]
    fn invalid_ticket_cross_chain_and_noncanonical_hash_fail_closed() {
        let chain = init_chain_state(CHAIN_ID.to_string());
        let genesis = chain.dag.genesis_hash.clone();
        let mut ticket = authenticated_ticket(&genesis);
        ticket.local_token = 0;
        assert!(matches!(
            prepare_authenticated_direct_getblock_snapshot_v1(
                &chain, CHAIN_ID, CHAIN_ID, &ticket,
            ),
            Err(DirectGetBlockDaemonSnapshotErrorV1::InvalidLocalTicket)
        ));

        let ticket = authenticated_ticket(&genesis);
        assert!(matches!(
            prepare_authenticated_direct_getblock_snapshot_v1(
                &chain, CHAIN_ID, "foreign", &ticket,
            ),
            Err(DirectGetBlockDaemonSnapshotErrorV1::RejectChainDomain)
        ));

        let ticket = authenticated_ticket(&"A".repeat(64));
        assert!(matches!(
            prepare_authenticated_direct_getblock_snapshot_v1(
                &chain, CHAIN_ID, CHAIN_ID, &ticket,
            ),
            Err(DirectGetBlockDaemonSnapshotErrorV1::RejectNoncanonicalHash)
        ));
    }

    #[test]
    fn forged_accepted_map_key_is_not_snapshot_authority() {
        let mut chain = init_chain_state(CHAIN_ID.to_string());
        let key = "c".repeat(64);
        let mismatching_block = chain
            .dag
            .blocks
            .get(&chain.dag.genesis_hash)
            .unwrap()
            .clone();
        chain.dag.blocks.insert(key.clone(), mismatching_block);
        let snapshot = prepare_authenticated_direct_getblock_snapshot_v1(
            &chain,
            CHAIN_ID,
            CHAIN_ID,
            &authenticated_ticket(&key),
        )
        .unwrap();
        assert!(snapshot.block().is_none());
    }

    #[test]
    fn accepted_genesis_is_available_by_exact_hash() {
        let chain = init_chain_state(CHAIN_ID.to_string());
        let hash = chain.dag.genesis_hash.clone();
        assert!(matches!(
            lookup_accepted_direct_getblock_v1(&chain, CHAIN_ID, CHAIN_ID, &hash),
            DirectGetBlockAuthorityDecisionV1::Accepted(block) if block.hash == hash
        ));
    }

    #[test]
    fn staged_and_orphan_blocks_are_never_authority_for_direct_getblock() {
        let mut chain = init_chain_state(CHAIN_ID.to_string());
        let mut unaccepted = chain
            .dag
            .blocks
            .get(&chain.dag.genesis_hash)
            .unwrap()
            .clone();
        unaccepted.hash = "a".repeat(64);
        assert!(!chain.dag.blocks.contains_key(&unaccepted.hash));
        chain
            .orphan_blocks
            .insert(unaccepted.hash.clone(), unaccepted.clone());
        assert!(matches!(
            lookup_accepted_direct_getblock_v1(&chain, CHAIN_ID, CHAIN_ID, &unaccepted.hash),
            DirectGetBlockAuthorityDecisionV1::Missing
        ));
    }

    #[test]
    fn cross_chain_and_bad_local_snapshot_are_rejected_not_reported_missing() {
        let mut chain = init_chain_state(CHAIN_ID.to_string());
        let hash = chain.dag.genesis_hash.clone();
        assert!(matches!(
            lookup_accepted_direct_getblock_v1(&chain, CHAIN_ID, "foreign-testnet", &hash),
            DirectGetBlockAuthorityDecisionV1::RejectChainDomain
        ));
        chain.chain_id = "foreign-testnet".to_string();
        assert!(matches!(
            lookup_accepted_direct_getblock_v1(&chain, CHAIN_ID, CHAIN_ID, &hash),
            DirectGetBlockAuthorityDecisionV1::RejectChainDomain
        ));
    }

    #[test]
    fn canonical_hash_and_map_key_must_match_accepted_block_identity() {
        let mut chain = init_chain_state(CHAIN_ID.to_string());
        let hash = "b".repeat(64);
        let accepted_genesis = chain
            .dag
            .blocks
            .get(&chain.dag.genesis_hash)
            .unwrap()
            .clone();
        chain.dag.blocks.insert(hash.clone(), accepted_genesis);
        assert!(matches!(
            lookup_accepted_direct_getblock_v1(&chain, CHAIN_ID, CHAIN_ID, &hash),
            DirectGetBlockAuthorityDecisionV1::Missing
        ));
        assert!(matches!(
            lookup_accepted_direct_getblock_v1(&chain, CHAIN_ID, CHAIN_ID, &"B".repeat(64),),
            DirectGetBlockAuthorityDecisionV1::RejectNoncanonicalHash
        ));
    }
}
