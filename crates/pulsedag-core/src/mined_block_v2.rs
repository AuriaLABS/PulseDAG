use std::collections::BTreeSet;

use crate::{
    accept::{mutate_chain_state_serialized, AcceptSource, AtomicBlockAcceptance, BlockAcceptanceResult},
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    apply::apply_transaction,
    errors::PulseError,
    header_v2::{compute_block_hash_v2, validate_block_header_v2_shape},
    mempool_protocol::reconcile_mempool_for_protocol,
    mining::{current_ts, is_coinbase},
    mining_protocol::derive_activated_v2_mining_parent_context,
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

fn invalid_mined_block(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "activated-v2 mined block acceptance: {}",
        message.into()
    ))
}

fn validate_mined_block_envelope(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    if resolve_pow_validation_path(identity, state)? != PowValidationPath::ActivatedV2 {
        return Err(invalid_mined_block(
            "requires the activated_v2 protocol identity",
        ));
    }
    if state.dag.blocks.contains_key(&block.hash) {
        return Err(PulseError::BlockAlreadyExists);
    }
    if block.header.version != BLOCK_HEADER_VERSION_V2 {
        return Err(invalid_mined_block(format!(
            "requires block header version {BLOCK_HEADER_VERSION_V2}, got {}",
            block.header.version
        )));
    }
    validate_block_header_v2_shape(&block.header, &identity.chain_id)?;
    let computed_hash = compute_block_hash_v2(&block.header, &identity.chain_id)?;
    if computed_hash != block.hash {
        return Err(invalid_mined_block(format!(
            "block hash mismatch: supplied {}, computed {}",
            block.hash, computed_hash
        )));
    }

    let parent_context = derive_activated_v2_mining_parent_context(state, identity)?;
    if block.header.parents != parent_context.parents {
        return Err(invalid_mined_block(
            "parents do not match the deterministic activated-v2 mining parent context",
        ));
    }
    if block.header.blue_score != parent_context.blue_score {
        return Err(invalid_mined_block(format!(
            "blue score {} does not match derived {}",
            block.header.blue_score, parent_context.blue_score
        )));
    }

    let mut expected_height = 0_u64;
    let mut newest_parent_timestamp = 0_u64;
    for parent in &parent_context.parents {
        let parent_block = state
            .dag
            .blocks
            .get(parent)
            .ok_or_else(|| invalid_mined_block(format!("missing parent {parent}")))?;
        expected_height = expected_height.max(parent_block.header.height.saturating_add(1));
        newest_parent_timestamp = newest_parent_timestamp.max(parent_block.header.timestamp);
    }
    if block.header.height != expected_height {
        return Err(invalid_mined_block(format!(
            "height {} does not match derived {}",
            block.header.height, expected_height
        )));
    }
    if block.header.timestamp == 0 || block.header.timestamp < newest_parent_timestamp {
        return Err(invalid_mined_block(format!(
            "timestamp {} is older than newest parent {}",
            block.header.timestamp, newest_parent_timestamp
        )));
    }
    let now = current_ts();
    let max_future = dev_max_future_drift_secs();
    if block.header.timestamp > now.saturating_add(max_future) {
        return Err(invalid_mined_block(format!(
            "timestamp too far in the future: {} > {} + {}",
            block.header.timestamp, now, max_future
        )));
    }

    let expected_difficulty =
        expected_difficulty_for_parent(state, &parent_context.selected_parent).ok_or_else(|| {
            invalid_mined_block(format!(
                "difficulty context unavailable for selected parent {}",
                parent_context.selected_parent
            ))
        })?;
    if block.header.difficulty != expected_difficulty {
        return Err(invalid_mined_block(format!(
            "difficulty {} does not match derived {}",
            block.header.difficulty, expected_difficulty
        )));
    }

    let Some(coinbase) = block.transactions.first() else {
        return Err(invalid_mined_block("block has no coinbase transaction"));
    };
    if !is_coinbase(coinbase) {
        return Err(invalid_mined_block(
            "first transaction is not a coinbase",
        ));
    }
    if block.transactions.iter().skip(1).any(is_coinbase) {
        return Err(invalid_mined_block(
            "block contains more than one coinbase transaction",
        ));
    }

    let mut seen_txids = BTreeSet::new();
    for transaction in &block.transactions {
        if !seen_txids.insert(transaction.txid.clone()) {
            return Err(invalid_mined_block(
                "block contains a duplicate transaction",
            ));
        }
        if transaction.version != TRANSACTION_VERSION_V2 {
            return Err(invalid_mined_block(format!(
                "transaction {} uses version {}, expected {}",
                transaction.txid, transaction.version, TRANSACTION_VERSION_V2
            )));
        }
        let computed_txid = compute_txid_v2(transaction, &identity.chain_id)?;
        if computed_txid != transaction.txid {
            return Err(PulseError::InvalidTxid);
        }
    }
    if compute_merkle_root(&block.transactions) != block.header.merkle_root {
        return Err(invalid_mined_block("merkle root mismatch"));
    }
    validate_coinbase_reward(block)?;
    validate_created_utxo_outpoints(block, state)?;

    // Validate candidate transactions against the authoritative pre-block UTXO
    // without live mempool spent markers masking transactions that are being
    // confirmed by this block. Apply each accepted transaction to the local
    // context so dependencies inside the candidate are checked in order.
    let mut transaction_context = state.clone();
    transaction_context.mempool.transactions.clear();
    transaction_context.mempool.spent_outpoints.clear();
    apply_transaction(coinbase, &mut transaction_context, block.header.height)?;
    for transaction in block.transactions.iter().skip(1) {
        validate_transaction_for_protocol(transaction, &transaction_context, identity)?;
        apply_transaction(
            transaction,
            &mut transaction_context,
            block.header.height,
        )?;
    }

    validate_pow_for_protocol(&block.header, state, identity)?;
    Ok(())
}

