use std::time::{SystemTime, UNIX_EPOCH};

use crate::{accept::{accept_block, AcceptSource}, state::ChainState, types::{Block, Hash}};

pub const DEFAULT_ORPHAN_MAX_COUNT: usize = 512;
pub const DEFAULT_ORPHAN_MAX_AGE_MS: u64 = 15 * 60 * 1000;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn missing_block_parents(block: &Block, state: &ChainState) -> Vec<Hash> {
    block
        .header
        .parents
        .iter()
        .filter(|parent| !state.dag.blocks.contains_key(*parent))
        .cloned()
        .collect()
}

pub fn prune_orphans(state: &mut ChainState, max_count: usize, max_age_ms: u64) -> usize {
    let now = now_ms();
    let mut removed = 0usize;

    let expired = state
        .orphan_received_at_ms
        .iter()
        .filter_map(|(hash, received_at)| if now.saturating_sub(*received_at) > max_age_ms { Some(hash.clone()) } else { None })
        .collect::<Vec<_>>();
    for hash in expired {
        if state.orphan_blocks.remove(&hash).is_some() {
            removed += 1;
        }
        state.orphan_missing_parents.remove(&hash);
        state.orphan_received_at_ms.remove(&hash);
    }

    if state.orphan_blocks.len() > max_count {
        let mut oldest = state.orphan_received_at_ms.iter().map(|(hash, ts)| (hash.clone(), *ts)).collect::<Vec<_>>();
        oldest.sort_by_key(|(_, ts)| *ts);
        let overflow = state.orphan_blocks.len().saturating_sub(max_count);
        for (hash, _) in oldest.into_iter().take(overflow) {
            if state.orphan_blocks.remove(&hash).is_some() {
                removed += 1;
            }
            state.orphan_missing_parents.remove(&hash);
            state.orphan_received_at_ms.remove(&hash);
        }
    }

    removed
}

pub fn queue_orphan_block(state: &mut ChainState, block: Block, missing_parents: Vec<Hash>) -> bool {
    let hash = block.hash.clone();
    let inserted = state.orphan_blocks.insert(hash.clone(), block).is_none();
    state.orphan_missing_parents.insert(hash.clone(), missing_parents);
    state.orphan_received_at_ms.insert(hash, now_ms());
    let _ = prune_orphans(state, DEFAULT_ORPHAN_MAX_COUNT, DEFAULT_ORPHAN_MAX_AGE_MS);
    inserted
}

pub fn adopt_ready_orphans(state: &mut ChainState, source: AcceptSource) -> usize {
    let mut adopted = 0usize;
    loop {
        let ready = state
            .orphan_blocks
            .iter()
            .find_map(|(hash, block)| {
                let missing = missing_block_parents(block, state);
                if missing.is_empty() { Some(hash.clone()) } else { None }
            });

        let Some(hash) = ready else { break };
        let Some(block) = state.orphan_blocks.remove(&hash) else { continue };
        state.orphan_missing_parents.remove(&hash);
        state.orphan_received_at_ms.remove(&hash);
        if let Ok(()) = accept_block(block, state, source) {
            adopted += 1;
        }
    }
    adopted
}
