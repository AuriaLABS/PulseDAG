//! Dormant, strictly versioned v3 block envelope with nanosecond headers.
//!
//! Unlike `types::Block`, this type contains `BlockHeaderV3`. It is not
//! registered with mined/P2P admission, storage, RPC or authoritative replay.
//! A validated envelope is NOT proof of PoW, correct selected-parent/blue-work,
//! UTXO validity, valid signatures, acceptable reward, or launch authorization.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{
    errors::PulseError,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    tx::compute_txid_v2,
    types::{compute_merkle_root, BlockId, Transaction},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockV3 {
    pub hash: BlockId,
    pub header: BlockHeaderV3,
    pub transactions: Vec<Transaction>,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(message.into())
}

fn verify_v3_body_commitments(
    header: &BlockHeaderV3,
    transactions: &[Transaction],
    chain_id: &str,
) -> Result<(), PulseError> {
    // V3 genesis remains zero allocation. No transaction-bearing genesis can
    // pass just because its header and Merkle root are internally consistent.
    if header.height == 0 && !transactions.is_empty() {
        return Err(invalid("v3 genesis block must have zero transactions"));
    }

    let mut seen = HashSet::with_capacity(transactions.len());
    for transaction in transactions {
        // V3.0 reuses the v2 chain-bound transaction domain, NOT v1.
        // This validates deterministic txid/body binding, not signatures.
        let expected_txid = compute_txid_v2(transaction, chain_id)?;
        if expected_txid != transaction.txid {
            return Err(invalid("v3 block transaction txid/body mismatch"));
        }
        if !seen.insert(transaction.txid.as_str()) {
            return Err(invalid("v3 block contains duplicate transaction txids"));
        }
    }

    if header.merkle_root != compute_merkle_root(transactions) {
        return Err(invalid("v3 block header Merkle root differs from transactions"));
    }
    Ok(())
}

/// Produce a chain-bound, v3-only envelope from a header/body pair.
/// No standalone result from this helper can authorize mined/P2P acceptance.
pub fn build_block_v3_envelope(
    header: BlockHeaderV3,
    transactions: Vec<Transaction>,
    chain_id: &str,
) -> Result<BlockV3, PulseError> {
    let hash = compute_block_hash_v3(&header, chain_id)?;
    verify_v3_body_commitments(&header, &transactions, chain_id)?;
    Ok(BlockV3 {
        hash,
        header,
        transactions,
    })
}

