//! DORMANT read-only native-v3 accepted GetBlock authority.
//! No production accepted native GD-08/UTXO store exists yet. In particular,
//! legacy ChainState and retained v2 Blocks MUST NEVER be consulted here.
//! The independent read interface must eventually be implemented by a single
//! atomically committed native-v3 monetary database. Test stores are not proof.

use pulsedag_core::{
    block_envelope_v3::BlockEnvelopeV3, genesis_v3::dormant_native_genesis_envelope_v3_ns,
};
use pulsedag_p2p::messages::{
    direct_getblock_v1::{
        validate_direct_getblock_request, DirectGetBlockRequestV1, DirectGetBlockWireError,
    },
    native_v3_getblock_candidate_v1::{
        encode_native_v3_getblock_candidate_response, NativeV3GetBlockCandidateResponse,
        NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
    },
};
use pulsedag_storage::{NativeV3AcceptedRocksDbReadOnlyV1, NativeV3EnvelopeRestoreRecordV1};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NativeV3AcceptedReadErrorV1 {
    InvalidGenesis,
    InvalidDomain,
    InvalidHash,
    MissingMembership,
    InvalidAcceptedRecord,
    WrongStateRoot,
    InvalidOrderCommit,
    TornGeneration,
    Storage(String),
    Wire(DirectGetBlockWireError),
}

#[derive(Debug, Clone)]
pub(super) struct NativeV3CommittedEnvelopeRowV1 {
    // This frame proves only block hash/PoW/body format, NOT GD-08 authority.
    pub frame: NativeV3EnvelopeRestoreRecordV1,
    pub chain_id: String,
    pub genesis_hash: String,
    pub committed_generation: u64,
    pub committed_state_root: String,
    pub committed_gd08_digest: String,
}

/// A separate, independently committed GD-08/monetary consensus journal.
/// Neither the P2P frame nor its accepted-row JSON is allowed to supply this
/// evidence. Generation, root and digest are from the *same* durable commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeV3ConsensusCommitmentV1 {
    pub accepted_generation: u64,
    pub monetary_state_root: String,
    pub gd08_order_digest: String,
}

/// A future atomic consensus store must independently implement these calls.
/// The accepted namespace, NOT a generic candidate cache, must be consulted.
/// There is intentionally no production implementation or writer in v3.0.
pub(super) trait NativeV3AcceptedReadOnlyV1 {
    fn current_generation(&self) -> Result<u64, String>;
    fn read_native_accepted(
        &self,
        hash: &str,
    ) -> Result<Option<NativeV3CommittedEnvelopeRowV1>, String>;
    fn has_committed_native_membership(&self, hash: &str, generation: u64) -> Result<Option<u64>, String>;
    /// Independent immutable consensus-journal lookup. Implementations MUST
    /// NOT derive this from the candidate envelope, accepted body row or
    /// remote peer's claim. No production implementation exists yet.
    fn independent_consensus_commitment(
        &self,
        hash: &str,
        generation: u64,
    ) -> Result<Option<NativeV3ConsensusCommitmentV1>, String>;
}

/// Read-only disk adapter. This is NOT the GD-08 monetary writer: a separate
/// native accepted journal is opened read-only and every accepted entry MUST
/// already have been atomically sealed by an independently validated future
/// consensus authority. Test fixture rows cannot confer production authority.
impl NativeV3AcceptedReadOnlyV1 for NativeV3AcceptedRocksDbReadOnlyV1 {
    fn current_generation(&self) -> Result<u64, String> {
        NativeV3AcceptedRocksDbReadOnlyV1::current_generation(self)
    }

    fn read_native_accepted(
        &self,
        hash: &str,
    ) -> Result<Option<NativeV3CommittedEnvelopeRowV1>, String> {
        self.read_accepted(hash).map(|row| {
            row.map(|value| NativeV3CommittedEnvelopeRowV1 {
                frame: value.frame,
                chain_id: value.chain_id,
                genesis_hash: value.genesis_hash,
                committed_generation: value.committed_generation,
                committed_state_root: value.committed_state_root,
                committed_gd08_digest: value.committed_gd08_digest,
            })
        })
    }

