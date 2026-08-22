use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    accept_block_with_result, adopt_ready_orphans, assert_dag_consistent_for_tests,
    build_candidate_block, build_coinbase_transaction, consensus_difficulty_snapshot, current_ts,
    dev_mine_header, expected_difficulty, merge_set_digest, missing_block_parents,
    ordered_dag_digest, preferred_tip_hash, queue_orphan_block, refresh_block_consensus_ids,
    refresh_block_consensus_ids_with_state, selection_digest, state_digest, AcceptSource, Block,
    BlockAcceptanceResult, ChainState, CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
};

const CHAIN_ID: &str = "task29-high-cadence-envelope";
const BLOCKS_PER_SWEEP: usize = 10;
const CADENCE_SWEEP_SECS: [u64; 4] = [1, 2, 5, 15];

fn mine_next_block(state: &ChainState, timestamp: u64, nonce_seed: u64) -> Block {
    let parent = preferred_tip_hash(state).expect("fixture must have a selected tip");
    let height = state.dag.best_height.saturating_add(1);
    let transactions = vec![build_coinbase_transaction(
        &format!("task29-miner-{height}"),
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
    assert!(mined, "Task29 cadence fixture must satisfy dev PoW");
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

fn build_canonical_sweep(cadence_secs: u64) -> Vec<Block> {
    assert!(cadence_secs < CONSENSUS_TARGET_BLOCK_INTERVAL_SECS);
    let mut builder = init_chain_state(CHAIN_ID.to_string());
    let start = current_ts().saturating_sub(10_000);
    let mut blocks = Vec::with_capacity(BLOCKS_PER_SWEEP);
    for index in 0..BLOCKS_PER_SWEEP {
        let timestamp = start.saturating_add((index as u64 + 1).saturating_mul(cadence_secs));
        let block = mine_next_block(&builder, timestamp, 10_000 + index as u64);
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

fn replay_with_pairwise_reordering(blocks: &[Block]) -> ChainState {
    let mut state = init_chain_state(CHAIN_ID.to_string());
    let mut index = 0usize;
    while index < blocks.len() {
        if index + 1 >= blocks.len() {
            accept_valid(&mut state, blocks[index].clone());
            break;
        }

        let parent = blocks[index].clone();
        let child = blocks[index + 1].clone();
        assert_eq!(
            accept_block_with_result(child.clone(), &mut state, AcceptSource::P2p),
            BlockAcceptanceResult::MissingParent
        );
        let missing = missing_block_parents(&child, &state);
        assert_eq!(missing, vec![parent.hash.clone()]);
        assert!(queue_orphan_block(&mut state, child, missing));
        assert!(
            state.orphan_blocks.len() <= 1,
            "orphan burst must stay bounded"
        );
        assert!(
            state.orphan_parent_index.len() <= 1,
            "missing-parent index must stay bounded"
        );

        accept_valid(&mut state, parent);
        assert_eq!(adopt_ready_orphans(&mut state, AcceptSource::P2p), 1);
        assert!(state.orphan_blocks.is_empty());
        assert!(state.orphan_parent_index.is_empty());
        assert_dag_consistent_for_tests(&state);
        index += 2;
    }
    state
}

fn replay_with_duplicate_delivery(blocks: &[Block]) -> ChainState {
    let mut state = init_chain_state(CHAIN_ID.to_string());
    for block in blocks {
        accept_valid(&mut state, block.clone());
        assert_eq!(
            accept_block_with_result(block.clone(), &mut state, AcceptSource::P2p),
            BlockAcceptanceResult::Duplicate,
            "repeat delivery must be duplicate-suppressed"
        );
    }
    state
}

fn assert_converged(label: &str, left: &ChainState, right: &ChainState) {
    assert_eq!(
        preferred_tip_hash(left),
        preferred_tip_hash(right),
        "selected tip diverged for {label}"
    );
    assert_eq!(
        left.dag.selected_chain, right.dag.selected_chain,
        "selected chain diverged for {label}"
    );
    assert_eq!(
        ordered_dag_digest(left),
        ordered_dag_digest(right),
        "ordered DAG digest diverged for {label}"
    );
    assert_eq!(
        selection_digest(left),
        selection_digest(right),
        "selection digest diverged for {label}"
    );
    assert_eq!(
        merge_set_digest(left),
        merge_set_digest(right),
        "merge-set digest diverged for {label}"
    );
    assert_eq!(
        state_digest(left).unwrap(),
        state_digest(right).unwrap(),
        "state digest diverged for {label}"
    );

    let left_pow = consensus_difficulty_snapshot(left);
    let right_pow = consensus_difficulty_snapshot(right);
    assert_eq!(left_pow.expected_bits, right_pow.expected_bits);
    assert_eq!(
        left_pow.retarget_multiplier_bps,
        right_pow.retarget_multiplier_bps
    );
    assert_eq!(
        left_pow.avg_block_interval_secs,
        right_pow.avg_block_interval_secs
    );
    assert_eq!(
        left_pow.target_block_interval_secs, CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        "experimental cadence must not mutate the version-frozen consensus clock"
    );
    assert_eq!(
        right_pow.target_block_interval_secs, CONSENSUS_TARGET_BLOCK_INTERVAL_SECS,
        "reordered delivery must not mutate the version-frozen consensus clock"
    );
}

#[test]
fn controlled_fast_cadence_sweeps_converge_after_reordering_and_duplicates() {
    for cadence_secs in CADENCE_SWEEP_SECS {
        let blocks = build_canonical_sweep(cadence_secs);
        let ordered = replay_in_order(&blocks);
        let reordered = replay_with_pairwise_reordering(&blocks);
        let duplicate_delivery = replay_with_duplicate_delivery(&blocks);

        let label = format!("cadence_{cadence_secs}s");
        assert_converged(&label, &ordered, &reordered);
        assert_converged(&label, &ordered, &duplicate_delivery);

        let snapshot = consensus_difficulty_snapshot(&ordered);
        assert_eq!(snapshot.avg_block_interval_secs, cadence_secs);
        assert_eq!(
            snapshot.target_block_interval_secs,
            CONSENSUS_TARGET_BLOCK_INTERVAL_SECS
        );
        assert!(ordered.orphan_blocks.is_empty());
        assert!(ordered.orphan_parent_index.is_empty());
        assert!(reordered.orphan_blocks.is_empty());
        assert!(reordered.orphan_parent_index.is_empty());
        assert_dag_consistent_for_tests(&ordered);
        assert_dag_consistent_for_tests(&reordered);
        assert_dag_consistent_for_tests(&duplicate_delivery);
    }
}