/// Recheck the v3 header hash, exact v2-chain-bound txids, canonical Merkle
/// commitment, and zero-allocation genesis. PoW, consensus time, and state
/// transitions are deliberately outside this dormant structural check.
pub fn verify_block_v3_envelope(block: &BlockV3, chain_id: &str) -> Result<(), PulseError> {
    let expected = compute_block_hash_v3(&block.header, chain_id)?;
    if block.hash != expected {
        return Err(invalid("v3 block hash is not the chain-bound header commitment"));
    }
    verify_v3_body_commitments(&block.header, &block.transactions, chain_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::{BLOCK_HEADER_VERSION_V2, BLOCK_HEADER_VERSION_V3},
        tx::TRANSACTION_VERSION_V2,
        types::{Block, BlockHeader, TxOutput},
    };

    const CHAIN: &str = "pulsedag-v3-envelope-candidate";
    const NS: u64 = 1_800_000_000_000_000_000;

    fn header(height: u64, txs: &[Transaction]) -> BlockHeaderV3 {
        BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: if height == 0 {
                vec![]
            } else {
                vec!["11".repeat(32)]
            },
            timestamp_ns: NS + height * 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: height,
            merkle_root: compute_merkle_root(txs),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        }
    }

    fn tx() -> Transaction {
        let mut transaction = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "pulse1candidate".into(),
                amount: 42,
            }],
            fee: 0,
            nonce: 7,
        };
        transaction.txid = compute_txid_v2(&transaction, CHAIN).unwrap();
        transaction
    }

    #[test]
    fn exact_v3_header_and_empty_genesis_roundtrip() {
        let block = build_block_v3_envelope(header(0, &[]), vec![], CHAIN).unwrap();
        verify_block_v3_envelope(&block, CHAIN).unwrap();
        assert_eq!(block.header.timestamp_ns, NS);
        assert!(block.transactions.is_empty());

        let json = serde_json::to_vec(&block).unwrap();
        let restored: BlockV3 = serde_json::from_slice(&json).unwrap();
        verify_block_v3_envelope(&restored, CHAIN).unwrap();
        assert_eq!(restored.hash, block.hash);
    }

    #[test]
    fn v2_seconds_blocks_cannot_deserialize_into_v3_envelope() {
        let block = Block {
            hash: "44".repeat(32),
            header: BlockHeader {
                version: BLOCK_HEADER_VERSION_V2,
                parents: vec!["11".repeat(32)],
                timestamp: NS / 1_000_000_000,
                difficulty: 0x207f_ffff,
                nonce: 4,
                merkle_root: compute_merkle_root(&[]),
                state_root: "33".repeat(32),
                blue_score: 1,
                height: 1,
            },
            transactions: vec![],
        };
        let json = serde_json::to_vec(&block).unwrap();
        assert!(serde_json::from_slice::<BlockV3>(&json).is_err());

        let v3 = build_block_v3_envelope(header(1, &[]), vec![], CHAIN).unwrap();
        let json = serde_json::to_vec(&v3).unwrap();
        assert!(serde_json::from_slice::<Block>(&json).is_err());
    }

    #[test]
    fn chain_domain_and_header_body_tampering_fail_closed() {
        let transaction = tx();
        let mut block =
            build_block_v3_envelope(header(1, &[transaction.clone()]), vec![transaction], CHAIN)
                .unwrap();
        verify_block_v3_envelope(&block, CHAIN).unwrap();

        assert!(verify_block_v3_envelope(&block, "different-chain").is_err());

        block.transactions[0].outputs[0].amount += 1;
        assert!(verify_block_v3_envelope(&block, CHAIN).is_err());

        let transaction = tx();
        let mut block =
            build_block_v3_envelope(header(1, &[transaction.clone()]), vec![transaction], CHAIN)
                .unwrap();
        block.header.timestamp_ns += 1;
        assert!(verify_block_v3_envelope(&block, CHAIN).is_err());

        block.header.timestamp_ns -= 1;
        block.hash = "00".repeat(32);
        assert!(verify_block_v3_envelope(&block, CHAIN).is_err());
    }

    #[test]
    fn merkle_substitution_and_duplicate_txids_are_rejected() {
        let transaction = tx();
        let mut wrong_root = header(1, &[]);
        let error = build_block_v3_envelope(wrong_root.clone(), vec![transaction.clone()], CHAIN)
            .unwrap_err();
        assert!(error.to_string().contains("Merkle"));

        wrong_root.merkle_root = compute_merkle_root(&[
            transaction.clone(),
            transaction.clone(),
        ]);
        let error = build_block_v3_envelope(
            wrong_root,
            vec![transaction.clone(), transaction],
            CHAIN,
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn zero_allocation_v3_genesis_rejects_transaction_even_with_valid_merkle() {
        let transaction = tx();
        let error = build_block_v3_envelope(
            header(0, &[transaction.clone()]),
            vec![transaction],
            CHAIN,
        )
        .unwrap_err();
        assert!(error.to_string().contains("zero transactions"));
    }

    #[test]
    fn wrong_transaction_version_cannot_enter_envelope() {
        let mut transaction = tx();
        transaction.version = 1;
        let error = build_block_v3_envelope(
            header(1, &[transaction.clone()]),
            vec![transaction],
            CHAIN,
        )
        .unwrap_err();
        assert!(error.to_string().contains("transaction"));
    }
}
