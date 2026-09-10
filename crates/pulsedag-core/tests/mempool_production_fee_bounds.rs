use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_transaction_with_mempool_policy_v3,
    genesis::init_chain_state,
    mempool_policy_rejection_code_from_reason_v3,
    mempool_v3::MEMPOOL_POLICY_V3_PRODUCTION_MAX_TRANSACTION_FEE,
    tx::{address_from_public_key, compute_txid, signing_message},
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    validation::validate_transaction,
    AcceptSource, MempoolPolicyV3, TxAcceptanceResult,
};

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public_key_hex(key: &SigningKey) -> String {
    hex::encode(key.verifying_key().to_bytes())
}

fn funded_state(
    name: &str,
    key: &SigningKey,
    amount: u64,
) -> (pulsedag_core::ChainState, OutPoint) {
    let mut state = init_chain_state(name.to_string());
    let address = address_from_public_key(&public_key_hex(key));
    let outpoint = OutPoint {
        txid: format!("{name}-funding"),
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
    (state, outpoint)
}

fn signed_tx(
    key: &SigningKey,
    previous_output: OutPoint,
    output_amount: u64,
    fee: u64,
    nonce: u64,
) -> Transaction {
    let public_key = public_key_hex(key);
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs: vec![TxInput {
            previous_output,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: "pulse1productionfeebounds".to_string(),
            amount: output_amount,
        }],
        fee,
        nonce,
    };
    tx.inputs[0].signature = hex::encode(key.sign(&signing_message(&tx)).to_bytes());
    tx.txid = compute_txid(&tx);
    tx
}

fn rejection_code(result: TxAcceptanceResult) -> Option<String> {
    match result {
        TxAcceptanceResult::Rejected(reason) => {
            mempool_policy_rejection_code_from_reason_v3(&reason).map(ToOwned::to_owned)
        }
        _ => None,
    }
}

#[test]
fn consensus_transaction_validation_still_accepts_zero_fee() {
    let key = signing_key(101);
    let (state, funding) = funded_state("production-fee-zero-consensus", &key, 10);
    let tx = signed_tx(&key, funding, 10, 0, 1);

    validate_transaction(&tx, &state)
        .expect("zero fee remains valid at consensus transaction layer");
}

#[test]
fn production_zero_fee_relay_rejects_before_mempool_insertion() {
    let key = signing_key(102);
    let (mut state, funding) = funded_state("production-fee-zero-relay", &key, 10);
    let funded_outpoint = funding.clone();
    let utxo_count = state.utxo.utxos.len();
    let tx = signed_tx(&key, funding, 10, 0, 2);

    let result = accept_transaction_with_mempool_policy_v3(
        tx,
        &mut state,
        AcceptSource::Rpc,
        MempoolPolicyV3::production_default(),
    );

    assert_eq!(
        rejection_code(result).as_deref(),
        Some("MEMPOOL_V3_BELOW_MIN_RELAY_FEE_RATE")
    );
    assert!(state.mempool.transactions.is_empty());
    assert!(state.mempool.spent_outpoints.is_empty());
    assert_eq!(state.utxo.utxos.len(), utxo_count);
    assert!(state.utxo.utxos.contains_key(&funded_outpoint));
}

#[test]
fn production_fee_ceiling_is_inclusive_and_max_plus_one_rejects_before_mempool_insertion() {
    let key = signing_key(103);
    let exact_max = MEMPOOL_POLICY_V3_PRODUCTION_MAX_TRANSACTION_FEE;

    let (mut accepted_state, accepted_funding) =
        funded_state("production-fee-exact-max", &key, exact_max + 1);
    let accepted = signed_tx(&key, accepted_funding, 1, exact_max, 3);
    assert_eq!(
        accept_transaction_with_mempool_policy_v3(
            accepted.clone(),
            &mut accepted_state,
            AcceptSource::Rpc,
            MempoolPolicyV3::production_default(),
        ),
        TxAcceptanceResult::Accepted
    );
    assert!(accepted_state
        .mempool
        .transactions
        .contains_key(&accepted.txid));

    let over_max = exact_max + 1;
    let (mut rejected_state, rejected_funding) =
        funded_state("production-fee-over-max", &key, over_max + 1);
    let funded_outpoint = rejected_funding.clone();
    let utxo_count = rejected_state.utxo.utxos.len();
    let rejected = signed_tx(&key, rejected_funding, 1, over_max, 4);
    let result = accept_transaction_with_mempool_policy_v3(
        rejected,
        &mut rejected_state,
        AcceptSource::Rpc,
        MempoolPolicyV3::production_default(),
    );

    assert_eq!(
        rejection_code(result).as_deref(),
        Some("MEMPOOL_V3_ABOVE_MAX_TRANSACTION_FEE")
    );
    assert!(rejected_state.mempool.transactions.is_empty());
    assert!(rejected_state.mempool.spent_outpoints.is_empty());
    assert_eq!(rejected_state.utxo.utxos.len(), utxo_count);
    assert!(rejected_state.utxo.utxos.contains_key(&funded_outpoint));
}