    fn has_committed_native_membership(&self, hash: &str, generation: u64) -> Result<Option<u64>, String> {
        self.has_accepted_membership(hash, generation)
    }

    fn independent_consensus_commitment(
        &self,
        hash: &str,
        generation: u64,
    ) -> Result<Option<NativeV3ConsensusCommitmentV1>, String> {
        self.independent_journal(hash, generation).map(|value| {
            value.map(|entry| NativeV3ConsensusCommitmentV1 {
                accepted_generation: entry.accepted_generation,
                monetary_state_root: entry.monetary_state_root,
                gd08_order_digest: entry.gd08_order_digest,
            })
        })
    }
}

fn canonical_hex256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn lookup_consistent<R: NativeV3AcceptedReadOnlyV1>(
    reader: &R,
    chain_id: &str,
    genesis_ns: u64,
    genesis_hash: &str,
    requested_hash: &str,
) -> Result<Option<BlockEnvelopeV3>, NativeV3AcceptedReadErrorV1> {
    let before = reader
        .current_generation()
        .map_err(NativeV3AcceptedReadErrorV1::Storage)?;
    let record = reader
        .read_native_accepted(requested_hash)
        .map_err(NativeV3AcceptedReadErrorV1::Storage)?;
    let accepted_membership_generation = reader
        .has_committed_native_membership(requested_hash, before)
        .map_err(NativeV3AcceptedReadErrorV1::Storage)?;
    let independent = reader
        .independent_consensus_commitment(requested_hash, before)
        .map_err(NativeV3AcceptedReadErrorV1::Storage)?;
    let after = reader
        .current_generation()
        .map_err(NativeV3AcceptedReadErrorV1::Storage)?;
    if before != after || before == 0 {
        return Err(NativeV3AcceptedReadErrorV1::TornGeneration);
    }
    let Some(record) = record else {
        if accepted_membership_generation.is_some() || independent.is_some() {
            // A corrupted accepted index/missing body is NOT an authenticated
            // peer-local Missing. Fail rather than hide a broken local store.
            return Err(NativeV3AcceptedReadErrorV1::MissingMembership);
        }
        return Ok(None);
    };
    if accepted_membership_generation.is_none() || independent.is_none() {
        // A valid PoW candidate is NOT automatically an accepted block.
        return Err(NativeV3AcceptedReadErrorV1::MissingMembership);
    }
    let independently_committed = independent.expect("checked Some above");
    let membership_generation = accepted_membership_generation.expect("checked Some above");
    if membership_generation != record.committed_generation
        || independently_committed.accepted_generation != record.committed_generation
        || independently_committed.accepted_generation > before
        || independently_committed.accepted_generation == 0
    {
        return Err(NativeV3AcceptedReadErrorV1::TornGeneration);
    }
    if record.committed_generation == 0 || record.committed_generation > before {
        return Err(NativeV3AcceptedReadErrorV1::TornGeneration);
    }
    if record.chain_id != chain_id || record.genesis_hash != genesis_hash {
        return Err(NativeV3AcceptedReadErrorV1::InvalidDomain);
    }
    if !canonical_hex256(&record.committed_gd08_digest)
        || !canonical_hex256(&independently_committed.gd08_order_digest)
        || record.committed_gd08_digest != independently_committed.gd08_order_digest
    {
        return Err(NativeV3AcceptedReadErrorV1::InvalidOrderCommit);
    }
    record
        .frame
        .verify(chain_id, genesis_ns, genesis_hash)
        .map_err(|_| NativeV3AcceptedReadErrorV1::InvalidAcceptedRecord)?;
    let body = record.frame.envelope();
    if body.hash != requested_hash {
        return Err(NativeV3AcceptedReadErrorV1::InvalidHash);
    }
    if record.committed_state_root != body.header.state_root
        || record.committed_state_root != independently_committed.monetary_state_root
        || !canonical_hex256(&independently_committed.monetary_state_root)
        || !canonical_hex256(&record.committed_state_root)
    {
        return Err(NativeV3AcceptedReadErrorV1::WrongStateRoot);
    }
    Ok(Some(body.clone()))
}

