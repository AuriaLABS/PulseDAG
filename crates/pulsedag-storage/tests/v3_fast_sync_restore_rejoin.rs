use std::collections::BTreeMap;

use pulsedag_core::{
    build_candidate_block_v2, build_coinbase_transaction_v2, canonicalize_block_parents_v2,
    commit_ghostdag_v1_metadata_for_activated_v2, compute_block_hash_v2, current_ts,
    drive_activated_v2_p2p_block_with_runtime_persistence, materialize_authoritative_state_v2,
    rebuild_authoritative_state_v2, state_digest, validate_pow_for_protocol,
    ActivatedV2P2pRuntime, ActivatedV2P2pRuntimePersistence, Block, CandidateBlockV2Spec,
    ChainState, Hash, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_core::snapshot_transfer::snapshot_transfer_commitment_set_digest_v1;
use pulsedag_storage::{
    FastSyncNetworkTransferPlanV1, Storage, FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION,
};

const CHAIN_ID: &str = "v3-fast-sync-restore-rejoin";

macro_rules! runtime_persistence {
    ($storage:ident, $identity:ident) => {
        ActivatedV2P2pRuntimePersistence::new(
            |state: &ChainState, durable_runtime: &ActivatedV2P2pRuntime| {
                $storage.persist_activated_v2_p2p_runtime_snapshot(
                    &$identity,
                    state,
                    durable_runtime,
                )
            },
            |block: &Block, state: &ChainState, durable_runtime: &ActivatedV2P2pRuntime| {
                $storage.persist_activated_v2_p2p_block_and_runtime(
                    block,
                    &$identity,
                    state,
                    durable_runtime,
                )
            },
            |blocks: &[Block], state: &ChainState, durable_runtime: &ActivatedV2P2pRuntime| {
                $storage.persist_activated_v2_p2p_blocks_and_runtime(
                    blocks,
                    &$identity,
                    state,
                    durable_runtime,
                )
            },
        )
    };
}

fn temp_db_path(test_name: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("pulsedag-v3-fast-sync-{test_name}-{unique}"))
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

fn seed_persisted_genesis(storage: &Storage, live: &ChainState) {
    let stored = storage.load_or_init_genesis(CHAIN_ID.to_string()).unwrap();
    assert_eq!(stored.chain_id, live.chain_id);
    assert_eq!(stored.dag.genesis_hash, live.dag.genesis_hash);
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
        &format!("pulse1fastsyncrejoin{coinbase_nonce}"),
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

fn network_plan(
    prepared: &pulsedag_storage::PreparedFastSyncSnapshotTransferV1,
) -> FastSyncNetworkTransferPlanV1 {
    let transfer = &prepared.plan;
    let manifest = &transfer.snapshot_manifest;
    FastSyncNetworkTransferPlanV1 {
        plan_version: FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION,
        payload_encoding: transfer.payload_encoding.clone(),
        chain_id: manifest.chain_id.clone(),
        genesis_hash: manifest.genesis_hash.clone(),
        protocol_fingerprint: manifest.protocol_fingerprint.clone(),
        manifest_version: manifest.manifest_version,
        protocol_snapshot_bundle_format_version: manifest.protocol_snapshot_bundle_format_version,
        storage_schema_version: manifest.storage_schema_version,
        transfer_id: transfer.transfer_id.clone(),
        commitment_set_id: snapshot_transfer_commitment_set_digest_v1(
            &transfer.transfer_id,
            &transfer.chunk_commitments,
        ),
        payload_len: transfer.payload_len,
        chunk_size: transfer.chunk_size,
        chunk_count: transfer.chunk_count,
        chunk_commitments: transfer.chunk_commitments.clone(),
        best_height: manifest.best_height,
        selected_tip: manifest.selected_tip.clone(),
        state_commitment: manifest.state_commitment.clone(),
        prune_boundary_height: manifest.prune_boundary_height,
        snapshot_generation: manifest.snapshot_generation,
        accepted_storage_generation: manifest.accepted_storage_generation,
        delta_start_generation: manifest.delta_start_generation,
        delta_end_generation: manifest.delta_end_generation,
    }
}

#[test]
fn fast_sync_import_restores_rejoins_and_replays_new_blocks_across_restarts() {
    let source_path = temp_db_path("source");
    let target_path = temp_db_path("target");
    let source = Storage::open(&source_path).unwrap();
    let (mut source_state, identity) = activated_base();
    let mut source_runtime = ActivatedV2P2pRuntime::default();
    seed_persisted_genesis(&source, &source_state);
    source
        .persist_activated_v2_p2p_runtime_snapshot(&identity, &source_state, &source_runtime)
        .unwrap();

    let genesis = source_state.dag.genesis_hash.clone();
    let first = finalized_block(&source_state, &identity, vec![genesis], 101);
    drive_activated_v2_p2p_block_with_runtime_persistence(
        first.clone(),
        &mut source_state,
        &mut source_runtime,
        &identity,
        runtime_persistence!(source, identity),
        |_| Ok(()),
    )
    .unwrap();
    let second = finalized_block(&source_state, &identity, vec![first.hash.clone()], 102);
    drive_activated_v2_p2p_block_with_runtime_persistence(
        second.clone(),
        &mut source_state,
        &mut source_runtime,
        &identity,
        runtime_persistence!(source, identity),
        |_| Ok(()),
    )
    .unwrap();
    assert!(source_runtime.pending_is_empty());
    assert!(source_runtime.staging().is_empty());
    assert_eq!(source_state.dag.ordered_dag_tip.as_ref(), Some(&second.hash));

    let (bundle, source_report) = source
        .export_fast_sync_snapshot_bundle_v1(&identity)
        .unwrap();
    assert!(source_report.restore_guarantees_explicit);
    let snapshot_commitment = bundle.manifest.state_commitment.clone();
    let snapshot_height = bundle.manifest.best_height;
    let prepared = source
        .prepare_fast_sync_snapshot_transfer_v1(&bundle, &identity, 256)
        .unwrap()
        .0;
    let plan = network_plan(&prepared);
    let chunks = (0..prepared.plan.chunk_count)
        .map(|chunk_index| (chunk_index, prepared.chunk(chunk_index).unwrap().to_vec()))
        .collect::<BTreeMap<_, _>>();

    let target = Storage::open(&target_path).unwrap();
    let (import_report, imported_state, imported_runtime) = target
        .import_complete_fast_sync_bootstrap_v1(&plan, &chunks, &identity)
        .unwrap();
    assert!(import_report.restore_guarantees_explicit);
    assert_eq!(imported_state.dag.best_height, snapshot_height);
    assert_eq!(imported_state.dag.ordered_dag_tip.as_ref(), Some(&second.hash));
    assert_eq!(state_digest(&imported_state).unwrap(), snapshot_commitment);
    assert!(imported_runtime.pending_is_empty());
    assert!(imported_runtime.staging().is_empty());
    drop(target);

    let target = Storage::open(&target_path).unwrap();
    let (mut live, mut runtime) = target
        .load_activated_v2_p2p_runtime_snapshot(&identity)
        .unwrap();
    assert_eq!(state_digest(&live).unwrap(), snapshot_commitment);
    let third = finalized_block(&live, &identity, vec![second.hash.clone()], 103);
    drive_activated_v2_p2p_block_with_runtime_persistence(
        third.clone(),
        &mut live,
        &mut runtime,
        &identity,
        runtime_persistence!(target, identity),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(live.dag.ordered_dag_tip.as_ref(), Some(&third.hash));
    assert_eq!(live.dag.best_height, snapshot_height + 1);
    assert!(runtime.pending_is_empty());
    assert!(runtime.staging().is_empty());
    let after_first_rejoin_digest = state_digest(&live).unwrap();
    assert_ne!(after_first_rejoin_digest, snapshot_commitment);
    drop(target);

    let target = Storage::open(&target_path).unwrap();
    let (mut rejoined_state, mut rejoined_runtime) = target
        .load_activated_v2_p2p_runtime_snapshot(&identity)
        .unwrap();
    assert_eq!(state_digest(&rejoined_state).unwrap(), after_first_rejoin_digest);
    assert_eq!(rejoined_state.dag.ordered_dag_tip.as_ref(), Some(&third.hash));
    assert!(rejoined_runtime.pending_is_empty());
    assert!(rejoined_runtime.staging().is_empty());

    let fourth = finalized_block(&rejoined_state, &identity, vec![third.hash.clone()], 104);
    drive_activated_v2_p2p_block_with_runtime_persistence(
        fourth.clone(),
        &mut rejoined_state,
        &mut rejoined_runtime,
        &identity,
        runtime_persistence!(target, identity),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(rejoined_state.dag.ordered_dag_tip.as_ref(), Some(&fourth.hash));
    assert_eq!(rejoined_state.dag.best_height, snapshot_height + 2);
    let final_digest = state_digest(&rejoined_state).unwrap();
    assert!(rejoined_runtime.pending_is_empty());
    assert!(rejoined_runtime.staging().is_empty());
    drop(target);

    let target = Storage::open(&target_path).unwrap();
    let (final_state, final_runtime) = target
        .load_activated_v2_p2p_runtime_snapshot(&identity)
        .unwrap();
    assert_eq!(final_state.dag.ordered_dag_tip.as_ref(), Some(&fourth.hash));
    assert_eq!(final_state.dag.best_height, snapshot_height + 2);
    assert_eq!(state_digest(&final_state).unwrap(), final_digest);
    assert!(final_runtime.pending_is_empty());
    assert!(final_runtime.staging().is_empty());
    assert!(target.chain_anchor_valid(&final_state).unwrap());

    drop(source);
    drop(target);
    let _ = std::fs::remove_dir_all(source_path);
    let _ = std::fs::remove_dir_all(target_path);
}