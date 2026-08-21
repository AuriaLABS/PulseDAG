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

fn runtime_rejected(block_hash: Hash, result: BlockAcceptanceResult) -> ActivatedV2P2pRuntimeOutcome {
    ActivatedV2P2pRuntimeOutcome::Rejected { block_hash, result }
}

fn process_one<FPersistOne, FPersistBundle, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    queue_missing: bool,
    persist_one: &mut FPersistOne,
    persist_bundle: &mut FPersistBundle,
    broadcast: &mut FBroadcast,
) -> Result<ActivatedV2P2pRuntimeOutcome, PulseError>
where
    FPersistOne: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let block_hash = block.hash.clone();
    let stage = stage_activated_v2_p2p_block(
        block.clone(),
        state,
        &mut runtime.staging,
        identity,
    )?;

    match stage {
        ActivatedV2P2pStageOutcome::Duplicate => {
            runtime.pending_missing.remove(&block_hash);
            Ok(ActivatedV2P2pRuntimeOutcome::Duplicate { block_hash })
        }
        ActivatedV2P2pStageOutcome::MissingParents {
            missing_parents, ..
        } => {
            if queue_missing {
                runtime.queue_missing(block)?;
            }
            Ok(ActivatedV2P2pRuntimeOutcome::MissingParents {
                block_hash,
                missing_parents,
                pending_count: runtime.pending_missing.len(),
            })
        }
        ActivatedV2P2pStageOutcome::ImmediatelyFinalizable(_) => {
            let acceptance = accept_activated_v2_p2p_block_atomically(
                block,
                state,
                AcceptSource::P2p,
                identity,
                |candidate, prepared| persist_one(candidate, prepared),
                |candidate| broadcast(candidate),
            )?;
            if acceptance.result.is_accepted() {
                runtime.pending_missing.remove(&block_hash);
                Ok(ActivatedV2P2pRuntimeOutcome::Accepted {
                    block_hash,
                    generation: state.chain_state_generation,
                })
            } else if matches!(acceptance.result, BlockAcceptanceResult::Duplicate) {
                runtime.pending_missing.remove(&block_hash);
                Ok(ActivatedV2P2pRuntimeOutcome::Duplicate { block_hash })
            } else {
                runtime.pending_missing.remove(&block_hash);
                Ok(runtime_rejected(block_hash, acceptance.result))
            }
        }
        ActivatedV2P2pStageOutcome::Staged { staged_count, .. } => {
            runtime.pending_missing.remove(&block_hash);
            Ok(ActivatedV2P2pRuntimeOutcome::Staged {
                block_hash,
                staged_count,
            })
        }
        ActivatedV2P2pStageOutcome::ReadyForPromotion { .. } => {
            let promotion = promote_activated_v2_p2p_anchor_atomically(
                &block_hash,
                state,
                &mut runtime.staging,
                identity,
                |bundle, prepared| persist_bundle(bundle, prepared),
                |candidate| broadcast(candidate),
            )?;
            runtime.pending_missing.remove(&block_hash);
            Ok(ActivatedV2P2pRuntimeOutcome::Promoted {
                anchor_hash: promotion.anchor_hash,
                promoted_hashes: promotion.promoted_hashes,
                generation: promotion.generation,
            })
        }
    }
}

fn retry_pending_until_stable<FPersistOne, FPersistBundle, FBroadcast>(
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    persist_one: &mut FPersistOne,
    persist_bundle: &mut FPersistBundle,
    broadcast: &mut FBroadcast,
) -> Result<Vec<ActivatedV2P2pRuntimeOutcome>, PulseError>
where
    FPersistOne: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState) -> Result<(), PulseError>,
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
            match process_one(
                block,
                state,
                runtime,
                identity,
                false,
                persist_one,
                persist_bundle,
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
                    runtime.pending_missing.remove(&hash);
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

/// Drive one activated-v2 P2P block through the finalizable/staged/missing-parent
/// boundary and then retry any bounded pending blocks whose parent context may
/// have become available.
///
/// Missing-parent blocks are retained only in this protocol-specific runtime
/// queue. They are never inserted into `ChainState::orphan_blocks`, so the
/// legacy orphan reprocessor cannot accidentally retry them through v1 block
/// validation.
pub fn drive_activated_v2_p2p_block_atomically<FPersistOne, FPersistBundle, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    mut persist_one: FPersistOne,
    mut persist_bundle: FPersistBundle,
    mut broadcast: FBroadcast,
) -> Result<ActivatedV2P2pDriveResult, PulseError>
where
    FPersistOne: FnMut(&Block, &ChainState) -> Result<(), PulseError>,
    FPersistBundle: FnMut(&[Block], &ChainState) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let primary = process_one(
        block,
        state,
        runtime,
        identity,
        true,
        &mut persist_one,
        &mut persist_bundle,
        &mut broadcast,
    )?;

    let retried = if primary.made_parent_context_available() {
        retry_pending_until_stable(
            state,
            runtime,
            identity,
            &mut persist_one,
            &mut persist_bundle,
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
        panic!("expected PoW-limit fixture to find a valid nonce");
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
        let mut persisted = false;
        let mut broadcast = false;

        let driven = drive_activated_v2_p2p_block_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected_identity,
            |_, _| {
                persisted = true;
                Ok(())
            },
            |_, _| {
                persisted = true;
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
        assert!(!persisted);
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
            |bundle, _| {
                persisted_hashes.extend(bundle.iter().map(|block| block.hash.clone()));
                Ok(())
            },
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
            |bundle, _| {
                persisted_hashes.extend(bundle.iter().map(|block| block.hash.clone()));
                Ok(())
            },
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
        assert_eq!(persisted_hashes, vec![parent.hash.clone(), child.hash.clone()]);
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
        let child = finalized_block(
            &side_state,
            &expected_identity,
            vec![side.hash.clone()],
            23,
        );
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
            |_, _| Err(PulseError::StorageError("fixture persistence failure".into())),
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
}
