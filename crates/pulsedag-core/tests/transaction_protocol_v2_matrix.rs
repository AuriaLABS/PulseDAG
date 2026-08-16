use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    address_from_public_key, classify_transaction_version, compute_txid, compute_txid_v2,
    genesis::init_chain_state, signing_message, signing_message_v2,
    validate_transaction_for_protocol, ChainState, OutPoint, ProtocolActivationIdentity,
    PulseError, Transaction, TransactionRejectionClass, TransactionValidationPath, TxInput,
    TxOutput, Utxo, GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V1, TRANSACTION_VERSION_V2,
};

fn funded_state(chain_id: &str) -> (ChainState, SigningKey, OutPoint, String) {
    let signing_key = SigningKey::from_bytes(&[19_u8; 32]);
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    let source_address = address_from_public_key(&public_key);
    let outpoint = OutPoint {
        txid: "matrix-funding".to_string(),
        index: 0,
    };

    let mut state = init_chain_state(chain_id.to_string());
    state.utxo.utxos.insert(
        outpoint.clone(),
        Utxo {
            outpoint: outpoint.clone(),
            address: source_address.clone(),
            amount: 25,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(source_address)
        .or_default()
        .push(outpoint.clone());

    (state, signing_key, outpoint, public_key)
}

fn signed_v1_transaction(
    signing_key: &SigningKey,
    outpoint: &OutPoint,
    public_key: &str,
) -> Transaction {
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V1,
        inputs: vec![TxInput {
            previous_output: outpoint.clone(),
            public_key: public_key.to_string(),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1matrixrecipient".to_string(),
            amount: 24,
        }],
        fee: 1,
        nonce: 31,
    };
    let message = signing_message(&tx);
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid(&tx);
    tx
}

fn signed_v2_transaction(
    signing_key: &SigningKey,
    outpoint: &OutPoint,
    public_key: &str,
    chain_id: &str,
) -> Transaction {
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: outpoint.clone(),
            public_key: public_key.to_string(),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1matrixrecipient".to_string(),
            amount: 24,
        }],
        fee: 1,
        nonce: 31,
    };
    let message = signing_message_v2(&tx, chain_id).unwrap();
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid_v2(&tx, chain_id).unwrap();
    tx
}

#[test]
fn legacy_v1_and_activated_v2_each_validate_on_their_explicit_path() {
    let (state, signing_key, outpoint, public_key) = funded_state("pulsedag-matrix");
    let v1 = signed_v1_transaction(&signing_key, &outpoint, &public_key);
    let legacy_identity = ProtocolActivationIdentity::legacy_from_state(&state);
    validate_transaction_for_protocol(&v1, &state, &legacy_identity).unwrap();

    let v2 = signed_v2_transaction(&signing_key, &outpoint, &public_key, "pulsedag-matrix");
    let activated_identity = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    validate_transaction_for_protocol(&v2, &state, &activated_identity).unwrap();
}

#[test]
fn protocol_paths_reject_the_other_known_transaction_version_before_utxo_work() {
    let (state, signing_key, outpoint, public_key) = funded_state("pulsedag-matrix");
    let v1 = signed_v1_transaction(&signing_key, &outpoint, &public_key);
    let v2 = signed_v2_transaction(&signing_key, &outpoint, &public_key, "pulsedag-matrix");

    let legacy_identity = ProtocolActivationIdentity::legacy_from_state(&state);
    assert!(matches!(
        validate_transaction_for_protocol(&v2, &state, &legacy_identity),
        Err(PulseError::InvalidTransaction(message))
            if message.contains("requires transaction version 1")
    ));

    let activated_identity = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    assert!(matches!(
        validate_transaction_for_protocol(&v1, &state, &activated_identity),
        Err(PulseError::InvalidTransaction(message))
            if message.contains("requires transaction version 2")
    ));
}

#[test]
fn v2_transaction_signed_for_another_chain_fails_on_the_active_chain() {
    let (state, signing_key, outpoint, public_key) = funded_state("pulsedag-testnet-v2");
    let wrong_chain_tx =
        signed_v2_transaction(&signing_key, &outpoint, &public_key, "pulsedag-private-v2");
    let identity = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );

    assert!(matches!(
        validate_transaction_for_protocol(&wrong_chain_tx, &state, &identity),
        Err(PulseError::InvalidTxid)
    ));
}

#[test]
fn version_classification_matches_the_frozen_rejection_matrix() {
    assert_eq!(
        classify_transaction_version(TransactionValidationPath::LegacyV1, 2),
        Err(TransactionRejectionClass::InactiveTransactionVersion)
    );
    assert_eq!(
        classify_transaction_version(TransactionValidationPath::ActivatedV2, 1),
        Err(TransactionRejectionClass::InactiveTransactionVersion)
    );
    assert_eq!(
        classify_transaction_version(TransactionValidationPath::LegacyV1, 3),
        Err(TransactionRejectionClass::UnsupportedTransactionVersion)
    );
}
