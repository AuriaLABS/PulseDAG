use crate::{
    errors::PulseError,
    state::ChainState,
    types::{OutPoint, Transaction, Utxo},
    validation::validate_transaction,
};

#[derive(Debug, Clone)]
pub struct MempoolReconcileResult {
    pub removed_txids: Vec<String>,
    pub kept_txids: Vec<String>,
}

fn simulate_mempool_accept(tx: &Transaction, state: &mut ChainState) -> Result<(), PulseError> {
    for input in &tx.inputs {
        let spent = state
            .utxo
            .utxos
            .remove(&input.previous_output)
            .ok_or(PulseError::UtxoNotFound)?;
        if let Some(entries) = state.utxo.address_index.get_mut(&spent.address) {
            entries.retain(|op| op != &input.previous_output);
        }
        state
            .mempool
            .spent_outpoints
            .insert(input.previous_output.clone());
    }

    for (index, output) in tx.outputs.iter().enumerate() {
        let outpoint = OutPoint {
            txid: tx.txid.clone(),
            index: index as u32,
        };
        let utxo = Utxo {
            outpoint: outpoint.clone(),
            address: output.address.clone(),
            amount: output.amount,
            coinbase: false,
            height: 0,
        };
        state.utxo.utxos.insert(outpoint.clone(), utxo);
        state
            .utxo
            .address_index
            .entry(output.address.clone())
            .or_default()
            .push(outpoint);
    }

    state.mempool.transactions.insert(tx.txid.clone(), tx.clone());
    Ok(())
}

pub fn reconcile_mempool(state: &mut ChainState) -> MempoolReconcileResult {
    let tx_count = state.mempool.transactions.len();
    if tx_count == 0 {
        state.mempool.spent_outpoints.clear();
        return MempoolReconcileResult {
            removed_txids: Vec::new(),
            kept_txids: Vec::new(),
        };
    }

    let mut txs = std::mem::take(&mut state.mempool.transactions)
        .into_values()
        .collect::<Vec<_>>();
    txs.sort_by(|a, b| a.txid.cmp(&b.txid));

    let mut working = state.clone();
    working.mempool.transactions.clear();
    working.mempool.spent_outpoints.clear();

    let mut removed_txids = Vec::with_capacity(tx_count);
    let mut kept_txids = Vec::with_capacity(tx_count);

    for tx in txs {
        let txid = tx.txid.clone();
        let valid = validate_transaction(&tx, &working).is_ok()
            && simulate_mempool_accept(&tx, &mut working).is_ok();
        if valid {
            kept_txids.push(txid);
        } else {
            removed_txids.push(txid);
        }
    }

    state.mempool = working.mempool;

    MempoolReconcileResult {
        removed_txids,
        kept_txids,
    }
}

