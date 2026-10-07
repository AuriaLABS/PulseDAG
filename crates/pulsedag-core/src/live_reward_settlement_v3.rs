use crate::{
    bind_reward_finality_boundary_v3, derive_ordered_dag_v2,
    derive_reward_settlement_snapshot_v3, mature_reward_prefix_score_v3,
    verify_authoritative_state_snapshot_v3, ChainState, MonetaryCadenceSegment, PulseError,
    RewardFinalityBoundaryV3, RewardSettlementSnapshotV3, REWARD_FINALITY_POLICY_VERSION_V3,
};

fn invalid_live_settlement(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "monetary-v3 live reward settlement: {}",
        message.into()
    ))
}

/// Bind the frozen v3.0.0 monetary reward-finality policy to the exact mature
/// ordered-DAG prefix.
///
/// This policy is intentionally distinct from the general P2P/finality metadata
/// version. Monetary settlement uses the canonical ordered score plus the frozen
/// 3,600-economic-second maturity rule. A reorganization recomputes the prefix
/// from the new authoritative order.
pub fn derive_live_reward_finality_boundary_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    expected_finality_policy_version: &str,
) -> Result<RewardFinalityBoundaryV3, PulseError> {
    if expected_finality_policy_version != REWARD_FINALITY_POLICY_VERSION_V3 {
        return Err(invalid_live_settlement(format!(
            "unsupported live reward-finality policy {}; implemented monetary engine is {}",
            expected_finality_policy_version, REWARD_FINALITY_POLICY_VERSION_V3
        )));
    }

    let ordered = derive_ordered_dag_v2(state).map_err(|error| {
        invalid_live_settlement(format!("ordered DAG derivation failed: {error:?}"))
    })?;
    let current_score = u64::try_from(ordered.blocks.len().saturating_sub(1))
        .map_err(|_| invalid_live_settlement("monetary score exceeds u64"))?;
    let finalized_through_score =
        mature_reward_prefix_score_v3(current_score, cadence_segments)?;

    bind_reward_finality_boundary_v3(
        state,
        finalized_through_score,
        expected_finality_policy_version,
    )
    .map_err(|error| invalid_live_settlement(format!("monetary finality binding failed: {error}")))
}

/// Validate the live reward-finality snapshot against the exact authoritative
/// v3 replay.
///
/// A policy string alone is never authority. The persisted/live ChainState must
/// already contain the same UTXO root, ordered DAG, tip and conflict decisions
/// produced by v3 replay. This makes mature reward materialization part of the
/// consensus state rather than a reporting-only calculation.
pub fn validate_live_reward_settlement_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    expected_finality_policy_version: &str,
) -> Result<RewardSettlementSnapshotV3, PulseError> {
    if expected_finality_policy_version != REWARD_FINALITY_POLICY_VERSION_V3 {
        return Err(invalid_live_settlement(format!(
            "unsupported live reward-finality policy {}; implemented monetary engine is {}",
            expected_finality_policy_version, REWARD_FINALITY_POLICY_VERSION_V3
        )));
    }

    verify_authoritative_state_snapshot_v3(state, cadence_segments).map_err(|error| {
        invalid_live_settlement(format!(
            "authoritative v3 reward replay does not match live state: {error}"
        ))
    })?;

    let boundary = derive_live_reward_finality_boundary_v3(
        state,
        cadence_segments,
        expected_finality_policy_version,
    )?;
    derive_reward_settlement_snapshot_v3(
        state,
        cadence_segments,
        expected_finality_policy_version,
        Some(&boundary),
    )
    .map_err(|error| {
        invalid_live_settlement(format!("reward settlement derivation failed: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis_v3::init_chain_state_v3, materialize_authoritative_state_v3,
        reward_settlement_v3::build_reward_claim_transaction_v3,
        types::{Block, BlockHeader},
    };

    const ONE_HOUR_PER_SCORE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 3_600_000_000_000,
    }];

    fn append_linear_reward(state: &mut ChainState, hash: &str, score: u64) {
        let parent = state
            .dag
            .selected_chain
            .last()
            .cloned()
            .unwrap_or_else(|| state.dag.genesis_hash.clone());
        let claim = build_reward_claim_transaction_v3(
            &format!("pulse1livefinality{score}"),
            score,
            &state.chain_id,
        )
        .unwrap();
        let block = Block {
            hash: hash.to_string(),
            header: BlockHeader {
                version: 2,
                parents: vec![parent.clone()],
                timestamp: 1_800_000_000 + score,
                difficulty: 1,
                nonce: score,
                merkle_root: format!("merkle-{score}"),
                state_root: format!("placeholder-{score}"),
                blue_score: score,
                height: score,
            },
            transactions: vec![claim],
        };
        state.dag.blocks.insert(hash.to_string(), block);
        state.dag.blue_work.insert(hash.to_string(), u128::from(score));
        state
            .dag
            .selected_parents
            .insert(hash.to_string(), Some(parent));
        state.dag.merge_set_blues.insert(hash.to_string(), vec![]);
        state.dag.merge_set_reds.insert(hash.to_string(), vec![]);
        state.dag.selected_chain.push(hash.to_string());
        state.dag.best_height = score;
    }

    #[test]
    fn mature_prefix_policy_binds_exact_economic_boundary() {
        let mut state =
            init_chain_state_v3("monetary-v3-live-finality".into(), 1_800_000_000).unwrap();
        append_linear_reward(&mut state, "b1", 1);
        append_linear_reward(&mut state, "b2", 2);
        let state = materialize_authoritative_state_v3(&state, &ONE_HOUR_PER_SCORE).unwrap();

        let boundary = derive_live_reward_finality_boundary_v3(
            &state,
            &ONE_HOUR_PER_SCORE,
            REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap();
        assert_eq!(boundary.finalized_through_score, 1);

        let snapshot = validate_live_reward_settlement_v3(
            &state,
            &ONE_HOUR_PER_SCORE,
            REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap();
        assert_eq!(snapshot.finality_boundary.unwrap(), boundary);
        assert!(snapshot.claims[0].finality_protected);
        assert!(snapshot.claims[0].economic_maturity_reached);
        assert!(!snapshot.claims[1].finality_protected);
    }

    #[test]
    fn unmaterialized_mature_reward_fails_closed() {
        let mut state =
            init_chain_state_v3("monetary-v3-live-finality-stale".into(), 1_800_000_010).unwrap();
        append_linear_reward(&mut state, "b1", 1);
        append_linear_reward(&mut state, "b2", 2);

        let error = validate_live_reward_settlement_v3(
            &state,
            &ONE_HOUR_PER_SCORE,
            REWARD_FINALITY_POLICY_VERSION_V3,
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not match live state"));
    }

    #[test]
    fn general_p2p_finality_policy_is_not_monetary_reward_finality() {
        let state =
            init_chain_state_v3("monetary-v3-live-finality-wrong-policy".into(), 1_800_000_020)
                .unwrap();

        assert!(validate_live_reward_settlement_v3(
            &state,
            &ONE_HOUR_PER_SCORE,
            crate::GHOSTDAG_V1_FINALITY_POLICY_VERSION,
        )
        .unwrap_err()
        .to_string()
        .contains("unsupported live reward-finality policy"));
    }
}
