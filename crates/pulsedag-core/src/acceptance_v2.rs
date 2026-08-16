use crate::{
    errors::PulseError,
    ghostdag_v1::{classify_merge_set_v1, GhostdagV1Classification, GHOSTDAG_V1_K},
    header_v2::{compute_block_hash_v2, validate_block_header_v2_shape},
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    protocol::{ProtocolActivationIdentity, ProtocolConsensusMode, BLOCK_HEADER_VERSION_V2},
    selection_v2::{calculate_selected_tip_v1, rebuild_selected_chain_v1},
    state::ChainState,
    tx::TRANSACTION_VERSION_V2,
    types::{Block, Hash},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedV2MetadataCommit {
    pub classification: GhostdagV1Classification,
    pub selected_tip: Option<Hash>,
    pub selected_chain: Vec<Hash>,
}

fn invalid_activation_identity(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "invalid activated-v2 metadata identity: {}",
        message.into()
    ))
}

fn verify_activated_v2_metadata_identity(
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    identity.validate().map_err(invalid_activation_identity)?;
    if identity.chain_id != state.chain_id {
        return Err(PulseError::ChainIdMismatch);
    }
    if identity.genesis_hash != state.dag.genesis_hash {
        return Err(invalid_activation_identity(
            "genesis hash does not match chain state",
        ));
    }
    if identity.transaction_protocol_version != TRANSACTION_VERSION_V2
        || identity.block_header_protocol_version != BLOCK_HEADER_VERSION_V2
        || identity.consensus_mode != ProtocolConsensusMode::GhostdagV1
        || identity.dag_ordering_version != GHOSTDAG_V1_ORDERING_VERSION
    {
        return Err(invalid_activation_identity(
            "identity is not the frozen ghostdag_v1 transaction/header/ordering tuple",
        ));
    }
    Ok(())
}

