use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{state::ChainState, types::Hash};

pub const GHOSTDAG_V1_ORDERING_VERSION: &str = "ghostdag-v1-topological-v1";
const ORDERING_DIGEST_DOMAIN: &[u8] = b"PulseDAG:ghostdag-v1-ordering:v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum OrderingV2Error {
    DuplicateSelectedChainBlock {
        hash: Hash,
    },
    MissingSelectedChainBlock {
        hash: Hash,
    },
    MissingSelectedParentMetadata {
        hash: Hash,
    },
    SelectedChainParentMismatch {
        hash: Hash,
        expected_parent: Option<Hash>,
        observed_parent: Option<Hash>,
    },
    MissingClassificationMetadata {
        anchor: Hash,
    },
    ClassificationReferencesMissingBlock {
        anchor: Hash,
        hash: Hash,
    },
    ClassificationReferencesSelectedChainBlock {
        anchor: Hash,
        hash: Hash,
    },
    DuplicateClassification {
        hash: Hash,
        first_anchor: Hash,
        second_anchor: Hash,
    },
    BlueRedOverlap {
        anchor: Hash,
        hash: Hash,
    },
    UnclassifiedBlock {
        hash: Hash,
    },
    MissingParent {
        block: Hash,
        parent: Hash,
    },
    MissingBlueWork {
        hash: Hash,
    },
    TopologyCycle {
        remaining: Vec<Hash>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum OrderingClass {
    Blue = 0,
    Red = 1,
    SelectedChain = 2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderedDagV2 {
    pub ordering_version: String,
    pub blocks: Vec<Hash>,
    pub digest: String,
}

type ReadyKey = (usize, u8, Reverse<u128>, Reverse<u64>, Reverse<u64>, Hash);

fn ordering_digest(order: &[Hash]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ORDERING_DIGEST_DOMAIN);
    hasher.update((GHOSTDAG_V1_ORDERING_VERSION.len() as u32).to_le_bytes());
    hasher.update(GHOSTDAG_V1_ORDERING_VERSION.as_bytes());
    hasher.update((order.len() as u64).to_le_bytes());
    for hash in order {
        hasher.update((hash.len() as u32).to_le_bytes());
        hasher.update(hash.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn ready_key(
    hash: &Hash,
    state: &ChainState,
    priorities: &BTreeMap<Hash, (usize, OrderingClass)>,
) -> Result<ReadyKey, OrderingV2Error> {
    let block = state
        .dag
        .blocks
        .get(hash)
        .ok_or_else(|| OrderingV2Error::MissingSelectedChainBlock { hash: hash.clone() })?;
    let blue_work = state
        .dag
        .blue_work
        .get(hash)
        .copied()
        .ok_or_else(|| OrderingV2Error::MissingBlueWork { hash: hash.clone() })?;
    let (anchor_index, class) = priorities
        .get(hash)
        .copied()
        .ok_or_else(|| OrderingV2Error::UnclassifiedBlock { hash: hash.clone() })?;
    Ok((
        anchor_index,
        class as u8,
        Reverse(blue_work),
        Reverse(block.header.blue_score),
        Reverse(block.header.height),
        hash.clone(),
    ))
}

fn build_priorities(
    state: &ChainState,
) -> Result<BTreeMap<Hash, (usize, OrderingClass)>, OrderingV2Error> {
    let mut priorities = BTreeMap::new();
    let selected_set = state
        .dag
        .selected_chain
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if selected_set.len() != state.dag.selected_chain.len() {
        let mut seen = BTreeSet::new();
        let duplicate = state
            .dag
            .selected_chain
            .iter()
            .find(|hash| !seen.insert((*hash).clone()))
            .cloned()
            .unwrap_or_default();
        return Err(OrderingV2Error::DuplicateSelectedChainBlock { hash: duplicate });
    }

    for (index, hash) in state.dag.selected_chain.iter().enumerate() {
        if !state.dag.blocks.contains_key(hash) {
            return Err(OrderingV2Error::MissingSelectedChainBlock { hash: hash.clone() });
        }
        let observed_parent = state
            .dag
            .selected_parents
            .get(hash)
            .cloned()
            .ok_or_else(|| OrderingV2Error::MissingSelectedParentMetadata { hash: hash.clone() })?;
        let expected_parent = if index == 0 {
            None
        } else {
            Some(state.dag.selected_chain[index - 1].clone())
        };
        if observed_parent != expected_parent {
            return Err(OrderingV2Error::SelectedChainParentMismatch {
                hash: hash.clone(),
                expected_parent,
                observed_parent,
            });
        }
        priorities.insert(hash.clone(), (index, OrderingClass::SelectedChain));
    }

    let mut classified_by = BTreeMap::<Hash, Hash>::new();
    for (index, anchor) in state.dag.selected_chain.iter().enumerate() {
        let Some(blues) = state.dag.merge_set_blues.get(anchor) else {
            return Err(OrderingV2Error::MissingClassificationMetadata {
                anchor: anchor.clone(),
            });
        };
        let Some(reds) = state.dag.merge_set_reds.get(anchor) else {
            return Err(OrderingV2Error::MissingClassificationMetadata {
                anchor: anchor.clone(),
            });
        };
        let red_set = reds.iter().cloned().collect::<BTreeSet<_>>();

        for (class, hashes) in [(OrderingClass::Blue, blues), (OrderingClass::Red, reds)] {
            let mut local_seen = BTreeSet::new();
            for hash in hashes {
                if !local_seen.insert(hash.clone()) {
                    let first_anchor = classified_by
                        .get(hash)
                        .cloned()
                        .unwrap_or_else(|| anchor.clone());
                    return Err(OrderingV2Error::DuplicateClassification {
                        hash: hash.clone(),
                        first_anchor,
                        second_anchor: anchor.clone(),
                    });
                }
                if class == OrderingClass::Blue && red_set.contains(hash) {
                    return Err(OrderingV2Error::BlueRedOverlap {
                        anchor: anchor.clone(),
                        hash: hash.clone(),
                    });
                }
                if !state.dag.blocks.contains_key(hash) {
                    return Err(OrderingV2Error::ClassificationReferencesMissingBlock {
                        anchor: anchor.clone(),
                        hash: hash.clone(),
                    });
                }
                if selected_set.contains(hash) {
                    return Err(
                        OrderingV2Error::ClassificationReferencesSelectedChainBlock {
                            anchor: anchor.clone(),
                            hash: hash.clone(),
                        },
                    );
                }
                if let Some(first_anchor) = classified_by.insert(hash.clone(), anchor.clone()) {
                    return Err(OrderingV2Error::DuplicateClassification {
                        hash: hash.clone(),
                        first_anchor,
                        second_anchor: anchor.clone(),
                    });
                }
                priorities.insert(hash.clone(), (index, class));
            }
        }
    }

    for hash in state.dag.blocks.keys() {
        if !priorities.contains_key(hash) {
            return Err(OrderingV2Error::UnclassifiedBlock { hash: hash.clone() });
        }
    }

    Ok(priorities)
}

/// Derive the reserved v2.4.0 authoritative DAG order without mutating state.
///
/// The calculator is intentionally fail-closed: every accepted block must be
/// on the selected chain or classified exactly once by a selected-chain anchor,
/// every referenced parent must be present, every block must have blue-work
/// metadata, and the accepted graph must be acyclic.
///
/// Among currently topology-ready blocks, priority is frozen as:
/// selected-chain anchor index, blue before red before selected-chain anchor,
/// then descending blue work, descending blue score, descending height, and
/// finally ascending canonical hash. Parent-before-child topology always wins.
pub fn derive_ordered_dag_v2(state: &ChainState) -> Result<OrderedDagV2, OrderingV2Error> {
    let priorities = build_priorities(state)?;
    let mut indegree = BTreeMap::<Hash, usize>::new();
    let mut children = BTreeMap::<Hash, BTreeSet<Hash>>::new();

    for (hash, block) in &state.dag.blocks {
        indegree.entry(hash.clone()).or_insert(0);
        let mut parent_set = BTreeSet::new();
        for parent in &block.header.parents {
            if !parent_set.insert(parent.clone()) {
                continue;
            }
            if !state.dag.blocks.contains_key(parent) {
                return Err(OrderingV2Error::MissingParent {
                    block: hash.clone(),
                    parent: parent.clone(),
                });
            }
            *indegree.entry(hash.clone()).or_insert(0) += 1;
            children
                .entry(parent.clone())
                .or_default()
                .insert(hash.clone());
        }
    }

    let mut ready = BTreeSet::<ReadyKey>::new();
    for (hash, degree) in &indegree {
        if *degree == 0 {
            ready.insert(ready_key(hash, state, &priorities)?);
        }
    }

    let mut ordered = Vec::with_capacity(state.dag.blocks.len());
    while let Some(key) = ready.pop_first() {
        let hash = key.5;
        ordered.push(hash.clone());
        if let Some(block_children) = children.get(&hash) {
            for child in block_children {
                let degree = indegree
                    .get_mut(child)
                    .expect("child indegree is initialized from accepted blocks");
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    ready.insert(ready_key(child, state, &priorities)?);
                }
            }
        }
    }

    if ordered.len() != state.dag.blocks.len() {
        let ordered_set = ordered.iter().cloned().collect::<BTreeSet<_>>();
        let remaining = state
            .dag
            .blocks
            .keys()
            .filter(|hash| !ordered_set.contains(*hash))
            .cloned()
            .collect();
        return Err(OrderingV2Error::TopologyCycle { remaining });
    }

    let digest = ordering_digest(&ordered);
    Ok(OrderedDagV2 {
        ordering_version: GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        blocks: ordered,
        digest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{Block, BlockHeader},
    };

    fn block(hash: &str, parents: Vec<&str>, height: u64, blue_score: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 1,
                parents: parents.into_iter().map(str::to_string).collect(),
                timestamp: height.saturating_add(10),
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

    fn diamond_state(reverse_insert: bool) -> ChainState {
        let mut state = init_chain_state("ordering-v2-test".to_string());
        let genesis = state.dag.genesis_hash.clone();
        let a = block("a", vec![&genesis], 1, 10);
        let b = block("b", vec![&genesis], 1, 9);
        let c = block("c", vec!["a", "b"], 2, 20);
        let entries = if reverse_insert {
            vec![c.clone(), b.clone(), a.clone()]
        } else {
            vec![a.clone(), b.clone(), c.clone()]
        };
        for candidate in entries {
            state.dag.blue_work.insert(
                candidate.hash.clone(),
                candidate.header.blue_score as u128 * 10,
            );
            state.dag.blocks.insert(candidate.hash.clone(), candidate);
        }
        state
            .dag
            .selected_parents
            .insert("a".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("b".into(), Some(genesis.clone()));
        state
            .dag
            .selected_parents
            .insert("c".into(), Some("a".into()));
        state.dag.selected_chain = vec![genesis.clone(), "a".into(), "c".into()];
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis, vec![]);
        state.dag.merge_set_blues.insert("a".into(), vec![]);
        state.dag.merge_set_reds.insert("a".into(), vec![]);
        state
            .dag
            .merge_set_blues
            .insert("c".into(), vec!["b".into()]);
        state.dag.merge_set_reds.insert("c".into(), vec![]);
        state
    }

    #[test]
    fn diamond_order_is_topological_and_arrival_independent() {
        let a = derive_ordered_dag_v2(&diamond_state(false)).unwrap();
        let b = derive_ordered_dag_v2(&diamond_state(true)).unwrap();

        assert_eq!(a, b);
        assert_eq!(a.blocks[1..], ["a", "b", "c"]);
        assert_eq!(a.ordering_version, GHOSTDAG_V1_ORDERING_VERSION);
    }

    #[test]
    fn parent_order_does_not_change_order_or_digest() {
        let state = diamond_state(false);
        let mut permuted = state.clone();
        permuted
            .dag
            .blocks
            .get_mut("c")
            .unwrap()
            .header
            .parents
            .reverse();

        assert_eq!(
            derive_ordered_dag_v2(&state).unwrap(),
            derive_ordered_dag_v2(&permuted).unwrap()
        );
    }

    #[test]
    fn missing_parent_fails_closed() {
        let mut state = diamond_state(false);
        state.dag.blocks.get_mut("b").unwrap().header.parents = vec!["missing".into()];

        assert_eq!(
            derive_ordered_dag_v2(&state),
            Err(OrderingV2Error::MissingParent {
                block: "b".into(),
                parent: "missing".into()
            })
        );
    }

    #[test]
    fn incomplete_classification_fails_closed() {
        let mut state = diamond_state(false);
        state.dag.merge_set_blues.get_mut("c").unwrap().clear();

        assert_eq!(
            derive_ordered_dag_v2(&state),
            Err(OrderingV2Error::UnclassifiedBlock { hash: "b".into() })
        );
    }

    #[test]
    fn selected_chain_parent_mismatch_fails_closed() {
        let mut state = diamond_state(false);
        state
            .dag
            .selected_parents
            .insert("c".into(), Some("b".into()));

        assert_eq!(
            derive_ordered_dag_v2(&state),
            Err(OrderingV2Error::SelectedChainParentMismatch {
                hash: "c".into(),
                expected_parent: Some("a".into()),
                observed_parent: Some("b".into())
            })
        );
    }

    #[test]
    fn blue_red_overlap_fails_closed() {
        let mut state = diamond_state(false);
        state
            .dag
            .merge_set_reds
            .get_mut("c")
            .unwrap()
            .push("b".into());

        assert_eq!(
            derive_ordered_dag_v2(&state),
            Err(OrderingV2Error::BlueRedOverlap {
                anchor: "c".into(),
                hash: "b".into()
            })
        );
    }
}
