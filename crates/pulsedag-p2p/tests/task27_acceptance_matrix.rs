use std::collections::BTreeSet;

use pulsedag_core::{
    canonicalize_block_parents_v2, classify_merge_set_v1,
    commit_ghostdag_v1_metadata_for_activated_v2, compute_block_hash_v2, derive_ordered_dag_v2,
    genesis::init_chain_state, merge_set_digest, missing_block_parents, queue_orphan_block,
    rebuild_authoritative_state_v2, rebuild_orphan_parent_index, selection_digest, Block,
    BlockHeader, ChainState, Hash, ProtocolActivationIdentity, Transaction, TxOutput,
    BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V2,
};
use pulsedag_p2p::messages::{
    build_dag_frontier_response_v1, build_selected_chain_locator_v1,
    plan_dag_frontier_reconciliation_v1, DagFrontierResponseV1,
};

const CHAIN_ID: &str = "task27-acceptance-matrix";

#[derive(Debug, Clone)]
struct Fixture {
    source: ChainState,
    fund: Block,
    left1: Block,
    left2: Block,
    right1: Block,
    right2: Block,
    merge: Block,
    side: Block,
}

#[derive(Debug, Clone, Copy)]
enum DeliveryOrder {
    Planned,
    Reverse,
    EvenOdd,
    OddEven,
}

#[derive(Debug, Clone, Copy)]
enum DrainOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConvergenceSnapshot {
    selected_chain: Vec<Hash>,
    tips: BTreeSet<Hash>,
    selection_digest: String,
    merge_set_digest: String,
    ordered_blocks: Vec<Hash>,
    ordered_digest: String,
    state_root: String,
}

fn activated_state() -> ChainState {
    let mut state = init_chain_state(CHAIN_ID.to_string());
    state.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    state
}

fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn funding_transaction() -> Transaction {
    Transaction {
        txid: "task27-funding-v2".to_string(),
        version: TRANSACTION_VERSION_V2,
        inputs: Vec::new(),
        outputs: vec![TxOutput {
            address: "task27-funded".to_string(),
            amount: 25,
        }],
        fee: 0,
        nonce: 0,
    }
}

fn candidate(
    state: &ChainState,
    label: &str,
    parents: Vec<Hash>,
    nonce: u64,
    transactions: Vec<Transaction>,
) -> Block {
    let parents = canonicalize_block_parents_v2(&parents).expect("canonical test parents");
    let height = parents
        .iter()
        .map(|parent| {
            state
                .dag
                .blocks
                .get(parent)
                .unwrap_or_else(|| panic!("fixture parent {parent} must exist"))
                .header
                .height
                .saturating_add(1)
        })
        .max()
        .expect("post-genesis candidate has parents");
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents,
            timestamp: 2_200_000_000_u64.saturating_add(height),
            difficulty: 1,
            nonce,
            merkle_root: format!("merkle-{label}-{nonce}"),
            state_root: format!("state-{label}-{nonce}"),
            blue_score: 0,
            height,
        },
        transactions,
    };
    let classification = classify_merge_set_v1(&block, state).expect("fixture classification");
    block.header.blue_score = classification.blue_score;
    block.hash = compute_block_hash_v2(&block.header, CHAIN_ID).expect("fixture block hash");
    block
}

fn commit_ready(state: &mut ChainState, block: Block) {
    let identity = activated_identity(state);
    commit_ghostdag_v1_metadata_for_activated_v2(&block, state, &identity)
        .expect("fixture block must commit through activated-v2 metadata path");
}

fn build_fixture() -> Fixture {
    let mut source = activated_state();
    let genesis = source.dag.genesis_hash.clone();

    let fund = candidate(
        &source,
        "fund",
        vec![genesis],
        1,
        vec![funding_transaction()],
    );
    commit_ready(&mut source, fund.clone());

    let left1 = candidate(&source, "left1", vec![fund.hash.clone()], 11, Vec::new());
    let right1 = candidate(&source, "right1", vec![fund.hash.clone()], 12, Vec::new());
    commit_ready(&mut source, left1.clone());
    commit_ready(&mut source, right1.clone());

    let left2 = candidate(&source, "left2", vec![left1.hash.clone()], 21, Vec::new());
    let right2 = candidate(
        &source,
        "right2",
        vec![right1.hash.clone()],
        22,
        Vec::new(),
    );
    commit_ready(&mut source, left2.clone());
    commit_ready(&mut source, right2.clone());

    let merge = candidate(
        &source,
        "merge",
        vec![left2.hash.clone(), right2.hash.clone()],
        31,
        Vec::new(),
    );
    let side = candidate(&source, "side", vec![right2.hash.clone()], 32, Vec::new());
    commit_ready(&mut source, merge.clone());
    commit_ready(&mut source, side.clone());

    assert_eq!(
        source.dag.selected_chain.last(),
        Some(&merge.hash),
        "fixture must keep the merge block as selected tip"
    );
    assert_eq!(source.dag.tips.len(), 2);
    assert!(source.dag.tips.contains(&merge.hash));
    assert!(source.dag.tips.contains(&side.hash));

    Fixture {
        source,
        fund,
        left1,
        left2,
        right1,
        right2,
        merge,
        side,
    }
}

