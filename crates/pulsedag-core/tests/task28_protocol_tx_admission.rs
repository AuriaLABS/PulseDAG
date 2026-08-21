use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_transaction_with_result, accept_transaction_with_result_for_protocol,
    address_from_public_key, compute_txid, compute_txid_v2, genesis::init_chain_state,
    signing_message, signing_message_v2, AcceptSource, ChainState, OutPoint,
    ProtocolActivationIdentity, Transaction, TxAcceptanceResult, TxInput, TxOutput, Utxo,
    GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2,
};

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().to_bytes())
}

fn fund_key(state: &mut ChainState, txid: &str, signing_key: &SigningKey, amount: u64) -> OutPoint {
    let public_key = public_key_hex(signing_key);
    let address = address_from_public_key(&public_key);
    let outpoint = OutPoint {
        txid: txid.to_string(),
        index: 0,
    };
    state.utxo.utxos.insert(
        outpoint.clone(),
        Utxo {
            outpoint: outpoint.clone(),
            address: address.clone(),
            amount,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(address)
        .or_default()
        .push(outpoint.clone());
    outpoint
}

fn signed_v1_transaction(
    signing_key: &SigningKey,
    previous_output: OutPoint,
    nonce: u64,
) -> Transaction {
    let public_key = public_key_hex(signing_key);
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V1,
        inputs: vec![TxInput {
            previous_output,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1task28recipient".to_string(),
            amount: 9,
        }],
        fee: 1,
        nonce,
    };
    let message = signing_message(&tx);
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid(&tx);
    tx
}

fn signed_v2_transaction(
    signing_key: &SigningKey,
    previous_output: OutPoint,
    nonce: u64,
    chain_id: &str,
) -> Transaction {
    let public_key = public_key_hex(signing_key);
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1task28recipient".to_string(),
            amount: 9,
        }],
        fee: 1,
        nonce,
    };
    let message = signing_message_v2(&tx, chain_id).unwrap();
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid_v2(&tx, chain_id).unwrap();
    tx
}

