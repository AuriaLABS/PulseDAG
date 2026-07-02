use std::collections::{hash_map::Entry, BTreeSet};

use crate::{
    errors::PulseError,
    ghostdag::classify_merge_set,
    mining::is_coinbase,
    ordering::refresh_ordered_dag,
    selection::refresh_selected_chain,
    state::ChainState,
    types::{Block, OutPoint, Transaction, Utxo},
    validation::validate_block,
};

fn outpoint_label(outpoint: &OutPoint) -> String {
    format!("{}:{}", outpoint.txid, outpoint.index)
}

pub fn apply_transaction(
    tx: &Transaction,
    state: &mut ChainState,
    height: u64,
) -> Result<(), PulseError> {
    let mut created_outpoints = Vec::with_capacity(tx.outputs.len());
    let mut seen_created_outpoints = BTreeSet::new();
    for (index, _) in tx.outputs.iter().enumerate() {
        let outpoint = OutPoint {
            txid: tx.txid.clone(),
            index: index as u32,
        };
        if !seen_created_outpoints.insert(outpoint.clone())
            || state.utxo.utxos.contains_key(&outpoint)
        {
            return Err(PulseError::DuplicateUtxoOutpoint(outpoint_label(&outpoint)));
        }
        created_outpoints.push(outpoint);
    }

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

    for (outpoint, output) in created_outpoints.into_iter().zip(&tx.outputs) {
        let utxo = Utxo {
            outpoint: outpoint.clone(),
            address: output.address.clone(),
            amount: output.amount,
            coinbase: is_coinbase(tx),
            height,
        };
        match state.utxo.utxos.entry(outpoint.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(utxo);
            }
            Entry::Occupied(_) => {
                return Err(PulseError::DuplicateUtxoOutpoint(outpoint_label(&outpoint)));
            }
        }
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

pub fn commit_block_to_state(block: &Block, state: &mut ChainState) -> Result<(), PulseError> {
    let classification = classify_merge_set(block, state);
    let mut committed_block = block.clone();
    committed_block.header.blue_score = classification.blue_score;
    let block = &committed_block;
    let height = block.header.height;
    for tx in &block.transactions {
        apply_transaction(tx, state, height)?;
    }
    for parent in &block.header.parents {
        state.dag.tips.remove(parent);
        let children = state.dag.children.entry(parent.clone()).or_default();
        children.push(block.hash.clone());
        children.sort();
        children.dedup();
    }
    state.dag.tips.insert(block.hash.clone());
    state.dag.best_height = state.dag.best_height.max(height);
    state
        .dag
        .selected_parents
        .insert(block.hash.clone(), classification.selected_parent.clone());
    state
        .dag
        .merge_set_blues
        .insert(block.hash.clone(), classification.blues.clone());
    state
        .dag
        .merge_set_reds
        .insert(block.hash.clone(), classification.reds.clone());
    state
        .dag
        .blue_work
        .insert(block.hash.clone(), classification.blue_work);
    state
        .dag
        .merge_set_diagnostics
        .insert(block.hash.clone(), classification.diagnostics.clone());
    state.dag.blocks.insert(block.hash.clone(), block.clone());
    refresh_selected_chain(state);
    refresh_ordered_dag(state);
    Ok(())
}

pub fn prepare_block_state(block: &Block, state: &ChainState) -> Result<ChainState, PulseError> {
    validate_block(block, state)?;

    let mut working = state.clone();
    commit_block_to_state(block, &mut working)?;
    Ok(working)
}

pub fn apply_block(block: &Block, state: &mut ChainState) -> Result<(), PulseError> {
    let working = prepare_block_state(block, state)?;
    *state = working;
    Ok(())
}
