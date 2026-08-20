use std::collections::BTreeSet;

use crate::{
    accept::{
        mutate_chain_state_serialized, AcceptSource, AtomicBlockAcceptance, BlockAcceptanceResult,
    },
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    apply::apply_transaction,
    errors::PulseError,
    ghostdag_v1::classify_merge_set_v1,
    header_v2::{compute_block_hash_v2, validate_block_header_v2_shape},
    mempool_protocol::reconcile_mempool_for_protocol,
    mining::{current_ts, is_coinbase},
    pow::dev_max_future_drift_secs,
    pow_protocol::{resolve_pow_validation_path, validate_pow_for_protocol, PowValidationPath},
    protocol::{ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V2},
    retarget::expected_difficulty_for_parent,
    state::ChainState,
    state_replay_v2::materialize_authoritative_state_v2,
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    tx_protocol::validate_transaction_for_protocol,
    types::{compute_merkle_root, Block},
    validation::{validate_coinbase_reward, validate_created_utxo_outpoints},
};

fn invalid_network_block(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "activated-v2 p2p block acceptance: {}",
        message.into()
    ))
}

fn classify_network_block_error(error: &PulseError) -> BlockAcceptanceResult {
    match error {
        PulseError::BlockAlreadyExists => BlockAcceptanceResult::Duplicate,
        PulseError::InvalidBlock(message) => {
            let normalized = message.to_ascii_lowercase();
            if normalized.contains("missing parent") {
                BlockAcceptanceResult::MissingParent
            } else if normalized.contains("state root")
                || normalized.contains("ordered dag")
                || normalized.contains("ordering")
            {
                BlockAcceptanceResult::Rejected(error.to_string())
            } else if normalized.contains("difficulty")
                || normalized.contains("proof of work")
                || normalized.contains("pow")
            {
                BlockAcceptanceResult::InvalidPow
            } else {
                BlockAcceptanceResult::Malformed
            }
        }
        PulseError::InvalidTransaction(_)
        | PulseError::InvalidTxid
        | PulseError::InvalidSignature
        | PulseError::DoubleSpend
        | PulseError::InsufficientFunds
        | PulseError::UtxoNotFound => BlockAcceptanceResult::InvalidTransaction,
        PulseError::MissingCoinbase
        | PulseError::MultipleCoinbase
        | PulseError::CoinbaseNotFirst
        | PulseError::ExcessiveCoinbaseReward
        | PulseError::DuplicateUtxoOutpoint(_)
        | PulseError::DuplicateOutpoint(_)
        | PulseError::RewardOverflow => BlockAcceptanceResult::Malformed,
        _ => BlockAcceptanceResult::Rejected(error.to_string()),
    }
}

