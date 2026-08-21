use crate::{
    drive_activated_v2_p2p_block_atomically, errors::PulseError, ActivatedV2P2pDriveResult,
    ActivatedV2P2pRuntime, Block, ChainState, ProtocolActivationIdentity,
};

/// Drive one activated-v2 P2P block against isolated candidate state/runtime,
/// durably persist the final authoritative snapshot and transient runtime as one
/// boundary, and only then publish the candidate state/runtime and broadcasts.
///
/// The existing runtime driver persists each authoritative mutation before it
/// performs the corresponding in-memory pending/staging cleanup. Reusing those
/// callbacks directly for a durable runtime sidecar would therefore persist a
/// transient snapshot that is one mutation behind. This wrapper keeps the
/// validated driver semantics intact while moving the durable boundary around
/// the complete drive:
///
/// 1. clone live chain state and activated-v2 runtime;
/// 2. run the existing driver on those clones with no external persistence or
///    broadcast side effects;
/// 3. persist every accepted block together with the *final* candidate chain
///    state and runtime through one caller-supplied atomic storage callback;
/// 4. publish candidate state/runtime to memory;
/// 5. broadcast exactly the blocks the candidate drive committed.
///
/// A storage failure leaves both live state and live runtime unchanged. This is
/// intentionally non-activating: callers must still provide the exact verified
/// activated-v2 protocol identity before entering this boundary.
pub fn drive_activated_v2_p2p_block_with_runtime_persistence_atomically<FPersist, FBroadcast>(
    block: Block,
    state: &mut ChainState,
    runtime: &mut ActivatedV2P2pRuntime,
    identity: &ProtocolActivationIdentity,
    mut persist: FPersist,
    mut broadcast: FBroadcast,
) -> Result<ActivatedV2P2pDriveResult, PulseError>
where
    FPersist: FnMut(&[Block], &ChainState, &ActivatedV2P2pRuntime) -> Result<(), PulseError>,
    FBroadcast: FnMut(&Block) -> Result<(), PulseError>,
{
    let mut candidate_state = state.clone();
    let mut candidate_runtime = runtime.clone();
    let mut committed_blocks = Vec::<Block>::new();

    let driven = drive_activated_v2_p2p_block_atomically(
        block,
        &mut candidate_state,
        &mut candidate_runtime,
        identity,
        |_, _| Ok(()),
        |_, _| Ok(()),
        |candidate| {
            committed_blocks.push(candidate.clone());
            Ok(())
        },
    )?;

    persist(&committed_blocks, &candidate_state, &candidate_runtime)?;

    *state = candidate_state;
    *runtime = candidate_runtime;

    for candidate in &committed_blocks {
        broadcast(candidate)?;
    }

    Ok(driven)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        build_activated_v2_mining_template, compute_block_hash_v2, current_ts,
        mining_template_v2::ActivatedV2MiningTemplateSpec,
        prepare_activated_v2_p2p_block_state, validate_pow_for_protocol,
        ActivatedV2P2pRuntimeOutcome, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task28-p2p-v2-runtime-persistence";

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
        miner_address: &str,
    ) -> Block {
        let template = build_activated_v2_mining_template(
            state,
            identity,
            ActivatedV2MiningTemplateSpec {
                miner_address: miner_address.to_string(),
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

    fn parent_child_fixture() -> (ChainState, ProtocolActivationIdentity, Block, Block) {
        let base = crate::genesis::init_chain_state(CHAIN_ID.to_string());
        let expected = identity(&base);
        let parent = mined_block(&base, &expected, 41, "pulse1runtimepersistparent");
        let parent_state =
            prepare_activated_v2_p2p_block_state(&parent, &base, &expected).unwrap();
        let child = mined_block(
            &parent_state,
            &expected,
            42,
            "pulse1runtimepersistchild",
        );
        (base, expected, parent, child)
    }

    #[test]
    fn persistence_callback_observes_final_runtime_after_pending_retry() {
        let (mut live, expected, parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();
        let queued_generation = live.chain_state_generation;

        let queued = drive_activated_v2_p2p_block_with_runtime_persistence_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected,
            |blocks, prepared, post_runtime| {
                assert!(blocks.is_empty());
                assert_eq!(prepared.chain_state_generation, queued_generation);
                assert_eq!(post_runtime.pending_len(), 1);
                assert!(post_runtime.pending_contains(&child.hash));
                Ok(())
            },
            |_| panic!("missing-parent queue must not broadcast"),
        )
        .unwrap();
        assert!(matches!(
            queued.primary,
            ActivatedV2P2pRuntimeOutcome::MissingParents { .. }
        ));
        assert!(runtime.pending_contains(&child.hash));

        let mut persisted_hashes = Vec::new();
        let mut broadcasts = Vec::new();
        let driven = drive_activated_v2_p2p_block_with_runtime_persistence_atomically(
            parent.clone(),
            &mut live,
            &mut runtime,
            &expected,
            |blocks, prepared, post_runtime| {
                persisted_hashes = blocks.iter().map(|block| block.hash.clone()).collect();
                assert_eq!(
                    persisted_hashes,
                    vec![parent.hash.clone(), child.hash.clone()]
                );
                assert!(post_runtime.pending_is_empty());
                assert!(post_runtime.staging().is_empty());
                assert!(prepared.dag.blocks.contains_key(&parent.hash));
                assert!(prepared.dag.blocks.contains_key(&child.hash));
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
        assert!(live.dag.blocks.contains_key(&parent.hash));
        assert!(live.dag.blocks.contains_key(&child.hash));
        assert_eq!(broadcasts, persisted_hashes);
    }

    #[test]
    fn persistence_failure_rolls_back_live_state_runtime_and_broadcasts() {
        let (mut live, expected, parent, child) = parent_child_fixture();
        let mut runtime = ActivatedV2P2pRuntime::default();

        drive_activated_v2_p2p_block_with_runtime_persistence_atomically(
            child.clone(),
            &mut live,
            &mut runtime,
            &expected,
            |blocks, _, post_runtime| {
                assert!(blocks.is_empty());
                assert!(post_runtime.pending_contains(&child.hash));
                Ok(())
            },
            |_| Ok(()),
        )
        .unwrap();

        let state_before = bincode::serialize(&live).unwrap();
        let runtime_before = bincode::serialize(&runtime).unwrap();
        let generation_before_failure = live.chain_state_generation;
        let mut broadcast_count = 0usize;
        let error = drive_activated_v2_p2p_block_with_runtime_persistence_atomically(
            parent,
            &mut live,
            &mut runtime,
            &expected,
            |blocks, prepared, post_runtime| {
                assert_eq!(blocks.len(), 2);
                assert!(post_runtime.pending_is_empty());
                assert!(prepared.chain_state_generation > generation_before_failure);
                Err(PulseError::StorageError(
                    "fixture final runtime persistence failure".into(),
                ))
            },
            |_| {
                broadcast_count = broadcast_count.saturating_add(1);
                Ok(())
            },
        )
        .unwrap_err();

        assert!(matches!(error, PulseError::StorageError(_)));
        assert_eq!(bincode::serialize(&live).unwrap(), state_before);
        assert_eq!(bincode::serialize(&runtime).unwrap(), runtime_before);
        assert!(runtime.pending_contains(&child.hash));
        assert_eq!(broadcast_count, 0);
    }
}
