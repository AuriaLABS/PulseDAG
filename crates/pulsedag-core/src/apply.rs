use std::collections::{hash_map::Entry, BTreeSet};

use crate::{
    errors::PulseError,
    genesis::init_chain_state,
    ghostdag::classify_merge_set,
    mining::is_coinbase,
    ordering::{ordered_dag_tip, refresh_ordered_dag},
    selection::refresh_selected_chain,
    state::{ChainState, ConsensusMode, UtxoState},
    types::{Block, Hash, OutPoint, Transaction, Utxo},
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
    if state.dag.consensus_mode == ConsensusMode::GhostdagDev {
        accept_block_to_dag_metadata(block, state)?;
        refresh_selected_chain(state);
        refresh_ordered_dag(state);
        let rebuilt = match rebuild_state_from_ordered_dag(state) {
            Ok(rebuilt) => rebuilt,
            Err(err) => {
                state.dag.ordered_dag_rebuild_failed_total =
                    state.dag.ordered_dag_rebuild_failed_total.saturating_add(1);
                return Err(err);
            }
        };
        commit_rebuilt_state(state, rebuilt);
    } else {
        accept_block_to_dag_metadata(block, state)?;
        refresh_selected_chain(state);
        state.dag.ordered_dag = state.dag.selected_chain.clone();
        state.dag.ordering_version = "legacy".to_string();
        state.dag.ordered_dag_tip = state.dag.ordered_dag.last().cloned();

        let mut rebuilt = init_chain_state(state.chain_id.clone());
        rebuilt.dag.consensus_mode = state.dag.consensus_mode;
        rebuilt.dag.selected_parent_policy = state.dag.selected_parent_policy;
        for hash in &state.dag.selected_chain {
            if hash == &state.dag.genesis_hash {
                continue;
            }
            let selected_block = state.dag.blocks.get(hash).ok_or_else(|| {
                PulseError::Internal(format!("selected chain references missing block {hash}"))
            })?;
            for tx in &selected_block.transactions {
                apply_transaction(tx, &mut rebuilt, selected_block.header.height)?;
            }
            accept_block_to_dag_metadata(selected_block, &mut rebuilt)?;
            refresh_selected_chain(&mut rebuilt);
        }
        state.utxo = rebuilt.utxo;
        state.dag.ordered_dag_state_root = state.utxo.compute_state_root().ok();
    }
    Ok(())
}

pub fn accept_block_to_dag_metadata(
    block: &Block,
    state: &mut ChainState,
) -> Result<(), PulseError> {
    let ghostdag_metadata_active = state.dag.consensus_mode.ghostdag_metadata_active();
    let classification = classify_merge_set(block, state);
    let mut committed_block = block.clone();
    committed_block.header.blue_score = classification.blue_score;
    let block = &committed_block;
    let height = block.header.height;
    for parent in &block.header.parents {
        state.dag.tips.remove(parent);
        let children = state.dag.children.entry(parent.clone()).or_default();
        children.push(block.hash.clone());
        children.sort();
        children.dedup();
    }
    state.dag.tips.insert(block.hash.clone());
    state.dag.best_height = state.dag.best_height.max(height);
    let selected_parent = if ghostdag_metadata_active {
        classification.selected_parent.clone()
    } else {
        block
            .header
            .parents
            .iter()
            .filter(|parent| state.dag.blocks.contains_key(*parent))
            .max()
            .cloned()
    };
    state
        .dag
        .selected_parents
        .insert(block.hash.clone(), selected_parent);
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
    Ok(())
}

pub fn refresh_selected_chain_phase(state: &mut ChainState) {
    refresh_selected_chain(state);
}

