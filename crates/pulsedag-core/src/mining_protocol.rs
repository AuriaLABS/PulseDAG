use serde::{Deserialize, Serialize};

use crate::{
    errors::PulseError,
    ghostdag_v1::{classify_merge_set_v1, GhostdagV1Classification, GhostdagV1Error},
    header_v2::canonicalize_block_parents_v2,
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    pow_protocol::{resolve_pow_validation_path, PowValidationPath},
    protocol::{ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V2},
    selection_v2::{
        calculate_selected_tip_v1, compare_selection_scores_v1, SelectionScoreV1,
        SelectionV2Error, GHOSTDAG_V1_MAX_PARENTS,
    },
    state::ChainState,
    types::{Block, BlockHeader, Hash},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum MiningParentExclusionReasonV1 {
    ParentCountLimit { max: usize },
    AncestorVisitLimit { max: usize },
    MergeSetSizeLimit { observed: usize, max: usize },
    RelationVisitLimit { max: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MiningParentExclusionV1 {
    pub hash: Hash,
    pub reason: MiningParentExclusionReasonV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivatedV2MiningParentContext {
    pub selected_tip: Hash,
    pub parents: Vec<Hash>,
    pub included_parallel_parents: Vec<Hash>,
    pub excluded_parallel_parents: Vec<MiningParentExclusionV1>,
    pub selected_parent: Hash,
    pub blue_score: u64,
    pub blue_work: u128,
    pub merge_set: Vec<Hash>,
    pub blues: Vec<Hash>,
    pub reds: Vec<Hash>,
    pub classification_digest: String,
}

fn invalid_context(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "activated-v2 mining parent context: {}",
        message.into()
    ))
}

fn selection_error(error: SelectionV2Error) -> PulseError {
    invalid_context(format!("selected-tip derivation failed: {error:?}"))
}

fn ghostdag_error(error: GhostdagV1Error) -> PulseError {
    invalid_context(format!("GHOSTDAG parent classification failed: {error:?}"))
}

fn exclusion_from_limit(error: GhostdagV1Error) -> Option<MiningParentExclusionReasonV1> {
    match error {
        GhostdagV1Error::AncestorVisitLimitExceeded { max } => {
            Some(MiningParentExclusionReasonV1::AncestorVisitLimit { max })
        }
        GhostdagV1Error::MergeSetSizeLimitExceeded { observed, max } => {
            Some(MiningParentExclusionReasonV1::MergeSetSizeLimit { observed, max })
        }
        GhostdagV1Error::RelationVisitLimitExceeded { max } => {
            Some(MiningParentExclusionReasonV1::RelationVisitLimit { max })
        }
        _ => None,
    }
}

fn candidate_for_parents(state: &ChainState, parents: Vec<Hash>) -> Result<Block, PulseError> {
    let parents = canonicalize_block_parents_v2(&parents)?;
    let height = parents
        .iter()
        .map(|parent| {
            state
                .dag
                .blocks
                .get(parent)
                .map(|block| block.header.height.saturating_add(1))
                .ok_or_else(|| invalid_context(format!("missing parent {parent}")))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max()
        .ok_or_else(|| invalid_context("candidate parent set is empty"))?;
    let timestamp = parents
        .iter()
        .filter_map(|parent| state.dag.blocks.get(parent))
        .map(|block| block.header.timestamp)
        .max()
        .unwrap_or(1)
        .max(1);

    Ok(Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents,
            timestamp,
            difficulty: 1,
            nonce: 0,
            merkle_root: "00".repeat(32),
            state_root: "00".repeat(32),
            blue_score: 0,
            height,
        },
        transactions: Vec::new(),
    })
}

fn score_tip(hash: &Hash, state: &ChainState) -> Result<SelectionScoreV1, PulseError> {
    let block = state
        .dag
        .blocks
        .get(hash)
        .ok_or_else(|| selection_error(SelectionV2Error::MissingTipBlock { hash: hash.clone() }))?;
    let blue_work = state.dag.blue_work.get(hash).copied().ok_or_else(|| {
        selection_error(SelectionV2Error::MissingTipBlueWork { hash: hash.clone() })
    })?;
    Ok(SelectionScoreV1 {
        blue_work,
        blue_score: block.header.blue_score,
        height: block.header.height,
        hash: hash.clone(),
    })
}

fn verify_activated_identity(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    if resolve_pow_validation_path(identity, state)? != PowValidationPath::ActivatedV2 {
        return Err(invalid_context(
            "requires the activated_v2 protocol identity",
        ));
    }
    if identity.dag_ordering_version != GHOSTDAG_V1_ORDERING_VERSION {
        return Err(invalid_context(format!(
            "DAG ordering version {} does not match {}",
            identity.dag_ordering_version, GHOSTDAG_V1_ORDERING_VERSION
        )));
    }
    Ok(())
}

fn final_context(
    selected_tip: Hash,
    parents: Vec<Hash>,
    excluded_parallel_parents: Vec<MiningParentExclusionV1>,
    classification: GhostdagV1Classification,
) -> Result<ActivatedV2MiningParentContext, PulseError> {
    if classification.selected_parent.as_ref() != Some(&selected_tip) {
        return Err(invalid_context(format!(
            "derived selected parent {:?} does not match selected tip {}",
            classification.selected_parent, selected_tip
        )));
    }
    let included_parallel_parents = parents
        .iter()
        .filter(|hash| *hash != &selected_tip)
        .cloned()
        .collect();
    Ok(ActivatedV2MiningParentContext {
        selected_tip: selected_tip.clone(),
        parents,
        included_parallel_parents,
        excluded_parallel_parents,
        selected_parent: selected_tip,
        blue_score: classification.blue_score,
        blue_work: classification.blue_work,
        merge_set: classification.merge_set,
        blues: classification.blues,
        reds: classification.reds,
        classification_digest: classification.classification_digest,
    })
}

/// Derive the deterministic parent context for an activated v2 mining template.
///
/// This helper is deliberately non-live: it does not construct a block, mutate
/// chain state, change daemon configuration, or activate GhostdagV1. Callers
/// must supply the exact persisted/negotiated protocol activation identity.
///
/// The selected tip is mandatory. Additional current tips are considered in
/// deterministic selection-score order and included only while the resulting
/// GHOSTDAG-v1 classification remains within the frozen parent/merge-set visit
/// bounds. Missing block/work metadata fails closed instead of silently dropping
/// a tip. Limit-triggering parallel tips are recorded explicitly as exclusions.
pub fn derive_activated_v2_mining_parent_context(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ActivatedV2MiningParentContext, PulseError> {
    verify_activated_identity(state, identity)?;

    let selected_tip = calculate_selected_tip_v1(state)
        .map_err(selection_error)?
        .ok_or_else(|| invalid_context("no selected tip is available"))?;

    let mut parallel_scores = state
        .dag
        .tips
        .iter()
        .filter(|hash| *hash != &selected_tip)
        .map(|hash| score_tip(hash, state))
        .collect::<Result<Vec<_>, _>>()?;
    parallel_scores.sort_by(|left, right| compare_selection_scores_v1(right, left));

    let mut parents = vec![selected_tip.clone()];
    let mut excluded_parallel_parents = Vec::new();

    for score in parallel_scores {
        if parents.len() >= GHOSTDAG_V1_MAX_PARENTS {
            excluded_parallel_parents.push(MiningParentExclusionV1 {
                hash: score.hash,
                reason: MiningParentExclusionReasonV1::ParentCountLimit {
                    max: GHOSTDAG_V1_MAX_PARENTS,
                },
            });
            continue;
        }

        let candidate = candidate_for_parents(
            state,
            parents
                .iter()
                .cloned()
                .chain(std::iter::once(score.hash.clone()))
                .collect(),
        )?;
        match classify_merge_set_v1(&candidate, state) {
            Ok(_) => parents = candidate.header.parents,
            Err(error) => {
                if let Some(reason) = exclusion_from_limit(error.clone()) {
                    excluded_parallel_parents.push(MiningParentExclusionV1 {
                        hash: score.hash,
                        reason,
                    });
                } else {
                    return Err(ghostdag_error(error));
                }
            }
        }
    }

    let candidate = candidate_for_parents(state, parents)?;
    let parents = candidate.header.parents.clone();
    let classification = classify_merge_set_v1(&candidate, state).map_err(ghostdag_error)?;
    final_context(
        selected_tip,
        parents,
        excluded_parallel_parents,
        classification,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{genesis::init_chain_state, types::Transaction};

    fn hash(value: u64) -> String {
        format!("{value:064x}")
    }

    fn tip_block(hash: String, genesis: &str, score: u64) -> Block {
        Block {
            hash,
            header: BlockHeader {
                version: BLOCK_HEADER_VERSION_V2,
                parents: vec![genesis.to_string()],
                timestamp: 1_900_000_000 + score,
                difficulty: 1,
                nonce: 0,
                merkle_root: "11".repeat(32),
                state_root: "22".repeat(32),
                blue_score: score,
                height: 1,
            },
            transactions: Vec::<Transaction>::new(),
        }
    }

    fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn parallel_state(order: &[u64]) -> ChainState {
        let mut state = init_chain_state("task28-mining-parent-context".to_string());
        let genesis = state.dag.genesis_hash.clone();
        state.dag.tips.clear();
        for value in order {
            let block_hash = hash(*value);
            let score = *value;
            let block = tip_block(block_hash.clone(), &genesis, score);
            state.dag.blocks.insert(block_hash.clone(), block);
            state
                .dag
                .blue_work
                .insert(block_hash.clone(), u128::from(score));
            state.dag.tips.insert(block_hash);
        }
        state
    }

    #[test]
    fn selected_tip_and_parallel_parents_are_deterministic() {
        let forward = parallel_state(&[1, 2, 3, 4]);
        let reverse = parallel_state(&[4, 3, 2, 1]);
        let forward_context =
            derive_activated_v2_mining_parent_context(&forward, &activated_identity(&forward))
                .unwrap();
        let reverse_context =
            derive_activated_v2_mining_parent_context(&reverse, &activated_identity(&reverse))
                .unwrap();

        assert_eq!(forward_context, reverse_context);
        assert_eq!(forward_context.selected_tip, hash(4));
        assert_eq!(forward_context.selected_parent, hash(4));
        assert_eq!(forward_context.parents.len(), 4);
        assert_eq!(forward_context.included_parallel_parents.len(), 3);
        assert_eq!(forward_context.merge_set.len(), 3);
        assert_eq!(forward_context.blues.len(), 2);
        assert_eq!(forward_context.reds.len(), 1);
        assert!(forward_context.excluded_parallel_parents.is_empty());
    }

    #[test]
    fn legacy_or_wrong_ordering_identity_cannot_derive_v2_mining_context() {
        let state = parallel_state(&[1, 2]);
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        assert!(derive_activated_v2_mining_parent_context(&state, &legacy).is_err());

        let mut wrong_ordering = activated_identity(&state);
        wrong_ordering.dag_ordering_version.push_str("-wrong");
        assert!(derive_activated_v2_mining_parent_context(&state, &wrong_ordering).is_err());
    }

    #[test]
    fn missing_tip_work_fails_closed() {
        let mut state = parallel_state(&[1, 2]);
        state.dag.blue_work.remove(&hash(1));
        assert!(derive_activated_v2_mining_parent_context(&state, &activated_identity(&state))
            .unwrap_err()
            .to_string()
            .contains("MissingTipBlueWork"));
    }

    #[test]
    fn parent_count_bound_records_excluded_parallel_tips() {
        let values = (1..=67).collect::<Vec<_>>();
        let state = parallel_state(&values);
        let context =
            derive_activated_v2_mining_parent_context(&state, &activated_identity(&state)).unwrap();

        assert_eq!(context.parents.len(), GHOSTDAG_V1_MAX_PARENTS);
        assert_eq!(context.included_parallel_parents.len(), 63);
        assert_eq!(context.excluded_parallel_parents.len(), 3);
        assert!(context.excluded_parallel_parents.iter().all(|excluded| {
            matches!(
                excluded.reason,
                MiningParentExclusionReasonV1::ParentCountLimit {
                    max: GHOSTDAG_V1_MAX_PARENTS
                }
            )
        }));
    }
}
