//! Dormant, versioned v3 block envelope.
//!
//! An ordinary `types::Block` has a Unix-**seconds** v1/v2 header. It must
//! never be cast, serialized, or reinterpreted as a nanosecond v3 block.
//! This standalone envelope is not accepted by mining, P2P, RPC, storage,
//! state replay or production startup. Validation here is necessary, but not
//! sufficient for consensus admission: PoW, parent selection, retarget,
//! contracts-disabled policy, monetary rewards and state transitions require
//! separate authoritative gates.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    errors::PulseError,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    reward_settlement_v3::validate_reward_claim_transaction_v3,
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    types::{compute_merkle_root, BlockId, Transaction},
};

/// A candidate nanosecond block, with its chain identity authenticated by
/// the v3 hash domain. No implicit conversion to or from `types::Block`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockEnvelopeV3 {
    pub chain_id: String,
    pub hash: BlockId,
    pub header: BlockHeaderV3,
    pub transactions: Vec<Transaction>,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(message.into())
}

/// Stateless envelope/transaction-commitment preflight only.
///
/// This does *not* make a block acceptable on any network. In particular,
/// parent existence and authority, height, timestamp drift, PoW, transaction
/// signatures, UTXO conflicts and deterministic state replay are *not*
/// established here.
pub fn validate_block_envelope_v3(
    candidate: &BlockEnvelopeV3,
    expected_chain_id: &str,
) -> Result<(), PulseError> {
    if candidate.chain_id != expected_chain_id {
        return Err(PulseError::ChainIdMismatch);
    }

    let expected_hash = compute_block_hash_v3(&candidate.header, expected_chain_id)?;
    if candidate.hash != expected_hash {
        return Err(invalid(
            "v3 envelope hash does not match chain-bound v3 header",
        ));
    }

    // Genesis in v3.0 is strictly zero-allocation. This guard does not
    // replace verification against the frozen genesis identity/timestamp.
    if candidate.header.height == 0 && !candidate.transactions.is_empty() {
        return Err(invalid(
            "v3 zero-allocation genesis cannot contain transactions",
        ));
    }

    let mut seen_txids = BTreeSet::new();
    for tx in &candidate.transactions {
        // The currently implemented v3 monetary path retains version-2
        // transactions. Do not silently accept a v1 or speculative v3 format.
        if tx.version != TRANSACTION_VERSION_V2 {
            return Err(invalid(format!(
                "v3 envelope requires transaction protocol v2, got {}",
                tx.version
            )));
        }
        if tx.txid != compute_txid_v2(tx, expected_chain_id)? {
            return Err(invalid(
                "v3 envelope transaction has invalid chain-bound v2 txid",
            ));
        }
        if !seen_txids.insert(tx.txid.as_str()) {
            return Err(invalid("v3 envelope contains duplicate transaction IDs"));
        }
    }
    if candidate.header.merkle_root != compute_merkle_root(&candidate.transactions) {
        return Err(invalid("v3 envelope transaction Merkle root mismatch"));
    }
    Ok(())
}


/**
 * Stateless monetary-v3 body guard for a native nanosecond envelope.
 *
 * This rejects missing/invalid amountless reward claims and any additional
 * inputless transaction which could otherwise become a hidden issuance path.
 * Unlike the legacy `types::Block` staging guard, it takes a genuine
 * `BlockEnvelopeV3`, NOT a seconds-based block.
 *
 * This is an INERT prerequisite, NOT monetary consensus admission. It does
 * NOT verify PoW, ancestry, K, selected-parent, retarget, ordinary Ed25519
 * signatures, UTXO availability, fees, reward settlement, state roots or
 * contract inactivity. Neither mining nor P2P calls this function.
 */
