use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    accept_block_with_result, build_candidate_block, build_coinbase_transaction, sorted_tip_hashes,
    AcceptSource, Block, BlockAcceptanceResult, ChainState, Hash,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct DagSnapshot {
    block_hashes: BTreeSet<Hash>,
    tips: BTreeSet<Hash>,
    children: BTreeMap<Hash, Vec<Hash>>,
    best_height: u64,
}

fn dag_snapshot(state: &ChainState) -> DagSnapshot {
    let block_hashes = state.dag.blocks.keys().cloned().collect();
    let tips = state.dag.tips.iter().cloned().collect();
    let children = state
        .dag
        .children
        .iter()
        .map(|(parent, children)| {
            let mut children = children.clone();
            children.sort();
            (parent.clone(), children)
        })
        .collect();

    DagSnapshot {
        block_hashes,
        tips,
        children,
        best_height: state.dag.best_height,
    }
}

fn assert_dag_invariants(state: &ChainState) {
    assert!(
        state.dag.blocks.contains_key(&state.dag.genesis_hash),
        "genesis {} is missing from dag.blocks",
        state.dag.genesis_hash
    );

    for (hash, block) in &state.dag.blocks {
        if hash == &state.dag.genesis_hash {
            continue;
        }

        assert!(
            !block.header.parents.is_empty(),
            "accepted non-genesis block {hash} has no parents"
        );
        for parent in &block.header.parents {
            assert!(
                state.dag.blocks.contains_key(parent),
                "accepted block {hash} references unknown parent {parent}"
            );
        }
    }

    for (parent, children) in &state.dag.children {
        assert!(
            state.dag.blocks.contains_key(parent),
            "children map contains unknown parent {parent}"
        );
        for child in children {
            assert!(
                state.dag.blocks.contains_key(child),
                "children map references unknown child {child} for parent {parent}"
            );
        }
    }

    for tip in &state.dag.tips {
        assert!(
            state.dag.blocks.contains_key(tip),
            "tip {tip} is missing from dag.blocks"
        );
    }

    for (parent, children) in &state.dag.children {
        if !children.is_empty() {
            assert!(
                !state.dag.tips.contains(parent),
                "block {parent} has children but remains a tip"
            );
        }
    }

    let max_height = state
        .dag
        .blocks
        .values()
        .map(|block| block.header.height)
        .max()
        .expect("genesis must ensure at least one accepted block");
    assert_eq!(
        state.dag.best_height, max_height,
        "best_height must equal max accepted block height"
    );
}

fn test_block(hash: &str, parents: Vec<Hash>, height: u64, timestamp: u64) -> Block {
    let txs = vec![build_coinbase_transaction(
        &format!("miner-{hash}"),
        50,
        height,
    )];
    let mut block = build_candidate_block(parents, height, 1, txs);
    block.hash = hash.to_string();
    block.header.timestamp = timestamp;
    block.header.blue_score = height;
    block.header.merkle_root = format!("merkle-{hash}");
    block.header.state_root = format!("state-{hash}");
    block
}

fn accept_test_block(state: &mut ChainState, block: Block) {
    let hash = block.hash.clone();
    let outcome = accept_block_with_result(block, state, AcceptSource::P2p);
    assert_eq!(outcome, BlockAcceptanceResult::Accepted, "accepting {hash}");
    assert_dag_invariants(state);
}

#[test]
fn dag_invariant_genesis_exists_in_blocks() {
    let state = init_chain_state("dag-invariant-genesis".to_string());

    assert_dag_invariants(&state);
    assert!(state.dag.blocks.contains_key(&state.dag.genesis_hash));
}

#[test]
fn dag_invariant_duplicate_block_acceptance_does_not_mutate_dag_state() {
    let mut state = init_chain_state("dag-invariant-duplicate".to_string());
    let block = test_block(
        "duplicate-child",
        vec![state.dag.genesis_hash.clone()],
        1,
        1,
    );
    accept_test_block(&mut state, block.clone());
    let before_duplicate = dag_snapshot(&state);

    let outcome = accept_block_with_result(block, &mut state, AcceptSource::P2p);

    assert_eq!(outcome, BlockAcceptanceResult::Duplicate);
    assert_eq!(dag_snapshot(&state), before_duplicate);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_accepting_siblings_preserves_blocks_and_deterministic_tips() {
    let mut state = init_chain_state("dag-invariant-siblings".to_string());
    let genesis = state.dag.genesis_hash.clone();
    let sibling_a = test_block("sibling-a", vec![genesis.clone()], 1, 1);
    let sibling_b = test_block("sibling-b", vec![genesis], 1, 1);

    accept_test_block(&mut state, sibling_a);
    accept_test_block(&mut state, sibling_b);

    assert!(state.dag.blocks.contains_key("sibling-a"));
    assert!(state.dag.blocks.contains_key("sibling-b"));
    assert_eq!(sorted_tip_hashes(&state), vec!["sibling-b", "sibling-a"]);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_accepting_child_removes_parent_from_tips_when_appropriate() {
    let mut state = init_chain_state("dag-invariant-child-tip".to_string());
    let parent = test_block("parent-tip", vec![state.dag.genesis_hash.clone()], 1, 1);
    accept_test_block(&mut state, parent);
    assert_eq!(sorted_tip_hashes(&state), vec!["parent-tip"]);

    let child = test_block("child-tip", vec!["parent-tip".to_string()], 2, 2);
    accept_test_block(&mut state, child);

    assert!(!state.dag.tips.contains("parent-tip"));
    assert_eq!(sorted_tip_hashes(&state), vec!["child-tip"]);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_invalid_blocks_do_not_mutate_dag_state() {
    let mut state = init_chain_state("dag-invariant-invalid".to_string());
    let before_invalid = dag_snapshot(&state);
    let invalid_height = test_block("invalid-height", vec![state.dag.genesis_hash.clone()], 2, 1);

    let outcome = accept_block_with_result(invalid_height, &mut state, AcceptSource::P2p);

    assert_ne!(outcome, BlockAcceptanceResult::Accepted);
    assert_eq!(dag_snapshot(&state), before_invalid);
    assert_dag_invariants(&state);
}
