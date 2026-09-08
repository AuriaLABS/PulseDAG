use pulsedag_core::{
    canonical_mempool_txids, genesis_v2::init_chain_state_v2, ActivatedV2P2pRuntime,
    ProtocolActivationIdentity, Transaction, TxOutput, GHOSTDAG_V1_ORDERING_VERSION,
    TRANSACTION_VERSION_V2,
};
use pulsedag_storage::Storage;

fn temp_db_path(test_name: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir()
        .join(format!("pulsedag-v3-mempool-restart-{test_name}-{unique}"))
        .to_string_lossy()
        .into_owned()
}

fn dummy_v2_tx(txid: &str, nonce: u64) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version: TRANSACTION_VERSION_V2,
        inputs: Vec::new(),
        outputs: vec![TxOutput {
            address: "pulse1mempoolrestartvector".to_string(),
            amount: 1,
        }],
        fee: 1,
        nonce,
    }
}

#[test]
fn activated_v2_restart_preserves_mempool_first_seen_order_exactly() {
    let path = temp_db_path("first-seen");
    let chain_id = "pulsedag-v3-mempool-restart".to_string();
    let mut state = init_chain_state_v2(chain_id.clone()).unwrap();
    let identity = ProtocolActivationIdentity::activated_v2(
        chain_id,
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );

    let tx_a = dummy_v2_tx("tx-a", 1);
    let tx_b = dummy_v2_tx("tx-b", 2);
    let tx_c = dummy_v2_tx("tx-c", 3);

    // Deliberately populate the HashMap in a different order from the
    // authoritative first-seen ordering. The persisted first_seen map, not
    // HashMap iteration order, is the restart ordering authority.
    state
        .mempool
        .transactions
        .insert(tx_c.txid.clone(), tx_c);
    state
        .mempool
        .transactions
        .insert(tx_b.txid.clone(), tx_b);
    state
        .mempool
        .transactions
        .insert(tx_a.txid.clone(), tx_a);
    state.mempool.first_seen.insert("tx-a".to_string(), 3);
    state.mempool.first_seen.insert("tx-c".to_string(), 7);
    state.mempool.first_seen.insert("tx-b".to_string(), 7);
    state.mempool.next_first_seen = 8;

    let expected_order = vec![
        "tx-a".to_string(),
        "tx-b".to_string(),
        "tx-c".to_string(),
    ];
    assert_eq!(canonical_mempool_txids(&state), expected_order);

    let storage = Storage::open(&path).unwrap();
    storage
        .persist_activated_v2_p2p_runtime_snapshot(
            &identity,
            &state,
            &ActivatedV2P2pRuntime::default(),
        )
        .unwrap();
    drop(storage);

    let storage = Storage::open(&path).unwrap();
    let (restored, runtime) = storage
        .load_activated_v2_p2p_runtime_snapshot(&identity)
        .unwrap();

    assert!(runtime.pending_is_empty());
    assert!(runtime.staging().is_empty());
    assert_eq!(canonical_mempool_txids(&restored), expected_order);
    assert_eq!(restored.mempool.first_seen, state.mempool.first_seen);
    assert_eq!(restored.mempool.next_first_seen, state.mempool.next_first_seen);
    assert_eq!(restored.mempool.transactions.len(), 3);

    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}
