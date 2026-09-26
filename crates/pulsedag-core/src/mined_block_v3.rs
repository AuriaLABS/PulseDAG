use crate::{
    accept::{AcceptSource, AtomicBlockAcceptance},
    audit_monetary_state_v3,
    errors::PulseError,
    mined_block_v2::accept_activated_v2_mined_block_atomically,
    monetary_v3::MonetaryCadenceSegment,
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    types::Block,
    validate_ordered_monetary_reward_v3,
};

fn invalid_monetary_mined_block(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "monetary-v3 mined block acceptance: {}",
        message.into()
    ))
}

/// Accept one locally/RPC-mined v3 monetary block through the existing
/// activated-v2 header/DAG/state machinery while replacing legacy height
/// subsidy authority with the frozen ordered-DAG monetary policy.
///
/// The underlying v2 envelope still performs header, parent, transaction,
/// state-root and PoW validation. Before the prepared state can be persisted or
/// published, this wrapper additionally requires:
/// - smart-contract execution to remain inactive;
/// - an amountless, chain-bound v3 reward claim at the canonical ordered score;
/// - no additional inputless issuance path;
/// - exact cumulative authorized supply for the whole accepted ordered DAG.
///
/// Because the monetary checks run inside the serialized persistence callback,
/// any failure aborts before the live ChainState commit and before broadcast.
pub fn accept_monetary_v3_mined_block_atomically<FPersist, FBroadcast>(
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
    if state.contracts.config.enabled {
        return Err(invalid_monetary_mined_block(
            "v3.0.0 monetary acceptance requires smart-contract execution to remain inactive",
        ));
    }

    accept_activated_v2_mined_block_atomically(
        block,
        state,
        source,
        identity,
        |accepted_block, prepared| {
            if prepared.contracts.config.enabled {
                return Err(invalid_monetary_mined_block(
                    "prepared state enabled smart-contract execution",
                ));
            }

            validate_ordered_monetary_reward_v3(prepared, &accepted_block.hash, cadence_segments)
                .map_err(|error| {
                invalid_monetary_mined_block(format!("ordered reward validation failed: {error}"))
            })?;

            audit_monetary_state_v3(prepared, cadence_segments).map_err(|error| {
                invalid_monetary_mined_block(format!(
                    "accepted-state monetary audit failed: {error}"
                ))
            })?;

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
        panic!("expected monetary-v3 PoW-limit fixture to find a valid nonce");
    }

    #[test]
    fn monetary_candidate_accepts_without_legacy_height_subsidy_authority() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state =
            init_chain_state_v3("monetary-v3-mined-acceptance".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);
        let template = build_monetary_mining_template_v3(
            &state,
            &identity,
            &ONE_SECOND,
            "pulse1monetaryminer",
            1,
            timestamp,
            vec![],
        )
        .unwrap();
        let finalized =
            finalize_monetary_mining_template_v3(&state, &identity, &ONE_SECOND, &template)
                .unwrap();
        assert_eq!(finalized.block.transactions[0].outputs[0].amount, 0);
        let block = mine(finalized.block, &state, &identity);
        let expected_hash = block.hash.clone();
        let mut persisted = false;

        let accepted = accept_monetary_v3_mined_block_atomically(
            block,
            &mut state,
            AcceptSource::Rpc,
            &identity,
            &ONE_SECOND,
            |accepted_block, prepared| {
                persisted = true;
                assert_eq!(accepted_block.hash, expected_hash);
                let audit = audit_monetary_state_v3(prepared, &ONE_SECOND).unwrap();
                assert_eq!(audit.current_monetary_score, 1);
                assert_eq!(audit.hidden_issuance_paths, 0);
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
    fn legacy_height_subsidy_coinbase_is_rejected_under_monetary_activation() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state = init_chain_state_v3("monetary-v3-reject-legacy".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);
        let legacy = build_activated_v2_mining_template(
            &state,
            &identity,
            ActivatedV2MiningTemplateSpec {
                miner_address: "pulse1legacyminer".into(),
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
        let before = bincode::serialize(&state).unwrap();
        let mut persisted = false;

        let error = accept_monetary_v3_mined_block_atomically(
            block,
            &mut state,
            AcceptSource::Rpc,
            &identity,
            &ONE_SECOND,
            |_, _| {
                persisted = true;
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("reward"));
        assert!(!persisted);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn contracts_enabled_fails_before_acceptance() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state =
            init_chain_state_v3("monetary-v3-contracts-disabled".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);
        let template = build_monetary_mining_template_v3(
            &state,
            &identity,
            &ONE_SECOND,
            "pulse1miner",
            3,
            timestamp,
            vec![],
        )
        .unwrap();
        let finalized =
            finalize_monetary_mining_template_v3(&state, &identity, &ONE_SECOND, &template)
                .unwrap();
        let block = mine(finalized.block, &state, &identity);
        state.contracts.config.enabled = true;

        let error = accept_monetary_v3_mined_block_atomically(
            block,
            &mut state,
            AcceptSource::Rpc,
            &identity,
            &ONE_SECOND,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("smart-contract"));
    }
}
