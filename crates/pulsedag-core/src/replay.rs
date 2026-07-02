use crate::{
    apply::apply_block, errors::PulseError, genesis::init_chain_state, state::ChainState,
    types::Block, validation::validate_block,
};

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
