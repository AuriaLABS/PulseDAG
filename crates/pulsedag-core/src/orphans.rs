use std::{
    collections::{BTreeMap, BTreeSet},
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
    pub accepted_hashes: Vec<Hash>,
    pub failure_reasons: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrphanBacklogClassification {
    pub retryable_ready: usize,
    pub waiting_missing_parent: usize,
    pub stale_missing_parent_entries: usize,
    pub unindexed_missing_parent_entries: usize,
}

fn reprocess_failure_reason(result: &BlockAcceptanceResult) -> String {
    match result {
        BlockAcceptanceResult::MissingParent => "missing_parent".to_string(),
        BlockAcceptanceResult::Duplicate => "duplicate".to_string(),
        BlockAcceptanceResult::InvalidPow => "invalid_pow".to_string(),
        BlockAcceptanceResult::InvalidTransaction => "invalid_transaction".to_string(),
        BlockAcceptanceResult::Malformed => "malformed".to_string(),
        BlockAcceptanceResult::Rejected(message) => {
            let normalized = message.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                "rejected".to_string()
            } else {
                normalized
                    .chars()
                    .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
                    .collect::<String>()
                    .split('_')
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("_")
            }
        }
        BlockAcceptanceResult::Accepted => "accepted".to_string(),
    }
}

