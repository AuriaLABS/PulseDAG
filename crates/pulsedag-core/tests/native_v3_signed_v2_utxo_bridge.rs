//! TEST-ONLY bridge: genuine Ed25519-signed *v2* transfers committed to
//! native-v3 header envelopes and fed into the detached UTXO accounting model.
//!
//! The current native v3 envelope explicitly requires transaction VERSION 2.
//! Reserved PQC tx version 3 serializes but does not have ML-DSA verification.
//! This is NOT a signed PQC v3 transaction, monetary reward settlement,
//! canonical ordered DAG replay, live admission or a balance/state-root update.

use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    address_from_public_key,
    block_envelope_v3::build_block_envelope_v3,
    compute_txid_v2,
    errors::PulseError,
    genesis_v3::{dormant_native_genesis_envelope_v3_ns, init_chain_state_v3},
    header_v3::BlockHeaderV3,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    signing_message_v2,
    state::ChainState,
    tx::TRANSACTION_VERSION_V2,
    types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    validation_v2::validate_transaction_v2,
};

#[path = "support/native_v3_utxo_preview.rs"]
#[allow(dead_code)]
// shared test helper exposes additional UTXO adversarial checks in a separate suite
mod detached_utxo;

use detached_utxo::{apply, coin, fingerprint, undo, HypotheticalBlockEffects, PreviewSet};

const CHAIN: &str = "pulsedag-dormant-native-v3-signed-v2-utxo-bridge";
const GENESIS_NS: u64 = 1_800_000_007_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn signer() -> SigningKey {
    SigningKey::from_bytes(&[29_u8; 32])
}

fn test_state() -> (ChainState, Utxo) {
    let mut state = init_chain_state_v3(CHAIN.into(), GENESIS_NS / 1_000_000_000).unwrap();
    assert!(!state.contracts.config.enabled);
    // Synthetic test UTXO, expressly NOT mintable from zero-allocation genesis.
    let public = hex::encode(signer().verifying_key().to_bytes());
    let funding = coin(
        "test-only-preloaded-funding",
        100,
        &address_from_public_key(&public),
        false,
        1,
    );
    state
        .utxo
        .utxos
        .insert(funding.outpoint.clone(), funding.clone());
    state
        .utxo
        .address_index
        .entry(funding.address.clone())
        .or_default()
        .push(funding.outpoint.clone());
    (state, funding)
}

fn signed_v2_transfer(source: &OutPoint, recipient: &str, amount: u64, fee: u64) -> Transaction {
    let signing_key = signer();
    let public_key = hex::encode(signing_key.verifying_key().to_bytes());
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: source.clone(),
            public_key,
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: recipient.into(),
            amount,
        }],
        fee,
        nonce: 7,
    };
    tx.inputs[0].signature = hex::encode(
        signing_key
            .sign(&signing_message_v2(&tx, CHAIN).unwrap())
            .to_bytes(),
    );
    tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
    tx
}

fn mined_native_v3_transfer(
    tx: &Transaction,
    parent: String,
    marker: u8,
) -> pulsedag_core::block_envelope_v3::BlockEnvelopeV3 {
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent],
        timestamp_ns: GENESIS_NS + 500_000_000 + u64::from(marker) * 100_000,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(std::slice::from_ref(tx)),
        state_root: "33".repeat(32), // Placeholder, NOT monetary state root.
        blue_score: 1,
        height: 1,
    };
    for nonce in 0..1_024u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            verify_v3_header_pow(&header, CHAIN).unwrap();
            return build_block_envelope_v3(CHAIN, header, vec![tx.clone()])
                .expect("v3 native envelope commits genuine chain-bound signed-v2 tx");
        }
    }
    panic!("test-only easy target nonce budget exceeded");
}

/// STRICT LOCAL TEST-ONLY conversion: do not assume v2 signature validation
/// is the frozen v3 monetary admission or fee policy. This bridge additionally
/// rejects implicit fee/burn gaps, which the standalone v2 mempool validator
/// does not necessarily reject. No live consumer.
fn signed_v2_effect(
    block_id: &str,
    tx: &Transaction,
    state: &ChainState,
) -> Result<HypotheticalBlockEffects, String> {
    if state.chain_id != CHAIN {
        return Err("chain identity mismatch".into());
    }
    validate_transaction_v2(tx, state, CHAIN)
        .map_err(|error| format!("v2 signed transfer invalid: {error}"))?;
    let spent: Vec<Utxo> = tx
        .inputs
        .iter()
        .map(|input| {
            state
                .utxo
                .utxos
                .get(&input.previous_output)
                .cloned()
                .ok_or("input lacks a settled UTXO".to_string())
        })
        .collect::<Result<_, _>>()?;
    let created: Vec<Utxo> = tx
        .outputs
        .iter()
        .enumerate()
        .map(|(i, output)| {
            Ok(Utxo {
                outpoint: OutPoint {
                    txid: tx.txid.clone(),
                    index: u32::try_from(i).map_err(|_| "output index overflow")?,
                },
                address: output.address.clone(),
                amount: output.amount,
                coinbase: false,
                height: 1,
            })
        })
        .collect::<Result<_, String>>()?;
    let inputs = spent.iter().try_fold(0u128, |sum, coin| {
        sum.checked_add(u128::from(coin.amount))
            .ok_or("input sum overflow".to_string())
    })?;
    let outputs = created.iter().try_fold(0u128, |sum, coin| {
        sum.checked_add(u128::from(coin.amount))
            .ok_or("output sum overflow".to_string())
    })?;
    if inputs != outputs + u128::from(tx.fee) {
        return Err("strict test-only input/output/fee equality mismatch".into());
    }
    Ok(HypotheticalBlockEffects {
        block: block_id.into(),
        spent,
        created,
        test_issuance: 0,
        // This *illustrative* fee burn is NOT the real v3 fee-to-miner rule.
        test_fee_burn: tx.fee,
    })
}

