//! TEST-ONLY bridge: genuine Ed25519-signed *v2* transfers committed to
//! native-v3 header envelopes and fed into the detached UTXO accounting model.
//!
//! The current native v3 envelope explicitly requires transaction VERSION 2.
//! Reserved PQC tx version 3 serializes but does not have ML-DSA verification.
//! This is NOT a signed PQC v3 transaction, monetary reward settlement,
//! canonical ordered DAG replay, live admission or a balance/state-root update.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    address_from_public_key,
    block_envelope_v3::build_block_envelope_v3,
    compute_txid_v2,
    errors::PulseError,
    genesis_v3::{dormant_native_genesis_envelope_v3_ns, init_chain_state_v3},
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    signing_message_v2,
    state::ChainState,
    tx::TRANSACTION_VERSION_V2,
    types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
    validation_v2::validate_transaction_v2,
};

#[path = "support/native_v3_trial_order_preview.rs"]
#[allow(dead_code)]
mod trial_order_preview;

use trial_order_preview::{preview_unapproved_native_v3_order, UnapprovedTrialOrder};

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

const HARD_BITS: u32 = 0x1f7f_ffff; // Unequal target, trial work 512 vs 2.

fn mine_full_envelope(
    mut parents: Vec<String>,
    transactions: Vec<Transaction>,
    score: u64,
    height: u64,
    difficulty: u32,
    timestamp_ns: u64,
) -> pulsedag_core::block_envelope_v3::BlockEnvelopeV3 {
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: "33".repeat(32), // Real monetary root NOT validated here.
        blue_score: score,
        height,
    };
    for nonce in 0..65_536u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            verify_v3_header_pow(&header, CHAIN).unwrap();
            return build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        }
    }
    panic!("failed PoW trial for native-v3 envelope");
}

#[derive(Debug)]
struct ConditionalTrialReplay {
    accepted: Vec<String>,
    rejected_red: Vec<String>,
    rejected_conflict: Vec<String>,
    accepted_fees: u64,
    unspent_atoms: u128,
    snapshot: PreviewSet,
}

// This diagnostic intentionally compares TWO UNAPPROVED interpretations:
// include all side-block transactions vs exclude red blocks, followed by the
// same fail-closed double-spend rule for eligible transactions. The comparison
// is EVIDENCE OF POLICY SENSITIVITY, not a selection/endorsement of either.
fn replay_trial_order(
    order: &UnapprovedTrialOrder,
    original: &PreviewSet,
    effects: &BTreeMap<String, (Transaction, HypotheticalBlockEffects)>,
    include_red: bool,
) -> ConditionalTrialReplay {
    let mut snapshot = original.clone();
    let mut accepted = Vec::new();
    let mut rejected_red = Vec::new();
    let mut rejected_conflict = Vec::new();
    let mut accepted_fees = 0u64;
    let reds: BTreeSet<_> = order.side_reds.iter().cloned().collect();
    let mut seen = BTreeSet::new();
    for hash in &order.ordered_blocks {
        if let Some((tx, effect)) = effects.get(hash) {
            assert!(seen.insert(tx.txid.clone()), "no duplicate committed txid");
            if !include_red && reds.contains(hash) {
                rejected_red.push(tx.txid.clone());
                continue;
            }
            assert_eq!(effect.block, *hash);
            if tx
                .inputs
                .iter()
                .any(|input| !snapshot.contains_key(&input.previous_output))
            {
                // All inputs MUST exist in the original fixture; otherwise a
                // bogus/missing input cannot masquerade as an ordinary conflict.
                assert!(
                    tx.inputs
                        .iter()
                        .all(|input| { original.contains_key(&input.previous_output) }),
                    "unknown original input must not be silently skipped"
                );
                rejected_conflict.push(tx.txid.clone());
                continue;
            }
            snapshot = detached_utxo::apply(&snapshot, effect).unwrap();
            accepted_fees = accepted_fees.checked_add(tx.fee).unwrap();
            accepted.push(tx.txid.clone());
        }
    }
    ConditionalTrialReplay {
        accepted,
        rejected_red,
        rejected_conflict,
        accepted_fees,
        unspent_atoms: detached_utxo::total_supply(&snapshot).unwrap(),
        snapshot,
    }
}

