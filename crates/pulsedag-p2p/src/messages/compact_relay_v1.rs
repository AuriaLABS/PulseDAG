use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use pulsedag_core::{
    compute_block_hash_v2,
    types::{compute_block_hash, compute_merkle_root, Block, BlockHeader, Hash, Transaction},
    BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2,
};
use serde::{Deserialize, Serialize};

use crate::{MAX_INV_BLOCK_HASHES, MAX_INV_BLOCK_REQUEST_FANOUT};

pub const COMPACT_DAG_RELAY_VERSION_V1: u32 = 1;
pub const MAX_COMPACT_BLOCK_TXIDS_V1: usize = MAX_INV_BLOCK_HASHES;
pub const MAX_COMPACT_PREFILLED_TRANSACTIONS_V1: usize = MAX_INV_BLOCK_REQUEST_FANOUT;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactPrefilledTransactionV1 {
    pub index: u32,
    pub transaction: Transaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompactBlockRelayV1 {
    pub version: u32,
    pub chain_id: String,
    pub block_hash: Hash,
    pub header: BlockHeader,
    /// Canonical transaction order for the block. Full txids are intentional in
    /// v1: collision-prone short identifiers are not part of this foundation.
    pub transaction_ids: Vec<Hash>,
    /// Transactions that must not depend on receiver mempool state. Index zero
    /// (the coinbase position) is mandatory.
    pub prefilled_transactions: Vec<CompactPrefilledTransactionV1>,
}

#[derive(Debug, Clone)]
pub enum CompactBlockReconstructionV1 {
    Complete(Block),
    NeedFullBlock {
        block_hash: Hash,
        missing_transaction_ids: Vec<Hash>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactRelayErrorV1 {
    UnsupportedVersion(u32),
    EmptyChainId,
    UnsupportedHeaderVersion(u32),
    BlockHashMismatch,
    EmptyTransactionList,
    TooManyTransactionIds(usize),
    TooManyPrefilledTransactions(usize),
    DuplicateTransactionId(Hash),
    DuplicatePrefilledIndex(u32),
    PrefilledIndexOutOfRange(u32),
    MissingPrefilledCoinbase,
    PrefilledTransactionIdMismatch { index: u32 },
    LocalTransactionIdMismatch(Hash),
    MerkleRootMismatch,
}

impl fmt::Display for CompactRelayErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported compact DAG relay version {version}")
            }
            Self::EmptyChainId => f.write_str("compact DAG relay chain_id must not be empty"),
            Self::UnsupportedHeaderVersion(version) => {
                write!(f, "unsupported compact DAG relay block header version {version}")
            }
            Self::BlockHashMismatch => f.write_str("compact DAG relay block hash mismatch"),
            Self::EmptyTransactionList => {
                f.write_str("compact DAG relay transaction list must not be empty")
            }
            Self::TooManyTransactionIds(count) => write!(
                f,
                "compact DAG relay contains {count} transaction ids; maximum is {MAX_COMPACT_BLOCK_TXIDS_V1}"
            ),
            Self::TooManyPrefilledTransactions(count) => write!(
                f,
                "compact DAG relay contains {count} prefilled transactions; maximum is {MAX_COMPACT_PREFILLED_TRANSACTIONS_V1}"
            ),
            Self::DuplicateTransactionId(txid) => {
                write!(f, "compact DAG relay contains duplicate transaction id {txid}")
            }
            Self::DuplicatePrefilledIndex(index) => {
                write!(f, "compact DAG relay contains duplicate prefilled index {index}")
            }
            Self::PrefilledIndexOutOfRange(index) => {
                write!(f, "compact DAG relay prefilled index {index} is out of range")
            }
            Self::MissingPrefilledCoinbase => {
                f.write_str("compact DAG relay requires transaction index zero to be prefilled")
            }
            Self::PrefilledTransactionIdMismatch { index } => write!(
                f,
                "compact DAG relay prefilled transaction at index {index} does not match declared txid"
            ),
            Self::LocalTransactionIdMismatch(txid) => write!(
                f,
                "local transaction selected for compact DAG relay does not match txid {txid}"
            ),
            Self::MerkleRootMismatch => {
                f.write_str("reconstructed compact DAG relay block has a merkle-root mismatch")
            }
        }
    }
}

impl Error for CompactRelayErrorV1 {}

