use std::{cmp::Ordering, collections::BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{state::ChainState, types::Hash, Block};

/// Hard parent-count bound for the reserved v2.4.0 GHOSTDAG selection path.
/// This calculator is non-activating; live consensus remains controlled by the
/// runtime activation gate.
pub const GHOSTDAG_V1_MAX_PARENTS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum SelectionV2Error {
    TooManyParents { observed: usize, max: usize },
    DuplicateParent { hash: Hash },
    MissingParent { hash: Hash },
    MissingParentBlueWork { hash: Hash },
    MissingTipBlock { hash: Hash },
    MissingTipBlueWork { hash: Hash },
    MissingSelectedParentMetadata { hash: Hash },
    SelectedParentCycle { hash: Hash },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionScoreV1 {
    pub blue_work: u128,
    pub blue_score: u64,
    pub height: u64,
    pub hash: Hash,
}

fn score_for_known_block(
    hash: &Hash,
    block: &Block,
    state: &ChainState,
    missing_work: impl FnOnce(Hash) -> SelectionV2Error,
) -> Result<SelectionScoreV1, SelectionV2Error> {
    let blue_work = state
        .dag
        .blue_work
        .get(hash)
        .copied()
        .ok_or_else(|| missing_work(hash.clone()))?;
    Ok(SelectionScoreV1 {
        blue_work,
        blue_score: block.header.blue_score,
        height: block.header.height,
        hash: hash.clone(),
    })
}

/// Compare two fully materialized selection scores.
///
/// Higher cumulative blue work wins, followed by higher blue score and height.
/// The lowest canonical block hash wins the final tie. The ordering is defined
/// so `Ordering::Greater` means `a` is the preferred candidate.
pub fn compare_selection_scores_v1(a: &SelectionScoreV1, b: &SelectionScoreV1) -> Ordering {
    a.blue_work
        .cmp(&b.blue_work)
        .then_with(|| a.blue_score.cmp(&b.blue_score))
        .then_with(|| a.height.cmp(&b.height))
        .then_with(|| b.hash.cmp(&a.hash))
}

/// Calculate the reserved v2.4.0 selected parent without mutating chain state.
///
/// Unlike the legacy helper, this function never silently drops unknown
/// parents or parents whose cumulative blue-work metadata is missing. Any
/// incomplete required input fails closed so orphan/restart timing cannot alter
/// the result.
pub fn calculate_selected_parent_v1(
    block: &Block,
    state: &ChainState,
) -> Result<Option<Hash>, SelectionV2Error> {
    if block.header.parents.len() > GHOSTDAG_V1_MAX_PARENTS {
        return Err(SelectionV2Error::TooManyParents {
            observed: block.header.parents.len(),
            max: GHOSTDAG_V1_MAX_PARENTS,
        });
    }

    let mut seen = BTreeSet::new();
    let mut scores = Vec::with_capacity(block.header.parents.len());
    for parent in &block.header.parents {
        if !seen.insert(parent.clone()) {
            return Err(SelectionV2Error::DuplicateParent {
                hash: parent.clone(),
            });
        }
        let parent_block =
            state
                .dag
                .blocks
                .get(parent)
                .ok_or_else(|| SelectionV2Error::MissingParent {
                    hash: parent.clone(),
                })?;
        scores.push(score_for_known_block(
            parent,
            parent_block,
            state,
            |hash| SelectionV2Error::MissingParentBlueWork { hash },
        )?);
    }

    Ok(scores
        .into_iter()
        .max_by(compare_selection_scores_v1)
        .map(|score| score.hash))
}

/// Calculate the reserved v2.4.0 selected tip from the complete known tip set.
/// Missing tip block/work metadata fails closed instead of being filtered out.
pub fn calculate_selected_tip_v1(state: &ChainState) -> Result<Option<Hash>, SelectionV2Error> {
    let mut tip_hashes = state.dag.tips.iter().cloned().collect::<Vec<_>>();
    tip_hashes.sort();

    let mut scores = Vec::with_capacity(tip_hashes.len());
    for hash in tip_hashes {
        let block = state
            .dag
            .blocks
            .get(&hash)
            .ok_or_else(|| SelectionV2Error::MissingTipBlock { hash: hash.clone() })?;
        scores.push(score_for_known_block(&hash, block, state, |hash| {
            SelectionV2Error::MissingTipBlueWork { hash }
        })?);
    }

    Ok(scores
        .into_iter()
        .max_by(compare_selection_scores_v1)
        .map(|score| score.hash))
}

/// Rebuild a selected-chain projection only from complete persisted/recomputed
/// selected-parent metadata. Missing metadata or a cycle fails closed.
pub fn rebuild_selected_chain_v1(
    state: &ChainState,
    selected_tip: Option<Hash>,
) -> Result<Vec<Hash>, SelectionV2Error> {
    let mut reversed = Vec::new();
    let mut seen = BTreeSet::new();
    let mut cursor = selected_tip;

    while let Some(hash) = cursor {
        if !seen.insert(hash.clone()) {
            return Err(SelectionV2Error::SelectedParentCycle { hash });
        }
        if !state.dag.blocks.contains_key(&hash) {
            return Err(SelectionV2Error::MissingTipBlock { hash });
        }
        reversed.push(hash.clone());
        cursor = state
            .dag
            .selected_parents
            .get(&hash)
            .cloned()
            .ok_or_else(|| SelectionV2Error::MissingSelectedParentMetadata {
                hash: hash.clone(),
            })?;
    }

    reversed.reverse();
    Ok(reversed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{BlockHeader, Transaction},
    };

    fn block(hash: &str, height: u64, blue_score: u64, parents: Vec<String>) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents,
                timestamp: height.saturating_add(1),
                difficulty: 1,
                nonce: 0,
                merkle_root: format!("merkle-{hash}"),
                state_root: format!("state-{hash}"),
                blue_score,
                height,
            },
            transactions: Vec::<Transaction>::new(),
        }
    }

    fn selection_state() -> ChainState {
        let mut state = init_chain_state("selection-v2-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        let a = block("a", 1, 10, vec![genesis.clone()]);
        let b = block("b", 1, 11, vec![genesis]);
        state.dag.blocks.insert(a.hash.clone(), a);
        state.dag.blocks.insert(b.hash.clone(), b);
        state.dag.blue_work.insert("a".to_string(), 100);
        state.dag.blue_work.insert("b".to_string(), 200);
        state
    }

    #[test]
    fn selected_parent_uses_blue_work_before_blue_score_and_parent_order() {
        let state = selection_state();
        let forward = block("candidate", 2, 0, vec!["a".to_string(), "b".to_string()]);
        let reverse = block("candidate", 2, 0, vec!["b".to_string(), "a".to_string()]);

        assert_eq!(
            calculate_selected_parent_v1(&forward, &state).unwrap(),
            Some("b".to_string())
        );
        assert_eq!(
            calculate_selected_parent_v1(&reverse, &state).unwrap(),
            Some("b".to_string())
        );
    }

    #[test]
    fn selected_parent_fails_closed_on_missing_parent_or_work() {
        let mut state = selection_state();
        let missing_parent = block(
            "candidate",
            2,
            0,
            vec!["a".to_string(), "missing".to_string()],
        );
        assert_eq!(
            calculate_selected_parent_v1(&missing_parent, &state),
            Err(SelectionV2Error::MissingParent {
                hash: "missing".to_string()
            })
        );

        state.dag.blue_work.remove("b");
        let missing_work = block("candidate", 2, 0, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(
            calculate_selected_parent_v1(&missing_work, &state),
            Err(SelectionV2Error::MissingParentBlueWork {
                hash: "b".to_string()
            })
        );
    }

    #[test]
    fn lowest_hash_is_canonical_final_tie_break() {
        let mut state = selection_state();
        state.dag.blue_work.insert("a".to_string(), 200);
        state.dag.blocks.get_mut("a").unwrap().header.blue_score = 11;
        let candidate = block("candidate", 2, 0, vec!["b".to_string(), "a".to_string()]);

        assert_eq!(
            calculate_selected_parent_v1(&candidate, &state).unwrap(),
            Some("a".to_string())
        );
    }

    #[test]
    fn selected_tip_is_independent_of_hash_set_iteration() {
        let mut state = selection_state();
        state.dag.tips.clear();
        state.dag.tips.insert("a".to_string());
        state.dag.tips.insert("b".to_string());
        assert_eq!(
            calculate_selected_tip_v1(&state).unwrap(),
            Some("b".to_string())
        );

        state.dag.tips.clear();
        state.dag.tips.insert("b".to_string());
        state.dag.tips.insert("a".to_string());
        assert_eq!(
            calculate_selected_tip_v1(&state).unwrap(),
            Some("b".to_string())
        );
    }

    #[test]
    fn selected_chain_rebuild_requires_complete_metadata() {
        let mut state = selection_state();
        let genesis = state.dag.genesis_hash.clone();
        state
            .dag
            .selected_parents
            .insert("a".to_string(), Some(genesis.clone()));
        assert_eq!(
            rebuild_selected_chain_v1(&state, Some("a".to_string())).unwrap(),
            vec![genesis, "a".to_string()]
        );

        state.dag.selected_parents.remove("a");
        assert_eq!(
            rebuild_selected_chain_v1(&state, Some("a".to_string())),
            Err(SelectionV2Error::MissingSelectedParentMetadata {
                hash: "a".to_string()
            })
        );
    }
}
