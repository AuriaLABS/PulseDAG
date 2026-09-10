use pulsedag_core::{
    accept::AcceptSource,
    accept_transaction_with_mempool_policy_v3,
    accept_transaction_with_mempool_policy_v3_for_protocol,
    canonical_resource_survivors_v1,
    genesis::init_chain_state,
    mempool_resource_rejection_code_from_reason_v1,
    normalize_production_mempool_resources_v1,
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    protocol::ProtocolActivationIdentity,
    types::{OutPoint, Transaction, TxInput, TxOutput},
    MempoolPolicyV3, MempoolResourcePolicyV1, TxAcceptanceResult,
    MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE, TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2,
};

fn tx(txid: &str, fee: u64) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version: TRANSACTION_VERSION_V1,
        inputs: Vec::new(),
        outputs: vec![TxOutput {
            address: "pulse1resource0000000000000000000000000000000000000000".to_string(),
            amount: 1,
        }],
        fee,
        nonce: 1,
    }
}

fn oversized_tx(txid: &str, version: u32) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version,
        inputs: Vec::new(),
        outputs: vec![TxOutput {
            address: "a".repeat(33_000),
            amount: 1,
        }],
        fee: 100_000_000,
        nonce: 9,
    }
}

fn insert_live(
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
    state
        .mempool
        .transactions
        .insert(txid.clone(), transaction);
    state.mempool.first_seen.insert(txid.clone(), first_seen);
    if let Some(height) = admission_height {
        state.mempool.admission_height.insert(txid, height);
    }
}

fn rejected_reason(result: TxAcceptanceResult) -> String {
    match result {
        TxAcceptanceResult::Rejected(reason) => reason,
        other => panic!("expected rejected result, got {other:?}"),
    }
}

#[test]
fn production_resource_identity_is_golden_and_fee_identity_is_unchanged() {
    let policy = MempoolResourcePolicyV1::production_default();
    assert_eq!(policy.version, 1);
    assert_eq!(policy.max_transactions, 4_096);
    assert_eq!(policy.max_spent_outpoints, 8_192);
    assert_eq!(policy.max_orphans, 512);
    assert_eq!(policy.max_canonical_tx_bytes, 32_768);
    assert_eq!(policy.max_age_blocks, 1_440);
    assert_eq!(
        policy.fingerprint(),
        "759a2820217e8b2d897634745b1fffad347f9f72f62348bb9effd5f1b79034cf"
    );
    assert_eq!(
        MempoolPolicyV3::production_default().fingerprint(),
        "fc08725ab79ace07323f11d085c2c105ed5f5e6338b67555103d8cb273c732c8"
    );
}

#[test]
fn expiry_boundary_is_1440_heights_and_missing_age_retains_fail_safe() {
    let mut state = init_chain_state("resource-expiry".to_string());
    state.dag.best_height = 1_439;
    insert_live(&mut state, tx("aged", 10), 1, Some(0));
    insert_live(&mut state, tx("legacy", 11), 2, None);

    let before = normalize_production_mempool_resources_v1(&mut state);
    assert!(before.expired_live_txids.is_empty());
    assert!(state.mempool.transactions.contains_key("aged"));
    assert!(state.mempool.transactions.contains_key("legacy"));

    state.dag.best_height = 1_440;
    let at_boundary = normalize_production_mempool_resources_v1(&mut state);
    assert_eq!(at_boundary.expired_live_txids, vec!["aged".to_string()]);
    assert!(!state.mempool.transactions.contains_key("aged"));
    assert!(state.mempool.transactions.contains_key("legacy"));
}

#[test]
fn oversized_restored_live_root_removes_descendants_and_all_live_metadata() {
    let mut state = init_chain_state("resource-restored-live".to_string());
    let parent = oversized_tx("oversized-parent", TRANSACTION_VERSION_V1);
    let child = Transaction {
        txid: "child".to_string(),
        version: TRANSACTION_VERSION_V1,
        inputs: vec![TxInput {
            previous_output: OutPoint {
                txid: parent.txid.clone(),
                index: 0,
            },
            public_key: String::new(),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1child000000000000000000000000000000000000000000".to_string(),
            amount: 1,
        }],
        fee: 100,
        nonce: 2,
    };
    insert_live(&mut state, parent, 1, Some(1));
    insert_live(&mut state, child, 2, Some(1));

    let result = normalize_production_mempool_resources_v1(&mut state);
    assert_eq!(
        result.removed_live_txids,
        vec!["child".to_string(), "oversized-parent".to_string()]
    );
    assert!(state.mempool.transactions.is_empty());
    assert!(state.mempool.first_seen.is_empty());
    assert!(state.mempool.admission_height.is_empty());
    assert!(state.mempool.spent_outpoints.is_empty());
}

