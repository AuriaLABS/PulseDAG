//! Test-only native v3 PoW reward claim ↔ frozen monetary curve ↔ UTXO root.
//!
//! An amountless reward claim is NOT a spendable UTXO. Only a separately
//! confirmed canonical finality boundary PLUS economic maturity would allow
//! real reward settlement. We do not claim to prove that boundary here.
//! The candidate DAG, mining, storage, P2P and contracts remain untouched.

use std::collections::BTreeMap;

use pulsedag_core::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    monetary_v3::{
        economic_maturity_reached, max_coinbase_claim_atoms, subsidy_atoms_for_score,
        total_supply_atoms_for_score, COINBASE_MATURITY_NS, CONSENSUS_BURN_BPS,
        GENESIS_ISSUANCE_ATOMS, MAX_SUPPLY_ATOMS, ORDINARY_FEE_RECIPIENT_BPS,
        PRODUCTION_CADENCE_TARGET_INTERVAL_NS_V3, PRODUCTION_CADENCE_V3,
    },
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::{
        build_reward_claim_transaction_v3, settlement_outpoint_v3,
        validate_reward_claim_transaction_v3,
    },
    state::UtxoState,
    state_replay_v3::mature_reward_prefix_score_v3,
    types::{compute_merkle_root, Transaction, Utxo},
};

#[path = "support/native_v3_utxo_preview.rs"]
#[allow(dead_code)]
mod detached_utxo;

use detached_utxo::{apply, fingerprint, total_supply, undo, HypotheticalBlockEffects, PreviewSet};

const CHAIN: &str = "pulsedag-dormant-reward-claim-native-v3-test";
const GENESIS_NS: u64 = 1_800_000_009_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn mined_claim(parent: &str, beneficiary: &str, marker: u64) -> (BlockEnvelopeV3, Transaction) {
    let claim = build_reward_claim_transaction_v3(beneficiary, marker, CHAIN).unwrap();
    assert_eq!(claim.outputs[0].amount, 0, "claim must never directly mint");
    assert!(claim.inputs.is_empty());
    validate_reward_claim_transaction_v3(&claim, CHAIN).unwrap();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.to_owned()],
        timestamp_ns: GENESIS_NS + 500_000_000 + marker * 100_000,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(std::slice::from_ref(&claim)),
        state_root: UtxoState::default().compute_state_root().unwrap(),
        blue_score: 1,
        height: 1,
    };
    for nonce in 0..1_024u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            verify_v3_header_pow(&header, CHAIN).unwrap();
            let envelope = build_block_envelope_v3(CHAIN, header, vec![claim.clone()])
                .expect("correct v2-wire amountless reward claim");
            return (envelope, claim);
        }
    }
    panic!("easy PoW test-only nonce budget exhausted");
}

fn policy_reward_effect(
    envelope: &BlockEnvelopeV3,
    claim: &Transaction,
    score: u64,
) -> HypotheticalBlockEffects {
    assert!(score > 0);
    validate_reward_claim_transaction_v3(claim, CHAIN).unwrap();
    assert_eq!(envelope.transactions[0].txid, claim.txid);
    let subsidy = subsidy_atoms_for_score(score, &PRODUCTION_CADENCE_V3).unwrap();
    assert!(subsidy > 0, "test score must produce positive subsidy");
    let reward = Utxo {
        outpoint: settlement_outpoint_v3(CHAIN, &envelope.hash, &claim.txid),
        address: claim.outputs[0].address.clone(),
        amount: subsidy,
        coinbase: true,
        height: envelope.header.height,
    };
    assert_ne!(reward.outpoint.txid, claim.txid);
    HypotheticalBlockEffects {
        block: envelope.hash.clone(),
        spent: vec![],
        created: vec![reward],
        // This amount is obtained from the *real frozen monetary v3 policy*,
        // not a user-supplied hypothetical issuance number.
        test_issuance: subsidy,
        test_fee_burn: 0,
    }
}

fn root_of(snapshot: &PreviewSet) -> String {
    let mut utxo = UtxoState::default();
    for coin in snapshot.values() {
        assert!(utxo.utxos.insert(coin.outpoint.clone(), coin.clone()).is_none());
        utxo.address_index
            .entry(coin.address.clone())
            .or_default()
            .push(coin.outpoint.clone());
    }
    utxo.validate_deterministic_indexes().unwrap();
    utxo.compute_state_root().unwrap()
}

