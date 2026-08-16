use pulsedag_core::{
    calculate_bounded_merge_set_v1, genesis::init_chain_state, Block, BlockHeader, ChainState,
    GhostdagV1Error, Transaction, BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_MAX_ANCESTOR_VISITS,
    GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS,
};

fn synthetic_block(hash: String, parent: String, height: u64) -> Block {
    Block {
        hash: hash.clone(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: vec![parent],
            timestamp: 1_900_000_000_u64.saturating_add(height),
            difficulty: 1,
            nonce: height,
            merkle_root: format!("merkle-{hash}"),
            state_root: format!("state-{hash}"),
            blue_score: height,
            height,
        },
        transactions: Vec::<Transaction>::new(),
    }
}

fn insert_linear_branch(
    state: &mut ChainState,
    prefix: &str,
    start_parent: String,
    count: usize,
    work_base: u128,
) -> Vec<String> {
    let mut parent = start_parent;
    let mut hashes = Vec::with_capacity(count);
    for index in 0..count {
        let height = u64::try_from(index + 1).expect("fixture height fits u64");
        let hash = format!("{prefix}-{index:06}");
        let block = synthetic_block(hash.clone(), parent, height);
        state.dag.blocks.insert(hash.clone(), block);
        state
            .dag
            .blue_work
            .insert(hash.clone(), work_base.saturating_add(index as u128));
        parent = hash.clone();
        hashes.push(hash);
    }
    hashes
}

fn candidate(parents: Vec<String>, height: u64) -> Block {
    Block {
        hash: format!("candidate-{height}"),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents,
            timestamp: 2_000_000_000_u64.saturating_add(height),
            difficulty: 1,
            nonce: height,
            merkle_root: format!("candidate-merkle-{height}"),
            state_root: format!("candidate-state-{height}"),
            blue_score: 0,
            height,
        },
        transactions: Vec::new(),
    }
}

#[test]
fn real_ancestor_visit_boundary_accepts_exact_limit_and_rejects_one_more() {
    let mut state = init_chain_state("task25-real-ancestor-limit".to_string());
    let genesis = state.dag.genesis_hash.clone();
    let chain = insert_linear_branch(
        &mut state,
        "deep",
        genesis,
        GHOSTDAG_V1_MAX_ANCESTOR_VISITS,
        10_000,
    );

    // The selected-parent past contains the genesis plus every synthetic block
    // up to the chosen parent. Selecting index MAX-2 therefore consumes exactly
    // the frozen MAX ancestor visits and must still succeed.
    let exact_parent = chain[GHOSTDAG_V1_MAX_ANCESTOR_VISITS - 2].clone();
    let exact_candidate = candidate(vec![exact_parent], GHOSTDAG_V1_MAX_ANCESTOR_VISITS as u64);
    let exact = calculate_bounded_merge_set_v1(&exact_candidate, &state)
        .expect("exact frozen ancestor-visit budget must be accepted");
    assert_eq!(exact.ancestor_visits, GHOSTDAG_V1_MAX_ANCESTOR_VISITS);
    assert!(exact.merge_set.is_empty());

    // One additional ancestor requires MAX+1 visits and must fail closed with
    // the exact frozen limit in the error, rather than truncating traversal.
    let overflow_parent = chain[GHOSTDAG_V1_MAX_ANCESTOR_VISITS - 1].clone();
    let overflow_candidate = candidate(
        vec![overflow_parent],
        GHOSTDAG_V1_MAX_ANCESTOR_VISITS as u64 + 1,
    );
    assert_eq!(
        calculate_bounded_merge_set_v1(&overflow_candidate, &state),
        Err(GhostdagV1Error::AncestorVisitLimitExceeded {
            max: GHOSTDAG_V1_MAX_ANCESTOR_VISITS,
        })
    );
}

#[test]
fn real_merge_set_boundary_accepts_4096_and_rejects_4097() {
    let mut state = init_chain_state("task25-real-merge-set-limit".to_string());
    let genesis = state.dag.genesis_hash.clone();

    let selected_hash = "selected-parent".to_string();
    let selected = synthetic_block(selected_hash.clone(), genesis.clone(), 1);
    state.dag.blocks.insert(selected_hash.clone(), selected);
    state.dag.blue_work.insert(selected_hash.clone(), 1_000_000);

    let side = insert_linear_branch(
        &mut state,
        "side",
        genesis,
        GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS + 1,
        100,
    );

    // The side branch contributes exactly 4,096 blocks outside the selected
    // parent's past. This is the frozen v1 maximum and must be accepted.
    let exact_candidate = candidate(
        vec![
            selected_hash.clone(),
            side[GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS - 1].clone(),
        ],
        (GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS + 2) as u64,
    );
    let exact = calculate_bounded_merge_set_v1(&exact_candidate, &state)
        .expect("exact frozen merge-set size must be accepted");
    assert_eq!(exact.selected_parent, Some(selected_hash.clone()));
    assert_eq!(exact.merge_set.len(), GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS);

    // Advancing the same branch by one block creates a 4,097-member merge set
    // and must fail closed with an explicit observed/max pair.
    let overflow_candidate = candidate(
        vec![
            selected_hash,
            side[GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS].clone(),
        ],
        (GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS + 3) as u64,
    );
    assert_eq!(
        calculate_bounded_merge_set_v1(&overflow_candidate, &state),
        Err(GhostdagV1Error::MergeSetSizeLimitExceeded {
            observed: GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS + 1,
            max: GHOSTDAG_V1_MAX_MERGE_SET_BLOCKS,
        })
    );
}
