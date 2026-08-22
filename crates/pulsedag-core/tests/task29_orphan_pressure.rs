use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    accept_block_with_result, adopt_ready_orphans, assert_dag_consistent_for_tests,
    build_candidate_block, build_coinbase_transaction, classify_orphan_backlog,
    consensus_difficulty_snapshot, current_ts, dev_mine_header, expected_difficulty,
    merge_set_digest, missing_block_parents, ordered_dag_digest, preferred_tip_hash,
    queue_orphan_block, queue_orphan_block_bounded, refresh_block_consensus_ids,
    refresh_block_consensus_ids_with_state, selection_digest, state_digest, AcceptSource, Block,
    BlockAcceptanceResult, ChainState, CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
};

const CHAIN_ID: &str = "task29-orphan-pressure";
const PRESSURE_BLOCKS: usize = 32;
const CADENCE_SECS: u64 = 15;
const TEST_ORPHAN_CAPACITY: usize = 4;

fn mine_next_block(state: &ChainState, timestamp: u64, nonce_seed: u64) -> Block {
    let parent = preferred_tip_hash(state).expect("fixture must have a selected tip");
    let height = state.dag.best_height.saturating_add(1);
    let transactions = vec![build_coinbase_transaction(
        &format!("task29-pressure-miner-{height}"),
        50,
        nonce_seed,
    )];
    let difficulty = expected_difficulty(state);
    let mut block = build_candidate_block(vec![parent], height, difficulty, transactions);
    block.header.timestamp = timestamp;
    block.header.blue_score = height;
    block.header.nonce = nonce_seed;
    refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
    let (header, mined, _, _) = dev_mine_header(block.header.clone(), 2_000_000);
    assert!(mined, "Task29 orphan-pressure fixture must satisfy dev PoW");
    block.header = header;
    refresh_block_consensus_ids(&mut block);
    block
}

fn accept_valid(state: &mut ChainState, block: Block) {
    let hash = block.hash.clone();
    assert_eq!(
        accept_block_with_result(block, state, AcceptSource::P2p),
        BlockAcceptanceResult::Accepted,
        "accepting {hash}"
    );
    assert_dag_consistent_for_tests(state);
}

fn build_canonical_chain() -> Vec<Block> {
    let mut builder = init_chain_state(CHAIN_ID.to_string());
    let start = current_ts().saturating_sub(20_000);
    let mut blocks = Vec::with_capacity(PRESSURE_BLOCKS);

    for index in 0..PRESSURE_BLOCKS {
        let timestamp = start.saturating_add((index as u64 + 1).saturating_mul(CADENCE_SECS));
        let block = mine_next_block(&builder, timestamp, 50_000 + index as u64);
        accept_valid(&mut builder, block.clone());
        blocks.push(block);
    }

    blocks
}

fn replay_in_order(blocks: &[Block]) -> ChainState {
    let mut state = init_chain_state(CHAIN_ID.to_string());
    for block in blocks {
        accept_valid(&mut state, block.clone());
    }
    state
}

fn replay_reverse_orphan_pressure(blocks: &[Block]) -> ChainState {
    let mut state = init_chain_state(CHAIN_ID.to_string());

    for (queued, block) in blocks.iter().skip(1).rev().enumerate() {
        assert_eq!(
            accept_block_with_result(block.clone(), &mut state, AcceptSource::P2p),
            BlockAcceptanceResult::MissingParent,
            "reverse delivery must classify descendant as missing-parent"
        );
        let missing = missing_block_parents(block, &state);
        assert_eq!(
            missing.len(),
            1,
            "linear fixture has one direct missing parent"
        );
        assert!(queue_orphan_block(&mut state, block.clone(), missing));
        assert_eq!(state.orphan_blocks.len(), queued + 1);
        assert_eq!(state.orphan_parent_index.len(), queued + 1);
    }

    assert_eq!(state.orphan_blocks.len(), blocks.len() - 1);
    assert_eq!(state.orphan_parent_index.len(), blocks.len() - 1);

    accept_valid(&mut state, blocks[0].clone());

    let mut adopted = 0usize;
    loop {
        let accepted = adopt_ready_orphans(&mut state, AcceptSource::P2p);
        if accepted == 0 {
            break;
        }
        adopted = adopted.saturating_add(accepted);
        assert_dag_consistent_for_tests(&state);
    }

    assert_eq!(adopted, blocks.len() - 1);
    assert!(state.orphan_blocks.is_empty());
    assert!(state.orphan_parent_index.is_empty());
    assert_dag_consistent_for_tests(&state);
    state
}

