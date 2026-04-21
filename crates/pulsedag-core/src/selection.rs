use std::cmp::Ordering;

use crate::{state::ChainState, types::{Block, Hash}};

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
        .filter_map(|hash| state.dag.blocks.get(hash).map(|block| (hash.clone(), block)))
        .collect::<Vec<_>>();
    tips.sort_by(|(_, a), (_, b)| compare_tip_blocks(b, a));
    tips.into_iter().map(|(hash, _)| hash).collect()
}

pub fn preferred_tip_hash(state: &ChainState) -> Option<Hash> {
    sorted_tip_hashes(state).into_iter().next()
}