pub fn validate_dormant_native_v3_monetary_body_preflight(
    candidate: &BlockEnvelopeV3,
    expected_chain_id: &str,
) -> Result<(), PulseError> {
    validate_block_envelope_v3(candidate, expected_chain_id)?;
    if candidate.header.height == 0 {
        // The envelope validator has already enforced zero-allocation genesis.
        return Ok(());
    }
    let claim = candidate
        .transactions
        .first()
        .ok_or_else(|| invalid("native-v3 monetary block lacks its reward claim"))?;
    validate_reward_claim_transaction_v3(claim, expected_chain_id)
        .map_err(|error| invalid(format!("invalid native-v3 amountless reward claim: {error}")))?;
    if let Some(hidden) = candidate
        .transactions
        .iter()
        .skip(1)
        .find(|tx| tx.inputs.is_empty())
    {
        return Err(invalid(format!(
            "native-v3 block has additional inputless transaction {}",
            hidden.txid
        )));
    }
    Ok(())
}

/// Build a candidate envelope with the canonical v3 header hash and frozen
/// v2 transaction commitments. It is still unmined/unaccepted.
pub fn build_block_envelope_v3(
    chain_id: &str,
    header: BlockHeaderV3,
    transactions: Vec<Transaction>,
) -> Result<BlockEnvelopeV3, PulseError> {
    let hash = compute_block_hash_v3(&header, chain_id)?;
    let candidate = BlockEnvelopeV3 {
        chain_id: chain_id.to_owned(),
        hash,
        header,
        transactions,
    };
    validate_block_envelope_v3(&candidate, chain_id)?;
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        header_v2::compute_block_hash_v2,
        protocol::{BLOCK_HEADER_VERSION_V2, BLOCK_HEADER_VERSION_V3},
        types::{Block, BlockHeader},
    };

    const CHAIN: &str = "pulsedag-v3-envelope-candidate";
    const TS_NS: u64 = 1_800_000_000_500_000_000;

    fn header(height: u64, transactions: &[Transaction]) -> BlockHeaderV3 {
        BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: if height == 0 {
                vec![]
            } else {
                vec!["11".repeat(32)]
            },
            timestamp_ns: TS_NS,
            difficulty: 0x207f_ffff,
            nonce: 42,
            merkle_root: compute_merkle_root(transactions),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        }
    }

    fn transaction(nonce: u64) -> Transaction {
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![],
            outputs: vec![],
            fee: 0,
            nonce,
        };
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        tx
    }

    #[test]
    fn v3_envelope_roundtrip_commits_chain_and_nanosecond_header() {
        let txs = vec![transaction(1), transaction(2)];
        let block = build_block_envelope_v3(CHAIN, header(1, &txs), txs).unwrap();
        assert_eq!(block.header.timestamp_ns, TS_NS);
        validate_block_envelope_v3(&block, CHAIN).unwrap();

        let encoded = serde_json::to_vec(&block).unwrap();
        let decoded: BlockEnvelopeV3 = serde_json::from_slice(&encoded).unwrap();
        validate_block_envelope_v3(&decoded, CHAIN).unwrap();
        assert_eq!(decoded.hash, block.hash);

        // Old v2 envelope has `timestamp` in seconds and cannot be
        // deserialized as a v3 envelope.
        let v2 = Block {
            hash: compute_block_hash_v2(
                &BlockHeader {
                    version: BLOCK_HEADER_VERSION_V2,
                    parents: block.header.parents.clone(),
                    timestamp: TS_NS / 1_000_000_000,
                    difficulty: block.header.difficulty,
                    nonce: block.header.nonce,
                    merkle_root: block.header.merkle_root.clone(),
                    state_root: block.header.state_root.clone(),
                    blue_score: block.header.blue_score,
                    height: block.header.height,
                },
                CHAIN,
            )
            .unwrap(),
            header: BlockHeader {
                version: BLOCK_HEADER_VERSION_V2,
                parents: block.header.parents,
                timestamp: TS_NS / 1_000_000_000,
                difficulty: block.header.difficulty,
                nonce: block.header.nonce,
                merkle_root: block.header.merkle_root,
                state_root: block.header.state_root,
                blue_score: block.header.blue_score,
                height: block.header.height,
            },
            transactions: vec![],
        };
        assert!(
            serde_json::from_value::<BlockEnvelopeV3>(serde_json::to_value(v2).unwrap()).is_err()
        );
        assert!(serde_json::from_value::<Block>(serde_json::to_value(decoded).unwrap()).is_err());
    }

    #[test]
    fn envelope_deserialization_rejects_mixed_seconds_and_future_header_fields() {
        let envelope = build_block_envelope_v3(CHAIN, header(1, &[]), vec![]).unwrap();
        let canonical = serde_json::to_value(&envelope).unwrap();
        let decoded: BlockEnvelopeV3 = serde_json::from_value(canonical.clone()).unwrap();
        validate_block_envelope_v3(&decoded, CHAIN).unwrap();
        assert_eq!(decoded.hash, envelope.hash);

        // BlockEnvelopeV3 must not silently accept legacy timestamp fields
        // in its nested BlockHeaderV3 through generic JSON deserialization.
        let mut mixed = canonical.clone();
        mixed["header"]["timestamp"] = serde_json::json!(TS_NS / 1_000_000_000);
        assert!(serde_json::from_value::<BlockEnvelopeV3>(mixed).is_err());

        let mut future = canonical;
        future["header"]["future_consensus_field"] = serde_json::json!(true);
        assert!(serde_json::from_value::<BlockEnvelopeV3>(future).is_err());
    }

    #[test]
    fn wrong_chain_forged_hash_and_tampered_merkle_fail() {
        let txs = vec![transaction(7)];
        let mut block = build_block_envelope_v3(CHAIN, header(1, &txs), txs).unwrap();
        assert!(matches!(
            validate_block_envelope_v3(&block, "another-network"),
            Err(PulseError::ChainIdMismatch)
        ));
        block.hash = "aa".repeat(32);
        assert!(validate_block_envelope_v3(&block, CHAIN).is_err());
        block.hash = compute_block_hash_v3(&block.header, CHAIN).unwrap();
        block.header.merkle_root = "22".repeat(32);
        block.hash = compute_block_hash_v3(&block.header, CHAIN).unwrap();
        assert!(validate_block_envelope_v3(&block, CHAIN).is_err());
    }

    #[test]
    fn duplicate_txid_wrong_txid_and_legacy_tx_protocol_fail() {
        let tx = transaction(9);
        let mut txs = vec![tx.clone(), tx];
        let mut block = BlockEnvelopeV3 {
            chain_id: CHAIN.to_owned(),
            hash: String::new(),
            header: header(1, &txs),
            transactions: txs.clone(),
        };
        block.hash = compute_block_hash_v3(&block.header, CHAIN).unwrap();
        assert!(validate_block_envelope_v3(&block, CHAIN).is_err());

        txs.pop();
        txs[0].txid = "00".repeat(32);
        block.transactions = txs.clone();
        block.header = header(1, &txs);
        block.hash = compute_block_hash_v3(&block.header, CHAIN).unwrap();
        assert!(validate_block_envelope_v3(&block, CHAIN).is_err());

        txs[0].version = 1;
        block.transactions = txs.clone();
        block.header = header(1, &txs);
        block.hash = compute_block_hash_v3(&block.header, CHAIN).unwrap();
        assert!(validate_block_envelope_v3(&block, CHAIN).is_err());
    }

    #[test]
    fn dormant_native_monetary_body_preflight_refuses_missing_or_hidden_issuance() {
        use crate::reward_settlement_v3::build_reward_claim_transaction_v3;

        let claim = build_reward_claim_transaction_v3("pulse1native-miner", 10, CHAIN).unwrap();
        let body = |transactions: Vec<Transaction>| {
            build_block_envelope_v3(CHAIN, header(1, &transactions), transactions).unwrap()
        };
        let valid = body(vec![claim.clone()]);
        validate_dormant_native_v3_monetary_body_preflight(&valid, CHAIN).unwrap();
        assert!(
            validate_dormant_native_v3_monetary_body_preflight(&valid, "foreign-chain").is_err()
        );

        let missing_claim = body(vec![]);
        let error = validate_dormant_native_v3_monetary_body_preflight(&missing_claim, CHAIN)
            .unwrap_err()
            .to_string();
        assert!(error.contains("lacks its reward claim"), "{error}");

        let double_claim = body(vec![
            claim.clone(),
            build_reward_claim_transaction_v3("pulse1hidden-miner", 11, CHAIN).unwrap(),
        ]);
        let error = validate_dormant_native_v3_monetary_body_preflight(&double_claim, CHAIN)
            .unwrap_err()
            .to_string();
        assert!(error.contains("additional inputless transaction"), "{error}");

        let mut minted = claim.clone();
        minted.outputs[0].amount = 999;
        minted.txid = compute_txid_v2(&minted, CHAIN).unwrap();
        let invalid_coinbase = body(vec![minted]);
        let error = validate_dormant_native_v3_monetary_body_preflight(&invalid_coinbase, CHAIN)
            .unwrap_err()
            .to_string();
        assert!(error.contains("reward claim"), "{error}");

        // A different inputless shape is not a legitimate ordinary spend.
        let hidden_no_input = body(vec![claim.clone(), transaction(121)]);
        assert!(validate_dormant_native_v3_monetary_body_preflight(&hidden_no_input, CHAIN)
            .unwrap_err()
            .to_string()
            .contains("additional inputless transaction"));

        // Explicitly independent of PoW or finality: genesis can be empty,
        // but no positive-height block can skip its amountless reward claim.
        let genesis = build_block_envelope_v3(CHAIN, header(0, &[]), vec![]).unwrap();
        validate_dormant_native_v3_monetary_body_preflight(&genesis, CHAIN).unwrap();
    }

    #[test]
    fn dormant_native_monetary_body_preflight_is_not_signed_utxo_admission() {
        use crate::{
            reward_settlement_v3::build_reward_claim_transaction_v3,
            types::{OutPoint, TxInput, TxOutput},
        };

        let claim = build_reward_claim_transaction_v3("pulse1native-miner", 12, CHAIN).unwrap();
        // Intentionally invalid signature and nonexistent UTXO: the stateless
        // shape/issuance preflight CANNOT authenticate this spend. A separate
        // native-v3 monetary replay MUST reject it before any live admission.
        let mut unverified = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: OutPoint {
                    txid: "ff".repeat(32),
                    index: 0,
                },
                public_key: "aa".repeat(32),
                signature: "bb".repeat(64),
            }],
            outputs: vec![TxOutput {
                address: "pulse1unverified".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 21,
        };
        unverified.txid = compute_txid_v2(&unverified, CHAIN).unwrap();
        let transactions = vec![claim, unverified];
        let envelope = build_block_envelope_v3(
            CHAIN,
            header(1, &transactions),
            transactions,
        )
        .unwrap();
        validate_dormant_native_v3_monetary_body_preflight(&envelope, CHAIN).unwrap();
        assert_eq!(envelope.transactions.len(), 2);

        // Malformed commitment must fail even if the reward claim is valid.
        let mut bad_merkle = envelope;
        bad_merkle.header.merkle_root = "00".repeat(32);
        bad_merkle.hash = compute_block_hash_v3(&bad_merkle.header, CHAIN).unwrap();
        assert!(validate_dormant_native_v3_monetary_body_preflight(&bad_merkle, CHAIN).is_err());
    }

    #[test]
    fn only_empty_zero_allocation_genesis_is_structurally_allowed() {
        let genesis = build_block_envelope_v3(CHAIN, header(0, &[]), vec![]).unwrap();
        assert!(genesis.header.parents.is_empty());
        assert!(genesis.transactions.is_empty());

        let txs = vec![transaction(1)];
        let envelope = BlockEnvelopeV3 {
            chain_id: CHAIN.to_owned(),
            hash: compute_block_hash_v3(&header(0, &txs), CHAIN).unwrap(),
            header: header(0, &txs),
            transactions: txs,
        };
        assert!(validate_block_envelope_v3(&envelope, CHAIN).is_err());
    }
}
