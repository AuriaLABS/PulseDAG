use crate::{
    errors::PulseError, state::ChainState, types::Transaction, validation::validate_transaction,
};

#[derive(Debug, Clone)]
pub struct MempoolReconcileResult {
    pub removed_txids: Vec<String>,
    pub kept_txids: Vec<String>,
}

fn simulate_mempool_accept(tx: &Transaction, state: &mut ChainState) -> Result<(), PulseError> {
    for input in &tx.inputs {
        if state
            .mempool
            .spent_outpoints
            .contains(&input.previous_output)
        {
            return Err(PulseError::DoubleSpend);
        }
        state
            .mempool
            .spent_outpoints
            .insert(input.previous_output.clone());
    }

    state
        .mempool
        .transactions
        .insert(tx.txid.clone(), tx.clone());
    Ok(())
}

pub fn reconcile_mempool(state: &mut ChainState) -> MempoolReconcileResult {
    let tx_count = state.mempool.transactions.len();
    state.mempool.counters.reconcile_runs_total = state
        .mempool
        .counters
        .reconcile_runs_total
        .saturating_add(1);
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

    let mut rebuilt_mempool = working.mempool;
    rebuilt_mempool.counters = state.mempool.counters.clone();
    rebuilt_mempool.max_transactions = state.mempool.max_transactions;
    rebuilt_mempool.counters.reconcile_removed_total = rebuilt_mempool
        .counters
        .reconcile_removed_total
        .saturating_add(removed_txids.len() as u64);
    state.mempool = rebuilt_mempool;

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
    use proptest::prelude::*;

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

    #[test]
    fn admits_higher_priority_transaction_under_pressure_by_evicting_lowest_priority() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 2;

        let key_a = signing_key(41);
        let key_b = signing_key(42);
        let key_c = signing_key(43);

        let input_a = fund_address(
            &mut state,
            "fund-pressure-a",
            0,
            address_from_public_key(&public_key_hex(&key_a)),
            80,
        );
        let input_b = fund_address(
            &mut state,
            "fund-pressure-b",
            0,
            address_from_public_key(&public_key_hex(&key_b)),
            90,
        );
        let input_c = fund_address(
            &mut state,
            "fund-pressure-c",
            0,
            address_from_public_key(&public_key_hex(&key_c)),
            100,
        );

        let low_fee_tx = signed_tx(
            &key_a,
            vec![input_a.clone()],
            vec![TxOutput {
                address: "pulse1dest-low".into(),
                amount: 79,
            }],
            1,
            1,
        );
        let mid_fee_tx = signed_tx(
            &key_b,
            vec![input_b.clone()],
            vec![TxOutput {
                address: "pulse1dest-mid".into(),
                amount: 85,
            }],
            5,
            2,
        );
        let high_fee_tx = signed_tx(
            &key_c,
            vec![input_c.clone()],
            vec![TxOutput {
                address: "pulse1dest-high".into(),
                amount: 90,
            }],
            10,
            3,
        );

        accept_transaction(low_fee_tx.clone(), &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(mid_fee_tx.clone(), &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(high_fee_tx.clone(), &mut state, AcceptSource::Rpc).unwrap();

        assert_eq!(state.mempool.transactions.len(), 2);
        assert!(!state.mempool.transactions.contains_key(&low_fee_tx.txid));
        assert!(state.mempool.transactions.contains_key(&mid_fee_tx.txid));
        assert!(state.mempool.transactions.contains_key(&high_fee_tx.txid));
        assert_eq!(state.mempool.spent_outpoints.len(), 2);
        assert!(!state.mempool.spent_outpoints.contains(&input_a));
        assert!(state.mempool.spent_outpoints.contains(&input_b));
        assert!(state.mempool.spent_outpoints.contains(&input_c));
    }

    #[test]
    fn rejects_lower_priority_transaction_when_capacity_reached() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 1;

        let high_key = signing_key(51);
        let low_key = signing_key(52);

        let high_input = fund_address(
            &mut state,
            "fund-capacity-high",
            0,
            address_from_public_key(&public_key_hex(&high_key)),
            100,
        );
        let low_input = fund_address(
            &mut state,
            "fund-capacity-low",
            0,
            address_from_public_key(&public_key_hex(&low_key)),
            100,
        );

        let high_tx = signed_tx(
            &high_key,
            vec![high_input],
            vec![TxOutput {
                address: "pulse1dest-cap-high".into(),
                amount: 80,
            }],
            20,
            1,
        );
        let low_tx = signed_tx(
            &low_key,
            vec![low_input.clone()],
            vec![TxOutput {
                address: "pulse1dest-cap-low".into(),
                amount: 95,
            }],
            5,
            2,
        );

        accept_transaction(high_tx.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let err = accept_transaction(low_tx, &mut state, AcceptSource::Rpc)
            .expect_err("lower priority tx should be rejected under pressure");
        assert!(matches!(err, PulseError::InvalidTransaction(_)));
        assert_eq!(state.mempool.transactions.len(), 1);
        assert!(state.mempool.transactions.contains_key(&high_tx.txid));
        assert_eq!(state.mempool.counters.rejected_low_priority_total, 1);
    }

    #[test]
    fn mempool_pressure_counters_stay_coherent() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 1;

        let key_a = signing_key(61);
        let key_b = signing_key(62);
        let key_c = signing_key(63);

        let input_a = fund_address(
            &mut state,
            "fund-counter-a",
            0,
            address_from_public_key(&public_key_hex(&key_a)),
            100,
        );
        let input_b = fund_address(
            &mut state,
            "fund-counter-b",
            0,
            address_from_public_key(&public_key_hex(&key_b)),
            100,
        );
        let input_c = fund_address(
            &mut state,
            "fund-counter-c",
            0,
            address_from_public_key(&public_key_hex(&key_c)),
            100,
        );

        let first = signed_tx(
            &key_a,
            vec![input_a],
            vec![TxOutput {
                address: "pulse1dest-counter-a".into(),
                amount: 96,
            }],
            4,
            1,
        );
        let second_higher = signed_tx(
            &key_b,
            vec![input_b],
            vec![TxOutput {
                address: "pulse1dest-counter-b".into(),
                amount: 90,
            }],
            10,
            2,
        );
        let third_lower = signed_tx(
            &key_c,
            vec![input_c],
            vec![TxOutput {
                address: "pulse1dest-counter-c".into(),
                amount: 99,
            }],
            1,
            3,
        );

        accept_transaction(first, &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(second_higher.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let _ = accept_transaction(third_lower, &mut state, AcceptSource::Rpc);

        assert_eq!(state.mempool.transactions.len(), 1);
        assert!(state.mempool.transactions.contains_key(&second_higher.txid));
        assert_eq!(state.mempool.counters.accepted_total, 2);
        assert_eq!(state.mempool.counters.evicted_total, 1);
        assert_eq!(state.mempool.counters.pressure_events_total, 2);
        assert_eq!(state.mempool.counters.rejected_total, 1);
        assert_eq!(state.mempool.counters.rejected_low_priority_total, 1);
    }

    #[test]
    fn equal_fee_pressure_eviction_is_deterministic_by_txid() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 1;

        let key_a = signing_key(66);
        let key_b = signing_key(67);
        let input_a = fund_address(
            &mut state,
            "fund-equal-a",
            0,
            address_from_public_key(&public_key_hex(&key_a)),
            100,
        );
        let input_b = fund_address(
            &mut state,
            "fund-equal-b",
            0,
            address_from_public_key(&public_key_hex(&key_b)),
            100,
        );

        let first = signed_tx(
            &key_a,
            vec![input_a],
            vec![TxOutput {
                address: "pulse1equal-a".into(),
                amount: 95,
            }],
            5,
            1,
        );
        let second = signed_tx(
            &key_b,
            vec![input_b],
            vec![TxOutput {
                address: "pulse1equal-b".into(),
                amount: 95,
            }],
            5,
            2,
        );

        accept_transaction(first.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let _ = accept_transaction(second.clone(), &mut state, AcceptSource::Rpc);

        let expected = std::cmp::min(first.txid.clone(), second.txid.clone());
        assert_eq!(state.mempool.transactions.len(), 1);
        assert!(state.mempool.transactions.contains_key(&expected));
    }

    #[test]
    fn reconcile_and_restart_like_rebuild_preserve_mempool_counters_coherently() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 1;

        let key_a = signing_key(64);
        let key_b = signing_key(65);
        let input_a = fund_address(
            &mut state,
            "fund-reconcile-counter-a",
            0,
            address_from_public_key(&public_key_hex(&key_a)),
            100,
        );
        let input_b = fund_address(
            &mut state,
            "fund-reconcile-counter-b",
            0,
            address_from_public_key(&public_key_hex(&key_b)),
            100,
        );
        let first = signed_tx(
            &key_a,
            vec![input_a],
            vec![TxOutput {
                address: "pulse1reconcile-counter-a".into(),
                amount: 98,
            }],
            2,
            1,
        );
        let second = signed_tx(
            &key_b,
            vec![input_b],
            vec![TxOutput {
                address: "pulse1reconcile-counter-b".into(),
                amount: 90,
            }],
            10,
            2,
        );

        accept_transaction(first, &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(second.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let before_reconcile = state.mempool.counters.clone();
        let reconcile = reconcile_mempool(&mut state);
        assert!(reconcile.removed_txids.is_empty());
        assert_eq!(state.mempool.counters.reconcile_runs_total, 1);
        assert_eq!(state.mempool.counters.reconcile_removed_total, 0);

        let mut restored = init_chain_state("test".into());
        restored.utxo = state.utxo.clone();
        restored.mempool.transactions = state.mempool.transactions.clone();
        restored.mempool.max_transactions = state.mempool.max_transactions;
        restored.mempool.counters = state.mempool.counters.clone();
        restored.mempool.spent_outpoints.clear();
        let restarted_reconcile = reconcile_mempool(&mut restored);
        assert!(restarted_reconcile.removed_txids.is_empty());
        assert_eq!(
            restored.mempool.counters.accepted_total,
            before_reconcile.accepted_total
        );
        assert_eq!(
            restored.mempool.counters.evicted_total,
            before_reconcile.evicted_total
        );
        assert_eq!(
            restored.mempool.counters.pressure_events_total,
            before_reconcile.pressure_events_total
        );
        assert_eq!(restored.mempool.counters.reconcile_runs_total, 2);
        assert_eq!(restored.mempool.counters.reconcile_removed_total, 0);
        assert_eq!(restored.mempool.transactions.len(), 1);
        assert!(restored.mempool.transactions.contains_key(&second.txid));
    }

    #[test]
    fn restart_like_spent_index_loss_is_reconciled_before_pressure_admission() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 2;

        let key = signing_key(68);
        let address = address_from_public_key(&public_key_hex(&key));
        let shared_input = fund_address(&mut state, "fund-restart-pressure", 0, address, 60);

        let tx_a = signed_tx(
            &key,
            vec![shared_input.clone()],
            vec![TxOutput {
                address: "pulse1restart-a".into(),
                amount: 50,
            }],
            10,
            1,
        );
        let tx_b = signed_tx(
            &key,
            vec![shared_input.clone()],
            vec![TxOutput {
                address: "pulse1restart-b".into(),
                amount: 49,
            }],
            11,
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

        let outsider_key = signing_key(69);
        let outsider_input = fund_address(
            &mut state,
            "fund-restart-outsider",
            0,
            address_from_public_key(&public_key_hex(&outsider_key)),
            100,
        );
        let outsider_tx = signed_tx(
            &outsider_key,
            vec![outsider_input],
            vec![TxOutput {
                address: "pulse1restart-outsider".into(),
                amount: 80,
            }],
            20,
            3,
        );

        accept_transaction(outsider_tx, &mut state, AcceptSource::Rpc).unwrap();
        assert_eq!(state.mempool.transactions.len(), 2);
        assert_eq!(state.mempool.counters.reconcile_runs_total, 1);
        assert_eq!(state.mempool.counters.reconcile_removed_total, 1);
        assert_eq!(state.mempool.spent_outpoints.len(), 2);
        assert!(state.mempool.spent_outpoints.contains(&shared_input));
    }

    #[test]
    fn package_dependency_set_is_admitted_and_reconciled_coherently() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 8;

        let parent_key = signing_key(70);
        let child_key = signing_key(71);
        let parent_address = address_from_public_key(&public_key_hex(&parent_key));
        let parent_input = fund_address(&mut state, "fund-package-parent", 0, parent_address, 100);

        let parent = signed_tx(
            &parent_key,
            vec![parent_input.clone()],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&child_key)),
                amount: 90,
            }],
            10,
            1,
        );
        let child = signed_tx(
            &child_key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1package-child-dest".into(),
                amount: 80,
            }],
            10,
            2,
        );

        accept_transaction(parent.clone(), &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(child.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let reconcile = reconcile_mempool(&mut state);

        assert!(reconcile.removed_txids.is_empty());
        assert!(state.mempool.transactions.contains_key(&parent.txid));
        assert!(state.mempool.transactions.contains_key(&child.txid));
        assert!(state.mempool.spent_outpoints.contains(&parent_input));
        assert!(state.mempool.spent_outpoints.contains(&OutPoint {
            txid: parent.txid.clone(),
            index: 0,
        }));
    }

    #[test]
    fn pressure_eviction_respects_dependency_package_threshold() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 2;

        let parent_key = signing_key(72);
        let child_key = signing_key(73);
        let outsider_key = signing_key(74);
        let parent_input = fund_address(
            &mut state,
            "fund-package-evict-parent",
            0,
            address_from_public_key(&public_key_hex(&parent_key)),
            100,
        );
        let outsider_input = fund_address(
            &mut state,
            "fund-package-evict-outsider",
            0,
            address_from_public_key(&public_key_hex(&outsider_key)),
            100,
        );

        let parent = signed_tx(
            &parent_key,
            vec![parent_input],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&child_key)),
                amount: 98,
            }],
            2,
            1,
        );
        let child = signed_tx(
            &child_key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1package-evict-child".into(),
                amount: 83,
            }],
            15,
            2,
        );
        let outsider = signed_tx(
            &outsider_key,
            vec![outsider_input],
            vec![TxOutput {
                address: "pulse1package-evict-outsider".into(),
                amount: 90,
            }],
            10,
            3,
        );

        accept_transaction(parent.clone(), &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(child.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let err = accept_transaction(outsider, &mut state, AcceptSource::Rpc)
            .expect_err("outsider should not evict a higher-priority dependency package");
        assert!(matches!(err, PulseError::InvalidTransaction(_)));
        assert!(state.mempool.transactions.contains_key(&parent.txid));
        assert!(state.mempool.transactions.contains_key(&child.txid));
    }

    #[test]
    fn conflicting_child_spend_is_rejected_for_existing_dependency_set() {
        let mut state = init_chain_state("test".into());

        let parent_key = signing_key(75);
        let child_a_key = signing_key(76);
        let child_b_key = signing_key(77);
        let parent_input = fund_address(
            &mut state,
            "fund-package-conflict-parent",
            0,
            address_from_public_key(&public_key_hex(&parent_key)),
            100,
        );

        let parent = signed_tx(
            &parent_key,
            vec![parent_input],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&child_a_key)),
                amount: 95,
            }],
            5,
            1,
        );
        let child_a = signed_tx(
            &child_a_key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1conflict-a".into(),
                amount: 80,
            }],
            15,
            2,
        );
        let child_b = signed_tx(
            &child_b_key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1conflict-b".into(),
                amount: 79,
            }],
            16,
            3,
        );

        accept_transaction(parent, &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(child_a.clone(), &mut state, AcceptSource::Rpc).unwrap();
        let err = accept_transaction(child_b, &mut state, AcceptSource::Rpc)
            .expect_err("conflicting spend must still be rejected");
        assert!(matches!(err, PulseError::DoubleSpend));
        assert!(state.mempool.transactions.contains_key(&child_a.txid));
    }

    #[test]
    fn simple_independent_pressure_behavior_has_no_regression() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_transactions = 1;

        let low_key = signing_key(78);
        let high_key = signing_key(79);
        let low_input = fund_address(
            &mut state,
            "fund-ind-low",
            0,
            address_from_public_key(&public_key_hex(&low_key)),
            100,
        );
        let high_input = fund_address(
            &mut state,
            "fund-ind-high",
            0,
            address_from_public_key(&public_key_hex(&high_key)),
            100,
        );
        let low = signed_tx(
            &low_key,
            vec![low_input],
            vec![TxOutput {
                address: "pulse1ind-low".into(),
                amount: 99,
            }],
            1,
            1,
        );
        let high = signed_tx(
            &high_key,
            vec![high_input],
            vec![TxOutput {
                address: "pulse1ind-high".into(),
                amount: 80,
            }],
            20,
            2,
        );

        accept_transaction(low, &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(high.clone(), &mut state, AcceptSource::Rpc).unwrap();
        assert_eq!(state.mempool.transactions.len(), 1);
        assert!(state.mempool.transactions.contains_key(&high.txid));
    }

    #[test]
    fn orphan_tx_is_stored_in_orphan_pool() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_orphans = 8;

        let parent_key = signing_key(71);
        let child_key = signing_key(72);

        let parent_output = OutPoint {
            txid: "missing-parent".into(),
            index: 0,
        };
        let orphan = signed_tx(
            &child_key,
            vec![parent_output.clone()],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&parent_key)),
                amount: 10,
            }],
            1,
            1,
        );

        accept_transaction(orphan.clone(), &mut state, AcceptSource::Rpc).unwrap();

        assert!(state.mempool.transactions.is_empty());
        assert!(state.mempool.orphan_transactions.contains_key(&orphan.txid));
        assert_eq!(
            state.mempool.orphan_missing_outpoints.get(&orphan.txid),
            Some(&vec![parent_output])
        );
    }

    #[test]
    fn orphan_is_promoted_when_dependency_arrives() {
        let mut state = init_chain_state("test".into());
        let parent_key = signing_key(81);

        let funded = fund_address(
            &mut state,
            "fund-parent",
            0,
            address_from_public_key(&public_key_hex(&parent_key)),
            50,
        );
        let parent = signed_tx(
            &parent_key,
            vec![funded],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&parent_key)),
                amount: 45,
            }],
            5,
            1,
        );
        let child = signed_tx(
            &parent_key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1child-dest".into(),
                amount: 40,
            }],
            5,
            2,
        );

        accept_transaction(child.clone(), &mut state, AcceptSource::Rpc).unwrap();
        assert!(state.mempool.orphan_transactions.contains_key(&child.txid));

        accept_transaction(parent.clone(), &mut state, AcceptSource::Rpc).unwrap();
        assert!(state.mempool.transactions.contains_key(&parent.txid));
        assert!(state.mempool.transactions.contains_key(&child.txid));
        assert!(!state.mempool.orphan_transactions.contains_key(&child.txid));
        assert_eq!(state.mempool.counters.orphan_promoted_total, 1);
    }

    #[test]
    fn invalid_orphan_is_dropped_safely_on_promotion_attempt() {
        let mut state = init_chain_state("test".into());
        let parent_key = signing_key(91);
        let child_key = signing_key(92);

        let funded = fund_address(
            &mut state,
            "fund-invalid-parent",
            0,
            address_from_public_key(&public_key_hex(&parent_key)),
            60,
        );
        let parent = signed_tx(
            &parent_key,
            vec![funded],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&parent_key)),
                amount: 55,
            }],
            5,
            1,
        );
        let invalid_child = signed_tx(
            &child_key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1invalid-child".into(),
                amount: 50,
            }],
            5,
            2,
        );

        accept_transaction(invalid_child.clone(), &mut state, AcceptSource::Rpc).unwrap();
        assert!(state
            .mempool
            .orphan_transactions
            .contains_key(&invalid_child.txid));

        accept_transaction(parent, &mut state, AcceptSource::Rpc).unwrap();
        assert!(!state
            .mempool
            .orphan_transactions
            .contains_key(&invalid_child.txid));
        assert!(!state.mempool.transactions.contains_key(&invalid_child.txid));
        assert!(state.mempool.counters.orphan_dropped_total >= 1);
    }

    #[test]
    fn orphan_limits_prevent_unbounded_growth() {
        let mut state = init_chain_state("test".into());
        state.mempool.max_orphans = 2;
        let key = signing_key(101);

        for idx in 0..3u32 {
            let orphan = signed_tx(
                &key,
                vec![OutPoint {
                    txid: format!("missing-limit-{idx}"),
                    index: 0,
                }],
                vec![TxOutput {
                    address: "pulse1limit".into(),
                    amount: 10,
                }],
                1,
                idx as u64,
            );
            accept_transaction(orphan, &mut state, AcceptSource::Rpc).unwrap();
        }

        assert_eq!(state.mempool.orphan_transactions.len(), 2);
        assert_eq!(state.mempool.counters.orphan_pruned_total, 1);
    }

    #[test]
    fn double_spend_conflict_is_enforced_after_orphan_promotion() {
        let mut state = init_chain_state("test".into());
        let key = signing_key(111);
        let funded = fund_address(
            &mut state,
            "fund-promotion-conflict",
            0,
            address_from_public_key(&public_key_hex(&key)),
            70,
        );

        let parent = signed_tx(
            &key,
            vec![funded],
            vec![TxOutput {
                address: address_from_public_key(&public_key_hex(&key)),
                amount: 60,
            }],
            10,
            1,
        );
        let orphan_a = signed_tx(
            &key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1conflict-a".into(),
                amount: 55,
            }],
            5,
            2,
        );
        let orphan_b = signed_tx(
            &key,
            vec![OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            }],
            vec![TxOutput {
                address: "pulse1conflict-b".into(),
                amount: 54,
            }],
            6,
            3,
        );

        accept_transaction(orphan_a.clone(), &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(orphan_b.clone(), &mut state, AcceptSource::Rpc).unwrap();
        accept_transaction(parent, &mut state, AcceptSource::Rpc).unwrap();

        let promoted = [
            state.mempool.transactions.contains_key(&orphan_a.txid),
            state.mempool.transactions.contains_key(&orphan_b.txid),
        ]
        .into_iter()
        .filter(|present| *present)
        .count();
        assert_eq!(promoted, 1, "exactly one conflicting orphan should promote");
    }

    proptest! {
        #[test]
        fn reconcile_never_keeps_more_than_one_conflicting_spend(tx_count in 2usize..8usize) {
            let mut state = init_chain_state("test".into());
            let key = signing_key(23);
            let address = address_from_public_key(&public_key_hex(&key));
            let shared_input = fund_address(&mut state, "fund-shared-prop", 0, address, 500);

            for nonce in 0..tx_count {
                let tx = signed_tx(
                    &key,
                    vec![shared_input.clone()],
                    vec![TxOutput {
                        address: format!("pulse1dest-{nonce}"),
                        amount: 400,
                    }],
                    100,
                    nonce as u64 + 1,
                );
                state
                    .mempool
                    .transactions
                    .insert(tx.txid.clone(), tx);
            }

            let result = reconcile_mempool(&mut state);

            prop_assert_eq!(result.kept_txids.len(), 1);
            prop_assert_eq!(state.mempool.transactions.len(), 1);
            prop_assert!(state.mempool.spent_outpoints.contains(&shared_input));
        }
    }
}
