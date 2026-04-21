use crate::{apply::apply_block, dev_pow_accepts, errors::PulseError, selected_pow_name, state::ChainState, types::{Block, Transaction}, validation::{validate_block, validate_transaction}};

#[derive(Debug, Clone, Copy)]
pub enum AcceptSource {
    Rpc,
    P2p,
    LocalMining,
}

pub fn accept_transaction(tx: Transaction, state: &mut ChainState, _source: AcceptSource) -> Result<(), PulseError> {
    validate_transaction(&tx, state)?;
    if state.mempool.transactions.contains_key(&tx.txid) {
        return Err(PulseError::TxAlreadyExists);
    }
    for input in &tx.inputs {
        state.mempool.spent_outpoints.insert(input.previous_output.clone());
    }
    state.mempool.transactions.insert(tx.txid.clone(), tx);
    Ok(())
}

pub fn accept_block(block: Block, state: &mut ChainState, source: AcceptSource) -> Result<(), PulseError> {
    validate_block(&block, state)?;

    let enforce_pow = matches!(source, AcceptSource::Rpc | AcceptSource::P2p | AcceptSource::LocalMining);
    if enforce_pow && !dev_pow_accepts(&block.header) {
        return Err(PulseError::InvalidBlock(format!("pow rejected by current {} policy", selected_pow_name())));
    }

    apply_block(&block, state)?;
    Ok(())
}