#[test]
fn native_v3_k_red_policy_ambiguity_changes_signed_fee_and_utxo_outcome() {
    let (mut state, funding_a) = test_state();
    // A second artificial settled coin is needed to distinguish a true
    // blue/red fee choice from a mere double-spend conflict.
    let funding_b = coin("test-only-second-funding", 80, &funding_a.address, false, 1);
    state
        .utxo
        .utxos
        .insert(funding_b.outpoint.clone(), funding_b.clone());
    state
        .utxo
        .address_index
        .entry(funding_b.address.clone())
        .or_default()
        .push(funding_b.outpoint.clone());
    let initial = PreviewSet::from([
        (funding_a.outpoint.clone(), funding_a.clone()),
        (funding_b.outpoint.clone(), funding_b.clone()),
    ]);

    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a_tx = signed_v2_transfer(&funding_a.outpoint, "pulse1selected", 90, 10);
    let b_tx = signed_v2_transfer(&funding_b.outpoint, "pulse1independent", 73, 7);
    let c_tx = signed_v2_transfer(&funding_a.outpoint, "pulse1conflict", 92, 8);
    for tx in [&a_tx, &b_tx, &c_tx] {
        validate_transaction_v2(tx, &state, CHAIN).unwrap();
    }
    let a = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![a_tx.clone()],
        1,
        1,
        HARD_BITS,
        GENESIS_NS + 500_000_000,
    );
    let b = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![b_tx.clone()],
        1,
        1,
        EASY_BITS,
        GENESIS_NS + 600_000_000,
    );
    let c = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![c_tx.clone()],
        1,
        1,
        EASY_BITS,
        GENESIS_NS + 700_000_000,
    );
    let mut headers = BTreeMap::from([
        (genesis.hash.clone(), genesis.header),
        (a.hash.clone(), a.header.clone()),
        (b.hash.clone(), b.header.clone()),
        (c.hash.clone(), c.header.clone()),
    ]);
    let effects = BTreeMap::from([
        (
            a.hash.clone(),
            (
                a_tx.clone(),
                signed_v2_effect(&a.hash, &a_tx, &state).unwrap(),
            ),
        ),
        (
            b.hash.clone(),
            (
                b_tx.clone(),
                signed_v2_effect(&b.hash, &b_tx, &state).unwrap(),
            ),
        ),
        (
            c.hash.clone(),
            (
                c_tx.clone(),
                signed_v2_effect(&c.hash, &c_tx, &state).unwrap(),
            ),
        ),
    ]);
    let mut parents = vec![a.hash.clone(), b.hash.clone(), c.hash.clone()];
    parents.sort();
    for k in [0_usize, 1, 2] {
        // The K-specific blue score is committed in the new *mined* join.
        let join = mine_full_envelope(
            parents.clone(),
            vec![],
            2 + k as u64,
            2,
            EASY_BITS,
            GENESIS_NS + 1_000_000_000 + k as u64 * 10_000,
        );
        headers.insert(join.hash.clone(), join.header.clone());
        let order = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&join.hash),
            &headers,
            k,
        )
        .expect("complete authentic native-v3 GHOSTDAG trial ancestor graph");
        let trial = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&join.hash),
            &headers,
            k,
        )
        .unwrap();
        assert_eq!(
            trial.blocks[&join.hash].selected_parent.as_deref(),
            Some(a.hash.as_str())
        );
        assert_eq!(
            order.selected_spine,
            vec![genesis.hash.clone(), a.hash.clone(), join.hash.clone()]
        );
        assert_eq!(order.ordered_blocks.len(), 5);
        assert_eq!(order.side_reds.len(), 2 - k);
        assert_eq!(order.side_blues.len(), k);

        let blue_only = replay_trial_order(&order, &initial, &effects, false);
        let all = replay_trial_order(&order, &initial, &effects, true);
        assert_eq!(all.accepted.len(), 2);
        assert_eq!(all.rejected_conflict.len(), 1);
        assert!(all.accepted.contains(&a_tx.txid));
        assert!(all.accepted.contains(&b_tx.txid));
        assert_eq!(all.rejected_conflict, vec![c_tx.txid.clone()]);
        assert_eq!(all.accepted_fees, 17);
        assert_eq!(all.unspent_atoms, 163);
        assert!(all.rejected_red.is_empty());

        assert!(blue_only.accepted.contains(&a_tx.txid));
        assert_eq!(blue_only.rejected_red.len(), 2 - k);
        assert_eq!(
            blue_only.unspent_atoms,
            180 - blue_only.accepted_fees as u128
        );
        assert_eq!(
            blue_only.accepted_fees,
            if order.side_reds.contains(&b.hash) {
                10
            } else {
                17
            }
        );
        assert_eq!(
            blue_only.accepted.contains(&b_tx.txid),
            !order.side_reds.contains(&b.hash)
        );
        assert_eq!(
            blue_only.rejected_conflict.contains(&c_tx.txid),
            !order.side_reds.contains(&c.hash)
        );
        if k == 0 {
            assert_eq!(blue_only.accepted_fees, 10);
            assert_eq!(blue_only.unspent_atoms, 170);
            assert_ne!(
                detached_utxo::fingerprint(&blue_only.snapshot),
                detached_utxo::fingerprint(&all.snapshot)
            );
        }
        if k == 2 {
            assert_eq!(
                detached_utxo::fingerprint(&blue_only.snapshot),
                detached_utxo::fingerprint(&all.snapshot)
            );
        }
        // Rebuilding the test fixture in reversed map insertion order must
        // preserve the same trial economic result under a fixed policy.
        let reversed: BTreeMap<_, _> = headers
            .iter()
            .rev()
            .map(|(hash, h)| (hash.clone(), h.clone()))
            .collect();
        let order_again = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&join.hash),
            &reversed,
            k,
        )
        .unwrap();
        assert_eq!(order, order_again);
        assert_eq!(
            replay_trial_order(&order_again, &initial, &effects, false).accepted,
            blue_only.accepted
        );
        assert_eq!(
            replay_trial_order(&order_again, &initial, &effects, true).accepted,
            all.accepted
        );
    }
    // No tests above change live chainstate; genesis funding is SYNTHETIC.
    assert_eq!(state.utxo.utxos[&funding_a.outpoint].amount, 100);
    assert_eq!(state.utxo.utxos[&funding_b.outpoint].amount, 80);
}

