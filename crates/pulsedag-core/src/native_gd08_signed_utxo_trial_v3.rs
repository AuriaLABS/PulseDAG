//! A fail-closed, **non-authoritative** native-v3 GD-08 signed-UTXO prefix.
//!
//! Consumes complete genuine BlockEnvelopeV3 bodies ordered by authenticated
//! native-v3 GHOSTDAG trial (#1450). Verifies real chain-bound Ed25519 spending,
//! exact input = output + fee, duplicate outpoints, and conflict-loser fee
//! exclusion without legacy types::Block or OrderedDagV2 ordering.
//!
//! IMPORTANT: the supplied opening UTXOs are NOT independently authenticated.
//! This code DOES NOT mint/mature native rewards, verify committed state roots,
//! freeze K/blue-red monetary eligibility, prove checkpoint trust, or authorize
//! block admission, mining, P2P, startup or contracts. Never use this result as
//! monetary authority until #1309/#1410/#1367 and #781 are independently closed.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::{
    block_envelope_v3::{validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3},
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_gd08_order_kernel_v3::derive_dormant_native_gd08_order_from_ghostdag_trial,
    native_parent_dag_context_v3::MAX_DORMANT_V3_DAG_HEADERS,
    state::UtxoState,
    tx::{address_from_public_key, compute_txid_v2, signing_message_v2, TRANSACTION_VERSION_V2},
    types::{BlockId, OutPoint, Transaction, Utxo},
};

// Strictly local test/diagnostic memory budgets; not network consensus values.
const MAX_TRIAL_ORDINARY_TRANSACTIONS: usize = 2_048;
const MAX_TRIAL_TOTAL_INPUTS: usize = 8_192;
const MAX_TRIAL_TOTAL_OUTPUTS: usize = 8_192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeGd08SignedPrefixV3 {
    pub order_digest: String,
    pub ordered_blocks: Vec<BlockId>,
    pub applied_txids: Vec<String>,
    pub skipped_conflict_txids: Vec<String>,
    /// Fees earned solely by transfers that actually changed the UTXO set.
    /// NOT an authorized reward or coinbase settlement.
    pub eligible_fees_by_block: Vec<(BlockId, u64)>,
    pub resulting_state_root: String,
}

fn reject(reason: impl Into<String>) -> PulseError {
    PulseError::InvalidTransaction(format!(
        "UNAPPROVED native GD-08 signed-UTXO prefix: {}",
        reason.into()
    ))
}

/// Independently verify signatures and full monetary conservation against
/// input UTXOs, even for transactions that later lose a canonical conflict.
/// Never classify a forged or phantom transfer as a harmless conflict loser.
fn verify_full_signed_spend(
    tx: &Transaction,
    chain_id: &str,
    view: &BTreeMap<OutPoint, Utxo>,
) -> Result<(), PulseError> {
    if tx.version != TRANSACTION_VERSION_V2 || tx.inputs.is_empty() || tx.outputs.is_empty() {
        return Err(reject(
            "unsupported transfer version or empty inputs/outputs",
        ));
    }
    if tx.outputs.iter().any(|output| output.amount == 0) {
        return Err(reject("zero-value ordinary output"));
    }
    if compute_txid_v2(tx, chain_id)? != tx.txid {
        return Err(PulseError::InvalidTxid);
    }

    let mut seen_inputs = BTreeSet::new();
    let message = signing_message_v2(tx, chain_id)?;
    let mut input_sum = 0_u128;
    for input in &tx.inputs {
        if !seen_inputs.insert(input.previous_output.clone()) {
            return Err(reject("duplicate input in signed transfer"));
        }
        let referenced = view
            .get(&input.previous_output)
            .ok_or(PulseError::UtxoNotFound)?;
        if address_from_public_key(&input.public_key) != referenced.address {
            return Err(PulseError::InvalidSignature);
        }
        let key_bytes = hex::decode(&input.public_key).map_err(|_| PulseError::InvalidSignature)?;
        let signature_bytes =
            hex::decode(&input.signature).map_err(|_| PulseError::InvalidSignature)?;
        let key: [u8; 32] = key_bytes
            .try_into()
            .map_err(|_| PulseError::InvalidSignature)?;
        let sig: [u8; 64] = signature_bytes
            .try_into()
            .map_err(|_| PulseError::InvalidSignature)?;
        let verifier = VerifyingKey::from_bytes(&key).map_err(|_| PulseError::InvalidSignature)?;
        verifier
            .verify(&message, &Signature::from_bytes(&sig))
            .map_err(|_| PulseError::InvalidSignature)?;
        input_sum = input_sum
            .checked_add(u128::from(referenced.amount))
            .ok_or_else(|| reject("input amount overflow"))?;
    }
    let output_sum = tx.outputs.iter().try_fold(0_u128, |sum, output| {
        sum.checked_add(u128::from(output.amount))
            .ok_or_else(|| reject("output amount overflow"))
    })?;
    let required = output_sum
        .checked_add(u128::from(tx.fee))
        .ok_or_else(|| reject("fee amount overflow"))?;
    if input_sum != required {
        return Err(reject(
            "signed input value must equal outputs plus explicit fee",
        ));
    }
    Ok(())
}

