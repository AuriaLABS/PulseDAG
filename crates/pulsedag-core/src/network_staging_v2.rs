use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use serde::{Deserialize, Serialize};

use crate::{
    accept::mutate_chain_state_serialized,
    acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
    errors::PulseError,
    ghostdag_v1::GHOSTDAG_V1_MAX_ANCESTOR_VISITS,
    mempool_protocol::reconcile_mempool_for_protocol,
    network_context_v2::{
        validate_activated_v2_p2p_block_context, ActivatedV2P2pContextDisposition,
        ActivatedV2P2pContextValidation,
    },
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    state_replay_v2::materialize_authoritative_state_v2,
    types::{Block, Hash},
};

pub const ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS: usize = 4_096;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivatedV2P2pStaging {
    blocks: BTreeMap<Hash, Block>,
}

impl ActivatedV2P2pStaging {
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn contains(&self, hash: &Hash) -> bool {
        self.blocks.contains_key(hash)
    }

    pub fn get(&self, hash: &Hash) -> Option<&Block> {
        self.blocks.get(hash)
    }

    pub fn hashes(&self) -> Vec<Hash> {
        self.blocks.keys().cloned().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivatedV2P2pStageOutcome {
    Duplicate,
    MissingParents {
        block_hash: Hash,
        missing_parents: Vec<Hash>,
    },
    ImmediatelyFinalizable(ActivatedV2P2pContextValidation),
    Staged {
        validation: ActivatedV2P2pContextValidation,
        staged_parent_closure: Vec<Hash>,
        staged_count: usize,
    },
    ReadyForPromotion {
        validation: ActivatedV2P2pContextValidation,
        staged_parent_closure: Vec<Hash>,
        staged_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedV2P2pPromotion {
    pub anchor_hash: Hash,
    pub promoted_hashes: Vec<Hash>,
    pub generation: u64,
    pub persisted: bool,
    pub committed: bool,
    pub broadcast_count: usize,
}

#[derive(Debug, Clone)]
struct StagedClosure {
    ordered: Vec<Hash>,
    missing: Vec<Hash>,
}

#[derive(Debug, Clone)]
struct PreparedPromotion {
    bundle: Vec<Block>,
    staged_hashes: Vec<Hash>,
}

fn invalid_staging(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("activated-v2 p2p staging: {}", message.into()))
}

fn collect_staged_closure(
    roots: &[Hash],
    state: &ChainState,
    staging: &ActivatedV2P2pStaging,
) -> Result<StagedClosure, PulseError> {
    let mut closure = BTreeSet::<Hash>::new();
    let mut missing = BTreeSet::<Hash>::new();
    let mut pending = roots.iter().cloned().collect::<BTreeSet<_>>();
    let mut visits = 0usize;

    while let Some(hash) = pending.pop_first() {
        if state.dag.blocks.contains_key(&hash) || closure.contains(&hash) {
            continue;
        }
        let Some(block) = staging.blocks.get(&hash) else {
            missing.insert(hash);
            continue;
        };
        if visits >= GHOSTDAG_V1_MAX_ANCESTOR_VISITS {
            return Err(invalid_staging(format!(
                "staged-parent closure exceeds bounded ancestor visit limit {}",
                GHOSTDAG_V1_MAX_ANCESTOR_VISITS
            )));
        }
        visits = visits.saturating_add(1);
        closure.insert(hash);
        for parent in &block.header.parents {
            if !state.dag.blocks.contains_key(parent) && !closure.contains(parent) {
                pending.insert(parent.clone());
            }
        }
    }

    if !missing.is_empty() {
        return Ok(StagedClosure {
            ordered: Vec::new(),
            missing: missing.into_iter().collect(),
        });
    }

    let mut indegree = BTreeMap::<Hash, usize>::new();
    let mut children = BTreeMap::<Hash, BTreeSet<Hash>>::new();
    for hash in &closure {
        indegree.insert(hash.clone(), 0);
    }
    for hash in &closure {
        let block = staging
            .blocks
            .get(hash)
            .ok_or_else(|| invalid_staging(format!("staged block {hash} disappeared")))?;
        for parent in &block.header.parents {
            if closure.contains(parent) {
                *indegree
                    .get_mut(hash)
                    .ok_or_else(|| invalid_staging("staged closure indegree missing"))? += 1;
                children
                    .entry(parent.clone())
                    .or_default()
                    .insert(hash.clone());
            }
        }
    }

    let mut ready = indegree
        .iter()
        .filter_map(|(hash, degree)| (*degree == 0).then_some(hash.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(closure.len());
    while let Some(hash) = ready.pop_first() {
        ordered.push(hash.clone());
        if let Some(block_children) = children.get(&hash) {
            for child in block_children {
                let degree = indegree
                    .get_mut(child)
                    .ok_or_else(|| invalid_staging("staged child indegree missing"))?;
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    ready.insert(child.clone());
                }
            }
        }
    }
    if ordered.len() != closure.len() {
        return Err(invalid_staging("staged-parent closure contains a cycle"));
    }

    Ok(StagedClosure {
        ordered,
        missing: Vec::new(),
    })
}

fn augment_with_staged_parents(
    block: &Block,
    state: &ChainState,
    staging: &ActivatedV2P2pStaging,
    identity: &ProtocolActivationIdentity,
) -> Result<(ChainState, StagedClosure), PulseError> {
    let closure = collect_staged_closure(&block.header.parents, state, staging)?;
    if !closure.missing.is_empty() {
        return Ok((state.clone(), closure));
    }

    let mut working = state.clone();
    for hash in &closure.ordered {
        let staged = staging
            .blocks
            .get(hash)
            .ok_or_else(|| invalid_staging(format!("staged block {hash} disappeared")))?;
        validate_activated_v2_p2p_block_context(staged, &working, identity)?;
        commit_ghostdag_v1_metadata_for_activated_v2(staged, &mut working, identity)?;
    }
    Ok((working, closure))
}

pub fn stage_activated_v2_p2p_block(
    block: Block,
    state: &ChainState,
    staging: &mut ActivatedV2P2pStaging,
    identity: &ProtocolActivationIdentity,
) -> Result<ActivatedV2P2pStageOutcome, PulseError> {
    if state.dag.blocks.contains_key(&block.hash) || staging.blocks.contains_key(&block.hash) {
        return Ok(ActivatedV2P2pStageOutcome::Duplicate);
    }

    let (augmented, closure) = augment_with_staged_parents(&block, state, staging, identity)?;
    if !closure.missing.is_empty() {
        return Ok(ActivatedV2P2pStageOutcome::MissingParents {
            block_hash: block.hash,
            missing_parents: closure.missing,
        });
    }

    let validation = validate_activated_v2_p2p_block_context(&block, &augmented, identity)?;
    if closure.ordered.is_empty()
        && validation.disposition == ActivatedV2P2pContextDisposition::ImmediatelyFinalizable
    {
        return Ok(ActivatedV2P2pStageOutcome::ImmediatelyFinalizable(
            validation,
        ));
    }
    if staging.blocks.len() >= ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS {
        return Err(invalid_staging(format!(
            "capacity {} reached",
            ACTIVATED_V2_P2P_STAGING_MAX_BLOCKS
        )));
    }

    let ready_for_promotion = !closure.ordered.is_empty()
        && validation.disposition == ActivatedV2P2pContextDisposition::ImmediatelyFinalizable;
    staging.blocks.insert(block.hash.clone(), block);
    let staged_count = staging.blocks.len();
    if ready_for_promotion {
        Ok(ActivatedV2P2pStageOutcome::ReadyForPromotion {
            validation,
            staged_parent_closure: closure.ordered,
            staged_count,
        })
    } else {
        Ok(ActivatedV2P2pStageOutcome::Staged {
            validation,
            staged_parent_closure: closure.ordered,
            staged_count,
        })
    }
}

fn remove_promoted_transactions_from_mempool(
    state: &mut ChainState,
    blocks: &[Block],
    identity: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    for block in blocks {
        for transaction in block.transactions.iter().skip(1) {
            if state
                .mempool
                .transactions
                .remove(&transaction.txid)
                .is_some()
            {
                state.mempool.first_seen.remove(&transaction.txid);
                state.mempool.counters.confirmed_removed_total = state
                    .mempool
                    .counters
                    .confirmed_removed_total
                    .saturating_add(1);
            }
            for input in &transaction.inputs {
                state.mempool.spent_outpoints.remove(&input.previous_output);
            }
        }
    }
    reconcile_mempool_for_protocol(state, identity)?;
    Ok(())
}

fn prepare_anchor_promotion(
    anchor_hash: &Hash,
    state: &ChainState,
    staging: &ActivatedV2P2pStaging,
    identity: &ProtocolActivationIdentity,
) -> Result<(ChainState, PreparedPromotion), PulseError> {
    if state.dag.blocks.contains_key(anchor_hash) {
        return Err(PulseError::BlockAlreadyExists);
    }
    let anchor = staging
        .blocks
        .get(anchor_hash)
        .ok_or_else(|| PulseError::NotFound(format!("staged anchor {anchor_hash}")))?;
    let closure = collect_staged_closure(&anchor.header.parents, state, staging)?;
    if !closure.missing.is_empty() {
        return Err(invalid_staging(format!(
            "anchor {} still has missing parents: {}",
            anchor_hash,
            closure.missing.join(",")
        )));
    }

    let mut ordered_hashes = closure.ordered;
    ordered_hashes.push(anchor_hash.clone());
    let mut working = state.clone();
    let mut bundle = Vec::with_capacity(ordered_hashes.len());

    for hash in &ordered_hashes {
        let block = staging
            .blocks
            .get(hash)
            .ok_or_else(|| invalid_staging(format!("staged block {hash} disappeared")))?;
        validate_activated_v2_p2p_block_context(block, &working, identity)?;
        commit_ghostdag_v1_metadata_for_activated_v2(block, &mut working, identity)?;
        bundle.push(block.clone());
    }

    let mut materialized = materialize_authoritative_state_v2(&working).map_err(|error| {
        invalid_staging(format!(
            "anchor {anchor_hash} does not yet close the authoritative DAG: {error}"
        ))
    })?;
    if materialized.dag.ordered_dag_tip.as_ref() != Some(anchor_hash) {
        return Err(invalid_staging(format!(
            "anchor {anchor_hash} is not the authoritative ordered DAG tip {:?}",
            materialized.dag.ordered_dag_tip
        )));
    }
    let observed_state_root = materialized.utxo.compute_state_root()?;
    if observed_state_root != anchor.header.state_root {
        return Err(invalid_staging(format!(
            "anchor {anchor_hash} state root mismatch: committed {}, replay produced {}",
            anchor.header.state_root, observed_state_root
        )));
    }
    remove_promoted_transactions_from_mempool(&mut materialized, &bundle, identity)?;

    Ok((
        materialized,
        PreparedPromotion {
            staged_hashes: ordered_hashes,
            bundle,
        },
    ))
}

pub fn promote_activated_v2_p2p_anchor_atomically<FPersist, FBroadcast>(
    anchor_hash: &Hash,
    state: &mut ChainState,
    staging: &mut ActivatedV2P2pStaging,
    identity: &ProtocolActivationIdentity,
    mut persist: FPersist,
    mut broadcast: FBroadcast,
) -> Result<ActivatedV2P2pPromotion, PulseError>
where
    FPersist: FnMut(&[Block], &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let persist_bundle = RefCell::new(Vec::<Block>::new());
    let mutation = mutate_chain_state_serialized(
        state,
        "p2p_v2_staged_promotion",
        |base| {
            let (prepared, details) =
                prepare_anchor_promotion(anchor_hash, base, staging, identity)?;
            *persist_bundle.borrow_mut() = details.bundle.clone();
            Ok((prepared, details))
        },
        |prepared| {
            let bundle = persist_bundle.borrow();
            persist(bundle.as_slice(), prepared)
        },
    )?;

    let details = mutation.result;
    for hash in &details.staged_hashes {
        staging.blocks.remove(hash);
    }

    let mut broadcast_count = 0usize;
    for block in &details.bundle {
        broadcast(block)?;
        broadcast_count = broadcast_count.saturating_add(1);
    }

    Ok(ActivatedV2P2pPromotion {
        anchor_hash: anchor_hash.clone(),
        promoted_hashes: details
            .bundle
            .iter()
            .map(|block| block.hash.clone())
            .collect(),
        generation: mutation.generation,
        persisted: true,
        committed: true,
        broadcast_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        acceptance_v2::commit_ghostdag_v1_metadata_for_activated_v2,
        ghostdag_v1::classify_merge_set_v1,
        header_v2::{canonicalize_block_parents_v2, compute_block_hash_v2},
        mining::current_ts,
        mining_v2::{
            build_candidate_block_v2, build_coinbase_transaction_v2, CandidateBlockV2Spec,
        },
        network_block_v2::prepare_activated_v2_p2p_block_state,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        pow_protocol::validate_pow_for_protocol,
        retarget::expected_difficulty_for_parent,
        state_replay_v2::rebuild_authoritative_state_v2,
        validation::block_subsidy,
    };

    const CHAIN_ID: &str = "task28-p2p-v2-staging";

    fn identity(state: &ChainState) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    fn finalized_block(
        state: &ChainState,
        identity: &ProtocolActivationIdentity,
        parents: Vec<Hash>,
        coinbase_nonce: u64,
    ) -> Block {
        let parents = canonicalize_block_parents_v2(&parents).unwrap();
        let height = parents
            .iter()
            .map(|parent| state.dag.blocks[parent].header.height.saturating_add(1))
            .max()
            .unwrap();
        let timestamp = parents
            .iter()
            .map(|parent| state.dag.blocks[parent].header.timestamp)
            .max()
            .unwrap()
            .max(current_ts().saturating_sub(1));
        let coinbase = build_coinbase_transaction_v2(
            &format!("pulse1stage{coinbase_nonce}"),
            block_subsidy(height),
            coinbase_nonce,
            &identity.chain_id,
        )
        .unwrap();
        let mut block = build_candidate_block_v2(
            CandidateBlockV2Spec {
                parents,
                timestamp,
                height,
                blue_score: 0,
                difficulty: 1,
                state_root: "00".repeat(32),
            },
            vec![coinbase],
            &identity.chain_id,
        )
        .unwrap();
        let classification = classify_merge_set_v1(&block, state).unwrap();
        block.header.blue_score = classification.blue_score;
        block.header.difficulty =
            expected_difficulty_for_parent(state, classification.selected_parent.as_ref().unwrap())
                .unwrap();
        block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();

        let mut first_state = state.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut first_state, identity).unwrap();
        let first = rebuild_authoritative_state_v2(&first_state).unwrap();
        block.header.state_root = first.diagnostics.state_root;
        block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();

        let mut final_state = state.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(&block, &mut final_state, identity).unwrap();
        let final_replay = rebuild_authoritative_state_v2(&final_state).unwrap();
        assert_eq!(final_replay.diagnostics.state_root, block.header.state_root);
        assert_eq!(
            final_replay.diagnostics.ordered_dag_tip,
            Some(block.hash.clone())
        );

        for nonce in 0..=200_000_u64 {
            block.header.nonce = nonce;
            block.hash = compute_block_hash_v2(&block.header, &identity.chain_id).unwrap();
            if validate_pow_for_protocol(&block.header, state, identity).is_ok() {
                return block;
            }
        }
        panic!("expected PoW-limit fixture to find a valid nonce");
    }

    fn staged_side_fixture() -> (
        ChainState,
        ProtocolActivationIdentity,
        ActivatedV2P2pStaging,
        Block,
        Block,
    ) {
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let genesis = base.dag.genesis_hash.clone();
        let main = finalized_block(&base, &expected_identity, vec![genesis.clone()], 11);
        let side = finalized_block(&base, &expected_identity, vec![genesis], 12);
        assert_ne!(main.hash, side.hash);
        let live = prepare_activated_v2_p2p_block_state(&main, &base, &expected_identity).unwrap();
        let mut staging = ActivatedV2P2pStaging::default();
        let outcome =
            stage_activated_v2_p2p_block(side.clone(), &live, &mut staging, &expected_identity)
                .unwrap();
        assert!(matches!(outcome, ActivatedV2P2pStageOutcome::Staged { .. }));
        (live, expected_identity, staging, main, side)
    }

    #[test]
    fn side_tip_is_staged_without_mutating_authoritative_state() {
        let (live, expected_identity, staging, _main, side) = staged_side_fixture();
        let before = bincode::serialize(&live).unwrap();
        assert!(staging.contains(&side.hash));
        assert_eq!(staging.len(), 1);
        assert_eq!(bincode::serialize(&live).unwrap(), before);
        assert_eq!(expected_identity.chain_id, live.chain_id);
    }

    #[test]
    fn child_of_staged_parent_validates_through_bounded_staged_closure() {
        let (live, expected_identity, mut staging, _main, side) = staged_side_fixture();
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let side_state =
            prepare_activated_v2_p2p_block_state(&side, &base, &expected_identity).unwrap();
        let child = finalized_block(&side_state, &expected_identity, vec![side.hash.clone()], 21);

        let outcome =
            stage_activated_v2_p2p_block(child.clone(), &live, &mut staging, &expected_identity)
                .unwrap();
        match outcome {
            ActivatedV2P2pStageOutcome::Staged {
                staged_parent_closure,
                ..
            } => assert_eq!(staged_parent_closure, vec![side.hash.clone()]),
            other => panic!("expected staged child, got {other:?}"),
        }
        assert!(staging.contains(&child.hash));
    }

    fn stage_merge_anchor(
        live: &ChainState,
        identity: &ProtocolActivationIdentity,
        staging: &mut ActivatedV2P2pStaging,
        main: &Block,
        side: &Block,
    ) -> Block {
        let mut pre_anchor = live.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(side, &mut pre_anchor, identity).unwrap();
        let anchor = finalized_block(
            &pre_anchor,
            identity,
            vec![main.hash.clone(), side.hash.clone()],
            31,
        );
        let outcome =
            stage_activated_v2_p2p_block(anchor.clone(), live, staging, identity).unwrap();
        assert!(matches!(
            outcome,
            ActivatedV2P2pStageOutcome::ReadyForPromotion { .. }
        ));
        anchor
    }

    #[test]
    fn merge_anchor_promotes_staged_side_tip_and_state_atomically() {
        let (mut live, expected_identity, mut staging, main, side) = staged_side_fixture();
        let anchor = stage_merge_anchor(&live, &expected_identity, &mut staging, &main, &side);
        let mut persisted = false;
        let mut broadcasts = Vec::new();

        let promoted = promote_activated_v2_p2p_anchor_atomically(
            &anchor.hash,
            &mut live,
            &mut staging,
            &expected_identity,
            |bundle, prepared| {
                assert_eq!(bundle.len(), 2);
                assert_eq!(bundle[0].hash, side.hash);
                assert_eq!(bundle[1].hash, anchor.hash);
                assert!(prepared.dag.blocks.contains_key(&side.hash));
                assert!(prepared.dag.blocks.contains_key(&anchor.hash));
                persisted = true;
                Ok(())
            },
            |block| {
                broadcasts.push(block.hash.clone());
                Ok(())
            },
        )
        .unwrap();

        assert!(persisted);
        assert!(promoted.persisted && promoted.committed);
        assert_eq!(
            promoted.promoted_hashes,
            vec![side.hash.clone(), anchor.hash.clone()]
        );
        assert_eq!(broadcasts, promoted.promoted_hashes);
        assert_eq!(promoted.broadcast_count, 2);
        assert!(staging.is_empty());
        assert!(live.dag.blocks.contains_key(&side.hash));
        assert!(live.dag.blocks.contains_key(&anchor.hash));
        assert_eq!(live.dag.ordered_dag_tip.as_ref(), Some(&anchor.hash));
    }

    #[test]
    fn promotion_persistence_failure_leaves_live_state_and_staging_unchanged() {
        let (mut live, expected_identity, mut staging, main, side) = staged_side_fixture();
        let anchor = stage_merge_anchor(&live, &expected_identity, &mut staging, &main, &side);
        let live_before = bincode::serialize(&live).unwrap();
        let staging_before = bincode::serialize(&staging).unwrap();
        let mut broadcast = false;

        let error = promote_activated_v2_p2p_anchor_atomically(
            &anchor.hash,
            &mut live,
            &mut staging,
            &expected_identity,
            |_, _| {
                Err(PulseError::StorageError(
                    "fixture persistence failure".into(),
                ))
            },
            |_| {
                broadcast = true;
                Ok(())
            },
        )
        .unwrap_err();

        assert!(matches!(error, PulseError::StorageError(_)));
        assert!(!broadcast);
        assert_eq!(bincode::serialize(&live).unwrap(), live_before);
        assert_eq!(bincode::serialize(&staging).unwrap(), staging_before);
    }
}
