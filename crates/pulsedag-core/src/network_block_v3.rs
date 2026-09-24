use crate::{
    accept::{AcceptSource, AtomicBlockAcceptance, BlockAcceptanceResult},
    audit_monetary_state_v3,
    errors::PulseError,
    validate_live_reward_settlement_v3,
    GHOSTDAG_V1_FINALITY_POLICY_VERSION,
    monetary_v3::MonetaryCadenceSegment,
    network_block_v2::{
        accept_activated_v2_p2p_block_atomically, preflight_activated_v2_p2p_block,
        prepare_activated_v2_p2p_block_state, ActivatedV2P2pDisposition,
    },
    protocol::ProtocolActivationIdentity,
    reward_settlement_v3::validate_reward_claim_transaction_v3,
    state::ChainState,
    types::Block,
    validate_ordered_monetary_reward_v3,
};

fn invalid_monetary_network_block(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "monetary-v3 p2p block acceptance: {}",
        message.into()
    ))
}

/// Validate the branch-independent v3 monetary envelope before a network block
/// is allowed into finalizable, staged, or missing-parent runtime state.
///
/// This check is intentionally independent of canonical monetary score so it
/// can run before DAG placement is known. It prevents a legacy amount-bearing
/// coinbase or any second inputless issuance transaction from being retained as
/// transient P2P context under a v3 monetary activation.
pub fn validate_monetary_v3_p2p_staging_envelope(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    if state.contracts.config.enabled {
        return Err(invalid_monetary_network_block(
            "v3.0.0 monetary acceptance requires smart-contract execution to remain inactive",
        ));
    }
    if identity.chain_id != state.chain_id {
        return Err(PulseError::ChainIdMismatch);
    }

    let claim = block
        .transactions
        .first()
        .ok_or_else(|| invalid_monetary_network_block("block has no reward claim"))?;
    validate_reward_claim_transaction_v3(claim, &identity.chain_id).map_err(|error| {
        invalid_monetary_network_block(format!("invalid reward claim: {error}"))
    })?;

    if let Some(hidden) = block
        .transactions
        .iter()
        .skip(1)
        .find(|transaction| transaction.inputs.is_empty())
    {
        return Err(invalid_monetary_network_block(format!(
            "additional inputless transaction {} creates a hidden issuance path",
            hidden.txid
        )));
    }

    Ok(())
}

