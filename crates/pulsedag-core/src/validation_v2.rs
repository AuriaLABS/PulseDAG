use std::collections::BTreeSet;

use crate::{
    errors::PulseError,
    state::ChainState,
    tx::{compute_txid_v2, verify_transaction_signatures_v2},
    types::Transaction,
    validation::tx_output_amount,
};

/// Validate a transaction against the frozen v2 chain-bound transaction domain.
///
/// This deliberately mirrors the economic/UTXO checks of the legacy validator
/// while replacing only the consensus identity and signature domain. It is a
/// non-activating primitive: the live acceptance path must opt into it only
/// after the v2.4 protocol activation gate is satisfied.
pub fn validate_transaction_v2(
    tx: &Transaction,
    state: &ChainState,
    chain_id: &str,
) -> Result<(), PulseError> {
    if tx.outputs.is_empty() {
        return Err(PulseError::InvalidTransaction("no outputs".into()));
    }
    if tx.inputs.is_empty() {
        return Err(PulseError::InvalidTransaction("no inputs".into()));
    }
    if tx.outputs.iter().any(|output| output.amount == 0) {
        return Err(PulseError::InvalidTransaction("zero-value output".into()));
    }

    let mut seen_inputs = BTreeSet::new();
    for input in &tx.inputs {
        if !seen_inputs.insert(input.previous_output.clone()) {
            return Err(PulseError::InvalidTransaction("duplicate input".into()));
        }
    }

    if compute_txid_v2(tx, chain_id)? != tx.txid {
        return Err(PulseError::InvalidTxid);
    }

    let total_input = tx.inputs.iter().try_fold(0_u64, |acc, input| {
        let input_amount =
            tx_output_amount(state, &input.previous_output).ok_or(PulseError::UtxoNotFound)?;
        if state
            .mempool
            .spent_outpoints
            .contains(&input.previous_output)
        {
            return Err(PulseError::DoubleSpend);
        }
        acc.checked_add(input_amount)
            .ok_or_else(|| PulseError::InvalidTransaction("input overflow".into()))
    })?;

    let total_output = tx
        .outputs
        .iter()
        .try_fold(0_u64, |acc, output| acc.checked_add(output.amount))
        .ok_or_else(|| PulseError::InvalidTransaction("output overflow".into()))?;
    let required = total_output
        .checked_add(tx.fee)
        .ok_or_else(|| PulseError::InvalidTransaction("output overflow".into()))?;
    if total_input < required {
        return Err(PulseError::InsufficientFunds);
    }

    verify_transaction_signatures_v2(tx, state, chain_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    use crate::{
        address_from_public_key, compute_txid_v2,
        genesis::init_chain_state,
        signing_message_v2,
        types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
        TRANSACTION_VERSION_V2,
    };

    fn signed_transaction(chain_id: &str) -> (ChainState, Transaction) {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let public_key = hex::encode(signing_key.verifying_key().to_bytes());
        let source_address = address_from_public_key(&public_key);
        let outpoint = OutPoint {
            txid: "funding".to_string(),
            index: 0,
        };

        let mut state = init_chain_state(chain_id.to_string());
        state.utxo.utxos.insert(
            outpoint.clone(),
            Utxo {
                outpoint: outpoint.clone(),
                address: source_address.clone(),
                amount: 10,
                coinbase: false,
                height: 1,
            },
        );
        state
            .utxo
            .address_index
            .entry(source_address)
            .or_default()
            .push(outpoint.clone());

        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![TxInput {
                previous_output: outpoint,
                public_key,
                signature: String::new(),
            }],
            outputs: vec![TxOutput {
                address: "pulse1recipient".to_string(),
                amount: 9,
            }],
            fee: 1,
            nonce: 5,
        };
        let message = signing_message_v2(&tx, chain_id).unwrap();
        tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
        tx.txid = compute_txid_v2(&tx, chain_id).unwrap();
        (state, tx)
    }

    #[test]
    fn accepts_valid_chain_bound_v2_transaction() {
        let (state, tx) = signed_transaction("pulsedag-testnet");
        assert!(validate_transaction_v2(&tx, &state, "pulsedag-testnet").is_ok());
    }

    #[test]
    fn wrong_chain_fails_before_transaction_can_be_admitted() {
        let (state, tx) = signed_transaction("pulsedag-testnet");
        assert!(matches!(
            validate_transaction_v2(&tx, &state, "pulsedag-private"),
            Err(PulseError::InvalidTxid)
        ));
    }

    #[test]
    fn empty_chain_id_fails_closed() {
        let (state, tx) = signed_transaction("pulsedag-testnet");
        assert!(matches!(
            validate_transaction_v2(&tx, &state, ""),
            Err(PulseError::ChainIdMismatch)
        ));
    }

    #[test]
    fn v2_validator_preserves_common_double_spend_rule() {
        let (mut state, tx) = signed_transaction("pulsedag-testnet");
        state
            .mempool
            .spent_outpoints
            .insert(tx.inputs[0].previous_output.clone());
        assert!(matches!(
            validate_transaction_v2(&tx, &state, "pulsedag-testnet"),
            Err(PulseError::DoubleSpend)
        ));
    }
}