fn assert_converged(ordered: &ChainState, pressured: &ChainState) {
    assert_eq!(preferred_tip_hash(ordered), preferred_tip_hash(pressured));
    assert_eq!(ordered.dag.selected_chain, pressured.dag.selected_chain);
    assert_eq!(ordered_dag_digest(ordered), ordered_dag_digest(pressured));
    assert_eq!(selection_digest(ordered), selection_digest(pressured));
    assert_eq!(merge_set_digest(ordered), merge_set_digest(pressured));
    assert_eq!(
        state_digest(ordered).unwrap(),
        state_digest(pressured).unwrap()
    );

    let ordered_pow = consensus_difficulty_snapshot(ordered);
    let pressured_pow = consensus_difficulty_snapshot(pressured);
    assert_eq!(ordered_pow.expected_bits, pressured_pow.expected_bits);
    assert_eq!(
        ordered_pow.retarget_multiplier_bps,
        pressured_pow.retarget_multiplier_bps
    );
    assert_eq!(
        ordered_pow.avg_block_interval_secs,
        pressured_pow.avg_block_interval_secs
    );
    assert_eq!(
        ordered_pow.target_block_interval_secs,
        CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
    );
    assert_eq!(
        pressured_pow.target_block_interval_secs,
        CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
    );
}

#[test]
fn accelerated_reverse_delivery_drains_bounded_orphan_pressure_and_converges() {
    let blocks = build_canonical_chain();
    let ordered = replay_in_order(&blocks);
    let pressured = replay_reverse_orphan_pressure(&blocks);

    assert_converged(&ordered, &pressured);
    assert_eq!(
        consensus_difficulty_snapshot(&ordered).avg_block_interval_secs,
        CADENCE_SECS
    );
    assert_dag_consistent_for_tests(&ordered);
    assert_dag_consistent_for_tests(&pressured);
}

#[test]
fn configured_orphan_capacity_evicts_overflow_and_keeps_indexes_bounded() {
    let mut state = init_chain_state(format!("{CHAIN_ID}-capacity"));
    let mut total_evicted = 0usize;

    for index in 0..TEST_ORPHAN_CAPACITY + 2 {
        let missing_parent = format!("task29-missing-parent-{index}");
        let block = build_candidate_block(
            vec![missing_parent.clone()],
            index as u64 + 1,
            expected_difficulty(&state),
            vec![build_coinbase_transaction(
                &format!("task29-capacity-miner-{index}"),
                50,
                90_000 + index as u64,
            )],
        );
        let result = queue_orphan_block_bounded(
            &mut state,
            block,
            vec![missing_parent],
            TEST_ORPHAN_CAPACITY,
            u64::MAX,
        );
        total_evicted = total_evicted.saturating_add(result.evicted);
        assert!(state.orphan_blocks.len() <= TEST_ORPHAN_CAPACITY);
        assert!(state.orphan_parent_index.len() <= TEST_ORPHAN_CAPACITY);
    }

    assert_eq!(total_evicted, 2);
    assert_eq!(state.orphan_blocks.len(), TEST_ORPHAN_CAPACITY);
    assert_eq!(state.orphan_parent_index.len(), TEST_ORPHAN_CAPACITY);
    assert_eq!(state.orphan_missing_parents.len(), TEST_ORPHAN_CAPACITY);

    let classification = classify_orphan_backlog(&state);
    assert_eq!(
        classification.waiting_missing_parent,
        TEST_ORPHAN_CAPACITY
    );
    assert_eq!(classification.retryable_ready, 0);
    assert_eq!(classification.stale_missing_parent_entries, 0);
    assert_eq!(classification.unindexed_missing_parent_entries, 0);
}
