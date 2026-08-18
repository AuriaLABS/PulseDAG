use crate::{
    errors::PulseError,
    mempool::MempoolReconcileResult,
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    tx_protocol::{resolve_transaction_validation_path, validate_transaction_for_protocol},
    types::Transaction,
};

fn assign_first_seen(txid: &str, state: &mut ChainState) {
    if state.mempool.first_seen.contains_key(txid) {
        return;
    }
    let sequence = state.mempool.next_first_seen;
    state.mempool.next_first_seen = state.mempool.next_first_seen.saturating_add(1);
    state.mempool.first_seen.insert(txid.to_string(), sequence);
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

    assign_first_seen(&tx.txid, state);
    state
        .mempool
        .transactions
        .insert(tx.txid.clone(), tx.clone());
    Ok(())
}

/// Rebuild the live mempool against the current UTXO view using one explicit
/// protocol activation identity.
///
/// The historical `reconcile_mempool` path remains v1-only. This companion
/// performs the same deterministic first-seen/txid rebuild while dispatching
/// validation through the frozen protocol selector, so an activated-v2 block
/// commit cannot accidentally revalidate remaining v2 transactions as v1.
pub fn reconcile_mempool_for_protocol(
    state: &mut ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<MempoolReconcileResult, PulseError> {
    // Resolve the identity before any counter or mempool mutation so a bad
    // activation tuple fails closed.
    resolve_transaction_validation_path(identity, state)?;

    let tx_count = state.mempool.transactions.len();
    state.mempool.counters.reconcile_runs_total = state
        .mempool
        .counters
        .reconcile_runs_total
        .saturating_add(1);
    if tx_count == 0 {
        state.mempool.spent_outpoints.clear();
        return Ok(MempoolReconcileResult {
            removed_txids: Vec::new(),
            kept_txids: Vec::new(),
        });
    }

    let original_first_seen = state.mempool.first_seen.clone();
    let mut txs = std::mem::take(&mut state.mempool.transactions)
        .into_values()
        .collect::<Vec<_>>();
    txs.sort_by_key(|tx| {
        (
            original_first_seen
                .get(&tx.txid)
                .copied()
                .unwrap_or(u64::MAX),
            tx.txid.clone(),
        )
    });

    let mut working = state.clone();
    working.mempool.transactions.clear();
    working.mempool.spent_outpoints.clear();

    let mut removed_txids = Vec::with_capacity(tx_count);
    let mut kept_txids = Vec::with_capacity(tx_count);
    let mut pending = txs;

    while !pending.is_empty() {
        let mut next_pending = Vec::new();
        let mut progressed = false;

        for tx in pending {
            let txid = tx.txid.clone();
            match validate_transaction_for_protocol(&tx, &working, identity) {
                Ok(()) => {
                    if simulate_mempool_accept(&tx, &mut working).is_ok() {
                        kept_txids.push(txid);
                        progressed = true;
                    } else {
                        removed_txids.push(txid);
                    }
                }
                Err(PulseError::UtxoNotFound) => next_pending.push(tx),
                Err(_) => removed_txids.push(txid),
            }
        }

        if !progressed {
            removed_txids.extend(next_pending.into_iter().map(|tx| tx.txid));
            break;
        }
        pending = next_pending;
    }

    let mut rebuilt_mempool = working.mempool;
    rebuilt_mempool
        .first_seen
        .retain(|txid, _| rebuilt_mempool.transactions.contains_key(txid));
    rebuilt_mempool.counters = state.mempool.counters.clone();
    rebuilt_mempool.max_transactions = state.mempool.max_transactions;
    rebuilt_mempool.max_spent_outpoints = state.mempool.max_spent_outpoints;
    rebuilt_mempool.next_first_seen = rebuilt_mempool
        .first_seen
        .values()
        .copied()
        .max()
        .map(|sequence| sequence.saturating_add(1))
        .unwrap_or(0);
    rebuilt_mempool.counters.reconcile_removed_total = rebuilt_mempool
        .counters
        .reconcile_removed_total
        .saturating_add(removed_txids.len() as u64);
    state.mempool = rebuilt_mempool;

    Ok(MempoolReconcileResult {
        removed_txids,
        kept_txids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accept::{accept_transaction_for_protocol, AcceptSource},
        address_from_public_key, compute_txid_v2,
        genesis::init_chain_state,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        signing_message_v2,
        types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
        TRANSACTION_VERSION_V2,
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn signed_v2_transaction(state: &mut ChainState, seed: u8, funding: &str) -> Transaction {
        let signing_key = SigningKey::from_bytes(&[seed; 32]);
        let public_key = hex::encode(signing_key.verifying_key().to_bytes());
        let address = address_from_public_key(&public_key);
        let outpoint = OutPoint {
            txid: funding.to_string(),
            index: 0,
        };
        state.utxo.utxos.insert(
            outpoint.clone(),
            Utxo {
                outpoint: outpoint.clone(),
                address: address.clone(),
                amount: 10,
                coinbase: false,
                height: 0,
            },
        );
        state
            .utxo
            .address_index
            .entry(address)
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
            nonce: u64::from(seed),
        };
        let message = signing_message_v2(&tx, &state.chain_id).unwrap();
        tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
        tx.txid = compute_txid_v2(&tx, &state.chain_id).unwrap();
        tx
    }

    #[test]
    fn activated_v2_reconcile_keeps_valid_v2_transactions() {
        let mut state = init_chain_state("task28-mempool-protocol".to_string());
        let identity = identity(&state);
        let tx = signed_v2_transaction(&mut state, 7, "funding-a");
        accept_transaction_for_protocol(tx.clone(), &mut state, AcceptSource::Rpc, &identity)
            .unwrap();

        let result = reconcile_mempool_for_protocol(&mut state, &identity).unwrap();

        assert_eq!(result.kept_txids, vec![tx.txid.clone()]);
        assert!(result.removed_txids.is_empty());
        assert!(state.mempool.transactions.contains_key(&tx.txid));
    }

    #[test]
    fn wrong_identity_fails_before_reconciliation_mutates_state() {
        let mut state = init_chain_state("task28-mempool-protocol".to_string());
        let mut wrong = identity(&state);
        wrong.chain_id.push_str("-wrong");
        let before = bincode::serialize(&state).unwrap();

        assert!(reconcile_mempool_for_protocol(&mut state, &wrong).is_err());
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }
}
