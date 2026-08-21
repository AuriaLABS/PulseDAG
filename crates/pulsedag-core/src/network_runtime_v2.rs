use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    accept::{AcceptSource, BlockAcceptanceResult},
    errors::PulseError,
    network_block_v2::accept_activated_v2_p2p_block_atomically,
    network_staging_v2::{
        promote_activated_v2_p2p_anchor_atomically, stage_activated_v2_p2p_block,
        ActivatedV2P2pStageOutcome, ActivatedV2P2pStaging,
    },
    protocol::ProtocolActivationIdentity,
    state::ChainState,
    types::{Block, Hash},
};

pub const ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS: usize = 4_096;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ActivatedV2P2pRuntime {
    staging: ActivatedV2P2pStaging,
    pending_missing: BTreeMap<Hash, Block>,
}

impl ActivatedV2P2pRuntime {
    pub fn staging(&self) -> &ActivatedV2P2pStaging {
        &self.staging
    }

    pub fn pending_len(&self) -> usize {
        self.pending_missing.len()
    }

    pub fn pending_is_empty(&self) -> bool {
        self.pending_missing.is_empty()
    }

    pub fn pending_contains(&self, hash: &Hash) -> bool {
        self.pending_missing.contains_key(hash)
    }

    pub fn pending_hashes(&self) -> Vec<Hash> {
        self.pending_missing.keys().cloned().collect()
    }

    fn queue_missing(&mut self, block: Block) -> Result<(), PulseError> {
        if !self.pending_missing.contains_key(&block.hash)
            && self.pending_missing.len() >= ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS
        {
            return Err(PulseError::InvalidBlock(format!(
                "activated-v2 p2p pending-parent capacity {} reached",
                ACTIVATED_V2_P2P_PENDING_MAX_BLOCKS
            )));
        }
        self.pending_missing.insert(block.hash.clone(), block);
        Ok(())
    }
}

pub struct ActivatedV2P2pRuntimePersistence<FPersistRuntime, FPersistOne, FPersistBundle> {
    persist_runtime: FPersistRuntime,
    persist_one: FPersistOne,
    persist_bundle: FPersistBundle,
}

