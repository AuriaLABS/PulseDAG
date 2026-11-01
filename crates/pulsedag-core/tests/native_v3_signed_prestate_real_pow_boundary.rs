//! Dormant native-v3 bridge: genuine KHeavyHash PoW does NOT authorize
//! signed-v2 monetary settlement, native DAG ordering, or contract execution.
//! Both existing inspectors are used independently; no production call sites.

use std::collections::BTreeMap;

use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    address_from_public_key,
    apply::apply_transaction,
    block_envelope_v3::{
        build_block_envelope_v3, validate_block_envelope_v3,
        validate_dormant_native_v3_monetary_body_preflight, BlockEnvelopeV3,
    },
    compute_txid_v2,
    genesis_v3::{dormant_native_genesis_envelope_v3_ns, init_chain_state_v3},
    header_v3::BlockHeaderV3,
    native_v3_envelope_trial::inspect_dormant_native_v3_full_envelope_trial,
    native_v3_signed_prestate_trial::{
        inspect_dormant_native_v3_explicit_order_trial,
        inspect_dormant_native_v3_signed_prestate_trial,
    },
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    signing_message_v2,
    state::ChainState,
    tx::TRANSACTION_VERSION_V2,
    types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
};

const CHAIN: &str = "pulsedag-dormant-native-v3-pow-signed-prestate-bridge";
const GENESIS_NS: u64 = 1_800_000_015_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn signer() -> SigningKey {
    SigningKey::from_bytes(&[41_u8; 32])
}

fn source_address() -> String {
    address_from_public_key(&hex::encode(signer().verifying_key().to_bytes()))
}

fn trusted_test_prestate() -> (ChainState, OutPoint) {
    let mut state = init_chain_state_v3(CHAIN.into(), GENESIS_NS / 1_000_000_000).unwrap();
    assert!(!state.contracts.config.enabled);
    // Explicitly synthetic, NOT created by zero-allocation mainnet genesis.
    let source = OutPoint {
        txid: "test-only-funded-outpoint".into(),
        index: 0,
    };
    state.utxo.utxos.insert(
        source.clone(),
        Utxo {
            outpoint: source.clone(),
            address: source_address(),
            amount: 100,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(source_address())
        .or_default()
        .push(source.clone());
    (state, source)
}

fn signed_spend(source: OutPoint, destination: &str, amount: u64, fee: u64, nonce: u64) -> Transaction {
    let key = signer();
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: source,
            public_key: hex::encode(key.verifying_key().to_bytes()),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: destination.into(),
            amount,
        }],
        fee,
        nonce,
    };
    tx.inputs[0].signature =
        hex::encode(key.sign(&signing_message_v2(&tx, CHAIN).unwrap()).to_bytes());
    tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
    tx
}

fn claim(marker: u64) -> Transaction {
    build_reward_claim_transaction_v3("pulse1bridge-test-miner", marker, CHAIN).unwrap()
}

fn mine_full_native_envelope(
    genesis_hash: &str,
    transactions: Vec<Transaction>,
    marker: u64,
) -> BlockEnvelopeV3 {
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![genesis_hash.into()],
        timestamp_ns: GENESIS_NS + 500_000_000 + marker * 100_000,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: "33".repeat(32), // Placeholder, NOT authenticated monetary state.
        blue_score: 1,
        height: 1,
    };
    for nonce in 0..4_096u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            verify_v3_header_pow(&header, CHAIN).unwrap();
            return build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        }
    }
    panic!("test-only easy-target KHeavyHash nonce search exhausted");
}

