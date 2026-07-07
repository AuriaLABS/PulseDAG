use pulsedag_core::apply::commit_block_to_state;
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{Block, BlockHeader, ChainState, ConsensusMode, Hash, SelectedParentPolicy};

fn block(hash: &str, parents: Vec<Hash>, height: u64, blue_score: u64) -> Block {
    Block {
        hash: hash.to_string(),
        header: BlockHeader {
            version: 1,
            parents,
            timestamp: height,
            difficulty: 1,
            nonce: 0,
            merkle_root: format!("merkle-{hash}"),
            state_root: format!("state-{hash}"),
            blue_score,
            height,
        },
        transactions: vec![],
    }
}

fn init_ghostdag_dev(chain_id: &str) -> ChainState {
    let mut state = init_chain_state(chain_id.to_string());
    state.dag.consensus_mode = ConsensusMode::GhostdagDev;
    state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
    state
}

fn accept_all(mut state: ChainState, blocks: Vec<Block>) -> ChainState {
    for block in blocks {
        commit_block_to_state(&block, &mut state).expect("test block should commit");
    }
    state
}

fn build_dag_blocks(genesis: &str) -> (Block, Block, Block, Block) {
    let a = block("a", vec![genesis.to_string()], 1, 1);
    let b = block("b", vec![genesis.to_string()], 1, 1);
    let a2 = block("a2", vec!["a".to_string()], 2, 3);
    let merge = block("merge", vec!["b".to_string(), "a2".to_string()], 3, 4);
    (a, b, a2, merge)
}

#[test]
fn selected_parent_same_dag_different_arrival_orders_is_deterministic() {
    let base = init_ghostdag_dev("selected-parent-arrival");
    let genesis = base.dag.genesis_hash.clone();
    let (a, b, a2, merge) = build_dag_blocks(&genesis);

    let state_one = accept_all(
        base.clone(),
        vec![a.clone(), b.clone(), a2.clone(), merge.clone()],
    );
    let state_two = accept_all(base, vec![b, a, a2, merge.clone()]);

    assert_eq!(
        state_one.dag.selected_parents.get(&merge.hash),
        Some(&Some("a2".to_string()))
    );
    assert_eq!(
        state_one.dag.selected_parents.get(&merge.hash),
        state_two.dag.selected_parents.get(&merge.hash)
    );
}

#[test]
fn replay_selected_chain_from_snapshot_is_stable() {
    let base = init_ghostdag_dev("selected-parent-snapshot");
    let genesis = base.dag.genesis_hash.clone();
    let (a, b, a2, merge) = build_dag_blocks(&genesis);

    let snapshot = accept_all(base.clone(), vec![a.clone(), b.clone()]);
    let replayed_from_snapshot = accept_all(snapshot, vec![a2.clone(), merge.clone()]);
    let clean_replay = accept_all(base, vec![a, b, a2, merge]);

    assert_eq!(
        replayed_from_snapshot.dag.selected_chain,
        clean_replay.dag.selected_chain
    );
    assert_eq!(
        replayed_from_snapshot.dag.selected_chain,
        vec![
            genesis,
            "a".to_string(),
            "a2".to_string(),
            "merge".to_string()
        ]
    );
}

#[test]
fn selected_parent_orphan_adoption_preserves_final_selection() {
    let base = init_ghostdag_dev("selected-parent-orphan");
    let genesis = base.dag.genesis_hash.clone();
    let (a, b, a2, merge) = build_dag_blocks(&genesis);

    let adopted_order = accept_all(
        base.clone(),
        vec![b.clone(), a.clone(), a2.clone(), merge.clone()],
    );
    let canonical_order = accept_all(base, vec![a, b, a2, merge.clone()]);

    assert_eq!(
        adopted_order.dag.selected_parents.get(&merge.hash),
        canonical_order.dag.selected_parents.get(&merge.hash)
    );
    assert_eq!(
        adopted_order.dag.selected_parents.get(&merge.hash),
        Some(&Some("a2".to_string()))
    );
}

#[test]
fn selected_parent_competing_same_height_tips_use_hash_tie_break() {
    let base = init_ghostdag_dev("selected-parent-tie");
    let genesis = base.dag.genesis_hash.clone();
    let z = block("z-parent", vec![genesis.clone()], 1, 1);
    let a = block("a-parent", vec![genesis], 1, 1);
    let merge = block(
        "merge",
        vec!["z-parent".to_string(), "a-parent".to_string()],
        2,
        2,
    );

    let state = accept_all(base, vec![z, a, merge.clone()]);

    assert_eq!(
        state.dag.selected_parents.get(&merge.hash),
        Some(&Some("a-parent".to_string()))
    );
}

#[test]
fn selected_parent_genesis_behavior_remains_stable() {
    let state = init_chain_state("selected-parent-genesis".to_string());

    assert_eq!(
        state.dag.selected_parents.get(&state.dag.genesis_hash),
        Some(&None)
    );
    assert_eq!(
        state.dag.selected_chain,
        vec![state.dag.genesis_hash.clone()]
    );
}
