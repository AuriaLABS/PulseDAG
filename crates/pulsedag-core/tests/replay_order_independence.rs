use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::{
    accept_block_with_result, adopt_ready_orphans, build_candidate_block,
    build_coinbase_transaction, missing_block_parents, queue_orphan_block,
    refresh_block_consensus_ids, AcceptSource, Block, BlockAcceptanceResult, ChainState, Hash,
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

fn normalized_utxo(utxo: &Utxo) -> NormalizedUtxo {
    NormalizedUtxo {
        address: utxo.address.clone(),
        amount: utxo.amount,
        coinbase: utxo.coinbase,
        height: utxo.height,
    }
}

fn test_block(hash: &str, parents: Vec<Hash>, height: u64, timestamp: u64, nonce: u64) -> Block {
    let txs = vec![build_coinbase_transaction(
        &format!("miner-{hash}"),
        50,
        nonce,
    )];
    let mut block = build_candidate_block(parents, height, 1, txs);
    block.header.timestamp = timestamp;
    block.header.nonce = nonce;
    block.header.blue_score = height;
    block.header.state_root = format!("state-{hash}");
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

fn parent_child_blocks(genesis: &str) -> (Block, Block) {
    let parent = test_block("replay-parent-a", vec![genesis.to_string()], 1, 10, 11);
    let child = test_block("replay-child-b", vec![parent.hash.clone()], 2, 20, 12);
    (parent, child)
}

fn sibling_merge_blocks(genesis: &str) -> (Block, Block, Block) {
    let sibling_a1 = test_block("replay-sibling-a1", vec![genesis.to_string()], 1, 10, 21);
    let sibling_a2 = test_block("replay-sibling-a2", vec![genesis.to_string()], 1, 10, 22);
    let merge = test_block(
        "replay-merge-multi-parent",
        vec![sibling_a1.hash.clone(), sibling_a2.hash.clone()],
        2,
        20,
        23,
    );
    (sibling_a1, sibling_a2, merge)
}

#[test]
fn replay_parent_child_is_equivalent_when_child_arrives_as_orphan_first() {
    let mut parent_then_child = init_chain_state("replay-parent-then-child".to_string());
    let (parent, child) = parent_child_blocks(&parent_then_child.dag.genesis_hash);
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
        normalize_chain_state_for_comparison(&parent_then_child),
        normalize_chain_state_for_comparison(&child_orphan_then_parent)
    );
}

#[test]
fn replay_sibling_order_and_multi_parent_merge_are_equivalent() {
    let mut a1_then_a2 = init_chain_state("replay-a1-then-a2".to_string());
    let (a1, a2, merge) = sibling_merge_blocks(&a1_then_a2.dag.genesis_hash);
    accept_valid(&mut a1_then_a2, a1.clone());
    accept_valid(&mut a1_then_a2, a2.clone());
    accept_valid(&mut a1_then_a2, merge.clone());

    let mut a2_then_a1 = init_chain_state("replay-a2-then-a1".to_string());
    accept_valid(&mut a2_then_a1, a2);
    accept_valid(&mut a2_then_a1, a1);
    accept_valid(&mut a2_then_a1, merge);

    assert_eq!(
        normalize_chain_state_for_comparison(&a1_then_a2),
        normalize_chain_state_for_comparison(&a2_then_a1)
    );
}

#[test]
fn replay_invalid_intermediate_block_does_not_change_final_valid_state() {
    let mut clean_replay = init_chain_state("replay-clean".to_string());
    let (a1, a2, merge) = sibling_merge_blocks(&clean_replay.dag.genesis_hash);
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
        normalize_chain_state_for_comparison(&clean_replay),
        normalize_chain_state_for_comparison(&invalid_then_valid)
    );
}