fn block_is_ready(state: &ChainState, block: &Block) -> bool {
    missing_block_parents(block, state).is_empty()
}

fn take_ready_orphan(state: &mut ChainState, hash: &Hash) -> Block {
    let block = state
        .orphan_blocks
        .remove(hash)
        .unwrap_or_else(|| panic!("ready orphan {hash} must remain staged until drain"));
    state.orphan_missing_parents.remove(hash);
    state.orphan_received_at_ms.remove(hash);
    rebuild_orphan_parent_index(state);
    block
}

fn drain_ready_orphans(state: &mut ChainState, order: DrainOrder) {
    loop {
        let mut ready = state
            .orphan_blocks
            .iter()
            .filter_map(|(hash, block)| block_is_ready(state, block).then_some(hash.clone()))
            .collect::<Vec<_>>();
        if ready.is_empty() {
            break;
        }
        ready.sort();
        if matches!(order, DrainOrder::Descending) {
            ready.reverse();
        }
        for hash in ready {
            let block = take_ready_orphan(state, &hash);
            commit_ready(state, block);
        }
    }
}

fn reorder(mut hashes: Vec<Hash>, order: DeliveryOrder) -> Vec<Hash> {
    match order {
        DeliveryOrder::Planned => hashes,
        DeliveryOrder::Reverse => {
            hashes.reverse();
            hashes
        }
        DeliveryOrder::EvenOdd | DeliveryOrder::OddEven => {
            let first_parity = usize::from(matches!(order, DeliveryOrder::OddEven));
            let mut out = Vec::with_capacity(hashes.len());
            for parity in [first_parity, 1 - first_parity] {
                out.extend(
                    hashes
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| index % 2 == parity)
                        .map(|(_, hash)| hash.clone()),
                );
            }
            out
        }
    }
}

fn deliver_requested_blocks(
    source: &ChainState,
    receiver: &mut ChainState,
    request_hashes: Vec<Hash>,
    delivery_order: DeliveryOrder,
    drain_order: DrainOrder,
) -> usize {
    let mut staged = 0usize;
    for hash in reorder(request_hashes, delivery_order) {
        if receiver.dag.blocks.contains_key(&hash) {
            continue;
        }
        let incoming = source
            .dag
            .blocks
            .get(&hash)
            .unwrap_or_else(|| panic!("source must contain requested block {hash}"))
            .clone();
        let missing = missing_block_parents(&incoming, receiver);
        if missing.is_empty() {
            commit_ready(receiver, incoming);
        } else {
            staged = staged.saturating_add(1);
            assert!(queue_orphan_block(receiver, incoming, missing));
        }
        drain_ready_orphans(receiver, drain_order);
    }
    drain_ready_orphans(receiver, drain_order);
    assert!(receiver.orphan_blocks.is_empty(), "all Task 27 context must resolve");
    assert!(receiver.orphan_parent_index.is_empty());
    assert!(receiver.orphan_missing_parents.is_empty());
    staged
}

fn sync_once(
    source: &ChainState,
    receiver: &mut ChainState,
    delivery_order: DeliveryOrder,
    drain_order: DrainOrder,
) -> (DagFrontierResponseV1, usize) {
    let identity = activated_identity(source);
    assert_eq!(activated_identity(receiver), identity);
    let locator = build_selected_chain_locator_v1(identity.clone(), &receiver.dag.selected_chain)
        .expect("receiver selected-chain locator");
    let response = build_dag_frontier_response_v1(&identity, &locator, source)
        .expect("valid source frontier")
        .expect("retained common ancestor");
    let known = receiver
        .dag
        .blocks
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let plan = plan_dag_frontier_reconciliation_v1(&identity, &response, &known)
        .expect("valid deterministic reconciliation plan");
    let staged = deliver_requested_blocks(
        source,
        receiver,
        plan.request_hashes,
        delivery_order,
        drain_order,
    );
    (response, staged)
}

fn snapshot(state: &ChainState) -> ConvergenceSnapshot {
    let ordered = derive_ordered_dag_v2(state).expect("canonical v2 ordering");
    let replay = rebuild_authoritative_state_v2(state).expect("canonical v2 state replay");
    assert_eq!(replay.ordered_dag, ordered);
    ConvergenceSnapshot {
        selected_chain: state.dag.selected_chain.clone(),
        tips: state.dag.tips.iter().cloned().collect(),
        selection_digest: selection_digest(state),
        merge_set_digest: merge_set_digest(state),
        ordered_blocks: ordered.blocks,
        ordered_digest: ordered.digest,
        state_root: replay.diagnostics.state_root,
    }
}

