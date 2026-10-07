use crate::{
    accept::{
        mutate_chain_state_serialized, AcceptSource, AtomicBlockAcceptance, BlockAcceptanceResult,
    },
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    audit_monetary_state_v3,
    errors::PulseError,
    mempool_protocol::reconcile_mempool_for_protocol,
    monetary_v3::MonetaryCadenceSegment,
    network_block_v2::{
        classify_network_block_error, preflight_activated_v2_p2p_block_context,
        validate_network_block_envelope, ActivatedV2P2pDisposition,
    },
    protocol::ProtocolActivationIdentity,
    reward_settlement_v3::validate_reward_claim_transaction_v3,
    state::ChainState,
    state_replay_v3::materialize_authoritative_state_v3,
    types::Block,
    validate_live_reward_settlement_v3, validate_ordered_monetary_reward_v3,
    REWARD_FINALITY_POLICY_VERSION_V3,
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
    validate_network_block_envelope(block, state, identity)?;

    let mut working = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)?;
    let mut prepared =
        materialize_authoritative_state_v3(&working, cadence_segments).map_err(|error| {
            invalid_monetary_network_block(format!(
                "candidate is not finalizable under authoritative v3 replay: {error}"
            ))
        })?;

    let observed_state_root = prepared.utxo.compute_state_root()?;
    if observed_state_root != block.header.state_root {
        return Err(invalid_monetary_network_block(format!(
            "state root mismatch for {}: committed {}, v3 replay produced {}",
            block.hash, block.header.state_root, observed_state_root
        )));
    }
    if prepared.dag.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(invalid_monetary_network_block(format!(
            "candidate {} is not the authoritative v3 ordered DAG tip {:?}",
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
        |error| {
            invalid_monetary_network_block(format!("ordered reward validation failed: {error}"))
        },
    )?;
    audit_monetary_state_v3(&prepared, cadence_segments).map_err(|error| {
        invalid_monetary_network_block(format!("accepted-state monetary audit failed: {error}"))
    })?;
    validate_live_reward_settlement_v3(
        &prepared,
        cadence_segments,
        REWARD_FINALITY_POLICY_VERSION_V3,
    )?;

    Ok(prepared)
}

/// Classify a v3 monetary P2P candidate without allowing invalid issuance into
/// transient staging. Ordering/context classification is shared with v2, but
/// finalizable candidates are materialized with the v3 monetary replay.
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

    match preflight_activated_v2_p2p_block_context(block, state, identity) {
        ActivatedV2P2pDisposition::Finalizable => {
            match prepare_monetary_v3_p2p_block_state(block, state, identity, cadence_segments) {
                Ok(_) => ActivatedV2P2pDisposition::Finalizable,
                Err(error) => ActivatedV2P2pDisposition::Rejected(BlockAcceptanceResult::Rejected(
                    error.to_string(),
                )),
            }
        }
        disposition => disposition,
    }
}

/// Atomically accept a finalizable v3 P2P block using the v3 authoritative
/// materialization before persistence and broadcast.
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
    if !matches!(source, AcceptSource::P2p) {
        return Err(invalid_monetary_network_block(
            "network acceptance requires the P2P source boundary",
        ));
    }

    if let Err(error) =
        prepare_monetary_v3_p2p_block_state(&block, state, identity, cadence_segments)
    {
        return Ok(AtomicBlockAcceptance::rejected(
            classify_network_block_error(&error),
        ));
    }

    let mutation = match mutate_chain_state_serialized(
        state,
        source.as_str(),
        |base| {
            let prepared =
                prepare_monetary_v3_p2p_block_state(&block, base, identity, cadence_segments)?;
            Ok((prepared, ()))
        },
        |prepared| persist(&block, prepared),
    ) {
        Ok(mutation) => mutation,
        Err(error @ PulseError::StorageError(_)) => return Err(error),
        Err(error) => {
            return Ok(AtomicBlockAcceptance::rejected(
                classify_network_block_error(&error),
            ))
        }
    };
    debug_assert_eq!(state.chain_state_generation, mutation.generation);

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
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        validate_pow_for_protocol, Transaction, TRANSACTION_VERSION_V2,
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
        let parent = state
            .dag
            .selected_chain
            .last()
            .cloned()
            .unwrap_or_else(|| state.dag.genesis_hash.clone());
        let parent_block = &state.dag.blocks[&parent];
        let timestamp = parent_block.header.timestamp.saturating_add(1);
        let claim_nonce = parent_block.header.height.saturating_add(1);
        let template = build_monetary_mining_template_v3(
            state,
            identity,
            &ONE_SECOND,
            "pulse1p2pminer",
            claim_nonce,
            timestamp,
            vec![],
        )
        .unwrap();
        let finalized =
            finalize_monetary_mining_template_v3(state, identity, &ONE_SECOND, &template).unwrap();
        mine(finalized.block, state, identity)
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
    fn accepted_state_audit_rejects_hidden_historical_issuance_before_persist() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state =
            init_chain_state_v3("monetary-v3-p2p-audit-boundary".into(), frozen_ts).unwrap();
        let identity = identity(&state);

        let first_block = monetary_block(&state, &identity);
        let first_hash = first_block.hash.clone();
        let first_claim_txid = first_block.transactions[0].txid.clone();
        accept_monetary_v3_p2p_block_atomically(
            first_block,
            &mut state,
            AcceptSource::P2p,
            &identity,
            &ONE_SECOND,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();

        // Build the second block while history is still clean. The deterministic
        // per-height reward-claim nonce guarantees that the second claim does
        // not collide with the first claim's v2 compatibility outpoint.
        let second_block = monetary_block(&state, &identity);
        assert_ne!(second_block.transactions[0].txid, first_claim_txid);

        let hidden = hidden_inputless_noop(&state.chain_id, 10_001);
        state
            .dag
            .blocks
            .get_mut(&first_hash)
            .unwrap()
            .transactions
            .push(hidden);

        // The second block was built against authoritative v3 state. After
        // mutating accepted history, monetary preflight must fail closed before
        // persistence; no activated-v2 compatibility replay is authoritative here.
        match preflight_monetary_v3_p2p_block(&second_block, &state, &identity, &ONE_SECOND) {
            ActivatedV2P2pDisposition::Rejected(BlockAcceptanceResult::Rejected(reason)) => {
                assert!(reason.contains("additional inputless transaction"));
            }
            other => panic!("expected authoritative v3 replay rejection, got {other:?}"),
        }

        let before = bincode::serialize(&state).unwrap();
        let mut persisted = false;
        let accepted = accept_monetary_v3_p2p_block_atomically(
            second_block,
            &mut state,
            AcceptSource::P2p,
            &identity,
            &ONE_SECOND,
            |_, _| {
                persisted = true;
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();

        assert_eq!(accepted.result, BlockAcceptanceResult::Malformed);
        assert!(!accepted.persisted);
        assert!(!accepted.committed);
        assert!(!accepted.broadcast);
        assert!(!persisted);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn contracts_enabled_rejects_network_candidate() {
        let frozen_ts = current_ts().saturating_sub(10).max(1);
        let mut state = init_chain_state_v3("monetary-v3-p2p-contracts".into(), frozen_ts).unwrap();
        let identity = identity(&state);
        let block = monetary_block(&state, &identity);
        state.contracts.config.enabled = true;

        assert!(
            validate_monetary_v3_p2p_staging_envelope(&block, &state, &identity)
                .unwrap_err()
                .to_string()
                .contains("smart-contract")
        );
    }
}
