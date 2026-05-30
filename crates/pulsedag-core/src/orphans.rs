use std::{
    collections::{BTreeSet, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    accept::{accept_block_with_result, AcceptSource, BlockAcceptanceResult},
    state::ChainState,
    types::{Block, Hash},
};

pub const DEFAULT_ORPHAN_MAX_COUNT: usize = 512;
pub const DEFAULT_ORPHAN_MAX_AGE_MS: u64 = 15 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanQueueResult {
    pub queued: bool,
    pub evicted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanAdoptionResult {
    pub accepted: usize,
    pub rejected: usize,
    pub retried: usize,
}

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
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn index_missing_parent(state: &mut ChainState, orphan_hash: &Hash, parent_hash: &Hash) {
    let children = state
        .orphan_missing_parent_index
        .entry(parent_hash.clone())
        .or_default();
    if !children.contains(orphan_hash) {
        children.push(orphan_hash.clone());
        children.sort();
    }
}

fn unindex_missing_parent(state: &mut ChainState, orphan_hash: &Hash, parent_hash: &Hash) {
    if let Some(children) = state.orphan_missing_parent_index.get_mut(parent_hash) {
        children.retain(|child| child != orphan_hash);
        if children.is_empty() {
            state.orphan_missing_parent_index.remove(parent_hash);
        }
    }
}

fn remove_orphan(state: &mut ChainState, hash: &Hash) -> Option<Block> {
    let block = state.orphan_blocks.remove(hash)?;
    if let Some(missing) = state.orphan_missing_parents.remove(hash) {
        for parent in missing {
            unindex_missing_parent(state, hash, &parent);
        }
    }
    state.orphan_received_at_ms.remove(hash);
    Some(block)
}

fn set_missing_parents(state: &mut ChainState, hash: &Hash, missing_parents: Vec<Hash>) {
    if let Some(previous) = state
        .orphan_missing_parents
        .insert(hash.clone(), missing_parents.clone())
    {
        for parent in previous {
            unindex_missing_parent(state, hash, &parent);
        }
    }
    for parent in missing_parents {
        index_missing_parent(state, hash, &parent);
    }
}

pub fn rebuild_missing_parent_index(state: &mut ChainState) {
    state.orphan_missing_parent_index.clear();
    for (orphan_hash, missing_parents) in state.orphan_missing_parents.clone() {
        if state.orphan_blocks.contains_key(&orphan_hash) {
            for parent in missing_parents {
                index_missing_parent(state, &orphan_hash, &parent);
            }
        }
    }
}

pub fn orphans_waiting_for_parent(state: &ChainState, parent_hash: &Hash) -> Vec<Hash> {
    state
        .orphan_missing_parent_index
        .get(parent_hash)
        .cloned()
        .unwrap_or_default()
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
        if remove_orphan(state, &hash).is_some() {
            removed += 1;
        }
    }

    if state.orphan_blocks.len() > max_count {
        let mut oldest = state
            .orphan_received_at_ms
            .iter()
            .map(|(hash, ts)| (hash.clone(), *ts))
            .collect::<Vec<_>>();
        oldest.sort_by(|(left_hash, left_ts), (right_hash, right_ts)| {
            left_ts
                .cmp(right_ts)
                .then_with(|| left_hash.cmp(right_hash))
        });
        let overflow = state.orphan_blocks.len().saturating_sub(max_count);
        for (hash, _) in oldest.into_iter().take(overflow) {
            if remove_orphan(state, &hash).is_some() {
                removed += 1;
            }
        }
    }

    removed
}

pub fn queue_orphan_block_bounded(
    state: &mut ChainState,
    block: Block,
    missing_parents: Vec<Hash>,
    max_count: usize,
    max_age_ms: u64,
) -> OrphanQueueResult {
    let hash = block.hash.clone();
    if state.orphan_blocks.contains_key(&hash) {
        return OrphanQueueResult {
            queued: false,
            evicted: 0,
        };
    }
    state.orphan_blocks.insert(hash.clone(), block);
    state.orphan_received_at_ms.insert(hash.clone(), now_ms());
    set_missing_parents(state, &hash, missing_parents);
    let evicted = prune_orphans(state, max_count, max_age_ms);
    OrphanQueueResult {
        queued: state.orphan_blocks.contains_key(&hash),
        evicted,
    }
}

pub fn queue_orphan_block(
    state: &mut ChainState,
    block: Block,
    missing_parents: Vec<Hash>,
) -> bool {
    queue_orphan_block_bounded(
        state,
        block,
        missing_parents,
        DEFAULT_ORPHAN_MAX_COUNT,
        DEFAULT_ORPHAN_MAX_AGE_MS,
    )
    .queued
}

pub fn adopt_ready_orphans_with_result(
    state: &mut ChainState,
    source: AcceptSource,
    arrived_parent: Option<&Hash>,
) -> OrphanAdoptionResult {
    if state.orphan_missing_parent_index.is_empty() && !state.orphan_missing_parents.is_empty() {
        rebuild_missing_parent_index(state);
    }

    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut retried = 0usize;
    let mut candidates = arrived_parent
        .map(|parent| orphans_waiting_for_parent(state, parent))
        .unwrap_or_else(|| state.orphan_blocks.keys().cloned().collect::<Vec<_>>());

    loop {
        candidates.sort();
        candidates.dedup();
        let mut ready = Vec::new();
        let mut still_missing = HashSet::new();
        for hash in candidates.drain(..) {
            let Some(block) = state.orphan_blocks.get(&hash) else {
                continue;
            };
            let missing = missing_block_parents(block, state);
            if missing.is_empty() {
                ready.push(hash);
            } else {
                set_missing_parents(state, &hash, missing.clone());
                still_missing.extend(missing);
            }
        }

        if ready.is_empty() {
            break;
        }

        for hash in ready {
            let Some(block) = remove_orphan(state, &hash) else {
                continue;
            };
            retried += 1;
            match accept_block_with_result(block, state, source) {
                BlockAcceptanceResult::Accepted => {
                    accepted += 1;
                    candidates.extend(orphans_waiting_for_parent(state, &hash));
                }
                BlockAcceptanceResult::MissingParent => {
                    still_missing.insert(hash);
                }
                _ => rejected += 1,
            }
        }

        if candidates.is_empty() {
            candidates.extend(
                still_missing
                    .iter()
                    .flat_map(|parent| orphans_waiting_for_parent(state, parent)),
            );
            if candidates.is_empty() {
                break;
            }
        }
    }

    OrphanAdoptionResult {
        accepted,
        rejected,
        retried,
    }
}

pub fn adopt_ready_orphans(state: &mut ChainState, source: AcceptSource) -> usize {
    adopt_ready_orphans_with_result(state, source, None).accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        accept::{accept_block, BlockAcceptanceResult},
        apply::apply_block,
        genesis::init_chain_state,
        mining::{
            build_candidate_block, build_coinbase_transaction, refresh_block_consensus_ids,
            refresh_block_consensus_ids_with_state,
        },
    };

    fn candidate_for_state(
        state: &ChainState,
        parents: Vec<Hash>,
        height: u64,
        _hash: &str,
        nonce: u64,
    ) -> Block {
        let mut block = build_candidate_block(
            parents,
            height,
            1,
            vec![build_coinbase_transaction("miner", 50, nonce)],
        );
        refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
        block
    }

    fn candidate(parents: Vec<Hash>, height: u64, hash: &str, nonce: u64) -> Block {
        let state = init_chain_state("candidate".into());
        candidate_for_state(&state, parents, height, hash, nonce)
    }

    fn state_after(state: &ChainState, block: &Block) -> ChainState {
        let mut advanced = state.clone();
        apply_block(block, &mut advanced).unwrap();
        advanced
    }

    fn queue_missing(state: &mut ChainState, block: Block) -> Vec<Hash> {
        let missing = missing_block_parents(&block, state);
        assert!(!missing.is_empty());
        assert!(queue_orphan_block(state, block, missing.clone()));
        missing
    }

    fn assert_orphan_indexes_consistent(state: &ChainState) {
        let mut block_hashes = state.orphan_blocks.keys().cloned().collect::<Vec<_>>();
        let mut missing_hashes = state
            .orphan_missing_parents
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut received_hashes = state
            .orphan_received_at_ms
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut indexed_hashes = state
            .orphan_missing_parent_index
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        block_hashes.sort();
        missing_hashes.sort();
        received_hashes.sort();
        indexed_hashes.sort();
        indexed_hashes.dedup();
        assert_eq!(missing_hashes, block_hashes);
        assert_eq!(received_hashes, block_hashes);
        assert_eq!(indexed_hashes, block_hashes);
    }

    #[test]
    fn child_before_parent_queues_as_orphan() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "parent-1",
            1,
        );
        let child = candidate(vec![parent.hash.clone()], 2, "child-1", 2);

        assert_eq!(
            accept_block_with_result(child.clone(), &mut state, AcceptSource::P2p),
            BlockAcceptanceResult::MissingParent
        );
        let missing = queue_missing(&mut state, child.clone());

        assert_eq!(missing, vec![parent.hash]);
        assert!(state.orphan_blocks.contains_key(&child.hash));
        assert_eq!(
            state.orphan_missing_parents.get(&child.hash),
            Some(&missing)
        );
        assert_eq!(
            orphan_children_waiting_for_parent(&state, &missing[0]),
            vec![child.hash.clone()]
        );
        assert_eq!(pending_missing_parent_count(&state), 1);
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn parent_arrival_adopts_ready_child() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "parent-1",
            1,
        );
        let parent_state = state_after(&state, &parent);
        let child = candidate_for_state(&parent_state, vec![parent.hash.clone()], 2, "child-1", 2);
        queue_missing(&mut state, child.clone());

        assert!(accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);

        assert_eq!(adopted, 1);
        assert!(state.dag.blocks.contains_key(&child.hash));
        assert!(!state.orphan_blocks.contains_key(&child.hash));
        assert!(!state.orphan_missing_parents.contains_key(&child.hash));
        assert!(!state.orphan_received_at_ms.contains_key(&child.hash));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn chained_orphan_adoption_waits_for_ancestors_and_then_cascades() {
        let mut state = init_chain_state("test".into());
        let parent =
            candidate_for_state(&state, vec![state.dag.genesis_hash.clone()], 1, "parent", 1);
        let parent_state = state_after(&state, &parent);
        let child = candidate_for_state(&parent_state, vec![parent.hash.clone()], 2, "child", 2);
        let child_state = state_after(&parent_state, &child);
        let grandchild = candidate_for_state(
            &child_state,
            vec![parent.hash.clone(), child.hash.clone()],
            3,
            "grandchild",
            3,
        );

        queue_missing(&mut state, grandchild.clone());
        queue_missing(&mut state, child.clone());
        assert_eq!(adopt_ready_orphans(&mut state, AcceptSource::P2p), 0);
        assert!(state.orphan_blocks.contains_key(&child.hash));
        assert!(state.orphan_blocks.contains_key(&grandchild.hash));

        assert!(accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);

        assert_eq!(adopted, 2);
        assert!(state.dag.blocks.contains_key(&child.hash));
        assert!(state.dag.blocks.contains_key(&grandchild.hash));
        assert!(state.orphan_blocks.is_empty());
        assert!(state.orphan_missing_parents.is_empty());
        assert!(state.orphan_received_at_ms.is_empty());
    }

    #[test]
    fn invalid_pow_orphan_does_not_adopt() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "pow-parent",
            1,
        );
        let mut child = build_candidate_block(
            vec![parent.hash.clone()],
            2,
            0x01000000,
            vec![build_coinbase_transaction("miner", 50, 2)],
        );
        child.header.nonce = 0;
        refresh_block_consensus_ids(&mut child);
        queue_missing(&mut state, child.clone());

        assert!(accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);

        assert_eq!(adopted, 0);
        assert!(!state.dag.blocks.contains_key(&child.hash));
        assert!(!state.orphan_blocks.contains_key(&child.hash));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn malformed_orphan_does_not_adopt() {
        let mut state = init_chain_state("test".into());
        let parent = candidate(
            vec![state.dag.genesis_hash.clone()],
            1,
            "malformed-parent",
            1,
        );
        let parent_state = state_after(&state, &parent);
        let mut child = candidate_for_state(
            &parent_state,
            vec![parent.hash.clone()],
            2,
            "malformed-child",
            2,
        );
        child.transactions.clear();
        queue_missing(&mut state, child.clone());

        assert!(accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);

        assert_eq!(adopted, 0);
        assert!(!state.dag.blocks.contains_key(&child.hash));
        assert!(!state.orphan_blocks.contains_key(&child.hash));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn missing_parent_index_updates_when_an_orphan_becomes_partially_ready() {
        let mut state = init_chain_state("test".into());
        let parent_a = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "parent-a",
            1,
        );
        let parent_b = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "parent-b",
            2,
        );
        let state_with_a = state_after(&state, &parent_a);
        let child = candidate_for_state(
            &state_with_a,
            vec![parent_a.hash.clone(), parent_b.hash.clone()],
            2,
            "child-ab",
            3,
        );
        queue_missing(&mut state, child.clone());

        assert_eq!(pending_missing_parent_count(&state), 2);
        assert_eq!(
            orphan_children_waiting_for_parent(&state, &parent_a.hash),
            vec![child.hash.clone()]
        );
        assert_eq!(
            orphan_children_waiting_for_parent(&state, &parent_b.hash),
            vec![child.hash.clone()]
        );

        assert!(accept_block(parent_a.clone(), &mut state, AcceptSource::P2p).is_ok());
        assert_eq!(adopt_ready_orphans(&mut state, AcceptSource::P2p), 0);

        assert!(orphan_children_waiting_for_parent(&state, &parent_a.hash).is_empty());
        assert_eq!(
            orphan_children_waiting_for_parent(&state, &parent_b.hash),
            vec![child.hash.clone()]
        );
        assert_eq!(pending_missing_parent_count(&state), 1);
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn duplicate_orphan_is_ignored() {
        let mut state = init_chain_state("test".into());
        let orphan = candidate(vec!["missing-parent".into()], 1, "dup-orphan", 1);

        assert!(queue_orphan_block(
            &mut state,
            orphan.clone(),
            vec!["missing-parent".into()]
        ));
        assert!(!queue_orphan_block(
            &mut state,
            orphan,
            vec!["missing-parent".into()]
        ));

        assert_eq!(state.orphan_blocks.len(), 1);
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn orphan_capacity_pruning_removes_oldest_entries() {
        let mut state = init_chain_state("test".into());

        let mut orphan_hashes = Vec::new();
        for i in 0..4 {
            let orphan = candidate(
                vec![format!("missing-parent-{i}")],
                1,
                &format!("orphan-{i}"),
                i + 1,
            );
            orphan_hashes.push(orphan.hash.clone());
            assert!(queue_orphan_block(
                &mut state,
                orphan,
                vec![format!("missing-parent-{i}")]
            ));
        }
        for (i, hash) in orphan_hashes.iter().enumerate() {
            state
                .orphan_received_at_ms
                .insert(hash.clone(), 1_000 + i as u64);
        }

        let removed = prune_orphans(&mut state, 2, u64::MAX);

        assert_eq!(removed, 2);
        assert!(!state.orphan_blocks.contains_key(&orphan_hashes[0]));
        assert!(!state.orphan_blocks.contains_key(&orphan_hashes[1]));
        assert!(state.orphan_blocks.contains_key(&orphan_hashes[2]));
        assert!(state.orphan_blocks.contains_key(&orphan_hashes[3]));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn orphan_age_pruning_removes_expired_entries() {
        let mut state = init_chain_state("test".into());
        let fresh = candidate(vec!["missing-fresh".into()], 1, "fresh-orphan", 1);
        let fresh_hash = fresh.hash.clone();
        let expired = candidate(vec!["missing-expired".into()], 1, "expired-orphan", 2);
        let expired_hash = expired.hash.clone();
        assert!(queue_orphan_block(
            &mut state,
            fresh,
            vec!["missing-fresh".into()]
        ));
        assert!(queue_orphan_block(
            &mut state,
            expired,
            vec!["missing-expired".into()]
        ));
        let now = now_ms();
        state.orphan_received_at_ms.insert(fresh_hash.clone(), now);
        state
            .orphan_received_at_ms
            .insert(expired_hash.clone(), now.saturating_sub(10_000));

        let removed = prune_orphans(&mut state, 10, 1_000);

        assert_eq!(removed, 1);
        assert!(state.orphan_blocks.contains_key(&fresh_hash));
        assert!(!state.orphan_blocks.contains_key(&expired_hash));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn targeted_adoption_rebuilds_missing_parent_index_for_legacy_state() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "legacy-parent",
            1,
        );
        let parent_state = state_after(&state, &parent);
        let child = candidate_for_state(
            &parent_state,
            vec![parent.hash.clone()],
            2,
            "legacy-child",
            2,
        );
        queue_missing(&mut state, child.clone());
        state.orphan_missing_parent_index.clear();

        assert!(accept_block(parent.clone(), &mut state, AcceptSource::P2p).is_ok());
        let adoption =
            adopt_ready_orphans_with_result(&mut state, AcceptSource::P2p, Some(&parent.hash));

        assert_eq!(adoption.accepted, 1);
        assert_eq!(adoption.retried, 1);
        assert!(state.dag.blocks.contains_key(&child.hash));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn orphan_missing_parents_is_cleaned_after_adoption() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "clean-parent",
            1,
        );
        let parent_state = state_after(&state, &parent);
        let child = candidate_for_state(
            &parent_state,
            vec![parent.hash.clone()],
            2,
            "clean-child",
            2,
        );
        queue_missing(&mut state, child.clone());

        assert!(accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        assert_eq!(adopt_ready_orphans(&mut state, AcceptSource::P2p), 1);

        assert!(!state.orphan_blocks.contains_key(&child.hash));
        assert!(!state.orphan_missing_parents.contains_key(&child.hash));
        assert!(!state.orphan_received_at_ms.contains_key(&child.hash));
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn failed_adoption_does_not_leave_inconsistent_orphan_state() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "shared-parent",
            1,
        );
        let parent_state = state_after(&state, &parent);
        let mut bad_child =
            candidate_for_state(&parent_state, vec![parent.hash.clone()], 2, "bad-child", 2);
        bad_child.header.height = 99;
        let blocked_child = candidate(vec!["still-missing".into()], 2, "blocked-child", 3);
        queue_missing(&mut state, bad_child.clone());
        queue_missing(&mut state, blocked_child.clone());

        assert!(accept_block(parent, &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);

        assert_eq!(adopted, 0);
        assert!(!state.dag.blocks.contains_key(&bad_child.hash));
        assert!(!state.orphan_blocks.contains_key(&bad_child.hash));
        assert!(state.orphan_blocks.contains_key(&blocked_child.hash));
        assert_eq!(
            state.orphan_missing_parents.get(&blocked_child.hash),
            Some(&vec!["still-missing".into()])
        );
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn adoption_order_is_deterministic_when_multiple_orphans_become_ready() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "det-parent",
            1,
        );
        let parent_state = state_after(&state, &parent);
        let beta = candidate_for_state(
            &parent_state,
            vec![parent.hash.clone()],
            2,
            "orphan-beta",
            2,
        );
        let alpha = candidate_for_state(
            &parent_state,
            vec![parent.hash.clone()],
            2,
            "orphan-alpha",
            3,
        );
        queue_missing(&mut state, beta.clone());
        queue_missing(&mut state, alpha.clone());

        assert!(accept_block(parent.clone(), &mut state, AcceptSource::P2p).is_ok());
        let adopted = adopt_ready_orphans(&mut state, AcceptSource::P2p);

        assert_eq!(adopted, 1);
        let adopted_children = state.dag.children.get(&parent.hash).unwrap();
        assert_eq!(adopted_children.len(), 1);
        assert!(adopted_children[0] == alpha.hash || adopted_children[0] == beta.hash);
    }
}