/// Prepare the exact post-commit state for a mined activated-v2 block without
/// mutating the caller.
///
/// This boundary is intentionally narrower than general P2P block acceptance:
/// it requires the deterministic Task 28 mining parent context. After protocol,
/// transaction and PoW checks pass, it commits frozen GHOSTDAG metadata on a
/// clone, materializes the Task 26 authoritative ordered-DAG state, verifies the
/// submitted state root, and reconciles the remaining mempool in the same v2
/// transaction domain.
pub fn prepare_activated_v2_mined_block_state(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ChainState, PulseError> {
    validate_mined_block_envelope(block, state, identity)?;

    let mut working = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)?;
    let mut materialized = materialize_authoritative_state_v2(&working)?;

    let observed_state_root = materialized.utxo.compute_state_root()?;
    if observed_state_root != block.header.state_root {
        return Err(PulseError::InvalidStateRoot(format!(
            "activated-v2 mined block {} committed {}, authoritative replay produced {}",
            block.hash, block.header.state_root, observed_state_root
        )));
    }
    if materialized.dag.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(invalid_mined_block(format!(
            "accepted mined block {} is not the authoritative ordered DAG tip {:?}",
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

/// Atomically persist and publish a mined activated-v2 block through the same
/// serialized ChainState mutation coordinator used by existing acceptance.
///
/// P2P callers are deliberately excluded from this mining-specific boundary;
/// general activated-v2 network block admission remains a separate Task 28
/// slice with broader valid-parent semantics.
pub fn accept_activated_v2_mined_block_atomically<FPersist, FBroadcast>(
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
    if matches!(source, AcceptSource::P2p) {
        return Err(invalid_mined_block(
            "mining-specific acceptance cannot be used for P2P blocks",
        ));
    }

    let mutation = mutate_chain_state_serialized(
        state,
        source.as_str(),
        |base| {
            let prepared = prepare_activated_v2_mined_block_state(&block, base, identity)?;
            Ok((prepared, ()))
        },
        |prepared| persist(&block, prepared),
    )?;
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
        build_activated_v2_mining_template, compute_block_hash_v2,
        genesis::init_chain_state,
        mining_template_v2::ActivatedV2MiningTemplateSpec,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task28-v2-mined-block-acceptance";

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
                miner_address: "pulse1task28miner".to_string(),
                timestamp: current_ts(),
                coinbase_nonce: 7,
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
    fn prepared_mined_block_materializes_authoritative_v2_state() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = identity(&state);
        let block = mined_block(&state, &identity);
        let before = bincode::serialize(&state).unwrap();

        let prepared = prepare_activated_v2_mined_block_state(&block, &state, &identity).unwrap();

        assert_eq!(bincode::serialize(&state).unwrap(), before);
        assert!(prepared.dag.blocks.contains_key(&block.hash));
        assert_eq!(prepared.dag.ordered_dag_tip, Some(block.hash.clone()));
        assert_eq!(prepared.utxo.compute_state_root().unwrap(), block.header.state_root);
        assert_eq!(
            prepared.dag.ordered_dag_state_root.as_deref(),
            Some(block.header.state_root.as_str())
        );
    }

    #[test]
    fn atomic_acceptance_persists_publishes_and_broadcasts_one_state() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let identity = identity(&state);
        let block = mined_block(&state, &identity);
        let expected_hash = block.hash.clone();
        let mut persist_called = false;
        let mut broadcast_called = false;

        let acceptance = accept_activated_v2_mined_block_atomically(
            block.clone(),
            &mut state,
            AcceptSource::Rpc,
            &identity,
            |persisted_block, persisted_state| {
                persist_called = true;
                assert_eq!(persisted_block.hash, expected_hash);
                assert!(persisted_state.dag.blocks.contains_key(&expected_hash));
                assert_eq!(
                    persisted_state.utxo.compute_state_root().unwrap(),
                    persisted_block.header.state_root
                );
                Ok(())
            },
            |broadcast_block| {
                broadcast_called = true;
                assert_eq!(broadcast_block.hash, expected_hash);
                Ok(())
            },
        )
        .unwrap();

        assert!(acceptance.result.is_accepted());
        assert!(acceptance.persisted && acceptance.committed && acceptance.broadcast);
        assert!(persist_called);
        assert!(broadcast_called);
        assert!(state.dag.blocks.contains_key(&expected_hash));
        assert_eq!(state.chain_state_generation, 1);
    }

    #[test]
    fn wrong_identity_fails_without_persist_publish_or_broadcast() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let identity = identity(&state);
        let block = mined_block(&state, &identity);
        let before = bincode::serialize(&state).unwrap();
        let mut wrong = identity.clone();
        wrong.genesis_hash.push_str("-wrong");
        let mut persist_called = false;
        let mut broadcast_called = false;

        assert!(accept_activated_v2_mined_block_atomically(
            block,
            &mut state,
            AcceptSource::Rpc,
            &wrong,
            |_block, _state| {
                persist_called = true;
                Ok(())
            },
            |_block| {
                broadcast_called = true;
                Ok(())
            },
        )
        .is_err());

        assert!(!persist_called);
        assert!(!broadcast_called);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn p2p_cannot_use_mining_specific_acceptance_boundary() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let identity = identity(&state);
        let block = mined_block(&state, &identity);

        assert!(accept_activated_v2_mined_block_atomically(
            block,
            &mut state,
            AcceptSource::P2p,
            &identity,
            |_block, _state| Ok(()),
            |_block| Ok(()),
        )
        .unwrap_err()
        .to_string()
        .contains("cannot be used for P2P"));
    }
}