#[test]
fn native_v3_amountless_claim_obeys_actual_subsidy_maturity_and_finality_separation() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let (envelope, claim) = mined_claim(&genesis.hash, "pulse1miner-a", 1);
    assert_eq!(envelope.transactions[0].outputs[0].amount, 0);
    assert_eq!(envelope.header.state_root, root_of(&PreviewSet::new()));
    assert_eq!(GENESIS_ISSUANCE_ATOMS, 0);
    assert_eq!(total_supply_atoms_for_score(0, &PRODUCTION_CADENCE_V3).unwrap(), 0);
    let subsidy = subsidy_atoms_for_score(1, &PRODUCTION_CADENCE_V3).unwrap();
    assert_eq!(subsidy, total_supply_atoms_for_score(1, &PRODUCTION_CADENCE_V3).unwrap());
    assert!(subsidy > 0 && subsidy < MAX_SUPPLY_ATOMS);
    assert_eq!(PRODUCTION_CADENCE_TARGET_INTERVAL_NS_V3, 500_000_000);
    assert_eq!(COINBASE_MATURITY_NS, 3_600_000_000_000);
    assert_eq!(CONSENSUS_BURN_BPS, 0);
    assert_eq!(ORDINARY_FEE_RECIPIENT_BPS, 10_000);
    assert!(!economic_maturity_reached(1, 7_200, &PRODUCTION_CADENCE_V3).unwrap());
    assert!(economic_maturity_reached(1, 7_201, &PRODUCTION_CADENCE_V3).unwrap());
    assert_eq!(mature_reward_prefix_score_v3(7_200, &PRODUCTION_CADENCE_V3).unwrap(), 0);
    assert_eq!(mature_reward_prefix_score_v3(7_201, &PRODUCTION_CADENCE_V3).unwrap(), 1);
    assert!(!economic_maturity_reached(7_201, 7_200, &PRODUCTION_CADENCE_V3).unwrap());
    assert_eq!(max_coinbase_claim_atoms(1, 13, &PRODUCTION_CADENCE_V3).unwrap(), subsidy + 13);

    // Neither the zero-value wire claim nor economic maturity by itself
    // authorizes materializing the reward. We NEVER call policy_reward_effect
    // inside a production acceptance, because the independent FINALITY proof
    // is NOT provided by this isolated test.
    let mut altered = claim.clone();
    altered.outputs[0].amount = subsidy;
    assert!(validate_reward_claim_transaction_v3(&altered, CHAIN).is_err());
    let mut bad_network = claim.clone();
    bad_network.txid = pulsedag_core::compute_txid_v2(&claim, "other-network").unwrap();
    assert!(validate_reward_claim_transaction_v3(&bad_network, CHAIN).is_err());
    assert!(build_reward_claim_transaction_v3("pulse1miner-a", 1, "").is_err());
}

#[test]
fn detached_real_policy_reward_outpoints_and_utxo_roots_are_reversible_if_finality_is_supplied() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let (a, claim_a) = mined_claim(&genesis.hash, "pulse1miner-a", 1);
    let (b, claim_b) = mined_claim(&genesis.hash, "pulse1miner-b", 2);
    let policy_effects = BTreeMap::from([
        (a.hash.clone(), policy_reward_effect(&a, &claim_a, 1)),
        (b.hash.clone(), policy_reward_effect(&b, &claim_b, 1)),
    ]);
    assert_ne!(a.hash, b.hash);
    assert_ne!(
        policy_effects[&a.hash].created[0].outpoint,
        policy_effects[&b.hash].created[0].outpoint
    );
    let initial = PreviewSet::new();
    let initial_root = root_of(&initial);
    // Hypothetically *after* both independent finality+maturity gates:
    // bookkeeping of an authorized settlement value, without making a
    // maturity/finality decision or altering the real ChainState.
    let after_a = apply(&initial, &policy_effects[&a.hash]).unwrap();
    let after_b = apply(&initial, &policy_effects[&b.hash]).unwrap();
    let subsidy = subsidy_atoms_for_score(1, &PRODUCTION_CADENCE_V3).unwrap();
    assert_eq!(total_supply(&after_a).unwrap(), u128::from(subsidy));
    assert_eq!(total_supply(&after_b).unwrap(), u128::from(subsidy));
    assert_ne!(root_of(&after_a), initial_root);
    assert_ne!(root_of(&after_a), root_of(&after_b));
    let back_to_zero = undo(&after_a, &policy_effects[&a.hash]).unwrap();
    assert_eq!(root_of(&back_to_zero), initial_root);
    let alternate = apply(&back_to_zero, &policy_effects[&b.hash]).unwrap();
    assert_eq!(root_of(&alternate), root_of(&after_b));
    assert_eq!(fingerprint(&alternate), fingerprint(&after_b));
    assert_eq!(root_of(&undo(&after_b, &policy_effects[&b.hash]).unwrap()), initial_root);

    let mut forged = policy_effects[&a.hash].clone();
    forged.created[0].amount += 1;
    assert!(apply(&initial, &forged).is_err(), "unauthorized subsidy mismatch");
    assert_eq!(root_of(&initial), initial_root);
    assert!(apply(&after_a, &policy_effects[&a.hash]).is_err(), "duplicate reward outpoint");
    assert!(undo(&after_b, &policy_effects[&a.hash]).is_err(), "wrong fork undo");
}