#[test]
fn restored_capacity_normalization_is_insertion_order_deterministic() {
    fn fixture(reverse: bool) -> pulsedag_core::ChainState {
        let mut state = init_chain_state("resource-capacity".to_string());
        state.mempool.max_transactions = 2;
        let mut entries = vec![tx("a", 1), tx("b", 2), tx("c", 3)];
        if reverse {
            entries.reverse();
        }
        for transaction in entries {
            let first_seen = match transaction.txid.as_str() {
                "a" => 0,
                "b" => 1,
                "c" => 2,
                _ => unreachable!("fixed fixture txid"),
            };
            insert_live(&mut state, transaction, first_seen, Some(0));
        }
        state
    }

    let mut a = fixture(false);
    let mut b = fixture(true);
    let ra = normalize_production_mempool_resources_v1(&mut a);
    let rb = normalize_production_mempool_resources_v1(&mut b);
    assert_eq!(ra.removed_live_txids, rb.removed_live_txids);
    assert_eq!(canonical_resource_survivors_v1(&a), canonical_resource_survivors_v1(&b));
    assert_eq!(a.mempool.spent_outpoints, b.mempool.spent_outpoints);
    assert_eq!(
        a.mempool.first_seen.keys().collect::<std::collections::BTreeSet<_>>(),
        b.mempool.first_seen.keys().collect::<std::collections::BTreeSet<_>>()
    );
    assert_eq!(
        a.mempool
            .admission_height
            .keys()
            .collect::<std::collections::BTreeSet<_>>(),
        b.mempool
            .admission_height
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
    );
}

#[test]
fn oversized_restored_orphan_is_pruned_with_orphan_metadata() {
    let mut state = init_chain_state("resource-orphan".to_string());
    let orphan = oversized_tx("oversized-orphan", TRANSACTION_VERSION_V1);
    state
        .mempool
        .orphan_transactions
        .insert(orphan.txid.clone(), orphan);
    state
        .mempool
        .orphan_missing_outpoints
        .insert("oversized-orphan".to_string(), Vec::new());
    state
        .mempool
        .orphan_received_order
        .insert("oversized-orphan".to_string(), 1);

    let result = normalize_production_mempool_resources_v1(&mut state);
    assert_eq!(
        result.dropped_orphan_txids,
        vec!["oversized-orphan".to_string()]
    );
    assert!(state.mempool.orphan_transactions.is_empty());
    assert!(state.mempool.orphan_missing_outpoints.is_empty());
    assert!(state.mempool.orphan_received_order.is_empty());
}

#[test]
fn production_fresh_admission_rejects_oversize_before_legacy_or_v2_mutation() {
    let policy = MempoolPolicyV3::production_default();
    let mut legacy = init_chain_state("resource-fresh-v1".to_string());
    let reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
        oversized_tx("oversized-v1", TRANSACTION_VERSION_V1),
        &mut legacy,
        AcceptSource::Rpc,
        policy,
    ));
    assert_eq!(
        mempool_resource_rejection_code_from_reason_v1(&reason),
        Some(MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE)
    );
    assert!(legacy.mempool.transactions.is_empty());
    assert!(legacy.mempool.orphan_transactions.is_empty());

    let mut v2 = init_chain_state("resource-fresh-v2".to_string());
    let identity = ProtocolActivationIdentity::activated_v2(
        v2.chain_id.clone(),
        v2.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    let reason = rejected_reason(accept_transaction_with_mempool_policy_v3_for_protocol(
        oversized_tx("oversized-v2", TRANSACTION_VERSION_V2),
        &mut v2,
        AcceptSource::Rpc,
        &identity,
        policy,
    ));
    assert_eq!(
        mempool_resource_rejection_code_from_reason_v1(&reason),
        Some(MEMPOOL_RESOURCE_TX_TOO_LARGE_CODE)
    );
    assert!(v2.mempool.transactions.is_empty());
    assert!(v2.mempool.orphan_transactions.is_empty());
}