#[cfg(test)]
mod tests {
    use super::reconcile_mempool;
    use crate::{
        accept::{accept_transaction, AcceptSource},
        errors::PulseError,
        genesis::init_chain_state,
        tx::{address_from_public_key, compute_txid, signing_message},
        types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public_key_hex(signing_key: &SigningKey) -> String {
        hex::encode(signing_key.verifying_key().to_bytes())
    }

    fn fund_address(
        state: &mut crate::state::ChainState,
        txid: &str,
        index: u32,
        address: String,
        amount: u64,
    ) -> OutPoint {
        let outpoint = OutPoint {
            txid: txid.to_string(),
            index,
        };
        let utxo = Utxo {
            outpoint: outpoint.clone(),
            address: address.clone(),
            amount,
            coinbase: false,
            height: 1,
        };
        state.utxo.utxos.insert(outpoint.clone(), utxo);
        state
            .utxo
            .address_index
            .entry(address)
            .or_default()
            .push(outpoint.clone());
        outpoint
    }

    fn signed_tx(
        signing_key: &SigningKey,
        previous_outputs: Vec<OutPoint>,
        outputs: Vec<TxOutput>,
        fee: u64,
        nonce: u64,
    ) -> Transaction {
        let public_key = public_key_hex(signing_key);
        let mut tx = Transaction {
            txid: String::new(),
            version: 1,
            inputs: previous_outputs
                .into_iter()
                .map(|previous_output| TxInput {
                    previous_output,
                    public_key: public_key.clone(),
                    signature: String::new(),
                })
                .collect(),
            outputs,
            fee,
            nonce,
        };

        let message = signing_message(&tx);
        let signature = signing_key.sign(&message);
        let signature_hex = hex::encode(signature.to_bytes());
        for input in &mut tx.inputs {
            input.signature = signature_hex.clone();
        }
        tx.txid = compute_txid(&tx);
        tx
    }

    #[test]
    fn reconcile_keeps_two_valid_non_conflicting_transactions() {
        let mut state = init_chain_state("test".into());

        let key_a = signing_key(7);
        let key_b = signing_key(8);

        let address_a = address_from_public_key(&public_key_hex(&key_a));
        let address_b = address_from_public_key(&public_key_hex(&key_b));

        let input_a = fund_address(&mut state, "fund-a", 0, address_a, 60);
        let input_b = fund_address(&mut state, "fund-b", 0, address_b, 80);

        let tx_a = signed_tx(
            &key_a,
            vec![input_a.clone()],
            vec![TxOutput {
                address: "pulse1dest-a".into(),
                amount: 55,
            }],
            5,
            1,
        );
        let tx_b = signed_tx(
            &key_b,
            vec![input_b.clone()],
            vec![TxOutput {
                address: "pulse1dest-b".into(),
                amount: 70,
            }],
            10,
            2,
        );

        state
            .mempool
            .transactions
            .insert(tx_a.txid.clone(), tx_a.clone());
        state
            .mempool
            .transactions
            .insert(tx_b.txid.clone(), tx_b.clone());
        state.mempool.spent_outpoints.clear();

        let result = reconcile_mempool(&mut state);

        assert!(result.removed_txids.is_empty());
        assert_eq!(result.kept_txids.len(), 2);
        assert_eq!(state.mempool.transactions.len(), 2);
        assert_eq!(state.mempool.spent_outpoints.len(), 2);
        assert!(state.mempool.spent_outpoints.contains(&input_a));
        assert!(state.mempool.spent_outpoints.contains(&input_b));
    }

    #[test]
    fn reconcile_removes_deterministically_conflicting_double_spend() {
        let mut state = init_chain_state("test".into());

        let key = signing_key(11);
        let address = address_from_public_key(&public_key_hex(&key));
        let shared_input = fund_address(&mut state, "fund-shared", 0, address, 50);

        let tx_a = signed_tx(
            &key,
            vec![shared_input.clone()],
            vec![TxOutput {
                address: "pulse1dest-a".into(),
                amount: 45,
            }],
            5,
            1,
        );
        let tx_b = signed_tx(
            &key,
            vec![shared_input.clone()],
            vec![TxOutput {
                address: "pulse1dest-b".into(),
                amount: 40,
            }],
            10,
            2,
        );

        state
            .mempool
            .transactions
            .insert(tx_a.txid.clone(), tx_a.clone());
        state
            .mempool
            .transactions
            .insert(tx_b.txid.clone(), tx_b.clone());
        state.mempool.spent_outpoints.clear();

        let result = reconcile_mempool(&mut state);
        let expected_kept = std::cmp::min(tx_a.txid.clone(), tx_b.txid.clone());

        assert_eq!(result.kept_txids, vec![expected_kept.clone()]);
        assert_eq!(result.removed_txids.len(), 1);
        assert_eq!(state.mempool.transactions.len(), 1);
        assert!(state.mempool.transactions.contains_key(&expected_kept));
        assert_eq!(state.mempool.spent_outpoints.len(), 1);
        assert!(state.mempool.spent_outpoints.contains(&shared_input));
    }

    #[test]
    fn reconcile_rebuilds_spent_outpoints_count_for_multi_input_transaction() {
        let mut state = init_chain_state("test".into());

        let key = signing_key(21);
        let address = address_from_public_key(&public_key_hex(&key));
        let input_a = fund_address(&mut state, "fund-multi-a", 0, address.clone(), 30);
        let input_b = fund_address(&mut state, "fund-multi-b", 0, address, 25);

        let tx = signed_tx(
            &key,
            vec![input_a.clone(), input_b.clone()],
            vec![TxOutput {
                address: "pulse1dest-multi".into(),
                amount: 50,
            }],
            5,
            3,
        );

        state
            .mempool
            .transactions
            .insert(tx.txid.clone(), tx.clone());
        state.mempool.spent_outpoints.clear();

        let result = reconcile_mempool(&mut state);

        assert!(result.removed_txids.is_empty());
        assert_eq!(result.kept_txids, vec![tx.txid.clone()]);
        assert_eq!(state.mempool.transactions.len(), 1);
        assert_eq!(state.mempool.spent_outpoints.len(), 2);
        assert!(state.mempool.spent_outpoints.contains(&input_a));
        assert!(state.mempool.spent_outpoints.contains(&input_b));
    }

    #[test]
    fn reconcile_restores_double_spend_protection_after_restart_like_state_loss() {
        let mut state = init_chain_state("test".into());

        let key = signing_key(31);
        let address = address_from_public_key(&public_key_hex(&key));
        let shared_input = fund_address(&mut state, "fund-restart", 0, address, 50);

        let kept_tx = signed_tx(
            &key,
            vec![shared_input.clone()],
            vec![TxOutput {
                address: "pulse1dest-keep".into(),
                amount: 45,
            }],
            5,
            1,
        );
        let conflicting_tx = signed_tx(
            &key,
            vec![shared_input.clone()],
            vec![TxOutput {
                address: "pulse1dest-conflict".into(),
                amount: 44,
            }],
            6,
            2,
        );

        state
            .mempool
            .transactions
            .insert(kept_tx.txid.clone(), kept_tx);
        state.mempool.spent_outpoints.clear();

        let result = reconcile_mempool(&mut state);
        assert!(result.removed_txids.is_empty());
        assert_eq!(state.mempool.spent_outpoints.len(), 1);

        let err = accept_transaction(conflicting_tx, &mut state, AcceptSource::Rpc)
            .expect_err("conflicting transaction should be rejected after reconcile");
        assert!(matches!(err, PulseError::DoubleSpend));
    }
}
