use pulsedag_core::{
    genesis::init_chain_state,
    mempool_admission_v3::{
        accept_transaction_with_mempool_policy_v3,
        accept_transaction_with_mempool_policy_v3_for_protocol,
    },
    mempool_resource_v1::{
        canonical_transaction_size_for_resource_v1, mempool_resource_rejection_code_from_reason_v1,
        transaction_message_size_for_resource_v1, MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1,
        MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1,
    },
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    types::{Transaction, TxOutput},
    AcceptSource, MempoolPolicyV3, ProtocolActivationIdentity, TxAcceptanceResult,
    TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2,
};

fn transaction(version: u32, txid: &str, fee: u64, address: String) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version,
        inputs: Vec::new(),
        outputs: vec![TxOutput { address, amount: 7 }],
        fee,
        nonce: 41,
    }
}

fn rejected_reason(result: TxAcceptanceResult) -> String {
    match result {
        TxAcceptanceResult::Rejected(reason) => reason,
        other => panic!("expected rejection, got {other:?}"),
    }
}

#[test]
fn low_fee_rejection_does_not_normalize_unrelated_mempool_state() {
    let mut state = init_chain_state("resource-preflight-bounded".to_string());
    let unrelated = transaction(
        TRANSACTION_VERSION_V1,
        "unrelated-existing",
        10,
        "pulse1unrelated".to_string(),
    );
    state
        .mempool
        .transactions
        .insert(unrelated.txid.clone(), unrelated);
    state
        .mempool
        .first_seen
        .insert("unrelated-existing".to_string(), 7);
    assert!(!state
        .mempool
        .admission_height
        .contains_key("unrelated-existing"));

    let before_transactions = bincode::serialize(&state.mempool.transactions).unwrap();
    let before_first_seen = state.mempool.first_seen.clone();
    let before_admission_height = state.mempool.admission_height.clone();
    let before_spent = state.mempool.spent_outpoints.clone();
    let before_orphans = bincode::serialize(&state.mempool.orphan_transactions).unwrap();
    let before_limits = (
        state.mempool.max_transactions,
        state.mempool.max_spent_outpoints,
        state.mempool.max_orphans,
    );

    let reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
        transaction(
            TRANSACTION_VERSION_V1,
            "incoming-low-fee",
            0,
            "pulse1incoming".to_string(),
        ),
        &mut state,
        AcceptSource::P2p,
        MempoolPolicyV3::production_default(),
    ));

    assert_eq!(
        pulsedag_core::mempool_policy_rejection_code_from_reason_v3(&reason),
        Some("MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE")
    );
    assert_eq!(
        bincode::serialize(&state.mempool.transactions).unwrap(),
        before_transactions
    );
    assert_eq!(state.mempool.first_seen, before_first_seen);
    assert_eq!(state.mempool.admission_height, before_admission_height);
    assert_eq!(state.mempool.spent_outpoints, before_spent);
    assert_eq!(
        bincode::serialize(&state.mempool.orphan_transactions).unwrap(),
        before_orphans
    );
    assert_eq!(
        (
            state.mempool.max_transactions,
            state.mempool.max_spent_outpoints,
            state.mempool.max_orphans,
        ),
        before_limits
    );
}

#[test]
fn carrier_oversize_rejection_has_ordinary_and_activated_v2_parity() {
    let escaped_address = "\"".repeat(40_000);

    let mut ordinary = init_chain_state("resource-carrier-ordinary".to_string());
    let ordinary_tx = transaction(
        TRANSACTION_VERSION_V1,
        "carrier-v1",
        100_000,
        escaped_address.clone(),
    );
    assert!(
        canonical_transaction_size_for_resource_v1(&ordinary_tx, &ordinary.chain_id).unwrap()
            <= MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1
    );
    assert!(
        transaction_message_size_for_resource_v1(&ordinary_tx, &ordinary.chain_id).unwrap()
            > MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1
    );
    let ordinary_reason = rejected_reason(accept_transaction_with_mempool_policy_v3(
        ordinary_tx,
        &mut ordinary,
        AcceptSource::Rpc,
        MempoolPolicyV3::production_default(),
    ));
    assert_eq!(
        mempool_resource_rejection_code_from_reason_v1(&ordinary_reason),
        Some("MEMPOOL_RESOURCE_V1_TRANSACTION_TOO_LARGE")
    );
    assert!(ordinary.mempool.transactions.is_empty());

    let mut activated = init_chain_state("resource-carrier-activated-v2".to_string());
    let identity = ProtocolActivationIdentity::activated_v2(
        activated.chain_id.clone(),
        activated.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    let activated_tx = transaction(
        TRANSACTION_VERSION_V2,
        "carrier-v2",
        100_000,
        escaped_address,
    );
    assert!(
        canonical_transaction_size_for_resource_v1(&activated_tx, &activated.chain_id).unwrap()
            <= MEMPOOL_RESOURCE_MAX_TRANSACTION_BYTES_V1
    );
    assert!(
        transaction_message_size_for_resource_v1(&activated_tx, &activated.chain_id).unwrap()
            > MEMPOOL_RESOURCE_MAX_TRANSACTION_MESSAGE_BYTES_V1
    );
    let activated_reason = rejected_reason(accept_transaction_with_mempool_policy_v3_for_protocol(
        activated_tx,
        &mut activated,
        AcceptSource::Rpc,
        &identity,
        MempoolPolicyV3::production_default(),
    ));
    assert_eq!(
        mempool_resource_rejection_code_from_reason_v1(&activated_reason),
        Some("MEMPOOL_RESOURCE_V1_TRANSACTION_TOO_LARGE")
    );
    assert!(activated.mempool.transactions.is_empty());
}
