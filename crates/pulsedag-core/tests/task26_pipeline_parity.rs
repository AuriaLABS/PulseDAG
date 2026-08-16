use pulsedag_core::{
    canonicalize_block_parents_v2, classify_merge_set_v1,
    commit_ghostdag_v1_metadata_for_activated_v2, compute_block_hash_v2,
    derive_finality_boundary_v1, derive_ordered_dag_v2, genesis::init_chain_state,
    missing_block_parents, queue_orphan_block, rebuild_authoritative_state_v2,
    rebuild_orphan_parent_index, Block, BlockHeader, ChainState, FinalityBoundaryV1, Hash,
    OutPoint, ProtocolActivationIdentity, Transaction, TxInput, TxOutput, BLOCK_HEADER_VERSION_V2,
    GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V2,
};
use serde::Serialize;

const CHAIN_ID: &str = "task26-pipeline-parity";
const LABELS: [&str; 4] = ["fund", "winner", "loser", "merge"];

#[derive(Debug, Clone, Copy)]
enum DrainOrder {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CanonicalUtxo {
    txid: String,
    index: u32,
    address: String,
    amount: u64,
    coinbase: bool,
    height: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PipelineSnapshot {
    ordered_blocks: Vec<Hash>,
    ordered_digest: String,
    state_root: String,
    applied_transactions: usize,
    skipped_conflicting_transactions: usize,
    conflict_diagnostics: Vec<String>,
    utxos: Vec<CanonicalUtxo>,
    finality: FinalityBoundaryV1,
}

#[derive(Debug, Clone)]
struct Fixture {
    fund: Block,
    winner: Block,
    loser: Block,
    merge: Block,
}

impl Fixture {
    fn by_label(&self, label: &str) -> &Block {
        match label {
            "fund" => &self.fund,
            "winner" => &self.winner,
            "loser" => &self.loser,
            "merge" => &self.merge,
            other => panic!("unknown fixture label {other}"),
        }
    }
}

fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn transaction(txid: &str, inputs: Vec<OutPoint>, outputs: Vec<(&str, u64)>) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version: TRANSACTION_VERSION_V2,
        inputs: inputs
            .into_iter()
            .map(|previous_output| TxInput {
                previous_output,
                public_key: "00".repeat(32),
                signature: "00".repeat(64),
            })
            .collect(),
        outputs: outputs
            .into_iter()
            .map(|(address, amount)| TxOutput {
                address: address.to_string(),
                amount,
            })
            .collect(),
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
    let parents = canonicalize_block_parents_v2(&parents).unwrap();
    let height = parents
        .iter()
        .map(|parent| state.dag.blocks[parent].header.height.saturating_add(1))
        .max()
        .unwrap();
    let mut block = Block {
        hash: String::new(),
        header: BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents,
            timestamp: 2_100_000_000_u64.saturating_add(height),
            difficulty: 1,
            nonce,
            merkle_root: format!("merkle-{label}-{nonce}"),
            state_root: format!("state-{label}-{nonce}"),
            blue_score: 0,
            height,
        },
        transactions,
    };
    let classification = classify_merge_set_v1(&block, state).unwrap();
    block.header.blue_score = classification.blue_score;
    block.hash = compute_block_hash_v2(&block.header, CHAIN_ID).unwrap();
    block
}

fn commit_ready(state: &mut ChainState, block: Block) {
    let identity = activated_identity(state);
    commit_ghostdag_v1_metadata_for_activated_v2(&block, state, &identity).unwrap();
}

