use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    accept::{accept_block, AcceptSource},
    state::ChainState,
    types::{Block, Hash},
};

pub const DEFAULT_ORPHAN_MAX_COUNT: usize = 512;
pub const DEFAULT_ORPHAN_MAX_AGE_MS: u64 = 15 * 60 * 1000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        .filter_map(|(hash, received_at)| {
            if now.saturating_sub(*received_at) > max_age_ms {
                Some(hash.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for hash in expired {
        if state.orphan_blocks.remove(&hash).is_some() {
            removed += 1;
        }
        state.orphan_missing_parents.remove(&hash);
        state.orphan_received_at_ms.remove(&hash);
    }

    if state.orphan_blocks.len() > max_count {
        let mut oldest = state
            .orphan_received_at_ms
            .iter()
            .map(|(hash, ts)| (hash.clone(), *ts))
            .collect::<Vec<_>>();
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

pub fn queue_orphan_block(
    state: &mut ChainState,
    block: Block,
    missing_parents: Vec<Hash>,
) -> bool {
    let hash = block.hash.clone();
    let inserted = state.orphan_blocks.insert(hash.clone(), block).is_none();
    state
        .orphan_missing_parents
        .insert(hash.clone(), missing_parents);
    state.orphan_received_at_ms.insert(hash, now_ms());
    let _ = prune_orphans(state, DEFAULT_ORPHAN_MAX_COUNT, DEFAULT_ORPHAN_MAX_AGE_MS);
    inserted
}

pub fn adopt_ready_orphans(state: &mut ChainState, source: AcceptSource) -> usize {
    let mut adopted = 0usize;
    loop {
        let ready = state.orphan_blocks.iter().find_map(|(hash, block)| {
            let missing = missing_block_parents(block, state);
            if missing.is_empty() {
                Some(hash.clone())
            } else {
                None
            }
        });

        let Some(hash) = ready else { break };
        let Some(block) = state.orphan_blocks.remove(&hash) else {
            continue;
        };
        state.orphan_missing_parents.remove(&hash);
        state.orphan_received_at_ms.remove(&hash);
        if let Ok(()) = accept_block(block, state, source) {
            adopted += 1;
        }
    }
    adopted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accept::accept_block_with_result,
        genesis::init_chain_state,
        mining::{build_candidate_block, build_coinbase_transaction},
    };

    #[test]
    fn child_before_parent_is_queued_and_adopted_when_parent_arrives() {
        let mut state = init_chain_state("test".into());
        let genesis = state.dag.genesis_hash.clone();

        let mut parent = build_candidate_block(
            vec![genesis.clone()],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 1)],
        );
        parent.hash = "parent-1".into();

        let mut child = build_candidate_block(
            vec![parent.hash.clone()],
            2,
            1,
            vec![build_coinbase_transaction("miner", 50, 2)],
        );
        child.hash = "child-1".into();

        assert_eq!(
            accept_block_with_result(child.clone(), &mut state, AcceptSource::P2p),
            crate::accept::BlockAcceptanceResult::UnknownParent
        );
        let missing = missing_block_parents(&child, &state);
        assert_eq!(missing, vec![parent.hash.clone()]);
        assert!(queue_orphan_block(&mut state, child.clone(), missing));
        assert!(state.orphan_blocks.contains_key(&child.hash));

        assert!(crate::accept::accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);
        assert_eq!(adopted, 1);
        assert!(state.dag.blocks.contains_key(&child.hash));
        assert!(!state.orphan_blocks.contains_key(&child.hash));
    }

    #[test]
    fn invalid_pow_orphan_never_adopts() {
        let mut state = init_chain_state("test".into());
        let genesis = state.dag.genesis_hash.clone();

        let mut parent = build_candidate_block(
            vec![genesis],
            1,
            1,
            vec![build_coinbase_transaction("miner", 50, 1)],
        );
        parent.hash = "pow-parent".into();

        let mut child = build_candidate_block(
            vec![parent.hash.clone()],
            2,
            u32::MAX,
            vec![build_coinbase_transaction("miner", 50, 2)],
        );
        child.hash = "pow-child-invalid".into();
        child.header.nonce = 0;

        let missing = missing_block_parents(&child, &state);
        assert!(queue_orphan_block(&mut state, child.clone(), missing));
        assert!(crate::accept::accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);
        assert_eq!(adopted, 0);
        assert!(!state.dag.blocks.contains_key(&child.hash));
    }

    #[test]
    fn orphan_capacity_limit_evicts_oldest() {
        let mut state = init_chain_state("test".into());

        for i in 0..3 {
            let parent_hash = format!("missing-parent-{i}");
            let mut orphan = build_candidate_block(
                vec![parent_hash],
                1,
                1,
                vec![build_coinbase_transaction("miner", 50, i + 1)],
            );
            orphan.hash = format!("orphan-{i}");
            queue_orphan_block(&mut state, orphan, vec![format!("missing-parent-{i}")]);
        }
        let removed = prune_orphans(&mut state, 2, u64::MAX);
        assert_eq!(removed, 1);
        assert_eq!(state.orphan_blocks.len(), 2);

        let remaining = ["orphan-0", "orphan-1", "orphan-2"]
            .into_iter()
            .filter(|hash| state.orphan_blocks.contains_key(*hash))
            .count();
        assert_eq!(remaining, 2);
    }
}
