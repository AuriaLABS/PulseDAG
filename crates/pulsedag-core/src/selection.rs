use std::cmp::Ordering;

use crate::{
    state::{ChainState, SelectedParentPolicy},
    types::{Block, Hash},
};

/// PulseDAG's legacy deterministic tip ordering policy.
///
/// Tips are ordered by descending `height`, then descending `blue_score`, then
/// descending `timestamp`, then descending `hash`. This is a local deterministic
/// selection rule for stable node behavior; it is not full GHOSTDAG and does not
/// claim Kaspa consensus compatibility.
fn compare_tip_blocks(a: &Block, b: &Block) -> Ordering {
    a.header
        .height
        .cmp(&b.header.height)
        .then_with(|| a.header.blue_score.cmp(&b.header.blue_score))
        .then_with(|| a.header.timestamp.cmp(&b.header.timestamp))
        .then_with(|| a.hash.cmp(&b.hash))
}

/// Core GHOSTDAG-inspired selected-parent ordering.
///
/// This is intentionally not a full blue/red merge-set calculation. It chooses
/// between already-known DAG candidates using stable metadata only: descending
/// blue score, descending height, then ascending hash. The final hash tie-break
/// is canonical and arrival-order independent.
pub fn compare_selected_parent_candidates(a: &Block, b: &Block) -> Ordering {
    a.header
        .blue_score
        .cmp(&b.header.blue_score)
        .then_with(|| a.header.height.cmp(&b.header.height))
        .then_with(|| b.hash.cmp(&a.hash))
}

fn sorted_tip_hashes_with_policy(state: &ChainState, policy: SelectedParentPolicy) -> Vec<Hash> {
    let mut tips = state
        .dag
        .tips
        .iter()
        .filter_map(|hash| {
            state
                .dag
                .blocks
                .get(hash)
                .map(|block| (hash.clone(), block))
        })
        .collect::<Vec<_>>();
    match policy {
        SelectedParentPolicy::LegacyTip => tips.sort_by(|(_, a), (_, b)| compare_tip_blocks(b, a)),
        SelectedParentPolicy::GhostdagInspired => {
            tips.sort_by(|(_, a), (_, b)| compare_selected_parent_candidates(b, a));
        }
    }
    tips.into_iter().map(|(hash, _)| hash).collect()
}

pub fn sorted_tip_hashes(state: &ChainState) -> Vec<Hash> {
    sorted_tip_hashes_with_policy(state, state.dag.selected_parent_policy)
}

pub fn sorted_legacy_tip_hashes(state: &ChainState) -> Vec<Hash> {
    sorted_tip_hashes_with_policy(state, SelectedParentPolicy::LegacyTip)
}

pub fn preferred_tip_hash(state: &ChainState) -> Option<Hash> {
    sorted_tip_hashes(state).into_iter().next()
}

pub fn legacy_preferred_tip_hash(state: &ChainState) -> Option<Hash> {
    sorted_legacy_tip_hashes(state).into_iter().next()
}

pub fn calculate_selected_parent(block: &Block, state: &ChainState) -> Option<Hash> {
    block
        .header
        .parents
        .iter()
        .filter_map(|parent| state.dag.blocks.get(parent).map(|block| (parent, block)))
        .max_by(|(_, a), (_, b)| compare_selected_parent_candidates(a, b))
        .map(|(hash, _)| hash.clone())
}

pub fn rebuild_selected_chain_from_tip(state: &ChainState, tip: Option<Hash>) -> Vec<Hash> {
    let mut chain = Vec::new();
    let mut cursor = tip;
    while let Some(hash) = cursor {
        if !state.dag.blocks.contains_key(&hash) || chain.contains(&hash) {
            break;
        }
        cursor = state.dag.selected_parents.get(&hash).cloned().flatten();
        chain.push(hash);
    }
    chain.reverse();
    chain
}

pub fn refresh_selected_chain(state: &mut ChainState) {
    let tip = preferred_tip_hash(state);
    state.dag.selected_chain = rebuild_selected_chain_from_tip(state, tip);
}

#[cfg(test)]
mod tests {
    use crate::{
        genesis::init_chain_state,
        selection::{
            legacy_preferred_tip_hash, preferred_tip_hash, sorted_legacy_tip_hashes,
            sorted_tip_hashes,
        },
        state::SelectedParentPolicy,
        types::{Block, BlockHeader},
    };

    fn tip_block(hash: &str, height: u64, blue_score: u64, timestamp: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: vec!["genesis-block".to_string()],
                timestamp,
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("merkle-{hash}"),
                state_root: format!("state-{hash}"),
                blue_score,
                height,
            },
            transactions: vec![],
        }
    }

    fn state_with_tips(blocks: Vec<Block>) -> crate::ChainState {
        let mut state = init_chain_state("selection-test".to_string());
        state.dag.tips.clear();

        for block in blocks {
            state.dag.tips.insert(block.hash.clone());
            state.dag.blocks.insert(block.hash.clone(), block);
        }

        state
    }

    #[test]
    fn selection_higher_blue_score_wins_before_height() {
        let state = state_with_tips(vec![
            tip_block("higher-height", 4, 1, 1),
            tip_block("higher-blue", 3, 100, 100),
        ]);

        assert_eq!(
            sorted_tip_hashes(&state),
            vec!["higher-blue", "higher-height"]
        );
        assert_eq!(preferred_tip_hash(&state), Some("higher-blue".to_string()));
    }

    #[test]
    fn selection_same_blue_score_higher_height_wins() {
        let state = state_with_tips(vec![
            tip_block("lower-height", 3, 100, 100),
            tip_block("higher-height", 4, 100, 1),
        ]);

        assert_eq!(
            sorted_tip_hashes(&state),
            vec!["higher-height", "lower-height"]
        );
        assert_eq!(
            preferred_tip_hash(&state),
            Some("higher-height".to_string())
        );
    }

    #[test]
    fn selection_same_score_and_height_lowest_hash_tie_break_is_deterministic() {
        let state = state_with_tips(vec![
            tip_block("hash-a", 7, 11, 101),
            tip_block("hash-c", 7, 11, 101),
            tip_block("hash-b", 7, 11, 101),
        ]);

        assert_eq!(
            sorted_tip_hashes(&state),
            vec!["hash-a", "hash-b", "hash-c"]
        );
        assert_eq!(preferred_tip_hash(&state), Some("hash-a".to_string()));
    }

    #[test]
    fn legacy_tip_policy_remains_available() {
        let mut state = state_with_tips(vec![
            tip_block("lower-height", 3, 100, 100),
            tip_block("higher-height", 4, 1, 1),
        ]);
        state.dag.selected_parent_policy = SelectedParentPolicy::LegacyTip;

        assert_eq!(
            sorted_tip_hashes(&state),
            vec!["higher-height", "lower-height"]
        );
        assert_eq!(
            preferred_tip_hash(&state),
            Some("higher-height".to_string())
        );
        assert_eq!(sorted_legacy_tip_hashes(&state), sorted_tip_hashes(&state));
        assert_eq!(
            legacy_preferred_tip_hash(&state),
            preferred_tip_hash(&state)
        );
    }
}
