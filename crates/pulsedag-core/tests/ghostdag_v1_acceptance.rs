use std::collections::BTreeMap;

use pulsedag_core::{
    calculate_selected_tip_v1, canonicalize_block_parents_v2, classify_merge_set_v1,
    compute_block_hash_v2, genesis::init_chain_state, missing_block_parents, queue_orphan_block,
    rebuild_orphan_parent_index, rebuild_selected_chain_v1, Block, BlockHeader, ChainState, Hash,
    Transaction, BLOCK_HEADER_VERSION_V2,
};
use serde::Serialize;

const CHAIN_ID: &str = "task25-acceptance";
const LABELS: [&str; 6] = ["p0", "p1", "p2", "p3", "merge", "side"];

#[derive(Debug, Clone)]
struct FixtureBlock {
    block: Block,
}

#[derive(Debug, Clone, Copy)]
enum DrainOrder {
    Ascending,
    Descending,
}

#[derive(Debug)]
struct MaterializedFixture {
    state: ChainState,
    classification_digests: BTreeMap<Hash, String>,
    staged_before_ready: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AcceptanceSnapshot {
    accepted_hashes: Vec<Hash>,
    selected_parents: BTreeMap<Hash, Option<Hash>>,
    selected_tip: Option<Hash>,
    selected_chain: Vec<Hash>,
    merge_set_blues: BTreeMap<Hash, Vec<Hash>>,
    merge_set_reds: BTreeMap<Hash, Vec<Hash>>,
    classification_digests: BTreeMap<Hash, String>,
    blue_scores: BTreeMap<Hash, u64>,
    blue_work: BTreeMap<Hash, u128>,
}

fn v2_block_for_state(
    state: &ChainState,
    label: &str,
    height: u64,
    parents: Vec<Hash>,
    nonce: u64,
) -> Block {
    let parents =
        canonicalize_block_parents_v2(&parents).expect("fixture parents must be canonical");
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents,
            timestamp: 1_800_000_000_u64.saturating_add(height),
            difficulty: 1,
            nonce,
            merkle_root: format!("merkle-{label}"),
            state_root: format!("state-{label}"),
            blue_score: 0,
            height,
        },
        transactions: Vec::<Transaction>::new(),
    };
    let classification = classify_merge_set_v1(&block, state)
        .unwrap_or_else(|error| panic!("fixture score derivation failed for {label}: {error:?}"));
    block.header.blue_score = classification.blue_score;
    block.hash =
        compute_block_hash_v2(&block.header, CHAIN_ID).expect("fixture header must be valid v2");
    block
}

fn accept_fixture_block(
    state: &mut ChainState,
    block: Block,
    classification_digests: &mut BTreeMap<Hash, String>,
) {
    let hash = block.hash.clone();
    let classification = classify_merge_set_v1(&block, state)
        .unwrap_or_else(|error| panic!("classification failed for {hash}: {error:?}"));
    assert_eq!(
        block.header.blue_score, classification.blue_score,
        "header blue score must be derived from the same v1 classification inputs"
    );

    for parent in &block.header.parents {
        state
            .dag
            .children
            .entry(parent.clone())
            .or_default()
            .push(hash.clone());
        if let Some(children) = state.dag.children.get_mut(parent) {
            children.sort();
            children.dedup();
        }
        state.dag.tips.remove(parent);
    }
    state.dag.tips.insert(hash.clone());
    state.dag.best_height = state.dag.best_height.max(block.header.height);
    state
        .dag
        .selected_parents
        .insert(hash.clone(), classification.selected_parent.clone());
    state
        .dag
        .merge_set_blues
        .insert(hash.clone(), classification.blues.clone());
    state
        .dag
        .merge_set_reds
        .insert(hash.clone(), classification.reds.clone());
    state
        .dag
        .blue_work
        .insert(hash.clone(), classification.blue_work);
    state.dag.blocks.insert(hash.clone(), block);
    classification_digests.insert(hash, classification.classification_digest);
}

