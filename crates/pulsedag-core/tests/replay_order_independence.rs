use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::apply::apply_block;
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    accept_block_with_result, adopt_ready_orphans, assert_dag_consistent_for_tests,
    build_candidate_block, build_coinbase_transaction, merge_set_digest, missing_block_parents,
    ordered_dag_digest, preferred_tip_hash, queue_orphan_block, rebuild_state_from_blocks,
    rebuild_state_from_snapshot_and_blocks, refresh_block_consensus_ids_with_state,
    selection_digest, state_digest, AcceptSource, Block, BlockAcceptanceResult, ChainState, Hash,
    OutPoint, Utxo,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedChainState {
    blocks: BTreeMap<Hash, NormalizedBlock>,
    tips: BTreeSet<Hash>,
    children: BTreeMap<Hash, Vec<Hash>>,
    best_height: u64,
    utxos: BTreeMap<String, NormalizedUtxo>,
    address_index: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedBlock {
    parents: Vec<Hash>,
    height: u64,
    timestamp: u64,
    txids: Vec<Hash>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedUtxo {
    address: String,
    amount: u64,
    coinbase: bool,
    height: u64,
}

fn outpoint_key(outpoint: &OutPoint) -> String {
    format!("{}:{}", outpoint.txid, outpoint.index)
}

fn normalize_chain_state_for_comparison(state: &ChainState) -> NormalizedChainState {
    let blocks = state
        .dag
        .blocks
        .iter()
        .map(|(hash, block)| {
            let mut parents = block.header.parents.clone();
            parents.sort();
            let txids = block
                .transactions
                .iter()
                .map(|tx| tx.txid.clone())
                .collect::<Vec<_>>();
            (
                hash.clone(),
                NormalizedBlock {
                    parents,
                    height: block.header.height,
                    timestamp: block.header.timestamp,
                    txids,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let children = state
        .dag
        .children
        .iter()
        .map(|(parent, children)| {
            let mut children = children.clone();
            children.sort();
            (parent.clone(), children)
        })
        .collect::<BTreeMap<_, _>>();

    let utxos = state
        .utxo
        .utxos
        .iter()
        .map(|(outpoint, utxo)| (outpoint_key(outpoint), normalized_utxo(utxo)))
        .collect::<BTreeMap<_, _>>();

    let address_index = state
        .utxo
        .address_index
        .iter()
        .map(|(address, outpoints)| {
            let mut outpoints = outpoints.iter().map(outpoint_key).collect::<Vec<_>>();
            outpoints.sort();
            (address.clone(), outpoints)
        })
        .collect::<BTreeMap<_, _>>();

    NormalizedChainState {
        blocks,
        tips: state.dag.tips.iter().cloned().collect(),
        children,
        best_height: state.dag.best_height,
        utxos,
        address_index,
    }
}

fn assert_replay_states_equal(
    left_label: &str,
    left: &ChainState,
    right_label: &str,
    right: &ChainState,
) {
    let left_accepted = left.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
    let right_accepted = right.dag.blocks.keys().cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        left_accepted, right_accepted,
        "accepted block set diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        preferred_tip_hash(left),
        preferred_tip_hash(right),
        "selected tip diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        left.dag.selected_chain, right.dag.selected_chain,
        "selected chain diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        left.dag.ordered_dag, right.dag.ordered_dag,
        "ordered DAG diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        ordered_dag_digest(left),
        ordered_dag_digest(right),
        "ordered DAG digest diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        state_digest(left).unwrap(),
        state_digest(right).unwrap(),
        "UTXO/state root diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        merge_set_digest(left),
        merge_set_digest(right),
        "merge-set digest diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        selection_digest(left),
        selection_digest(right),
        "selection digest diverged between {left_label} and {right_label}"
    );
    assert_eq!(
        normalize_chain_state_for_comparison(left),
        normalize_chain_state_for_comparison(right),
        "normalized children/tip/UTXO indexes diverged between {left_label} and {right_label}"
    );
}

fn normalized_utxo(utxo: &Utxo) -> NormalizedUtxo {
    NormalizedUtxo {
        address: utxo.address.clone(),
        amount: utxo.amount,
        coinbase: utxo.coinbase,
        height: utxo.height,
    }
}

fn test_block(
    state: &ChainState,
    hash: &str,
    parents: Vec<Hash>,
    height: u64,
    timestamp: u64,
    nonce: u64,
) -> Block {
    let txs = vec![build_coinbase_transaction(
        &format!("miner-{hash}"),
        50,
        nonce,
    )];
    let mut block = build_candidate_block(parents, height, 1, txs);
    block.header.timestamp = timestamp;
    block.header.nonce = nonce;
    block.header.blue_score = height;
    refresh_block_consensus_ids_with_state(&mut block, state).unwrap();
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

fn queue_orphan(state: &mut ChainState, block: Block) {
    let missing = missing_block_parents(&block, state);
    assert!(
        !missing.is_empty(),
        "{} should be missing parents",
        block.hash
    );
    assert!(queue_orphan_block(state, block, missing));
}

fn parent_child_blocks(state: &ChainState) -> (Block, Block) {
    let parent = test_block(
        state,
        "replay-parent-a",
        vec![state.dag.genesis_hash.clone()],
        1,
        10,
        11,
    );
    let mut parent_state = state.clone();
    apply_block(&parent, &mut parent_state).unwrap();
    let child = test_block(
        &parent_state,
        "replay-child-b",
        vec![parent.hash.clone()],
        2,
        20,
        12,
    );
    (parent, child)
}

fn sibling_merge_blocks(state: &ChainState) -> (Block, Block, Block) {
    let sibling_a1 = test_block(
        state,
        "replay-sibling-a1",
        vec![state.dag.genesis_hash.clone()],
        1,
        10,
        21,
    );
    let sibling_a2 = test_block(
        state,
        "replay-sibling-a2",
        vec![state.dag.genesis_hash.clone()],
        1,
        10,
        22,
    );
    let mut canonical_merge_state = state.clone();
    for sibling in [&sibling_a1, &sibling_a2] {
        apply_block(sibling, &mut canonical_merge_state).unwrap();
    }
    let merge = test_block(
        &canonical_merge_state,
        "replay-merge-multi-parent",
        vec![sibling_a1.hash.clone(), sibling_a2.hash.clone()],
        2,
        20,
        23,
    );
    (sibling_a1, sibling_a2, merge)
}

fn deterministic_equal_height_siblings(state: &ChainState) -> (Block, Block) {
    for first_nonce in 100..160 {
        let first = test_block(
            state,
            "replay-equal-height-first",
            vec![state.dag.genesis_hash.clone()],
            1,
            30,
            first_nonce,
        );
        for second_nonce in 200..260 {
            let second = test_block(
                state,
                "replay-equal-height-second",
                vec![state.dag.genesis_hash.clone()],
                1,
                30,
                second_nonce,
            );
            if first.hash < second.hash {
                return (first, second);
            }
        }
    }
    panic!("could not build deterministically ordered equal-height siblings");
}

#[test]
fn replay_parent_child_is_equivalent_when_child_arrives_as_orphan_first() {
    let mut parent_then_child = init_chain_state("replay-parent-then-child".to_string());
    let (parent, child) = parent_child_blocks(&parent_then_child);
    accept_valid(&mut parent_then_child, parent.clone());
    accept_valid(&mut parent_then_child, child.clone());

    let mut child_orphan_then_parent =
        init_chain_state("replay-child-orphan-then-parent".to_string());
    assert_eq!(
        accept_block_with_result(
            child.clone(),
            &mut child_orphan_then_parent,
            AcceptSource::P2p
        ),
        BlockAcceptanceResult::MissingParent
    );
    queue_orphan(&mut child_orphan_then_parent, child);
    accept_valid(&mut child_orphan_then_parent, parent);
    assert_eq!(
        adopt_ready_orphans(&mut child_orphan_then_parent, AcceptSource::P2p),
        1
    );

    assert_eq!(
        preferred_tip_hash(&parent_then_child),
        preferred_tip_hash(&child_orphan_then_parent)
    );
    assert_eq!(
        normalize_chain_state_for_comparison(&parent_then_child),
        normalize_chain_state_for_comparison(&child_orphan_then_parent)
    );
    assert_dag_consistent_for_tests(&parent_then_child);
    assert_dag_consistent_for_tests(&child_orphan_then_parent);
}

#[test]
fn replay_sibling_order_and_multi_parent_merge_are_equivalent() {
    let mut a1_then_a2 = init_chain_state("replay-a1-then-a2".to_string());
    let (a1, a2, merge) = sibling_merge_blocks(&a1_then_a2);
    accept_valid(&mut a1_then_a2, a1.clone());
    accept_valid(&mut a1_then_a2, a2.clone());
    accept_valid(&mut a1_then_a2, merge.clone());

    let mut a2_then_a1 = init_chain_state("replay-a1-then-a2".to_string());
    accept_valid(&mut a2_then_a1, a2);
    accept_valid(&mut a2_then_a1, a1);
    accept_valid(&mut a2_then_a1, merge.clone());

    assert!(a1_then_a2.dag.blocks.contains_key(&merge.hash));
    assert_eq!(preferred_tip_hash(&a1_then_a2), Some(merge.hash.clone()));
    assert_replay_states_equal("a1_then_a2", &a1_then_a2, "a2_then_a1", &a2_then_a1);
    assert_dag_consistent_for_tests(&a1_then_a2);
    assert_dag_consistent_for_tests(&a2_then_a1);
}

#[test]
fn replay_invalid_intermediate_block_does_not_change_final_valid_state() {
    let mut clean_replay = init_chain_state("replay-clean".to_string());
    let (a1, a2, merge) = sibling_merge_blocks(&clean_replay);
    accept_valid(&mut clean_replay, a1.clone());
    accept_valid(&mut clean_replay, a2.clone());
    accept_valid(&mut clean_replay, merge.clone());

    let mut invalid_then_valid = init_chain_state("replay-invalid-then-valid".to_string());
    let mut invalid_a1 = a1.clone();
    invalid_a1.header.height = 2;
    assert_eq!(
        accept_block_with_result(invalid_a1, &mut invalid_then_valid, AcceptSource::P2p),
        BlockAcceptanceResult::Malformed
    );
    assert_eq!(
        normalize_chain_state_for_comparison(&invalid_then_valid),
        normalize_chain_state_for_comparison(&init_chain_state(
            "replay-invalid-then-valid".to_string()
        ))
    );

    accept_valid(&mut invalid_then_valid, a1);
    accept_valid(&mut invalid_then_valid, a2);
    accept_valid(&mut invalid_then_valid, merge);

    assert_eq!(
        preferred_tip_hash(&clean_replay),
        preferred_tip_hash(&invalid_then_valid)
    );
    assert_eq!(
        normalize_chain_state_for_comparison(&clean_replay),
        normalize_chain_state_for_comparison(&invalid_then_valid)
    );
    assert_dag_consistent_for_tests(&clean_replay);
    assert_dag_consistent_for_tests(&invalid_then_valid);
}

#[test]
fn replay_rebuild_equal_height_blocks_is_independent_of_input_order() {
    let state = init_chain_state("replay-equal-height-order".to_string());
    let (first, second) = deterministic_equal_height_siblings(&state);

    let sorted_input = rebuild_state_from_blocks(
        "replay-equal-height-order".to_string(),
        vec![first.clone(), second.clone()],
    )
    .unwrap();
    let reversed_input =
        rebuild_state_from_blocks("replay-equal-height-order".to_string(), vec![second, first])
            .unwrap();

    assert_replay_states_equal(
        "sorted_input",
        &sorted_input,
        "reversed_input",
        &reversed_input,
    );
    assert_dag_consistent_for_tests(&sorted_input);
    assert_dag_consistent_for_tests(&reversed_input);
}

#[test]
fn snapshot_delta_rebuild_equal_height_blocks_is_independent_of_input_order() {
    let snapshot = init_chain_state("replay-equal-height-snapshot".to_string());
    let (first, second) = deterministic_equal_height_siblings(&snapshot);

    let sorted_input = rebuild_state_from_snapshot_and_blocks(
        snapshot.clone(),
        vec![first.clone(), second.clone()],
    )
    .unwrap();
    let reversed_input =
        rebuild_state_from_snapshot_and_blocks(snapshot, vec![second, first]).unwrap();

    assert_replay_states_equal(
        "sorted_input",
        &sorted_input,
        "reversed_input",
        &reversed_input,
    );
    assert_dag_consistent_for_tests(&sorted_input);
    assert_dag_consistent_for_tests(&reversed_input);
}

#[test]
fn replay_rebuild_sorts_child_before_parent_input_into_same_selected_tip() {
    let state = init_chain_state("replay-rebuild-order".to_string());
    let (parent, child) = parent_child_blocks(&state);

    let parent_first = rebuild_state_from_blocks(
        "replay-rebuild-order".to_string(),
        vec![parent.clone(), child.clone()],
    )
    .unwrap();
    let child_first =
        rebuild_state_from_blocks("replay-rebuild-order".to_string(), vec![child, parent]).unwrap();

    assert_replay_states_equal("parent_first", &parent_first, "child_first", &child_first);
    assert_dag_consistent_for_tests(&parent_first);
    assert_dag_consistent_for_tests(&child_first);
}
