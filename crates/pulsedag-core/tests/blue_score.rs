use pulsedag_core::apply::commit_block_to_state;
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{Block, BlockHeader, ChainState, Hash};

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
    commit_block_to_state(block, state).unwrap();
}

#[test]
fn blue_score_linear_chain_all_blue() {
    let mut state = init_chain_state("blue-linear".to_string());
    let genesis = state.dag.genesis_hash.clone();
    let a = block("a", vec![genesis], 1);
    let b = block("b", vec!["a".into()], 2);
    let c = block("c", vec!["b".into()], 3);
    for item in [&a, &b, &c] {
        commit(&mut state, item);
    }
    assert_eq!(state.dag.blocks["a"].header.blue_score, 1);
    assert_eq!(state.dag.blocks["b"].header.blue_score, 2);
    assert_eq!(state.dag.blocks["c"].header.blue_score, 3);
    assert!(state.dag.merge_set_blues["c"].is_empty());
}

#[test]
fn replay_gives_same_blue_score() {
    let genesis = init_chain_state("tmp-blue".to_string()).dag.genesis_hash;
    let a = block("a", vec![genesis.clone()], 1);
    let b = block("b", vec![genesis], 1);
    let merge = block("merge", vec!["a".into(), "b".into()], 2);
    let mut one = init_chain_state("blue-replay-one".to_string());
    let mut two = init_chain_state("blue-replay-two".to_string());
    for item in [&a, &b, &merge] {
        commit(&mut one, item);
    }
    for item in [&b, &a, &merge] {
        commit(&mut two, item);
    }
    assert_eq!(
        one.dag.blocks["merge"].header.blue_score,
        two.dag.blocks["merge"].header.blue_score
    );
    assert_eq!(
        one.dag.merge_set_diagnostics["merge"],
        two.dag.merge_set_diagnostics["merge"]
    );
}