/// A strict **non-normative diagnostic**, not a new network-level rule.
/// Duplicate txids are harmless at the *individual envelope* boundary but
/// require an explicit canonical DAG-level policy before monetary replay.
/// This trial check deliberately rejects them even if the second header is
/// classified red; "reject" vs "skip duplicate" remains a policy choice.
fn diagnostic_unique_native_v3_txids(
    order: &UnapprovedTrialOrder,
    trusted_genesis: &str,
    envelopes: &BTreeMap<String, pulsedag_core::block_envelope_v3::BlockEnvelopeV3>,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut checked_blocks = 0_usize;
    for hash in &order.ordered_blocks {
        if hash == trusted_genesis {
            continue;
        }
        let envelope = envelopes
            .get(hash)
            .ok_or_else(|| format!("missing authenticated native envelope {hash}"))?;
        if envelope.hash != *hash {
            return Err("envelope identity mismatch".into());
        }
        pulsedag_core::block_envelope_v3::validate_block_envelope_v3(envelope, CHAIN)
            .map_err(|e| format!("invalid native envelope: {e}"))?;
        verify_v3_header_pow(&envelope.header, CHAIN)
            .map_err(|e| format!("invalid native PoW: {e}"))?;
        checked_blocks += 1;
        for tx in &envelope.transactions {
            if !seen.insert(tx.txid.as_str()) {
                return Err(format!(
                    "duplicate signed transaction ID across DAG: {}",
                    tx.txid
                ));
            }
        }
    }
    if checked_blocks != envelopes.len() {
        return Err("extraneous candidate envelope outside authenticated order".into());
    }
    Ok(())
}

