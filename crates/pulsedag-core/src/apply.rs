use crate::{
    errors::PulseError,
    mining::is_coinbase,
    state::ChainState,
    types::{Block, OutPoint, Transaction, Utxo},
};

pub fn apply_transaction(
    tx: &Transaction,
    state: &mut ChainState,
    height: u64,
) -> Result<(), PulseError> {
    if !is_coinbase(tx) {
        for input in &tx.inputs {
            let spent = state
                .utxo
                .utxos
                .remove(&input.previous_output)
                .ok_or(PulseError::UtxoNotFound)?;
            if let Some(entries) = state.utxo.address_index.get_mut(&spent.address) {
                entries.retain(|op| op != &input.previous_output);
            }
        }
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
            coinbase: is_coinbase(tx),
            height,
        };
        state.utxo.utxos.insert(outpoint.clone(), utxo);
        state
            .utxo
            .address_index
            .entry(output.address.clone())
            .or_default()
            .push(outpoint);
    }

    state.mempool.transactions.remove(&tx.txid);
    for input in &tx.inputs {
        state.mempool.spent_outpoints.remove(&input.previous_output);
    }

    Ok(())
}

pub fn apply_block(block: &Block, state: &mut ChainState) -> Result<(), PulseError> {
    let height = block.header.height;
    for tx in &block.transactions {
        apply_transaction(tx, state, height)?;
    }
    for parent in &block.header.parents {
        state.dag.tips.remove(parent);
        state
            .dag
            .children
            .entry(parent.clone())
            .or_default()
            .push(block.hash.clone());
    }
    state.dag.tips.insert(block.hash.clone());
    state.dag.best_height = state.dag.best_height.max(height);
    state.dag.blocks.insert(block.hash.clone(), block.clone());
    Ok(())
}