#[test]
fn genuine_signed_v2_spends_have_native_v3_header_pow_merkle_and_reversible_preview() {
    let (state, funding) = test_state();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let alice = signed_v2_transfer(&funding.outpoint, "pulse1alice", 90, 10);
    let bob = signed_v2_transfer(&funding.outpoint, "pulse1bob", 92, 8);
    for tx in [&alice, &bob] {
        validate_transaction_v2(tx, &state, CHAIN).unwrap();
    }
    assert_ne!(alice.txid, bob.txid);
    let env_a = mined_native_v3_transfer(&alice, genesis.hash.clone(), 1);
    let env_b = mined_native_v3_transfer(&bob, genesis.hash.clone(), 2);
    assert_ne!(env_a.hash, env_b.hash);
    assert_eq!(env_a.header.parents, env_b.header.parents);
    assert_eq!(env_a.transactions[0].txid, alice.txid);
    assert_eq!(env_b.transactions[0].txid, bob.txid);

    let initial = PreviewSet::from([(funding.outpoint.clone(), funding.clone())]);
    let effects_a = signed_v2_effect(&env_a.hash, &alice, &state).unwrap();
    let effects_b = signed_v2_effect(&env_b.hash, &bob, &state).unwrap();
    let after_a = apply(&initial, &effects_a).unwrap();
    let after_b = apply(&initial, &effects_b).unwrap();
    assert_eq!(detached_utxo::total_supply(&after_a).unwrap(), 90);
    assert_eq!(detached_utxo::total_supply(&after_b).unwrap(), 92);
    assert!(
        apply(&after_a, &effects_b).is_err(),
        "conflicting signed spends cannot coexist"
    );
    assert!(
        apply(&after_b, &effects_a).is_err(),
        "opposite spend is also exclusive"
    );

    let from_alice = undo(&after_a, &effects_a).unwrap();
    assert_eq!(fingerprint(&from_alice), fingerprint(&initial));
    let new_branch = apply(&from_alice, &effects_b).unwrap();
    assert_eq!(fingerprint(&new_branch), fingerprint(&after_b));
    let from_bob = undo(&after_b, &effects_b).unwrap();
    assert_eq!(fingerprint(&from_bob), fingerprint(&initial));
    assert_eq!(
        fingerprint(&apply(&from_bob, &effects_a).unwrap()),
        fingerprint(&after_a)
    );
    assert_eq!(state.utxo.utxos[&funding.outpoint].amount, 100);
}

#[test]
fn signed_v2_carrier_refuses_tamper_cross_chain_and_reserved_unsigned_pqc_v3() {
    let (state, funding) = test_state();
    let tx = signed_v2_transfer(&funding.outpoint, "pulse1alice", 90, 10);
    let mut modified = tx.clone();
    modified.outputs[0].address = "pulse1hijacker".into();
    modified.txid = compute_txid_v2(&modified, CHAIN).unwrap();
    assert!(matches!(
        validate_transaction_v2(&modified, &state, CHAIN),
        Err(PulseError::InvalidSignature)
    ));

    let mut bad_signature = tx.clone();
    bad_signature.inputs[0].signature = "00".repeat(64);
    bad_signature.txid = compute_txid_v2(&bad_signature, CHAIN).unwrap();
    assert!(validate_transaction_v2(&bad_signature, &state, CHAIN).is_err());

    assert!(validate_transaction_v2(&tx, &state, "other-network").is_err());
    let mut bad_txid = tx.clone();
    bad_txid.txid = "00".repeat(32);
    assert!(validate_transaction_v2(&bad_txid, &state, CHAIN).is_err());

    let mut gap = signed_v2_transfer(&funding.outpoint, "pulse1alice", 85, 10);
    assert!(validate_transaction_v2(&gap, &state, CHAIN).is_ok());
    assert!(signed_v2_effect("gap", &gap, &state).is_err());
    gap.version = pulsedag_core::TRANSACTION_VERSION_V3;
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![genesis.hash],
        timestamp_ns: GENESIS_NS + 500_000_000,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(std::slice::from_ref(&gap)),
        state_root: "33".repeat(32),
        blue_score: 1,
        height: 1,
    };
    assert!(build_block_envelope_v3(CHAIN, header, vec![gap]).is_err());

    let mut spent_state = state.clone();
    spent_state.utxo.utxos.remove(&funding.outpoint);
    assert!(validate_transaction_v2(&tx, &spent_state, CHAIN).is_err());
    let mut foreign_state = state.clone();
    foreign_state.chain_id = "other-network".into();
    assert!(signed_v2_effect("mismatch", &tx, &foreign_state).is_err());
}