#[test]
fn duplicate_signed_txid_in_distinct_valid_native_v3_headers_needs_explicit_policy() {
    let (state, funding) = test_state();
    let tx = signed_v2_transfer(&funding.outpoint, "pulse1alice", 90, 10);
    validate_transaction_v2(&tx, &state, CHAIN).unwrap();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![tx.clone()],
        1,
        1,
        HARD_BITS,
        GENESIS_NS + 500_000_000,
    );
    let second = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![tx.clone()],
        1,
        1,
        EASY_BITS,
        GENESIS_NS + 600_000_000,
    );
    assert_ne!(first.hash, second.hash);
    assert_eq!(first.transactions[0].txid, second.transactions[0].txid);
    let mut parents = vec![first.hash.clone(), second.hash.clone()];
    parents.sort();
    let mut headers = BTreeMap::from([
        (genesis.hash.clone(), genesis.header),
        (first.hash.clone(), first.header.clone()),
        (second.hash.clone(), second.header.clone()),
    ]);
    for k in 0..=1_usize {
        let join = mine_full_envelope(
            parents.clone(),
            vec![],
            2 + k as u64,
            2,
            EASY_BITS,
            GENESIS_NS + 1_000_000_000 + k as u64 * 100_000,
        );
        headers.insert(join.hash.clone(), join.header.clone());
        let order = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&join.hash),
            &headers,
            k,
        )
        .unwrap();
        assert_eq!(
            order.selected_spine,
            vec![genesis.hash.clone(), first.hash.clone(), join.hash.clone()]
        );
        assert_eq!(order.side_reds.len(), 1 - k);
        assert_eq!(order.side_blues.len(), k);
        let complete = BTreeMap::from([
            (first.hash.clone(), first.clone()),
            (second.hash.clone(), second.clone()),
            (join.hash.clone(), join.clone()),
        ]);
        let err = diagnostic_unique_native_v3_txids(&order, &genesis.hash, &complete)
            .expect_err("valid envelopes do not imply unique txids across blue/red DAG blocks");
        assert!(
            err.contains("duplicate signed transaction ID across DAG"),
            "{err}"
        );

        let mut missing = complete.clone();
        missing.remove(&second.hash);
        assert!(
            diagnostic_unique_native_v3_txids(&order, &genesis.hash, &missing,).is_err(),
            "a missing authenticated side envelope cannot be skipped"
        );
    }
    let distinct = signed_v2_transfer(&funding.outpoint, "pulse1bob", 92, 8);
    validate_transaction_v2(&distinct, &state, CHAIN).unwrap();
    let second_different = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![distinct],
        1,
        1,
        EASY_BITS,
        GENESIS_NS + 600_000_000,
    );
    let mut different_parents = vec![first.hash.clone(), second_different.hash.clone()];
    different_parents.sort();
    let alternate_join = mine_full_envelope(
        different_parents,
        vec![],
        2,
        2,
        EASY_BITS,
        GENESIS_NS + 1_000_000_000,
    );
    let other_headers = BTreeMap::from([
        (genesis.hash.clone(), headers[&genesis.hash].clone()),
        (first.hash.clone(), first.header.clone()),
        (
            second_different.hash.clone(),
            second_different.header.clone(),
        ),
        (alternate_join.hash.clone(), alternate_join.header.clone()),
    ]);
    let order = preview_unapproved_native_v3_order(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&alternate_join.hash),
        &other_headers,
        0,
    )
    .unwrap();
    let distinct_envelopes = BTreeMap::from([
        (first.hash.clone(), first),
        (second_different.hash.clone(), second_different),
        (alternate_join.hash.clone(), alternate_join),
    ]);
    assert!(
        diagnostic_unique_native_v3_txids(&order, &genesis.hash, &distinct_envelopes,).is_ok(),
        "different txids can still conflict by spending the same UTXO"
    );
}

/// This simulator is not the frozen DAG acceptance policy. It explicitly
/// varies whether an eligible repeated TXID is a whole-order error or a
/// deterministic no-op. Neither variant is yet authorized for mainnet.
#[derive(Clone, Copy)]
enum TrialDuplicatePolicy {
    RejectEntireOrder,
    SkipRepeatedTxid,
}

struct TrialDuplicateOutcome {
    snapshot: PreviewSet,
    accepted_txids: Vec<String>,
    duplicate_txids: Vec<String>,
    conflict_txids: Vec<String>,
    excluded_red_txids: Vec<String>,
    fee_by_containing_block: BTreeMap<String, u64>,
}

