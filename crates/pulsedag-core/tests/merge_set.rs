use pulsedag_core::apply::commit_block_to_state;
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{Block, BlockHeader, ChainState, ConsensusMode, Hash, SelectedParentPolicy};

fn init_ghostdag_dev(chain_id: &str) -> ChainState {
    let mut state = init_chain_state(chain_id.to_string());
    state.dag.consensus_mode = ConsensusMode::GhostdagDev;
    state.dag.selected_parent_policy = SelectedParentPolicy::GhostdagInspired;
    state
}

fn block(hash: &str, parents: Vec<Hash>, height: u64) -> Block {
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
            blue_score: 0,
            height,
        },
        transactions: vec![],
    }
}

fn commit(state: &mut ChainState, block: &Block) {
    commit_block_to_state(block, state).expect("test block should commit");
}

#[test]
fn merge_set_parallel_blocks_within_k_become_blue() {
    let mut state = init_ghostdag_dev("merge-set-within-k");
    state.dag.merge_set_k = 2;
    let genesis = state.dag.genesis_hash.clone();
    let a = block("a", vec![genesis.clone()], 1);
    let b = block("b", vec![genesis.clone()], 1);
    let c = block("c", vec![genesis], 1);
    commit(&mut state, &a);
    commit(&mut state, &b);
    commit(&mut state, &c);

    let merge = block(
        "merge",
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        2,
    );
    commit(&mut state, &merge);

    assert_eq!(state.dag.merge_set_blues.get("merge").unwrap().len(), 2);
    assert!(state.dag.merge_set_reds.get("merge").unwrap().is_empty());
    let diagnostics = state.dag.merge_set_diagnostics.get("merge").unwrap();
    assert_eq!(diagnostics.merge_set_size, 2);
    assert_eq!(diagnostics.merge_set_blues_count, 2);
    assert_eq!(diagnostics.merge_set_reds_count, 0);
}

#[test]
fn merge_set_blocks_outside_k_become_red() {
    let mut state = init_ghostdag_dev("merge-set-outside-k");
    state.dag.merge_set_k = 1;
    let genesis = state.dag.genesis_hash.clone();
    let a = block("a", vec![genesis.clone()], 1);
    let b = block("b", vec![genesis.clone()], 1);
    let c = block("c", vec![genesis.clone()], 1);
    let d = block("d", vec![genesis], 1);
    for item in [&a, &b, &c, &d] {
        commit(&mut state, item);
    }

    let merge = block(
        "merge",
        vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ],
        2,
    );
    commit(&mut state, &merge);

    assert_eq!(state.dag.merge_set_blues.get("merge").unwrap().len(), 1);
    assert_eq!(state.dag.merge_set_reds.get("merge").unwrap().len(), 2);
    assert_eq!(
        state
            .dag
            .merge_set_diagnostics
            .get("merge")
            .unwrap()
            .merge_set_reds_count,
        2
    );
}

#[test]
fn same_dag_arrival_order_gives_same_blue_red_set() {
    let genesis = init_chain_state("tmp".to_string()).dag.genesis_hash;
    let a = block("a", vec![genesis.clone()], 1);
    let b = block("b", vec![genesis.clone()], 1);
    let c = block("c", vec![genesis], 1);
    let merge = block("merge", vec!["a".into(), "b".into(), "c".into()], 2);

    let mut one = init_chain_state("arrival-one".to_string());
    one.dag.merge_set_k = 1;
    for item in [&a, &b, &c, &merge] {
        commit(&mut one, item);
    }

    let mut two = init_chain_state("arrival-two".to_string());
    two.dag.merge_set_k = 1;
    for item in [&c, &b, &a, &merge] {
        commit(&mut two, item);
    }

    assert_eq!(
        one.dag.merge_set_blues.get("merge"),
        two.dag.merge_set_blues.get("merge")
    );
    assert_eq!(
        one.dag.merge_set_reds.get("merge"),
        two.dag.merge_set_reds.get("merge")
    );
}

#[test]
fn orphan_adoption_order_recomputes_merge_set_consistently() {
    let genesis = init_chain_state("tmp-orphan".to_string()).dag.genesis_hash;
    let a = block("a", vec![genesis.clone()], 1);
    let b = block("b", vec![genesis], 1);
    let merge = block("merge", vec!["a".into(), "b".into()], 2);

    let mut adopted_like = init_ghostdag_dev("adopted-like");
    commit(&mut adopted_like, &b);
    commit(&mut adopted_like, &a);
    commit(&mut adopted_like, &merge);

    let mut canonical = init_ghostdag_dev("canonical");
    commit(&mut canonical, &a);
    commit(&mut canonical, &b);
    commit(&mut canonical, &merge);

    assert_eq!(
        adopted_like.dag.selected_parents.get("merge"),
        canonical.dag.selected_parents.get("merge")
    );
    assert_eq!(
        adopted_like.dag.merge_set_blues.get("merge"),
        canonical.dag.merge_set_blues.get("merge")
    );
    assert_eq!(
        adopted_like.dag.blocks["merge"].header.blue_score,
        canonical.dag.blocks["merge"].header.blue_score
    );
}
