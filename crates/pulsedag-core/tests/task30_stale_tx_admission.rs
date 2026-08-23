use pulsedag_core::{
    accept_transaction_with_result, accept_transaction_with_result_for_protocol,
    compact_snapshot_to_retained_blocks,
    genesis::init_chain_state,
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    protocol::ProtocolActivationIdentity,
    state::ConsensusMode,
    tx::{compute_txid, compute_txid_v2},
    types::{OutPoint, Transaction, TxInput, TxOutput},
    validation::{transaction_is_confirmed, validate_transaction},
    validation_v2::validate_transaction_v2,
    AcceptSource, PulseError, TxAcceptanceResult, TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2,
};

fn transaction(version: u32) -> Transaction {
    Transaction {
        txid: String::new(),
        version,
        inputs: vec![TxInput {
            previous_output: OutPoint {
                txid: "already-spent-funding".to_string(),
                index: 0,
            },
            public_key: "confirmed-public-key".to_string(),
            signature: "confirmed-signature".to_string(),
        }],
        outputs: vec![TxOutput {
            address: "confirmed-recipient".to_string(),
            amount: 1,
        }],
        fee: 1,
        nonce: 30,
    }
}

fn record_as_confirmed(state: &mut pulsedag_core::ChainState, tx: &Transaction) {
    let genesis = state.dag.genesis_hash.clone();
    state
        .dag
        .blocks
        .get_mut(&genesis)
        .expect("genesis block exists")
        .transactions
        .push(tx.clone());
}

fn record_in_noncanonical_block(
    state: &mut pulsedag_core::ChainState,
    tx: &Transaction,
    hash: &str,
) {
    let genesis = state.dag.genesis_hash.clone();
    let mut block = state
        .dag
        .blocks
        .get(&genesis)
        .expect("genesis block exists")
        .clone();
    block.hash = hash.to_string();
    block.header.parents = vec![genesis];
    block.header.height = 1;
    block.header.timestamp = 1;
    block.transactions = vec![tx.clone()];
    state.dag.blocks.insert(hash.to_string(), block);
}

#[test]
fn confirmed_v1_resubmission_is_duplicate_not_orphan() {
    let mut state = init_chain_state("task30-stale-v1".to_string());
    let mut tx = transaction(TRANSACTION_VERSION_V1);
    tx.txid = compute_txid(&tx);
    record_as_confirmed(&mut state, &tx);

    assert!(transaction_is_confirmed(&tx.txid, &state));
    assert!(matches!(
        validate_transaction(&tx, &state),
        Err(PulseError::TxAlreadyExists)
    ));
    assert_eq!(
        accept_transaction_with_result(tx.clone(), &mut state, AcceptSource::P2p),
        TxAcceptanceResult::Duplicate
    );
    assert!(!state.mempool.transactions.contains_key(&tx.txid));
    assert!(!state.mempool.orphan_transactions.contains_key(&tx.txid));
}

#[test]
fn confirmed_v2_resubmission_is_duplicate_not_orphan() {
    let chain_id = "task30-stale-v2";
    let mut state = init_chain_state(chain_id.to_string());
    let mut tx = transaction(TRANSACTION_VERSION_V2);
    tx.txid = compute_txid_v2(&tx, chain_id).expect("canonical v2 txid");
    record_as_confirmed(&mut state, &tx);

    assert!(transaction_is_confirmed(&tx.txid, &state));
    assert!(matches!(
        validate_transaction_v2(&tx, &state, chain_id),
        Err(PulseError::TxAlreadyExists)
    ));

    let identity = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    assert_eq!(
        accept_transaction_with_result_for_protocol(
            tx.clone(),
            &mut state,
            AcceptSource::P2p,
            &identity,
        ),
        TxAcceptanceResult::Duplicate
    );
    assert!(!state.mempool.transactions.contains_key(&tx.txid));
    assert!(!state.mempool.orphan_transactions.contains_key(&tx.txid));
}

#[test]
fn legacy_side_dag_transaction_is_not_confirmed() {
    let mut state = init_chain_state("task30-side-dag".to_string());
    let mut tx = transaction(TRANSACTION_VERSION_V1);
    tx.txid = compute_txid(&tx);
    let side_hash = "task30-side-only";
    record_in_noncanonical_block(&mut state, &tx, side_hash);

    assert!(!state
        .dag
        .selected_chain
        .iter()
        .any(|hash| hash == side_hash));
    assert!(
        !transaction_is_confirmed(&tx.txid, &state),
        "raw side-DAG membership must not imply canonical confirmation"
    );
    assert!(
        !matches!(
            validate_transaction(&tx, &state),
            Err(PulseError::TxAlreadyExists)
        ),
        "side-DAG transaction must not be rejected as an already-confirmed duplicate"
    );
}