/// Prepare one finalizable v3 network block and prove its exact monetary effect
/// against the state-derived ordered-DAG score before it can become authoritative.
pub fn prepare_monetary_v3_p2p_block_state(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<ChainState, PulseError> {
    validate_monetary_v3_p2p_staging_envelope(block, state, identity)?;
    let prepared = prepare_activated_v2_p2p_block_state(block, state, identity)?;

    validate_ordered_monetary_reward_v3(&prepared, &block.hash, cadence_segments).map_err(
        |error| {
            invalid_monetary_network_block(format!(
                "ordered reward validation failed: {error}"
            ))
        },
    )?;
    audit_monetary_state_v3(&prepared, cadence_segments).map_err(|error| {
        invalid_monetary_network_block(format!("accepted-state monetary audit failed: {error}"))
    })?;
    validate_live_reward_settlement_v3(
        &prepared,
        cadence_segments,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION,
    )?;

    Ok(prepared)
}

/// Classify a v3 monetary P2P candidate without allowing invalid issuance into
/// transient staging. Canonical-score checks are deferred only when the
/// activated-v2 DAG classifier itself reports deferred context.
pub fn preflight_monetary_v3_p2p_block(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
) -> ActivatedV2P2pDisposition {
    if let Err(error) = validate_monetary_v3_p2p_staging_envelope(block, state, identity) {
        return ActivatedV2P2pDisposition::Rejected(BlockAcceptanceResult::Rejected(
            error.to_string(),
        ));
    }

    match preflight_activated_v2_p2p_block(block, state, identity) {
        ActivatedV2P2pDisposition::Finalizable => {
            match prepare_monetary_v3_p2p_block_state(block, state, identity, cadence_segments) {
                Ok(_) => ActivatedV2P2pDisposition::Finalizable,
                Err(error) => ActivatedV2P2pDisposition::Rejected(
                    BlockAcceptanceResult::Rejected(error.to_string()),
                ),
            }
        }
        disposition => disposition,
    }
}

/// Atomically accept a finalizable v3 P2P block. The existing activated-v2
/// network path continues to own header/DAG/state/PoW checks, while monetary
/// validation is injected into the serialized persistence boundary. A failed
/// monetary audit therefore prevents persistence, live-state commit and
/// broadcast.
pub fn accept_monetary_v3_p2p_block_atomically<FPersist, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
    mut persist: FPersist,
    broadcast: FBroadcast,
) -> Result<AtomicBlockAcceptance, PulseError>
where
    FPersist: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnOnce(&Block) -> Result<(), PulseError>,
{
    validate_monetary_v3_p2p_staging_envelope(&block, state, identity)?;

    accept_activated_v2_p2p_block_atomically(
        block,
        state,
        source,
        identity,
        |accepted_block, prepared| {
            validate_ordered_monetary_reward_v3(
                prepared,
                &accepted_block.hash,
                cadence_segments,
            )
            .map_err(|error| {
                invalid_monetary_network_block(format!(
                    "ordered reward validation failed: {error}"
                ))
            })?;
            audit_monetary_state_v3(prepared, cadence_segments).map_err(|error| {
                invalid_monetary_network_block(format!(
                    "accepted-state monetary audit failed: {error}"
                ))
            })?;
            validate_live_reward_settlement_v3(
                prepared,
                cadence_segments,
                GHOSTDAG_V1_FINALITY_POLICY_VERSION,
            )?;
            persist(accepted_block, prepared)
        },
        broadcast,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_subsidy, build_activated_v2_mining_template, build_monetary_mining_template_v3,
        compute_block_hash_v2, current_ts, finalize_monetary_mining_template_v3,
        genesis_v3::init_chain_state_v3, mining_template_v2::ActivatedV2MiningTemplateSpec,
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

    fn mine(mut block: Block, state: &ChainState, identity: &ProtocolActivationIdentity) -> Block {
        for nonce in 0..=200_000_u64 {
            block.header.nonce = nonce;
            block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
            if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
                return block;
            }
        }
        panic!("expected monetary-v3 P2P fixture to find a valid nonce");
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
            "pulse1p2pminer",
            1,
            timestamp,
            vec![],
        )
        .unwrap();
        let finalized =
            finalize_monetary_mining_template_v3(state, identity, &ONE_SECOND, &template).unwrap();
        mine(finalized.block, state, identity)
    }

    #[test]
    fn amountless_reward_claim_is_finalizable_and_audited() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let state = init_chain_state_v3("monetary-v3-p2p".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let block = monetary_block(&state, &identity);

        assert_eq!(block.transactions[0].outputs[0].amount, 0);
        assert_eq!(
            preflight_monetary_v3_p2p_block(&block, &state, &identity, &ONE_SECOND),
            ActivatedV2P2pDisposition::Finalizable
        );

        let prepared =
            prepare_monetary_v3_p2p_block_state(&block, &state, &identity, &ONE_SECOND).unwrap();
        let audit = audit_monetary_state_v3(&prepared, &ONE_SECOND).unwrap();
        assert_eq!(audit.current_monetary_score, 1);
        assert_eq!(audit.hidden_issuance_paths, 0);
    }

    #[test]
    fn legacy_height_subsidy_block_is_rejected_before_p2p_staging() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let state = init_chain_state_v3("monetary-v3-p2p-legacy".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);
        let legacy = build_activated_v2_mining_template(
            &state,
            &identity,
            ActivatedV2MiningTemplateSpec {
                miner_address: "pulse1legacy".into(),
                timestamp,
                coinbase_nonce: 2,
                transactions: vec![],
            },
        )
        .unwrap();
        assert_eq!(
            legacy.block.transactions[0].outputs[0].amount,
            block_subsidy(legacy.block.header.height)
        );
        let block = mine(legacy.block, &state, &identity);

        assert!(matches!(
            preflight_monetary_v3_p2p_block(&block, &state, &identity, &ONE_SECOND),
            ActivatedV2P2pDisposition::Rejected(_)
        ));
    }

    #[test]
    fn atomic_p2p_acceptance_audits_before_persist_and_commit() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state = init_chain_state_v3("monetary-v3-p2p-atomic".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let block = monetary_block(&state, &identity);
        let expected_hash = block.hash.clone();
        let mut persisted = false;

        let accepted = accept_monetary_v3_p2p_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            &identity,
            &ONE_SECOND,
            |accepted_block, prepared| {
                persisted = true;
                assert_eq!(accepted_block.hash, expected_hash);
                audit_monetary_state_v3(prepared, &ONE_SECOND).unwrap();
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(accepted.result.is_accepted());
        assert!(persisted);
        assert!(state.dag.blocks.contains_key(&expected_hash));
    }

    #[test]
    fn contracts_enabled_rejects_network_candidate() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state =
            init_chain_state_v3("monetary-v3-p2p-contracts".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let block = monetary_block(&state, &identity);
        state.contracts.config.enabled = true;

        assert!(validate_monetary_v3_p2p_staging_envelope(&block, &state, &identity)
            .unwrap_err()
            .to_string()
            .contains("smart-contract"));
    }
}