/// Build only a wire response. A real inbound Noise channel MUST be separately
/// authenticated, one-shot and associated with this exact request. This method
/// does not send a packet, approve a block or start the server in production.
/// The future storage implementation MUST establish monetary authority before
/// any accepted row can exist; equality checks alone are insufficient.
pub(super) fn prepare_native_v3_accepted_getblock_wire<R: NativeV3AcceptedReadOnlyV1>(
    reader: &R,
    chain_id: &str,
    genesis_ns: u64,
    trusted_genesis_hash: &str,
    request: &DirectGetBlockRequestV1,
    authenticated_remote_requester: &str,
    authenticated_local_responder: &str,
) -> Result<Vec<u8>, NativeV3AcceptedReadErrorV1> {
    validate_direct_getblock_request(
        request,
        chain_id,
        authenticated_remote_requester,
        authenticated_local_responder,
    )
    .map_err(NativeV3AcceptedReadErrorV1::Wire)?;
    let frozen = dormant_native_genesis_envelope_v3_ns(chain_id, genesis_ns)
        .map_err(|_| NativeV3AcceptedReadErrorV1::InvalidGenesis)?;
    if frozen.hash != trusted_genesis_hash {
        return Err(NativeV3AcceptedReadErrorV1::InvalidGenesis);
    }
    if request.block_hash == trusted_genesis_hash {
        // The genesis has a separate immutable, unmined bootstrap identity.
        return Err(NativeV3AcceptedReadErrorV1::InvalidHash);
    }
    let body = lookup_consistent(
        reader,
        chain_id,
        genesis_ns,
        trusted_genesis_hash,
        &request.block_hash,
    )?;
    let response = NativeV3GetBlockCandidateResponse {
        version: NATIVE_V3_GETBLOCK_CANDIDATE_VERSION,
        chain_id: chain_id.to_string(),
        request_id: request.request_id.clone(),
        responding_peer_id: authenticated_local_responder.to_string(),
        block_hash: request.block_hash.clone(),
        block: body,
    };
    encode_native_v3_getblock_candidate_response(
        &response,
        request,
        chain_id,
        authenticated_remote_requester,
        authenticated_local_responder,
    )
    .map_err(NativeV3AcceptedReadErrorV1::Wire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3, header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3, types::compute_merkle_root,
    };
    use pulsedag_p2p::messages::{
        direct_getblock_v1::DIRECT_GETBLOCK_PROTOCOL_V1,
        native_v3_getblock_candidate_v1::decode_native_v3_getblock_candidate_response,
    };
    use std::cell::Cell;
    use std::collections::{BTreeMap, BTreeSet};

    const CHAIN: &str = "native-v3-accepted-getblock-reader-test-only";
    const NS: u64 = 1_800_000_000_500_000_000;
    const REQUESTER: &str = "requesterPeerAAAA";
    const RESPONDER: &str = "responderPeerBBBB";

    #[derive(Default)]
    struct FakeStore {
        generation: Cell<u64>,
        membership_generation_override: Cell<u64>,
        mutate_on_read: bool,
        accepted: BTreeSet<String>,
        rows: BTreeMap<String, NativeV3CommittedEnvelopeRowV1>,
        independently_committed: BTreeMap<String, NativeV3ConsensusCommitmentV1>,
    }
    impl NativeV3AcceptedReadOnlyV1 for FakeStore {
        fn current_generation(&self) -> Result<u64, String> {
            Ok(self.generation.get())
        }
        fn read_native_accepted(
            &self,
            hash: &str,
        ) -> Result<Option<NativeV3CommittedEnvelopeRowV1>, String> {
            if self.mutate_on_read {
                self.generation.set(self.generation.get() + 1);
            }
            Ok(self.rows.get(hash).cloned())
        }
        fn has_committed_native_membership(&self, hash: &str, _gen: u64) -> Result<Option<u64>, String> {
            let generation = if self.membership_generation_override.get() == 0 {
                self.generation.get()
            } else {
                self.membership_generation_override.get()
            };
            Ok(self.accepted.contains(hash).then_some(generation))
        }
        fn independent_consensus_commitment(
            &self,
            hash: &str,
            _generation: u64,
        ) -> Result<Option<NativeV3ConsensusCommitmentV1>, String> {
            Ok(self.independently_committed.get(hash).cloned())
        }
    }
    fn fixture() -> (String, BlockEnvelopeV3, DirectGetBlockRequestV1, FakeStore) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let txs =
            vec![
                build_reward_claim_transaction_v3("pulse1native-read-only-fixture", 1, CHAIN)
                    .unwrap(),
            ];
        let mut h = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash.clone()],
            timestamp_ns: NS + 1_000_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: "2".repeat(64),
            blue_score: 1,
            height: 1,
        };
        h.nonce = (0..4096)
            .find(|nonce| {
                h.nonce = *nonce;
                evaluate_v3_header_pow(&h, CHAIN).unwrap().pow.accepted
            })
            .unwrap();
        let b = build_block_envelope_v3(CHAIN, h, txs).unwrap();
        let req = DirectGetBlockRequestV1 {
            version: DIRECT_GETBLOCK_PROTOCOL_V1,
            chain_id: CHAIN.to_string(),
            request_id: format!("getblock:read-only:{}:1", b.hash),
            requesting_peer_id: REQUESTER.into(),
            requested_peer_id: RESPONDER.into(),
            block_hash: b.hash.clone(),
        };
        let record = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
            b.clone(),
            CHAIN,
            NS,
            &genesis.hash,
        )
        .unwrap();
        let row = NativeV3CommittedEnvelopeRowV1 {
            frame: record,
            chain_id: CHAIN.to_string(),
            genesis_hash: genesis.hash.clone(),
            committed_generation: 5,
            committed_state_root: b.header.state_root.clone(),
            committed_gd08_digest: "a".repeat(64),
        };
        let mut store = FakeStore::default();
        store.generation.set(5);
        store.independently_committed.insert(
            b.hash.clone(),
            NativeV3ConsensusCommitmentV1 {
                accepted_generation: 5,
                monetary_state_root: b.header.state_root.clone(),
                gd08_order_digest: "a".repeat(64),
            },
        );
        store.rows.insert(b.hash.clone(), row);
        (genesis.hash, b, req, store)
    }
    fn wire(
        store: &FakeStore,
        genesis: &str,
        request: &DirectGetBlockRequestV1,
    ) -> Result<Vec<u8>, NativeV3AcceptedReadErrorV1> {
        prepare_native_v3_accepted_getblock_wire(
            store, CHAIN, NS, genesis, request, REQUESTER, RESPONDER,
        )
    }
    #[test]
    fn staged_pow_valid_candidate_cannot_be_served_without_independent_acceptance() {
        let (genesis, b, req, mut store) = fixture();
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::MissingMembership)
        );
        store.accepted.insert(b.hash.clone());
        let reply = wire(&store, &genesis, &req).unwrap();
        let parsed =
            decode_native_v3_getblock_candidate_response(&reply, &req, CHAIN, REQUESTER, RESPONDER)
                .unwrap();
        assert_eq!(parsed.block.unwrap().hash, b.hash);
    }
    #[test]
    fn true_absence_is_missing_but_torn_accepted_index_is_error() {
        let (genesis, b, req, mut store) = fixture();
        store.rows.clear();
        store.independently_committed.clear();
        let reply = wire(&store, &genesis, &req).unwrap();
        let parsed =
            decode_native_v3_getblock_candidate_response(&reply, &req, CHAIN, REQUESTER, RESPONDER)
                .unwrap();
        assert!(parsed.block.is_none());
        store.accepted.insert(b.hash);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::MissingMembership)
        );
    }
    #[test]
    fn torn_generation_wrong_root_chain_and_noise_binding_fail_closed() {
        let (genesis, b, req, mut store) = fixture();
        store.accepted.insert(b.hash.clone());
        store.mutate_on_read = true;
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::TornGeneration)
        );
        store.mutate_on_read = false;
        store.generation.set(5);
        assert_eq!(
            wire(&store, &"f".repeat(64), &req),
            Err(NativeV3AcceptedReadErrorV1::InvalidGenesis)
        );
        store.rows.get_mut(&b.hash).unwrap().committed_state_root = "f".repeat(64);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::WrongStateRoot)
        );
        store.rows.get_mut(&b.hash).unwrap().committed_state_root = b.header.state_root.clone();
        store.rows.get_mut(&b.hash).unwrap().committed_gd08_digest = "INVALID".into();
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::InvalidOrderCommit)
        );
        assert!(matches!(
            prepare_native_v3_accepted_getblock_wire(
                &store,
                CHAIN,
                NS,
                &genesis,
                &req,
                REQUESTER,
                "forgedOtherPeer",
            ),
            Err(NativeV3AcceptedReadErrorV1::Wire(_))
        ));
    }
    #[test]
    fn well_formed_but_wrong_gd08_digest_cannot_launder_an_accepted_body() {
        let (genesis, b, req, mut store) = fixture();
        store.accepted.insert(b.hash.clone());
        // This is the exact Codex P2: the original implementation accepted
        // arbitrary canonical "a"/"b" hex even if committed GD-08 differs.
        store.rows.get_mut(&b.hash).unwrap().committed_gd08_digest = "b".repeat(64);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::InvalidOrderCommit)
        );
        store.rows.get_mut(&b.hash).unwrap().committed_gd08_digest = "a".repeat(64);
        store
            .independently_committed
            .get_mut(&b.hash)
            .unwrap()
            .gd08_order_digest = "c".repeat(64);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::InvalidOrderCommit)
        );
        store
            .independently_committed
            .get_mut(&b.hash)
            .unwrap()
            .gd08_order_digest = "a".repeat(64);
        store
            .independently_committed
            .get_mut(&b.hash)
            .unwrap()
            .monetary_state_root = "e".repeat(64);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::WrongStateRoot)
        );
        store
            .independently_committed
            .get_mut(&b.hash)
            .unwrap()
            .monetary_state_root = b.header.state_root.clone();
        store
            .independently_committed
            .get_mut(&b.hash)
            .unwrap()
            .accepted_generation = 4;
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::TornGeneration)
        );
        store.independently_committed.remove(&b.hash);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::MissingMembership)
        );
    }

    #[test]
    fn stale_membership_generation_cannot_match_newer_body_and_journal() {
        let (genesis, b, req, mut store) = fixture();
        store.accepted.insert(b.hash.clone());
        store.membership_generation_override.set(3);
        // Body + journal refer to commit generation 5, but accepted index
        // still records generation 3. A boolean membership silently passes.
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::TornGeneration)
        );
        store.membership_generation_override.set(5);
        assert!(wire(&store, &genesis, &req).is_ok());
    }

    #[test]
    fn wrong_accepted_key_and_future_generation_are_rejected() {
        let (genesis, b, mut req, mut store) = fixture();
        let wrong = "b".repeat(64);
        let row = store.rows.remove(&b.hash).unwrap();
        store.rows.insert(wrong.clone(), row);
        let independent = store.independently_committed.remove(&b.hash).unwrap();
        store
            .independently_committed
            .insert(wrong.clone(), independent);
        store.accepted.insert(wrong.clone());
        req.block_hash = wrong.clone();
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::InvalidHash)
        );
        req.block_hash = b.hash.clone();
        let mut row = store.rows.remove(&"b".repeat(64)).unwrap();
        row.committed_generation = 6;
        store.rows.insert(b.hash.clone(), row);
        let independent = store.independently_committed.remove(&wrong).unwrap();
        store
            .independently_committed
            .insert(b.hash.clone(), independent);
        store.accepted.insert(b.hash);
        assert_eq!(
            wire(&store, &genesis, &req),
            Err(NativeV3AcceptedReadErrorV1::TornGeneration)
        );
    }
}