fn assert_frontier_metadata_matches(receiver: &ChainState, response: &DagFrontierResponseV1) {
    for entry in &response.frontier {
        assert_eq!(
            receiver.dag.selected_parents.get(&entry.hash),
            Some(&entry.consensus.selected_parent),
            "selected-parent mismatch for {}",
            entry.hash
        );
        assert_eq!(
            receiver.dag.blue_work.get(&entry.hash).map(u128::to_string),
            Some(entry.consensus.blue_work_decimal.clone()),
            "blue-work mismatch for {}",
            entry.hash
        );
        let mut blues = receiver
            .dag
            .merge_set_blues
            .get(&entry.hash)
            .cloned()
            .unwrap_or_default();
        let mut reds = receiver
            .dag
            .merge_set_reds
            .get(&entry.hash)
            .cloned()
            .unwrap_or_default();
        blues.sort();
        reds.sort();
        assert_eq!(blues, entry.consensus.merge_set_blues);
        assert_eq!(reds, entry.consensus.merge_set_reds);
    }
}

#[test]
fn clean_node_catchup_fetches_transitive_context_and_converges() {
    let fixture = build_fixture();
    let mut clean = activated_state();
    let (response, staged) = sync_once(
        &fixture.source,
        &mut clean,
        DeliveryOrder::Reverse,
        DrainOrder::Descending,
    );

    let selected_left = fixture
        .source
        .dag
        .selected_chain
        .iter()
        .any(|hash| hash == &fixture.left2.hash);
    let (context_parent, context_tip) = if selected_left {
        (&fixture.right1.hash, &fixture.right2.hash)
    } else {
        (&fixture.left1.hash, &fixture.left2.hash)
    };
    assert!(response.required_context.contains(context_parent));
    assert!(response.required_context.contains(context_tip));
    assert!(staged > 0, "reverse delivery must exercise missing-parent staging");
    assert_frontier_metadata_matches(&clean, &response);
    assert_eq!(snapshot(&clean), snapshot(&fixture.source));
}

#[test]
fn delivery_order_permutations_converge_to_identical_selected_frontier_order_and_state() {
    let fixture = build_fixture();
    let expected = snapshot(&fixture.source);
    let mut staged_total = 0usize;

    for (delivery, drain) in [
        (DeliveryOrder::Planned, DrainOrder::Ascending),
        (DeliveryOrder::Reverse, DrainOrder::Descending),
        (DeliveryOrder::EvenOdd, DrainOrder::Descending),
        (DeliveryOrder::OddEven, DrainOrder::Ascending),
    ] {
        let mut receiver = activated_state();
        let (_, staged) = sync_once(&fixture.source, &mut receiver, delivery, drain);
        staged_total = staged_total.saturating_add(staged);
        assert_eq!(snapshot(&receiver), expected);
    }

    assert!(staged_total > 0, "permutation matrix must exercise orphan recovery");
}

#[test]
fn clean_offline_and_same_height_nodes_converge_without_reset() {
    let fixture = build_fixture();
    let expected = snapshot(&fixture.source);

    let mut clean = activated_state();

    let mut offline = activated_state();
    for block in [&fixture.fund, &fixture.left1, &fixture.right1] {
        commit_ready(&mut offline, block.clone());
    }
    let offline_before = offline.dag.blocks.len();

    let mut same_height = activated_state();
    for block in [
        &fixture.fund,
        &fixture.right1,
        &fixture.right2,
        &fixture.side,
    ] {
        commit_ready(&mut same_height, block.clone());
    }
    let source_tip = fixture
        .source
        .dag
        .selected_chain
        .last()
        .expect("source selected tip");
    let divergent_tip = same_height
        .dag
        .selected_chain
        .last()
        .expect("same-height selected tip");
    assert_ne!(source_tip, divergent_tip);
    assert_eq!(
        fixture.source.dag.blocks[source_tip].header.height,
        same_height.dag.blocks[divergent_tip].header.height,
        "fixture must exercise same-height divergence"
    );

    for (receiver, delivery, drain) in [
        (&mut clean, DeliveryOrder::Reverse, DrainOrder::Ascending),
        (&mut offline, DeliveryOrder::EvenOdd, DrainOrder::Descending),
        (&mut same_height, DeliveryOrder::OddEven, DrainOrder::Ascending),
    ] {
        sync_once(&fixture.source, receiver, delivery, drain);
        assert_eq!(snapshot(receiver), expected);
    }

    assert!(
        offline.dag.blocks.len() > offline_before,
        "offline rejoin must advance existing state instead of resetting it"
    );
}

#[test]
fn pruning_boundary_peer_fails_closed_and_retained_peer_can_continue() {
    let fixture = build_fixture();
    let receiver = activated_state();
    let identity = activated_identity(&fixture.source);
    let locator = build_selected_chain_locator_v1(identity.clone(), &receiver.dag.selected_chain)
        .expect("receiver locator");

    let mut pruned = fixture.source.clone();
    pruned.dag.selected_chain = vec![fixture.merge.hash.clone()];
    assert_eq!(
        build_dag_frontier_response_v1(&identity, &locator, &pruned)
            .expect("pruning result is explicit"),
        None,
        "peer with no retained locator overlap must not invent an ancestor"
    );

    let retained = build_dag_frontier_response_v1(&identity, &locator, &fixture.source)
        .expect("retained peer response")
        .expect("retained peer has common ancestor");
    assert_eq!(retained.common_ancestor, receiver.dag.genesis_hash);
}