fn fixture_blocks(genesis: &Hash) -> BTreeMap<&'static str, FixtureBlock> {
    let mut planning_state = init_chain_state(CHAIN_ID.to_string());
    assert_eq!(&planning_state.dag.genesis_hash, genesis);
    let mut planning_digests = BTreeMap::new();

    let p0 = v2_block_for_state(&planning_state, "p0", 1, vec![genesis.clone()], 10);
    accept_fixture_block(&mut planning_state, p0.clone(), &mut planning_digests);
    let p1 = v2_block_for_state(&planning_state, "p1", 1, vec![genesis.clone()], 11);
    accept_fixture_block(&mut planning_state, p1.clone(), &mut planning_digests);
    let p2 = v2_block_for_state(&planning_state, "p2", 1, vec![genesis.clone()], 12);
    accept_fixture_block(&mut planning_state, p2.clone(), &mut planning_digests);
    let p3 = v2_block_for_state(&planning_state, "p3", 1, vec![genesis.clone()], 13);
    accept_fixture_block(&mut planning_state, p3.clone(), &mut planning_digests);

    let selected_parallel_parent = [p0.hash.clone(), p1.hash.clone()]
        .into_iter()
        .min()
        .unwrap();

    let merge = v2_block_for_state(
        &planning_state,
        "merge",
        2,
        vec![
            p0.hash.clone(),
            p1.hash.clone(),
            p2.hash.clone(),
            p3.hash.clone(),
        ],
        20,
    );
    accept_fixture_block(&mut planning_state, merge.clone(), &mut planning_digests);
    let side = v2_block_for_state(
        &planning_state,
        "side",
        2,
        vec![selected_parallel_parent],
        21,
    );

    BTreeMap::from([
        ("p0", FixtureBlock { block: p0 }),
        ("p1", FixtureBlock { block: p1 }),
        ("p2", FixtureBlock { block: p2 }),
        ("p3", FixtureBlock { block: p3 }),
        ("merge", FixtureBlock { block: merge }),
        ("side", FixtureBlock { block: side }),
    ])
}

fn block_is_ready(state: &ChainState, block: &Block) -> bool {
    missing_block_parents(block, state).is_empty()
}

fn take_staged_orphan(state: &mut ChainState, hash: &Hash) -> Block {
    let block = state
        .orphan_blocks
        .remove(hash)
        .unwrap_or_else(|| panic!("ready orphan {hash} must still be queued"));
    state.orphan_missing_parents.remove(hash);
    state.orphan_received_at_ms.remove(hash);
    rebuild_orphan_parent_index(state);
    block
}

fn drain_ready(
    state: &mut ChainState,
    order: DrainOrder,
    classification_digests: &mut BTreeMap<Hash, String>,
) {
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
            let block = take_staged_orphan(state, &hash);
            accept_fixture_block(state, block, classification_digests);
        }
    }
}