fn replay_explicit_duplicate_policy(
    order: &UnapprovedTrialOrder,
    initial: &PreviewSet,
    effects: &BTreeMap<String, (Transaction, HypotheticalBlockEffects)>,
    include_red: bool,
    duplicate_policy: TrialDuplicatePolicy,
) -> Result<TrialDuplicateOutcome, String> {
    let mut seen = BTreeSet::new();
    let mut snapshot = initial.clone();
    let mut accepted_txids = Vec::new();
    let mut duplicate_txids = Vec::new();
    let mut conflict_txids = Vec::new();
    let mut excluded_red_txids = Vec::new();
    let mut fee_by_containing_block = BTreeMap::new();
    let reds: BTreeSet<_> = order.side_reds.iter().cloned().collect();
    for hash in &order.ordered_blocks {
        if let Some((tx, effect)) = effects.get(hash) {
            if effect.block != *hash {
                return Err("cross-block test effect identity mismatch".into());
            }
            if !include_red && reds.contains(hash) {
                excluded_red_txids.push(tx.txid.clone());
                continue;
            }
            if !seen.insert(tx.txid.clone()) {
                match duplicate_policy {
                    TrialDuplicatePolicy::RejectEntireOrder => {
                        return Err("eligible duplicate signed txid across native-v3 DAG".into());
                    }
                    TrialDuplicatePolicy::SkipRepeatedTxid => {
                        duplicate_txids.push(tx.txid.clone());
                        continue;
                    }
                }
            }
            if tx
                .inputs
                .iter()
                .any(|input| !snapshot.contains_key(&input.previous_output))
            {
                if tx
                    .inputs
                    .iter()
                    .any(|input| !initial.contains_key(&input.previous_output))
                {
                    return Err("unknown input, not a legitimate ordered conflict".into());
                }
                conflict_txids.push(tx.txid.clone());
                continue;
            }
            snapshot = detached_utxo::apply(&snapshot, effect)?;
            if fee_by_containing_block
                .insert(hash.clone(), tx.fee)
                .is_some()
            {
                return Err("duplicate fee entitlement at same containing block".into());
            }
            accepted_txids.push(tx.txid.clone());
        }
    }
    Ok(TrialDuplicateOutcome {
        snapshot,
        accepted_txids,
        duplicate_txids,
        conflict_txids,
        excluded_red_txids,
        fee_by_containing_block,
    })
}

