use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    apply::apply_transaction,
    errors::PulseError,
    genesis::init_chain_state,
    ghostdag_v1::{classify_merge_set_v1, GHOSTDAG_V1_MAX_ANCESTOR_VISITS},
    header_v2::{compute_block_hash_v2, validate_block_header_v2_shape},
    mining::{current_ts, is_coinbase},
    ordering_v2::{derive_ordered_dag_v2, OrderingV2Error},
    pow::dev_max_future_drift_secs,
    pow_protocol::{resolve_pow_validation_path, validate_pow_for_protocol, PowValidationPath},
    protocol::{ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V2},
    retarget::expected_difficulty_for_parent,
    selection_v2::{calculate_selected_tip_v1, rebuild_selected_chain_v1},
    state::ChainState,
    state_replay_v2::rebuild_authoritative_state_v2,
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    tx_protocol::validate_transaction_for_protocol,
    types::{compute_merkle_root, Block, Hash},
    validation::{validate_coinbase_reward, validate_created_utxo_outpoints},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivatedV2P2pContextDisposition {
    ImmediatelyFinalizable,
    DeferredSideTip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedV2P2pContextValidation {
    pub block_hash: Hash,
    pub disposition: ActivatedV2P2pContextDisposition,
    pub selected_parent: Hash,
    pub ordered_dag_digest: String,
    pub state_root: String,
}

fn invalid_context_block(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "activated-v2 p2p context validation: {}",
        message.into()
    ))
}

fn candidate_past_hashes(block: &Block, state: &ChainState) -> Result<BTreeSet<Hash>, PulseError> {
    let mut seen = BTreeSet::new();
    let mut pending = block
        .header
        .parents
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut visits = 0usize;

    while let Some(hash) = pending.pop_first() {
        if !seen.insert(hash.clone()) {
            continue;
        }
        if visits >= GHOSTDAG_V1_MAX_ANCESTOR_VISITS {
            return Err(invalid_context_block(format!(
                "candidate past exceeds bounded ancestor visit limit {}",
                GHOSTDAG_V1_MAX_ANCESTOR_VISITS
            )));
        }
        visits = visits.saturating_add(1);
        let parent = state
            .dag
            .blocks
            .get(&hash)
            .ok_or_else(|| invalid_context_block(format!("missing parent {hash}")))?;
        for ancestor in &parent.header.parents {
            if !seen.contains(ancestor) {
                pending.insert(ancestor.clone());
            }
        }
    }

    if !seen.contains(&state.dag.genesis_hash) {
        return Err(invalid_context_block(
            "candidate past does not close over the configured genesis",
        ));
    }
    Ok(seen)
}

fn candidate_past_projection(block: &Block, state: &ChainState) -> Result<ChainState, PulseError> {
    let past = candidate_past_hashes(block, state)?;
    let mut projected = state.clone();

    projected.dag.blocks.retain(|hash, _| past.contains(hash));
    projected
        .dag
        .selected_parents
        .retain(|hash, _| past.contains(hash));
    projected.dag.blue_work.retain(|hash, _| past.contains(hash));
    projected
        .dag
        .merge_set_diagnostics
        .retain(|hash, _| past.contains(hash));
    projected
        .dag
        .merge_set_blues
        .retain(|hash, _| past.contains(hash));
    projected
        .dag
        .merge_set_reds
        .retain(|hash, _| past.contains(hash));

    for (anchor, hashes) in projected
        .dag
        .merge_set_blues
        .iter()
        .chain(projected.dag.merge_set_reds.iter())
    {
        if let Some(outside) = hashes.iter().find(|hash| !past.contains(*hash)) {
            return Err(invalid_context_block(format!(
                "past metadata for anchor {anchor} references non-past block {outside}"
            )));
        }
    }
    for (hash, selected_parent) in &projected.dag.selected_parents {
        if selected_parent
            .as_ref()
            .is_some_and(|parent| !past.contains(parent))
        {
            return Err(invalid_context_block(format!(
                "past selected-parent metadata for {hash} escapes candidate past"
            )));
        }
    }

    projected.dag.children = HashMap::new();
    projected.dag.tips = past.iter().cloned().collect::<HashSet<_>>();
    projected.dag.best_height = 0;
    let blocks = projected.dag.blocks.values().cloned().collect::<Vec<_>>();
    for accepted in blocks {
        projected.dag.best_height = projected.dag.best_height.max(accepted.header.height);
        for parent in &accepted.header.parents {
            if !past.contains(parent) {
                return Err(invalid_context_block(format!(
                    "candidate past is not parent-closed at block {} parent {}",
                    accepted.hash, parent
                )));
            }
            projected.dag.tips.remove(parent);
            let children = projected.dag.children.entry(parent.clone()).or_default();
            children.push(accepted.hash.clone());
            children.sort();
            children.dedup();
        }
    }

    let selected_tip = calculate_selected_tip_v1(&projected).map_err(|error| {
        invalid_context_block(format!(
            "cannot select candidate-past tip: {error:?}"
        ))
    })?;
    projected.dag.selected_chain = rebuild_selected_chain_v1(&projected, selected_tip).map_err(
        |error| {
            invalid_context_block(format!(
                "cannot rebuild candidate-past selected chain: {error:?}"
            ))
        },
    )?;
    projected.dag.ordered_dag.clear();
    projected.dag.ordered_dag_tip = None;
    projected.dag.ordered_dag_state_root = None;
    projected.dag.ordered_dag_conflict_diagnostics.clear();
    Ok(projected)
}