fn build_fixture() -> Fixture {
    let mut planning = init_chain_state(CHAIN_ID.to_string());
    let genesis = planning.dag.genesis_hash.clone();

    let funding = transaction("funding-v2", vec![], vec![("alice", 30), ("bob", 20)]);
    let funding_a = OutPoint {
        txid: funding.txid.clone(),
        index: 0,
    };
    let funding_b = OutPoint {
        txid: funding.txid.clone(),
        index: 1,
    };
    let winner_tx = transaction("winner-v2", vec![funding_b.clone()], vec![("winner", 20)]);
    let loser_tx = transaction("loser-v2", vec![funding_a, funding_b], vec![("loser", 50)]);

    let fund = candidate(&planning, "fund", vec![genesis], 1, vec![funding]);
    commit_ready(&mut planning, fund.clone());

    // Both parallel children have identical score/work/height, so the lower
    // canonical hash is the selected parent. Freeze the one-input spend as the
    // lower hash so the two-input competing spend always encounters its second
    // input already consumed during authoritative replay.
    let loser = candidate(
        &planning,
        "loser",
        vec![fund.hash.clone()],
        10_000,
        vec![loser_tx],
    );
    let winner = (2..10_000)
        .map(|nonce| {
            candidate(
                &planning,
                "winner",
                vec![fund.hash.clone()],
                nonce,
                vec![winner_tx.clone()],
            )
        })
        .find(|candidate| candidate.hash < loser.hash)
        .expect("fixture must find a canonical winner hash below loser hash");

    commit_ready(&mut planning, winner.clone());
    commit_ready(&mut planning, loser.clone());

    let merge = candidate(
        &planning,
        "merge",
        vec![winner.hash.clone(), loser.hash.clone()],
        20_000,
        vec![],
    );
    commit_ready(&mut planning, merge.clone());
    assert_eq!(
        planning.dag.selected_parents.get(&merge.hash),
        Some(&Some(winner.hash.clone())),
        "fixture must place the one-input spend on the selected chain"
    );

    Fixture {
        fund,
        winner,
        loser,
        merge,
    }
}

fn block_is_ready(state: &ChainState, block: &Block) -> bool {
    missing_block_parents(block, state).is_empty()
}

fn take_staged_orphan(state: &mut ChainState, hash: &Hash) -> Block {
    let block = state
        .orphan_blocks
        .remove(hash)
        .unwrap_or_else(|| panic!("ready orphan {hash} must remain staged until drain"));
    state.orphan_missing_parents.remove(hash);
    state.orphan_received_at_ms.remove(hash);
    rebuild_orphan_parent_index(state);
    block
}

fn drain_ready(state: &mut ChainState, order: DrainOrder) {
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
            commit_ready(state, block);
        }
    }
}

fn materialize(
    fixture: &Fixture,
    arrival: &[&str],
    drain_order: DrainOrder,
) -> (ChainState, usize) {
    let mut state = init_chain_state(CHAIN_ID.to_string());
    let mut staged = 0;

    for label in arrival {
        let incoming = fixture.by_label(label).clone();
        let missing = missing_block_parents(&incoming, &state);
        if missing.is_empty() {
            commit_ready(&mut state, incoming);
        } else {
            staged += 1;
            let hash = incoming.hash.clone();
            assert!(queue_orphan_block(&mut state, incoming, missing));
            assert!(state.orphan_blocks.contains_key(&hash));
            assert!(!state.dag.blocks.contains_key(&hash));
            assert!(!state.dag.selected_parents.contains_key(&hash));
            assert!(!state.dag.blue_work.contains_key(&hash));
        }
        drain_ready(&mut state, drain_order);
    }
    drain_ready(&mut state, drain_order);

    assert!(state.orphan_blocks.is_empty());
    assert!(state.orphan_parent_index.is_empty());
    assert!(state.orphan_missing_parents.is_empty());
    (state, staged)
}

fn canonical_utxos(state: &pulsedag_core::UtxoState) -> Vec<CanonicalUtxo> {
    let mut out = state
        .utxos
        .values()
        .map(|utxo| CanonicalUtxo {
            txid: utxo.outpoint.txid.clone(),
            index: utxo.outpoint.index,
            address: utxo.address.clone(),
            amount: utxo.amount,
            coinbase: utxo.coinbase,
            height: utxo.height,
        })
        .collect::<Vec<_>>();
    out.sort_by(|left, right| {
        (&left.txid, left.index, &left.address, left.amount).cmp(&(
            &right.txid,
            right.index,
            &right.address,
            right.amount,
        ))
    });
    out
}

