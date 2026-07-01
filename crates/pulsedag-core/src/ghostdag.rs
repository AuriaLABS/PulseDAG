use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    selection::calculate_selected_parent,
    state::ChainState,
    types::{Block, Hash},
};

pub const DEFAULT_MERGE_SET_K: usize = 2;

pub fn default_merge_set_k() -> usize {
    DEFAULT_MERGE_SET_K
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeSetColor {
    Blue,
    Red,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeSetDiagnostics {
    pub selected_parent: Option<Hash>,
    pub blue_score: u64,
    pub merge_set_size: usize,
    pub merge_set_blues_count: usize,
    pub merge_set_reds_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeSetClassification {
    pub selected_parent: Option<Hash>,
    pub merge_set: Vec<Hash>,
    pub blues: Vec<Hash>,
    pub reds: Vec<Hash>,
    pub blue_score: u64,
    pub blue_work: u128,
    pub diagnostics: MergeSetDiagnostics,
}

fn ancestors_inclusive(hash: &Hash, state: &ChainState) -> HashSet<Hash> {
    let mut seen = HashSet::new();
    let mut stack = vec![hash.clone()];
    while let Some(cursor) = stack.pop() {
        if !seen.insert(cursor.clone()) {
            continue;
        }
        if let Some(block) = state.dag.blocks.get(&cursor) {
            stack.extend(block.header.parents.iter().cloned());
        }
    }
    seen
}

fn is_ancestor_of(candidate: &Hash, descendant: &Hash, state: &ChainState) -> bool {
    if candidate == descendant {
        return true;
    }
    let mut seen = HashSet::new();
    let mut stack = vec![descendant.clone()];
    while let Some(cursor) = stack.pop() {
        if !seen.insert(cursor.clone()) {
            continue;
        }
        if &cursor == candidate {
            return true;
        }
        if let Some(block) = state.dag.blocks.get(&cursor) {
            stack.extend(block.header.parents.iter().cloned());
        }
    }
    false
}

fn are_related(a: &Hash, b: &Hash, state: &ChainState) -> bool {
    is_ancestor_of(a, b, state) || is_ancestor_of(b, a, state)
}

pub fn calculate_merge_set(block: &Block, state: &ChainState) -> Vec<Hash> {
    let Some(selected_parent) = calculate_selected_parent(block, state) else {
        return Vec::new();
    };
    let selected_parent_past = ancestors_inclusive(&selected_parent, state);
    let mut merge_set = BTreeSet::new();

    for parent in &block.header.parents {
        if parent == &selected_parent {
            continue;
        }
        for ancestor in ancestors_inclusive(parent, state) {
            if !selected_parent_past.contains(&ancestor) {
                merge_set.insert(ancestor);
            }
        }
    }

    merge_set.into_iter().collect()
}

pub fn classify_merge_set(block: &Block, state: &ChainState) -> MergeSetClassification {
    classify_merge_set_with_k(block, state, state.dag.merge_set_k)
}

pub fn classify_merge_set_with_k(
    block: &Block,
    state: &ChainState,
    k: usize,
) -> MergeSetClassification {
    let selected_parent = calculate_selected_parent(block, state);
    let mut merge_set = calculate_merge_set(block, state);
    merge_set.sort_by(|a, b| {
        let block_a = state.dag.blocks.get(a);
        let block_b = state.dag.blocks.get(b);
        match (block_a, block_b) {
            (Some(a_block), Some(b_block)) => {
                crate::selection::compare_selected_parent_candidates(b_block, a_block)
                    .then_with(|| a.cmp(b))
            }
            _ => a.cmp(b),
        }
    });

    let mut blues = Vec::new();
    let mut reds = Vec::new();
    for candidate in &merge_set {
        let anticone_blues = blues
            .iter()
            .filter(|blue| !are_related(candidate, blue, state))
            .count();
        if anticone_blues < k {
            blues.push(candidate.clone());
        } else {
            reds.push(candidate.clone());
        }
    }

    let selected_parent_score = selected_parent
        .as_ref()
        .and_then(|hash| state.dag.blocks.get(hash))
        .map(|block| block.header.blue_score)
        .unwrap_or(0);
    let blue_score = selected_parent_score
        .saturating_add(1)
        .saturating_add(blues.len() as u64);
    let selected_parent_work = selected_parent
        .as_ref()
        .and_then(|hash| state.dag.blue_work.get(hash).copied())
        .unwrap_or(selected_parent_score as u128);
    let blue_work = selected_parent_work
        .saturating_add(1)
        .saturating_add(blues.len() as u128);

    MergeSetClassification {
        selected_parent: selected_parent.clone(),
        merge_set,
        blues: blues.clone(),
        reds: reds.clone(),
        blue_score,
        blue_work,
        diagnostics: MergeSetDiagnostics {
            selected_parent,
            blue_score,
            merge_set_size: blues.len() + reds.len(),
            merge_set_blues_count: blues.len(),
            merge_set_reds_count: reds.len(),
        },
    }
}
