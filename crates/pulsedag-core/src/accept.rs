use crate::{
    apply::apply_block,
    dev_pow_accepts,
    errors::PulseError,
    selected_pow_name,
    state::ChainState,
    types::{Block, Transaction},
    validation::{validate_block, validate_transaction},
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

pub fn accept_transaction(
    tx: Transaction,
    state: &mut ChainState,
    _source: AcceptSource,
) -> Result<(), PulseError> {
    if let Err(err) = validate_transaction(&tx, state) {
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return Err(err);
    }
    if state.mempool.transactions.contains_key(&tx.txid) {
        state.mempool.counters.rejected_total =
            state.mempool.counters.rejected_total.saturating_add(1);
        return Err(PulseError::TxAlreadyExists);
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
    if enforce_pow && !dev_pow_accepts(&block.header) {
        return Err(PulseError::InvalidBlock(format!(
            "pow rejected by current {} policy",
            selected_pow_name()
        )));
    }

    apply_block(&block, state)?;
    Ok(())
}