fn validate_network_block_envelope(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    if resolve_pow_validation_path(identity, state)? != PowValidationPath::ActivatedV2 {
        return Err(invalid_network_block(
            "requires the activated_v2 protocol identity",
        ));
    }
    if state.dag.blocks.contains_key(&block.hash) {
        return Err(PulseError::BlockAlreadyExists);
    }
    if block.header.version != BLOCK_HEADER_VERSION_V2 {
        return Err(invalid_network_block(format!(
            "requires block header version {BLOCK_HEADER_VERSION_V2}, got {}",
            block.header.version
        )));
    }
    validate_block_header_v2_shape(&block.header, &identity.chain_id)?;
    if block.header.parents.is_empty() {
        return Err(invalid_network_block(
            "non-genesis network block must contain at least one parent",
        ));
    }
    let computed_hash = compute_block_hash_v2(&block.header, &identity.chain_id)?;
    if computed_hash != block.hash {
        return Err(invalid_network_block(format!(
            "block hash mismatch: supplied {}, computed {}",
            block.hash, computed_hash
        )));
    }

    let mut expected_height = 0_u64;
    let mut newest_parent_timestamp = 0_u64;
    for parent in &block.header.parents {
        let parent_block = state
            .dag
            .blocks
            .get(parent)
            .ok_or_else(|| invalid_network_block(format!("missing parent {parent}")))?;
        expected_height = expected_height.max(parent_block.header.height.saturating_add(1));
        newest_parent_timestamp = newest_parent_timestamp.max(parent_block.header.timestamp);
    }
    if block.header.height != expected_height {
        return Err(invalid_network_block(format!(
            "height {} does not match derived {}",
            block.header.height, expected_height
        )));
    }
    if block.header.timestamp == 0 || block.header.timestamp < newest_parent_timestamp {
        return Err(invalid_network_block(format!(
            "timestamp {} is older than newest parent {}",
            block.header.timestamp, newest_parent_timestamp
        )));
    }
    let max_future = dev_max_future_drift_secs();
    let now = current_ts();
    if block.header.timestamp > now.saturating_add(max_future) {
        return Err(invalid_network_block(format!(
            "timestamp too far in the future: {} > {} + {}",
            block.header.timestamp, now, max_future
        )));
    }

    let classification = classify_merge_set_v1(block, state).map_err(|error| {
        invalid_network_block(format!("ghostdag_v1 classification failed: {error:?}"))
    })?;
    if block.header.blue_score != classification.blue_score {
        return Err(invalid_network_block(format!(
            "blue score {} does not match derived {}",
            block.header.blue_score, classification.blue_score
        )));
    }
    let selected_parent = classification.selected_parent.ok_or_else(|| {
        invalid_network_block("ghostdag_v1 classification produced no selected parent")
    })?;
    let expected_difficulty = expected_difficulty_for_parent(state, &selected_parent)
        .ok_or_else(|| {
            invalid_network_block(format!(
                "difficulty context unavailable for selected parent {selected_parent}"
            ))
        })?;
    if block.header.difficulty != expected_difficulty {
        return Err(invalid_network_block(format!(
            "difficulty {} does not match selected-parent derived {}",
            block.header.difficulty, expected_difficulty
        )));
    }

    let Some(coinbase) = block.transactions.first() else {
        return Err(PulseError::MissingCoinbase);
    };
    if !is_coinbase(coinbase) {
        return Err(PulseError::CoinbaseNotFirst);
    }
    if block.transactions.iter().skip(1).any(is_coinbase) {
        return Err(PulseError::MultipleCoinbase);
    }

    let mut seen_txids = BTreeSet::new();
    for transaction in &block.transactions {
        if !seen_txids.insert(transaction.txid.clone()) {
            return Err(invalid_network_block(
                "block contains a duplicate transaction",
            ));
        }
        if transaction.version != TRANSACTION_VERSION_V2 {
            return Err(PulseError::InvalidTransaction(format!(
                "network block transaction {} uses version {}, expected {}",
                transaction.txid, transaction.version, TRANSACTION_VERSION_V2
            )));
        }
        if compute_txid_v2(transaction, &identity.chain_id)? != transaction.txid {
            return Err(PulseError::InvalidTxid);
        }
    }
    if compute_merkle_root(&block.transactions) != block.header.merkle_root {
        return Err(invalid_network_block("merkle root mismatch"));
    }
    validate_coinbase_reward(block)?;
    validate_created_utxo_outpoints(block, state)?;

    // Validate spends against the authoritative pre-block UTXO without live
    // mempool spent markers. This first network boundary is intentionally
    // limited to blocks that can be finalized immediately; transient side-tip
    // conflict staging remains a separate live P2P integration slice.
    let mut transaction_context = state.clone();
    transaction_context.mempool.transactions.clear();
    transaction_context.mempool.spent_outpoints.clear();
    apply_transaction(coinbase, &mut transaction_context, block.header.height)?;
    for transaction in block.transactions.iter().skip(1) {
        validate_transaction_for_protocol(transaction, &transaction_context, identity)?;
        apply_transaction(transaction, &mut transaction_context, block.header.height)?;
    }

    validate_pow_for_protocol(&block.header, state, identity)?;
    Ok(())
}

/// Prepare a finalizable activated-v2 block received from the network.
///
/// Unlike the mining-only boundary, this does not require the block parent set
/// to equal the node's current mining-template parent set. It validates the
/// submitted parent set directly through GHOSTDAG-v1 classification, selected-
/// parent difficulty, chain-bound transaction/header identity and v2 PoW.
///
/// This first network slice deliberately accepts only candidates that can be
/// materialized immediately as the authoritative ordered-DAG tip. A valid but
/// still-unabsorbed side tip must be staged by the later live P2P transient-
/// context layer rather than being mislabeled as final canonical state.
pub fn prepare_activated_v2_p2p_block_state(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ChainState, PulseError> {
    validate_network_block_envelope(block, state, identity)?;

    let mut working = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)?;
    let mut materialized = materialize_authoritative_state_v2(&working).map_err(|error| {
        invalid_network_block(format!(
            "candidate is not yet finalizable in authoritative ordered DAG: {error}"
        ))
    })?;

    let observed_state_root = materialized.utxo.compute_state_root()?;
    if observed_state_root != block.header.state_root {
        return Err(invalid_network_block(format!(
            "state root mismatch for {}: committed {}, authoritative replay produced {}",
            block.hash, block.header.state_root, observed_state_root
        )));
    }
    if materialized.dag.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(invalid_network_block(format!(
            "candidate {} is not the authoritative ordered DAG tip {:?}",
            block.hash, materialized.dag.ordered_dag_tip
        )));
    }

    for transaction in block.transactions.iter().skip(1) {
        if materialized
            .mempool
            .transactions
            .remove(&transaction.txid)
            .is_some()
        {
            materialized.mempool.first_seen.remove(&transaction.txid);
            materialized.mempool.counters.confirmed_removed_total = materialized
                .mempool
                .counters
                .confirmed_removed_total
                .saturating_add(1);
        }
        for input in &transaction.inputs {
            materialized
                .mempool
                .spent_outpoints
                .remove(&input.previous_output);
        }
    }
    reconcile_mempool_for_protocol(&mut materialized, identity)?;
    Ok(materialized)
}