impl CompactBlockRelayV1 {
    pub fn validate_envelope(&self) -> Result<(), CompactRelayErrorV1> {
        if self.version != COMPACT_DAG_RELAY_VERSION_V1 {
            return Err(CompactRelayErrorV1::UnsupportedVersion(self.version));
        }
        if self.chain_id.is_empty() || self.chain_id.trim() != self.chain_id {
            return Err(CompactRelayErrorV1::EmptyChainId);
        }
        if self.transaction_ids.is_empty() {
            return Err(CompactRelayErrorV1::EmptyTransactionList);
        }
        if self.transaction_ids.len() > MAX_COMPACT_BLOCK_TXIDS_V1 {
            return Err(CompactRelayErrorV1::TooManyTransactionIds(
                self.transaction_ids.len(),
            ));
        }
        if self.prefilled_transactions.len() > MAX_COMPACT_PREFILLED_TRANSACTIONS_V1 {
            return Err(CompactRelayErrorV1::TooManyPrefilledTransactions(
                self.prefilled_transactions.len(),
            ));
        }

        match self.header.version {
            BLOCK_HEADER_VERSION_V1 => {
                if compute_block_hash(&self.header) != self.block_hash {
                    return Err(CompactRelayErrorV1::BlockHashMismatch);
                }
            }
            BLOCK_HEADER_VERSION_V2 => {
                let computed = compute_block_hash_v2(&self.header, &self.chain_id)
                    .map_err(|_| CompactRelayErrorV1::BlockHashMismatch)?;
                if computed != self.block_hash {
                    return Err(CompactRelayErrorV1::BlockHashMismatch);
                }
            }
            version => return Err(CompactRelayErrorV1::UnsupportedHeaderVersion(version)),
        }

        let mut seen_txids = HashSet::with_capacity(self.transaction_ids.len());
        for txid in &self.transaction_ids {
            if !seen_txids.insert(txid) {
                return Err(CompactRelayErrorV1::DuplicateTransactionId(txid.clone()));
            }
        }

        let mut seen_indexes = HashSet::with_capacity(self.prefilled_transactions.len());
        let mut coinbase_prefilled = false;
        for prefilled in &self.prefilled_transactions {
            let index = prefilled.index as usize;
            if index >= self.transaction_ids.len() {
                return Err(CompactRelayErrorV1::PrefilledIndexOutOfRange(
                    prefilled.index,
                ));
            }
            if !seen_indexes.insert(prefilled.index) {
                return Err(CompactRelayErrorV1::DuplicatePrefilledIndex(
                    prefilled.index,
                ));
            }
            if prefilled.index == 0 {
                coinbase_prefilled = true;
            }
            if prefilled.transaction.txid != self.transaction_ids[index] {
                return Err(CompactRelayErrorV1::PrefilledTransactionIdMismatch {
                    index: prefilled.index,
                });
            }
        }
        if !coinbase_prefilled {
            return Err(CompactRelayErrorV1::MissingPrefilledCoinbase);
        }
        Ok(())
    }
}

/// Reconstruct a compact relay candidate strictly from the declared ordered
/// txids, prefilled transactions and locally-known transactions.
///
/// Missing transactions are *not* fetched piecemeal in v1. The caller receives
/// `NeedFullBlock` and must use the existing GetBlock/BlockData fallback. A
/// successful reconstruction is an ordinary `Block`; callers must feed it into
/// the same P2P block preflight/acceptance path used by a full block. This
/// function deliberately performs no consensus acceptance or state mutation.
pub fn reconstruct_compact_block_v1(
    compact: &CompactBlockRelayV1,
    local_transactions: &HashMap<Hash, Transaction>,
) -> Result<CompactBlockReconstructionV1, CompactRelayErrorV1> {
    compact.validate_envelope()?;

    let mut prefilled_by_index = HashMap::with_capacity(compact.prefilled_transactions.len());
    for prefilled in &compact.prefilled_transactions {
        prefilled_by_index.insert(prefilled.index as usize, prefilled.transaction.clone());
    }

    let mut missing = Vec::new();
    let mut transactions = Vec::with_capacity(compact.transaction_ids.len());
    for (index, txid) in compact.transaction_ids.iter().enumerate() {
        let transaction = if let Some(prefilled) = prefilled_by_index.get(&index) {
            prefilled.clone()
        } else if let Some(local) = local_transactions.get(txid) {
            if local.txid != *txid {
                return Err(CompactRelayErrorV1::LocalTransactionIdMismatch(txid.clone()));
            }
            local.clone()
        } else {
            missing.push(txid.clone());
            continue;
        };
        transactions.push(transaction);
    }

    if !missing.is_empty() {
        return Ok(CompactBlockReconstructionV1::NeedFullBlock {
            block_hash: compact.block_hash.clone(),
            missing_transaction_ids: missing,
        });
    }

    debug_assert_eq!(transactions.len(), compact.transaction_ids.len());
    if compute_merkle_root(&transactions) != compact.header.merkle_root {
        return Err(CompactRelayErrorV1::MerkleRootMismatch);
    }

    Ok(CompactBlockReconstructionV1::Complete(Block {
        hash: compact.block_hash.clone(),
        header: compact.header.clone(),
        transactions,
    }))
}