fn pipeline_snapshot(state: &ChainState) -> PipelineSnapshot {
    let ordered = derive_ordered_dag_v2(state).unwrap();
    let replay = rebuild_authoritative_state_v2(state).unwrap();
    let finality = derive_finality_boundary_v1(state).unwrap();

    assert_eq!(replay.ordered_dag, ordered);
    assert_eq!(finality.ordered_dag_digest, ordered.digest);
    assert_eq!(
        finality.protocol_identity,
        ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    );
    assert!(!finality.pruning_enabled);
    assert!(finality.prunable_blocks.is_empty());

    PipelineSnapshot {
        ordered_blocks: ordered.blocks,
        ordered_digest: ordered.digest,
        state_root: replay.diagnostics.state_root,
        applied_transactions: replay.diagnostics.applied_transactions,
        skipped_conflicting_transactions: replay.diagnostics.skipped_conflicting_transactions,
        conflict_diagnostics: replay.diagnostics.conflict_diagnostics,
        utxos: canonical_utxos(&replay.utxo),
        finality,
    }
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
fn ordering_state_and_finality_are_identical_across_arrival_orphans_and_restart() {
    let fixture = build_fixture();
    let mut labels = LABELS;
    let mut arrivals = Vec::new();
    permutations(&mut labels, 0, &mut arrivals);
    assert_eq!(arrivals.len(), 24);

    let mut baseline: Option<PipelineSnapshot> = None;
    let mut baseline_bytes: Option<Vec<u8>> = None;
    let mut staged_total = 0usize;

    for arrival in &arrivals {
        for drain_order in [DrainOrder::Ascending, DrainOrder::Descending] {
            let (state, staged) = materialize(&fixture, arrival, drain_order);
            staged_total = staged_total.saturating_add(staged);
            let snapshot = pipeline_snapshot(&state);
            let bytes = bincode::serialize(&snapshot).unwrap();

            if let Some(expected) = &baseline {
                assert_eq!(&snapshot, expected);
                assert_eq!(Some(&bytes), baseline_bytes.as_ref());
            } else {
                baseline = Some(snapshot.clone());
                baseline_bytes = Some(bytes.clone());
            }

            let encoded_state = bincode::serialize(&state).unwrap();
            let restarted: ChainState = bincode::deserialize(&encoded_state).unwrap();
            let restarted_snapshot = pipeline_snapshot(&restarted);
            assert_eq!(restarted_snapshot, snapshot);
            assert_eq!(bincode::serialize(&restarted_snapshot).unwrap(), bytes);
        }
    }

    assert!(
        staged_total > 0,
        "permutation matrix must exercise real orphan staging"
    );
    let baseline = baseline.unwrap();
    assert_eq!(baseline.applied_transactions, 2);
    assert_eq!(baseline.skipped_conflicting_transactions, 1);
    assert_eq!(baseline.conflict_diagnostics.len(), 1);
    assert!(baseline
        .conflict_diagnostics
        .iter()
        .all(|entry| entry.contains("skipped_conflict_atomic")));
    assert!(baseline
        .utxos
        .iter()
        .any(|utxo| utxo.txid == "funding-v2" && utxo.index == 0));
    assert!(baseline
        .utxos
        .iter()
        .any(|utxo| utxo.txid == "winner-v2" && utxo.index == 0));
    assert!(!baseline.utxos.iter().any(|utxo| utxo.txid == "loser-v2"));
}

#[test]
fn rollback_reapply_reproduces_the_same_pipeline_snapshot() {
    let fixture = build_fixture();
    let mut pre_merge = init_chain_state(CHAIN_ID.to_string());
    for block in [&fixture.fund, &fixture.winner, &fixture.loser] {
        commit_ready(&mut pre_merge, block.clone());
    }
    let checkpoint = bincode::serialize(&pre_merge).unwrap();

    let mut first = pre_merge;
    commit_ready(&mut first, fixture.merge.clone());
    let first_snapshot = pipeline_snapshot(&first);

    let mut restored: ChainState = bincode::deserialize(&checkpoint).unwrap();
    commit_ready(&mut restored, fixture.merge.clone());
    let reapplied_snapshot = pipeline_snapshot(&restored);

    assert_eq!(reapplied_snapshot, first_snapshot);
    assert_eq!(
        bincode::serialize(&reapplied_snapshot).unwrap(),
        bincode::serialize(&first_snapshot).unwrap()
    );
}

#[test]
fn missing_pruned_side_context_fails_closed_across_the_pipeline() {
    let fixture = build_fixture();
    let (state, _) = materialize(
        &fixture,
        &["fund", "winner", "loser", "merge"],
        DrainOrder::Ascending,
    );
    let selected = state
        .dag
        .selected_parents
        .get(&fixture.merge.hash)
        .cloned()
        .flatten()
        .unwrap();
    let side = if selected == fixture.winner.hash {
        fixture.loser.hash.clone()
    } else {
        fixture.winner.hash.clone()
    };

    let mut compact = state;
    compact.dag.blocks.remove(&side);

    assert!(derive_ordered_dag_v2(&compact).is_err());
    assert!(rebuild_authoritative_state_v2(&compact).is_err());
    assert!(derive_finality_boundary_v1(&compact).is_err());
}
