use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    accept_block_with_result, assert_dag_consistent_for_tests, build_candidate_block,
    build_coinbase_transaction, refresh_block_consensus_ids,
    refresh_block_consensus_ids_with_state, sorted_tip_hashes, AcceptSource, Block,
    BlockAcceptanceResult, ChainState, Hash,
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
    assert_dag_consistent_for_tests(state);
}

fn test_block(
    state: &ChainState,
    hash: &str,
    parents: Vec<Hash>,
    height: u64,
    timestamp: u64,
) -> Block {
    let txs = vec![build_coinbase_transaction(
        &format!("miner-{hash}"),
        50,
        height,
    )];
    let mut block = build_candidate_block(parents, height, 1, txs);
    block.header.timestamp = timestamp;
    block.header.blue_score = height;
    refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
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
        &state,
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
    let sibling_a = test_block(&state, "sibling-a", vec![genesis.clone()], 1, 1);
    let sibling_a_hash = sibling_a.hash.clone();
    let mut sibling_b = test_block(&state, "sibling-b", vec![genesis], 1, 1);
    accept_test_block(&mut state, sibling_a);
    refresh_block_consensus_ids_with_state(&mut sibling_b, &state).unwrap();
    let sibling_b_hash = sibling_b.hash.clone();
    accept_test_block(&mut state, sibling_b);

    assert!(state.dag.blocks.contains_key(&sibling_a_hash));
    assert!(state.dag.blocks.contains_key(&sibling_b_hash));
    let sorted_once = sorted_tip_hashes(&state);
    assert_eq!(sorted_once, sorted_tip_hashes(&state));
    assert!(sorted_once.contains(&sibling_a_hash));
    assert!(sorted_once.contains(&sibling_b_hash));
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_accepting_child_removes_parent_from_tips_when_appropriate() {
    let mut state = init_chain_state("dag-invariant-child-tip".to_string());
    let parent = test_block(
        &state,
        "parent-tip",
        vec![state.dag.genesis_hash.clone()],
        1,
        1,
    );
    let parent_hash = parent.hash.clone();
    accept_test_block(&mut state, parent);
    assert_eq!(sorted_tip_hashes(&state), vec![parent_hash.clone()]);

    let child = test_block(&state, "child-tip", vec![parent_hash.clone()], 2, 2);
    let child_hash = child.hash.clone();
    accept_test_block(&mut state, child);

    assert!(!state.dag.tips.contains(&parent_hash));
    assert_eq!(sorted_tip_hashes(&state), vec![child_hash]);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_invalid_blocks_do_not_mutate_dag_state() {
    let mut state = init_chain_state("dag-invariant-invalid".to_string());
    let before_invalid = dag_snapshot(&state);
    let invalid_height = test_block(
        &state,
        "invalid-height",
        vec![state.dag.genesis_hash.clone()],
        2,
        1,
    );

    let outcome = accept_block_with_result(invalid_height, &mut state, AcceptSource::P2p);

    assert_ne!(outcome, BlockAcceptanceResult::Accepted);
    assert_eq!(dag_snapshot(&state), before_invalid);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_duplicate_parents_are_rejected_without_mutation() {
    let mut state = init_chain_state("dag-invariant-duplicate-parents".to_string());
    let before = dag_snapshot(&state);
    let genesis = state.dag.genesis_hash.clone();
    let duplicate_parent = test_block(
        &state,
        "duplicate-parent",
        vec![genesis.clone(), genesis],
        1,
        1,
    );

    let outcome = accept_block_with_result(duplicate_parent, &mut state, AcceptSource::P2p);

    assert_eq!(outcome, BlockAcceptanceResult::Malformed);
    assert_eq!(dag_snapshot(&state), before);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_missing_parent_becomes_orphan_candidate_without_mutation() {
    let mut state = init_chain_state("dag-invariant-missing-parent".to_string());
    let before = dag_snapshot(&state);
    let missing_parent = test_block(
        &state,
        "missing-parent-child",
        vec!["missing-parent".to_string()],
        1,
        1,
    );

    let outcome = accept_block_with_result(missing_parent, &mut state, AcceptSource::P2p);

    assert_eq!(outcome, BlockAcceptanceResult::MissingParent);
    assert_eq!(dag_snapshot(&state), before);
    assert_dag_invariants(&state);
}

#[test]
fn dag_invariant_future_timestamp_is_rejected_without_mutation() {
    let mut state = init_chain_state("dag-invariant-future-timestamp".to_string());
    let before = dag_snapshot(&state);
    let mut future = test_block(
        &state,
        "future-timestamp",
        vec![state.dag.genesis_hash.clone()],
        1,
        u64::MAX,
    );
    refresh_block_consensus_ids(&mut future);

    let outcome = accept_block_with_result(future, &mut state, AcceptSource::P2p);

    assert_eq!(outcome, BlockAcceptanceResult::Malformed);
    assert_eq!(dag_snapshot(&state), before);
    assert_dag_invariants(&state);
}
