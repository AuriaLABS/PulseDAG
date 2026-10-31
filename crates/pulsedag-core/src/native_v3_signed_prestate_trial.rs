//! DORMANT policy-neutral native-v3 envelope / signed-v2 UTXO prestate trial.
//!
//! This checks *only* ordinary signed transfers inside ONE native nanosecond
//! envelope against an explicitly supplied, already-trusted test prestate.
//! It does NOT authenticate the prestate, parent DAG, K/blue-work, PoW,
//! canonical monetary order, reward finality, coinbase maturity, or the
//! committed block state root. The zero-valued claim is NOT applied as money.
//! There are no production mining, P2P, startup or storage consumers.
//!
//! This is not a monetary-v3 block admission function and cannot be used to
//! authorize mining or final settlement. Contract execution stays inactive.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    apply::apply_transaction,
    block_envelope_v3::{validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3},
    errors::PulseError,
    state::ChainState,
    validation_v2::validate_transaction_v2,
};

fn invalid(why: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "DORMANT native-v3 signed-prestate trial (NOT consensus): {}",
        why.into()
    ))
}

/// Diagnostic result, not an authoritative committed state or fee entitlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeV3SignedPrestateResult {
    pub signed_transactions: usize,
    pub candidate_fees: u64,
    pub resulting_utxo_root: String,
}

/// One isolated native envelope's signed transfer effects. Only called on a
/// disposable state clone: errors may leave that clone partly updated.
fn apply_one_explicit_native_envelope(
    envelope: &BlockEnvelopeV3,
    working: &mut ChainState,
    seen_txids: &mut BTreeSet<String>,
) -> Result<(usize, u64), PulseError> {
    validate_dormant_native_v3_monetary_body_preflight(envelope, &working.chain_id)?;
    if envelope.header.height == 0 {
        return Err(invalid(
            "genesis is not a signed-spend monetary trial block",
        ));
    }
    if working.contracts.config.enabled {
        return Err(invalid("smart contracts must remain INACTIVE"));
    }

    let mut candidate_fees = 0_u64;
    let mut signed_transactions = 0_usize;
    for tx in envelope.transactions.iter().skip(1) {
        if !seen_txids.insert(tx.txid.clone()) {
            return Err(invalid(
                "repeated ordinary transaction ID across explicit order",
            ));
        }
        if u32::try_from(tx.outputs.len()).is_err() {
            return Err(invalid("transaction output indices exceed u32"));
        }

        // V2's general validator can consider unconfirmed mempool outputs.
        // This local-only native trial requires actual settled UTXOs.
        let mut inputs_seen = BTreeSet::new();
        let mut total_inputs = 0_u64;
        for input in &tx.inputs {
            if !inputs_seen.insert(input.previous_output.clone()) {
                return Err(invalid("duplicate transaction input"));
            }
            let coin = working
                .utxo
                .utxos
                .get(&input.previous_output)
                .ok_or(PulseError::UtxoNotFound)?;
            if coin.coinbase {
                return Err(invalid(
                    "coinbase input requires unimplemented native finality/maturity",
                ));
            }
            total_inputs = total_inputs
                .checked_add(coin.amount)
                .ok_or_else(|| invalid("input sum overflow"))?;
        }
        validate_transaction_v2(tx, working, &envelope.chain_id)?;
        let total_outputs = tx.outputs.iter().try_fold(0_u64, |sum, output| {
            sum.checked_add(output.amount)
                .ok_or_else(|| invalid("output sum overflow"))
        })?;
        let required = total_outputs
            .checked_add(tx.fee)
            .ok_or_else(|| invalid("output + fee overflow"))?;
        if total_inputs != required {
            return Err(invalid("inputs must equal outputs plus explicit fee"));
        }

        // The private state clone consumes UTXOs in the caller-supplied order;
        // no duplicate/conflicting transfer can claim a second fee.
        apply_transaction(tx, working, envelope.header.height)?;
        candidate_fees = candidate_fees
            .checked_add(tx.fee)
            .ok_or_else(|| invalid("candidate fee sum overflow"))?;
        signed_transactions += 1;
    }
    Ok((signed_transactions, candidate_fees))
}

/// Validate signed v2-wire transfers in one native-v3 envelope, using a
/// caller-supplied trusted prestate. The result is NOT a consensus state root.
pub fn inspect_dormant_native_v3_signed_prestate_trial(
    envelope: &BlockEnvelopeV3,
    prestate: &ChainState,
) -> Result<DormantNativeV3SignedPrestateResult, PulseError> {
    let mut working = prestate.clone();
    let (signed_transactions, candidate_fees) =
        apply_one_explicit_native_envelope(envelope, &mut working, &mut BTreeSet::new())?;
    Ok(DormantNativeV3SignedPrestateResult {
        signed_transactions,
        candidate_fees,
        resulting_utxo_root: working.utxo.compute_state_root()?,
    })
}

