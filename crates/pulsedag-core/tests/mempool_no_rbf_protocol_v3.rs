use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept_transaction_with_mempool_policy_v3,
    accept_transaction_with_mempool_policy_v3_for_protocol, assess_mempool_replacement_v3,
    genesis::init_chain_state,
    mempool::canonical_mempool_txids,
    mempool_policy_rejection_code_from_reason_v3,
    tx::{
        address_from_public_key, compute_txid, compute_txid_v2, signing_message, signing_message_v2,
    },
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    AcceptSource, MempoolPolicyV3, ProtocolActivationIdentity, TxAcceptanceResult,
    GHOSTDAG_V1_ORDERING_VERSION, TRANSACTION_VERSION_V2,
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
    previous_output: OutPoint,
    output_address: String,
    output_amount: u64,
    fee: u64,
    nonce: u64,
) -> Transaction {
    let public_key = public_key_hex(signing_key);
    let mut tx = Transaction {
        txid: String::new(),
        version: 1,
        inputs: vec![TxInput {
            previous_output,
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: output_address,
            amount: output_amount,
        }],
        fee,
        nonce,
    };

    let signature = signing_key.sign(&signing_message(&tx));
    tx.inputs[0].signature = hex::encode(signature.to_bytes());
    tx.txid = compute_txid(&tx);
    tx
}

fn signed_v2_tx(
    signing_key: &SigningKey,
    chain_id: &str,
    previous_output: OutPoint,
    output_address: String,
    output_amount: u64,
    fee: u64,
    nonce: u64,
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
            address: output_address,
            amount: output_amount,
        }],
        fee,
        nonce,
    };

    let message = signing_message_v2(&tx, chain_id).unwrap();
    tx.inputs[0].signature = hex::encode(signing_key.sign(&message).to_bytes());
    tx.txid = compute_txid_v2(&tx, chain_id).unwrap();
    tx
}

fn rejected_code(result: TxAcceptanceResult) -> Option<String> {
    match result {
        TxAcceptanceResult::Rejected(reason) => {
            mempool_policy_rejection_code_from_reason_v3(&reason).map(ToOwned::to_owned)
        }
        _ => None,
    }
}

fn assert_rejection_is_pre_mutation(
    before: &pulsedag_core::ChainState,
    after: &pulsedag_core::ChainState,
) {
    assert_eq!(
        canonical_mempool_txids(after),
        canonical_mempool_txids(before)
    );
    assert_eq!(
        after.mempool.spent_outpoints,
        before.mempool.spent_outpoints
    );
    assert_eq!(after.mempool.first_seen, before.mempool.first_seen);
    assert_eq!(
        after.mempool.admission_height,
        before.mempool.admission_height
    );
    assert_eq!(
        after.mempool.next_first_seen,
        before.mempool.next_first_seen
    );
    assert_eq!(
        after.mempool.counters.rejected_total,
        before.mempool.counters.rejected_total.saturating_add(1)
    );
}

#[test]
fn frozen_transaction_protocol_does_not_infer_rbf_from_fee_rate_nonce_or_policy_flag() {
    let mut state = init_chain_state("mempool-no-rbf-protocol-v3".to_string());
    let owner_key = signing_key(91);
    let owner_address = address_from_public_key(&public_key_hex(&owner_key));
    let funding = fund_address(
        &mut state,
        "fund-no-rbf-protocol-v3",
        owner_address.clone(),
        100,
    );

    let incumbent = signed_tx(&owner_key, funding.clone(), owner_address.clone(), 99, 1, 1);
    pulsedag_core::accept_transaction(incumbent.clone(), &mut state, AcceptSource::Rpc).unwrap();

    let incoming = signed_tx(&owner_key, funding, owner_address.clone(), 80, 20, 999);
    assert_ne!(incoming.txid, incumbent.txid);
    assert!(incoming.fee > incumbent.fee);
    assert!(incoming.nonce > incumbent.nonce);

    let assessment = assess_mempool_replacement_v3(&incoming, &state).unwrap();
    assert_eq!(
        assessment.direct_conflict_txids,
        vec![incumbent.txid.clone()]
    );
    assert_eq!(
        assessment.replacement_package_txids,
        vec![incumbent.txid.clone()]
    );
    assert!(assessment.pays_strictly_higher_total_fee);
    assert!(assessment.pays_strictly_higher_fee_rate);
    assert!(assessment.positive_fee_delta_over_package.is_some());

    for replacement_enabled in [false, true] {
        let policy = MempoolPolicyV3 {
            replacement_enabled,
            ..MempoolPolicyV3::compatibility_default()
        };
        let mut ordinary = state.clone();
        let ordinary_before = ordinary.clone();
        let ordinary_result = accept_transaction_with_mempool_policy_v3(
            incoming.clone(),
            &mut ordinary,
            AcceptSource::Rpc,
            policy,
        );
        assert_eq!(
            rejected_code(ordinary_result),
            Some("MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED".to_string())
        );
        assert_rejection_is_pre_mutation(&ordinary_before, &ordinary);
    }

    let mut activated = init_chain_state("mempool-no-rbf-protocol-v3-activated-v2".to_string());
    let identity = ProtocolActivationIdentity::activated_v2(
        activated.chain_id.clone(),
        activated.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    let activated_funding = fund_address(
        &mut activated,
        "fund-no-rbf-protocol-v3-activated-v2",
        owner_address.clone(),
        100,
    );
    let incumbent_v2 = signed_v2_tx(
        &owner_key,
        &activated.chain_id,
        activated_funding.clone(),
        owner_address.clone(),
        99,
        1,
        1,
    );
    assert_eq!(incumbent_v2.version, TRANSACTION_VERSION_V2);
    assert_eq!(
        accept_transaction_with_mempool_policy_v3_for_protocol(
            incumbent_v2.clone(),
            &mut activated,
            AcceptSource::Rpc,
            &identity,
            MempoolPolicyV3::compatibility_default(),
        ),
        TxAcceptanceResult::Accepted
    );

    let incoming_v2 = signed_v2_tx(
        &owner_key,
        &activated.chain_id,
        activated_funding,
        owner_address,
        80,
        20,
        999,
    );
    assert_eq!(incoming_v2.version, TRANSACTION_VERSION_V2);
    assert_ne!(incoming_v2.txid, incumbent_v2.txid);
    assert!(incoming_v2.fee > incumbent_v2.fee);
    assert!(incoming_v2.nonce > incumbent_v2.nonce);

    let activated_assessment = assess_mempool_replacement_v3(&incoming_v2, &activated).unwrap();
    assert_eq!(
        activated_assessment.direct_conflict_txids,
        vec![incumbent_v2.txid.clone()]
    );
    assert_eq!(
        activated_assessment.replacement_package_txids,
        vec![incumbent_v2.txid.clone()]
    );
    assert!(activated_assessment.pays_strictly_higher_total_fee);
    assert!(activated_assessment.pays_strictly_higher_fee_rate);
    assert!(activated_assessment
        .positive_fee_delta_over_package
        .is_some());

    let activated_before = activated.clone();
    let activated_result = accept_transaction_with_mempool_policy_v3_for_protocol(
        incoming_v2,
        &mut activated,
        AcceptSource::Rpc,
        &identity,
        MempoolPolicyV3 {
            replacement_enabled: true,
            ..MempoolPolicyV3::compatibility_default()
        },
    );
    assert_eq!(
        rejected_code(activated_result),
        Some("MEMPOOL_V3_REPLACEMENT_NOT_AUTHORIZED".to_string())
    );
    assert_rejection_is_pre_mutation(&activated_before, &activated);
}
