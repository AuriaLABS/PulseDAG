use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_transaction_with_mempool_policy_v3,
    accept_transaction_with_mempool_policy_v3_for_protocol,
    mempool_admission_v3::classify_mempool_conflicts_v3,
    mempool_policy_rejection_code_from_reason_v3,
    genesis::init_chain_state,
    mempool::{canonical_mempool_txids, reconcile_mempool},
    tx::{address_from_public_key, compute_txid, signing_message},
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    AcceptSource, MempoolPolicyV3, ProtocolActivationIdentity, TxAcceptanceResult,
};

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().to_bytes())
}

fn fund_address(
    state: &mut pulsedag_core::ChainState,
    txid: &str,
    address: String,
    amount: u64,
) -> OutPoint {
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

fn signed_tx(
    signing_key: &SigningKey,
    previous_outputs: Vec<OutPoint>,
    outputs: Vec<TxOutput>,
    fee: u64,
    nonce: u64,
) -> Transaction {
    let public_key = public_key_hex(signing_key);
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs: previous_outputs
            .into_iter()
            .map(|previous_output| TxInput {
                previous_output,
                public_key: public_key.clone(),
                signature: String::new(),
            })
            .collect(),
        outputs,
        fee,
        nonce,
    };

    let message = signing_message(&tx);
    let signature = signing_key.sign(&message);
    let signature_hex = hex::encode(signature.to_bytes());
    for input in &mut tx.inputs {
        input.signature = signature_hex.clone();
    }
    tx.txid = compute_txid(&tx);
    tx
}

fn sorted(mut txids: Vec<String>) -> Vec<String> {
    txids.sort();
    txids
}

fn rejected_code(result: TxAcceptanceResult) -> Option<String> {
    match result {
        TxAcceptanceResult::Rejected(reason) => {
            mempool_policy_rejection_code_from_reason_v3(&reason).map(ToOwned::to_owned)
        }
        _ => None,
    }
}

