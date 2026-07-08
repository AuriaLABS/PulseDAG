use std::collections::HashSet;

use crate::{
    state::ChainState,
    types::{Block, Hash},
};

pub const DAG_ORDERING_VERSION: &str = "selected-chain-merge-set-v1";

pub fn default_ordering_version() -> String {
    DAG_ORDERING_VERSION.to_string()
}

fn compare_order_blocks(a: &Block, b: &Block) -> std::cmp::Ordering {
    a.header
        .height
        .cmp(&b.header.height)
        .then_with(|| b.header.blue_score.cmp(&a.header.blue_score))
        .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
        .then_with(|| a.hash.cmp(&b.hash))
}

fn sorted_existing(mut hashes: Vec<Hash>, state: &ChainState) -> Vec<Hash> {
    hashes.sort_by(
        |a, b| match (state.dag.blocks.get(a), state.dag.blocks.get(b)) {
            (Some(a_block), Some(b_block)) => compare_order_blocks(a_block, b_block),
            _ => a.cmp(b),
        },
    );
    hashes.dedup();
    hashes
}

fn push_once(hash: Hash, state: &ChainState, seen: &mut HashSet<Hash>, ordered: &mut Vec<Hash>) {
    if state.dag.blocks.contains_key(&hash) && seen.insert(hash.clone()) {
        ordered.push(hash);
    }
}

/// Derive PulseDAG's deterministic accepted-block total order from selected-chain
/// and merge-set metadata.
///
/// Rule v1:
/// 1. Emit selected-chain blocks in selected-parent ancestry order.
/// 2. For each selected-chain block, emit its blue merge-set members not already
///    emitted, ordered by height, descending blue score, timestamp, then hash.
/// 3. Emit red merge-set members with the same deterministic tie-breaker.
/// 4. Emit any remaining accepted blocks with the same deterministic tie-breaker.
///
/// The final fallback makes the order total for partially-restored or legacy
/// snapshots that predate merge-set metadata.
pub fn derive_ordered_dag(state: &ChainState) -> Vec<Hash> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::with_capacity(state.dag.blocks.len());

    for hash in &state.dag.selected_chain {
        push_once(hash.clone(), state, &mut seen, &mut ordered);
    }

    for selected in &state.dag.selected_chain {
        let blues = sorted_existing(
            state
                .dag
                .merge_set_blues
                .get(selected)
                .cloned()
                .unwrap_or_default(),
            state,
        );
        for hash in blues {
            push_once(hash, state, &mut seen, &mut ordered);
        }
        let reds = sorted_existing(
            state
                .dag
                .merge_set_reds
                .get(selected)
                .cloned()
                .unwrap_or_default(),
            state,
        );
        for hash in reds {
            push_once(hash, state, &mut seen, &mut ordered);
        }
    }

    let remaining = sorted_existing(state.dag.blocks.keys().cloned().collect(), state);
    for hash in remaining {
        push_once(hash, state, &mut seen, &mut ordered);
    }

    ordered
}

pub fn refresh_ordered_dag(state: &mut ChainState) {
    state.dag.ordered_dag = derive_ordered_dag(state);
    state.dag.ordering_version = DAG_ORDERING_VERSION.to_string();
    state.dag.ordered_dag_tip = state.dag.ordered_dag.last().cloned();
}

pub fn ordered_dag_tip(state: &ChainState) -> Option<Hash> {
    state.dag.ordered_dag.last().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{genesis::init_chain_state, types::BlockHeader, ChainState};

    fn block(
        hash: &str,
        parents: Vec<&str>,
        height: u64,
        blue_score: u64,
        timestamp: u64,
    ) -> Block {
        Block {
            hash: hash.into(),
            header: BlockHeader {
                version: 1,
                parents: parents.into_iter().map(str::to_string).collect(),
                timestamp,
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("m-{hash}"),
                state_root: format!("s-{hash}"),
                blue_score,
                height,
            },
            transactions: vec![],
        }
    }

    fn insert(
        state: &mut ChainState,
        b: Block,
        selected_parent: Option<&str>,
        blues: Vec<&str>,
        reds: Vec<&str>,
    ) {
        for p in &b.header.parents {
            state.dag.tips.remove(p);
            state
                .dag
                .children
                .entry(p.clone())
                .or_default()
                .push(b.hash.clone());
        }
        state.dag.tips.insert(b.hash.clone());
        state.dag.best_height = state.dag.best_height.max(b.header.height);
        state
            .dag
            .selected_parents
            .insert(b.hash.clone(), selected_parent.map(str::to_string));
        state.dag.merge_set_blues.insert(
            b.hash.clone(),
            blues.into_iter().map(str::to_string).collect(),
        );
        state.dag.merge_set_reds.insert(
            b.hash.clone(),
            reds.into_iter().map(str::to_string).collect(),
        );
        state.dag.blocks.insert(b.hash.clone(), b);
    }

    fn fixture() -> ChainState {
        let mut s = init_chain_state("ordering-test".into());
        let g = s.dag.genesis_hash.clone();
        insert(
            &mut s,
            block("a", vec![&g], 1, 1, 10),
            Some(&g),
            vec![],
            vec![],
        );
        insert(
            &mut s,
            block("b", vec![&g], 1, 1, 8),
            Some(&g),
            vec![],
            vec![],
        );
        insert(
            &mut s,
            block("c", vec!["a", "b"], 2, 3, 12),
            Some("a"),
            vec!["b"],
            vec![],
        );
        insert(
            &mut s,
            block("r", vec![&g], 1, 1, 7),
            Some(&g),
            vec![],
            vec![],
        );
        insert(
            &mut s,
            block("d", vec!["c", "r"], 3, 4, 13),
            Some("c"),
            vec![],
            vec!["r"],
        );
        s.dag.selected_chain = vec![g, "a".into(), "c".into(), "d".into()];
        refresh_ordered_dag(&mut s);
        s
    }

    #[test]
    fn dag_ordering_same_dag_different_arrival_order_same_final_ordering() {
        let mut a = fixture();
        let expected = a.dag.ordered_dag.clone();
        let mut blocks: Vec<_> = a.dag.blocks.clone().into_iter().collect();
        blocks.sort_by(|(a, _), (b, _)| b.cmp(a));
        a.dag.blocks = blocks.into_iter().collect();
        refresh_ordered_dag(&mut a);
        assert_eq!(a.dag.ordered_dag, expected);
    }

    #[test]
    fn dag_ordering_parallel_blocks_ordered_deterministically() {
        let s = fixture();
        let b_pos = s.dag.ordered_dag.iter().position(|h| h == "b").unwrap();
        let r_pos = s.dag.ordered_dag.iter().position(|h| h == "r").unwrap();
        assert!(b_pos < r_pos);
    }

    #[test]
    fn dag_ordering_red_blocks_do_not_break_state_application_order() {
        let s = fixture();
        assert_eq!(s.dag.ordered_dag.last().unwrap(), "r");
        assert_eq!(ordered_dag_tip(&s).as_deref(), Some("r"));
    }

    #[test]
    fn dag_ordering_snapshot_restore_keeps_same_ordering() {
        let s = fixture();
        let restored = s.clone();
        assert_eq!(restored.dag.ordered_dag, s.dag.ordered_dag);
        assert_eq!(restored.dag.ordering_version, DAG_ORDERING_VERSION);
    }
}
