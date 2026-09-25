use crate::{
    bind_reward_finality_boundary_v3, derive_finality_boundary_v1, derive_ordered_dag_v2,
    derive_reward_settlement_snapshot_v3, materializable_reward_utxos_v3, ChainState,
    MonetaryCadenceSegment, PulseError, RewardFinalityBoundaryV3, RewardSettlementSnapshotV3,
    GHOSTDAG_V1_FINALITY_POLICY_VERSION,
};

fn invalid_live_settlement(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "monetary-v3 live reward settlement: {}",
        message.into()
    ))
}

/// Bind the currently implemented live finality engine to the monetary ordered
/// score.
///
/// A persisted monetary sidecar may name a reward-finality policy, but that name
/// is not itself an activation mechanism. v3.0.0 can only use a policy for which
/// the daemon has an implemented finality engine. At present that engine is the
/// conservative GhostDAG-v1 boundary, whose boundary is genesis and whose
/// pruning is disabled.
pub fn derive_live_reward_finality_boundary_v3(
    state: &ChainState,
    expected_finality_policy_version: &str,
) -> Result<RewardFinalityBoundaryV3, PulseError> {
    if expected_finality_policy_version != GHOSTDAG_V1_FINALITY_POLICY_VERSION {
        return Err(invalid_live_settlement(format!(
            "unsupported live reward-finality policy {}; implemented engine is {}",
            expected_finality_policy_version, GHOSTDAG_V1_FINALITY_POLICY_VERSION
        )));
    }

    let finality = derive_finality_boundary_v1(state).map_err(|error| {
        invalid_live_settlement(format!("finality boundary derivation failed: {error:?}"))
    })?;
    if finality.policy_version != expected_finality_policy_version {
        return Err(invalid_live_settlement(format!(
            "derived finality policy {} does not match persisted {}",
            finality.policy_version, expected_finality_policy_version
        )));
    }

    let ordered = derive_ordered_dag_v2(state).map_err(|error| {
        invalid_live_settlement(format!("ordered DAG derivation failed: {error:?}"))
    })?;
    let position = ordered
        .blocks
        .iter()
        .position(|hash| hash == &finality.boundary_hash)
        .ok_or_else(|| {
            invalid_live_settlement(format!(
                "finality boundary block {} is absent from ordered DAG",
                finality.boundary_hash
            ))
        })?;
    let finalized_through_score = u64::try_from(position)
        .map_err(|_| invalid_live_settlement("finality score exceeds u64"))?;

    bind_reward_finality_boundary_v3(
        state,
        finalized_through_score,
        expected_finality_policy_version,
    )
    .map_err(|error| invalid_live_settlement(format!("monetary finality binding failed: {error}")))
}

/// Validate reward finality + economic maturity under the currently implemented
/// live replay semantics.
///
/// The current finality engine protects only genesis, so no non-genesis reward
/// is materializable. If a future finality engine starts authorizing spendable
/// rewards before replay/state-root materialization is explicitly integrated,
/// this function fails closed rather than silently making the monetary state
/// diverge from the committed UTXO root.
pub fn validate_live_reward_settlement_v3(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
    expected_finality_policy_version: &str,
) -> Result<RewardSettlementSnapshotV3, PulseError> {
    let boundary =
        derive_live_reward_finality_boundary_v3(state, expected_finality_policy_version)?;
    let snapshot = derive_reward_settlement_snapshot_v3(
        state,
        cadence_segments,
        expected_finality_policy_version,
        Some(&boundary),
    )
    .map_err(|error| {
        invalid_live_settlement(format!("reward settlement derivation failed: {error}"))
    })?;

    let materializable = materializable_reward_utxos_v3(&snapshot);
    if !materializable.is_empty() {
        return Err(invalid_live_settlement(format!(
            "{} reward UTXOs became spendable but live settlement UTXO materialization is not activated",
            materializable.len()
        )));
    }

    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis_v3::init_chain_state_v3;

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    #[test]
    fn current_live_finality_binds_genesis_and_materializes_nothing() {
        let state = init_chain_state_v3("monetary-v3-live-finality".into(), 1_800_000_000).unwrap();

        let boundary =
            derive_live_reward_finality_boundary_v3(&state, GHOSTDAG_V1_FINALITY_POLICY_VERSION)
                .unwrap();
        assert_eq!(boundary.finalized_through_score, 0);
        assert_eq!(boundary.finalized_block_hash, state.dag.genesis_hash);

        let snapshot = validate_live_reward_settlement_v3(
            &state,
            &ONE_SECOND,
            GHOSTDAG_V1_FINALITY_POLICY_VERSION,
        )
        .unwrap();
        assert!(snapshot.claims.is_empty());
        assert!(materializable_reward_utxos_v3(&snapshot).is_empty());
    }

    #[test]
    fn unknown_live_finality_policy_fails_closed() {
        let state =
            init_chain_state_v3("monetary-v3-live-finality-unknown".into(), 1_800_000_001).unwrap();

        assert!(validate_live_reward_settlement_v3(
            &state,
            &ONE_SECOND,
            "future-finality-not-implemented",
        )
        .unwrap_err()
        .to_string()
        .contains("unsupported live reward-finality policy"));
    }
}
