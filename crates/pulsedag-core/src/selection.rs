use std::cmp::Ordering;

use crate::{
    state::ChainState,
    types::{Block, Hash},
};

/// PulseDAG's current deterministic tip ordering policy.
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

pub fn sorted_tip_hashes(state: &ChainState) -> Vec<Hash> {
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
    tips.sort_by(|(_, a), (_, b)| compare_tip_blocks(b, a));
    tips.into_iter().map(|(hash, _)| hash).collect()
}

pub fn preferred_tip_hash(state: &ChainState) -> Option<Hash> {
    sorted_tip_hashes(state).into_iter().next()
}

#[cfg(test)]
mod tests {
    use crate::{
        genesis::init_chain_state,
        selection::{preferred_tip_hash, sorted_tip_hashes},
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
    fn selection_higher_height_wins() {
        let state = state_with_tips(vec![
            tip_block("lower-height", 3, 100, 100),
            tip_block("higher-height", 4, 1, 1),
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
    fn selection_same_height_higher_blue_score_wins() {
        let state = state_with_tips(vec![
            tip_block("lower-blue", 7, 10, 100),
            tip_block("higher-blue", 7, 11, 1),
        ]);

        assert_eq!(sorted_tip_hashes(&state), vec!["higher-blue", "lower-blue"]);
        assert_eq!(preferred_tip_hash(&state), Some("higher-blue".to_string()));
    }

    #[test]
    fn selection_same_height_and_blue_score_newer_timestamp_wins() {
        let state = state_with_tips(vec![
            tip_block("older", 7, 11, 100),
            tip_block("newer", 7, 11, 101),
        ]);

        assert_eq!(sorted_tip_hashes(&state), vec!["newer", "older"]);
        assert_eq!(preferred_tip_hash(&state), Some("newer".to_string()));
    }

    #[test]
    fn selection_same_height_blue_score_and_timestamp_hash_tie_break_is_deterministic() {
        let state = state_with_tips(vec![
            tip_block("hash-a", 7, 11, 101),
            tip_block("hash-c", 7, 11, 101),
            tip_block("hash-b", 7, 11, 101),
        ]);

        assert_eq!(
            sorted_tip_hashes(&state),
            vec!["hash-c", "hash-b", "hash-a"]
        );
        assert_eq!(preferred_tip_hash(&state), Some("hash-c".to_string()));
    }

    #[test]
    fn selection_sorted_tip_hashes_returns_stable_order_across_repeated_calls() {
        let state = state_with_tips(vec![
            tip_block("tip-2", 2, 10, 300),
            tip_block("tip-4", 4, 1, 1),
            tip_block("tip-3", 3, 20, 200),
            tip_block("tip-1", 2, 10, 301),
        ]);
        let expected = vec!["tip-4", "tip-3", "tip-1", "tip-2"];

        for _ in 0..10 {
            assert_eq!(sorted_tip_hashes(&state), expected);
        }
    }

    #[test]
    fn selection_preferred_tip_hash_returns_first_sorted_tip() {
        let state = state_with_tips(vec![
            tip_block("tip-a", 1, 1, 1),
            tip_block("tip-b", 2, 1, 1),
            tip_block("tip-c", 2, 2, 1),
        ]);
        let sorted = sorted_tip_hashes(&state);

        assert_eq!(preferred_tip_hash(&state), sorted.first().cloned());
        assert_eq!(preferred_tip_hash(&state), Some("tip-c".to_string()));
    }

    #[test]
    fn selection_missing_tip_hash_is_ignored_safely() {
        let mut state = state_with_tips(vec![tip_block("known-tip", 1, 1, 1)]);
        state.dag.tips.insert("missing-tip".to_string());

        assert_eq!(sorted_tip_hashes(&state), vec!["known-tip"]);
        assert_eq!(preferred_tip_hash(&state), Some("known-tip".to_string()));
    }

    #[test]
    fn selection_multiple_sibling_blocks_produce_deterministic_preferred_tip() {
        let state = state_with_tips(vec![
            tip_block("sibling-low-height", 2, 100, 100),
            tip_block("sibling-low-blue", 3, 10, 300),
            tip_block("sibling-old", 3, 20, 100),
            tip_block("sibling-hash-a", 3, 20, 200),
            tip_block("sibling-hash-c", 3, 20, 200),
            tip_block("sibling-hash-b", 3, 20, 200),
        ]);

        assert_eq!(
            sorted_tip_hashes(&state),
            vec![
                "sibling-hash-c",
                "sibling-hash-b",
                "sibling-hash-a",
                "sibling-old",
                "sibling-low-blue",
                "sibling-low-height",
            ]
        );
        assert_eq!(
            preferred_tip_hash(&state),
            Some("sibling-hash-c".to_string())
        );
    }
}
