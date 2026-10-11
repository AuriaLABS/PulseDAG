use crate::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    errors::PulseError,
    genesis::init_chain_state,
    header_v2::compute_block_hash_v2,
    header_v3::BlockHeaderV3,
    monetary_v3::GENESIS_ISSUANCE_ATOMS,
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    protocol::{BLOCK_HEADER_VERSION_V2, BLOCK_HEADER_VERSION_V3},
    state::{ConsensusMode, Mempool, SelectedParentPolicy, UtxoState},
    types::{compute_merkle_root, Block, BlockHeader},
    ChainState,
};

/// Deterministic zero-allocation v3 genesis constructor.
///
/// The exact timestamp is a launch-freeze input and must be supplied explicitly;
/// runtime wall-clock time is never consulted. The current consensus header
/// encoding remains v2, so this constructor does not invent a new header version.
pub fn genesis_block_v3(chain_id: &str, frozen_timestamp: u64) -> Result<Block, PulseError> {
    if chain_id.is_empty() {
        return Err(PulseError::ChainIdMismatch);
    }
    if frozen_timestamp == 0 {
        return Err(PulseError::InvalidBlock(
            "v3 genesis timestamp must be explicitly frozen and non-zero".into(),
        ));
    }
    debug_assert_eq!(GENESIS_ISSUANCE_ATOMS, 0);

    let transactions = Vec::new();
    let utxo = UtxoState::default();
    let state_root = utxo.compute_state_root()?;
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: vec![],
            timestamp: frozen_timestamp,
            difficulty: crate::retarget::CONSENSUS_POW_LIMIT_BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root,
            blue_score: 0,
            height: 0,
        },
        transactions,
    };
    block.hash = compute_block_hash_v2(&block.header, chain_id)?;
    Ok(block)
}

/// Assemble the **dormant** native-v3 zero-allocation genesis envelope.
///
/// Unlike `genesis_block_v3`, whose currently retained monetary state uses
/// the seconds-based v2 `BlockHeader`, this is a separate versioned v3 header:
/// the explicitly frozen timestamp is in Unix nanoseconds and is committed by
/// the v3 header/PoW domain. Its state root is the canonical EMPTY UTXO state,
/// never an unsealed placeholder, and genesis is not a mined reward block.
///
/// This function does NOT initialize `ChainState`, freeze an actual mainnet or
/// testnet identity, change protocol activation, persist any bytes or authorize
/// mining, P2P, replay or production startup. Those end-to-end versioned paths
/// do not yet exist and `monetary_v3` must still fail closed before storage.
pub fn dormant_native_genesis_envelope_v3_ns(
    chain_id: &str,
    frozen_timestamp_ns: u64,
) -> Result<BlockEnvelopeV3, PulseError> {
    if GENESIS_ISSUANCE_ATOMS != 0 {
        return Err(PulseError::InvalidBlock(
            "native v3 genesis must never allocate spendable issuance".into(),
        ));
    }
    if frozen_timestamp_ns == 0 {
        return Err(PulseError::InvalidBlock(
            "native v3 genesis requires a frozen nonzero nanosecond timestamp".into(),
        ));
    }
    let transactions = Vec::new();
    let state_root = UtxoState::default().compute_state_root()?;
    let header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![],
        timestamp_ns: frozen_timestamp_ns,
        difficulty: crate::retarget::CONSENSUS_POW_LIMIT_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root,
        blue_score: 0,
        height: 0,
    };
    build_block_envelope_v3(chain_id, header, transactions)
}

