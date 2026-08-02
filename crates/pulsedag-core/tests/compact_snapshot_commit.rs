use pulsedag_core::apply::commit_block_to_state;
use pulsedag_core::genesis::init_chain_state;
use pulsedag_core::pow::dev_mine_header;
use pulsedag_core::retarget::expected_difficulty_for_parent;
use pulsedag_core::{
    accept_block_with_result, build_candidate_block, build_coinbase_transaction,
    compact_snapshot_to_retained_blocks, parent_state_context, refresh_block_consensus_ids,
    refresh_block_consensus_ids_with_state, state_digest, AcceptSource, Block,
    BlockAcceptanceResult, ChainState, Hash,
};

fn selected_tip(state: &ChainState) -> Hash {
    state
        .dag
        .selected_chain
        .last()
        .cloned()
        .unwrap_or_else(|| state.dag.genesis_hash.clone())
}

fn block_hash_at_height(state: &ChainState, height: u64) -> Hash {
    state
        .dag
        .blocks
        .values()
        .find(|block| block.header.height == height)
        .map(|block| block.hash.clone())
        .unwrap_or_else(|| panic!("missing block at height {height}"))
}

fn build_mined_block(
    state: &ChainState,
    parent: Hash,
    height: u64,
    timestamp: u64,
    coinbase_nonce: u64,
    miner: &str,
) -> Block {
    let difficulty = expected_difficulty_for_parent(state, &parent)
        .unwrap_or_else(|| panic!("missing retarget context for parent {parent}"));
    let mut block = build_candidate_block(
        vec![parent],
        height,
        difficulty,
        vec![build_coinbase_transaction(miner, 50, coinbase_nonce)],
    );
    block.header.timestamp = timestamp;
    let context = parent_state_context(&block, state)
        .expect("rebuild the exact selected-parent state for the block fixture");
    refresh_block_consensus_ids_with_state(&mut block, &context)
        .expect("prepare state-aware block fixture");
    let (header, mined, _, _) = dev_mine_header(block.header.clone(), 1_000_000);
    assert!(mined, "expected compact snapshot fixture to satisfy PoW");
    block.header = header;
    refresh_block_consensus_ids(&mut block);
    block
}

fn accept(state: &mut ChainState, block: Block) {
    let hash = block.hash.clone();
    let outcome = accept_block_with_result(block, state, AcceptSource::P2p);
    assert_eq!(
        outcome,
        BlockAcceptanceResult::Accepted,
        "expected block {hash} to be accepted"
    );
}

fn build_linear_chain(chain_id: &str, blocks_to_add: u64) -> ChainState {
    let mut state = init_chain_state(chain_id.to_string());
    for height in 1..=blocks_to_add {
        let parent = selected_tip(&state);
        let parent_timestamp = state
            .dag
            .blocks
            .get(&parent)
            .map(|block| block.header.timestamp)
            .unwrap_or(0);
        let block = build_mined_block(
            &state,
            parent,
            height,
            parent_timestamp.saturating_add(60).max(1),
            height,
            "linear-miner",
        );
        accept(&mut state, block);
    }
    state
}

fn compact_last_blocks(state: &ChainState, retained_count: u64) -> ChainState {
    let floor = state
        .dag
        .best_height
        .saturating_sub(retained_count.saturating_sub(1));
    let mut retained = state
        .dag
        .blocks
        .values()
        .filter(|block| block.header.height >= floor)
        .cloned()
        .collect::<Vec<_>>();
    retained.sort_by_key(|block| block.header.height);
    compact_snapshot_to_retained_blocks(state.clone(), &retained)
        .expect("compact the authoritative snapshot")
}