fn validate_metadata_candidate(
    block: &Block,
    state: &ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<GhostdagV1Classification, PulseError> {
    if state.dag.blocks.contains_key(&block.hash) {
        return Err(PulseError::BlockAlreadyExists);
    }

    validate_block_header_v2_shape(&block.header, &identity.chain_id)?;
    if block.header.parents.is_empty() {
        return Err(PulseError::InvalidBlock(
            "activated-v2 metadata candidate cannot introduce a parentless root after genesis"
                .to_string(),
        ));
    }
    let computed_hash = compute_block_hash_v2(&block.header, &identity.chain_id)?;
    if computed_hash != block.hash {
        return Err(PulseError::InvalidBlock(format!(
            "activated-v2 block hash mismatch: supplied {}, computed {}",
            block.hash, computed_hash
        )));
    }

    let mut expected_height = 0_u64;
    for parent in &block.header.parents {
        let parent_block = state
            .dag
            .blocks
            .get(parent)
            .ok_or_else(|| PulseError::InvalidBlock(format!("missing parent {parent}")))?;
        expected_height = expected_height.max(parent_block.header.height.saturating_add(1));
    }
    if block.header.height != expected_height {
        return Err(PulseError::InvalidBlock(format!(
            "activated-v2 invalid height {}, expected {}",
            block.header.height, expected_height
        )));
    }

    let classification = classify_merge_set_v1(block, state).map_err(|error| {
        PulseError::InvalidBlock(format!(
            "ghostdag_v1 metadata classification failed: {error:?}"
        ))
    })?;
    if block.header.blue_score != classification.blue_score {
        return Err(PulseError::InvalidBlock(format!(
            "activated-v2 blue score mismatch: header {}, derived {}",
            block.header.blue_score, classification.blue_score
        )));
    }
    Ok(classification)
}

/// Commit deterministic GHOSTDAG-v1 DAG metadata for a block whose activated-v2
/// block validity has been established by the caller.
///
/// This is deliberately not wired into live RPC/P2P/mining acceptance and does
/// not validate transaction economics, UTXO state roots, timestamps, difficulty
/// or proof of work. It provides the Task 25 metadata-commit boundary that a
/// later activation-gated full v2 block acceptance path can call after those
/// checks succeed.
///
/// All fallible work is performed on a cloned state. The caller-visible state
/// is replaced only after selected-parent, blue/red classification, cumulative
/// blue work, selected tip and selected-chain projection are complete.
pub fn commit_ghostdag_v1_metadata_for_activated_v2(
    block: &Block,
    state: &mut ChainState,
    identity: &ProtocolActivationIdentity,
) -> Result<ActivatedV2MetadataCommit, PulseError> {
    verify_activated_v2_metadata_identity(state, identity)?;
    let classification = validate_metadata_candidate(block, state, identity)?;

    let mut working = state.clone();
    for parent in &block.header.parents {
        working.dag.tips.remove(parent);
        let children = working.dag.children.entry(parent.clone()).or_default();
        children.push(block.hash.clone());
        children.sort();
        children.dedup();
    }
    working.dag.tips.insert(block.hash.clone());
    working.dag.best_height = working.dag.best_height.max(block.header.height);
    working
        .dag
        .selected_parents
        .insert(block.hash.clone(), classification.selected_parent.clone());
    working
        .dag
        .merge_set_blues
        .insert(block.hash.clone(), classification.blues.clone());
    working
        .dag
        .merge_set_reds
        .insert(block.hash.clone(), classification.reds.clone());
    working
        .dag
        .blue_work
        .insert(block.hash.clone(), classification.blue_work);
    working.dag.merge_set_k = GHOSTDAG_V1_K;
    working.dag.blocks.insert(block.hash.clone(), block.clone());

    let selected_tip = calculate_selected_tip_v1(&working).map_err(|error| {
        PulseError::InvalidBlock(format!(
            "ghostdag_v1 selected-tip calculation failed after metadata commit: {error:?}"
        ))
    })?;
    let selected_chain =
        rebuild_selected_chain_v1(&working, selected_tip.clone()).map_err(|error| {
            PulseError::InvalidBlock(format!(
                "ghostdag_v1 selected-chain rebuild failed after metadata commit: {error:?}"
            ))
        })?;
    working.dag.selected_chain = selected_chain.clone();

    *state = working;
    Ok(ActivatedV2MetadataCommit {
        classification,
        selected_tip,
        selected_chain,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis::init_chain_state,
        header_v2::canonicalize_block_parents_v2,
        protocol::ProtocolActivationIdentity,
        types::{BlockHeader, Transaction},
    };

    const CHAIN_ID: &str = "task25-activated-v2-metadata";

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn candidate_for_state(
        state: &ChainState,
        parents: Vec<Hash>,
        nonce: u64,
        score_override: Option<u64>,
    ) -> Block {
        let parents = canonicalize_block_parents_v2(&parents).unwrap();
        let height = parents
            .iter()
            .map(|parent| state.dag.blocks[parent].header.height.saturating_add(1))
            .max()
            .unwrap();
        let mut block = Block {
            hash: String::new(),
            header: BlockHeader {
                version: BLOCK_HEADER_VERSION_V2,
                parents,
                timestamp: 1_900_000_000_u64.saturating_add(height),
                difficulty: 1,
                nonce,
                merkle_root: format!("merkle-{nonce}"),
                state_root: format!("state-{nonce}"),
                blue_score: 0,
                height,
            },
            transactions: Vec::<Transaction>::new(),
        };
        let classification = classify_merge_set_v1(&block, state).unwrap();
        block.header.blue_score = score_override.unwrap_or(classification.blue_score);
        block.hash = compute_block_hash_v2(&block.header, CHAIN_ID).unwrap();
        block
    }

    #[test]
    fn activated_v2_metadata_commit_publishes_selection_only_after_complete_derivation() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let genesis = state.dag.genesis_hash.clone();
        let expected_identity = identity(&state);
        let runtime_mode = state.dag.consensus_mode;
        let ordering_version = state.dag.ordering_version.clone();
        let ordered_dag = state.dag.ordered_dag.clone();
        let utxo_root = state.utxo.compute_state_root().unwrap();
        let block = candidate_for_state(&state, vec![genesis.clone()], 1, None);

        let committed =
            commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut state, &expected_identity)
                .unwrap();

        assert_eq!(
            committed.classification.selected_parent,
            Some(genesis.clone())
        );
        assert_eq!(committed.classification.blue_score, 1);
        assert_eq!(committed.classification.blue_work, 1);
        assert_eq!(committed.selected_tip, Some(block.hash.clone()));
        assert_eq!(committed.selected_chain, vec![genesis, block.hash.clone()]);
        assert_eq!(state.dag.selected_chain, committed.selected_chain);
        assert_eq!(
            state.dag.selected_parents.get(&block.hash),
            Some(&committed.classification.selected_parent)
        );
        assert_eq!(state.dag.blue_work.get(&block.hash), Some(&1));
        assert_eq!(state.dag.merge_set_k, GHOSTDAG_V1_K);
        assert_eq!(state.dag.consensus_mode, runtime_mode);
        assert_eq!(state.dag.ordering_version, ordering_version);
        assert_eq!(state.dag.ordered_dag, ordered_dag);
        assert_eq!(state.utxo.compute_state_root().unwrap(), utxo_root);
    }

    #[test]
    fn red_merge_member_is_metadata_not_block_invalidity() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let genesis = state.dag.genesis_hash.clone();
        let mut parallel = Vec::new();
        for nonce in 10..14 {
            let block = candidate_for_state(&state, vec![genesis.clone()], nonce, None);
            commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut state, &expected_identity)
                .unwrap();
            parallel.push(block.hash);
        }

        let merge = candidate_for_state(&state, parallel, 20, None);
        let committed =
            commit_ghostdag_v1_metadata_for_activated_v2(&merge, &mut state, &expected_identity)
                .unwrap();

        assert_eq!(committed.classification.blues.len(), 2);
        assert_eq!(committed.classification.reds.len(), 1);
        assert!(state.dag.blocks.contains_key(&merge.hash));
        assert_eq!(
            state.dag.merge_set_reds.get(&merge.hash),
            Some(&committed.classification.reds)
        );
    }

    #[test]
    fn wrong_blue_score_fails_atomically() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let before = bincode::serialize(&state).unwrap();
        let genesis = state.dag.genesis_hash.clone();
        let block = candidate_for_state(&state, vec![genesis], 2, Some(99));

        assert!(matches!(
            commit_ghostdag_v1_metadata_for_activated_v2(
                &block,
                &mut state,
                &expected_identity,
            ),
            Err(PulseError::InvalidBlock(message)) if message.contains("blue score mismatch")
        ));
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn legacy_or_mismatched_identity_cannot_publish_v2_metadata() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let genesis = state.dag.genesis_hash.clone();
        let block = candidate_for_state(&state, vec![genesis], 3, None);
        let before = bincode::serialize(&state).unwrap();

        let legacy = ProtocolActivationIdentity::legacy_from_state(&state);
        assert!(commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut state, &legacy).is_err());
        assert_eq!(bincode::serialize(&state).unwrap(), before);

        let mut wrong_ordering = identity(&state);
        wrong_ordering.dag_ordering_version.push_str("-wrong");
        assert!(
            commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut state, &wrong_ordering,)
                .is_err()
        );
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }

    #[test]
    fn parentless_second_root_fails_atomically() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let mut block = Block {
            hash: String::new(),
            header: BlockHeader {
                version: BLOCK_HEADER_VERSION_V2,
                parents: Vec::new(),
                timestamp: 1_900_000_001,
                difficulty: 1,
                nonce: 5,
                merkle_root: "merkle-second-root".to_string(),
                state_root: "state-second-root".to_string(),
                blue_score: 1,
                height: 0,
            },
            transactions: Vec::new(),
        };
        block.hash = compute_block_hash_v2(&block.header, CHAIN_ID).unwrap();
        let before = bincode::serialize(&state).unwrap();

        assert!(matches!(
            commit_ghostdag_v1_metadata_for_activated_v2(
                &block,
                &mut state,
                &expected_identity,
            ),
            Err(PulseError::InvalidBlock(message)) if message.contains("parentless root")
        ));
        assert_eq!(bincode::serialize(&state).unwrap(), before);
        assert!(!state.dag.blocks.contains_key(&block.hash));
        assert_eq!(
            state.dag.selected_chain,
            vec![state.dag.genesis_hash.clone()]
        );
    }

    #[test]
    fn missing_parent_does_not_finalize_any_metadata() {
        let mut state = init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&state);
        let mut block = Block {
            hash: String::new(),
            header: BlockHeader {
                version: BLOCK_HEADER_VERSION_V2,
                parents: vec!["ff".repeat(32)],
                timestamp: 1_900_000_001,
                difficulty: 1,
                nonce: 4,
                merkle_root: "merkle-missing".to_string(),
                state_root: "state-missing".to_string(),
                blue_score: 1,
                height: 1,
            },
            transactions: Vec::new(),
        };
        block.hash = compute_block_hash_v2(&block.header, CHAIN_ID).unwrap();
        let before = bincode::serialize(&state).unwrap();

        assert!(matches!(
            commit_ghostdag_v1_metadata_for_activated_v2(
                &block,
                &mut state,
                &expected_identity,
            ),
            Err(PulseError::InvalidBlock(message)) if message.contains("missing parent")
        ));
        assert_eq!(bincode::serialize(&state).unwrap(), before);
        assert!(!state.dag.selected_parents.contains_key(&block.hash));
        assert!(!state.dag.blue_work.contains_key(&block.hash));
    }
}
