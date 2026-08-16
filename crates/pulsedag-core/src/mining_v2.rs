use crate::{
    errors::PulseError,
    header_v2::{canonicalize_block_parents_v2, compute_block_hash_v2},
    protocol::BLOCK_HEADER_VERSION_V2,
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    types::{compute_merkle_root, Block, BlockHeader, Transaction, TxOutput},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateBlockV2Spec {
    pub parents: Vec<String>,
    pub timestamp: u64,
    pub height: u64,
    pub blue_score: u64,
    pub difficulty: u32,
    pub state_root: String,
}

/// Build a chain-bound v2 coinbase transaction without changing the historical
/// v1 coinbase builder or any live mining caller.
pub fn build_coinbase_transaction_v2(
    miner_address: &str,
    reward: u64,
    nonce: u64,
    chain_id: &str,
) -> Result<Transaction, PulseError> {
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![],
        outputs: vec![TxOutput {
            address: miner_address.to_string(),
            amount: reward,
        }],
        fee: 0,
        nonce,
    };
    tx.txid = compute_txid_v2(&tx, chain_id)?;
    Ok(tx)
}

/// Build a canonical chain-bound v2 candidate block from explicit consensus
/// metadata. The caller must supply the GHOSTDAG-derived `blue_score` and the
/// authoritative `state_root`; this helper never substitutes height for either.
pub fn build_candidate_block_v2(
    spec: CandidateBlockV2Spec,
    txs: Vec<Transaction>,
    chain_id: &str,
) -> Result<Block, PulseError> {
    let parents = canonicalize_block_parents_v2(&spec.parents)?;
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents,
            timestamp: spec.timestamp,
            difficulty: spec.difficulty,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: spec.state_root,
            blue_score: spec.blue_score,
            height: spec.height,
        },
        transactions: txs,
    };
    block.hash = compute_block_hash_v2(&block.header, chain_id)?;
    Ok(block)
}

/// Refresh transaction-derived consensus ids for an already-formed v2 block.
/// State-root derivation remains a separate activation-gated concern because it
/// must use the final protocol-bound state transition path rather than legacy
/// `compute_post_state_root` implicitly.
pub fn refresh_block_consensus_ids_v2(block: &mut Block, chain_id: &str) -> Result<(), PulseError> {
    block.header.merkle_root = compute_merkle_root(&block.transactions);
    block.hash = compute_block_hash_v2(&block.header, chain_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mining::{build_candidate_block, build_coinbase_transaction},
        protocol::BLOCK_HEADER_VERSION_V1,
        tx::{compute_txid, TRANSACTION_VERSION_V1},
    };

    fn candidate_spec(parents: Vec<String>) -> CandidateBlockV2Spec {
        CandidateBlockV2Spec {
            parents,
            timestamp: 1_700_000_000,
            height: 10,
            blue_score: 42,
            difficulty: 0x1e00_ffff,
            state_root: "state-root-v2".to_string(),
        }
    }

    #[test]
    fn v2_coinbase_is_chain_bound_and_v1_builder_remains_unchanged() {
        let testnet =
            build_coinbase_transaction_v2("pulse1miner", 50, 7, "pulsedag-testnet").unwrap();
        let private =
            build_coinbase_transaction_v2("pulse1miner", 50, 7, "pulsedag-private").unwrap();
        assert_eq!(testnet.version, TRANSACTION_VERSION_V2);
        assert_eq!(
            testnet.txid,
            compute_txid_v2(&testnet, "pulsedag-testnet").unwrap()
        );
        assert_ne!(testnet.txid, private.txid);

        let legacy = build_coinbase_transaction("pulse1miner", 50, 7);
        assert_eq!(legacy.version, TRANSACTION_VERSION_V1);
        assert_eq!(legacy.txid, compute_txid(&legacy));
    }

    #[test]
    fn candidate_v2_canonicalizes_parent_permutations() {
        let tx = build_coinbase_transaction_v2("pulse1miner", 50, 7, "pulsedag-testnet").unwrap();
        let mut forward_spec = candidate_spec(vec!["11".repeat(32), "22".repeat(32)]);
        forward_spec.height = 9;
        forward_spec.blue_score = 15;
        let mut reverse_spec = forward_spec.clone();
        reverse_spec.parents.reverse();

        let forward = build_candidate_block_v2(
            forward_spec,
            vec![tx.clone()],
            "pulsedag-testnet",
        )
        .unwrap();
        let reverse =
            build_candidate_block_v2(reverse_spec, vec![tx], "pulsedag-testnet").unwrap();

        assert_eq!(forward.header.parents, reverse.header.parents);
        assert_eq!(forward.hash, reverse.hash);
    }

    #[test]
    fn candidate_v2_preserves_explicit_consensus_metadata() {
        let mut spec = candidate_spec(vec!["11".repeat(32)]);
        spec.state_root = "authoritative-state-root".to_string();
        let block = build_candidate_block_v2(spec, vec![], "pulsedag-testnet").unwrap();

        assert_eq!(block.header.version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(block.header.height, 10);
        assert_eq!(block.header.blue_score, 42);
        assert_eq!(block.header.state_root, "authoritative-state-root");
    }

    #[test]
    fn candidate_v2_hash_is_chain_bound_and_empty_chain_fails_closed() {
        let spec = candidate_spec(vec!["11".repeat(32)]);
        let testnet = build_candidate_block_v2(spec.clone(), vec![], "pulsedag-testnet").unwrap();
        let private = build_candidate_block_v2(spec.clone(), vec![], "pulsedag-private").unwrap();
        assert_ne!(testnet.hash, private.hash);

        assert!(build_candidate_block_v2(spec, vec![], "").is_err());
    }

    #[test]
    fn legacy_candidate_builder_remains_header_v1() {
        let block = build_candidate_block(vec!["parent".to_string()], 2, 1, vec![]);
        assert_eq!(block.header.version, BLOCK_HEADER_VERSION_V1);
    }
}
