use crate::{
    apply::apply_block,
    dev_pow_accepts,
    errors::PulseError,
    mempool::{evict_lowest_fee_density, fee_density},
    mining::current_ts,
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

pub fn accept_transaction(
    tx: Transaction,
    state: &mut ChainState,
    _source: AcceptSource,
) -> Result<(), PulseError> {
    if state
        .dag
        .blocks
        .values()
        .any(|block| block.transactions.iter().any(|known| known.txid == tx.txid))
    {
        state.mempool.rejected_total = state.mempool.rejected_total.saturating_add(1);
        return Err(PulseError::TxAlreadyExists);
    }
    if tx.fee < state.mempool.fee_floor {
        state.mempool.rejected_total = state.mempool.rejected_total.saturating_add(1);
        state.mempool.rejected_fee_floor_total =
            state.mempool.rejected_fee_floor_total.saturating_add(1);
        return Err(PulseError::InvalidTransaction(format!(
            "fee below mempool fee floor: {} < {}",
            tx.fee, state.mempool.fee_floor
        )));
    }
    if let Err(e) = validate_transaction(&tx, state) {
        state.mempool.rejected_total = state.mempool.rejected_total.saturating_add(1);
        return Err(e);
    }
    if state.mempool.transactions.contains_key(&tx.txid) {
        state.mempool.rejected_total = state.mempool.rejected_total.saturating_add(1);
        return Err(PulseError::TxAlreadyExists);
    }

    if state.mempool.transactions.len() >= state.mempool.limit {
        let incoming_density = fee_density(&tx);
        let mut worst_density = f64::INFINITY;
        let mut worst_seq = u64::MAX;
        let mut worst_txid = None::<String>;
        for candidate in state.mempool.transactions.values() {
            let density = fee_density(candidate);
            let seq = state
                .mempool
                .tx_sequence
                .get(&candidate.txid)
                .copied()
                .unwrap_or(u64::MAX);
            if density < worst_density || (density == worst_density && seq < worst_seq) {
                worst_density = density;
                worst_seq = seq;
                worst_txid = Some(candidate.txid.clone());
            }
        }
        if incoming_density <= worst_density || worst_txid.is_none() {
            state.mempool.rejected_total = state.mempool.rejected_total.saturating_add(1);
            return Err(PulseError::InvalidTransaction(
                "mempool full and tx fee density too low".into(),
            ));
        }
        let _ = evict_lowest_fee_density(state);
    }

    for input in &tx.inputs {
        state
            .mempool
            .spent_outpoints
            .insert(input.previous_output.clone());
    }
    let now = current_ts();
    let seq = state.mempool.next_sequence;
    state.mempool.next_sequence = state.mempool.next_sequence.saturating_add(1);
    state.mempool.received_at_unix.insert(tx.txid.clone(), now);
    state.mempool.tx_sequence.insert(tx.txid.clone(), seq);
    state.mempool.transactions.insert(tx.txid.clone(), tx);
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