#[test]
fn authentic_pow_merkle_signed_transfer_is_still_rejected_by_full_dag_admission_guard() {
    let (state, source) = trusted_test_prestate();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let signed = signed_spend(source.clone(), "pulse1test-recipient-a", 90, 10, 1);
    let block = mine_full_native_envelope(&genesis.hash, vec![claim(1), signed.clone()], 1);
    let state_before = state.utxo.compute_state_root().unwrap();
    validate_block_envelope_v3(&block, CHAIN).unwrap();
    validate_dormant_native_v3_monetary_body_preflight(&block, CHAIN).unwrap();
    let preview = inspect_dormant_native_v3_signed_prestate_trial(&block, &state).unwrap();
    assert_eq!(preview.signed_transactions, 1);
    assert_eq!(preview.candidate_fees, 10);

    let mut independently_applied = state.clone();
    apply_transaction(&signed, &mut independently_applied, 1).unwrap();
    assert_eq!(
        preview.resulting_utxo_root,
        independently_applied.utxo.compute_state_root().unwrap()
    );
    assert_ne!(block.header.state_root, preview.resulting_utxo_root);
    assert_eq!(state.utxo.compute_state_root().unwrap(), state_before);
    assert!(state.utxo.utxos.contains_key(&source));

    // A separately valid Ed25519 spend and valid native PoW do not close the
    // authoritative DAG-order / fee / finality / state-root proof gap.
    let corpus = BTreeMap::from([
        (genesis.hash.clone(), genesis.clone()),
        (block.hash.clone(), block.clone()),
    ]);
    let error = inspect_dormant_native_v3_full_envelope_trial(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&block.hash),
        &corpus,
        0,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("ordinary transfers require authoritative signed-UTXO"),
        "{error}"
    );
}

#[test]
fn competing_pow_valid_siblings_cannot_double_spend_in_either_candidate_order() {
    let (state, source) = trusted_test_prestate();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a = mine_full_native_envelope(
        &genesis.hash,
        vec![claim(2), signed_spend(source.clone(), "pulse1a", 90, 10, 2)],
        2,
    );
    let b = mine_full_native_envelope(
        &genesis.hash,
        vec![claim(3), signed_spend(source.clone(), "pulse1b", 80, 20, 3)],
        3,
    );
    assert_ne!(a.hash, b.hash);
    assert_eq!(a.header.parents, b.header.parents);
    assert_eq!(
        inspect_dormant_native_v3_signed_prestate_trial(&a, &state)
            .unwrap()
            .candidate_fees,
        10
    );
    assert_eq!(
        inspect_dormant_native_v3_signed_prestate_trial(&b, &state)
            .unwrap()
            .candidate_fees,
        20
    );
    let state_before = state.utxo.compute_state_root().unwrap();
    assert!(inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), b.clone()], &state).is_err());
    assert!(inspect_dormant_native_v3_explicit_order_trial(&[b, a], &state).is_err());
    assert_eq!(state.utxo.compute_state_root().unwrap(), state_before);
    assert!(state.utxo.utxos.contains_key(&source));
}

#[test]
fn mined_bad_signature_hidden_claim_and_enabled_contracts_fail_closed() {
    let (state, source) = trusted_test_prestate();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let good = signed_spend(source, "pulse1valid", 90, 10, 4);
    let mut forged = good.clone();
    forged.inputs[0].signature = "00".repeat(64);
    forged.txid = compute_txid_v2(&forged, CHAIN).unwrap();
    let bad = mine_full_native_envelope(&genesis.hash, vec![claim(4), forged], 4);
    // Structural body hash + real PoW may still validate a forged signature.
    validate_dormant_native_v3_monetary_body_preflight(&bad, CHAIN).unwrap();
    assert!(inspect_dormant_native_v3_signed_prestate_trial(&bad, &state).is_err());

    let hidden = mine_full_native_envelope(&genesis.hash, vec![claim(5), claim(6)], 5);
    assert!(validate_dormant_native_v3_monetary_body_preflight(&hidden, CHAIN).is_err());
    assert!(inspect_dormant_native_v3_signed_prestate_trial(&hidden, &state).is_err());

    let valid = mine_full_native_envelope(&genesis.hash, vec![claim(7), good], 7);
    let mut enabled = state.clone();
    enabled.contracts.config.enabled = true;
    let error = inspect_dormant_native_v3_signed_prestate_trial(&valid, &enabled)
        .unwrap_err()
        .to_string();
    assert!(error.contains("smart contracts must remain INACTIVE"), "{error}");
    assert!(!state.contracts.config.enabled);
}
