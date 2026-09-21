use std::collections::{HashMap, HashSet};
use std::fmt;

use pulsedag_core::{
    types::{compute_merkle_root, Block, BlockHeader, Hash, Transaction},
    GHOSTDAG_V1_MAX_PARENTS,
};
use serde::{Deserialize, Serialize};

use super::{P2P_WIRE_MAX_INVENTORY_ITEMS_V1, P2P_WIRE_MAX_REQUEST_ITEMS_V1};

pub const COMPACT_DAG_RELAY_VERSION_V1: u16 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct CompactBlockAnnouncementV1 {
    pub version: u16,
    pub block_hash: Hash,
    pub header: BlockHeader,
    pub txids: Vec<Hash>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactTransactionRequestV1 {
    pub version: u16,
    pub block_hash: Hash,
    pub txids: Vec<Hash>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactTransactionResponseV1 {
    pub version: u16,
    pub block_hash: Hash,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactRelayFallbackReasonV1 {
    MissingTransactionFanoutExceeded,
    MerkleRootMismatch,
}

#[derive(Debug, Clone)]
pub enum CompactBlockReconstructionPlanV1 {
    Complete(Block),
    RequestTransactions(CompactTransactionRequestV1),
    FullBlockFallback {
        block_hash: Hash,
        reason: CompactRelayFallbackReasonV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactRelayErrorV1 {
    UnsupportedVersion(u16),
    UnsupportedHeaderVersion(u32),
    HeaderParentCountTooLarge { observed: usize, maximum: usize },
    EmptyTransactionInventory,
    LocalBlockMerkleRootMismatch,
    TransactionInventoryTooLarge { observed: usize, maximum: usize },
    DuplicateTransactionId(Hash),
    KnownTransactionIdMismatch { requested: Hash, observed: Hash },
    ResponseBlockMismatch { expected: Hash, observed: Hash },
    ResponseCountMismatch { expected: usize, observed: usize },
    ResponseTransactionMismatch {
        index: usize,
        expected: Hash,
        observed: Hash,
    },
    UnexpectedResponseForCompleteBlock,
}

impl fmt::Display for CompactRelayErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported compact DAG relay version {version}")
            }
            Self::UnsupportedHeaderVersion(version) => {
                write!(formatter, "unsupported compact DAG relay header version {version}")
            }
            Self::HeaderParentCountTooLarge { observed, maximum } => write!(
                formatter,
                "compact DAG relay parent count exceeds bound: observed={observed} maximum={maximum}"
            ),
            Self::EmptyTransactionInventory => {
                write!(formatter, "compact block transaction inventory is empty")
            }
            Self::LocalBlockMerkleRootMismatch => {
                write!(formatter, "local block Merkle root does not match its transaction body")
            }
            Self::TransactionInventoryTooLarge { observed, maximum } => write!(
                formatter,
                "compact block transaction inventory exceeds bound: observed={observed} maximum={maximum}"
            ),
            Self::DuplicateTransactionId(txid) => {
                write!(formatter, "duplicate compact block transaction id {txid}")
            }
            Self::KnownTransactionIdMismatch {
                requested,
                observed,
            } => write!(
                formatter,
                "known transaction id mismatch: requested={requested} observed={observed}"
            ),
            Self::ResponseBlockMismatch { expected, observed } => write!(
                formatter,
                "compact transaction response block mismatch: expected={expected} observed={observed}"
            ),
            Self::ResponseCountMismatch { expected, observed } => write!(
                formatter,
                "compact transaction response count mismatch: expected={expected} observed={observed}"
            ),
            Self::ResponseTransactionMismatch {
                index,
                expected,
                observed,
            } => write!(
                formatter,
                "compact transaction response mismatch at index {index}: expected={expected} observed={observed}"
            ),
            Self::UnexpectedResponseForCompleteBlock => {
                write!(formatter, "compact transaction response supplied for already complete block")
            }
        }
    }
}

impl std::error::Error for CompactRelayErrorV1 {}

fn require_version(version: u16) -> Result<(), CompactRelayErrorV1> {
    if version == COMPACT_DAG_RELAY_VERSION_V1 {
        Ok(())
    } else {
        Err(CompactRelayErrorV1::UnsupportedVersion(version))
    }
}

fn validate_txids(txids: &[Hash]) -> Result<(), CompactRelayErrorV1> {
    if txids.is_empty() {
        return Err(CompactRelayErrorV1::EmptyTransactionInventory);
    }
    if txids.len() > P2P_WIRE_MAX_INVENTORY_ITEMS_V1 {
        return Err(CompactRelayErrorV1::TransactionInventoryTooLarge {
            observed: txids.len(),
            maximum: P2P_WIRE_MAX_INVENTORY_ITEMS_V1,
        });
    }

    let mut seen = HashSet::with_capacity(txids.len());
    for txid in txids {
        if !seen.insert(txid.clone()) {
            return Err(CompactRelayErrorV1::DuplicateTransactionId(txid.clone()));
        }
    }
    Ok(())
}

pub fn validate_compact_block_announcement_v1(
    announcement: &CompactBlockAnnouncementV1,
) -> Result<(), CompactRelayErrorV1> {
    require_version(announcement.version)?;
    if !super::supported_header_version(announcement.header.version) {
        return Err(CompactRelayErrorV1::UnsupportedHeaderVersion(
            announcement.header.version,
        ));
    }
    if announcement.header.parents.len() > GHOSTDAG_V1_MAX_PARENTS {
        return Err(CompactRelayErrorV1::HeaderParentCountTooLarge {
            observed: announcement.header.parents.len(),
            maximum: GHOSTDAG_V1_MAX_PARENTS,
        });
    }
    validate_txids(&announcement.txids)
}

pub fn build_compact_block_announcement_v1(
    block: &Block,
) -> Result<CompactBlockAnnouncementV1, CompactRelayErrorV1> {
    let txids = block
        .transactions
        .iter()
        .map(|transaction| transaction.txid.clone())
        .collect::<Vec<_>>();
    validate_txids(&txids)?;
    if compute_merkle_root(&block.transactions) != block.header.merkle_root {
        return Err(CompactRelayErrorV1::LocalBlockMerkleRootMismatch);
    }

    let announcement = CompactBlockAnnouncementV1 {
        version: COMPACT_DAG_RELAY_VERSION_V1,
        block_hash: block.hash.clone(),
        header: block.header.clone(),
        txids,
    };
    validate_compact_block_announcement_v1(&announcement)?;
    Ok(announcement)
}

fn reconstruct_known_transactions(
    announcement: &CompactBlockAnnouncementV1,
    known_transactions: &HashMap<Hash, Transaction>,
) -> Result<(Vec<Transaction>, Vec<Hash>), CompactRelayErrorV1> {
    let mut transactions = Vec::with_capacity(announcement.txids.len());
    let mut missing = Vec::new();

    for txid in &announcement.txids {
        match known_transactions.get(txid) {
            Some(transaction) => {
                if transaction.txid != *txid {
                    return Err(CompactRelayErrorV1::KnownTransactionIdMismatch {
                        requested: txid.clone(),
                        observed: transaction.txid.clone(),
                    });
                }
                transactions.push(transaction.clone());
            }
            None => missing.push(txid.clone()),
        }
    }

    Ok((transactions, missing))
}

pub fn plan_compact_block_reconstruction_v1(
    announcement: &CompactBlockAnnouncementV1,
    known_transactions: &HashMap<Hash, Transaction>,
) -> Result<CompactBlockReconstructionPlanV1, CompactRelayErrorV1> {
    validate_compact_block_announcement_v1(announcement)?;
    let (transactions, missing) =
        reconstruct_known_transactions(announcement, known_transactions)?;

    if missing.len() > P2P_WIRE_MAX_REQUEST_ITEMS_V1 {
        return Ok(CompactBlockReconstructionPlanV1::FullBlockFallback {
            block_hash: announcement.block_hash.clone(),
            reason: CompactRelayFallbackReasonV1::MissingTransactionFanoutExceeded,
        });
    }

    if !missing.is_empty() {
        return Ok(CompactBlockReconstructionPlanV1::RequestTransactions(
            CompactTransactionRequestV1 {
                version: COMPACT_DAG_RELAY_VERSION_V1,
                block_hash: announcement.block_hash.clone(),
                txids: missing,
            },
        ));
    }

    if compute_merkle_root(&transactions) != announcement.header.merkle_root {
        return Ok(CompactBlockReconstructionPlanV1::FullBlockFallback {
            block_hash: announcement.block_hash.clone(),
            reason: CompactRelayFallbackReasonV1::MerkleRootMismatch,
        });
    }

    Ok(CompactBlockReconstructionPlanV1::Complete(Block {
        hash: announcement.block_hash.clone(),
        header: announcement.header.clone(),
        transactions,
    }))
}

pub fn validate_compact_transaction_request_v1(
    request: &CompactTransactionRequestV1,
) -> Result<(), CompactRelayErrorV1> {
    require_version(request.version)?;
    if request.txids.is_empty() {
        return Err(CompactRelayErrorV1::EmptyTransactionInventory);
    }
    if request.txids.len() > P2P_WIRE_MAX_REQUEST_ITEMS_V1 {
        return Err(CompactRelayErrorV1::TransactionInventoryTooLarge {
            observed: request.txids.len(),
            maximum: P2P_WIRE_MAX_REQUEST_ITEMS_V1,
        });
    }

    let mut seen = HashSet::with_capacity(request.txids.len());
    for txid in &request.txids {
        if !seen.insert(txid.clone()) {
            return Err(CompactRelayErrorV1::DuplicateTransactionId(txid.clone()));
        }
    }
    Ok(())
}

pub fn build_compact_transaction_response_v1(
    request: &CompactTransactionRequestV1,
    available_transactions: &HashMap<Hash, Transaction>,
) -> Result<Option<CompactTransactionResponseV1>, CompactRelayErrorV1> {
    validate_compact_transaction_request_v1(request)?;
    let mut transactions = Vec::with_capacity(request.txids.len());

    for txid in &request.txids {
        let Some(transaction) = available_transactions.get(txid) else {
            return Ok(None);
        };
        if transaction.txid != *txid {
            return Err(CompactRelayErrorV1::KnownTransactionIdMismatch {
                requested: txid.clone(),
                observed: transaction.txid.clone(),
            });
        }
        transactions.push(transaction.clone());
    }

    Ok(Some(CompactTransactionResponseV1 {
        version: COMPACT_DAG_RELAY_VERSION_V1,
        block_hash: request.block_hash.clone(),
        transactions,
    }))
}

pub fn complete_compact_block_reconstruction_v1(
    announcement: &CompactBlockAnnouncementV1,
    known_transactions: &HashMap<Hash, Transaction>,
    response: &CompactTransactionResponseV1,
) -> Result<CompactBlockReconstructionPlanV1, CompactRelayErrorV1> {
    require_version(response.version)?;

    let request = match plan_compact_block_reconstruction_v1(announcement, known_transactions)? {
        CompactBlockReconstructionPlanV1::RequestTransactions(request) => request,
        CompactBlockReconstructionPlanV1::Complete(_) => {
            return Err(CompactRelayErrorV1::UnexpectedResponseForCompleteBlock);
        }
        fallback @ CompactBlockReconstructionPlanV1::FullBlockFallback { .. } => {
            return Ok(fallback);
        }
    };

    if response.block_hash != request.block_hash {
        return Err(CompactRelayErrorV1::ResponseBlockMismatch {
            expected: request.block_hash,
            observed: response.block_hash.clone(),
        });
    }
    if response.transactions.len() != request.txids.len() {
        return Err(CompactRelayErrorV1::ResponseCountMismatch {
            expected: request.txids.len(),
            observed: response.transactions.len(),
        });
    }

    let mut combined = known_transactions.clone();
    for (index, (expected_txid, transaction)) in request
        .txids
        .iter()
        .zip(&response.transactions)
        .enumerate()
    {
        if transaction.txid != *expected_txid {
            return Err(CompactRelayErrorV1::ResponseTransactionMismatch {
                index,
                expected: expected_txid.clone(),
                observed: transaction.txid.clone(),
            });
        }
        combined.insert(expected_txid.clone(), transaction.clone());
    }

    plan_compact_block_reconstruction_v1(announcement, &combined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::types::TxOutput;

    fn transaction(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }
    }

    fn block(txids: &[&str]) -> Block {
        let transactions = txids
            .iter()
            .map(|txid| transaction(txid))
            .collect::<Vec<_>>();
        Block {
            hash: "block-hash".into(),
            header: BlockHeader {
                version: 1,
                parents: vec!["parent-a".into(), "parent-b".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: compute_merkle_root(&transactions),
                state_root: "state".into(),
                blue_score: 2,
                height: 2,
            },
            transactions,
        }
    }

    fn known(block: &Block, keep: &[usize]) -> HashMap<Hash, Transaction> {
        keep.iter()
            .map(|index| {
                let transaction = block.transactions[*index].clone();
                (transaction.txid.clone(), transaction)
            })
            .collect()
    }

    #[test]
    fn announcement_is_header_first_and_preserves_transaction_order() {
        let block = block(&["coinbase", "tx-a", "tx-b"]);
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        assert_eq!(announcement.version, COMPACT_DAG_RELAY_VERSION_V1);
        assert_eq!(announcement.block_hash, block.hash);
        assert_eq!(announcement.header.parents, block.header.parents);
        assert_eq!(announcement.txids, vec!["coinbase", "tx-a", "tx-b"]);
    }

    #[test]
    fn complete_known_inventory_reconstructs_exact_block_body() {
        let block = block(&["coinbase", "tx-a", "tx-b"]);
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = known(&block, &[0, 1, 2]);

        match plan_compact_block_reconstruction_v1(&announcement, &known).unwrap() {
            CompactBlockReconstructionPlanV1::Complete(reconstructed) => {
                let txids = reconstructed
                    .transactions
                    .iter()
                    .map(|transaction| transaction.txid.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(reconstructed.hash, block.hash);
                assert_eq!(txids, vec!["coinbase", "tx-a", "tx-b"]);
                assert_eq!(
                    compute_merkle_root(&reconstructed.transactions),
                    reconstructed.header.merkle_root
                );
            }
            other => panic!("unexpected plan: {other:?}"),
        }
    }

    #[test]
    fn missing_transactions_use_bounded_body_on_demand_request() {
        let block = block(&["coinbase", "tx-a", "tx-b"]);
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = known(&block, &[0]);

        match plan_compact_block_reconstruction_v1(&announcement, &known).unwrap() {
            CompactBlockReconstructionPlanV1::RequestTransactions(request) => {
                assert_eq!(request.block_hash, block.hash);
                assert_eq!(request.txids, vec!["tx-a", "tx-b"]);
            }
            other => panic!("unexpected plan: {other:?}"),
        }
    }

    #[test]
    fn requested_transactions_complete_reconstruction_deterministically() {
        let block = block(&["coinbase", "tx-a", "tx-b"]);
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known_transactions = known(&block, &[0]);
        let available = known(&block, &[1, 2]);

        let request =
            match plan_compact_block_reconstruction_v1(&announcement, &known_transactions).unwrap() {
            CompactBlockReconstructionPlanV1::RequestTransactions(request) => request,
            other => panic!("unexpected plan: {other:?}"),
        };
        let response = build_compact_transaction_response_v1(&request, &available)
            .unwrap()
            .expect("all requested transactions available");

        match complete_compact_block_reconstruction_v1(
            &announcement,
            &known_transactions,
            &response,
        )
        .unwrap()
        {
            CompactBlockReconstructionPlanV1::Complete(reconstructed) => {
                assert_eq!(
                    reconstructed
                        .transactions
                        .iter()
                        .map(|transaction| transaction.txid.as_str())
                        .collect::<Vec<_>>(),
                    vec!["coinbase", "tx-a", "tx-b"]
                );
            }
            other => panic!("unexpected completion plan: {other:?}"),
        }
    }

    #[test]
    fn excessive_missing_transaction_fanout_falls_back_to_full_block() {
        let txids = (0..=P2P_WIRE_MAX_REQUEST_ITEMS_V1)
            .map(|index| format!("tx-{index}"))
            .collect::<Vec<_>>();
        let refs = txids.iter().map(String::as_str).collect::<Vec<_>>();
        let block = block(&refs);
        let announcement = build_compact_block_announcement_v1(&block).unwrap();

        match plan_compact_block_reconstruction_v1(&announcement, &HashMap::new()).unwrap() {
            CompactBlockReconstructionPlanV1::FullBlockFallback { reason, .. } => {
                assert_eq!(
                    reason,
                    CompactRelayFallbackReasonV1::MissingTransactionFanoutExceeded
                );
            }
            other => panic!("unexpected plan: {other:?}"),
        }
    }

    #[test]
    fn merkle_mismatch_falls_back_instead_of_claiming_reconstruction() {
        let block = block(&["coinbase", "tx-a"]);
        let mut announcement = build_compact_block_announcement_v1(&block).unwrap();
        announcement.header.merkle_root = "forged".into();
        let known = known(&block, &[0, 1]);

        match plan_compact_block_reconstruction_v1(&announcement, &known).unwrap() {
            CompactBlockReconstructionPlanV1::FullBlockFallback { reason, .. } => {
                assert_eq!(reason, CompactRelayFallbackReasonV1::MerkleRootMismatch);
            }
            other => panic!("unexpected plan: {other:?}"),
        }
    }

    #[test]
    fn excessive_parent_fanout_fails_closed_before_reconstruction() {
        let block = block(&["coinbase", "tx-a"]);
        let mut announcement = build_compact_block_announcement_v1(&block).unwrap();
        announcement.header.parents = (0..=GHOSTDAG_V1_MAX_PARENTS)
            .map(|index| format!("parent-{index}"))
            .collect();

        assert_eq!(
            validate_compact_block_announcement_v1(&announcement),
            Err(CompactRelayErrorV1::HeaderParentCountTooLarge {
                observed: GHOSTDAG_V1_MAX_PARENTS + 1,
                maximum: GHOSTDAG_V1_MAX_PARENTS,
            })
        );
    }

    #[test]
    fn unsupported_header_version_fails_closed_before_reconstruction() {
        let block = block(&["coinbase", "tx-a"]);
        let mut announcement = build_compact_block_announcement_v1(&block).unwrap();
        announcement.header.version = 99;

        assert_eq!(
            validate_compact_block_announcement_v1(&announcement),
            Err(CompactRelayErrorV1::UnsupportedHeaderVersion(99))
        );
    }

    #[test]
    fn duplicate_transaction_ids_fail_closed() {
        let block = block(&["coinbase", "tx-a"]);
        let mut announcement = build_compact_block_announcement_v1(&block).unwrap();
        announcement.txids.push("tx-a".into());

        assert_eq!(
            validate_compact_block_announcement_v1(&announcement),
            Err(CompactRelayErrorV1::DuplicateTransactionId("tx-a".into()))
        );
    }

    #[test]
    fn mismatched_response_transaction_fails_closed() {
        let block = block(&["coinbase", "tx-a"]);
        let announcement = build_compact_block_announcement_v1(&block).unwrap();
        let known = known(&block, &[0]);
        let response = CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: block.hash.clone(),
            transactions: vec![transaction("wrong")],
        };

        assert!(matches!(
            complete_compact_block_reconstruction_v1(&announcement, &known, &response),
            Err(CompactRelayErrorV1::ResponseTransactionMismatch { .. })
        ));
    }

    #[test]
    fn compact_inventory_is_bounded_before_reconstruction_state_is_created() {
        let txids = (0..=P2P_WIRE_MAX_INVENTORY_ITEMS_V1)
            .map(|index| format!("tx-{index}"))
            .collect::<Vec<_>>();
        let refs = txids.iter().map(String::as_str).collect::<Vec<_>>();
        let block = block(&refs);

        assert!(matches!(
            build_compact_block_announcement_v1(&block),
            Err(CompactRelayErrorV1::TransactionInventoryTooLarge {
                observed,
                maximum
            }) if observed == P2P_WIRE_MAX_INVENTORY_ITEMS_V1 + 1
                && maximum == P2P_WIRE_MAX_INVENTORY_ITEMS_V1
        ));
    }
}
