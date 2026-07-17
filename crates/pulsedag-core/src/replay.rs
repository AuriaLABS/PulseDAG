use crate::{
    apply::apply_block, errors::PulseError, genesis::init_chain_state, state::ChainState,
    types::Block, validation::validate_block,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

fn digest_parts(domain: &str, parts: impl IntoIterator<Item = String>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    for part in parts {
        hasher.update([0]);
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Digest of selected-parent metadata keyed by block hash.
pub fn selection_digest(state: &ChainState) -> String {
    let ordered = state
        .dag
        .selected_parents
        .iter()
        .map(|(block, parent)| (block.clone(), parent.clone().unwrap_or_default()))
        .collect::<BTreeMap<_, _>>();
    digest_parts(
        "PulseDAG:selection-digest:v1",
        ordered
            .into_iter()
            .map(|(block, parent)| format!("{block}->{parent}")),
    )
}

/// Digest of blue/red merge-set membership keyed by block hash.
pub fn merge_set_digest(state: &ChainState) -> String {
    let mut ordered = BTreeMap::new();
    for block in state.dag.blocks.keys() {
        let mut blues = state
            .dag
            .merge_set_blues
            .get(block)
            .cloned()
            .unwrap_or_default();
        let mut reds = state
            .dag
            .merge_set_reds
            .get(block)
            .cloned()
            .unwrap_or_default();
        blues.sort();
        reds.sort();
        ordered.insert(block.clone(), (blues, reds));
    }
    digest_parts(
        "PulseDAG:merge-set-digest:v1",
        ordered.into_iter().map(|(block, (blues, reds))| {
            format!("{block}|B:{}|R:{}", blues.join(","), reds.join(","))
        }),
    )
}

/// Digest of the deterministic ordered-DAG vector.
pub fn ordered_dag_digest(state: &ChainState) -> String {
    digest_parts(
        "PulseDAG:ordered-dag-digest:v1",
        state
            .dag
            .ordered_dag
            .iter()
            .enumerate()
            .map(|(index, hash)| format!("{index}:{hash}")),
    )
}

/// Digest of the canonical UTXO/state root.
pub fn state_digest(state: &ChainState) -> Result<String, PulseError> {
    Ok(digest_parts(
        "PulseDAG:state-digest:v1",
        [state.utxo.compute_state_root()?],
    ))
}

#[derive(Debug, Clone)]
pub struct ReplayDefensiveReport {
    pub state: ChainState,
    pub accepted_blocks: usize,
    pub skipped_blocks: usize,
    pub skipped_hashes: Vec<String>,
    pub skipped_reasons: Vec<String>,
}

pub fn sort_blocks_for_deterministic_replay(blocks: &mut [Block]) {
    blocks.sort_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
            .then_with(|| a.hash.cmp(&b.hash))
    });
}

pub fn rebuild_state_from_blocks(
    chain_id: String,
    mut blocks: Vec<Block>,
) -> Result<ChainState, PulseError> {
    if blocks.is_empty() {
        return Ok(init_chain_state(chain_id));
    }

    sort_blocks_for_deterministic_replay(&mut blocks);
    let mut state = init_chain_state(chain_id);

    for block in blocks.into_iter() {
        if block.hash == state.dag.genesis_hash {
            continue;
        }
        validate_block(&block, &state)?;
        apply_block(&block, &mut state)?;
    }

    Ok(state)
}

/// Compact a fully materialized snapshot so its in-memory DAG retained set
/// exactly matches the blocks preserved by storage pruning.
///
/// The compact form keeps accepted block bytes unchanged. Historical parents
/// below the retained floor remain committed by the boundary headers and are
/// described by snapshot metadata in the storage layer.
pub fn compact_snapshot_to_retained_blocks(
    mut snapshot: ChainState,
    retained_blocks: &[Block],
) -> Result<ChainState, PulseError> {
    if retained_blocks.is_empty() {
        return Err(PulseError::StorageError(
            "cannot compact a snapshot to an empty retained set".to_string(),
        ));
    }

    let retained = retained_blocks
        .iter()
        .map(|block| block.hash.clone())
        .collect::<HashSet<_>>();
    if retained.len() != retained_blocks.len() {
        return Err(PulseError::StorageError(
            "retained block set contains duplicate hashes".to_string(),
        ));
    }
    for block in retained_blocks {
        let Some(existing) = snapshot.dag.blocks.get(&block.hash) else {
            return Err(PulseError::StorageError(format!(
                "retained block {} is absent from the source snapshot",
                block.hash
            )));
        };
        let existing_bytes = serde_json::to_vec(existing)
            .map_err(|error| PulseError::StorageError(error.to_string()))?;
        let retained_bytes = serde_json::to_vec(block)
            .map_err(|error| PulseError::StorageError(error.to_string()))?;
        if existing_bytes != retained_bytes {
            return Err(PulseError::StorageError(format!(
                "retained block {} differs from the source snapshot",
                block.hash
            )));
        }
    }

    let boundary_height = retained_blocks
        .iter()
        .map(|block| block.header.height)
        .min()
        .unwrap_or(0);
    let source_floor = snapshot
        .dag
        .blocks
        .values()
        .map(|block| block.header.height)
        .min()
        .unwrap_or(0);
    for block in retained_blocks {
        for parent in &block.header.parents {
            if retained.contains(parent) {
                continue;
            }
            let source_contains_historical_parent = snapshot
                .dag
                .blocks
                .get(parent)
                .is_some_and(|parent_block| parent_block.header.height < boundary_height);
            let inherited_compact_boundary =
                !snapshot.dag.blocks.contains_key(&snapshot.dag.genesis_hash)
                    && source_floor > 0
                    && block.header.height == source_floor
                    && source_floor == boundary_height;
            if block.header.height != boundary_height
                || (!source_contains_historical_parent && !inherited_compact_boundary)
            {
                return Err(PulseError::StorageError(format!(
                    "retained block {} at height {} has invalid compact-boundary parent {}",
                    block.hash, block.header.height, parent
                )));
            }
        }
    }

    snapshot.dag.blocks = retained_blocks
        .iter()
        .map(|block| (block.hash.clone(), block.clone()))
        .collect();
    snapshot.dag.children.clear();
    for block in retained_blocks {
        for parent in &block.header.parents {
            if retained.contains(parent) {
                snapshot
                    .dag
                    .children
                    .entry(parent.clone())
                    .or_default()
                    .push(block.hash.clone());
            }
        }
    }
    for children in snapshot.dag.children.values_mut() {
        children.sort();
        children.dedup();
    }
    let parents_with_children = snapshot
        .dag
        .children
        .iter()
        .filter(|(_, children)| !children.is_empty())
        .map(|(parent, _)| parent.clone())
        .collect::<HashSet<_>>();
    snapshot.dag.tips = retained
        .difference(&parents_with_children)
        .cloned()
        .collect();

    snapshot.dag.selected_parents.retain(|hash, parent| {
        if !retained.contains(hash) {
            return false;
        }
        if parent
            .as_ref()
            .is_some_and(|value| !retained.contains(value))
        {
            *parent = None;
        }
        true
    });
    snapshot
        .dag
        .selected_chain
        .retain(|hash| retained.contains(hash));
    snapshot.dag.merge_set_blues.retain(|hash, values| {
        if !retained.contains(hash) {
            return false;
        }
        values.retain(|value| retained.contains(value));
        true
    });
    snapshot.dag.merge_set_reds.retain(|hash, values| {
        if !retained.contains(hash) {
            return false;
        }
        values.retain(|value| retained.contains(value));
        true
    });
    snapshot
        .dag
        .blue_work
        .retain(|hash, _| retained.contains(hash));
    snapshot
        .dag
        .merge_set_diagnostics
        .retain(|hash, diagnostics| {
            if !retained.contains(hash) {
                return false;
            }
            if diagnostics
                .selected_parent
                .as_ref()
                .is_some_and(|parent| !retained.contains(parent))
            {
                diagnostics.selected_parent = None;
            }
            true
        });
    for (hash, diagnostics) in &mut snapshot.dag.merge_set_diagnostics {
        let blue_count = snapshot
            .dag
            .merge_set_blues
            .get(hash)
            .map(Vec::len)
            .unwrap_or(0);
        let red_count = snapshot
            .dag
            .merge_set_reds
            .get(hash)
            .map(Vec::len)
            .unwrap_or(0);
        diagnostics.merge_set_blues_count = blue_count;
        diagnostics.merge_set_reds_count = red_count;
        diagnostics.merge_set_size = blue_count.saturating_add(red_count);
    }
    snapshot
        .dag
        .ordered_dag
        .retain(|hash| retained.contains(hash));
    if snapshot
        .dag
        .ordered_dag_tip
        .as_ref()
        .is_some_and(|hash| !retained.contains(hash))
    {
        return Err(PulseError::StorageError(
            "ordered DAG tip would be removed by snapshot compaction".to_string(),
        ));
    }
    snapshot.dag.ordered_dag_conflict_diagnostics.clear();
    snapshot.orphan_blocks.clear();
    snapshot.orphan_missing_parents.clear();
    snapshot.orphan_parent_index.clear();
    snapshot.orphan_received_at_ms.clear();
    snapshot.terminal_missing_parents.clear();

    let max_height = snapshot
        .dag
        .blocks
        .values()
        .map(|block| block.header.height)
        .max()
        .unwrap_or(0);
    if max_height != snapshot.dag.best_height {
        return Err(PulseError::StorageError(format!(
            "compacted snapshot max height {} does not match best height {}",
            max_height, snapshot.dag.best_height
        )));
    }
    let issues = crate::consistency::dag_consistency_issues(&snapshot);
    if !issues.is_empty() {
        return Err(PulseError::StorageError(format!(
            "compacted snapshot DAG is inconsistent: {}",
            issues.join("; ")
        )));
    }
    Ok(snapshot)
}

pub fn rebuild_state_from_snapshot_and_blocks(
    snapshot: ChainState,
    mut blocks: Vec<Block>,
) -> Result<ChainState, PulseError> {
    if blocks.is_empty() {
        return Ok(snapshot);
    }

    sort_blocks_for_deterministic_replay(&mut blocks);

    let snapshot_height = snapshot.dag.best_height;
    let mut state = snapshot;

    for block in blocks.into_iter() {
        if block.hash == state.dag.genesis_hash {
            continue;
        }
        if block.header.height <= snapshot_height {
            continue;
        }
        if state.dag.blocks.contains_key(&block.hash) {
            continue;
        }
        validate_block(&block, &state)?;
        apply_block(&block, &mut state)?;
    }

    Ok(state)
}

pub fn rebuild_state_from_blocks_defensive(
    chain_id: String,
    mut blocks: Vec<Block>,
) -> ReplayDefensiveReport {
    if blocks.is_empty() {
        return ReplayDefensiveReport {
            state: init_chain_state(chain_id),
            accepted_blocks: 0,
            skipped_blocks: 0,
            skipped_hashes: Vec::new(),
            skipped_reasons: Vec::new(),
        };
    }

    sort_blocks_for_deterministic_replay(&mut blocks);
    let mut state = init_chain_state(chain_id);
    let mut accepted_blocks = 0usize;
    let mut skipped_hashes = Vec::new();
    let mut skipped_reasons = Vec::new();

    for block in blocks.into_iter() {
        if block.hash == state.dag.genesis_hash {
            continue;
        }
        match validate_block(&block, &state).and_then(|_| apply_block(&block, &mut state)) {
            Ok(_) => accepted_blocks += 1,
            Err(err) => {
                skipped_hashes.push(block.hash.clone());
                skipped_reasons.push(format!("{}: {}", block.hash, err));
            }
        }
    }

    let skipped_blocks = skipped_hashes.len();
    ReplayDefensiveReport {
        state,
        accepted_blocks,
        skipped_blocks,
        skipped_hashes,
        skipped_reasons,
    }
}

#[cfg(test)]
mod dag_ordering_replay_tests {
    use crate::{
        genesis::init_chain_state,
        ordering::refresh_ordered_dag,
        types::{Block, BlockHeader},
    };

    fn block(hash: &str, parent: &str, height: u64, timestamp: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: vec![parent.to_string()],
                timestamp,
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("m-{hash}"),
                state_root: format!("s-{hash}"),
                blue_score: height,
                height,
            },
            transactions: vec![],
        }
    }

    #[test]
    fn compact_snapshot_matches_retained_block_set() {
        let mut state = init_chain_state("compact-snapshot".to_string());
        let genesis = state.dag.genesis_hash.clone();
        let a = block("a", &genesis, 1, 10);
        let b = block("b", "a", 2, 11);
        let c = block("c", "b", 3, 12);
        for item in [a.clone(), b.clone(), c.clone()] {
            state
                .dag
                .selected_parents
                .insert(item.hash.clone(), item.header.parents.first().cloned());
            state.dag.selected_chain.push(item.hash.clone());
            state.dag.ordered_dag.push(item.hash.clone());
            state.dag.blocks.insert(item.hash.clone(), item);
        }
        state
            .dag
            .children
            .insert("a".to_string(), vec!["b".to_string()]);
        state
            .dag
            .children
            .insert("b".to_string(), vec!["c".to_string()]);
        state.dag.tips = ["c".to_string()].into_iter().collect();
        state.dag.best_height = 3;
        state.dag.ordered_dag_tip = Some("c".to_string());

        let compact = super::compact_snapshot_to_retained_blocks(state, &[b, c])
            .expect("compact retained snapshot");
        let hashes = compact
            .dag
            .blocks
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            hashes,
            ["b".to_string(), "c".to_string()].into_iter().collect()
        );
        assert_eq!(compact.dag.selected_chain, vec!["b", "c"]);
        assert_eq!(compact.dag.ordered_dag, vec!["b", "c"]);
        assert_eq!(compact.dag.selected_parents.get("b"), Some(&None));
        assert_eq!(compact.dag.children.get("b"), Some(&vec!["c".to_string()]));
        assert!(!compact.dag.children.contains_key("a"));
        assert!(crate::consistency::dag_consistency_issues(&compact).is_empty());
    }

    #[test]
    fn replay_order_independence_keeps_same_ordered_dag() {
        let genesis = init_chain_state("replay-ordering".to_string())
            .dag
            .genesis_hash;
        let a = block("a", &genesis, 1, 10);
        let b = block("b", "a", 2, 11);
        let mut forward = vec![a.clone(), b.clone()];
        let mut reverse = vec![b, a];
        super::sort_blocks_for_deterministic_replay(&mut forward);
        super::sort_blocks_for_deterministic_replay(&mut reverse);
        assert_eq!(
            forward.iter().map(|b| &b.hash).collect::<Vec<_>>(),
            reverse.iter().map(|b| &b.hash).collect::<Vec<_>>()
        );

        let mut state = init_chain_state("replay-ordering".to_string());
        for block in forward {
            state
                .dag
                .selected_parents
                .insert(block.hash.clone(), block.header.parents.first().cloned());
            state.dag.selected_chain.push(block.hash.clone());
            state.dag.blocks.insert(block.hash.clone(), block);
        }
        refresh_ordered_dag(&mut state);
        assert_eq!(state.dag.selected_chain, state.dag.ordered_dag);
    }
}