#[test]
fn ghostdag_conflict_loser_transaction_is_not_confirmed() {
    let mut state = init_chain_state("task30-conflict-loser".to_string());
    state.dag.consensus_mode = ConsensusMode::GhostdagDev;
    let mut tx = transaction(TRANSACTION_VERSION_V1);
    tx.txid = compute_txid(&tx);
    let loser_hash = "task30-loser-block";
    record_in_noncanonical_block(&mut state, &tx, loser_hash);
    state.dag.ordered_dag.push(loser_hash.to_string());
    state.dag.ordered_dag_conflict_diagnostics.push(format!(
        "ordered_pos=1 block={loser_hash} tx={} skipped_conflict",
        tx.txid
    ));

    assert!(state.dag.ordered_dag.iter().any(|hash| hash == loser_hash));
    assert!(
        !transaction_is_confirmed(&tx.txid, &state),
        "a transaction skipped by canonical DAG replay must not be treated as confirmed"
    );
    assert!(
        !matches!(
            validate_transaction(&tx, &state),
            Err(PulseError::TxAlreadyExists)
        ),
        "conflict-loser transaction must not be rejected as an already-confirmed duplicate"
    );
}

#[test]
fn activated_v2_applied_ordered_transaction_is_confirmed() {
    let chain_id = "task30-v2-ordered-applied";
    let mut state = init_chain_state(chain_id.to_string());
    state.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    let mut tx = transaction(TRANSACTION_VERSION_V2);
    tx.txid = compute_txid_v2(&tx, chain_id).expect("canonical v2 txid");
    let ordered_hash = "task30-v2-ordered-side";
    record_in_noncanonical_block(&mut state, &tx, ordered_hash);
    state.dag.ordered_dag.push(ordered_hash.to_string());

    assert!(!state.dag.consensus_mode.ghostdag_metadata_active());
    assert!(!state
        .dag
        .selected_chain
        .iter()
        .any(|hash| hash == ordered_hash));
    assert!(state
        .dag
        .ordered_dag
        .iter()
        .any(|hash| hash == ordered_hash));
    assert!(
        transaction_is_confirmed(&tx.txid, &state),
        "activated-v2 confirmation must follow the authoritative ordered DAG even when the runtime consensus enum remains legacy"
    );
    assert!(matches!(
        validate_transaction_v2(&tx, &state, chain_id),
        Err(PulseError::TxAlreadyExists)
    ));
}

#[test]
fn activated_v2_conflict_loser_is_not_confirmed() {
    let chain_id = "task30-v2-conflict-loser";
    let mut state = init_chain_state(chain_id.to_string());
    state.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    let mut tx = transaction(TRANSACTION_VERSION_V2);
    tx.txid = compute_txid_v2(&tx, chain_id).expect("canonical v2 txid");
    let loser_hash = "task30-v2-loser-block";
    record_in_noncanonical_block(&mut state, &tx, loser_hash);
    state.dag.selected_chain.push(loser_hash.to_string());
    state.dag.ordered_dag.push(loser_hash.to_string());
    state.dag.ordered_dag_conflict_diagnostics.push(format!(
        "ordered_pos=1 block={loser_hash} tx={} skipped_conflict_atomic",
        tx.txid
    ));

    assert!(!state.dag.consensus_mode.ghostdag_metadata_active());
    assert!(state
        .dag
        .selected_chain
        .iter()
        .any(|hash| hash == loser_hash));
    assert!(state.dag.ordered_dag.iter().any(|hash| hash == loser_hash));
    assert!(
        !transaction_is_confirmed(&tx.txid, &state),
        "activated-v2 replay-skipped conflict loser must not be treated as confirmed"
    );
    assert!(
        !matches!(
            validate_transaction_v2(&tx, &state, chain_id),
            Err(PulseError::TxAlreadyExists)
        ),
        "activated-v2 conflict loser must remain outside confirmed duplicate semantics"
    );
}

#[test]
fn compact_prune_preserves_retained_conflict_loser_semantics() {
    let chain_id = "task30-v2-pruned-conflict-loser";
    let mut state = init_chain_state(chain_id.to_string());
    state.dag.ordering_version = GHOSTDAG_V1_ORDERING_VERSION.to_string();
    let mut tx = transaction(TRANSACTION_VERSION_V2);
    tx.txid = compute_txid_v2(&tx, chain_id).expect("canonical v2 txid");
    let loser_hash = "task30-v2-retained-loser";
    record_in_noncanonical_block(&mut state, &tx, loser_hash);
    state.dag.best_height = 1;
    state.dag.ordered_dag = vec![state.dag.genesis_hash.clone(), loser_hash.to_string()];
    state.dag.ordered_dag_tip = Some(loser_hash.to_string());
    state.dag.ordered_dag_conflict_diagnostics.push(format!(
        "ordered_pos=1 block={loser_hash} tx={} skipped_conflict_atomic",
        tx.txid
    ));

    assert!(!transaction_is_confirmed(&tx.txid, &state));
    let retained = vec![state
        .dag
        .blocks
        .get(loser_hash)
        .expect("retained loser block exists")
        .clone()];
    let compact = compact_snapshot_to_retained_blocks(state, &retained)
        .expect("compact snapshot should retain the boundary block");

    assert_eq!(compact.dag.ordered_dag, vec![loser_hash.to_string()]);
    assert!(
        compact
            .dag
            .ordered_dag_conflict_diagnostics
            .iter()
            .any(|entry| entry.contains(&format!("block={loser_hash} tx={}", tx.txid))),
        "compact prune must retain conflict semantics for retained ordered blocks"
    );
    assert!(
        !transaction_is_confirmed(&tx.txid, &compact),
        "retained conflict loser must remain non-confirmed after compact prune"
    );
}
