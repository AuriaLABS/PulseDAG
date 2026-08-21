use pulsedag_core::{
    build_candidate_block_v2, build_coinbase_transaction_v2, canonicalize_block_parents_v2,
    commit_ghostdag_v1_metadata_for_activated_v2, compute_block_hash_v2, current_ts,
    drive_activated_v2_p2p_block_with_runtime_persistence, materialize_authoritative_state_v2,
    prepare_activated_v2_p2p_block_state, rebuild_authoritative_state_v2,
    validate_pow_for_protocol, ActivatedV2P2pRuntime, Block, CandidateBlockV2Spec, ChainState, Hash,
    ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_storage::Storage;

const CHAIN_ID: &str = "task28-storage-p2p-runtime-restart";

fn temp_db_path(test_name: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("pulsedag-task28-p2p-runtime-{test_name}-{unique}"))
        .to_string_lossy()
        .into_owned()
}

fn activated_base() -> (ChainState, ProtocolActivationIdentity) {
    let mut state = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    let genesis = state.dag.genesis_hash.clone();
    state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
    state.dag.merge_set_reds.insert(genesis, vec![]);
    let state = materialize_authoritative_state_v2(&state).unwrap();
    let identity = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    (state, identity)
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
        &format!("pulse1restart{coinbase_nonce}"),
        pulsedag_core::block_subsidy(height),
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
    let classification = pulsedag_core::ghostdag_v1::classify_merge_set_v1(&block, state).unwrap();
    block.header.blue_score = classification.blue_score;
    block.header.difficulty = pulsedag_core::retarget::expected_difficulty_for_parent(
        state,
        classification.selected_parent.as_ref().unwrap(),
    )
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

#[test]
fn pending_parent_runtime_restores_and_retries_to_empty_after_restart() {
    let path = temp_db_path("pending-retry");
    let (mut live, identity) = activated_base();
    let genesis = live.dag.genesis_hash.clone();
    let parent = finalized_block(&live, &identity, vec![genesis], 11);
    let parent_state = prepare_activated_v2_p2p_block_state(&parent, &live, &identity).unwrap();
    let child = finalized_block(
        &parent_state,
        &identity,
        vec![parent.hash.clone()],
        12,
    );
    let mut runtime = ActivatedV2P2pRuntime::default();

    {
        let storage = Storage::open(&path).unwrap();
        storage
            .persist_activated_v2_p2p_runtime_snapshot(&identity, &live, &runtime)
            .unwrap();

        drive_activated_v2_p2p_block_with_runtime_persistence(
            child.clone(),
            &mut live,
            &mut runtime,
            &identity,
            |state, durable_runtime| {
                storage.persist_activated_v2_p2p_runtime_snapshot(
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |block, state, durable_runtime| {
                storage.persist_activated_v2_p2p_block_and_runtime(
                    block,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |blocks, state, durable_runtime| {
                storage.persist_activated_v2_p2p_blocks_and_runtime(
                    blocks,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |_| Ok(()),
        )
        .unwrap();
        assert!(runtime.pending_contains(&child.hash));
    }

    {
        let storage = Storage::open(&path).unwrap();
        let (restored_state, restored_runtime) = storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .unwrap();
        live = restored_state;
        runtime = restored_runtime;
        assert!(runtime.pending_contains(&child.hash));
        assert!(runtime.staging().is_empty());

        let driven = drive_activated_v2_p2p_block_with_runtime_persistence(
            parent.clone(),
            &mut live,
            &mut runtime,
            &identity,
            |state, durable_runtime| {
                storage.persist_activated_v2_p2p_runtime_snapshot(
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |block, state, durable_runtime| {
                storage.persist_activated_v2_p2p_block_and_runtime(
                    block,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |blocks, state, durable_runtime| {
                storage.persist_activated_v2_p2p_blocks_and_runtime(
                    blocks,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(driven.retried.iter().any(|outcome| {
            matches!(
                outcome,
                pulsedag_core::ActivatedV2P2pRuntimeOutcome::Accepted { block_hash, .. }
                    if block_hash == &child.hash
            )
        }));
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().is_empty());
        assert!(live.dag.blocks.contains_key(&parent.hash));
        assert!(live.dag.blocks.contains_key(&child.hash));
    }

    {
        let storage = Storage::open(&path).unwrap();
        let (restored_state, restored_runtime) = storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .unwrap();
        assert!(restored_runtime.pending_is_empty());
        assert!(restored_runtime.staging().is_empty());
        assert!(restored_state.dag.blocks.contains_key(&parent.hash));
        assert!(restored_state.dag.blocks.contains_key(&child.hash));
    }

    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn staged_side_tip_restores_and_promotes_atomically_after_restart() {
    let path = temp_db_path("staged-promotion");
    let (mut live, identity) = activated_base();
    let genesis = live.dag.genesis_hash.clone();
    let main = finalized_block(&live, &identity, vec![genesis.clone()], 21);
    let side = finalized_block(&live, &identity, vec![genesis], 22);
    let mut runtime = ActivatedV2P2pRuntime::default();
    let promoted_anchor_hash;

    {
        let storage = Storage::open(&path).unwrap();
        storage
            .persist_activated_v2_p2p_runtime_snapshot(&identity, &live, &runtime)
            .unwrap();

        drive_activated_v2_p2p_block_with_runtime_persistence(
            main.clone(),
            &mut live,
            &mut runtime,
            &identity,
            |state, durable_runtime| {
                storage.persist_activated_v2_p2p_runtime_snapshot(
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |block, state, durable_runtime| {
                storage.persist_activated_v2_p2p_block_and_runtime(
                    block,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |blocks, state, durable_runtime| {
                storage.persist_activated_v2_p2p_blocks_and_runtime(
                    blocks,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |_| Ok(()),
        )
        .unwrap();

        drive_activated_v2_p2p_block_with_runtime_persistence(
            side.clone(),
            &mut live,
            &mut runtime,
            &identity,
            |state, durable_runtime| {
                storage.persist_activated_v2_p2p_runtime_snapshot(
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |block, state, durable_runtime| {
                storage.persist_activated_v2_p2p_block_and_runtime(
                    block,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |blocks, state, durable_runtime| {
                storage.persist_activated_v2_p2p_blocks_and_runtime(
                    blocks,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |_| Ok(()),
        )
        .unwrap();
        assert!(runtime.staging().contains(&side.hash));
    }

    {
        let storage = Storage::open(&path).unwrap();
        let (restored_state, restored_runtime) = storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .unwrap();
        live = restored_state;
        runtime = restored_runtime;
        assert!(runtime.staging().contains(&side.hash));

        let mut pre_anchor = live.clone();
        commit_ghostdag_v1_metadata_for_activated_v2(&side, &mut pre_anchor, &identity).unwrap();
        let anchor = finalized_block(
            &pre_anchor,
            &identity,
            vec![main.hash.clone(), side.hash.clone()],
            23,
        );
        promoted_anchor_hash = anchor.hash.clone();

        drive_activated_v2_p2p_block_with_runtime_persistence(
            anchor.clone(),
            &mut live,
            &mut runtime,
            &identity,
            |state, durable_runtime| {
                storage.persist_activated_v2_p2p_runtime_snapshot(
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |block, state, durable_runtime| {
                storage.persist_activated_v2_p2p_block_and_runtime(
                    block,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |blocks, state, durable_runtime| {
                storage.persist_activated_v2_p2p_blocks_and_runtime(
                    blocks,
                    &identity,
                    state,
                    durable_runtime,
                )
            },
            |_| Ok(()),
        )
        .unwrap();

        assert!(runtime.staging().is_empty());
        assert!(runtime.pending_is_empty());
        assert!(live.dag.blocks.contains_key(&side.hash));
        assert!(live.dag.blocks.contains_key(&anchor.hash));
    }

    {
        let storage = Storage::open(&path).unwrap();
        let (restored_state, restored_runtime) = storage
            .load_activated_v2_p2p_runtime_snapshot(&identity)
            .unwrap();
        assert!(restored_runtime.staging().is_empty());
        assert!(restored_runtime.pending_is_empty());
        assert!(restored_state.dag.blocks.contains_key(&side.hash));
        assert!(restored_state.dag.blocks.contains_key(&promoted_anchor_hash));
        assert_eq!(
            restored_state.dag.ordered_dag_tip.as_ref(),
            Some(&promoted_anchor_hash)
        );
    }

    let _ = std::fs::remove_dir_all(path);
}
