use pulsedag_core::{
    build_candidate_block, build_coinbase_transaction, compact_snapshot_to_retained_blocks,
    consensus_difficulty_snapshot, current_ts, dev_mine_header, expected_difficulty,
    preferred_tip_hash, rebuild_state_from_blocks, rebuild_state_from_snapshot_and_blocks,
    refresh_block_consensus_ids, refresh_block_consensus_ids_with_state, state_digest, Block,
    ChainState,
};

const CHAIN_ID: &str = "retarget-recovery-agreement";
const RETAINED_RETARGET_BLOCKS: usize = 20;

fn consensus_bytes(state: &ChainState) -> Vec<u8> {
    serde_json::to_vec(&consensus_difficulty_snapshot(state))
        .expect("consensus snapshot should serialize deterministically")
}

fn next_block(state: &ChainState, timestamp: u64, nonce: u64) -> Block {
    let parent = preferred_tip_hash(state).expect("chain should have a preferred tip");
    let height = state.dag.best_height.saturating_add(1);
    let difficulty = expected_difficulty(state);
    let coinbase = build_coinbase_transaction(&format!("agreement-miner-{height}"), 50, nonce);
    let mut block = build_candidate_block(vec![parent], height, difficulty, vec![coinbase]);
    block.header.timestamp = timestamp;
    refresh_block_consensus_ids_with_state(&mut block, state)
        .expect("fixture state root should be computable");
    let (header, mined, _, _) = dev_mine_header(block.header.clone(), 1_000_000);
    assert!(
        mined,
        "expected recovery-agreement fixture to mine height {height} at bits {difficulty}"
    );
    block.header = header;
    refresh_block_consensus_ids(&mut block);
    block
}

fn append_block(state: &mut ChainState, timestamp: u64, nonce: u64) -> Block {
    let block = next_block(state, timestamp, nonce);
    pulsedag_core::apply::apply_block(&block, state)
        .expect("fixture block should apply to canonical state");
    block
}

fn last_selected_blocks(state: &ChainState, count: usize) -> Vec<Block> {
    let mut retained = state
        .dag
        .selected_chain
        .iter()
        .rev()
        .filter_map(|hash| state.dag.blocks.get(hash))
        .filter(|block| block.header.height > 0)
        .take(count)
        .cloned()
        .collect::<Vec<_>>();
    retained.sort_by_key(|block| block.header.height);
    retained
}

fn assert_consensus_agreement(label: &str, expected: &ChainState, actual: &ChainState) {
    let expected_snapshot = consensus_difficulty_snapshot(expected);
    let actual_snapshot = consensus_difficulty_snapshot(actual);

    assert_eq!(
        expected_snapshot.expected_bits, actual_snapshot.expected_bits,
        "expected bits diverged for {label}"
    );
    assert_eq!(
        expected_snapshot.expected_target_hex, actual_snapshot.expected_target_hex,
        "expected target hex diverged for {label}"
    );
    assert_eq!(
        expected_snapshot.current_bits, actual_snapshot.current_bits,
        "current bits diverged for {label}"
    );
    assert_eq!(
        expected_snapshot.avg_block_interval_secs, actual_snapshot.avg_block_interval_secs,
        "observed interval diverged for {label}"
    );
    assert_eq!(
        consensus_bytes(expected),
        consensus_bytes(actual),
        "serialized consensus snapshot diverged for {label}"
    );
    assert_eq!(
        state_digest(expected).expect("expected state digest"),
        state_digest(actual).expect("actual state digest"),
        "canonical state digest diverged for {label}"
    );
}

#[test]
fn retarget_is_identical_after_restart_pruning_delta_replay_and_second_node_replay() {
    let mut canonical = pulsedag_core::genesis::init_chain_state(CHAIN_ID.to_string());
    let start = current_ts().saturating_sub(3_600);
    let mut all_blocks = Vec::new();

    for height in 1..=25_u64 {
        let block = append_block(&mut canonical, start + height * 30, 10_000 + height);
        all_blocks.push(block);
    }

    let snapshot_point = canonical.clone();
    let snapshot_consensus = consensus_difficulty_snapshot(&snapshot_point);
    assert_eq!(
        snapshot_consensus.observed_block_count,
        RETAINED_RETARGET_BLOCKS
    );
    assert_ne!(
        snapshot_consensus.expected_bits,
        pulsedag_core::retarget::CONSENSUS_POW_LIMIT_BITS,
        "fixture must exercise a hardened nontrivial target"
    );

    let retained = last_selected_blocks(&snapshot_point, RETAINED_RETARGET_BLOCKS);
    assert_eq!(retained.len(), RETAINED_RETARGET_BLOCKS);
    let compact_snapshot = compact_snapshot_to_retained_blocks(snapshot_point.clone(), &retained)
        .expect("selected-chain snapshot should compact to the retarget window");
    assert_consensus_agreement(
        "compact snapshot at prune boundary",
        &snapshot_point,
        &compact_snapshot,
    );

    let mut delta_blocks = Vec::new();
    for height in 26..=30_u64 {
        let block = append_block(&mut canonical, start + height * 30, 10_000 + height);
        all_blocks.push(block.clone());
        delta_blocks.push(block);
    }

    let restarted_json = serde_json::to_vec(&canonical).expect("canonical state should serialize");
    let restarted: ChainState =
        serde_json::from_slice(&restarted_json).expect("canonical state should deserialize");
    assert_consensus_agreement("serialized restart", &canonical, &restarted);

    let restored_from_pruned =
        rebuild_state_from_snapshot_and_blocks(compact_snapshot, delta_blocks.clone())
            .expect("pruned snapshot plus canonical delta should rebuild");
    assert_consensus_agreement(
        "pruned snapshot plus delta replay",
        &canonical,
        &restored_from_pruned,
    );

    let second_node = rebuild_state_from_blocks(CHAIN_ID.to_string(), all_blocks.clone())
        .expect("second node should replay the identical selected-chain history");
    assert_consensus_agreement("second node full replay", &canonical, &second_node);

    let reverse_delta = rebuild_state_from_snapshot_and_blocks(
        compact_snapshot_to_retained_blocks(snapshot_point, &retained)
            .expect("second compact snapshot should be reproducible"),
        delta_blocks.into_iter().rev().collect(),
    )
    .expect("deterministic replay should sort reversed delta input");
    assert_consensus_agreement("reversed delta replay", &canonical, &reverse_delta);
}