fn materialize(
    fixture: &BTreeMap<&'static str, FixtureBlock>,
    arrival: &[&'static str],
    drain_order: DrainOrder,
) -> MaterializedFixture {
    let mut state = init_chain_state(CHAIN_ID.to_string());
    let mut classification_digests = BTreeMap::new();
    let mut staged_before_ready = 0;

    for label in arrival {
        let incoming = fixture
            .get(label)
            .unwrap_or_else(|| panic!("unknown fixture label {label}"))
            .block
            .clone();
        let missing = missing_block_parents(&incoming, &state);
        if missing.is_empty() {
            accept_fixture_block(&mut state, incoming, &mut classification_digests);
        } else {
            staged_before_ready += 1;
            let hash = incoming.hash.clone();
            assert!(queue_orphan_block(&mut state, incoming, missing.clone()));
            assert!(state.orphan_blocks.contains_key(&hash));
            assert!(
                !state.dag.blocks.contains_key(&hash),
                "staged orphan must not appear in accepted DAG blocks"
            );
            assert!(
                !state.dag.selected_parents.contains_key(&hash),
                "real orphan staging must not finalize selected-parent metadata"
            );
            assert!(
                !state.dag.merge_set_blues.contains_key(&hash)
                    && !state.dag.merge_set_reds.contains_key(&hash),
                "real orphan staging must not finalize blue/red metadata"
            );
            assert!(
                !state.dag.blue_work.contains_key(&hash),
                "real orphan staging must not finalize blue-work metadata"
            );
            for parent in missing {
                assert!(
                    state
                        .orphan_parent_index
                        .get(&parent)
                        .map(|waiting| waiting.contains(&hash))
                        .unwrap_or(false),
                    "staged orphan must be indexed by each missing parent"
                );
            }
        }
        drain_ready(&mut state, drain_order, &mut classification_digests);
    }

    drain_ready(&mut state, drain_order, &mut classification_digests);
    assert!(
        state.orphan_blocks.is_empty(),
        "all fixture orphans must eventually drain"
    );
    assert!(
        state.orphan_parent_index.is_empty() && state.orphan_missing_parents.is_empty(),
        "orphan indexes must be empty after the complete block set arrives"
    );

    let selected_tip = calculate_selected_tip_v1(&state).expect("selected tip must be complete");
    state.selected_chain_from_tip(selected_tip);

    MaterializedFixture {
        state,
        classification_digests,
        staged_before_ready,
    }
}

trait SelectedChainFixtureExt {
    fn selected_chain_from_tip(&mut self, selected_tip: Option<Hash>);
}

impl SelectedChainFixtureExt for ChainState {
    fn selected_chain_from_tip(&mut self, selected_tip: Option<Hash>) {
        self.dag.selected_chain = rebuild_selected_chain_v1(self, selected_tip)
            .expect("fixture selected-parent metadata must be complete");
    }
}

fn acceptance_snapshot(materialized: &MaterializedFixture) -> AcceptanceSnapshot {
    let selected_tip =
        calculate_selected_tip_v1(&materialized.state).expect("selected tip must recompute");
    let selected_chain = rebuild_selected_chain_v1(&materialized.state, selected_tip.clone())
        .expect("selected chain must recompute");

    let mut accepted_hashes = materialized
        .state
        .dag
        .blocks
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    accepted_hashes.sort();

    AcceptanceSnapshot {
        accepted_hashes,
        selected_parents: materialized
            .state
            .dag
            .selected_parents
            .iter()
            .map(|(hash, parent)| (hash.clone(), parent.clone()))
            .collect(),
        selected_tip,
        selected_chain,
        merge_set_blues: materialized
            .state
            .dag
            .merge_set_blues
            .iter()
            .map(|(hash, blues)| (hash.clone(), blues.clone()))
            .collect(),
        merge_set_reds: materialized
            .state
            .dag
            .merge_set_reds
            .iter()
            .map(|(hash, reds)| (hash.clone(), reds.clone()))
            .collect(),
        classification_digests: materialized.classification_digests.clone(),
        blue_scores: materialized
            .state
            .dag
            .blocks
            .iter()
            .map(|(hash, block)| (hash.clone(), block.header.blue_score))
            .collect(),
        blue_work: materialized
            .state
            .dag
            .blue_work
            .iter()
            .map(|(hash, work)| (hash.clone(), *work))
            .collect(),
    }
}

fn recompute_classification_digests(state: &ChainState) -> BTreeMap<Hash, String> {
    let mut hashes = state.dag.blocks.keys().cloned().collect::<Vec<_>>();
    hashes.sort();
    hashes
        .into_iter()
        .filter(|hash| hash != &state.dag.genesis_hash)
        .map(|hash| {
            let block = state.dag.blocks.get(&hash).expect("block must exist");
            let classification = classify_merge_set_v1(block, state)
                .unwrap_or_else(|error| panic!("reclassification failed for {hash}: {error:?}"));
            assert_eq!(block.header.blue_score, classification.blue_score);
            assert_eq!(
                state.dag.blue_work.get(&hash).copied(),
                Some(classification.blue_work),
                "restart recomputation must reproduce blue-work metadata"
            );
            (hash, classification.classification_digest)
        })
        .collect()
}

fn permutations(values: &mut [&'static str], start: usize, out: &mut Vec<Vec<&'static str>>) {
    if start == values.len() {
        out.push(values.to_vec());
        return;
    }
    for index in start..values.len() {
        values.swap(start, index);
        permutations(values, start + 1, out);
        values.swap(start, index);
    }
}

#[test]
fn same_block_set_is_byte_identical_across_all_arrival_and_orphan_drain_orders() {
    let initial = init_chain_state(CHAIN_ID.to_string());
    let fixture = fixture_blocks(&initial.dag.genesis_hash);
    let canonical_arrival = LABELS.to_vec();
    let baseline_materialized = materialize(&fixture, &canonical_arrival, DrainOrder::Ascending);
    let baseline = acceptance_snapshot(&baseline_materialized);
    let baseline_bytes = serde_json::to_vec(&baseline).expect("acceptance snapshot must serialize");

    let merge_hash = &fixture["merge"].block.hash;
    let expected_parent = [
        fixture["p0"].block.hash.clone(),
        fixture["p1"].block.hash.clone(),
        fixture["p2"].block.hash.clone(),
        fixture["p3"].block.hash.clone(),
    ]
    .into_iter()
    .min()
    .unwrap();
    assert_eq!(
        baseline.selected_parents.get(merge_hash),
        Some(&Some(expected_parent))
    );
    assert_eq!(baseline.merge_set_blues[merge_hash].len(), 2);
    assert_eq!(baseline.merge_set_reds[merge_hash].len(), 1);
    assert_eq!(baseline.blue_scores[merge_hash], 4);
    assert_eq!(baseline.blue_work[merge_hash], 4);
    assert_eq!(baseline.selected_tip, Some(merge_hash.clone()));

    let mut values = LABELS;
    let mut arrival_permutations = Vec::new();
    permutations(&mut values, 0, &mut arrival_permutations);
    assert_eq!(arrival_permutations.len(), 720);

    let mut saw_staged_orphan = false;
    for arrival in arrival_permutations {
        for drain_order in [DrainOrder::Ascending, DrainOrder::Descending] {
            let materialized = materialize(&fixture, &arrival, drain_order);
            saw_staged_orphan |= materialized.staged_before_ready > 0;
            let snapshot = acceptance_snapshot(&materialized);
            assert_eq!(
                snapshot, baseline,
                "arrival={arrival:?} drain={drain_order:?}"
            );
            assert_eq!(
                serde_json::to_vec(&snapshot).expect("acceptance snapshot must serialize"),
                baseline_bytes,
                "canonical acceptance bytes diverged for arrival={arrival:?} drain={drain_order:?}"
            );
        }
    }
    assert!(
        saw_staged_orphan,
        "fixture matrix must exercise real orphan staging"
    );
}

#[test]
fn restart_round_trip_recomputes_identical_selection_and_classification_metadata() {
    let initial = init_chain_state(CHAIN_ID.to_string());
    let fixture = fixture_blocks(&initial.dag.genesis_hash);
    let reverse = LABELS.into_iter().rev().collect::<Vec<_>>();
    let before = materialize(&fixture, &reverse, DrainOrder::Descending);
    assert!(before.staged_before_ready > 0);
    let baseline = acceptance_snapshot(&before);

    let encoded = bincode::serialize(&before.state).expect("chain state must serialize");
    let restored: ChainState = bincode::deserialize(&encoded).expect("chain state must restore");
    let recomputed_digests = recompute_classification_digests(&restored);
    let after = MaterializedFixture {
        state: restored,
        classification_digests: recomputed_digests,
        staged_before_ready: 0,
    };

    assert_eq!(acceptance_snapshot(&after), baseline);
}