impl<FPersistRuntime, FPersistOne, FPersistBundle>
    ActivatedV2P2pRuntimePersistence<FPersistRuntime, FPersistOne, FPersistBundle>
{
    pub fn new(
        persist_runtime: FPersistRuntime,
        persist_one: FPersistOne,
        persist_bundle: FPersistBundle,
    ) -> Self {
        Self {
            persist_runtime,
            persist_one,
            persist_bundle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivatedV2P2pRuntimeOutcome {
    Accepted {
        block_hash: Hash,
        generation: u64,
    },
    Staged {
        block_hash: Hash,
        staged_count: usize,
    },
    Promoted {
        anchor_hash: Hash,
        promoted_hashes: Vec<Hash>,
        generation: u64,
    },
    MissingParents {
        block_hash: Hash,
        missing_parents: Vec<Hash>,
        pending_count: usize,
    },
    Duplicate {
        block_hash: Hash,
    },
    Rejected {
        block_hash: Hash,
        result: BlockAcceptanceResult,
    },
}

impl ActivatedV2P2pRuntimeOutcome {
    pub fn block_hash(&self) -> &Hash {
        match self {
            Self::Accepted { block_hash, .. }
            | Self::Staged { block_hash, .. }
            | Self::MissingParents { block_hash, .. }
            | Self::Duplicate { block_hash }
            | Self::Rejected { block_hash, .. } => block_hash,
            Self::Promoted { anchor_hash, .. } => anchor_hash,
        }
    }

    fn made_parent_context_available(&self) -> bool {
        matches!(
            self,
            Self::Accepted { .. }
                | Self::Staged { .. }
                | Self::Promoted { .. }
                | Self::Duplicate { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivatedV2P2pDriveResult {
    pub primary: ActivatedV2P2pRuntimeOutcome,
    pub retried: Vec<ActivatedV2P2pRuntimeOutcome>,
    pub pending_count: usize,
    pub staged_count: usize,
}

fn runtime_rejected(
    block_hash: Hash,
    result: BlockAcceptanceResult,
) -> ActivatedV2P2pRuntimeOutcome {
    ActivatedV2P2pRuntimeOutcome::Rejected { block_hash, result }
}

fn persist_runtime_only_or_rollback<FPersistRuntime>(
    state: &ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    runtime_before: ActivatedV2P2pRuntime,
    persist_runtime: &mut FPersistRuntime,
) -> Result<(), PulseError>
where
    FPersistRuntime: FnMut(&ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
{
    if let Err(error) = persist_runtime(state, runtime) {
        *runtime = runtime_before;
        return Err(error);
    }
    Ok(())
}

fn process_one_with_runtime_persistence<FPersistRuntime, FPersistOne, FPersistBundle, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    persistence: &mut ActivatedV2P2pRuntimePersistence<
        FPersistRuntime,
        FPersistOne,
        FPersistBundle,
    >,
    broadcast: &mut FBroadcast,
) -> Result<ActivatedV2P2pRuntimeOutcome, PulseError>
where
    FPersistRuntime: FnMut(&ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistOne: FnMut(&Block, &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let block_hash = block.hash.clone();
    let runtime_before = runtime.clone();
    let stage = stage_activated_v2_p2p_block(block.clone(), state, &mut runtime.staging, identity)?;

    match stage {
        ActivatedV2P2pStageOutcome::Duplicate => {
            let removed_pending = runtime.pending_missing.remove(&block_hash).is_some();
            if removed_pending {
                persist_runtime_only_or_rollback(
                    state,
                    runtime,
                    runtime_before,
                    &mut persistence.persist_runtime,
                )?;
            }
            Ok(ActivatedV2P2pRuntimeOutcome::Duplicate { block_hash })
        }
        ActivatedV2P2pStageOutcome::MissingParents {
            missing_parents, ..
        } => Ok(ActivatedV2P2pRuntimeOutcome::MissingParents {
            block_hash,
            missing_parents,
            pending_count: runtime.pending_missing.len(),
        }),
        ActivatedV2P2pStageOutcome::ImmediatelyFinalizable(_) => {
            let pending_was_present = runtime.pending_missing.contains_key(&block_hash);
            let mut runtime_after = runtime.clone();
            runtime_after.pending_missing.remove(&block_hash);

            let acceptance = match accept_activated_v2_p2p_block_atomically(
                block,
                state,
                AcceptSource::P2p,
                identity,
                |candidate, prepared| {
                    (persistence.persist_one)(candidate, prepared, &runtime_after)
                },
                |candidate| broadcast(candidate),
            ) {
                Ok(acceptance) => acceptance,
                Err(error) => {
                    if state.dag.blocks.contains_key(&block_hash) {
                        *runtime = runtime_after;
                    }
                    return Err(error);
                }
            };

            if acceptance.result.is_accepted() {
                *runtime = runtime_after;
                Ok(ActivatedV2P2pRuntimeOutcome::Accepted {
                    block_hash,
                    generation: state.chain_state_generation,
                })
            } else if matches!(acceptance.result, BlockAcceptanceResult::Duplicate) {
                *runtime = runtime_after;
                if pending_was_present {
                    persist_runtime_only_or_rollback(
                        state,
                        runtime,
                        runtime_before,
                        &mut persistence.persist_runtime,
                    )?;
                }
                Ok(ActivatedV2P2pRuntimeOutcome::Duplicate { block_hash })
            } else {
                *runtime = runtime_after;
                if pending_was_present {
                    persist_runtime_only_or_rollback(
                        state,
                        runtime,
                        runtime_before,
                        &mut persistence.persist_runtime,
                    )?;
                }
                Ok(runtime_rejected(block_hash, acceptance.result))
            }
        }
        ActivatedV2P2pStageOutcome::Staged { staged_count, .. } => {
            runtime.pending_missing.remove(&block_hash);
            persist_runtime_only_or_rollback(
                state,
                runtime,
                runtime_before,
                &mut persistence.persist_runtime,
            )?;
            Ok(ActivatedV2P2pRuntimeOutcome::Staged {
                block_hash,
                staged_count,
            })
        }
        ActivatedV2P2pStageOutcome::ReadyForPromotion {
            staged_parent_closure,
            ..
        } => {
            let mut promoted_hashes = staged_parent_closure;
            promoted_hashes.push(block_hash.clone());
            let mut runtime_after = ActivatedV2P2pRuntime {
                staging: runtime.staging.snapshot_without_hashes(&promoted_hashes),
                pending_missing: runtime.pending_missing.clone(),
            };
            runtime_after.pending_missing.remove(&block_hash);

            let promotion = match promote_activated_v2_p2p_anchor_atomically(
                &block_hash,
                state,
                &mut runtime.staging,
                identity,
                |bundle, prepared| (persistence.persist_bundle)(bundle, prepared, &runtime_after),
                |candidate| broadcast(candidate),
            ) {
                Ok(promotion) => promotion,
                Err(error) => {
                    if state.dag.blocks.contains_key(&block_hash) {
                        *runtime = runtime_after;
                    } else {
                        *runtime = runtime_before;
                    }
                    return Err(error);
                }
            };
            *runtime = runtime_after;
            Ok(ActivatedV2P2pRuntimeOutcome::Promoted {
                anchor_hash: promotion.anchor_hash,
                promoted_hashes: promotion.promoted_hashes,
                generation: promotion.generation,
            })
        }
    }
}

fn retry_pending_until_stable_with_runtime_persistence<
    FPersistRuntime,
    FPersistOne,
    FPersistBundle,
    FBroadcast,
>(
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    persistence: &mut ActivatedV2P2pRuntimePersistence<
        FPersistRuntime,
        FPersistOne,
        FPersistBundle,
    >,
    broadcast: &mut FBroadcast,
) -> Result<Vec<ActivatedV2P2pRuntimeOutcome>, PulseError>
where
    FPersistRuntime: FnMut(&ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistOne: FnMut(&Block, &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let mut outcomes = Vec::new();
    let max_passes = runtime.pending_missing.len().max(1);

    for _ in 0..max_passes {
        let pending_hashes = runtime.pending_hashes();
        if pending_hashes.is_empty() {
            break;
        }
        let mut progressed = false;

        for hash in pending_hashes {
            let Some(block) = runtime.pending_missing.get(&hash).cloned() else {
                continue;
            };
            match process_one_with_runtime_persistence(
                block,
                state,
                runtime,
                identity,
                persistence,
                broadcast,
            ) {
                Ok(ActivatedV2P2pRuntimeOutcome::MissingParents { .. }) => {}
                Ok(outcome) => {
                    runtime.pending_missing.remove(&hash);
                    outcomes.push(outcome);
                    progressed = true;
                }
                Err(error @ PulseError::StorageError(_)) => return Err(error),
                Err(error) => {
                    if state.dag.blocks.contains_key(&hash) || runtime.staging.contains(&hash) {
                        runtime.pending_missing.remove(&hash);
                        return Err(error);
                    }
                    let runtime_before_cleanup = runtime.clone();
                    runtime.pending_missing.remove(&hash);
                    persist_runtime_only_or_rollback(
                        state,
                        runtime,
                        runtime_before_cleanup,
                        &mut persistence.persist_runtime,
                    )?;
                    outcomes.push(runtime_rejected(
                        hash,
                        BlockAcceptanceResult::Rejected(error.to_string()),
                    ));
                    progressed = true;
                }
            }
        }

        if !progressed {
            break;
        }
    }

    Ok(outcomes)
}

/// Drive one activated-v2 P2P block while durably binding every transient or
/// authoritative runtime transition to the exact post-transition runtime
/// snapshot that would be restored after a restart.
///
/// `persistence.persist_runtime` is used for staging/pending-only changes that
/// do not advance authoritative chain state. The single-block and bundle
/// callbacks are invoked from inside the existing serialized chain-state commit
/// boundary and receive a runtime snapshot with the accepted/promoted hashes
/// already removed from transient queues. This lets storage commit chain state
/// and runtime in one batch without persisting a pre-transition sidecar.
pub fn drive_activated_v2_p2p_block_with_runtime_persistence<
    FPersistRuntime,
    FPersistOne,
    FPersistBundle,
    FBroadcast,
>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    mut persistence: ActivatedV2P2pRuntimePersistence<FPersistRuntime, FPersistOne, FPersistBundle>,
    mut broadcast: FBroadcast,
) -> Result<ActivatedV2P2pDriveResult, PulseError>
where
    FPersistRuntime: FnMut(&ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistOne: FnMut(&Block, &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let mut primary = process_one_with_runtime_persistence(
        block.clone(),
        state,
        runtime,
        identity,
        &mut persistence,
        &mut broadcast,
    )?;

    if let ActivatedV2P2pRuntimeOutcome::MissingParents { pending_count, .. } = &mut primary {
        let runtime_before_queue = runtime.clone();
        runtime.queue_missing(block)?;
        *pending_count = runtime.pending_missing.len();
        persist_runtime_only_or_rollback(
            state,
            runtime,
            runtime_before_queue,
            &mut persistence.persist_runtime,
        )?;
    }

    let retried = if primary.made_parent_context_available() {
        retry_pending_until_stable_with_runtime_persistence(
            state,
            runtime,
            identity,
            &mut persistence,
            &mut broadcast,
        )?
    } else {
        Vec::new()
    };

    Ok(ActivatedV2P2pDriveResult {
        primary,
        retried,
        pending_count: runtime.pending_missing.len(),
        staged_count: runtime.staging.len(),
    })
}

/// Backward-compatible in-memory runtime driver used by callers that have not
/// yet wired the durable runtime sidecar. Authoritative block persistence keeps
/// the historical callback shape; runtime-only transitions remain in memory.
pub fn drive_activated_v2_p2p_block_atomically<FPersistOne, FPersistBundle, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    mut persist_one: FPersistOne,
    mut persist_bundle: FPersistBundle,
    broadcast: FBroadcast,
) -> Result<ActivatedV2P2pDriveResult, PulseError>
where
    FPersistOne: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    drive_activated_v2_p2p_block_with_runtime_persistence(
        block,
        state,
        runtime,
        identity,
        ActivatedV2P2pRuntimePersistence::new(
            |_: &ChainState, _: &ActivatedV2P2pRuntime| Ok(()),
            |candidate: &Block, prepared: &ChainState, _: &ActivatedV2P2pRuntime| {
                persist_one(candidate, prepared)
            },
            |bundle: &[Block], prepared: &ChainState, _: &ActivatedV2P2pRuntime| {
                persist_bundle(bundle, prepared)
            },
        ),
        broadcast,
    )
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

    const CHAIN_ID: &str = "task28-p2p-v2-runtime";

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
            &format!("pulse1runtime{coinbase_nonce}"),
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
        panic!("expected the PoW-limit fixture to find a valid nonce");
    }

    fn parent_child_fixture() -> (ChainState, ProtocolActivationIdentity, Block, Block) {
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let genesis = base.dag.genesis_hash.clone();
        let parent = finalized_block(&base, &expected_identity, vec![genesis], 11);
        let parent_state =
            prepare_activated_v2_p2p_block_state(&parent, &base, &expected_identity).unwrap();
        let child = finalized_block(
            &parent_state,
            &expected_identity,
            vec![parent.hash.clone()],
            12,
        );
        (base, expected_identity, parent, child)
    }

    #[test]
    fn missing_parent_is_queued_outside_legacy_orphans() {
        let (mut live, expected_identity, _parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();
        let mut persisted_one = false;
        let mut persisted_bundle = false;
        let mut broadcast = false;

        let driven = drive_activated_v2_p2p_block_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| {
                persisted_one = true;
                Ok(())
            },
            |_, _| {
                persisted_bundle = true;
                Ok(())
            },
            |_| {
                broadcast = true;
                Ok(())
            },
        )
        .unwrap();

        assert!(matches!(
            driven.primary,
            ActivatedV2P2pRuntimeOutcome::MissingParents { .. }
        ));
        assert_eq!(runtime.pending_len(), 1);
        assert!(runtime.pending_contains(&child.hash));
        assert!(runtime.staging().is_empty());
        assert!(live.orphan_blocks.is_empty());
        assert!(live.orphan_missing_parents.is_empty());
        assert!(!persisted_one);
        assert!(!persisted_bundle);
        assert!(!broadcast);
    }

    #[test]
    fn accepted_parent_retries_and_accepts_pending_child() {
        let (mut live, expected_identity, parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();
        let mut persisted_hashes = Vec::<Hash>::new();
        let mut broadcasts = Vec::<Hash>::new();

        drive_activated_v2_p2p_block_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |block, _| {
                persisted_hashes.push(block.hash.clone());
                Ok(())
            },
            |_, _| panic!("unexpected bundle persistence for a finalizable-only path"),
            |block| {
                broadcasts.push(block.hash.clone());
                Ok(())
            },
        )
        .unwrap();
        assert!(runtime.pending_contains(&child.hash));

        let driven = drive_activated_v2_p2p_block_atomically(
            parent.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |block, _| {
                persisted_hashes.push(block.hash.clone());
                Ok(())
            },
            |_, _| panic!("unexpected bundle persistence for a finalizable-only path"),
            |block| {
                broadcasts.push(block.hash.clone());
                Ok(())
            },
        )
        .unwrap();

        assert!(matches!(
            driven.primary,
            ActivatedV2P2pRuntimeOutcome::Accepted { ref block_hash, .. }
                if block_hash == &parent.hash
        ));
        assert!(driven.retried.iter().any(|outcome| matches!(
            outcome,
            ActivatedV2P2pRuntimeOutcome::Accepted { block_hash, .. }
                if block_hash == &child.hash
        )));
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().is_empty());
        assert!(live.dag.blocks.contains_key(&parent.hash));
        assert!(live.dag.blocks.contains_key(&child.hash));
        assert!(live.orphan_blocks.is_empty());
        assert_eq!(
            persisted_hashes,
            vec![parent.hash.clone(), child.hash.clone()]
        );
        assert_eq!(broadcasts, persisted_hashes);
    }

    #[test]
    fn staged_parent_retries_pending_child_without_touching_legacy_orphans() {
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let genesis = base.dag.genesis_hash.clone();
        let main = finalized_block(&base, &expected_identity, vec![genesis.clone()], 21);
        let side = finalized_block(&base, &expected_identity, vec![genesis], 22);
        let mut live =
            prepare_activated_v2_p2p_block_state(&main, &base, &expected_identity).unwrap();
        let side_state =
            prepare_activated_v2_p2p_block_state(&side, &base, &expected_identity).unwrap();
        let child = finalized_block(&side_state, &expected_identity, vec![side.hash.clone()], 23);
        let mut runtime = ActivatedV2P2pRuntime::default();

        let queued = drive_activated_v2_p2p_block_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        assert!(matches!(
            queued.primary,
            ActivatedV2P2pRuntimeOutcome::MissingParents { .. }
        ));

        let driven = drive_activated_v2_p2p_block_atomically(
            side.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();

        assert!(matches!(
            driven.primary,
            ActivatedV2P2pRuntimeOutcome::Staged { ref block_hash, .. }
                if block_hash == &side.hash
        ));
        assert!(driven.retried.iter().any(|outcome| matches!(
            outcome,
            ActivatedV2P2pRuntimeOutcome::Staged { block_hash, .. }
                if block_hash == &child.hash
        )));
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().contains(&side.hash));
        assert!(runtime.staging().contains(&child.hash));
        assert!(!live.dag.blocks.contains_key(&side.hash));
        assert!(!live.dag.blocks.contains_key(&child.hash));
        assert!(live.orphan_blocks.is_empty());
        assert!(live.orphan_missing_parents.is_empty());
    }

    #[test]
    fn promotion_persistence_failure_rolls_back_new_anchor_for_retry() {
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let genesis = base.dag.genesis_hash.clone();
        let main = finalized_block(&base, &expected_identity, vec![genesis.clone()], 31);
        let side = finalized_block(&base, &expected_identity, vec![genesis], 32);
        let mut live =
            prepare_activated_v2_p2p_block_state(&main, &base, &expected_identity).unwrap();
        let mut runtime = ActivatedV2P2pRuntime::default();

        let staged = drive_activated_v2_p2p_block_atomically(
            side.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| panic!("unexpected single-block persistence for side-tip staging"),
            |_, _| panic!("unexpected bundle persistence for side-tip staging"),
            |_| panic!("unexpected broadcast for side-tip staging"),
        )
        .unwrap();
        assert!(matches!(
            staged.primary,
            ActivatedV2P2pRuntimeOutcome::Staged { .. }
        ));

        let mut pre_anchor = live.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(&side, &mut pre_anchor, &expected_identity)
            .unwrap();
        let anchor = finalized_block(
            &pre_anchor,
            &expected_identity,
            vec![main.hash.clone(), side.hash.clone()],
            33,
        );
        let live_before = bincode::serialize(&live).unwrap();

        let error = drive_activated_v2_p2p_block_atomically(
            anchor.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| panic!("unexpected single-block persistence for merge-anchor promotion"),
            |_, _| {
                Err(PulseError::StorageError(
                    "fixture promotion persistence failure".into(),
                ))
            },
            |_| panic!("unexpected broadcast before promotion persistence succeeds"),
        )
        .unwrap_err();

        assert!(matches!(error, PulseError::StorageError(_)));
        assert_eq!(bincode::serialize(&live).unwrap(), live_before);
        assert!(runtime.staging().contains(&side.hash));
        assert!(!runtime.staging().contains(&anchor.hash));

        let mut persisted_bundle = false;
        let mut broadcasts = Vec::<Hash>::new();
        let retried = drive_activated_v2_p2p_block_atomically(
            anchor.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| panic!("unexpected single-block persistence for merge-anchor promotion"),
            |bundle, _| {
                assert_eq!(
                    bundle
                        .iter()
                        .map(|block| block.hash.clone())
                        .collect::<Vec<_>>(),
                    vec![side.hash.clone(), anchor.hash.clone()]
                );
                persisted_bundle = true;
                Ok(())
            },
            |block| {
                broadcasts.push(block.hash.clone());
                Ok(())
            },
        )
        .unwrap();

        assert!(persisted_bundle);
        assert!(matches!(
            retried.primary,
            ActivatedV2P2pRuntimeOutcome::Promoted { ref anchor_hash, .. }
                if anchor_hash == &anchor.hash
        ));
        assert_eq!(broadcasts, vec![side.hash.clone(), anchor.hash.clone()]);
        assert!(runtime.staging().is_empty());
        assert!(live.dag.blocks.contains_key(&side.hash));
        assert!(live.dag.blocks.contains_key(&anchor.hash));
    }

    #[test]
    fn parent_persistence_failure_keeps_pending_child_for_later_retry() {
        let (mut live, expected_identity, parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();

        drive_activated_v2_p2p_block_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap();
        let live_before = bincode::serialize(&live).unwrap();

        let error = drive_activated_v2_p2p_block_atomically(
            parent.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| {
                Err(PulseError::StorageError(
                    "fixture persistence failure".into(),
                ))
            },
            |_, _| Ok(()),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(matches!(error, PulseError::StorageError(_)));
        assert!(runtime.pending_contains(&child.hash));
        assert!(!live.dag.blocks.contains_key(&parent.hash));
        assert!(!live.dag.blocks.contains_key(&child.hash));
        assert_eq!(bincode::serialize(&live).unwrap(), live_before);
        assert!(live.orphan_blocks.is_empty());
    }

    #[test]
    fn durable_runtime_callback_observes_queued_missing_parent() {
        let (mut live, expected_identity, _parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();
        let mut snapshots = Vec::<(usize, usize)>::new();

        let driven = drive_activated_v2_p2p_block_with_runtime_persistence(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            ActivatedV2P2pRuntimePersistence::new(
                |_, durable_runtime| {
                    snapshots.push((
                        durable_runtime.pending_len(),
                        durable_runtime.staging().len(),
                    ));
                    Ok(())
                },
                |_, _, _| panic!("missing-parent queue must not persist an accepted block"),
                |_, _, _| panic!("missing-parent queue must not persist a promoted bundle"),
            ),
            |_| panic!("missing-parent queue must not broadcast"),
        )
        .unwrap();

        assert!(matches!(
            driven.primary,
            ActivatedV2P2pRuntimeOutcome::MissingParents { .. }
        ));
        assert_eq!(snapshots, vec![(1, 0)]);
        assert!(runtime.pending_contains(&child.hash));
    }

    #[test]
    fn durable_single_block_callbacks_receive_post_transition_runtime() {
        let (mut live, expected_identity, parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();

        drive_activated_v2_p2p_block_with_runtime_persistence(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            ActivatedV2P2pRuntimePersistence::new(
                |_, _| Ok(()),
                |_, _, _| panic!("missing-parent queue must not persist an accepted block"),
                |_, _, _| panic!("missing-parent queue must not persist a promoted bundle"),
            ),
            |_| Ok(()),
        )
        .unwrap();

        let mut persisted = Vec::<(Hash, usize)>::new();
        let driven = drive_activated_v2_p2p_block_with_runtime_persistence(
            parent.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            ActivatedV2P2pRuntimePersistence::new(
                |_, _| Ok(()),
                |block, _, durable_runtime| {
                    persisted.push((block.hash.clone(), durable_runtime.pending_len()));
                    Ok(())
                },
                |_, _, _| panic!("finalizable parent/child path must not persist a bundle"),
            ),
            |_| Ok(()),
        )
        .unwrap();

        assert!(matches!(
            driven.primary,
            ActivatedV2P2pRuntimeOutcome::Accepted { ref block_hash, .. }
                if block_hash == &parent.hash
        ));
        assert_eq!(
            persisted,
            vec![(parent.hash.clone(), 1), (child.hash.clone(), 0)]
        );
        assert!(runtime.pending_is_empty());
    }

    #[test]
    fn durable_bundle_callback_receives_post_promotion_runtime() {
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let expected_identity = identity(&base);
        let genesis = base.dag.genesis_hash.clone();
        let main = finalized_block(&base, &expected_identity, vec![genesis.clone()], 41);
        let side = finalized_block(&base, &expected_identity, vec![genesis], 42);
        let mut live =
            prepare_activated_v2_p2p_block_state(&main, &base, &expected_identity).unwrap();
        let mut runtime = ActivatedV2P2pRuntime::default();

        drive_activated_v2_p2p_block_with_runtime_persistence(
            side.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            ActivatedV2P2pRuntimePersistence::new(
                |_, durable_runtime| {
                    assert!(durable_runtime.staging().contains(&side.hash));
                    Ok(())
                },
                |_, _, _| panic!("side-tip staging must not persist an accepted block"),
                |_, _, _| panic!("side-tip staging must not persist a bundle"),
            ),
            |_| panic!("side-tip staging must not broadcast"),
        )
        .unwrap();

        let mut pre_anchor = live.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(&side, &mut pre_anchor, &expected_identity)
            .unwrap();
        let anchor = finalized_block(
            &pre_anchor,
            &expected_identity,
            vec![main.hash.clone(), side.hash.clone()],
            43,
        );
        let mut observed_post_promotion = false;

        let driven = drive_activated_v2_p2p_block_with_runtime_persistence(
            anchor.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            ActivatedV2P2pRuntimePersistence::new(
                |_, _| Ok(()),
                |_, _, _| panic!("merge-anchor promotion must not use single-block persistence"),
                |bundle, _, durable_runtime| {
                    assert_eq!(
                        bundle
                            .iter()
                            .map(|block| block.hash.clone())
                            .collect::<Vec<_>>(),
                        vec![side.hash.clone(), anchor.hash.clone()]
                    );
                    assert!(durable_runtime.staging().is_empty());
                    assert!(durable_runtime.pending_is_empty());
                    observed_post_promotion = true;
                    Ok(())
                },
            ),
            |_| Ok(()),
        )
        .unwrap();

        assert!(observed_post_promotion);
        assert!(matches!(
            driven.primary,
            ActivatedV2P2pRuntimeOutcome::Promoted { ref anchor_hash, .. }
                if anchor_hash == &anchor.hash
        ));
        assert!(runtime.staging().is_empty());
    }

    #[test]
    fn runtime_only_persistence_failure_rolls_back_missing_parent_queue() {
        let (mut live, expected_identity, _parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();
        let live_before = bincode::serialize(&live).unwrap();

        let error = drive_activated_v2_p2p_block_with_runtime_persistence(
            child,
            &mut live,
            &mut runtime,
            &expected_identity,
            ActivatedV2P2pRuntimePersistence::new(
                |_, _| {
                    Err(PulseError::StorageError(
                        "fixture runtime sidecar persistence failure".into(),
                    ))
                },
                |_, _, _| Ok(()),
                |_, _, _| Ok(()),
            ),
            |_| Ok(()),
        )
        .unwrap_err();

        assert!(matches!(error, PulseError::StorageError(_)));
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().is_empty());
        assert_eq!(bincode::serialize(&live).unwrap(), live_before);
    }
}
