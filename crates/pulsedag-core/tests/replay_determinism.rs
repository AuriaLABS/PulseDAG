use pulsedag_core::{
    accept_block_to_dag_metadata, accept_block_with_result, adopt_ready_orphans,
    build_candidate_block, build_coinbase_transaction, merge_set_digest, missing_block_parents,
    ordered_dag_digest, queue_orphan_block, rebuild_state_from_snapshot_and_blocks,
    refresh_block_consensus_ids_with_state, refresh_ordered_dag_phase,
    refresh_selected_chain_phase, selection_digest, state_digest, terminal_missing_parent_count,
    AcceptSource, Block, BlockAcceptanceResult, ChainState, ConsensusMode, Hash,
    SelectedParentPolicy,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayObservation {
    selected_parent_by_block: Vec<(Hash, Option<Hash>)>,
    selected_tip: Option<Hash>,
    selected_chain: Vec<Hash>,
    merge_set_blues: Vec<(Hash, Vec<Hash>)>,
    merge_set_reds: Vec<(Hash, Vec<Hash>)>,
    ordered_dag: Vec<Hash>,
    ordered_dag_digest: String,
    selection_digest: String,
    merge_set_digest: String,
    state_digest: String,
    terminal_missing_parent_count: usize,
}

fn ghostdag_state(chain_id: &str) -> ChainState {
    let mut state = pulsedag_core::genesis::init_chain_state(chain_id.to_string());
    state.dag.consensus_mode = ConsensusMode::GhostdagDev;
    state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
    state.dag.merge_set_k = 2;
    state
}

fn observe(state: &ChainState) -> ReplayObservation {
    let mut selected_parent_by_block = state
        .dag
        .selected_parents
        .iter()
        .map(|(block, parent)| (block.clone(), parent.clone()))
        .collect::<Vec<_>>();
    selected_parent_by_block.sort();
    let mut merge_set_blues = state
        .dag
        .merge_set_blues
        .iter()
        .map(|(block, hashes)| {
            let mut hashes = hashes.clone();
            hashes.sort();
            (block.clone(), hashes)
        })
        .collect::<Vec<_>>();
    merge_set_blues.sort();
    let mut merge_set_reds = state
        .dag
        .merge_set_reds
        .iter()
        .map(|(block, hashes)| {
            let mut hashes = hashes.clone();
            hashes.sort();
            (block.clone(), hashes)
        })
        .collect::<Vec<_>>();
    merge_set_reds.sort();
    ReplayObservation {
        selected_parent_by_block,
        selected_tip: pulsedag_core::preferred_tip_hash(state),
        selected_chain: state.dag.selected_chain.clone(),
        merge_set_blues,
        merge_set_reds,
        ordered_dag: state.dag.ordered_dag.clone(),
        ordered_dag_digest: ordered_dag_digest(state),
        selection_digest: selection_digest(state),
        merge_set_digest: merge_set_digest(state),
        state_digest: state_digest(state).unwrap(),
        terminal_missing_parent_count: terminal_missing_parent_count(state),
    }
}

fn block(
    state: &ChainState,
    parents: Vec<Hash>,
    height: u64,
    timestamp: u64,
    nonce: u64,
    miner: &str,
) -> Block {
    let txs = vec![build_coinbase_transaction(miner, 50, nonce)];
    let mut block = build_candidate_block(parents, height, 1, txs);
    block.header.timestamp = timestamp;
    block.header.nonce = nonce;
    refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
    block
}

fn chain_fixture() -> Vec<Block> {
    let s0 = ghostdag_state("fixture-linear");
    let a = block(&s0, vec![s0.dag.genesis_hash.clone()], 1, 10, 1, "a");
    let mut s1 = s0.clone();
    pulsedag_core::apply::apply_block(&a, &mut s1).unwrap();
    let b = block(&s1, vec![a.hash.clone()], 2, 20, 2, "b");
    let mut s2 = s1.clone();
    pulsedag_core::apply::apply_block(&b, &mut s2).unwrap();
    let c = block(&s2, vec![b.hash.clone()], 3, 30, 3, "c");
    vec![a, b, c]
}

fn competing_tips_fixture() -> Vec<Block> {
    let s = ghostdag_state("fixture-competing");
    vec![
        block(&s, vec![s.dag.genesis_hash.clone()], 1, 10, 11, "a"),
        block(&s, vec![s.dag.genesis_hash.clone()], 1, 10, 12, "b"),
    ]
}

fn merge_fixture() -> Vec<Block> {
    let s = ghostdag_state("fixture-merge");
    let a = block(&s, vec![s.dag.genesis_hash.clone()], 1, 10, 21, "a");
    let b = block(&s, vec![s.dag.genesis_hash.clone()], 1, 11, 22, "b");
    let mut sm = s.clone();
    accept_block_to_dag_metadata(&a, &mut sm).unwrap();
    accept_block_to_dag_metadata(&b, &mut sm).unwrap();
    let m = block(&sm, vec![a.hash.clone(), b.hash.clone()], 2, 20, 23, "m");
    vec![a, b, m]
}

fn parallel_blocks_fixture(conflicting_coinbase: bool) -> Vec<Block> {
    let s = ghostdag_state("fixture-parallel");
    let miner_b = if conflicting_coinbase { "same" } else { "b" };
    vec![
        block(&s, vec![s.dag.genesis_hash.clone()], 1, 10, 31, "same"),
        block(&s, vec![s.dag.genesis_hash.clone()], 1, 11, 31, miner_b),
    ]
}

fn red_blue_boundary_fixture() -> Vec<Block> {
    let s = ghostdag_state("fixture-boundary");
    let a = block(&s, vec![s.dag.genesis_hash.clone()], 1, 10, 41, "a");
    let b = block(&s, vec![s.dag.genesis_hash.clone()], 1, 11, 42, "b");
    let c = block(&s, vec![s.dag.genesis_hash.clone()], 1, 12, 43, "c");
    let d = block(&s, vec![s.dag.genesis_hash.clone()], 1, 13, 44, "d");
    let mut sm = s.clone();
    for x in [&a, &b, &c, &d] {
        accept_block_to_dag_metadata(x, &mut sm).unwrap();
    }
    let join = block(
        &sm,
        vec![
            a.hash.clone(),
            b.hash.clone(),
            c.hash.clone(),
            d.hash.clone(),
        ],
        2,
        30,
        45,
        "j",
    );
    vec![a, b, c, d, join]
}

fn ten_orders(len: usize) -> Vec<Vec<usize>> {
    let base = (0..len).collect::<Vec<_>>();
    let mut orders = vec![base.clone(), base.iter().rev().copied().collect()];
    for shift in 1..9 {
        let mut order = base.clone();
        order.rotate_left(shift % len.max(1));
        if !orders.contains(&order) {
            orders.push(order);
        }
    }
    while orders.len() < 10 {
        let mut order = base.clone();
        order.sort_by_key(|i| ((i * 7) + orders.len()) % len.max(1));
        orders.push(order);
    }
    orders.truncate(10);
    orders
}

fn replay_arrival_order(blocks: &[Block], order: &[usize], metadata_only: bool) -> ChainState {
    let mut state = ghostdag_state("determinism");
    let mut pending = Vec::<Block>::new();
    for index in order {
        let block = blocks[*index].clone();
        if metadata_only {
            if missing_block_parents(&block, &state).is_empty() {
                accept_block_to_dag_metadata(&block, &mut state).unwrap();
                refresh_selected_chain_phase(&mut state);
                refresh_ordered_dag_phase(&mut state);
            } else {
                pending.push(block);
            }
            let mut progressed = true;
            while progressed {
                progressed = false;
                let mut next_pending = Vec::new();
                for pending_block in pending.drain(..) {
                    if missing_block_parents(&pending_block, &state).is_empty() {
                        accept_block_to_dag_metadata(&pending_block, &mut state).unwrap();
                        refresh_selected_chain_phase(&mut state);
                        refresh_ordered_dag_phase(&mut state);
                        progressed = true;
                    } else {
                        next_pending.push(pending_block);
                    }
                }
                pending = next_pending;
            }
        } else {
            match accept_block_with_result(block.clone(), &mut state, AcceptSource::P2p) {
                BlockAcceptanceResult::Accepted => {}
                BlockAcceptanceResult::MissingParent => {
                    let missing = missing_block_parents(&block, &state);
                    queue_orphan_block(&mut state, block, missing);
                }
                other => panic!(
                    "unexpected acceptance result for {}: {:?}",
                    block.hash, other
                ),
            }
        }
        while adopt_ready_orphans(&mut state, AcceptSource::P2p) > 0 {}
    }
    assert!(
        pending.is_empty(),
        "metadata-only replay left unresolved pending parents"
    );
    if metadata_only {
        let rebuilt = pulsedag_core::apply::rebuild_state_from_ordered_dag(&state).unwrap();
        pulsedag_core::apply::commit_rebuilt_state(&mut state, rebuilt);
    }
    state
}

fn assert_fixture_is_deterministic(name: &str, blocks: Vec<Block>, metadata_only: bool) {
    let mut expected = None;
    for order in ten_orders(blocks.len()) {
        let state = replay_arrival_order(&blocks, &order, metadata_only);
        let actual = observe(&state);
        if let Some(expected) = &expected {
            assert_eq!(expected, &actual, "fixture {name} order {order:?}");
        } else {
            expected = Some(actual);
        }
    }
}

#[test]
fn replay_determinism_linear_chain() {
    assert_fixture_is_deterministic("linear", chain_fixture(), true);
}

#[test]
fn replay_determinism_two_same_height_competing_tips() {
    assert_fixture_is_deterministic("same-height", competing_tips_fixture(), true);
}

#[test]
fn replay_determinism_multi_parent_joining_two_tips() {
    assert_fixture_is_deterministic("merge", merge_fixture(), true);
}

#[test]
fn replay_determinism_missing_parent_then_orphan_adoption() {
    let blocks = chain_fixture();
    let state = replay_arrival_order(&blocks, &[2, 1, 0], true);
    assert_eq!(terminal_missing_parent_count(&state), 0);
    assert_fixture_is_deterministic("orphan-adoption", blocks, true);
}

#[test]
fn replay_determinism_parallel_non_conflicting_transactions() {
    assert_fixture_is_deterministic(
        "parallel-non-conflicting",
        parallel_blocks_fixture(false),
        true,
    );
}

#[test]
fn replay_determinism_parallel_conflicting_transactions() {
    assert_fixture_is_deterministic("parallel-conflicting", parallel_blocks_fixture(true), true);
}

#[test]
fn replay_determinism_red_blue_merge_set_boundary_k2() {
    let blocks = red_blue_boundary_fixture();
    let state = replay_arrival_order(&blocks, &[0, 1, 2, 3, 4], true);
    let join = &blocks[4].hash;
    assert_eq!(state.dag.merge_set_blues.get(join).unwrap().len(), 2);
    assert_eq!(state.dag.merge_set_reds.get(join).unwrap().len(), 1);
    assert_fixture_is_deterministic("k2-boundary", blocks, true);
}

#[test]
fn replay_determinism_snapshot_restore_replay() {
    let blocks = chain_fixture();
    let snapshot = replay_arrival_order(&blocks[..1], &[0], false);
    let restored_a =
        rebuild_state_from_snapshot_and_blocks(snapshot.clone(), blocks[1..].to_vec()).unwrap();
    let restored_b = rebuild_state_from_snapshot_and_blocks(
        snapshot,
        blocks[1..].iter().rev().cloned().collect(),
    )
    .unwrap();
    assert_eq!(observe(&restored_a), observe(&restored_b));
}