#[test]
fn conflict_package_vectors_are_identical_across_equivalent_state_and_reconcile() {
    let mut state = init_chain_state("mempool-conflict-golden-reconcile".to_string());
    let owner_key = signing_key(71);
    let child_key = signing_key(72);
    let owner_address = address_from_public_key(&public_key_hex(&owner_key));
    let child_address = address_from_public_key(&public_key_hex(&child_key));

    let funding_a = fund_address(&mut state, "fund-conflict-a", owner_address.clone(), 60);
    let funding_b = fund_address(&mut state, "fund-conflict-b", owner_address.clone(), 80);

    let direct_a = signed_tx(
        &owner_key,
        vec![funding_a.clone()],
        vec![TxOutput {
            address: child_address.clone(),
            amount: 50,
        }],
        10,
        1,
    );
    let direct_b = signed_tx(
        &owner_key,
        vec![funding_b.clone()],
        vec![TxOutput {
            address: child_address.clone(),
            amount: 70,
        }],
        10,
        2,
    );

    pulsedag_core::accept_transaction(direct_a.clone(), &mut state, AcceptSource::Rpc).unwrap();
    pulsedag_core::accept_transaction(direct_b.clone(), &mut state, AcceptSource::Rpc).unwrap();

    let shared_child = signed_tx(
        &child_key,
        vec![
            OutPoint {
                txid: direct_a.txid.clone(),
                index: 0,
            },
            OutPoint {
                txid: direct_b.txid.clone(),
                index: 0,
            },
        ],
        vec![TxOutput {
            address: child_address,
            amount: 110,
        }],
        10,
        3,
    );
    pulsedag_core::accept_transaction(
        shared_child.clone(),
        &mut state,
        AcceptSource::Rpc,
    )
    .unwrap();

    let incoming = signed_tx(
        &owner_key,
        vec![funding_a, funding_b],
        vec![TxOutput {
            address: owner_address,
            amount: 130,
        }],
        10,
        4,
    );

    let expected_direct = sorted(vec![direct_a.txid.clone(), direct_b.txid.clone()]);
    let expected_package = sorted(vec![
        direct_a.txid.clone(),
        direct_b.txid.clone(),
        shared_child.txid.clone(),
    ]);
    let expected_order = vec![
        direct_a.txid.clone(),
        direct_b.txid.clone(),
        shared_child.txid.clone(),
    ];

    let before = classify_mempool_conflicts_v3(&incoming, &state);
    assert_eq!(before.direct_conflict_txids, expected_direct);
    assert_eq!(before.conflict_package_txids, expected_package);
    assert_eq!(canonical_mempool_txids(&state), expected_order);

    let expected_first_seen = state.mempool.first_seen.clone();
    let expected_admission_height = state.mempool.admission_height.clone();

    let mut reordered = state.clone();
    reordered.mempool.transactions.clear();
    for tx in [&shared_child, &direct_b, &direct_a] {
        reordered
            .mempool
            .transactions
            .insert(tx.txid.clone(), tx.clone());
    }

    let reordered_before = classify_mempool_conflicts_v3(&incoming, &reordered);
    assert_eq!(reordered_before, before);
    assert_eq!(canonical_mempool_txids(&reordered), expected_order);

    let reconcile = reconcile_mempool(&mut state);
    let reordered_reconcile = reconcile_mempool(&mut reordered);
    assert!(reconcile.removed_txids.is_empty());
    assert!(reordered_reconcile.removed_txids.is_empty());
    assert_eq!(reconcile.kept_txids, expected_order);
    assert_eq!(reordered_reconcile.kept_txids, expected_order);

    let after = classify_mempool_conflicts_v3(&incoming, &state);
    let reordered_after = classify_mempool_conflicts_v3(&incoming, &reordered);
    assert_eq!(after, before);
    assert_eq!(reordered_after, before);
    assert_eq!(canonical_mempool_txids(&state), expected_order);
    assert_eq!(canonical_mempool_txids(&reordered), expected_order);
    assert_eq!(state.mempool.first_seen, expected_first_seen);
    assert_eq!(reordered.mempool.first_seen, expected_first_seen);
    assert_eq!(state.mempool.admission_height, expected_admission_height);
    assert_eq!(
        reordered.mempool.admission_height,
        expected_admission_height
    );

    let policy = MempoolPolicyV3::compatibility_default();
    let mut ordinary_state = state.clone();
    let ordinary_before_order = canonical_mempool_txids(&ordinary_state);
    let ordinary_before_first_seen = ordinary_state.mempool.first_seen.clone();
    let ordinary_before_admission_height = ordinary_state.mempool.admission_height.clone();
    let ordinary = accept_transaction_with_mempool_policy_v3(
        incoming.clone(),
        &mut ordinary_state,
        AcceptSource::Rpc,
        policy,
    );

    let mut protocol_state = reordered.clone();
    let identity = ProtocolActivationIdentity::legacy_from_state(&protocol_state);
    let protocol_before_order = canonical_mempool_txids(&protocol_state);
    let protocol_before_first_seen = protocol_state.mempool.first_seen.clone();
    let protocol_before_admission_height = protocol_state.mempool.admission_height.clone();
    let protocol = accept_transaction_with_mempool_policy_v3_for_protocol(
        incoming,
        &mut protocol_state,
        AcceptSource::Rpc,
        &identity,
        policy,
    );

    assert_eq!(
        rejected_code(ordinary),
        Some("MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED".to_string())
    );
    assert_eq!(
        rejected_code(protocol),
        Some("MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED".to_string())
    );
    assert_eq!(canonical_mempool_txids(&ordinary_state), ordinary_before_order);
    assert_eq!(canonical_mempool_txids(&protocol_state), protocol_before_order);
    assert_eq!(ordinary_state.mempool.first_seen, ordinary_before_first_seen);
    assert_eq!(protocol_state.mempool.first_seen, protocol_before_first_seen);
    assert_eq!(
        ordinary_state.mempool.admission_height,
        ordinary_before_admission_height
    );
    assert_eq!(
        protocol_state.mempool.admission_height,
        protocol_before_admission_height
    );
}