/// Evaluate a *hypothetical* UTXO prefix from a caller-provided opening view.
/// The result is non-authoritative until opening state, reward maturity, finality,
/// colors and exact state-root commitments derive from frozen native consensus.
///
/// A missing input is skipped only if a *previously accepted* spend in this
/// exact order consumed it. Its real signature and conservation must still
/// verify against archived spend provenance. Unknown input provenance fails.
pub fn inspect_dormant_native_gd08_signed_prefix(
    chain_id: &str,
    frozen_genesis_ns: u64,
    frozen_genesis_hash: &str,
    tip: &BlockId,
    envelopes: &BTreeMap<BlockId, BlockEnvelopeV3>,
    trial_k: usize,
    opening_utxo: &UtxoState,
) -> Result<DormantNativeGd08SignedPrefixV3, PulseError> {
    if chain_id.is_empty() || envelopes.is_empty() || envelopes.len() > MAX_DORMANT_V3_DAG_HEADERS {
        return Err(reject("invalid chain identity or bounded envelope count"));
    }
    opening_utxo.validate_deterministic_indexes()?;
    let mut headers: BTreeMap<BlockId, BlockHeaderV3> = BTreeMap::new();
    let mut total_txs = 0usize;
    let mut total_inputs = 0usize;
    let mut total_outputs = 0usize;

    for (hash, block) in envelopes {
        if hash != &block.hash {
            return Err(reject("body map key does not match native header hash"));
        }
        validate_dormant_native_v3_monetary_body_preflight(block, chain_id)?;
        total_txs = total_txs
            .checked_add(block.transactions.len().saturating_sub(1))
            .ok_or_else(|| reject("transaction budget overflow"))?;
        for tx in block.transactions.iter().skip(1) {
            total_inputs = total_inputs
                .checked_add(tx.inputs.len())
                .ok_or_else(|| reject("input budget overflow"))?;
            total_outputs = total_outputs
                .checked_add(tx.outputs.len())
                .ok_or_else(|| reject("output budget overflow"))?;
        }
        if total_txs > MAX_TRIAL_ORDINARY_TRANSACTIONS
            || total_inputs > MAX_TRIAL_TOTAL_INPUTS
            || total_outputs > MAX_TRIAL_TOTAL_OUTPUTS
        {
            return Err(reject(
                "trial transaction/input/output resource budget exceeded",
            ));
        }
        headers.insert(hash.clone(), block.header.clone());
    }

    let order = derive_dormant_native_gd08_order_from_ghostdag_trial(
        chain_id,
        frozen_genesis_ns,
        frozen_genesis_hash,
        tip,
        &headers,
        trial_k,
    )?;
    if order.ordered.len() != envelopes.len() {
        return Err(reject(
            "unrelated bodies or incomplete authenticated native DAG",
        ));
    }
    if !envelopes
        .get(frozen_genesis_hash)
        .is_some_and(|genesis| genesis.transactions.is_empty())
    {
        return Err(reject("genesis must contain zero transactions"));
    }

    let mut current = opening_utxo.clone();
    let mut consumed: BTreeMap<OutPoint, Utxo> = BTreeMap::new();
    let mut created_history: BTreeSet<OutPoint> = current.utxos.keys().cloned().collect();
    let mut applied_txids = Vec::new();
    let mut skipped_conflict_txids = Vec::new();
    let mut eligible_fees_by_block = Vec::new();

    for hash in order.ordered.iter().skip(1) {
        let envelope = envelopes
            .get(hash)
            .ok_or_else(|| reject("ordered body missing"))?;
        let mut block_fees = 0_u64;
        for tx in envelope.transactions.iter().skip(1) {
            // Assemble only referenced UTXOs. This is also the historical
            // provenance for previously consumed, but correctly signed, inputs.
            let mut input_view = BTreeMap::new();
            let mut conflict = false;
            for input in &tx.inputs {
                if let Some(coin) = current.utxos.get(&input.previous_output) {
                    input_view.insert(input.previous_output.clone(), coin.clone());
                } else if let Some(coin) = consumed.get(&input.previous_output) {
                    conflict = true;
                    input_view.insert(input.previous_output.clone(), coin.clone());
                } else {
                    return Err(reject(
                        "signed transfer references unauthenticated outpoint",
                    ));
                }
            }
            verify_full_signed_spend(tx, chain_id, &input_view)?;
            if conflict {
                skipped_conflict_txids.push(tx.txid.clone());
                continue;
            }

            let new_outputs = tx
                .outputs
                .iter()
                .enumerate()
                .map(|(index, output)| {
                    let index =
                        u32::try_from(index).map_err(|_| reject("output index overflow"))?;
                    let outpoint = OutPoint {
                        txid: tx.txid.clone(),
                        index,
                    };
                    if created_history.contains(&outpoint) {
                        return Err(reject(
                            "duplicate created outpoint (including spent history)",
                        ));
                    }
                    Ok((outpoint, output))
                })
                .collect::<Result<Vec<_>, PulseError>>()?;
            block_fees = block_fees
                .checked_add(tx.fee)
                .ok_or_else(|| reject("accepted transfer fee accumulation overflow"))?;

            // All fallible checks are complete: mutate the local prefix only.
            for input in &tx.inputs {
                let coin = current
                    .utxos
                    .remove(&input.previous_output)
                    .ok_or_else(|| reject("validated input disappeared during local apply"))?;
                if let Some(index) = current.address_index.get_mut(&coin.address) {
                    index.retain(|outpoint| outpoint != &coin.outpoint);
                }
                consumed.insert(input.previous_output.clone(), coin);
            }
            for (outpoint, output) in new_outputs {
                created_history.insert(outpoint.clone());
                current.utxos.insert(
                    outpoint.clone(),
                    Utxo {
                        outpoint: outpoint.clone(),
                        address: output.address.clone(),
                        amount: output.amount,
                        coinbase: false,
                        height: envelope.header.height,
                    },
                );
                current
                    .address_index
                    .entry(output.address.clone())
                    .or_default()
                    .push(outpoint);
            }
            applied_txids.push(tx.txid.clone());
        }
        eligible_fees_by_block.push((hash.clone(), block_fees));
    }
    let resulting_state_root = current.compute_state_root()?;
    Ok(DormantNativeGd08SignedPrefixV3 {
        order_digest: order.digest,
        ordered_blocks: order.ordered,
        applied_txids,
        skipped_conflict_txids,
        eligible_fees_by_block,
        resulting_state_root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        tx::{address_from_public_key, compute_txid_v2, signing_message_v2},
        types::{compute_merkle_root, TxInput, TxOutput},
    };
    use ed25519_dalek::{Signer, SigningKey};

    const CHAIN: &str = "pulsedag-dormant-native-gd08-signed-prefix";
    const TS: u64 = 1_800_000_100_000_000_000;

    fn initial() -> (UtxoState, SigningKey, OutPoint) {
        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let pk = hex::encode(key.verifying_key().to_bytes());
        let source = OutPoint {
            txid: "funding-test-only".into(),
            index: 0,
        };
        let coin = Utxo {
            outpoint: source.clone(),
            address: address_from_public_key(&pk),
            amount: 10,
            coinbase: false,
            height: 1,
        };
        let mut utxo = UtxoState::default();
        utxo.address_index
            .insert(coin.address.clone(), vec![source.clone()]);
        utxo.utxos.insert(source.clone(), coin);
        (utxo, key, source)
    }

    fn spend(
        key: &SigningKey,
        source: &OutPoint,
        amount: u64,
        fee: u64,
        nonce: u64,
    ) -> Transaction {
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: source.clone(),
                public_key: hex::encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulsedag-trial-recipient".to_string(),
                amount,
            }],
            fee,
            nonce,
        };
        let msg = signing_message_v2(&tx, CHAIN).unwrap();
        tx.inputs[0].signature = hex::encode(key.sign(&msg).to_bytes());
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        tx
    }

    fn fixture(
        transfers: Vec<Transaction>,
    ) -> (BlockId, BlockId, BTreeMap<BlockId, BlockEnvelopeV3>) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, TS).unwrap();
        let g = genesis.hash.clone();
        let claim = build_reward_claim_transaction_v3("miner", 1, CHAIN).unwrap();
        let mut transactions = vec![claim];
        transactions.extend(transfers);
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![g.clone()],
            timestamp_ns: TS + 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "33".repeat(32),
            blue_score: 1,
            height: 1,
        };
        for nonce in 0..4096_u64 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                break;
            }
        }
        let child = build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        let tip = child.hash.clone();
        (
            g.clone(),
            tip.clone(),
            BTreeMap::from([(g, genesis), (tip, child)]),
        )
    }

    #[test]
    fn authentic_signed_transfer_applies_exact_fee_and_deterministic_state_root() {
        let (opening, key, source) = initial();
        let (genesis, tip, bodies) = fixture(vec![spend(&key, &source, 9, 1, 1)]);
        let result = inspect_dormant_native_gd08_signed_prefix(
            CHAIN, TS, &genesis, &tip, &bodies, 0, &opening,
        )
        .unwrap();
        assert_eq!(result.applied_txids.len(), 1);
        assert!(result.skipped_conflict_txids.is_empty());
        assert_eq!(result.eligible_fees_by_block, vec![(tip, 1)]);
        assert_ne!(
            result.resulting_state_root,
            opening.compute_state_root().unwrap()
        );
    }

    #[test]
    fn conflict_loser_does_not_contribute_fees() {
        let (opening, key, source) = initial();
        let a = spend(&key, &source, 9, 1, 1);
        let b = spend(&key, &source, 8, 2, 2);
        let (genesis, tip, bodies) = fixture(vec![a, b]);
        let result = inspect_dormant_native_gd08_signed_prefix(
            CHAIN, TS, &genesis, &tip, &bodies, 0, &opening,
        )
        .unwrap();
        assert_eq!(result.applied_txids.len(), 1);
        assert_eq!(result.skipped_conflict_txids.len(), 1);
        assert_eq!(result.eligible_fees_by_block, vec![(tip, 1)]);
    }

    #[test]
    fn forged_signature_and_nonconservation_fail_closed() {
        let (opening, key, source) = initial();
        let mut forged = spend(&key, &source, 9, 1, 1);
        forged.inputs[0].signature = "00".repeat(64);
        forged.txid = compute_txid_v2(&forged, CHAIN).unwrap();
        let (genesis, tip, bodies) = fixture(vec![forged]);
        assert!(inspect_dormant_native_gd08_signed_prefix(
            CHAIN, TS, &genesis, &tip, &bodies, 0, &opening,
        )
        .is_err());
        let bad_amount = spend(&key, &source, 9, 2, 3);
        let (genesis, tip, bodies) = fixture(vec![bad_amount]);
        assert!(inspect_dormant_native_gd08_signed_prefix(
            CHAIN, TS, &genesis, &tip, &bodies, 0, &opening,
        )
        .is_err());
    }

    #[test]
    fn missing_source_provenance_and_wrong_chain_fail_closed() {
        let (opening, key, source) = initial();
        let (genesis, tip, bodies) = fixture(vec![spend(&key, &source, 9, 1, 1)]);
        assert!(inspect_dormant_native_gd08_signed_prefix(
            CHAIN,
            TS,
            &genesis,
            &tip,
            &bodies,
            0,
            &UtxoState::default(),
        )
        .is_err());
        assert!(inspect_dormant_native_gd08_signed_prefix(
            "other-chain",
            TS,
            &genesis,
            &tip,
            &bodies,
            0,
            &opening,
        )
        .is_err());
    }
}
