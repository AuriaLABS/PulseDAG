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

pub fn rebuild_state_from_blocks(
    chain_id: String,
    mut blocks: Vec<Block>,
) -> Result<ChainState, PulseError> {
    if blocks.is_empty() {
        return Ok(init_chain_state(chain_id));
    }

    blocks.sort_by_key(|b| b.header.height);
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

    let snapshot_height = snapshot.dag.best_height;
    let snapshot_hashes = &snapshot.dag.blocks;
    blocks.retain(|block| {
        block.header.height > snapshot_height
            && !snapshot_hashes.contains_key(&block.hash)
            && block.hash != snapshot.dag.genesis_hash
    });
    if blocks.is_empty() {
        return Ok(snapshot);
    }

    blocks.sort_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
            .then_with(|| a.hash.cmp(&b.hash))
    });

    let mut state = snapshot;

    for block in blocks.into_iter() {
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

    blocks.sort_by(|a, b| {
        a.header
            .height
            .cmp(&b.header.height)
            .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
            .then_with(|| a.hash.cmp(&b.hash))
    });
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