#[cfg(test)]
mod tests {
    use pulsedag_core::types::{TxOutput, Transaction};

    use super::*;

    fn tx(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                address: "pulse1fixture".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 1,
        }
    }

    fn compact_fixture() -> (CompactBlockRelayV1, Transaction, Transaction) {
        let coinbase = tx("coinbase");
        let spend = tx("spend");
        let transactions = vec![coinbase.clone(), spend.clone()];
        let header = BlockHeader {
            version: BLOCK_HEADER_VERSION_V1,
            parents: vec!["parent".into()],
            timestamp: 1,
            difficulty: 1,
            nonce: 7,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "state".into(),
            blue_score: 1,
            height: 1,
        };
        let block_hash = compute_block_hash(&header);
        (
            CompactBlockRelayV1 {
                version: COMPACT_DAG_RELAY_VERSION_V1,
                chain_id: "testnet".into(),
                block_hash,
                header,
                transaction_ids: transactions.into_iter().map(|tx| tx.txid).collect(),
                prefilled_transactions: vec![CompactPrefilledTransactionV1 {
                    index: 0,
                    transaction: coinbase.clone(),
                }],
            },
            coinbase,
            spend,
        )
    }

    #[test]
    fn reconstructs_complete_block_from_prefilled_and_local_transactions() {
        let (compact, coinbase, spend) = compact_fixture();
        let local = HashMap::from([(spend.txid.clone(), spend.clone())]);
        let result = reconstruct_compact_block_v1(&compact, &local).unwrap();
        match result {
            CompactBlockReconstructionV1::Complete(block) => {
                assert_eq!(block.hash, compact.block_hash);
                assert_eq!(block.transactions.len(), 2);
                assert_eq!(block.transactions[0].txid, coinbase.txid);
                assert_eq!(block.transactions[1].txid, spend.txid);
            }
            other => panic!("expected complete compact reconstruction, got {other:?}"),
        }
    }

    #[test]
    fn missing_transaction_requires_full_block_fallback() {
        let (compact, _, spend) = compact_fixture();
        let result = reconstruct_compact_block_v1(&compact, &HashMap::new()).unwrap();
        match result {
            CompactBlockReconstructionV1::NeedFullBlock {
                block_hash,
                missing_transaction_ids,
            } => {
                assert_eq!(block_hash, compact.block_hash);
                assert_eq!(missing_transaction_ids, vec![spend.txid]);
            }
            other => panic!("expected full-block fallback, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_transaction_ids_fail_closed() {
        let (mut compact, _, _) = compact_fixture();
        compact.transaction_ids[1] = compact.transaction_ids[0].clone();
        assert!(matches!(
            compact.validate_envelope(),
            Err(CompactRelayErrorV1::DuplicateTransactionId(_))
        ));
    }

    #[test]
    fn coinbase_must_be_prefilled() {
        let (mut compact, _, _) = compact_fixture();
        compact.prefilled_transactions.clear();
        assert_eq!(
            compact.validate_envelope(),
            Err(CompactRelayErrorV1::MissingPrefilledCoinbase)
        );
    }

    #[test]
    fn prefilled_transaction_must_match_declared_position() {
        let (mut compact, _, spend) = compact_fixture();
        compact.prefilled_transactions[0].transaction = spend;
        assert_eq!(
            compact.validate_envelope(),
            Err(CompactRelayErrorV1::PrefilledTransactionIdMismatch { index: 0 })
        );
    }

    #[test]
    fn reconstructed_merkle_root_must_match_header() {
        let (mut compact, _, spend) = compact_fixture();
        compact.header.merkle_root = "corrupt-merkle-root".into();
        compact.block_hash = compute_block_hash(&compact.header);
        let local = HashMap::from([(spend.txid.clone(), spend)]);
        assert_eq!(
            reconstruct_compact_block_v1(&compact, &local).unwrap_err(),
            CompactRelayErrorV1::MerkleRootMismatch
        );
    }

    #[test]
    fn transaction_list_is_bounded_by_shared_p2p_inventory_budget() {
        let (mut compact, _, _) = compact_fixture();
        compact.transaction_ids = (0..=MAX_COMPACT_BLOCK_TXIDS_V1)
            .map(|index| format!("tx-{index}"))
            .collect();
        assert_eq!(
            compact.validate_envelope(),
            Err(CompactRelayErrorV1::TooManyTransactionIds(
                MAX_COMPACT_BLOCK_TXIDS_V1 + 1
            ))
        );
    }
}