#[test]
fn duplicate_policy_matrix_never_pays_a_signed_fee_twice() {
    // Both genuine PoW-valid v3 blocks contain exactly the same Ed25519 v2
    // transaction, spending one test-only preloaded UTXO of 100 atoms.
    // Beneficiary identities and claim maturity are NOT inferred here.
    let (state, funding) = test_state();
    let signed = signed_v2_transfer(&funding.outpoint, "pulse1alice", 90, 10);
    validate_transaction_v2(&signed, &state, CHAIN).unwrap();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![signed.clone()],
        1,
        1,
        HARD_BITS,
        GENESIS_NS + 500_000_000,
    );
    let second = mine_full_envelope(
        vec![genesis.hash.clone()],
        vec![signed.clone()],
        1,
        1,
        EASY_BITS,
        GENESIS_NS + 600_000_000,
    );
    let mut parents = vec![first.hash.clone(), second.hash.clone()];
    parents.sort();
    let mut headers = BTreeMap::from([
        (genesis.hash.clone(), genesis.header),
        (first.hash.clone(), first.header.clone()),
        (second.hash.clone(), second.header.clone()),
    ]);
    let effects = BTreeMap::from([
        (
            first.hash.clone(),
            (
                signed.clone(),
                signed_v2_effect(&first.hash, &signed, &state).unwrap(),
            ),
        ),
        (
            second.hash.clone(),
            (
                signed.clone(),
                signed_v2_effect(&second.hash, &signed, &state).unwrap(),
            ),
        ),
    ]);
    let starting = PreviewSet::from([(funding.outpoint.clone(), funding.clone())]);
    for k in [0_usize, 1] {
        let join = mine_full_envelope(
            parents.clone(),
            vec![],
            2 + k as u64,
            2,
            EASY_BITS,
            GENESIS_NS + 1_000_000_000 + k as u64 * 100_000,
        );
        headers.insert(join.hash.clone(), join.header.clone());
        let order = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &genesis.hash,
            std::slice::from_ref(&join.hash),
            &headers,
            k,
        )
        .unwrap();
        assert_eq!(order.ordered_blocks.len(), 4);
        assert_eq!(order.side_reds.len(), 1 - k);
        assert_eq!(
            order.selected_spine,
            vec![genesis.hash.clone(), first.hash.clone(), join.hash.clone()]
        );
        let strict = replay_explicit_duplicate_policy(
            &order,
            &starting,
            &effects,
            true,
            TrialDuplicatePolicy::RejectEntireOrder,
        );
        assert!(
            strict.is_err(),
            "including two copies under strict policy must reject"
        );
        let skipped = replay_explicit_duplicate_policy(
            &order,
            &starting,
            &effects,
            true,
            TrialDuplicatePolicy::SkipRepeatedTxid,
        )
        .unwrap();
        assert_eq!(skipped.accepted_txids, vec![signed.txid.clone()]);
        assert_eq!(skipped.duplicate_txids, vec![signed.txid.clone()]);
        assert!(skipped.conflict_txids.is_empty());
        assert_eq!(skipped.fee_by_containing_block.len(), 1);
        assert_eq!(skipped.fee_by_containing_block[&first.hash], 10);
        assert!(!skipped.fee_by_containing_block.contains_key(&second.hash));
        assert_eq!(detached_utxo::total_supply(&skipped.snapshot).unwrap(), 90);
        assert_eq!(
            detached_utxo::fingerprint(&starting),
            detached_utxo::fingerprint(
                &detached_utxo::undo(&skipped.snapshot, &effects[&first.hash].1).unwrap()
            )
        );

        let blue_only = replay_explicit_duplicate_policy(
            &order,
            &starting,
            &effects,
            false,
            TrialDuplicatePolicy::SkipRepeatedTxid,
        )
        .unwrap();
        assert_eq!(
            detached_utxo::fingerprint(&blue_only.snapshot),
            detached_utxo::fingerprint(&skipped.snapshot)
        );
        assert_eq!(blue_only.fee_by_containing_block[&first.hash], 10);
        assert_eq!(blue_only.duplicate_txids.len(), k);
        assert_eq!(blue_only.excluded_red_txids.len(), 1 - k);
        if k == 0 {
            assert!(
                replay_explicit_duplicate_policy(
                    &order,
                    &starting,
                    &effects,
                    false,
                    TrialDuplicatePolicy::RejectEntireOrder,
                )
                .is_ok(),
                "red exclusion must happen before duplicate-policy evaluation"
            );
        } else {
            assert!(replay_explicit_duplicate_policy(
                &order,
                &starting,
                &effects,
                false,
                TrialDuplicatePolicy::RejectEntireOrder,
            )
            .is_err());
        }

        // Distinct signed txids consuming the same funding are NOT duplicates.
        // They are handled by the independent atomic UTXO-conflict gate.
        let conflicting = signed_v2_transfer(&funding.outpoint, "pulse1bob", 92, 8);
        validate_transaction_v2(&conflicting, &state, CHAIN).unwrap();
        let mixed = BTreeMap::from([
            (first.hash.clone(), effects[&first.hash].clone()),
            (
                second.hash.clone(),
                (
                    conflicting.clone(),
                    signed_v2_effect(&second.hash, &conflicting, &state).unwrap(),
                ),
            ),
        ]);
        let conflict = replay_explicit_duplicate_policy(
            &order,
            &starting,
            &mixed,
            true,
            TrialDuplicatePolicy::RejectEntireOrder,
        )
        .expect("different txids with same source must be an atomic spend conflict");
        assert!(conflict.duplicate_txids.is_empty());
        assert_eq!(conflict.conflict_txids, vec![conflicting.txid.clone()]);
        assert_eq!(conflict.fee_by_containing_block[&first.hash], 10);
        assert_eq!(conflict.fee_by_containing_block.len(), 1);
        assert_eq!(detached_utxo::total_supply(&conflict.snapshot).unwrap(), 90);
        assert_eq!(
            detached_utxo::fingerprint(&starting),
            detached_utxo::fingerprint(&PreviewSet::from([(
                funding.outpoint.clone(),
                funding.clone()
            )]))
        );
    }
}