pub fn refresh_ordered_dag_phase(state: &mut ChainState) {
    refresh_ordered_dag(state);
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderedDagRebuildDiagnostics {
    pub applied_transactions: usize,
    pub skipped_conflicting_transactions: usize,
    pub conflict_diagnostics: Vec<String>,
    pub state_root: String,
    pub ordered_dag_tip: Option<Hash>,
}

#[derive(Debug, Clone)]
pub struct OrderedDagRebuild {
    pub utxo: UtxoState,
    pub diagnostics: OrderedDagRebuildDiagnostics,
}

pub fn rebuild_state_from_ordered_dag(state: &ChainState) -> Result<OrderedDagRebuild, PulseError> {
    let mut rebuilt = init_chain_state(state.chain_id.clone());
    rebuilt.dag.consensus_mode = state.dag.consensus_mode;
    rebuilt.dag.selected_parent_policy = state.dag.selected_parent_policy;
    let mut diagnostics = OrderedDagRebuildDiagnostics {
        ordered_dag_tip: ordered_dag_tip(state),
        ..OrderedDagRebuildDiagnostics::default()
    };

    for hash in &state.dag.ordered_dag {
        if hash == &state.dag.genesis_hash {
            continue;
        }
        let block = state.dag.blocks.get(hash).ok_or_else(|| {
            PulseError::Internal(format!("ordered DAG references missing block {hash}"))
        })?;
        for tx in &block.transactions {
            match apply_transaction(tx, &mut rebuilt, block.header.height) {
                Ok(()) => diagnostics.applied_transactions += 1,
                Err(PulseError::UtxoNotFound | PulseError::DuplicateUtxoOutpoint(_)) => {
                    diagnostics.skipped_conflicting_transactions += 1;
                    diagnostics.conflict_diagnostics.push(format!(
                        "ordered_pos={} block={} tx={} skipped_conflict",
                        diagnostics.applied_transactions
                            + diagnostics.skipped_conflicting_transactions,
                        block.hash,
                        tx.txid
                    ));
                }
                Err(err) => return Err(err),
            }
        }
    }

    diagnostics.state_root = rebuilt.utxo.compute_state_root()?;
    Ok(OrderedDagRebuild {
        utxo: rebuilt.utxo,
        diagnostics,
    })
}

pub fn commit_rebuilt_state(state: &mut ChainState, rebuilt: OrderedDagRebuild) {
    state.utxo = rebuilt.utxo;
    state.dag.ordered_dag_rebuild_total = state.dag.ordered_dag_rebuild_total.saturating_add(1);
    state.dag.ordered_dag_state_root = Some(rebuilt.diagnostics.state_root);
    state.dag.ordered_dag_tip = rebuilt.diagnostics.ordered_dag_tip;
    state.dag.ordered_dag_conflict_diagnostics = rebuilt.diagnostics.conflict_diagnostics;
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

#[cfg(test)]
mod ordered_dag_state_rebuild_tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining::build_coinbase_transaction,
        state::{ConsensusMode, SelectedParentPolicy},
        types::BlockHeader,
    };

    fn ghostdag_state(chain_id: &str) -> ChainState {
        let mut state = init_chain_state(chain_id.to_string());
        state.dag.consensus_mode = ConsensusMode::GhostdagDev;
        state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
        state
    }

    fn block(hash: &str, parent: &str, height: u64, timestamp: u64, miner: &str) -> Block {
        let tx = build_coinbase_transaction(miner, 50, 0);
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: vec![parent.to_string()],
                timestamp,
                difficulty: 1,
                nonce: 0,
                merkle_root: crate::types::compute_merkle_root(std::slice::from_ref(&tx)),
                state_root: format!("state-{hash}"),
                blue_score: height,
                height,
            },
            transactions: vec![tx],
        }
    }

    fn accept_metadata_only(state: &mut ChainState, blocks: Vec<Block>) {
        for block in blocks {
            accept_block_to_dag_metadata(&block, state).unwrap();
            refresh_selected_chain_phase(state);
            refresh_ordered_dag_phase(state);
        }
    }

    #[test]
    fn ordered_dag_state_rebuild_is_independent_of_arrival_order() {
        let genesis = ghostdag_state("ordered-rebuild-a").dag.genesis_hash;
        let a = block("a", &genesis, 1, 20, "miner-a");
        let b = block("b", &genesis, 1, 10, "miner-b");

        let mut first = ghostdag_state("ordered-rebuild-a");
        accept_metadata_only(&mut first, vec![a.clone(), b.clone()]);
        let rebuilt_first = rebuild_state_from_ordered_dag(&first).unwrap();
        commit_rebuilt_state(&mut first, rebuilt_first);

        let mut second = ghostdag_state("ordered-rebuild-a");
        accept_metadata_only(&mut second, vec![b, a]);
        let rebuilt_second = rebuild_state_from_ordered_dag(&second).unwrap();
        commit_rebuilt_state(&mut second, rebuilt_second);

        assert_eq!(first.dag.ordered_dag, second.dag.ordered_dag);
        assert_eq!(
            first.dag.ordered_dag_state_root,
            second.dag.ordered_dag_state_root
        );
        assert_eq!(
            first.utxo.compute_state_root().unwrap(),
            second.utxo.compute_state_root().unwrap()
        );
    }

    #[test]
    fn ordered_dag_rebuild_skips_conflicting_transaction_deterministically() {
        let genesis = ghostdag_state("ordered-rebuild-conflict").dag.genesis_hash;
        let first = block("a", &genesis, 1, 20, "same-miner");
        let second = block("b", &genesis, 1, 10, "same-miner");
        assert_eq!(first.transactions[0].txid, second.transactions[0].txid);

        let mut state = ghostdag_state("ordered-rebuild-conflict");
        accept_metadata_only(&mut state, vec![first, second]);
        let rebuilt = rebuild_state_from_ordered_dag(&state).unwrap();

        assert_eq!(rebuilt.diagnostics.applied_transactions, 1);
        assert_eq!(rebuilt.diagnostics.skipped_conflicting_transactions, 1);
        assert_eq!(rebuilt.diagnostics.conflict_diagnostics.len(), 1);
    }
}