fn record_reprocess_failure(
    failure_reasons: &mut BTreeMap<String, usize>,
    result: &BlockAcceptanceResult,
) {
    let reason = reprocess_failure_reason(result);
    *failure_reasons.entry(reason).or_insert(0) += 1;
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

fn normalize_missing_parents(missing_parents: Vec<Hash>) -> Vec<Hash> {
    missing_parents
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn unindex_orphan_missing_parents(state: &mut ChainState, orphan_hash: &Hash) {
    if let Some(existing) = state.orphan_missing_parents.get(orphan_hash) {
        for parent in existing {
            if let Some(waiting) = state.orphan_parent_index.get_mut(parent) {
                waiting.remove(orphan_hash);
                if waiting.is_empty() {
                    state.orphan_parent_index.remove(parent);
                }
            }
        }
    }
}

fn index_orphan_missing_parents(
    state: &mut ChainState,
    orphan_hash: Hash,
    missing_parents: Vec<Hash>,
) {
    unindex_orphan_missing_parents(state, &orphan_hash);
    let missing_parents = normalize_missing_parents(missing_parents);
    for parent in &missing_parents {
        state
            .orphan_parent_index
            .entry(parent.clone())
            .or_default()
            .insert(orphan_hash.clone());
    }
    state
        .orphan_missing_parents
        .insert(orphan_hash, missing_parents);
}

fn remove_queued_orphan(state: &mut ChainState, orphan_hash: &Hash) -> Option<Block> {
    let block = state.orphan_blocks.remove(orphan_hash);
    unindex_orphan_missing_parents(state, orphan_hash);
    state.orphan_missing_parents.remove(orphan_hash);
    state.orphan_received_at_ms.remove(orphan_hash);
    block
}

pub fn orphan_children_waiting_for_parent(state: &ChainState, parent: &Hash) -> Vec<Hash> {
    state
        .orphan_parent_index
        .get(parent)
        .map(|children| children.iter().cloned().collect())
        .unwrap_or_default()
}

/// Return the missing external roots that must be fetched before `orphan_hash` can be adopted.
///
/// If an orphan is missing a parent that is itself already queued as an orphan, the search walks
/// through that queued parent and returns the first missing ancestors that are not available in the
/// DAG or the orphan pool. This mirrors Kaspa-style orphan-root recovery: ask for the earliest
/// unknown frontier instead of repeatedly requesting already-queued descendants.
pub fn orphan_missing_roots(state: &ChainState, orphan_hash: &Hash) -> Vec<Hash> {
    let Some(orphan) = state.orphan_blocks.get(orphan_hash) else {
        return Vec::new();
    };

    let mut roots = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut stack = orphan.header.parents.clone();
    stack.sort();

    while let Some(parent) = stack.pop() {
        if state.dag.blocks.contains_key(&parent) || !visited.insert(parent.clone()) {
            continue;
        }
        if let Some(queued_parent) = state.orphan_blocks.get(&parent) {
            for ancestor in queued_parent.header.parents.iter().rev() {
                if !state.dag.blocks.contains_key(ancestor) {
                    stack.push(ancestor.clone());
                }
            }
        } else {
            roots.insert(parent);
        }
    }

    roots.into_iter().collect()
}

/// Rebuild orphan missing-parent indexes from the currently queued orphan blocks.
///
/// This is intentionally derived from each block's real parents rather than existing orphan index
/// metadata, so it can repair states where `orphan_blocks` survived but `orphan_parent_index` or
/// `orphan_missing_parents` was empty/stale. The returned classification describes the rebuilt
/// backlog in retryable/waiting/stale/unindexed buckets.
pub fn rebuild_orphan_parent_index(state: &mut ChainState) -> OrphanBacklogClassification {
    state.orphan_missing_parents.clear();
    state.orphan_parent_index.clear();

    let mut orphan_hashes = state.orphan_blocks.keys().cloned().collect::<Vec<_>>();
    orphan_hashes.sort();
    for orphan_hash in orphan_hashes {
        let Some(block) = state.orphan_blocks.get(&orphan_hash) else {
            continue;
        };
        let missing = missing_block_parents(block, state);
        index_orphan_missing_parents(state, orphan_hash, missing);
    }

    classify_orphan_backlog(state)
}

pub fn classify_orphan_backlog(state: &ChainState) -> OrphanBacklogClassification {
    let mut classification = OrphanBacklogClassification::default();
    for (hash, block) in &state.orphan_blocks {
        let actual_missing = missing_block_parents(block, state);
        let recorded_missing = state
            .orphan_missing_parents
            .get(hash)
            .cloned()
            .unwrap_or_default();
        if actual_missing.is_empty() {
            classification.retryable_ready = classification.retryable_ready.saturating_add(1);
        } else {
            classification.waiting_missing_parent =
                classification.waiting_missing_parent.saturating_add(1);
        }
        if normalize_missing_parents(recorded_missing) != actual_missing {
            classification.stale_missing_parent_entries = classification
                .stale_missing_parent_entries
                .saturating_add(1);
        }
        for parent in actual_missing {
            if !state
                .orphan_parent_index
                .get(&parent)
                .map(|waiting| waiting.contains(hash))
                .unwrap_or(false)
            {
                classification.unindexed_missing_parent_entries = classification
                    .unindexed_missing_parent_entries
                    .saturating_add(1);
            }
        }
    }
    classification
}

pub fn pending_missing_parent_count(state: &ChainState) -> usize {
    let indexed = state.orphan_parent_index.len();
    if indexed > 0 {
        return indexed;
    }
    let recorded = state
        .orphan_missing_parents
        .values()
        .flat_map(|parents| parents.iter().cloned())
        .collect::<BTreeSet<_>>()
        .len();
    if recorded > 0 {
        return recorded;
    }
    state
        .orphan_blocks
        .keys()
        .flat_map(|hash| orphan_missing_roots(state, hash))
        .collect::<BTreeSet<_>>()
        .len()
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
        if remove_queued_orphan(state, &hash).is_some() {
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
            if remove_queued_orphan(state, &hash).is_some() {
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
    index_orphan_missing_parents(state, hash.clone(), missing_parents);
    state.orphan_received_at_ms.insert(hash.clone(), now_ms());
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
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut retried = 0usize;
    let mut accepted_hashes = Vec::new();
    let mut failure_reasons = BTreeMap::new();
    let mut candidates = arrived_parent
        .map(|parent| orphan_children_waiting_for_parent(state, parent))
        .unwrap_or_else(|| state.orphan_blocks.keys().cloned().collect::<Vec<_>>());

    loop {
        candidates.sort();
        candidates.dedup();
        let mut ready = Vec::new();
        for hash in candidates.drain(..) {
            let Some(block) = state.orphan_blocks.get(&hash) else {
                continue;
            };
            let missing = missing_block_parents(block, state);
            if missing.is_empty() {
                ready.push(hash);
            } else {
                index_orphan_missing_parents(state, hash, missing);
            }
        }

        if ready.is_empty() {
            break;
        }

        for hash in ready {
            let Some(block) = remove_queued_orphan(state, &hash) else {
                continue;
            };
            retried += 1;
            let result = accept_block_with_result(block.clone(), state, source);
            match result {
                BlockAcceptanceResult::Accepted => {
                    accepted += 1;
                    accepted_hashes.push(hash.clone());
                    candidates.extend(orphan_children_waiting_for_parent(state, &hash));
                }
                BlockAcceptanceResult::MissingParent => {
                    record_reprocess_failure(&mut failure_reasons, &result);
                    let missing = missing_block_parents(&block, state);
                    let _ = queue_orphan_block_bounded(
                        state,
                        block,
                        missing,
                        DEFAULT_ORPHAN_MAX_COUNT,
                        DEFAULT_ORPHAN_MAX_AGE_MS,
                    );
                }
                _ => {
                    rejected += 1;
                    record_reprocess_failure(&mut failure_reasons, &result);
                }
            }
        }
    }

    OrphanAdoptionResult {
        accepted,
        rejected,
        retried,
        accepted_hashes,
        failure_reasons,
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
            .orphan_parent_index
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

        let mut rebuilt_parent_index = std::collections::HashMap::<Hash, BTreeSet<Hash>>::new();
        for (orphan_hash, missing_parents) in &state.orphan_missing_parents {
            for parent in missing_parents {
                rebuilt_parent_index
                    .entry(parent.clone())
                    .or_default()
                    .insert(orphan_hash.clone());
            }
        }
        assert_eq!(state.orphan_parent_index, rebuilt_parent_index);
    }

    #[test]
    fn orphan_missing_roots_walks_queued_orphan_ancestors() {
        let mut state = init_chain_state("test".into());
        let root = candidate_for_state(&state, vec![state.dag.genesis_hash.clone()], 1, "root", 1);
        let root_state = state_after(&state, &root);
        let parent = candidate_for_state(&root_state, vec![root.hash.clone()], 2, "parent", 2);
        let parent_state = state_after(&root_state, &parent);
        let child = candidate_for_state(&parent_state, vec![parent.hash.clone()], 3, "child", 3);

        assert!(queue_orphan_block(
            &mut state,
            parent.clone(),
            vec![root.hash.clone()]
        ));
        assert!(queue_orphan_block(
            &mut state,
            child.clone(),
            vec![parent.hash.clone()]
        ));

        assert_eq!(orphan_missing_roots(&state, &child.hash), vec![root.hash]);
    }

    #[test]
    fn rebuild_orphan_parent_index_repairs_empty_indexes_from_real_parents() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "rebuild-parent",
            1,
        );
        let child = candidate(vec![parent.hash.clone()], 2, "rebuild-child", 2);
        queue_missing(&mut state, child.clone());
        state.orphan_missing_parents.clear();
        state.orphan_parent_index.clear();

        assert_eq!(pending_missing_parent_count(&state), 1);
        let classification = rebuild_orphan_parent_index(&mut state);

        assert_eq!(classification.waiting_missing_parent, 1);
        assert_eq!(classification.unindexed_missing_parent_entries, 0);
        assert_eq!(
            state.orphan_missing_parents.get(&child.hash),
            Some(&vec![parent.hash.clone()])
        );
        assert_eq!(
            orphan_children_waiting_for_parent(&state, &parent.hash),
            vec![child.hash]
        );
        assert_orphan_indexes_consistent(&state);
    }

    #[test]
    fn classifies_ready_and_stale_orphan_backlog() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "parent-1",
            1,
        );
        let child = candidate(vec![parent.hash.clone()], 2, "child-1", 2);
        let missing = queue_missing(&mut state, child.clone());
        assert_eq!(missing, vec![parent.hash.clone()]);

        let waiting = classify_orphan_backlog(&state);
        assert_eq!(waiting.retryable_ready, 0);
        assert_eq!(waiting.waiting_missing_parent, 1);
        assert_eq!(waiting.stale_missing_parent_entries, 0);
        assert_eq!(waiting.unindexed_missing_parent_entries, 0);

        apply_block(&parent, &mut state).unwrap();
        let ready = classify_orphan_backlog(&state);
        assert_eq!(ready.retryable_ready, 1);
        assert_eq!(ready.waiting_missing_parent, 0);
        assert_eq!(ready.stale_missing_parent_entries, 1);
        assert_eq!(ready.unindexed_missing_parent_entries, 0);
    }

    #[test]
    fn classifies_unindexed_missing_parent_entries() {
        let mut state = init_chain_state("test".into());
        let parent = candidate_for_state(
            &state,
            vec![state.dag.genesis_hash.clone()],
            1,
            "parent-1",
            1,
        );
        let child = candidate(vec![parent.hash.clone()], 2, "child-1", 2);
        queue_missing(&mut state, child);
        state.orphan_parent_index.clear();

        let classification = classify_orphan_backlog(&state);
        assert_eq!(classification.retryable_ready, 0);
        assert_eq!(classification.waiting_missing_parent, 1);
        assert_eq!(classification.stale_missing_parent_entries, 0);
        assert_eq!(classification.unindexed_missing_parent_entries, 1);
        assert_eq!(pending_missing_parent_count(&state), 1);
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
        let adoption = adopt_ready_orphans_with_result(&mut state, AcceptSource::P2p, None);

        assert_eq!(adoption.accepted, 0);
        assert_eq!(adoption.retried, 1);
        assert_eq!(adoption.failure_reasons.values().sum::<usize>(), 1);
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
        let adoption = adopt_ready_orphans_with_result(&mut state, AcceptSource::P2p, None);

        assert_eq!(adoption.accepted, 0);
        assert_eq!(adoption.retried, 1);
        assert_eq!(adoption.failure_reasons.values().sum::<usize>(), 1);
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
        let adoption = adopt_ready_orphans_with_result(&mut state, AcceptSource::P2p, None);

        assert_eq!(adoption.accepted, 0);
        assert_eq!(adoption.retried, 1);
        assert_eq!(adoption.failure_reasons.values().sum::<usize>(), 1);
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