/// A bounded *conditional* replay of a caller-provided sequence. No DAG
/// ancestry or selected-parent authority is implied by the input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantNativeV3ExplicitOrderResult {
    pub signed_transactions: usize,
    pub candidate_fees: u64,
    pub fees_by_block: BTreeMap<String, u64>,
    pub resulting_utxo_root: String,
}

// Resource cap for a disposable local diagnostic, NOT a network consensus
// parent/merge-set budget or an adopted GHOSTDAG policy parameter.
const MAX_EXPLICIT_NATIVE_TRIAL_BLOCKS: usize = 32;

/// Compare hypotheses about ordering without accepting either as consensus.
///
/// Each claimed block must have a distinct native-v3 envelope hash and a
/// valid amountless claim, and every ordinary transaction must have a unique
/// chain-bound txid across the supplied sequence. Repeated spends fail the
/// *whole* trial; they are NOT silently skipped nor paid fees. Incoming
/// blocks, order and prestate are NOT authenticated as a DAG or checkpoint.
/// The resulting root is intentionally NOT checked against any header root.
/// The supplied prestate is immutable even when a later block fails.
pub fn inspect_dormant_native_v3_explicit_order_trial(
    ordered_envelopes: &[BlockEnvelopeV3],
    prestate: &ChainState,
) -> Result<DormantNativeV3ExplicitOrderResult, PulseError> {
    if ordered_envelopes.is_empty() || ordered_envelopes.len() > MAX_EXPLICIT_NATIVE_TRIAL_BLOCKS {
        return Err(invalid(
            "explicit order exceeds bounded non-consensus diagnostic",
        ));
    }
    if prestate.contracts.config.enabled {
        return Err(invalid("smart contracts must remain INACTIVE"));
    }
    let mut working = prestate.clone();
    let mut seen_blocks = BTreeSet::new();
    let mut seen_txids = BTreeSet::new();
    let mut fees_by_block = BTreeMap::new();
    let mut signed_transactions = 0_usize;
    let mut candidate_fees = 0_u64;
    for block in ordered_envelopes {
        if !seen_blocks.insert(block.hash.clone()) {
            return Err(invalid("duplicate native-v3 envelope in explicit order"));
        }
        let (count, fees) =
            apply_one_explicit_native_envelope(block, &mut working, &mut seen_txids)?;
        signed_transactions = signed_transactions
            .checked_add(count)
            .ok_or_else(|| invalid("signed transaction count overflow"))?;
        candidate_fees = candidate_fees
            .checked_add(fees)
            .ok_or_else(|| invalid("candidate aggregate fees overflow"))?;
        if fees_by_block.insert(block.hash.clone(), fees).is_some() {
            return Err(invalid("repeated containing block fee attribution"));
        }
    }
    Ok(DormantNativeV3ExplicitOrderResult {
        signed_transactions,
        candidate_fees,
        fees_by_block,
        resulting_utxo_root: working.utxo.compute_state_root()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use ed25519_dalek::{Signer, SigningKey};

    use crate::{
        address_from_public_key,
        block_envelope_v3::build_block_envelope_v3,
        genesis_v3::init_chain_state_v3,
        header_v3::BlockHeaderV3,
        protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        signing_message_v2,
        tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
        types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    };

    const CHAIN: &str = "native-v3-dormant-signed-prestate-trial";

    fn signer() -> SigningKey {
        SigningKey::from_bytes(&[31_u8; 32])
    }

    fn address() -> String {
        address_from_public_key(&hex::encode(signer().verifying_key().to_bytes()))
    }

    fn funded_state() -> (ChainState, OutPoint) {
        let mut state = init_chain_state_v3(CHAIN.into(), 1_800_000_000).unwrap();
        assert!(!state.contracts.config.enabled);
        let source = OutPoint {
            txid: "test-only-trusted-funding".into(),
            index: 0,
        };
        let utxo = Utxo {
            outpoint: source.clone(),
            address: address(),
            amount: 100,
            coinbase: false,
            height: 1,
        };
        state.utxo.utxos.insert(source.clone(), utxo);
        state
            .utxo
            .address_index
            .entry(address())
            .or_default()
            .push(source.clone());
        (state, source)
    }

    fn signed(
        source: OutPoint,
        recipient: String,
        amount: u64,
        fee: u64,
        nonce: u64,
    ) -> Transaction {
        let key = signer();
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: source,
                public_key: hex::encode(key.verifying_key().to_bytes()),
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: recipient,
                amount,
            }],
            fee,
            nonce,
        };
        resign(&mut tx);
        tx
    }

    fn resign(tx: &mut Transaction) {
        tx.inputs[0].signature = hex::encode(
            signer()
                .sign(&signing_message_v2(tx, CHAIN).unwrap())
                .to_bytes(),
        );
        tx.txid = compute_txid_v2(tx, CHAIN).unwrap();
    }

    fn native_envelope(transfers: Vec<Transaction>) -> BlockEnvelopeV3 {
        let mut txs = vec![build_reward_claim_transaction_v3("pulse1miner", 17, CHAIN).unwrap()];
        txs.extend(transfers);
        let header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec!["11".repeat(32)],
            timestamp_ns: 1_800_000_000_500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: "33".repeat(32), // deliberately NOT the trial root
            blue_score: 1,
            height: 1,
        };
        // Header deliberately is not mined: prestate preview is NOT PoW
        // verification or a live block acceptance/settlement oracle.
        build_block_envelope_v3(CHAIN, header, txs).unwrap()
    }

    #[test]
    fn two_native_envelopes_apply_signed_chain_and_attribute_fees_once() {
        let (state, funding) = funded_state();
        let first = signed(funding.clone(), address(), 90, 10, 1);
        let second = signed(
            OutPoint {
                txid: first.txid.clone(),
                index: 0,
            },
            "pulse1recipient".into(),
            80,
            10,
            2,
        );
        let a = native_envelope(vec![first.clone()]);
        let mut b = native_envelope(vec![second.clone()]);
        b.header.height = 2;
        b.header.timestamp_ns += 500_000_000;
        b.hash = crate::header_v3::compute_block_hash_v3(&b.header, CHAIN).unwrap();
        let root_before = state.utxo.compute_state_root().unwrap();
        let result =
            inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), b.clone()], &state)
                .unwrap();
        assert_eq!(result.signed_transactions, 2);
        assert_eq!(result.candidate_fees, 20);
        assert_eq!(result.fees_by_block.get(&a.hash), Some(&10));
        assert_eq!(result.fees_by_block.get(&b.hash), Some(&10));
        let mut expected = state.clone();
        apply_transaction(&first, &mut expected, 1).unwrap();
        apply_transaction(&second, &mut expected, 2).unwrap();
        assert_eq!(
            result.resulting_utxo_root,
            expected.utxo.compute_state_root().unwrap()
        );
        assert_eq!(state.utxo.compute_state_root().unwrap(), root_before);
        // This is an explicitly supplied order, NOT inferred consensus.
        assert!(inspect_dormant_native_v3_explicit_order_trial(&[b, a], &state).is_err());
    }

    #[test]
    fn explicit_order_rejects_same_txid_across_distinct_native_envelopes() {
        let (state, funding) = funded_state();
        let tx = signed(funding, address(), 90, 10, 1);
        let first = native_envelope(vec![tx.clone()]);
        let mut second = native_envelope(vec![tx]);
        second.header.height = 2;
        second.hash = crate::header_v3::compute_block_hash_v3(&second.header, CHAIN).unwrap();
        let root_before = state.utxo.compute_state_root().unwrap();
        let error = inspect_dormant_native_v3_explicit_order_trial(&[first, second], &state)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("repeated ordinary transaction ID"),
            "{error}"
        );
        assert_eq!(state.utxo.compute_state_root().unwrap(), root_before);
    }

    #[test]
    fn explicit_order_rejects_distinct_double_spend_and_duplicate_envelope() {
        let (state, funding) = funded_state();
        let a = native_envelope(vec![signed(funding.clone(), address(), 90, 10, 1)]);
        let mut b = native_envelope(vec![signed(funding, address(), 80, 20, 2)]);
        b.header.height = 2;
        b.hash = crate::header_v3::compute_block_hash_v3(&b.header, CHAIN).unwrap();
        assert!(inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), b], &state).is_err());
        let repeated = inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), a], &state)
            .unwrap_err()
            .to_string();
        assert!(
            repeated.contains("duplicate native-v3 envelope"),
            "{repeated}"
        );
        assert!(inspect_dormant_native_v3_explicit_order_trial(&[], &state).is_err());
    }

    #[test]
    fn explicit_order_rejects_forged_later_block_without_mutating_prestate() {
        let (state, funding) = funded_state();
        let first = signed(funding, address(), 90, 10, 1);
        let mut forged = signed(
            OutPoint {
                txid: first.txid.clone(),
                index: 0,
            },
            "pulse1recipient".into(),
            80,
            10,
            2,
        );
        forged.inputs[0].signature = "00".repeat(64);
        forged.txid = compute_txid_v2(&forged, CHAIN).unwrap();
        let a = native_envelope(vec![first]);
        let mut b = native_envelope(vec![forged]);
        b.header.height = 2;
        b.hash = crate::header_v3::compute_block_hash_v3(&b.header, CHAIN).unwrap();
        let original_root = state.utxo.compute_state_root().unwrap();
        assert!(inspect_dormant_native_v3_explicit_order_trial(&[a, b], &state).is_err());
        assert_eq!(state.utxo.compute_state_root().unwrap(), original_root);
    }

    #[test]
    fn two_chain_bound_signed_spends_conserve_exact_fees_and_utxo_root() {
        let (state, funding) = funded_state();
        let root_before = state.utxo.compute_state_root().unwrap();
        let first = signed(funding.clone(), address(), 90, 10, 1);
        let second = signed(
            OutPoint {
                txid: first.txid.clone(),
                index: 0,
            },
            "pulse1recipient".into(),
            80,
            10,
            2,
        );
        let envelope = native_envelope(vec![first.clone(), second.clone()]);
        let trial = inspect_dormant_native_v3_signed_prestate_trial(&envelope, &state).unwrap();
        assert_eq!(trial.signed_transactions, 2);
        assert_eq!(trial.candidate_fees, 20);
        assert_ne!(trial.resulting_utxo_root, root_before);

        let mut expected = state.clone();
        apply_transaction(&first, &mut expected, 1).unwrap();
        apply_transaction(&second, &mut expected, 1).unwrap();
        assert_eq!(
            trial.resulting_utxo_root,
            expected.utxo.compute_state_root().unwrap()
        );
        assert_eq!(state.utxo.compute_state_root().unwrap(), root_before);
        assert!(state.utxo.utxos.contains_key(&funding));
    }

    #[test]
    fn rejects_signature_tamper_implicit_fee_gap_and_unknown_input() {
        let (state, funding) = funded_state();
        let good = signed(funding.clone(), address(), 90, 10, 1);
        let mut forged = good.clone();
        forged.inputs[0].signature = "00".repeat(64);
        forged.txid = compute_txid_v2(&forged, CHAIN).unwrap();
        assert!(inspect_dormant_native_v3_signed_prestate_trial(
            &native_envelope(vec![forged]),
            &state
        )
        .is_err());

        let mut fee_gap = good.clone();
        fee_gap.outputs[0].amount = 89;
        resign(&mut fee_gap);
        let error = inspect_dormant_native_v3_signed_prestate_trial(
            &native_envelope(vec![fee_gap]),
            &state,
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("inputs must equal outputs plus explicit fee"),
            "{error}"
        );

        let phantom = signed(
            OutPoint {
                txid: "missing-origin".into(),
                index: 1,
            },
            address(),
            90,
            10,
            3,
        );
        assert!(inspect_dormant_native_v3_signed_prestate_trial(
            &native_envelope(vec![phantom]),
            &state
        )
        .is_err());
    }

    #[test]
    fn refuses_same_envelope_double_spend_coinbase_and_cross_network_prestate() {
        let (state, funding) = funded_state();
        let a = signed(funding.clone(), address(), 90, 10, 1);
        let b = signed(funding.clone(), address(), 80, 20, 2);
        assert!(inspect_dormant_native_v3_signed_prestate_trial(
            &native_envelope(vec![a, b]),
            &state
        )
        .is_err());

        let mut coinbase_state = state.clone();
        coinbase_state
            .utxo
            .utxos
            .get_mut(&funding)
            .unwrap()
            .coinbase = true;
        let spend = signed(funding.clone(), address(), 90, 10, 1);
        assert!(inspect_dormant_native_v3_signed_prestate_trial(
            &native_envelope(vec![spend.clone()]),
            &coinbase_state
        )
        .unwrap_err()
        .to_string()
        .contains("finality/maturity"));

        let mut wrong_chain_state = state.clone();
        wrong_chain_state.chain_id = "other-network".into();
        assert!(inspect_dormant_native_v3_signed_prestate_trial(
            &native_envelope(vec![spend]),
            &wrong_chain_state
        )
        .is_err());
    }
}
