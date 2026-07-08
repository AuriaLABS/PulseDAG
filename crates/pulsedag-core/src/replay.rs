use crate::{
    apply::apply_block, errors::PulseError, genesis::init_chain_state, state::ChainState,
    types::Block, validation::validate_block,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

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
