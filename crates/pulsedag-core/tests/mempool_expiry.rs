use pulsedag_core::{
    apply::accept_block_to_dag_metadata,
    genesis::init_chain_state,
    mempool::{mempool_logical_clock, prune_expired_mempool},
    mining::build_candidate_block,
    types::{OutPoint, Transaction, TxInput},
};

fn tx(txid: &str, parent: Option<&str>) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version: 1,
        inputs: parent
            .map(|parent_txid| TxInput {
                previous_output: OutPoint {
                    txid: parent_txid.to_string(),
                    index: 0,
                },
                public_key: String::new(),
                signature: String::new(),
            })
            .into_iter()
            .collect(),
        outputs: Vec::new(),
        fee: 0,
        nonce: 0,
    }
}

fn insert(
    state: &mut pulsedag_core::ChainState,
    transaction: Transaction,
    first_seen: u64,
    admission_height: Option<u64>,
) {
    let txid = transaction.txid.clone();
    for input in &transaction.inputs {
        state
            .mempool
            .spent_outpoints
            .insert(input.previous_output.clone());
    }
    state.mempool.transactions.insert(txid.clone(), transaction);
    state.mempool.first_seen.insert(txid.clone(), first_seen);
    if let Some(height) = admission_height {
        state.mempool.admission_height.insert(txid, height);
    }
}

#[test]
fn expiry_boundary_and_fail_safe_cases_are_exact() {
    let mut before = init_chain_state("expiry-boundary-before".to_string());
    insert(&mut before, tx("boundary", None), 0, Some(5));
    let result = prune_expired_mempool(&mut before, 7, 3);
    assert!(result.expired_txids.is_empty());
    assert_eq!(result.kept_txids, vec!["boundary".to_string()]);

    let mut at = init_chain_state("expiry-boundary-at".to_string());
    insert(&mut at, tx("boundary", None), 0, Some(5));
    assert_eq!(
        prune_expired_mempool(&mut at, 8, 3).expired_txids,
        vec!["boundary".to_string()]
    );

    let mut zero = init_chain_state("expiry-zero".to_string());
    insert(&mut zero, tx("zero", None), 0, Some(5));
    assert_eq!(
        prune_expired_mempool(&mut zero, 5, 0).expired_txids,
        vec!["zero".to_string()]
    );

    let mut fail_safe = init_chain_state("expiry-fail-safe".to_string());
    insert(&mut fail_safe, tx("legacy", None), 0, None);
    insert(&mut fail_safe, tx("future", None), 1, Some(11));
    let result = prune_expired_mempool(&mut fail_safe, 10, 3);
    assert!(result.expired_txids.is_empty());
    assert_eq!(
        result.kept_txids,
        vec!["legacy".to_string(), "future".to_string()]
    );

    let mut overflow = init_chain_state("expiry-overflow".to_string());
    insert(&mut overflow, tx("overflow", None), 0, Some(u64::MAX - 1));
    let result = prune_expired_mempool(&mut overflow, u64::MAX, 3);
    assert!(result.expired_txids.is_empty());
    assert_eq!(result.kept_txids, vec!["overflow".to_string()]);
}

#[test]
fn expiring_parent_removes_descendants_and_rebuilds_metadata_indexes() {
    let mut state = init_chain_state("expiry-descendants".to_string());
    insert(&mut state, tx("parent", None), 0, Some(0));
    insert(&mut state, tx("child", Some("parent")), 1, Some(100));
    insert(&mut state, tx("survivor", Some("external")), 2, Some(100));
    state.mempool.first_seen.insert("stale".to_string(), 999);
    state
        .mempool
        .admission_height
        .insert("stale".to_string(), 999);

    let result = prune_expired_mempool(&mut state, 5, 5);

    assert_eq!(
        result.expired_txids,
        vec!["child".to_string(), "parent".to_string()]
    );
    assert_eq!(result.kept_txids, vec!["survivor".to_string()]);
    assert!(!state.mempool.first_seen.contains_key("parent"));
    assert!(!state.mempool.first_seen.contains_key("child"));
    assert!(!state.mempool.first_seen.contains_key("stale"));
    assert!(!state.mempool.admission_height.contains_key("parent"));
    assert!(!state.mempool.admission_height.contains_key("child"));
    assert!(!state.mempool.admission_height.contains_key("stale"));
    assert_eq!(state.mempool.spent_outpoints.len(), 1);
    assert!(state.mempool.spent_outpoints.contains(&OutPoint {
        txid: "external".to_string(),
        index: 0,
    }));
}

#[test]
fn equivalent_hashmap_orders_prune_to_identical_state() {
    fn fixture(reverse: bool) -> pulsedag_core::ChainState {
        let mut state = init_chain_state("expiry-order".to_string());
        let entries = vec![
            (tx("parent", None), 0, Some(0)),
            (tx("child", Some("parent")), 1, Some(100)),
            (tx("kept", None), 2, Some(100)),
        ];
        if reverse {
            for (transaction, first_seen, height) in entries.into_iter().rev() {
                insert(&mut state, transaction, first_seen, height);
            }
        } else {
            for (transaction, first_seen, height) in entries {
                insert(&mut state, transaction, first_seen, height);
            }
        }
        state
    }

    let mut forward = fixture(false);
    let mut reverse = fixture(true);
    let forward_result = prune_expired_mempool(&mut forward, 5, 5);
    let reverse_result = prune_expired_mempool(&mut reverse, 5, 5);
    assert_eq!(forward_result, reverse_result);
    assert_eq!(forward_result.kept_txids, vec!["kept".to_string()]);
    assert_eq!(
        forward.mempool.spent_outpoints,
        reverse.mempool.spent_outpoints
    );
    assert_eq!(forward.mempool.first_seen, reverse.mempool.first_seen);
    assert_eq!(
        forward.mempool.admission_height,
        reverse.mempool.admission_height
    );
}

#[test]
fn best_height_logical_clock_is_invariant_to_equivalent_dag_accept_order() {
    let mut a = init_chain_state("expiry-clock".to_string());
    let mut b = a.clone();
    let parent = a.dag.genesis_hash.clone();
    let low = build_candidate_block(vec![parent.clone()], 1, 1, Vec::new());
    let high = build_candidate_block(vec![parent], 3, 1, Vec::new());

    accept_block_to_dag_metadata(&low, &mut a).unwrap();
    accept_block_to_dag_metadata(&high, &mut a).unwrap();
    accept_block_to_dag_metadata(&high, &mut b).unwrap();
    accept_block_to_dag_metadata(&low, &mut b).unwrap();

    assert_eq!(
        a.dag
            .blocks
            .keys()
            .collect::<std::collections::BTreeSet<_>>(),
        b.dag.blocks.keys().collect()
    );
    assert_eq!(mempool_logical_clock(&a), 3);
    assert_eq!(mempool_logical_clock(&a), mempool_logical_clock(&b));
}
