use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    selection_v2::{
        calculate_selected_parent_v1, compare_selection_scores_v1, SelectionScoreV1,
        SelectionV2Error,
    },
    state::ChainState,
    types::{Block, Hash},
};

pub const GHOSTDAG_V1_K: usize = 2;
pub const GHOSTDAG_V1_MAX_ANCESTOR_VISITS: usize = 65_536;
pub const GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS: usize = 4_096;
pub const GHOSTDAG_V1_MAX_RELATION_VISITS: usize = 262_144;
const CLASSIFICATION_DIGEST_DOMAIN: &[u8] = b"PulseDAG:ghostdag-v1-classification:v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct GhostdagV1Limits {
    pub k: usize,
    pub max_ancestor_visits: usize,
    pub max_merge_set_blocks: usize,
    pub max_relation_visits: usize,
}

impl Default for GhostdagV1Limits {
    fn default() -> Self {
        Self {
            k: GHOSTDAG_V1_K,
            max_ancestor_visits: GHOSTDAG_V1_MAX_ANCESTOR_VISITS,
            max_merge_set_blocks: GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS,
            max_relation_visits: GHOSTDAG_V1_MAX_RELATION_VISITS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum GhostdagV1Error {
    Selection { error: SelectionV2Error },
    MissingBlock { hash: Hash },
    MissingBlueWork { hash: Hash },
    AncestorVisitLimitExceeded { max: usize },
    MergeSetSizeLimitExceeded { observed: usize, max: usize },
    RelationVisitLimitExceeded { max: usize },
}

impl From<SelectionV2Error> for GhostdagV1Error {
    fn from(error: SelectionV2Error) -> Self {
        Self::Selection { error }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundedMergeSetV1 {
    pub selected_parent: Option<Hash>,
    pub merge_set: Vec<Hash>,
    pub ancestor_visits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GhostdagV1Classification {
    pub selected_parent: Option<Hash>,
    pub merge_set: Vec<Hash>,
    pub blues: Vec<Hash>,
    pub reds: Vec<Hash>,
    /// Deterministic score for the candidate itself. The candidate header must
    /// commit this value before the v2 block hash is finalized.
    pub blue_score: u64,
    /// Deterministic cumulative work metadata for subsequent selection.
    ///
    /// V1 deliberately freezes the same cumulative unit semantics used by the
    /// existing PulseDAG metadata path: selected-parent work plus one unit for
    /// the candidate and one unit per accepted blue merge-set member. Any
    /// future PoW-weighted reinterpretation requires a separately versioned
    /// consensus rule rather than changing this calculator in place.
    pub blue_work: u128,
    pub k: usize,
    pub ancestor_visits: usize,
    pub relation_visits: usize,
    pub classification_digest: String,
}

fn consume_visit(counter: &mut usize, max: usize, relation: bool) -> Result<(), GhostdagV1Error> {
    if *counter >= max {
        return if relation {
            Err(GhostdagV1Error::RelationVisitLimitExceeded { max })
        } else {
            Err(GhostdagV1Error::AncestorVisitLimitExceeded { max })
        };
    }
    *counter += 1;
    Ok(())
}

fn ancestors_inclusive_bounded(
    hash: &Hash,
    state: &ChainState,
    visits: &mut usize,
    max_visits: usize,
) -> Result<BTreeSet<Hash>, GhostdagV1Error> {
    let mut seen = BTreeSet::new();
    let mut pending = BTreeSet::from([hash.clone()]);
    while let Some(cursor) = pending.pop_first() {
        if !seen.insert(cursor.clone()) {
            continue;
        }
        consume_visit(visits, max_visits, false)?;
        let block = state
            .dag
            .blocks
            .get(&cursor)
            .ok_or_else(|| GhostdagV1Error::MissingBlock {
                hash: cursor.clone(),
            })?;
        for parent in &block.header.parents {
            if !seen.contains(parent) {
                pending.insert(parent.clone());
            }
        }
    }
    Ok(seen)
}

fn calculate_bounded_merge_set_with_limits(
    block: &Block,
    state: &ChainState,
    limits: GhostdagV1Limits,
) -> Result<BoundedMergeSetV1, GhostdagV1Error> {
    let selected_parent = calculate_selected_parent_v1(block, state)?;
    let Some(selected_parent_hash) = selected_parent.clone() else {
        return Ok(BoundedMergeSetV1 {
            selected_parent: None,
            merge_set: Vec::new(),
            ancestor_visits: 0,
        });
    };
    let mut visits = 0;
    let selected_parent_past = ancestors_inclusive_bounded(
        &selected_parent_hash,
        state,
        &mut visits,
        limits.max_ancestor_visits,
    )?;
    let mut merge_set = BTreeSet::new();
    let mut parents = block.header.parents.clone();
    parents.sort();
    for parent in parents {
        if parent == selected_parent_hash {
            continue;
        }
        let ancestors =
            ancestors_inclusive_bounded(&parent, state, &mut visits, limits.max_ancestor_visits)?;
        for ancestor in ancestors {
            if selected_parent_past.contains(&ancestor) {
                continue;
            }
            merge_set.insert(ancestor);
            if merge_set.len() > limits.max_merge_set_blocks {
                return Err(GhostdagV1Error::MergeSetSizeLimitExceeded {
                    observed: merge_set.len(),
                    max: limits.max_merge_set_blocks,
                });
            }
        }
    }
    Ok(BoundedMergeSetV1 {
        selected_parent,
        merge_set: merge_set.into_iter().collect(),
        ancestor_visits: visits,
    })
}

pub fn calculate_bounded_merge_set_v1(
    block: &Block,
    state: &ChainState,
) -> Result<BoundedMergeSetV1, GhostdagV1Error> {
    calculate_bounded_merge_set_with_limits(block, state, GhostdagV1Limits::default())
}

fn is_ancestor_bounded(
    ancestor: &Hash,
    descendant: &Hash,
    state: &ChainState,
    visits: &mut usize,
    max_visits: usize,
) -> Result<bool, GhostdagV1Error> {
    if ancestor == descendant {
        return Ok(true);
    }
    let mut seen = BTreeSet::new();
    let mut pending = BTreeSet::from([descendant.clone()]);
    while let Some(cursor) = pending.pop_first() {
        if !seen.insert(cursor.clone()) {
            continue;
        }
        consume_visit(visits, max_visits, true)?;
        if &cursor == ancestor {
            return Ok(true);
        }
        let block = state
            .dag
            .blocks
            .get(&cursor)
            .ok_or_else(|| GhostdagV1Error::MissingBlock {
                hash: cursor.clone(),
            })?;
        for parent in &block.header.parents {
            if !seen.contains(parent) {
                pending.insert(parent.clone());
            }
        }
    }
    Ok(false)
}

fn are_related_bounded(
    a: &Hash,
    b: &Hash,
    state: &ChainState,
    visits: &mut usize,
    max_visits: usize,
) -> Result<bool, GhostdagV1Error> {
    if is_ancestor_bounded(a, b, state, visits, max_visits)? {
        return Ok(true);
    }
    is_ancestor_bounded(b, a, state, visits, max_visits)
}

fn classification_digest(
    selected_parent: &Option<Hash>,
    merge_set: &[Hash],
    blues: &[Hash],
    reds: &[Hash],
    k: usize,
) -> String {
    fn write_hash(hasher: &mut Sha256, hash: &str) {
        hasher.update((hash.len() as u32).to_le_bytes());
        hasher.update(hash.as_bytes());
    }
    fn write_hashes(hasher: &mut Sha256, hashes: &[Hash]) {
        hasher.update((hashes.len() as u32).to_le_bytes());
        for hash in hashes {
            write_hash(hasher, hash);
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(CLASSIFICATION_DIGEST_DOMAIN);
    hasher.update((k as u64).to_le_bytes());
    match selected_parent {
        Some(hash) => {
            hasher.update([1]);
            write_hash(&mut hasher, hash);
        }
        None => hasher.update([0]),
    }
    write_hashes(&mut hasher, merge_set);
    write_hashes(&mut hasher, blues);
    write_hashes(&mut hasher, reds);
    hex::encode(hasher.finalize())
}

fn derive_score_work_v1(
    selected_parent: &Option<Hash>,
    blues: &[Hash],
    state: &ChainState,
) -> Result<(u64, u128), GhostdagV1Error> {
    let (selected_parent_score, selected_parent_work) = match selected_parent {
        Some(hash) => {
            let parent = state
                .dag
                .blocks
                .get(hash)
                .ok_or_else(|| GhostdagV1Error::MissingBlock { hash: hash.clone() })?;
            let work = state
                .dag
                .blue_work
                .get(hash)
                .copied()
                .ok_or_else(|| GhostdagV1Error::MissingBlueWork { hash: hash.clone() })?;
            (parent.header.blue_score, work)
        }
        None => (0, 0),
    };
    let blue_count_score = u64::try_from(blues.len()).unwrap_or(u64::MAX);
    let blue_count_work = blues.len() as u128;
    Ok((
        selected_parent_score
            .saturating_add(1)
            .saturating_add(blue_count_score),
        selected_parent_work
            .saturating_add(1)
            .saturating_add(blue_count_work),
    ))
}

fn classify_merge_set_with_limits(
    block: &Block,
    state: &ChainState,
    limits: GhostdagV1Limits,
) -> Result<GhostdagV1Classification, GhostdagV1Error> {
    let bounded = calculate_bounded_merge_set_with_limits(block, state, limits)?;
    let mut candidates = Vec::with_capacity(bounded.merge_set.len());
    for hash in &bounded.merge_set {
        let candidate = state
            .dag
            .blocks
            .get(hash)
            .ok_or_else(|| GhostdagV1Error::MissingBlock { hash: hash.clone() })?;
        let blue_work = state
            .dag
            .blue_work
            .get(hash)
            .copied()
            .ok_or_else(|| GhostdagV1Error::MissingBlueWork { hash: hash.clone() })?;
        candidates.push((
            hash.clone(),
            SelectionScoreV1 {
                blue_work,
                blue_score: candidate.header.blue_score,
                height: candidate.header.height,
                hash: hash.clone(),
            },
        ));
    }
    candidates.sort_by(|(_, a), (_, b)| compare_selection_scores_v1(b, a));
    let merge_set = candidates
        .iter()
        .map(|(hash, _)| hash.clone())
        .collect::<Vec<_>>();
    let mut blues = Vec::new();
    let mut reds = Vec::new();
    let mut relation_visits = 0;
    for candidate in &merge_set {
        let mut anticone_blues = 0;
        for blue in &blues {
            if !are_related_bounded(
                candidate,
                blue,
                state,
                &mut relation_visits,
                limits.max_relation_visits,
            )? {
                anticone_blues += 1;
                if anticone_blues >= limits.k {
                    break;
                }
            }
        }
        if anticone_blues < limits.k {
            blues.push(candidate.clone());
        } else {
            reds.push(candidate.clone());
        }
    }
    let (blue_score, blue_work) = derive_score_work_v1(&bounded.selected_parent, &blues, state)?;
    let digest = classification_digest(
        &bounded.selected_parent,
        &merge_set,
        &blues,
        &reds,
        limits.k,
    );
    Ok(GhostdagV1Classification {
        selected_parent: bounded.selected_parent,
        merge_set,
        blues,
        reds,
        blue_score,
        blue_work,
        k: limits.k,
        ancestor_visits: bounded.ancestor_visits,
        relation_visits,
        classification_digest: digest,
    })
}

pub fn classify_merge_set_v1(
    block: &Block,
    state: &ChainState,
) -> Result<GhostdagV1Classification, GhostdagV1Error> {
    classify_merge_set_with_limits(block, state, GhostdagV1Limits::default())
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

    fn parallel_parent_state() -> ChainState {
        let mut state = init_chain_state("ghostdag-v1-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        for (hash, score, work) in [
            ("p0", 40, 400_u128),
            ("p1", 30, 300_u128),
            ("p2", 20, 200_u128),
            ("p3", 10, 100_u128),
        ] {
            let candidate = block(hash, 1, score, vec![genesis.clone()]);
            state.dag.blocks.insert(hash.to_string(), candidate);
            state.dag.blue_work.insert(hash.to_string(), work);
        }
        state
    }

    #[test]
    fn merge_set_and_classification_are_parent_order_independent() {
        let state = parallel_parent_state();
        let forward = block(
            "candidate",
            2,
            0,
            vec!["p0".into(), "p1".into(), "p2".into(), "p3".into()],
        );
        let reverse = block(
            "candidate",
            2,
            0,
            vec!["p3".into(), "p2".into(), "p1".into(), "p0".into()],
        );
        let a = classify_merge_set_v1(&forward, &state).unwrap();
        let b = classify_merge_set_v1(&reverse, &state).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.selected_parent, Some("p0".to_string()));
        assert_eq!(a.merge_set, vec!["p1", "p2", "p3"]);
        assert_eq!(a.blues, vec!["p1", "p2"]);
        assert_eq!(a.reds, vec!["p3"]);
        assert_eq!(a.blue_score, 43);
        assert_eq!(a.blue_work, 403);
    }

    #[test]
    fn missing_merge_set_ancestor_fails_closed() {
        let mut state = parallel_parent_state();
        state.dag.blocks.get_mut("p1").unwrap().header.parents = vec!["missing".to_string()];
        let candidate = block("candidate", 2, 0, vec!["p0".into(), "p1".into()]);
        assert_eq!(
            classify_merge_set_v1(&candidate, &state),
            Err(GhostdagV1Error::MissingBlock {
                hash: "missing".to_string()
            })
        );
    }

    #[test]
    fn merge_set_overflow_is_explicit_and_deterministic() {
        let state = parallel_parent_state();
        let candidate = block(
            "candidate",
            2,
            0,
            vec!["p0".into(), "p1".into(), "p2".into()],
        );
        let limits = GhostdagV1Limits {
            max_merge_set_blocks: 1,
            ..GhostdagV1Limits::default()
        };
        assert_eq!(
            calculate_bounded_merge_set_with_limits(&candidate, &state, limits),
            Err(GhostdagV1Error::MergeSetSizeLimitExceeded {
                observed: 2,
                max: 1
            })
        );
    }

    #[test]
    fn relation_budget_overflow_fails_closed() {
        let state = parallel_parent_state();
        let candidate = block(
            "candidate",
            2,
            0,
            vec!["p0".into(), "p1".into(), "p2".into()],
        );
        let limits = GhostdagV1Limits {
            max_relation_visits: 1,
            ..GhostdagV1Limits::default()
        };
        assert_eq!(
            classify_merge_set_with_limits(&candidate, &state, limits),
            Err(GhostdagV1Error::RelationVisitLimitExceeded { max: 1 })
        );
    }

    #[test]
    fn classification_digest_changes_when_partition_changes() {
        let state = parallel_parent_state();
        let candidate = block(
            "candidate",
            2,
            0,
            vec!["p0".into(), "p1".into(), "p2".into(), "p3".into()],
        );
        let k2 = classify_merge_set_with_limits(&candidate, &state, GhostdagV1Limits::default())
            .unwrap();
        let k3 = classify_merge_set_with_limits(
            &candidate,
            &state,
            GhostdagV1Limits {
                k: 3,
                ..GhostdagV1Limits::default()
            },
        )
        .unwrap();
        assert_ne!(k2.reds, k3.reds);
        assert_ne!(k2.classification_digest, k3.classification_digest);
        assert_ne!(k2.blue_score, k3.blue_score);
        assert_ne!(k2.blue_work, k3.blue_work);
    }

    #[test]
    fn real_relation_visit_counter_accepts_exact_limit_and_rejects_one_more() {
        let mut visits = GHOSTDAG_V1_MAX_RELATION_VISITS - 1;
        consume_visit(&mut visits, GHOSTDAG_V1_MAX_RELATION_VISITS, true)
            .expect("the exact frozen relation-visit budget must be accepted");
        assert_eq!(visits, GHOSTDAG_V1_MAX_RELATION_VISITS);
        assert_eq!(
            consume_visit(&mut visits, GHOSTDAG_V1_MAX_RELATION_VISITS, true,),
            Err(GhostdagV1Error::RelationVisitLimitExceeded {
                max: GHOSTDAG_V1_MAX_RELATION_VISITS,
            })
        );
    }
}
