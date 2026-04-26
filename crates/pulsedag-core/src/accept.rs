use crate::{
    apply::apply_block,
    errors::PulseError,
    mempool::reconcile_mempool,
    pow_evaluate, selected_pow_name,
    state::ChainState,
    types::{Block, Transaction},
    validation::{missing_transaction_inputs, validate_block, validate_transaction},
};

#[derive(Debug, Clone, Copy)]
pub enum AcceptSource {
    Rpc,
    P2p,
    LocalMining,
}

fn is_higher_priority(candidate: &Transaction, existing: &Transaction) -> bool {
    candidate.fee > existing.fee
        || (candidate.fee == existing.fee && candidate.txid < existing.txid)
}

fn lowest_priority_txid(state: &ChainState) -> Option<String> {
    state
        .mempool
        .transactions
        .values()
        .min_by(|a, b| a.fee.cmp(&b.fee).then_with(|| b.txid.cmp(&a.txid)))
        .map(|tx| tx.txid.clone())
}

fn prune_orphans(state: &mut ChainState) {
    if state.mempool.orphan_transactions.len() <= state.mempool.max_orphans {
        return;
    }
    let mut by_age = state
        .mempool
        .orphan_received_order
        .iter()
        .map(|(txid, order)| (txid.clone(), *order))
        .collect::<Vec<_>>();
    by_age.sort_by_key(|(_, order)| *order);

    let overflow = state
        .mempool
        .orphan_transactions
        .len()
        .saturating_sub(state.mempool.max_orphans);
    for (txid, _) in by_age.into_iter().take(overflow) {
        let removed = state.mempool.orphan_transactions.remove(&txid).is_some();
        state.mempool.orphan_missing_outpoints.remove(&txid);
        state.mempool.orphan_received_order.remove(&txid);
        if removed {
            state.mempool.counters.orphan_dropped_total = state
                .mempool
                .counters
                .orphan_dropped_total
                .saturating_add(1);
            state.mempool.counters.orphan_pruned_total =
                state.mempool.counters.orphan_pruned_total.saturating_add(1);
        }
    }
}

fn store_orphan_transaction(tx: Transaction, state: &mut ChainState) {
    let txid = tx.txid.clone();
    let missing = missing_transaction_inputs(&tx, state);
    let replaced = state
        .mempool
        .orphan_transactions
        .insert(txid.clone(), tx)
        .is_some();
    state
        .mempool
        .orphan_missing_outpoints
        .insert(txid.clone(), missing);
    if !replaced {
        let order = state.mempool.next_orphan_order;
        state.mempool.next_orphan_order = state.mempool.next_orphan_order.saturating_add(1);
        state.mempool.orphan_received_order.insert(txid, order);
        state.mempool.counters.orphaned_total =
            state.mempool.counters.orphaned_total.saturating_add(1);
    }
    prune_orphans(state);
}

fn remove_orphan_transaction(txid: &str, state: &mut ChainState) {
    state.mempool.orphan_transactions.remove(txid);
    state.mempool.orphan_missing_outpoints.remove(txid);
    state.mempool.orphan_received_order.remove(txid);
}

