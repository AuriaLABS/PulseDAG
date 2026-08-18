use serde::{Deserialize, Serialize};

use crate::{
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    errors::PulseError,
    header_v2::compute_block_hash_v2,
    mining::is_coinbase,
    mining_protocol::derive_activated_v2_mining_parent_context,
    pow_protocol::{resolve_pow_validation_path, PowValidationPath},
    protocol::{ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V2},
    retarget::expected_difficulty_for_parent,
    state::ChainState,
    state_replay_v2::{rebuild_authoritative_state_v2, StateReplayV2Diagnostics},
    tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
    types::{compute_merkle_root, Block, Hash},
    validation::validate_coinbase_reward,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivatedV2MiningStateContext {
    pub block_hash: Hash,
    pub state_root: String,
    pub ordered_dag_tip: Option<Hash>,
    pub ordered_dag_digest: String,
    pub applied_transactions: usize,
    pub skipped_conflicting_transactions: usize,
    pub conflict_diagnostics: Vec<String>,
}

fn invalid_state(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("activated-v2 mining state: {}", message.into()))
}

fn validate_candidate_envelope(
    block: &mut Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    if resolve_pow_validation_path(identity, state)? != PowValidationPath::ActivatedV2 {
        return Err(invalid_state("requires the activated_v2 protocol identity"));
    }
    if block.header.version != BLOCK_HEADER_VERSION_V2 {
        return Err(invalid_state(format!(
            "requires block header version {BLOCK_HEADER_VERSION_V2}, got {}",
            block.header.version
        )));
    }
    if block.header.nonce != 0 {
        return Err(invalid_state(
            "candidate state must be finalized before nonce search",
        ));
    }

    let parent_context = derive_activated_v2_mining_parent_context(state, identity)?;
    if block.header.parents != parent_context.parents {
        return Err(invalid_state(
            "candidate parents do not match the deterministic activated-v2 mining parent context",
        ));
    }
    if block.header.blue_score != parent_context.blue_score {
        return Err(invalid_state(format!(
            "candidate blue score {} does not match derived {}",
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
            .ok_or_else(|| invalid_state(format!("missing parent {parent}")))?;
        expected_height = expected_height.max(parent_block.header.height.saturating_add(1));
        newest_parent_timestamp = newest_parent_timestamp.max(parent_block.header.timestamp);
    }
    if block.header.height != expected_height {
        return Err(invalid_state(format!(
            "candidate height {} does not match derived {}",
            block.header.height, expected_height
        )));
    }
    if block.header.timestamp == 0 || block.header.timestamp < newest_parent_timestamp {
        return Err(invalid_state(format!(
            "candidate timestamp {} is older than newest parent {}",
            block.header.timestamp, newest_parent_timestamp
        )));
    }

    let expected_difficulty =
        expected_difficulty_for_parent(state, &parent_context.selected_parent).ok_or_else(
            || {
                invalid_state(format!(
                    "difficulty context unavailable for selected parent {}",
                    parent_context.selected_parent
                ))
            },
        )?;
    if block.header.difficulty != expected_difficulty {
        return Err(invalid_state(format!(
            "candidate difficulty {} does not match derived {}",
            block.header.difficulty, expected_difficulty
        )));
    }

    let Some(coinbase) = block.transactions.first() else {
        return Err(invalid_state("candidate has no coinbase transaction"));
    };
    if !is_coinbase(coinbase) {
        return Err(invalid_state(
            "first candidate transaction is not a coinbase",
        ));
    }
    if block.transactions.iter().skip(1).any(is_coinbase) {
        return Err(invalid_state(
            "candidate contains more than one coinbase transaction",
        ));
    }

    for transaction in &block.transactions {
        if transaction.version != TRANSACTION_VERSION_V2 {
            return Err(invalid_state(format!(
                "candidate transaction {} uses version {}, expected {}",
                transaction.txid, transaction.version, TRANSACTION_VERSION_V2
            )));
        }
        let computed_txid = compute_txid_v2(transaction, &identity.chain_id)?;
        if computed_txid != transaction.txid {
            return Err(invalid_state(format!(
                "candidate transaction txid mismatch: supplied {}, computed {}",
                transaction.txid, computed_txid
            )));
        }
    }

    validate_coinbase_reward(block)?;
    block.header.merkle_root = compute_merkle_root(&block.transactions);
    Ok(())
}

fn replay_candidate(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<StateReplayV2Diagnostics, PulseError> {
    let mut working = state.clone();
    commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)?;
    Ok(rebuild_authoritative_state_v2(&working)?.diagnostics)
}

/// Finalize the authoritative state commitment for an activated-v2 mining candidate.
///
/// This is a mining-only, non-live boundary. It assumes every non-coinbase
/// transaction came from protocol-bound mempool admission, verifies that every
/// transaction is v2/chain-bound, and never substitutes the legacy selected-tip
/// state transition. The candidate must already carry the deterministic parent,
/// height, blue-score and difficulty context and must not have entered nonce
/// search yet.
///
/// The block hash includes `state_root`, while the authoritative v2 state root is
/// derived after inserting the candidate into the deterministic DAG-order replay.
/// To avoid silently accepting a circular commitment, this helper starts from a
/// fixed placeholder root, derives the authoritative root, rebuilds the v2 hash,
/// and replays once more. The second replay must produce the exact same root or
/// finalization fails closed.
///
/// The caller's `ChainState` is never mutated. This function does not validate or
/// search PoW, persist the block, broadcast it, activate GhostdagV1, or change the
/// historical v1 mining path.
pub fn finalize_activated_v2_mining_candidate_state(
    block: &mut Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ActivatedV2MiningStateContext, PulseError> {
    validate_candidate_envelope(block, state, identity)?;

    block.header.state_root = "00".repeat(32);
    block.hash = compute_block_hash_v2(&block.header, &identity.chain_id)?;
    let first = replay_candidate(block, state, identity)?;

    block.header.state_root = first.state_root;
    block.hash = compute_block_hash_v2(&block.header, &identity.chain_id)?;
    let final_replay = replay_candidate(block, state, identity)?;

    if final_replay.state_root != block.header.state_root {
        return Err(invalid_state(format!(
            "state-root/hash fixed point is unstable: committed {}, replayed {}",
            block.header.state_root, final_replay.state_root
        )));
    }
    if final_replay.ordered_dag_tip.as_ref() != Some(&block.hash) {
        return Err(invalid_state(format!(
            "finalized candidate {} is not the authoritative ordered DAG tip {:?}",
            block.hash, final_replay.ordered_dag_tip
        )));
    }

    Ok(ActivatedV2MiningStateContext {
        block_hash: block.hash.clone(),
        state_root: block.header.state_root.clone(),
        ordered_dag_tip: final_replay.ordered_dag_tip,
        ordered_dag_digest: final_replay.ordered_dag_digest,
        applied_transactions: final_replay.applied_transactions,
        skipped_conflicting_transactions: final_replay.skipped_conflicting_transactions,
        conflict_diagnostics: final_replay.conflict_diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        mining_v2::{
            build_candidate_block_v2, build_coinbase_transaction_v2, CandidateBlockV2Spec,
        },
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        validation::block_subsidy,
    };

    const CHAIN_ID: &str = "task28-mining-state-root";

    fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn candidate(state: &ChainState, placeholder_root: &str) -> Block {
        let identity = activated_identity(state);
        let parent_context = derive_activated_v2_mining_parent_context(state, &identity).unwrap();
        let height = parent_context
            .parents
            .iter()
            .map(|parent| state.dag.blocks[parent].header.height.saturating_add(1))
            .max()
            .unwrap();
        let timestamp = parent_context
            .parents
            .iter()
            .map(|parent| state.dag.blocks[parent].header.timestamp)
            .max()
            .unwrap()
            .saturating_add(1);
        let difficulty =
            expected_difficulty_for_parent(state, &parent_context.selected_parent).unwrap();
        let coinbase = build_coinbase_transaction_v2(
            "pulse1task28miner",
            block_subsidy(height),
            7,
            &state.chain_id,
        )
        .unwrap();

        build_candidate_block_v2(
            CandidateBlockV2Spec {
                parents: parent_context.parents,
                timestamp,
                height,
                blue_score: parent_context.blue_score,
                difficulty,
                state_root: placeholder_root.to_string(),
            },
            vec![coinbase],
            &state.chain_id,
        )
        .unwrap()
    }

    #[test]
    fn finalization_replays_authoritative_state_without_mutating_base_state() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let before = bincode::serialize(&state).unwrap();
        let mut block = candidate(&state, "caller-placeholder");

        let finalized =
            finalize_activated_v2_mining_candidate_state(&mut block, &state, &identity).unwrap();

        assert_eq!(bincode::serialize(&state).unwrap(), before);
        assert_eq!(finalized.block_hash, block.hash);
        assert_eq!(finalized.state_root, block.header.state_root);
        assert_eq!(finalized.ordered_dag_tip, Some(block.hash.clone()));
        assert_eq!(finalized.applied_transactions, 1);
        assert_eq!(finalized.skipped_conflicting_transactions, 0);
        assert!(finalized.conflict_diagnostics.is_empty());

        let mut committed = state.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut committed, &identity).unwrap();
        let replay = rebuild_authoritative_state_v2(&committed).unwrap();
        assert_eq!(replay.diagnostics.state_root, block.header.state_root);
        assert_eq!(replay.diagnostics.ordered_dag_tip, Some(block.hash.clone()));
        assert_eq!(
            replay.diagnostics.ordered_dag_digest,
            finalized.ordered_dag_digest
        );
    }

    #[test]
    fn caller_placeholder_root_cannot_change_final_candidate() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let mut first = candidate(&state, "first-placeholder");
        let mut second = candidate(&state, "second-placeholder");
        assert_ne!(first.hash, second.hash);

        let first_context =
            finalize_activated_v2_mining_candidate_state(&mut first, &state, &identity).unwrap();
        let second_context =
            finalize_activated_v2_mining_candidate_state(&mut second, &state, &identity).unwrap();

        assert_eq!(first.hash, second.hash);
        assert_eq!(first.header.state_root, second.header.state_root);
        assert_eq!(first_context, second_context);
    }

    #[test]
    fn wrong_transaction_version_fails_closed_before_state_replay() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let mut block = candidate(&state, "placeholder");
        block.transactions[0].version = 1;

        assert!(
            finalize_activated_v2_mining_candidate_state(&mut block, &state, &identity)
                .unwrap_err()
                .to_string()
                .contains("transaction")
        );
    }

    #[test]
    fn mismatched_blue_score_fails_closed() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let mut block = candidate(&state, "placeholder");
        block.header.blue_score = block.header.blue_score.saturating_add(1);

        assert!(
            finalize_activated_v2_mining_candidate_state(&mut block, &state, &identity)
                .unwrap_err()
                .to_string()
                .contains("blue score")
        );
    }

    #[test]
    fn nonce_search_cannot_precede_state_finalization() {
        let state = init_chain_state(CHAIN_ID.to_string());
        let identity = activated_identity(&state);
        let mut block = candidate(&state, "placeholder");
        block.header.nonce = 1;

        assert!(
            finalize_activated_v2_mining_candidate_state(&mut block, &state, &identity)
                .unwrap_err()
                .to_string()
                .contains("before nonce search")
        );
    }
}
