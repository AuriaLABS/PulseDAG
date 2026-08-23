use std::collections::HashMap;

use crate::{
    errors::PulseError,
    genesis::{init_chain_state, GENESIS_SUPPLY, GENESIS_TREASURY},
    header_v2::compute_block_hash_v2,
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    protocol::BLOCK_HEADER_VERSION_V2,
    state::{ConsensusMode, Mempool, SelectedParentPolicy, UtxoState},
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    types::{compute_merkle_root, Block, BlockHeader, OutPoint, Transaction, TxOutput, Utxo},
    ChainState,
};

/// Build the chain-bound v2 genesis transaction without changing historical v1 genesis semantics.
pub fn genesis_transaction_v2(chain_id: &str) -> Result<Transaction, PulseError> {
    if chain_id.is_empty() {
        return Err(PulseError::ChainIdMismatch);
    }
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![],
        outputs: vec![TxOutput {
            address: GENESIS_TREASURY.into(),
            amount: GENESIS_SUPPLY,
        }],
        fee: 0,
        nonce: 0,
    };
    tx.txid = compute_txid_v2(&tx, chain_id)?;
    Ok(tx)
}

fn genesis_utxo_state_v2(tx: &Transaction) -> UtxoState {
    let outpoint = OutPoint {
        txid: tx.txid.clone(),
        index: 0,
    };
    let utxo = Utxo {
        outpoint: outpoint.clone(),
        address: GENESIS_TREASURY.into(),
        amount: GENESIS_SUPPLY,
        coinbase: false,
        height: 0,
    };
    let mut utxos = HashMap::new();
    utxos.insert(outpoint.clone(), utxo);
    let mut address_index = HashMap::new();
    address_index.insert(GENESIS_TREASURY.into(), vec![outpoint]);
    UtxoState {
        utxos,
        address_index,
    }
}

/// Build the clean-chain v2 genesis block. Both the transaction id and block hash are chain-bound.
pub fn genesis_block_v2(chain_id: &str) -> Result<Block, PulseError> {
    let tx = genesis_transaction_v2(chain_id)?;
    let txs = vec![tx.clone()];
    let utxo = genesis_utxo_state_v2(&tx);
    let state_root = utxo.compute_state_root()?;
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: vec![],
            timestamp: 0,
            difficulty: crate::retarget::CONSENSUS_POW_LIMIT_BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root,
            blue_score: 0,
            height: 0,
        },
        transactions: txs,
    };
    block.hash = compute_block_hash_v2(&block.header, chain_id)?;
    Ok(block)
}

/// Initialize a clean activated-v2 chain state while keeping the internal historical
/// `ConsensusMode` enum untouched. Release activation identity is carried separately by
/// `ProtocolActivationIdentity::activated_v2` and P2P/storage capability gates.
pub fn init_chain_state_v2(chain_id: String) -> Result<ChainState, PulseError> {
    let genesis = genesis_block_v2(&chain_id)?;
    let tx = genesis.transactions[0].clone();
    let utxo = genesis_utxo_state_v2(&tx);
    let state_root = utxo.compute_state_root()?;
    let genesis_hash = genesis.hash.clone();

    // Start from the legacy constructor only to inherit non-consensus operational defaults.
    // Every consensus-relevant genesis/DAG/UTXO field is replaced below.
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
    use crate::{
        protocol::{ProtocolActivationIdentity, ProtocolConsensusMode},
        TRANSACTION_VERSION_V1,
    };

    #[test]
    fn v2_genesis_is_chain_bound_and_does_not_mutate_v1() {
        let a = genesis_block_v2("pulsedag-private-v2.4.0").unwrap();
        let b = genesis_block_v2("pulsedag-testnet-v2.4.0").unwrap();
        assert_eq!(a.header.version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(a.transactions[0].version, TRANSACTION_VERSION_V2);
        assert_ne!(a.transactions[0].txid, b.transactions[0].txid);
        assert_ne!(a.hash, b.hash);

        let legacy = crate::genesis::genesis_block();
        assert_eq!(legacy.header.version, 1);
        assert_eq!(legacy.transactions[0].version, TRANSACTION_VERSION_V1);
    }

    #[test]
    fn clean_v2_state_matches_activated_protocol_identity() {
        let state = init_chain_state_v2("pulsedag-private-v2.4.0".to_string()).unwrap();
        assert_eq!(state.dag.ordering_version, GHOSTDAG_V1_ORDERING_VERSION);
        assert_eq!(state.dag.selected_parent_policy, SelectedParentPolicy::GhostdagInspired);
        assert_eq!(state.dag.ordered_dag, vec![state.dag.genesis_hash.clone()]);
        assert!(state.dag.ordered_dag_state_root.is_some());
        assert_eq!(state.dag.blocks[&state.dag.genesis_hash].header.version, 2);

        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            state.dag.ordering_version.clone(),
        );
        identity.validate().unwrap();
        assert_eq!(identity.consensus_mode, ProtocolConsensusMode::GhostdagV1);
        assert_eq!(identity.transaction_protocol_version, 2);
        assert_eq!(identity.block_header_protocol_version, 2);
    }

    #[test]
    fn v2_genesis_rejects_empty_chain_identity() {
        assert!(genesis_transaction_v2("").is_err());
        assert!(genesis_block_v2("").is_err());
    }
}