fn promote_ready_orphans(state: &mut ChainState, source: AcceptSource) {
    loop {
        let mut ready = state
            .mempool
            .orphan_transactions
            .iter()
            .filter_map(|(txid, tx)| {
                if missing_transaction_inputs(tx, state).is_empty() {
                    Some(txid.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if ready.is_empty() {
            break;
        }
        ready.sort();

        let mut promoted_any = false;
        for txid in ready {
            let Some(tx) = state.mempool.orphan_transactions.get(&txid).cloned() else {
                continue;
            };
            remove_orphan_transaction(&txid, state);
            match accept_transaction(tx.clone(), state, source) {
                Ok(()) => {
                    state.mempool.counters.orphan_promoted_total = state
                        .mempool
                        .counters
                        .orphan_promoted_total
                        .saturating_add(1);
                    promoted_any = true;
                }
                Err(PulseError::UtxoNotFound) => {
                    // Became unresolved again due to competing promotion; park back as orphan.
                    store_orphan_transaction(tx, state);
                }
                Err(_) => {
                    state.mempool.counters.orphan_dropped_total = state
                        .mempool
                        .counters
                        .orphan_dropped_total
                        .saturating_add(1);
                }
            }
        }
        if !promoted_any {
            break;
        }
    }
}

fn mempool_needs_reconcile(state: &ChainState) -> bool {
    let mut expected_spent = std::collections::HashSet::new();
    for tx in state.mempool.transactions.values() {
        for input in &tx.inputs {
            if !expected_spent.insert(input.previous_output.clone()) {
                return true;
            }
        }
    }
    expected_spent != state.mempool.spent_outpoints
}

pub fn accept_transaction(
    tx: Transaction,
    state: &mut ChainState,
    source: AcceptSource,
) -> Result<(), PulseError> {
    if mempool_needs_reconcile(state) {
        reconcile_mempool(state);
    }

    if state.mempool.transactions.contains_key(&tx.txid)
        || state.mempool.orphan_transactions.contains_key(&tx.txid)
    {
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return Err(PulseError::TxAlreadyExists);
    }

    if let Err(err) = validate_transaction(&tx, state) {
        if matches!(err, PulseError::UtxoNotFound) {
            store_orphan_transaction(tx, state);
            return Ok(());
        }
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return Err(err);
    }
    if state.mempool.transactions.len() >= state.mempool.max_transactions {
        state.mempool.counters.pressure_events_total = state
            .mempool
            .counters
            .pressure_events_total
            .saturating_add(1);

        let lowest_txid = lowest_priority_txid(state).ok_or_else(|| {
            PulseError::Internal("mempool pressure detected with no eviction candidate".into())
        })?;
        let should_evict = state
            .mempool
            .transactions
            .get(&lowest_txid)
            .map(|lowest| is_higher_priority(&tx, lowest))
            .unwrap_or(false);
        if !should_evict {
            state.mempool.counters.rejected_total =
                state.mempool.counters.rejected_total.saturating_add(1);
            state.mempool.counters.rejected_low_priority_total = state
                .mempool
                .counters
                .rejected_low_priority_total
                .saturating_add(1);
            return Err(PulseError::InvalidTransaction(
                "mempool under pressure: transaction priority below threshold".into(),
            ));
        }

        if let Some(evicted) = state.mempool.transactions.remove(&lowest_txid) {
            for input in &evicted.inputs {
                state.mempool.spent_outpoints.remove(&input.previous_output);
            }
            state.mempool.counters.evicted_total =
                state.mempool.counters.evicted_total.saturating_add(1);
        }
    }

    for input in &tx.inputs {
        state
            .mempool
            .spent_outpoints
            .insert(input.previous_output.clone());
    }
    state.mempool.transactions.insert(tx.txid.clone(), tx);
    state.mempool.counters.accepted_total = state.mempool.counters.accepted_total.saturating_add(1);
    promote_ready_orphans(state, source);
    Ok(())
}

pub fn accept_block(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
) -> Result<(), PulseError> {
    validate_block(&block, state)?;

    let enforce_pow = matches!(
        source,
        AcceptSource::Rpc | AcceptSource::P2p | AcceptSource::LocalMining
    );
    let pow = pow_evaluate(&block.header);
    if enforce_pow && !pow.accepted {
        return Err(PulseError::InvalidBlock(format!(
            "pow rejected by current {} policy",
            selected_pow_name()
        )));
    }

    apply_block(&block, state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining::{build_candidate_block, build_coinbase_transaction},
    };

    #[test]
    fn rejects_block_with_invalid_pow() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, u32::MAX, txs);
        block.hash = "bad-pow".to_string();
        block.header.nonce = 0;

        let err = accept_block(block, &mut state, AcceptSource::P2p).unwrap_err();
        assert!(matches!(err, PulseError::InvalidBlock(msg) if msg.contains("pow rejected")));
    }

    #[test]
    fn accepts_block_with_valid_pow() {
        let mut state = init_chain_state("test".to_string());
        let parents = vec![state.dag.genesis_hash.clone()];
        let txs = vec![build_coinbase_transaction("miner1", 50, 1)];
        let mut block = build_candidate_block(parents, 1, 1, txs);
        block.hash = "good-pow".to_string();
        block.header.nonce = 0;

        assert!(accept_block(block, &mut state, AcceptSource::P2p).is_ok());
    }
}
