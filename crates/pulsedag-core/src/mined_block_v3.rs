use std::time::Instant;

use crate::{
    accept::{
        mutate_chain_state_serialized, record_canonical_state_apply_latency, AcceptSource,
        AtomicBlockAcceptance, BlockAcceptanceResult,
    },
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    audit_monetary_state_v3,
    errors::PulseError,
    mempool_protocol::reconcile_mempool_for_protocol,
    mined_block_v2::validate_mined_block_envelope,
    monetary_v3::MonetaryCadenceSegment,
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    state_replay_v3::materialize_authoritative_state_v3,
    types::Block,
    validate_live_reward_settlement_v3, validate_ordered_monetary_reward_v3,
    REWARD_FINALITY_POLICY_VERSION_V3,
};

fn invalid_monetary_mined_block(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "monetary-v3 mined block acceptance: {}",
        message.into()
    ))
}

/// Prepare one mined/RPC v3 monetary block against the authoritative v3
/// ordered-DAG replay before it can be persisted.
pub fn prepare_monetary_v3_mined_block_state(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
    cadence_segments: &[MonetaryCadenceSegment],
) -> Result<ChainState, PulseError> {
    if state.contracts.config.enabled {
        return Err(invalid_monetary_mined_block(
            "v3.0.0 monetary acceptance requires smart-contract execution to remain inactive",
        ));
    }
    validate_mined_block_envelope(block, state, identity)?;

    let mut working = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)?;
    let mut prepared = materialize_authoritative_state_v3(&working, cadence_segments)?;

    let observed_state_root = prepared.utxo.compute_state_root()?;
    if observed_state_root != block.header.state_root {
        return Err(invalid_monetary_mined_block(format!(
            "state root mismatch for {}: committed {}, v3 replay produced {}",
            block.hash, block.header.state_root, observed_state_root
        )));
    }
    if prepared.dag.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(invalid_monetary_mined_block(format!(
            "accepted mined block {} is not the authoritative v3 ordered DAG tip {:?}",
            block.hash, prepared.dag.ordered_dag_tip
        )));
    }

    for transaction in block.transactions.iter().skip(1) {
        if prepared
            .mempool
            .transactions
            .remove(&transaction.txid)
            .is_some()
        {
            prepared.mempool.first_seen.remove(&transaction.txid);
            prepared.mempool.admission_height.remove(&transaction.txid);
            prepared.mempool.counters.confirmed_removed_total = prepared
                .mempool
                .counters
                .confirmed_removed_total
                .saturating_add(1);
        }
        for input in &transaction.inputs {
            prepared
                .mempool
                .spent_outpoints
                .remove(&input.previous_output);
        }
    }
    reconcile_mempool_for_protocol(&mut prepared, identity)?;

    validate_ordered_monetary_reward_v3(&prepared, &block.hash, cadence_segments).map_err(
        |error| invalid_monetary_mined_block(format!("ordered reward validation failed: {error}")),
    )?;
    audit_monetary_state_v3(&prepared, cadence_segments).map_err(|error| {
        invalid_monetary_mined_block(format!("accepted-state monetary audit failed: {error}"))
    })?;
    validate_live_reward_settlement_v3(
        &prepared,
        cadence_segments,
        REWARD_FINALITY_POLICY_VERSION_V3,
    )?;
    Ok(prepared)
}

/// Accept one locally/RPC-mined v3 monetary block through the serialized
/// ChainState coordinator while using the v3 monetary replay as state authority.
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
    if matches!(source, AcceptSource::P2p) {
        return Err(invalid_monetary_mined_block(
            "mining-specific acceptance cannot be used for P2P blocks",
        ));
    }
    if state.contracts.config.enabled {
        return Err(invalid_monetary_mined_block(
            "v3.0.0 monetary acceptance requires smart-contract execution to remain inactive",
        ));
    }

    let mut final_prepare_latency_us = None;
    let mut prepare_attempts = 0_u64;
    let mutation = mutate_chain_state_serialized(
        state,
        source.as_str(),
        |base| {
            let prepare_started = Instant::now();
            let prepared =
                prepare_monetary_v3_mined_block_state(&block, base, identity, cadence_segments)?;
            final_prepare_latency_us =
                Some(prepare_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
            prepare_attempts = prepare_attempts.saturating_add(1);
            Ok((prepared, ()))
        },
        |prepared| persist(&block, prepared),
    )?;
    debug_assert_eq!(state.chain_state_generation, mutation.generation);

    if let Some(latency_us) = final_prepare_latency_us {
        record_canonical_state_apply_latency(latency_us, prepare_attempts.saturating_sub(1));
    }
    broadcast(&block)?;
    Ok(AtomicBlockAcceptance {
        result: BlockAcceptanceResult::Accepted,
        persisted: true,
        committed: true,
        broadcast: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_subsidy, build_activated_v2_mining_template, build_monetary_mining_template_v3,
        compute_block_hash_v2, compute_txid_v2, current_ts, finalize_monetary_mining_template_v3,
        genesis_v3::init_chain_state_v3, mining_template_v2::ActivatedV2MiningTemplateSpec,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION, validate_pow_for_protocol, Transaction,
        TRANSACTION_VERSION_V2,
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

    fn hidden_inputless_noop(chain_id: &str, nonce: u64) -> Transaction {
        let mut transaction = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![],
            outputs: vec![],
            fee: 0,
            nonce,
        };
        transaction.txid = compute_txid_v2(&transaction, chain_id).unwrap();
        transaction
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
    fn accepted_state_audit_rejects_hidden_historical_issuance_before_persist() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state =
            init_chain_state_v3("monetary-v3-audit-boundary".into(), frozen_ts).unwrap();
        let identity = identity(&state);

        let first_timestamp = state.dag.blocks[&state.dag.genesis_hash]
            .header
            .timestamp
            .saturating_add(1);
        let first_template = build_monetary_mining_template_v3(
            &state,
            &identity,
            &ONE_SECOND,
            "pulse1auditboundary",
            44,
            first_timestamp,
            vec![],
        )
        .unwrap();
        let first_finalized =
            finalize_monetary_mining_template_v3(&state, &identity, &ONE_SECOND, &first_template)
                .unwrap();
        let first_block = mine(first_finalized.block, &state, &identity);
        let first_hash = first_block.hash.clone();

        accept_monetary_v3_mined_block_atomically(
            first_block,
            &mut state,
            AcceptSource::Rpc,
            &identity,
            &ONE_SECOND,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();

        let second_timestamp = state.dag.blocks[&first_hash]
            .header
            .timestamp
            .saturating_add(1);
        let second_template = build_monetary_mining_template_v3(
            &state,
            &identity,
            &ONE_SECOND,
            "pulse1auditboundary",
            45,
            second_timestamp,
            vec![],
        )
        .unwrap();
        let second_finalized =
            finalize_monetary_mining_template_v3(&state, &identity, &ONE_SECOND, &second_template)
                .unwrap();
        let second_block = mine(second_finalized.block, &state, &identity);

        let hidden = hidden_inputless_noop(&state.chain_id, 9_999);
        state
            .dag
            .blocks
            .get_mut(&first_hash)
            .unwrap()
            .transactions
            .push(hidden);

        // The historical no-op does not change v2 replay UTXO/state-root output,
        // but it is an additional inputless issuance path that the v3 audit must reject.
        let before = bincode::serialize(&state).unwrap();
        let mut persisted = false;
        let error = accept_monetary_v3_mined_block_atomically(
            second_block,
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

        assert!(error
            .to_string()
            .contains("accepted-state monetary audit failed"));
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
