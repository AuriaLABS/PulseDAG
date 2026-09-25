use crate::{
    audit_monetary_state_v3,
    errors::PulseError,
    monetary_v3::MonetaryCadenceSegment,
    network_block_v3::validate_monetary_v3_p2p_staging_envelope,
    network_runtime_v2::{
        drive_activated_v2_p2p_block_with_runtime_persistence, ActivatedV2P2pDriveResult,
        ActivatedV2P2pRuntime, ActivatedV2P2pRuntimePersistence,
    },
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    types::Block,
    validate_live_reward_settlement_v3, validate_ordered_monetary_reward_v3,
    GHOSTDAG_V1_FINALITY_POLICY_VERSION,
};

fn invalid_monetary_runtime(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("monetary-v3 p2p runtime: {}", message.into()))
}

fn validate_runtime_transient_monetary_envelopes(
    state: &ChainState,
    runtime: &ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    for hash in runtime.staging().hashes() {
        let block = runtime.staging().get(&hash).ok_or_else(|| {
            invalid_monetary_runtime(format!("staged block {hash} disappeared during audit"))
        })?;
        validate_monetary_v3_p2p_staging_envelope(block, state, identity)?;
    }
    for block in runtime.pending_blocks() {
        validate_monetary_v3_p2p_staging_envelope(block, state, identity)?;
    }
    Ok(())
}

