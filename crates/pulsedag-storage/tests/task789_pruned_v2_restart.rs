use std::collections::BTreeSet;

use pulsedag_core::{
    compact_snapshot_to_retained_blocks, genesis::init_chain_state,
    materialize_authoritative_state_v2, ActivatedV2P2pRuntime, Block, BlockHeader, ChainState,
    ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_storage::Storage;

fn temp_db_path(test_name: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("pulsedag-task789-{test_name}-{unique}"))
        .to_string_lossy()
        .into_owned()
}

fn block(hash: &str, parent: &str, height: u64) -> Block {
    Block {
        hash: hash.to_string(),
        header: BlockHeader {
            version: 1,
            parents: vec![parent.to_string()],
            timestamp: height.saturating_add(100),
            difficulty: 1,
            nonce: 0,
            merkle_root: format!("m-{hash}"),
            state_root: format!("s-{hash}"),
            blue_score: height,
            height,
        },
        transactions: vec![],
    }
}

fn activated_chain() -> (ChainState, ProtocolActivationIdentity) {
    let mut state = init_chain_state("task789-pruned-v2-restart".to_string());
    let genesis = state.dag.genesis_hash.clone();
    let a = block("task789-a", &genesis, 1);
    let b = block("task789-b", "task789-a", 2);
    let c = block("task789-c", "task789-b", 3);

    state.dag.selected_chain = vec![
        genesis.clone(),
        a.hash.clone(),
        b.hash.clone(),
        c.hash.clone(),
    ];
    state.dag.selected_parents.insert(genesis.clone(), None);
    state
        .dag
        .selected_parents
        .insert(a.hash.clone(), Some(genesis.clone()));
    state
        .dag
        .selected_parents
        .insert(b.hash.clone(), Some(a.hash.clone()));
    state
        .dag
        .selected_parents
        .insert(c.hash.clone(), Some(b.hash.clone()));

    state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
    state.dag.merge_set_reds.insert(genesis.clone(), vec![]);
    for item in [&a, &b, &c] {
        state.dag.merge_set_blues.insert(item.hash.clone(), vec![]);
        state.dag.merge_set_reds.insert(item.hash.clone(), vec![]);
        state
            .dag
            .blue_work
            .insert(item.hash.clone(), item.header.height as u128 * 100);
        state.dag.blocks.insert(item.hash.clone(), item.clone());
    }
    state.dag.best_height = 3;

    let state = materialize_authoritative_state_v2(&state).unwrap();
    let identity = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    (state, identity)
}

#[test]
fn compact_pruned_activated_v2_snapshot_survives_cold_restart() {
    let path = temp_db_path("cold-restart");
    let (state, identity) = activated_chain();
    let runtime = ActivatedV2P2pRuntime::default();
    let storage = Storage::open(&path).unwrap();

    let persisted_blocks = state.dag.blocks.values().cloned().collect::<Vec<_>>();
    storage
        .persist_activated_v2_p2p_blocks_and_runtime(&persisted_blocks, &identity, &state, &runtime)
        .unwrap();

    let retained_blocks = persisted_blocks
        .iter()
        .filter(|block| block.header.height >= 2)
        .cloned()
        .collect::<Vec<_>>();
    let retained_hashes = retained_blocks
        .iter()
        .map(|block| block.hash.clone())
        .collect::<BTreeSet<_>>();
    let compact = compact_snapshot_to_retained_blocks(state, &retained_blocks).unwrap();
    let expected_root = compact.utxo.compute_state_root().unwrap();
    let generation = storage.accepted_storage_generation().unwrap();

    let removed = storage
        .commit_compact_prune(&compact, &retained_hashes, generation)
        .unwrap();
    assert!(removed > 0);
    assert!(!compact.dag.blocks.contains_key(&compact.dag.genesis_hash));
    assert!(storage.chain_anchor_valid(&compact).unwrap());

    drop(storage);

    let storage = Storage::open(&path).unwrap();
    let (restored, restored_runtime) = storage
        .load_activated_v2_p2p_runtime_snapshot(&identity)
        .unwrap();

    assert!(!restored.dag.blocks.contains_key(&restored.dag.genesis_hash));
    assert_eq!(
        restored
            .dag
            .blocks
            .values()
            .map(|block| block.header.height)
            .min(),
        Some(2)
    );
    assert_eq!(restored.dag.best_height, compact.dag.best_height);
    assert_eq!(restored.dag.ordered_dag_tip, compact.dag.ordered_dag_tip);
    assert_eq!(restored.utxo.compute_state_root().unwrap(), expected_root);
    assert!(restored_runtime.pending_is_empty());
    assert!(restored_runtime.staging().is_empty());
    assert!(storage.chain_anchor_valid(&restored).unwrap());

    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}