fn activated_identity(state: &ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

#[test]
fn explicit_protocol_admission_accepts_matching_v1_and_v2_transactions() {
    let mut legacy_state = init_chain_state("task28-legacy".to_string());
    let legacy_key = signing_key(11);
    let legacy_outpoint = fund_key(&mut legacy_state, "legacy-funding", &legacy_key, 10);
    let legacy_tx = signed_v1_transaction(&legacy_key, legacy_outpoint, 1);
    let legacy_txid = legacy_tx.txid.clone();
    let legacy_identity = ProtocolActivationIdentity::legacy_from_state(&legacy_state);

    assert_eq!(
        accept_transaction_with_result_for_protocol(
            legacy_tx,
            &mut legacy_state,
            AcceptSource::P2p,
            &legacy_identity,
        ),
        TxAcceptanceResult::Accepted
    );
    assert!(legacy_state.mempool.transactions.contains_key(&legacy_txid));

    let mut v2_state = init_chain_state("task28-v2".to_string());
    let v2_key = signing_key(12);
    let v2_outpoint = fund_key(&mut v2_state, "v2-funding", &v2_key, 10);
    let v2_tx = signed_v2_transaction(&v2_key, v2_outpoint, 2, &v2_state.chain_id);
    let v2_txid = v2_tx.txid.clone();
    let v2_identity = activated_identity(&v2_state);

    assert_eq!(
        accept_transaction_with_result_for_protocol(
            v2_tx,
            &mut v2_state,
            AcceptSource::Rpc,
            &v2_identity,
        ),
        TxAcceptanceResult::Accepted
    );
    assert!(v2_state.mempool.transactions.contains_key(&v2_txid));
}

#[test]
fn activated_v2_admission_reconciles_existing_v2_mempool_with_v2_rules() {
    let mut state = init_chain_state("task28-v2-reconcile-admission".to_string());
    let identity = activated_identity(&state);

    let first_key = signing_key(31);
    let first_outpoint = fund_key(&mut state, "v2-reconcile-first", &first_key, 10);
    let first = signed_v2_transaction(&first_key, first_outpoint, 31, &state.chain_id);
    let first_txid = first.txid.clone();
    assert_eq!(
        accept_transaction_with_result_for_protocol(
            first,
            &mut state,
            AcceptSource::Rpc,
            &identity,
        ),
        TxAcceptanceResult::Accepted
    );

    // Force the admission-side bookkeeping repair path. A legacy-v1
    // reconcile would reject the already-admitted v2 tx by its v2 txid.
    state.mempool.spent_outpoints.clear();

    let second_key = signing_key(32);
    let second_outpoint = fund_key(&mut state, "v2-reconcile-second", &second_key, 10);
    let second = signed_v2_transaction(&second_key, second_outpoint, 32, &state.chain_id);
    let second_txid = second.txid.clone();

    assert_eq!(
        accept_transaction_with_result_for_protocol(
            second,
            &mut state,
            AcceptSource::Rpc,
            &identity,
        ),
        TxAcceptanceResult::Accepted
    );
    assert!(state.mempool.transactions.contains_key(&first_txid));
    assert!(state.mempool.transactions.contains_key(&second_txid));
    assert_eq!(state.mempool.transactions.len(), 2);
    assert!(state.mempool.counters.reconcile_runs_total >= 1);
}

#[test]
fn legacy_entrypoint_remains_v1_only() {
    let mut state = init_chain_state("task28-legacy-entrypoint".to_string());
    let key = signing_key(13);
    let outpoint = fund_key(&mut state, "v2-on-legacy", &key, 10);
    let chain_id = state.chain_id.clone();
    let v2 = signed_v2_transaction(&key, outpoint, 3, &chain_id);

    assert!(matches!(
        accept_transaction_with_result(v2, &mut state, AcceptSource::P2p),
        TxAcceptanceResult::Invalid(_)
    ));
    assert!(state.mempool.transactions.is_empty());
}

#[test]
fn activated_v2_rejects_v1_before_duplicate_or_reconcile_logic() {
    let mut state = init_chain_state("task28-version-precedence".to_string());
    let key = signing_key(14);
    let outpoint = fund_key(&mut state, "version-funding", &key, 10);
    let v1 = signed_v1_transaction(&key, outpoint, 4);
    let identity = activated_identity(&state);

    state
        .mempool
        .transactions
        .insert(v1.txid.clone(), v1.clone());
    let before_first_seen = state.mempool.next_first_seen;

    let outcome =
        accept_transaction_with_result_for_protocol(v1, &mut state, AcceptSource::P2p, &identity);

    assert!(matches!(
        outcome,
        TxAcceptanceResult::Invalid(message)
            if message.contains("requires transaction version 2")
    ));
    assert_eq!(state.mempool.transactions.len(), 1);
    assert_eq!(state.mempool.next_first_seen, before_first_seen);
}

#[test]
fn wrong_activation_identity_fails_without_staging_transaction_state() {
    let mut state = init_chain_state("task28-identity".to_string());
    let key = signing_key(15);
    let outpoint = fund_key(&mut state, "identity-funding", &key, 10);
    let chain_id = state.chain_id.clone();
    let v2 = signed_v2_transaction(&key, outpoint, 5, &chain_id);
    let mut identity = activated_identity(&state);
    identity.chain_id = "different-chain".to_string();

    let outcome =
        accept_transaction_with_result_for_protocol(v2, &mut state, AcceptSource::Rpc, &identity);

    assert!(matches!(outcome, TxAcceptanceResult::Rejected(_)));
    assert!(state.mempool.transactions.is_empty());
    assert!(state.mempool.orphan_transactions.is_empty());
    assert!(state.mempool.spent_outpoints.is_empty());
    assert_eq!(state.mempool.next_first_seen, 0);
}

#[test]
fn v2_orphan_promotion_reuses_the_same_protocol_identity() {
    let mut state = init_chain_state("task28-v2-orphan".to_string());
    let identity = activated_identity(&state);

    let late_key = signing_key(16);
    let late_outpoint = OutPoint {
        txid: "late-funding".to_string(),
        index: 0,
    };
    let orphan = signed_v2_transaction(&late_key, late_outpoint.clone(), 6, &state.chain_id);
    let orphan_txid = orphan.txid.clone();

    assert_eq!(
        accept_transaction_with_result_for_protocol(
            orphan,
            &mut state,
            AcceptSource::P2p,
            &identity,
        ),
        TxAcceptanceResult::Orphan
    );
    assert!(state.mempool.orphan_transactions.contains_key(&orphan_txid));

    let late_address = address_from_public_key(&public_key_hex(&late_key));
    state.utxo.utxos.insert(
        late_outpoint.clone(),
        Utxo {
            outpoint: late_outpoint.clone(),
            address: late_address.clone(),
            amount: 10,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(late_address)
        .or_default()
        .push(late_outpoint);

    let trigger_key = signing_key(17);
    let trigger_outpoint = fund_key(&mut state, "trigger-funding", &trigger_key, 10);
    let trigger = signed_v2_transaction(&trigger_key, trigger_outpoint, 7, &state.chain_id);

    assert_eq!(
        accept_transaction_with_result_for_protocol(
            trigger,
            &mut state,
            AcceptSource::P2p,
            &identity,
        ),
        TxAcceptanceResult::Accepted
    );
    assert!(state.mempool.transactions.contains_key(&orphan_txid));
    assert!(!state.mempool.orphan_transactions.contains_key(&orphan_txid));
    assert!(state.mempool.counters.orphan_promoted_total >= 1);
}
