use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    monetary_v3::{economic_time_ns_for_score, MonetaryCadenceSegment, MonetaryV3Error},
    ordering_v2::{derive_ordered_dag_v2, OrderingV2Error},
    reward_settlement_v3::{
        bind_reward_finality_boundary_v3, validate_reward_finality_boundary_v3,
        RewardFinalityBoundaryV3, RewardSettlementV3Error,
    },
    state::ChainState,
    types::Hash,
};

pub const FINALITY_V3_POLICY_SCHEMA_VERSION: u32 = 1;
const FINALITY_POLICY_DOMAIN_V3: &[u8] = b"PulseDAG:finality-policy:v3";
const NS_PER_SECOND: u128 = 1_000_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityPolicyV3 {
    pub schema_version: u32,
    pub name: String,
    /// Consensus finality delay expressed in economic time, not raw blocks.
    /// The production value remains a network-parameter freeze input.
    pub finality_delay_economic_seconds: u64,
}

impl FinalityPolicyV3 {
    pub fn new(name: impl Into<String>, finality_delay_economic_seconds: u64) -> Self {
        Self {
            schema_version: FINALITY_V3_POLICY_SCHEMA_VERSION,
            name: name.into(),
            finality_delay_economic_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalityDecisionV3 {
    pub policy: FinalityPolicyV3,
    pub policy_digest: String,
    /// The exact policy identity passed into the reward-settlement boundary.
    /// Encoding the digest here prevents the same human version label from being
    /// reused with different consensus parameters.
    pub policy_identity: String,
    pub current_monetary_score: u64,
    pub boundary: RewardFinalityBoundaryV3,
    pub advanced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FinalityV3Error {
    #[error("unsupported v3 finality policy schema {observed}, expected {expected}")]
    UnsupportedPolicySchema { observed: u32, expected: u32 },
    #[error("v3 finality policy name must not be empty")]
    EmptyPolicyName,
    #[error("v3 finality delay must be greater than zero economic seconds")]
    ZeroFinalityDelay,
    #[error("finality arithmetic overflow")]
    ArithmeticOverflow,
    #[error("ordered DAG derivation failed: {0}")]
    Ordering(String),
    #[error("selected chain must start at genesis {expected}, observed {observed:?}")]
    SelectedChainDoesNotStartAtGenesis {
        expected: Hash,
        observed: Option<Hash>,
    },
    #[error("selected-chain block {hash} is missing from authoritative ordered DAG")]
    SelectedChainBlockMissingFromOrder { hash: Hash },
    #[error("previous finality boundary uses policy identity {observed}, expected {expected}")]
    PreviousPolicyMismatch { expected: String, observed: String },
    #[error("previous finality boundary conflicts with current authoritative DAG: {detail}")]
    PreviousFinalityConflict { detail: String },
    #[error(transparent)]
    Monetary(#[from] MonetaryV3Error),
    #[error(transparent)]
    Settlement(#[from] RewardSettlementV3Error),
}

impl From<OrderingV2Error> for FinalityV3Error {
    fn from(error: OrderingV2Error) -> Self {
        Self::Ordering(format!("{error:?}"))
    }
}

fn validate_policy(policy: &FinalityPolicyV3) -> Result<(), FinalityV3Error> {
    if policy.schema_version != FINALITY_V3_POLICY_SCHEMA_VERSION {
        return Err(FinalityV3Error::UnsupportedPolicySchema {
            observed: policy.schema_version,
            expected: FINALITY_V3_POLICY_SCHEMA_VERSION,
        });
    }
    if policy.name.trim().is_empty() {
        return Err(FinalityV3Error::EmptyPolicyName);
    }
    if policy.finality_delay_economic_seconds == 0 {
        return Err(FinalityV3Error::ZeroFinalityDelay);
    }
    Ok(())
}

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

/// Deterministic digest of every consensus-visible finality-policy parameter.
pub fn finality_policy_digest_v3(policy: &FinalityPolicyV3) -> Result<String, FinalityV3Error> {
    validate_policy(policy)?;
    let mut bytes = Vec::with_capacity(128);
    encode_len_prefixed_bytes(&mut bytes, FINALITY_POLICY_DOMAIN_V3);
    bytes.extend_from_slice(&policy.schema_version.to_le_bytes());
    encode_len_prefixed_str(&mut bytes, &policy.name);
    bytes.extend_from_slice(&policy.finality_delay_economic_seconds.to_le_bytes());
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn finality_policy_identity_v3(policy: &FinalityPolicyV3) -> Result<String, FinalityV3Error> {
    let digest = finality_policy_digest_v3(policy)?;
    Ok(format!("{}@sha256:{digest}", policy.name))
}

fn selected_chain_positions(
    state: &ChainState,
    ordered_blocks: &[Hash],
) -> Result<Vec<(Hash, u64)>, FinalityV3Error> {
    if state.dag.selected_chain.first() != Some(&state.dag.genesis_hash) {
        return Err(FinalityV3Error::SelectedChainDoesNotStartAtGenesis {
            expected: state.dag.genesis_hash.clone(),
            observed: state.dag.selected_chain.first().cloned(),
        });
    }

    let positions = ordered_blocks
        .iter()
        .enumerate()
        .map(|(index, hash)| {
            u64::try_from(index)
                .map(|score| (hash.clone(), score))
                .map_err(|_| FinalityV3Error::ArithmeticOverflow)
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    state
        .dag
        .selected_chain
        .iter()
        .map(|hash| {
            positions
                .get(hash)
                .copied()
                .map(|score| (hash.clone(), score))
                .ok_or_else(|| FinalityV3Error::SelectedChainBlockMissingFromOrder {
                    hash: hash.clone(),
                })
        })
        .collect()
}

fn candidate_finalized_score_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    policy: &FinalityPolicyV3,
) -> Result<(u64, u64), FinalityV3Error> {
    validate_policy(policy)?;
    let ordered = derive_ordered_dag_v2(state)?;
    if ordered.blocks.first() != Some(&state.dag.genesis_hash) {
        return Err(FinalityV3Error::SelectedChainDoesNotStartAtGenesis {
            expected: state.dag.genesis_hash.clone(),
            observed: ordered.blocks.first().cloned(),
        });
    }

    let current_score = u64::try_from(ordered.blocks.len().saturating_sub(1))
        .map_err(|_| FinalityV3Error::ArithmeticOverflow)?;
    let current_time = economic_time_ns_for_score(current_score, cadence_segments)?;
    let delay_ns = u128::from(policy.finality_delay_economic_seconds)
        .checked_mul(NS_PER_SECOND)
        .ok_or(FinalityV3Error::ArithmeticOverflow)?;

    // Genesis is the fail-closed floor. A non-genesis finality point is selected
    // only from the selected-chain anchors whose economic age reaches the frozen
    // policy delay. This finalizes the deterministic ordered prefix ending at
    // that anchor without using local arrival order or raw block count.
    let mut candidate_score = 0_u64;
    for (_, score) in selected_chain_positions(state, &ordered.blocks)? {
        if score == 0 {
            continue;
        }
        let score_time = economic_time_ns_for_score(score, cadence_segments)?;
        if current_time.saturating_sub(score_time) >= delay_ns {
            candidate_score = candidate_score.max(score);
        }
    }

    Ok((candidate_score, current_score))
}

/// Derive the deterministic v3 finality boundary for the current state.
///
/// This function intentionally does not contain a mainnet duration constant.
/// The caller must supply the frozen network policy. A prior boundary, when
/// present, is first revalidated against the current ordered DAG. Any conflict
/// fails closed rather than silently choosing a competing history.
pub fn derive_finality_decision_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    policy: &FinalityPolicyV3,
    previous_boundary: Option<&RewardFinalityBoundaryV3>,
) -> Result<FinalityDecisionV3, FinalityV3Error> {
    let policy_digest = finality_policy_digest_v3(policy)?;
    let policy_identity = finality_policy_identity_v3(policy)?;
    let (mut candidate_score, current_monetary_score) =
        candidate_finalized_score_v3(state, cadence_segments, policy)?;

    if let Some(previous) = previous_boundary {
        if previous.policy_version != policy_identity {
            return Err(FinalityV3Error::PreviousPolicyMismatch {
                expected: policy_identity,
                observed: previous.policy_version.clone(),
            });
        }
        validate_reward_finality_boundary_v3(state, previous).map_err(|error| {
            FinalityV3Error::PreviousFinalityConflict {
                detail: error.to_string(),
            }
        })?;
        candidate_score = candidate_score.max(previous.finalized_through_score);
    }

    let boundary = bind_reward_finality_boundary_v3(state, candidate_score, &policy_identity)?;
    let advanced = previous_boundary
        .map(|previous| boundary.finalized_through_score > previous.finalized_through_score)
        .unwrap_or(boundary.finalized_through_score > 0);

    Ok(FinalityDecisionV3 {
        policy: policy.clone(),
        policy_digest,
        policy_identity,
        current_monetary_score,
        boundary,
        advanced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        types::{Block, BlockHeader},
    };

    const BPS1: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];
    const BPS2: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 500_000_000,
    }];

    fn block(hash: &str, parent: &str, height: u64, blue_score: u64) -> Block {
        Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 2,
                parents: vec![parent.to_string()],
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

    fn linear_state(chain_id: &str, non_genesis_blocks: u64) -> ChainState {
        let mut state = init_chain_state(chain_id.to_string());
        let genesis = state.dag.genesis_hash.clone();
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis.clone(), vec![]);

        let mut parent = genesis.clone();
        for height in 1..=non_genesis_blocks {
            let hash = format!("b{height:03}");
            let candidate = block(&hash, &parent, height, height);
            state.dag.blocks.insert(hash.clone(), candidate);
            state
                .dag
                .selected_parents
                .insert(hash.clone(), Some(parent.clone()));
            state.dag.blue_work.insert(hash.clone(), u128::from(height) * 10);
            state.dag.merge_set_blues.insert(hash.clone(), vec![]);
            state.dag.merge_set_reds.insert(hash.clone(), vec![]);
            state.dag.selected_chain.push(hash.clone());
            parent = hash;
        }
        state
    }

    fn diamond_state(chain_id: &str, selected_a: bool) -> ChainState {
        let mut state = init_chain_state(chain_id.to_string());
        let genesis = state.dag.genesis_hash.clone();
        let mut a = block("a", &genesis, 1, 10);
        let mut b = block("b", &genesis, 1, 9);
        let c = Block {
            hash: "c".into(),
            header: BlockHeader {
                version: 2,
                parents: vec!["a".into(), "b".into()],
                timestamp: 12,
                difficulty: 1,
                nonce: 0,
                merkle_root: "m-c".into(),
                state_root: "s-c".into(),
                blue_score: 20,
                height: 2,
            },
            transactions: vec![],
        };
        a.hash = "a".into();
        b.hash = "b".into();
        for candidate in [a, b, c] {
            state.dag.blue_work.insert(
                candidate.hash.clone(),
                u128::from(candidate.header.blue_score) * 10,
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
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis.clone(), vec![]);

        if selected_a {
            state
                .dag
                .selected_parents
                .insert("c".into(), Some("a".into()));
            state.dag.selected_chain = vec![genesis, "a".into(), "c".into()];
            state.dag.merge_set_blues.insert("a".into(), vec![]);
            state.dag.merge_set_reds.insert("a".into(), vec![]);
            state.dag.merge_set_blues.insert("c".into(), vec!["b".into()]);
            state.dag.merge_set_reds.insert("c".into(), vec![]);
        } else {
            state
                .dag
                .selected_parents
                .insert("c".into(), Some("b".into()));
            state.dag.selected_chain = vec![genesis, "b".into(), "c".into()];
            state.dag.merge_set_blues.insert("b".into(), vec![]);
            state.dag.merge_set_reds.insert("b".into(), vec![]);
            state.dag.merge_set_blues.insert("c".into(), vec!["a".into()]);
            state.dag.merge_set_reds.insert("c".into(), vec![]);
        }
        state
    }

    #[test]
    fn policy_digest_binds_the_finality_delay() {
        let a = FinalityPolicyV3::new("candidate-finality-v1", 3_600);
        let b = FinalityPolicyV3::new("candidate-finality-v1", 7_200);
        assert_ne!(
            finality_policy_digest_v3(&a).unwrap(),
            finality_policy_digest_v3(&b).unwrap()
        );
        assert_ne!(
            finality_policy_identity_v3(&a).unwrap(),
            finality_policy_identity_v3(&b).unwrap()
        );
    }

    #[test]
    fn finality_delay_tracks_economic_time_across_bps() {
        let policy = FinalityPolicyV3::new("candidate-finality-v1", 2);
        let one_bps = linear_state("finality-1bps", 4);
        let two_bps = linear_state("finality-2bps", 8);

        let one = derive_finality_decision_v3(&one_bps, &BPS1, &policy, None).unwrap();
        let two = derive_finality_decision_v3(&two_bps, &BPS2, &policy, None).unwrap();
        assert_eq!(one.boundary.finalized_through_score, 2);
        assert_eq!(two.boundary.finalized_through_score, 4);
        assert_eq!(
            economic_time_ns_for_score(one.boundary.finalized_through_score, &BPS1).unwrap(),
            economic_time_ns_for_score(two.boundary.finalized_through_score, &BPS2).unwrap()
        );
    }

    #[test]
    fn before_delay_only_genesis_is_final() {
        let policy = FinalityPolicyV3::new("candidate-finality-v1", 3_600);
        let state = linear_state("finality-young", 10);
        let decision = derive_finality_decision_v3(&state, &BPS1, &policy, None).unwrap();
        assert_eq!(decision.boundary.finalized_through_score, 0);
        assert!(!decision.advanced);
    }

    #[test]
    fn stale_previous_boundary_fails_closed_after_reorder() {
        let policy = FinalityPolicyV3::new("candidate-finality-v1", 1);
        let first = diamond_state("finality-conflict", true);
        let reordered = diamond_state("finality-conflict", false);
        let first_decision = derive_finality_decision_v3(&first, &BPS1, &policy, None).unwrap();
        assert_eq!(first_decision.boundary.finalized_block_hash, "a");

        assert!(matches!(
            derive_finality_decision_v3(
                &reordered,
                &BPS1,
                &policy,
                Some(&first_decision.boundary)
            ),
            Err(FinalityV3Error::PreviousFinalityConflict { .. })
        ));
    }

    #[test]
    fn previous_boundary_never_regresses() {
        let policy = FinalityPolicyV3::new("candidate-finality-v1", 2);
        let earlier = linear_state("finality-monotonic", 6);
        let first = derive_finality_decision_v3(&earlier, &BPS1, &policy, None).unwrap();
        assert_eq!(first.boundary.finalized_through_score, 4);

        let later = linear_state("finality-monotonic", 8);
        let second = derive_finality_decision_v3(
            &later,
            &BPS1,
            &policy,
            Some(&first.boundary),
        )
        .unwrap();
        assert_eq!(second.boundary.finalized_through_score, 6);
        assert!(second.advanced);
    }
}