#[test]
fn compact_snapshot_linear_extension_preserves_authoritative_utxo() {
    let baseline = build_linear_chain("compact-linear-extension", 25);
    let mut expected = baseline.clone();
    let parent = selected_tip(&expected);
    let parent_timestamp = expected.dag.blocks[&parent].header.timestamp;
    let extension = build_mined_block(
        &expected,
        parent,
        26,
        parent_timestamp + 60,
        26,
        "extension-miner",
    );
    accept(&mut expected, extension.clone());

    let mut compact = compact_last_blocks(&baseline, 20);
    assert!(!compact.dag.blocks.contains_key(&compact.dag.genesis_hash));
    accept(&mut compact, extension);

    assert_eq!(selected_tip(&compact), selected_tip(&expected));
    assert_eq!(compact.dag.best_height, expected.dag.best_height);
    assert_eq!(
        state_digest(&compact).expect("compact state digest"),
        state_digest(&expected).expect("full state digest")
    );
}

#[test]
fn compact_snapshot_side_parent_validation_rejects_without_mutation() {
    let baseline = build_linear_chain("compact-side-validation", 25);
    let parent = block_hash_at_height(&baseline, 23);
    let parent_timestamp = baseline.dag.blocks[&parent].header.timestamp;
    let side = build_mined_block(
        &baseline,
        parent,
        24,
        parent_timestamp + 1,
        24_001,
        "side-miner",
    );

    let mut compact = compact_last_blocks(&baseline, 20);
    let selected_before = compact.dag.selected_chain.clone();
    let digest_before = state_digest(&compact).expect("digest before side validation");
    let outcome = accept_block_with_result(side.clone(), &mut compact, AcceptSource::P2p);

    match outcome {
        BlockAcceptanceResult::Rejected(reason) => assert!(
            reason.contains("invalid state root")
                || reason.contains("parent state context unavailable"),
            "unexpected compact side-parent rejection: {reason}"
        ),
        other => panic!("expected compact side-parent rejection, got {other:?}"),
    }
    assert!(!compact.dag.blocks.contains_key(&side.hash));
    assert_eq!(compact.dag.selected_chain, selected_before);
    assert_eq!(
        state_digest(&compact).expect("digest after side validation"),
        digest_before
    );
}

#[test]
fn compact_snapshot_reorg_requiring_pruned_history_fails_closed() {
    let baseline = build_linear_chain("compact-reorg-fails-closed", 25);
    let mut branch_builder = baseline.clone();
    let fork_parent = block_hash_at_height(&baseline, 23);
    let fork_timestamp = baseline.dag.blocks[&fork_parent].header.timestamp;

    let side_24 = build_mined_block(
        &branch_builder,
        fork_parent,
        24,
        fork_timestamp + 1,
        24_101,
        "reorg-miner-24",
    );
    accept(&mut branch_builder, side_24.clone());

    let side_25 = build_mined_block(
        &branch_builder,
        side_24.hash.clone(),
        25,
        fork_timestamp + 2,
        25_101,
        "reorg-miner-25",
    );
    accept(&mut branch_builder, side_25.clone());

    let side_26 = build_mined_block(
        &branch_builder,
        side_25.hash.clone(),
        26,
        fork_timestamp + 3,
        26_101,
        "reorg-miner-26",
    );
    accept(&mut branch_builder, side_26.clone());

    let mut compact = compact_last_blocks(&baseline, 20);
    let canonical_digest = state_digest(&compact).expect("canonical compact digest");
    let canonical_selected_chain = compact.dag.selected_chain.clone();

    commit_block_to_state(&side_24, &mut compact).expect("commit non-selected side block 24");
    commit_block_to_state(&side_25, &mut compact).expect("commit non-selected side block 25");
    assert_eq!(compact.dag.selected_chain, canonical_selected_chain);
    assert_eq!(
        state_digest(&compact).expect("digest after non-selected side commits"),
        canonical_digest
    );

    let mut rejected_working = compact.clone();
    let err = commit_block_to_state(&side_26, &mut rejected_working)
        .expect_err("compact reorg must fail closed before publication");
    assert!(
        err.to_string().contains(
            "compact snapshot selected-chain transition requires unavailable historical state"
        ),
        "unexpected compact reorg error: {err}"
    );

    assert!(!compact.dag.blocks.contains_key(&side_26.hash));
    assert_eq!(compact.dag.selected_chain, canonical_selected_chain);
    assert_eq!(
        state_digest(&compact).expect("digest after rejected reorg"),
        canonical_digest
    );
}
