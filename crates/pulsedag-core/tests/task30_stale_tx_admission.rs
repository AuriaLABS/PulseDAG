use pulsedag_core::{
    accept_transaction_with_result, accept_transaction_with_result_for_protocol,
    genesis::init_chain_state,
    ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
    protocol::ProtocolActivationIdentity,
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