fn validate_context_envelope(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<Hash, PulseError> {
    if resolve_pow_validation_path(identity, state)? != PowValidationPath::ActivatedV2 {
        return Err(invalid_context_block(
            "requires the activated_v2 protocol identity",
        ));
    }
    if state.dag.blocks.contains_key(&block.hash) {
        return Err(PulseError::BlockAlreadyExists);
    }
    if block.header.version != BLOCK_HEADER_VERSION_V2 {
        return Err(invalid_context_block(format!(
            "requires block header version {BLOCK_HEADER_VERSION_V2}, got {}",
            block.header.version
        )));
    }
    validate_block_header_v2_shape(&block.header, &identity.chain_id)?;
    if block.header.parents.is_empty() {
        return Err(invalid_context_block(
            "non-genesis network block must contain at least one parent",
        ));
    }
    let computed_hash = compute_block_hash_v2(&block.header, &identity.chain_id)?;
    if computed_hash != block.hash {
        return Err(invalid_context_block(format!(
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
            .ok_or_else(|| invalid_context_block(format!("missing parent {parent}")))?;
        expected_height = expected_height.max(parent_block.header.height.saturating_add(1));
        newest_parent_timestamp = newest_parent_timestamp.max(parent_block.header.timestamp);
    }
    if block.header.height != expected_height {
        return Err(invalid_context_block(format!(
            "height {} does not match derived {}",
            block.header.height, expected_height
        )));
    }
    if block.header.timestamp == 0 || block.header.timestamp < newest_parent_timestamp {
        return Err(invalid_context_block(format!(
            "timestamp {} is older than newest parent {}",
            block.header.timestamp, newest_parent_timestamp
        )));
    }
    let now = current_ts();
    let max_future = dev_max_future_drift_secs();
    if block.header.timestamp > now.saturating_add(max_future) {
        return Err(invalid_context_block(format!(
            "timestamp too far in the future: {} > {} + {}",
            block.header.timestamp, now, max_future
        )));
    }

    let classification = classify_merge_set_v1(block, state).map_err(|error| {
        invalid_context_block(format!("ghostdag_v1 classification failed: {error:?}"))
    })?;
    if block.header.blue_score != classification.blue_score {
        return Err(invalid_context_block(format!(
            "blue score {} does not match derived {}",
            block.header.blue_score, classification.blue_score
        )));
    }
    let selected_parent = classification.selected_parent.ok_or_else(|| {
        invalid_context_block("ghostdag_v1 classification produced no selected parent")
    })?;
    let expected_difficulty =
        expected_difficulty_for_parent(state, &selected_parent).ok_or_else(|| {
            invalid_context_block(format!(
                "difficulty context unavailable for selected parent {selected_parent}"
            ))
        })?;
    if block.header.difficulty != expected_difficulty {
        return Err(invalid_context_block(format!(
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
            return Err(invalid_context_block(
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
        return Err(invalid_context_block("merkle root mismatch"));
    }
    validate_coinbase_reward(block)?;
    validate_pow_for_protocol(&block.header, state, identity)?;
    Ok(selected_parent)
}

fn replay_pre_candidate_state(
    context: &ChainState,
    candidate_hash: &Hash,
) -> Result<ChainState, PulseError> {
    let ordered = derive_ordered_dag_v2(context).map_err(|error| {
        invalid_context_block(format!(
            "candidate context has no authoritative ordered DAG: {error:?}"
        ))
    })?;
    if ordered.blocks.last() != Some(candidate_hash) {
        return Err(invalid_context_block(format!(
            "candidate {candidate_hash} is not the tip of its own past-context order {:?}",
            ordered.blocks.last()
        )));
    }

    let mut replay = init_chain_state(context.chain_id.clone());
    replay.dag.consensus_mode = context.dag.consensus_mode;
    replay.dag.selected_parent_policy = context.dag.selected_parent_policy;
    for hash in ordered.blocks {
        if hash == context.dag.genesis_hash {
            continue;
        }
        if &hash == candidate_hash {
            break;
        }
        let accepted = context.dag.blocks.get(&hash).ok_or_else(|| {
            invalid_context_block(format!(
                "ordered candidate context references missing block {hash}"
            ))
        })?;
        for transaction in &accepted.transactions {
            let mut next = replay.clone();
            match apply_transaction(transaction, &mut next, accepted.header.height) {
                Ok(()) => replay = next,
                Err(PulseError::UtxoNotFound | PulseError::DuplicateUtxoOutpoint(_)) => {}
                Err(error) => return Err(error),
            }
        }
    }
    replay.mempool.transactions.clear();
    replay.mempool.spent_outpoints.clear();
    replay.mempool.first_seen.clear();
    Ok(replay)
}

fn validate_candidate_transactions(
    block: &Block,
    context: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    let mut transaction_context = replay_pre_candidate_state(context, &block.hash)?;
    validate_created_utxo_outpoints(block, &transaction_context)?;
    let coinbase = block.transactions.first().ok_or(PulseError::MissingCoinbase)?;
    apply_transaction(coinbase, &mut transaction_context, block.header.height)?;
    for transaction in block.transactions.iter().skip(1) {
        validate_transaction_for_protocol(transaction, &transaction_context, identity)?;
        apply_transaction(transaction, &mut transaction_context, block.header.height)?;
    }
    Ok(())
}

fn live_disposition(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ActivatedV2P2pContextDisposition, PulseError> {
    let mut live = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut live, identity)?;
    match derive_ordered_dag_v2(&live) {
        Err(OrderingV2Error::UnclassifiedBlock { .. }) => {
            Ok(ActivatedV2P2pContextDisposition::DeferredSideTip)
        }
        Err(error) => Err(invalid_context_block(format!(
            "live ordered-DAG derivation failed outside deferred side-tip classification: {error:?}"
        ))),
        Ok(ordered) if ordered.blocks.last() != Some(&block.hash) => {
            Ok(ActivatedV2P2pContextDisposition::DeferredSideTip)
        }
        Ok(_) => {
            let replay = rebuild_authoritative_state_v2(&live)?;
            if replay.diagnostics.state_root == block.header.state_root
                && replay.diagnostics.ordered_dag_tip.as_ref() == Some(&block.hash)
            {
                Ok(ActivatedV2P2pContextDisposition::ImmediatelyFinalizable)
            } else {
                Ok(ActivatedV2P2pContextDisposition::DeferredSideTip)
            }
        }
    }
}

/// Validate one activated-v2 P2P block against the immutable DAG formed by its
/// known ancestors rather than the receiver's current global tip set.
///
/// This non-mutating boundary is the prerequisite for safe side-tip staging. It
/// proves header/hash/PoW/transaction identity, GHOSTDAG metadata, selected-
/// parent difficulty, exact authoritative transaction pre-state and the block's
/// own state-root commitment in its past context. Only after that proof does it
/// classify whether the same block is immediately finalizable in the receiver's
/// current full DAG or must be deferred until a later merge anchor classifies it.
///
/// This first context slice intentionally requires every parent to already exist
/// in the accepted DAG. Staged-parent closure and atomic anchor promotion are
/// separate follow-up boundaries. The caller's `ChainState` is never mutated.
pub fn validate_activated_v2_p2p_block_context(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ActivatedV2P2pContextValidation, PulseError> {
    if state.dag.blocks.contains_key(&block.hash) {
        return Err(PulseError::BlockAlreadyExists);
    }
    let projection = candidate_past_projection(block, state)?;
    let selected_parent = validate_context_envelope(block, &projection, identity)?;

    let mut context = projection;
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut context, identity)?;
    validate_candidate_transactions(block, &context, identity)?;
    let replay = rebuild_authoritative_state_v2(&context)?;
    if replay.diagnostics.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(invalid_context_block(format!(
            "candidate {} is not the authoritative tip of its past context {:?}",
            block.hash, replay.diagnostics.ordered_dag_tip
        )));
    }
    if replay.diagnostics.state_root != block.header.state_root {
        return Err(invalid_context_block(format!(
            "state root mismatch for {} in candidate past: committed {}, replay produced {}",
            block.hash, block.header.state_root, replay.diagnostics.state_root
        )));
    }

    Ok(ActivatedV2P2pContextValidation {
        block_hash: block.hash.clone(),
        disposition: live_disposition(block, state, identity)?,
        selected_parent,
        ordered_dag_digest: replay.diagnostics.ordered_dag_digest,
        state_root: replay.diagnostics.state_root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_activated_v2_mining_template,
        genesis::init_chain_state,
        mining_template_v2::ActivatedV2MiningTemplateSpec,
        network_block_v2::prepare_activated_v2_p2p_block_state,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task28-p2p-v2-context";

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn mined_block(
        state: &ChainState,
        identity: &ProtocolActivationIdentity,
        coinbase_nonce: u64,
    ) -> Block {
        let template = build_activated_v2_mining_template(
            state,
            identity,
            ActivatedV2MiningTemplateSpec {
                miner_address: format!("pulse1task28context{coinbase_nonce}"),
                timestamp: current_ts(),
                coinbase_nonce,
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
    fn current_tip_candidate_is_immediately_finalizable() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let block = mined_block(&state, &expected_identity, 41);
        let before = bincode::serialize(&state).unwrap();

        let validated =
            validate_activated_v2_p2p_block_context(&block, &state, &expected_identity).unwrap();

        assert_eq!(
            validated.disposition,
            ActivatedV2P2pContextDisposition::ImmediatelyFinalizable
        );
        assert_eq!(validated.block_hash, block.hash);
        assert_eq!(validated.state_root, block.header.state_root);
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn parallel_valid_block_is_deferred_instead_of_rejected_by_live_tip_state() {
        let base = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let first = mined_block(&base, &expected_identity, 51);
        let side = mined_block(&base, &expected_identity, 52);
        assert_ne!(first.hash, side.hash);

        let live = prepare_activated_v2_p2p_block_state(&first, &base, &expected_identity).unwrap();
        let before = bincode::serialize(&live).unwrap();
        let validated =
            validate_activated_v2_p2p_block_context(&side, &live, &expected_identity).unwrap();

        assert_eq!(
            validated.disposition,
            ActivatedV2P2pContextDisposition::DeferredSideTip
        );
        assert_eq!(validated.block_hash, side.hash);
        assert_eq!(validated.state_root, side.header.state_root);
        assert_eq!(bincode::serialize(&live).unwrap(), before);
        assert!(prepare_activated_v2_p2p_block_state(&side, &live, &expected_identity).is_err());
    }

    #[test]
    fn wrong_state_root_is_rejected_even_when_candidate_is_a_side_tip() {
        let base = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let first = mined_block(&base, &expected_identity, 61);
        let mut side = mined_block(&base, &expected_identity, 62);
        let live = prepare_activated_v2_p2p_block_state(&first, &base, &expected_identity).unwrap();

        side.header.state_root = "11".repeat(32);
        side.hash = compute_block_hash_v2(&side.header, &expected_identity.chain_id).unwrap();
        for nonce in 0..=200_000_u64 {
            side.header.nonce = nonce;
            side.hash = compute_block_hash_v2(&side.header, &expected_identity.chain_id).unwrap();
            if validate_pow_for_protocol(&side.header, &base, &expected_identity).is_ok() {
                break;
            }
        }

        let error = validate_activated_v2_p2p_block_context(&side, &live, &expected_identity)
            .unwrap_err();
        assert!(error.to_string().contains("state root mismatch"));
    }

    #[test]
    fn missing_parent_fails_before_any_live_state_mutation() {
        let base = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let mut block = mined_block(&base, &expected_identity, 71);
        block.header.parents = vec!["missing-parent".to_string()];
        block.hash = compute_block_hash_v2(&block.header, &expected_identity.chain_id).unwrap();
        let before = bincode::serialize(&base).unwrap();

        let error = validate_activated_v2_p2p_block_context(&block, &base, &expected_identity)
            .unwrap_err();
        assert!(error.to_string().contains("missing parent"));
        assert_eq!(bincode::serialize(&base).unwrap(), before);
    }
}