/// Atomically persist, publish and rebroadcast a finalizable activated-v2 P2P
/// block under one explicit protocol identity.
pub fn accept_activated_v2_p2p_block_atomically<FPersist, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    source: AcceptSource,
    identity: &ProtocolActivationIdentity,
    mut persist: FPersist,
    broadcast: FBroadcast,
) -> Result<AtomicBlockAcceptance, PulseError>
where
    FPersist: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnOnce(&Block) -> Result<(), PulseError>,
{
    if !matches!(source, AcceptSource::P2p) {
        return Err(invalid_network_block(
            "network acceptance requires the P2P source boundary",
        ));
    }

    if let Err(error) = prepare_activated_v2_p2p_block_state(&block, state, identity) {
        return Ok(AtomicBlockAcceptance::rejected(
            classify_network_block_error(&error),
        ));
    }

    let mutation = match mutate_chain_state_serialized(
        state,
        source.as_str(),
        |base| {
            let prepared = prepare_activated_v2_p2p_block_state(&block, base, identity)?;
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
        build_activated_v2_mining_template, genesis::init_chain_state,
        mining_template_v2::ActivatedV2MiningTemplateSpec,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task28-p2p-v2-finalizable";

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn mined_block(state: &ChainState, identity: &ProtocolActivationIdentity) -> Block {
        let template = build_activated_v2_mining_template(
            state,
            identity,
            ActivatedV2MiningTemplateSpec {
                miner_address: "pulse1task28p2p".to_string(),
                timestamp: current_ts(),
                coinbase_nonce: 19,
                transactions: Vec::new(),
            },
        )
        .unwrap();
        let mut block = template.block;
        for nonce in 0..=200_000_u64 {
            block.header.nonce = nonce;
            block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
            if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
                return block;
            }
        }
        panic!("expected the PoW-limit fixture to find a valid nonce");
    }

    #[test]
    fn valid_v2_block_commits_through_p2p_boundary() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let block = mined_block(&state, &expected_identity);
        let block_hash = block.hash.clone();
        let mut persisted = false;
        let mut broadcast = false;

        let accepted = accept_activated_v2_p2p_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            &expected_identity,
            |candidate, prepared| {
                assert_eq!(candidate.hash, block_hash);
                assert!(prepared.dag.blocks.contains_key(&block_hash));
                persisted = true;
                Ok(())
            },
            |candidate| {
                assert_eq!(candidate.hash, block_hash);
                broadcast = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(accepted.result, BlockAcceptanceResult::Accepted);
        assert!(accepted.persisted && accepted.committed && accepted.broadcast);
        assert!(persisted && broadcast);
        assert!(state.dag.blocks.contains_key(&block_hash));
        assert_eq!(state.dag.ordered_dag_tip.as_deref(), Some(block_hash.as_str()));
    }

    #[test]
    fn non_p2p_source_cannot_enter_network_boundary() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let block = mined_block(&state, &expected_identity);
        let before = bincode::serialize(&state).unwrap();

        let error = accept_activated_v2_p2p_block_atomically(
            block,
            &mut state,
            AcceptSource::LocalMining,
            &expected_identity,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(error.to_string().contains("P2P source boundary"));
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn legacy_identity_fails_closed_without_state_mutation() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let block = mined_block(&state, &expected_identity);
        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        let before = bincode::serialize(&state).unwrap();
        let mut persisted = false;

        let result = accept_activated_v2_p2p_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            &legacy,
            |_, _| {
                persisted = true;
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(!result.result.is_accepted());
        assert!(!persisted);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn missing_parent_has_stable_p2p_classification() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let mut block = mined_block(&state, &expected_identity);
        block.header.parents = vec!["missing-parent".to_string()];
        block.hash = compute_block_hash_v2(&block.header, &expected_identity.chain_id).unwrap();
        let before = bincode::serialize(&state).unwrap();

        let result = accept_activated_v2_p2p_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            &expected_identity,
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();

        assert_eq!(result.result, BlockAcceptanceResult::MissingParent);
        assert!(!result.persisted && !result.committed && !result.broadcast);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }
}