fn audit_authoritative_monetary_state(
    state: &ChainState,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<(), PulseError> {
    audit_monetary_state_v3(state, cadence_segments)
        .map(|_| ())
        .map_err(|error| {
            invalid_monetary_runtime(format!("authoritative monetary audit failed: {error}"))
        })?;
    validate_live_reward_settlement_v3(
        state,
        cadence_segments,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION,
    )?;
    Ok(())
}

/// Verify a restored/live activated-v2 runtime snapshot before it is allowed to
/// operate under a persisted v3 monetary activation.
pub fn validate_monetary_v3_p2p_runtime_snapshot(
    state: &ChainState,
    runtime: &ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<(), PulseError> {
    if state.contracts.config.enabled {
        return Err(invalid_monetary_runtime(
            "v3.0.0 monetary runtime requires smart-contract execution to remain inactive",
        ));
    }
    if identity.chain_id != state.chain_id || identity.genesis_hash != state.dag.genesis_hash {
        return Err(invalid_monetary_runtime(
            "protocol identity does not match the restored chain state",
        ));
    }
    validate_runtime_transient_monetary_envelopes(state, runtime, identity)?;
    audit_authoritative_monetary_state(state, cadence_segments)
}

/// Drive the activated-v2 transient P2P runtime under the frozen v3 monetary
/// contract.
///
/// Every incoming, staged, and pending block must carry the amountless v3
/// reward-claim envelope. Every authoritative single-block or promotion-bundle
/// persistence callback receives a state that has passed the complete ordered
/// monetary audit. Existing invalid transient v2 economics therefore cannot be
/// carried forward once this wrapper is selected.
pub fn drive_monetary_v3_p2p_block_with_runtime_persistence<
    FPersistRuntime,
    FPersistOne,
    FPersistBundle,
    FBroadcast,
>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
    mut persist_runtime: FPersistRuntime,
    mut persist_one: FPersistOne,
    mut persist_bundle: FPersistBundle,
    broadcast: FBroadcast,
) -> Result<ActivatedV2P2pDriveResult, PulseError>
where
    FPersistRuntime: FnMut(&ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistOne: FnMut(&Block, &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    validate_monetary_v3_p2p_runtime_snapshot(state, runtime, identity, cadence_segments)?;
    validate_monetary_v3_p2p_staging_envelope(&block, state, identity)?;

    drive_activated_v2_p2p_block_with_runtime_persistence(
        block,
        state,
        runtime,
        identity,
        ActivatedV2P2pRuntimePersistence::new(
            |prepared_state, prepared_runtime| {
                validate_runtime_transient_monetary_envelopes(
                    prepared_state,
                    prepared_runtime,
                    identity,
                )?;
                audit_authoritative_monetary_state(prepared_state, cadence_segments)?;
                persist_runtime(prepared_state, prepared_runtime)
            },
            |accepted_block, prepared_state, prepared_runtime| {
                validate_runtime_transient_monetary_envelopes(
                    prepared_state,
                    prepared_runtime,
                    identity,
                )?;
                validate_ordered_monetary_reward_v3(
                    prepared_state,
                    &accepted_block.hash,
                    cadence_segments,
                )
                .map_err(|error| {
                    invalid_monetary_runtime(format!(
                        "accepted block reward validation failed: {error}"
                    ))
                })?;
                audit_authoritative_monetary_state(prepared_state, cadence_segments)?;
                persist_one(accepted_block, prepared_state, prepared_runtime)
            },
            |bundle, prepared_state, prepared_runtime| {
                validate_runtime_transient_monetary_envelopes(
                    prepared_state,
                    prepared_runtime,
                    identity,
                )?;
                for accepted_block in bundle {
                    validate_ordered_monetary_reward_v3(
                        prepared_state,
                        &accepted_block.hash,
                        cadence_segments,
                    )
                    .map_err(|error| {
                        invalid_monetary_runtime(format!(
                            "promoted block {} reward validation failed: {error}",
                            accepted_block.hash
                        ))
                    })?;
                }
                audit_authoritative_monetary_state(prepared_state, cadence_segments)?;
                persist_bundle(bundle, prepared_state, prepared_runtime)
            },
        ),
        broadcast,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_monetary_mining_template_v3, compute_block_hash_v2, current_ts,
        finalize_monetary_mining_template_v3, genesis_v3::init_chain_state_v3,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION, validate_pow_for_protocol,
    };

    const ONE_SECOND: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn monetary_block(state: &ChainState, identity: &ProtocolActivationIdentity) -> Block {
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);
        let template = build_monetary_mining_template_v3(
            state,
            identity,
            &ONE_SECOND,
            "pulse1runtimev3",
            1,
            timestamp,
            vec![],
        )
        .unwrap();
        let finalized =
            finalize_monetary_mining_template_v3(state, identity, &ONE_SECOND, &template).unwrap();
        let mut block = finalized.block;
        for nonce in 0..=200_000_u64 {
            block.header.nonce = nonce;
            block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
            if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
                return block;
            }
        }
        panic!("expected monetary-v3 runtime fixture to find a valid nonce");
    }

    #[test]
    fn runtime_accepts_monetary_block_and_audits_persisted_state() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state = init_chain_state_v3("monetary-v3-p2p-runtime".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let block = monetary_block(&state, &identity);
        let expected_hash = block.hash.clone();
        let mut runtime = ActivatedV2P2pRuntime::default();
        let mut persisted = false;

        let driven = drive_monetary_v3_p2p_block_with_runtime_persistence(
            block,
            &mut state,
            &mut runtime,
            &identity,
            &ONE_SECOND,
            |_, _| Ok(()),
            |accepted, prepared, _| {
                persisted = true;
                assert_eq!(accepted.hash, expected_hash);
                audit_monetary_state_v3(prepared, &ONE_SECOND).unwrap();
                Ok(())
            },
            |_, _, _| panic!("single finalizable block must not persist a bundle"),
            |_| Ok(()),
        )
        .unwrap();

        assert!(persisted);
        assert!(state.dag.blocks.contains_key(&expected_hash));
        assert!(matches!(
            driven.primary,
            crate::ActivatedV2P2pRuntimeOutcome::Accepted { .. }
        ));
    }

    #[test]
    fn monetary_runtime_rejects_contract_activation_before_transition() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state =
            init_chain_state_v3("monetary-v3-p2p-runtime-contracts".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let block = monetary_block(&state, &identity);
        let mut runtime = ActivatedV2P2pRuntime::default();
        state.contracts.config.enabled = true;

        let error = drive_monetary_v3_p2p_block_with_runtime_persistence(
            block,
            &mut state,
            &mut runtime,
            &identity,
            &ONE_SECOND,
            |_, _| Ok(()),
            |_, _, _| Ok(()),
            |_, _, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("smart-contract"));
    }
}