/// Initialize a clean v3 candidate state whose genesis creates no spendable UTXO.
///
/// This prepares the monetary/genesis invariant without claiming final network
/// activation. Exact chain ID, timestamp and the external protocol/monetary
/// binding remain #781 freeze inputs.
pub fn init_chain_state_v3(
    chain_id: String,
    frozen_timestamp: u64,
) -> Result<ChainState, PulseError> {
    let genesis = genesis_block_v3(&chain_id, frozen_timestamp)?;
    let utxo = UtxoState::default();
    let state_root = utxo.compute_state_root()?;
    let genesis_hash = genesis.hash.clone();

    let mut state = init_chain_state(chain_id);
    state.dag.blocks.clear();
    state.dag.blocks.insert(genesis_hash.clone(), genesis);
    state.dag.tips.clear();
    state.dag.tips.insert(genesis_hash.clone());
    state.dag.children.clear();
    state.dag.genesis_hash = genesis_hash.clone();
    state.dag.best_height = 0;
    state.dag.consensus_mode = ConsensusMode::Legacy;
    state.dag.selected_parents.clear();
    state
        .dag
        .selected_parents
        .insert(genesis_hash.clone(), None);
    state.dag.selected_chain = vec![genesis_hash.clone()];
    state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
    state.dag.merge_set_k = crate::ghostdag::DEFAULT_MERGE_SET_K;
    state.dag.merge_set_blues.clear();
    state
        .dag
        .merge_set_blues
        .insert(genesis_hash.clone(), Vec::new());
    state.dag.merge_set_reds.clear();
    state
        .dag
        .merge_set_reds
        .insert(genesis_hash.clone(), Vec::new());
    state.dag.blue_work.clear();
    state.dag.blue_work.insert(genesis_hash.clone(), 0);
    state.dag.merge_set_diagnostics.clear();
    state.dag.ordered_dag = vec![genesis_hash.clone()];
    state.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    state.dag.ordered_dag_rebuild_total = 0;
    state.dag.ordered_dag_rebuild_failed_total = 0;
    state.dag.ordered_dag_state_root = Some(state_root);
    state.dag.ordered_dag_tip = Some(genesis_hash);
    state.dag.ordered_dag_conflict_diagnostics.clear();
    state.utxo = utxo;
    state.mempool = Mempool::default();
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FROZEN_TS: u64 = 1_800_000_000;

    #[test]
    fn v3_genesis_has_zero_spendable_allocation() {
        let block = genesis_block_v3("pulsedag-v3-mainnet-candidate", FROZEN_TS).unwrap();
        let state = init_chain_state_v3("pulsedag-v3-mainnet-candidate".into(), FROZEN_TS).unwrap();

        assert!(block.transactions.is_empty());
        assert!(state.utxo.utxos.is_empty());
        assert!(state.utxo.address_index.is_empty());
        assert_eq!(GENESIS_ISSUANCE_ATOMS, 0);
    }

    #[test]
    fn v3_genesis_is_deterministic_and_chain_bound() {
        let a = genesis_block_v3("pulsedag-v3-a", FROZEN_TS).unwrap();
        let a_again = genesis_block_v3("pulsedag-v3-a", FROZEN_TS).unwrap();
        let b = genesis_block_v3("pulsedag-v3-b", FROZEN_TS).unwrap();
        let later = genesis_block_v3("pulsedag-v3-a", FROZEN_TS + 1).unwrap();

        assert_eq!(a.hash, a_again.hash);
        assert_eq!(a.header.merkle_root, a_again.header.merkle_root);
        assert_eq!(a.header.state_root, a_again.header.state_root);
        assert_eq!(a.transactions.len(), a_again.transactions.len());
        assert_ne!(a.hash, b.hash);
        assert_ne!(a.hash, later.hash);
    }

    #[test]
    fn dormant_native_genesis_is_an_unmined_zero_allocation_v3_envelope() {
        use crate::{
            block_envelope_v3::validate_block_envelope_v3,
            header_v3::compute_block_hash_v3,
        };

        let frozen_ns = FROZEN_TS * 1_000_000_000 + 500_000_000;
        let chain_id = "pulsedag-v3-native-candidate";
        let genesis = dormant_native_genesis_envelope_v3_ns(chain_id, frozen_ns).unwrap();
        validate_block_envelope_v3(&genesis, chain_id).unwrap();
        assert!(genesis.transactions.is_empty());
        assert!(genesis.header.parents.is_empty());
        assert_eq!(genesis.header.version, BLOCK_HEADER_VERSION_V3);
        assert_eq!(genesis.header.height, 0);
        assert_eq!(genesis.header.blue_score, 0);
        assert_eq!(genesis.header.nonce, 0);
        assert_eq!(genesis.header.timestamp_ns, frozen_ns);
        assert_eq!(
            genesis.header.difficulty,
            crate::retarget::CONSENSUS_POW_LIMIT_BITS
        );
        assert_eq!(genesis.header.merkle_root, compute_merkle_root(&[]));
        assert_eq!(
            genesis.header.state_root,
            UtxoState::default().compute_state_root().unwrap()
        );
        assert_eq!(
            genesis.hash,
            compute_block_hash_v3(&genesis.header, chain_id).unwrap()
        );
        assert_eq!(GENESIS_ISSUANCE_ATOMS, 0);
    }

    #[test]
    fn native_genesis_freeze_is_chain_and_nanosecond_bound_not_v2_seconds() {
        use crate::block_envelope_v3::validate_block_envelope_v3;

        let first_ns = FROZEN_TS * 1_000_000_000 + 100_000_000;
        let second_ns = FROZEN_TS * 1_000_000_000 + 600_000_000;
        let first = dormant_native_genesis_envelope_v3_ns("candidate-a", first_ns).unwrap();
        let again = dormant_native_genesis_envelope_v3_ns("candidate-a", first_ns).unwrap();
        let later = dormant_native_genesis_envelope_v3_ns("candidate-a", second_ns).unwrap();
        let other_network =
            dormant_native_genesis_envelope_v3_ns("candidate-b", first_ns).unwrap();
        let old_v2 = genesis_block_v3("candidate-a", FROZEN_TS).unwrap();

        assert_eq!(first.hash, again.hash);
        assert_eq!(first.header, again.header);
        assert_eq!(first.header.timestamp_ns / 1_000_000_000, later.header.timestamp_ns / 1_000_000_000);
        assert_ne!(first.hash, later.hash);
        assert_ne!(first.hash, other_network.hash);
        assert_ne!(first.hash, old_v2.hash);
        assert!(validate_block_envelope_v3(&first, "candidate-b").is_err());
        assert!(dormant_native_genesis_envelope_v3_ns("", first_ns).is_err());
        assert!(dormant_native_genesis_envelope_v3_ns("candidate-a", 0).is_err());
    }

    #[test]
    fn v3_genesis_refuses_unfrozen_inputs() {
        assert!(genesis_block_v3("", FROZEN_TS).is_err());
        assert!(genesis_block_v3("pulsedag-v3", 0).is_err());
    }
}
