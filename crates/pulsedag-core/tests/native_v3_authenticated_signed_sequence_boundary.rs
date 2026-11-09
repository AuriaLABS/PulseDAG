//! Dormant native-v3 test: independently authenticated KHeavyHash ancestry
//! plus caller-specified signed-v2 UTXO order. NO canonical monetary ordering.
//!
//! Never feed diagnostic height/hash topological order into UTXO replay.
//! Prestate is synthetic and independently trusted; claims remain amountless.
//! This is not reward finality, committed monetary state or network admission.

use std::collections::BTreeMap;

use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    address_from_public_key,
    apply::apply_transaction,
    block_envelope_v3::{
        build_block_envelope_v3, validate_dormant_native_v3_monetary_body_preflight,
        BlockEnvelopeV3,
    },
    compute_txid_v2,
    genesis_v3::{dormant_native_genesis_envelope_v3_ns, init_chain_state_v3},
    header_v3::BlockHeaderV3,
    native_topological_evaluation_v3::derive_dormant_native_v3_topological_evaluation,
    native_v3_signed_prestate_trial::inspect_dormant_native_v3_explicit_order_trial,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    reward_settlement_v3::build_reward_claim_transaction_v3,
    signing_message_v2,
    state::ChainState,
    tx::TRANSACTION_VERSION_V2,
    types::{compute_merkle_root, OutPoint, Transaction, TxInput, TxOutput, Utxo},
};

const CHAIN: &str = "native-v3-dormant-authenticated-signed-sequence";
const GENESIS_NS: u64 = 1_800_000_026_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn key() -> SigningKey {
    SigningKey::from_bytes(&[47_u8; 32])
}

fn address() -> String {
    address_from_public_key(&hex::encode(key().verifying_key().to_bytes()))
}

/// Two *test-injected* UTXOs, not spendable outputs of v3 zero-issuance genesis.
fn trusted_test_state() -> (ChainState, [OutPoint; 2]) {
    let mut state = init_chain_state_v3(CHAIN.into(), GENESIS_NS / 1_000_000_000).unwrap();
    assert!(!state.contracts.config.enabled);
    let funding = [
        OutPoint {
            txid: "synthetic-funded-input".into(),
            index: 0,
        },
        OutPoint {
            txid: "synthetic-funded-input".into(),
            index: 1,
        },
    ];
    for outpoint in &funding {
        state.utxo.utxos.insert(
            outpoint.clone(),
            Utxo {
                outpoint: outpoint.clone(),
                address: address(),
                amount: 100,
                coinbase: false,
                height: 1,
            },
        );
        state
            .utxo
            .address_index
            .entry(address())
            .or_default()
            .push(outpoint.clone());
    }
    (state, funding)
}

fn signed(source: OutPoint, amount: u64, fee: u64, nonce: u64) -> Transaction {
    let signing_key = key();
    let mut tx = Transaction {
        txid: String::new(),
        version: TRANSACTION_VERSION_V2,
        inputs: vec![TxInput {
            previous_output: source,
            public_key: hex::encode(signing_key.verifying_key().to_bytes()),
            signature: String::new(),
        }],
        outputs: vec![TxOutput {
            address: address(),
            amount,
        }],
        fee,
        nonce,
    };
    tx.inputs[0].signature = hex::encode(
        signing_key
            .sign(&signing_message_v2(&tx, CHAIN).unwrap())
            .to_bytes(),
    );
    tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
    tx
}

fn mine(parent: &str, height: u64, marker: u64, tx: Transaction) -> BlockEnvelopeV3 {
    let transactions = vec![
        build_reward_claim_transaction_v3("pulse1synthetic-miner", marker, CHAIN).unwrap(),
        tx,
    ];
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.into()],
        timestamp_ns: GENESIS_NS + height * 500_000_000 + marker * 100_000,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: compute_merkle_root(&transactions),
        state_root: "33".repeat(32), // Deliberately unauthenticated monetary root.
        blue_score: height,
        height,
    };
    for nonce in 0..4_096u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            verify_v3_header_pow(&header, CHAIN).unwrap();
            let envelope = build_block_envelope_v3(CHAIN, header, transactions).unwrap();
            validate_dormant_native_v3_monetary_body_preflight(&envelope, CHAIN).unwrap();
            return envelope;
        }
    }
    panic!("test-only easy-target nonce budget exceeded");
}

fn authenticated_headers(
    genesis: &BlockEnvelopeV3,
    blocks: &[BlockEnvelopeV3],
) -> BTreeMap<String, BlockHeaderV3> {
    let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header.clone())]);
    for block in blocks {
        assert!(headers
            .insert(block.hash.clone(), block.header.clone())
            .is_none());
    }
    headers
}

#[test]
fn true_pow_parent_chain_and_two_linked_ed25519_spends_conserve_fees_and_utxos() {
    let (state, sources) = trusted_test_state();
    let initial_root = state.utxo.compute_state_root().unwrap();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = signed(sources[0].clone(), 90, 10, 11);
    let second = signed(
        OutPoint {
            txid: first.txid.clone(),
            index: 0,
        },
        80,
        10,
        12,
    );
    let a = mine(&genesis.hash, 1, 11, first.clone());
    let b = mine(&a.hash, 2, 12, second.clone());
    let headers = authenticated_headers(&genesis, &[a.clone(), b.clone()]);
    let authenticated = derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&b.hash),
        &headers,
    )
    .unwrap();
    assert_eq!(
        authenticated.ordered_reachable_headers,
        vec![genesis.hash.clone(), a.hash.clone(), b.hash.clone()]
    );
    assert_eq!(
        authenticated.verified_dag.authenticated_non_genesis_headers,
        2
    );

    // This explicit list is supplied by the test: NOT derived as economic
    // authority from the height/hash-only topological diagnostic above.
    let result =
        inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), b.clone()], &state).unwrap();
    assert_eq!(result.signed_transactions, 2);
    assert_eq!(result.candidate_fees, 20);
    assert_eq!(result.fees_by_block.get(&a.hash), Some(&10));
    assert_eq!(result.fees_by_block.get(&b.hash), Some(&10));
    let mut independently_applied = state.clone();
    apply_transaction(&first, &mut independently_applied, 1).unwrap();
    apply_transaction(&second, &mut independently_applied, 2).unwrap();
    assert_eq!(
        result.resulting_utxo_root,
        independently_applied.utxo.compute_state_root().unwrap()
    );
    assert_ne!(a.header.state_root, result.resulting_utxo_root);
    assert_ne!(b.header.state_root, result.resulting_utxo_root);
    assert_eq!(state.utxo.compute_state_root().unwrap(), initial_root);
    assert!(inspect_dormant_native_v3_explicit_order_trial(&[b, a], &state).is_err());
}

#[test]
fn two_independent_pow_siblings_have_equal_conditional_roots_in_both_explicit_orders() {
    let (state, sources) = trusted_test_state();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a = mine(&genesis.hash, 1, 21, signed(sources[0].clone(), 90, 10, 21));
    let b = mine(&genesis.hash, 1, 22, signed(sources[1].clone(), 95, 5, 22));
    assert_ne!(a.hash, b.hash);
    let headers = authenticated_headers(&genesis, &[a.clone(), b.clone()]);
    let mut tips = vec![a.hash.clone(), b.hash.clone()];
    tips.sort();
    let topo = derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        &tips,
        &headers,
    )
    .unwrap();
    assert_eq!(topo.ordered_reachable_headers.len(), 3);
    assert_eq!(topo.verified_dag.authenticated_non_genesis_headers, 2);
    // This topo walk is intentionally NEVER consumed by the monetary trial:
    // independent nonconflicting spends happen to commute, not because there
    // is a frozen network consensus on a canonical blue/red monetary order.
    let ab =
        inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), b.clone()], &state).unwrap();
    let ba = inspect_dormant_native_v3_explicit_order_trial(&[b, a], &state).unwrap();
    assert_eq!(ab.candidate_fees, 15);
    assert_eq!(ab.signed_transactions, 2);
    assert_eq!(ab.fees_by_block, ba.fees_by_block);
    assert_eq!(ab.resulting_utxo_root, ba.resulting_utxo_root);
    assert_eq!(ab.candidate_fees, ba.candidate_fees);
    assert!(state.utxo.utxos.contains_key(&sources[0]));
    assert!(state.utxo.utxos.contains_key(&sources[1]));
}

#[test]
fn signed_state_preview_does_not_authenticate_missing_or_wrong_network_ancestry() {
    let (state, sources) = trusted_test_state();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a = mine(&genesis.hash, 1, 31, signed(sources[0].clone(), 90, 10, 31));
    let b = mine(&a.hash, 2, 32, signed(sources[1].clone(), 95, 5, 32));
    let original_root = state.utxo.compute_state_root().unwrap();

    // The old explicit-order fee/UTXO diagnostic does not authenticate PoW,
    // the DAG or the pinned genesis: it is only a disposable state hypothesis.
    assert!(
        inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), b.clone()], &state).is_ok()
    );
    let mut missing = authenticated_headers(&genesis, &[a.clone(), b.clone()]);
    missing.remove(&a.hash);
    assert!(derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&b.hash),
        &missing,
    )
    .is_err());
    let complete = authenticated_headers(&genesis, &[a, b.clone()]);
    assert!(derive_dormant_native_v3_topological_evaluation(
        "wrong-network",
        GENESIS_NS,
        &genesis.hash,
        std::slice::from_ref(&b.hash),
        &complete,
    )
    .is_err());
    assert_eq!(state.utxo.compute_state_root().unwrap(), original_root);
}

#[test]
fn conflicting_pow_siblings_cannot_be_settled_by_arbitrary_explicit_order() {
    // Both siblings carry independently valid Ed25519 transactions spending
    // ONE synthetic input. Authenticated PoW/ancestry cannot authorize which
    // conflicting transaction receives fees without frozen GD-08 ordering.
    let (state, funding) = trusted_test_state();
    let original_root = state.utxo.compute_state_root().unwrap();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a = mine(&genesis.hash, 1, 41, signed(funding[0].clone(), 90, 10, 41));
    let b = mine(&genesis.hash, 1, 42, signed(funding[0].clone(), 85, 15, 42));
    assert_ne!(a.hash, b.hash);
    assert_ne!(a.transactions[1].txid, b.transactions[1].txid);
    verify_v3_header_pow(&a.header, CHAIN).unwrap();
    verify_v3_header_pow(&b.header, CHAIN).unwrap();

    let headers = authenticated_headers(&genesis, &[a.clone(), b.clone()]);
    let mut tips = vec![a.hash.clone(), b.hash.clone()];
    tips.sort();
    let ancestry = derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        &tips,
        &headers,
    )
    .unwrap();
    assert_eq!(ancestry.verified_dag.authenticated_non_genesis_headers, 2);
    assert_eq!(ancestry.ordered_reachable_headers.len(), 3);

    // Each alternative is a valid conditional signed-spend hypothesis.
    let only_a =
        inspect_dormant_native_v3_explicit_order_trial(std::slice::from_ref(&a), &state).unwrap();
    let only_b =
        inspect_dormant_native_v3_explicit_order_trial(std::slice::from_ref(&b), &state).unwrap();
    assert_eq!(only_a.candidate_fees, 10);
    assert_eq!(only_b.candidate_fees, 15);
    assert_eq!(only_a.signed_transactions, 1);
    assert_eq!(only_b.signed_transactions, 1);
    assert_ne!(only_a.resulting_utxo_root, only_b.resulting_utxo_root);

    // Neither peer arrival order may pay BOTH fees or accept both conflicts.
    // Do not reinterpret these rejects as an approved GHOSTDAG blue/red rule.
    for order in [[a.clone(), b.clone()], [b, a]] {
        assert!(inspect_dormant_native_v3_explicit_order_trial(&order, &state).is_err());
    }
    assert_eq!(state.utxo.compute_state_root().unwrap(), original_root);
}

// GD-08 research only: do not pay a fee from an unresolved conflicting branch
// merely because an unrelated valid sibling is also present. Caller supplies
// conditional orders; none is promoted to the canonical monetary order.
#[test]
fn three_pow_siblings_two_conflicts_and_one_independent_spend_never_count_both_fees() {
    let (state, sources) = trusted_test_state();
    let original_root = state.utxo.compute_state_root().unwrap();
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let a = mine(&genesis.hash, 1, 51, signed(sources[0].clone(), 90, 10, 51));
    let b = mine(&genesis.hash, 1, 52, signed(sources[0].clone(), 85, 15, 52));
    let c = mine(&genesis.hash, 1, 53, signed(sources[1].clone(), 93, 7, 53));
    for block in [&a, &b, &c] {
        verify_v3_header_pow(&block.header, CHAIN).unwrap();
    }
    let headers = authenticated_headers(&genesis, &[a.clone(), b.clone(), c.clone()]);
    // The native diagnostic requires a canonically sorted tip set, independent
    // of the caller-supplied conditional transaction replay orders below.
    let mut tips = vec![a.hash.clone(), b.hash.clone(), c.hash.clone()];
    tips.sort();
    let ancestry = derive_dormant_native_v3_topological_evaluation(
        CHAIN,
        GENESIS_NS,
        &genesis.hash,
        &tips,
        &headers,
    )
    .unwrap();
    assert_eq!(ancestry.verified_dag.authenticated_non_genesis_headers, 3);
    assert_eq!(ancestry.ordered_reachable_headers.len(), 4);

    // Conditional, conflict-free orders commute and credit ONLY their own fees.
    // No global winner or selected-parent/blue-work rule is implied.
    let ac =
        inspect_dormant_native_v3_explicit_order_trial(&[a.clone(), c.clone()], &state).unwrap();
    let ca =
        inspect_dormant_native_v3_explicit_order_trial(&[c.clone(), a.clone()], &state).unwrap();
    let bc =
        inspect_dormant_native_v3_explicit_order_trial(&[b.clone(), c.clone()], &state).unwrap();
    let cb =
        inspect_dormant_native_v3_explicit_order_trial(&[c.clone(), b.clone()], &state).unwrap();
    assert_eq!(ac.candidate_fees, 17);
    assert_eq!(bc.candidate_fees, 22);
    assert_eq!(ac.candidate_fees, ca.candidate_fees);
    assert_eq!(bc.candidate_fees, cb.candidate_fees);
    assert_eq!(ac.resulting_utxo_root, ca.resulting_utxo_root);
    assert_eq!(bc.resulting_utxo_root, cb.resulting_utxo_root);
    assert_ne!(ac.resulting_utxo_root, bc.resulting_utxo_root);
    assert_eq!(ac.fees_by_block.get(&a.hash), Some(&10));
    assert_eq!(ac.fees_by_block.get(&c.hash), Some(&7));
    assert!(!ac.fees_by_block.contains_key(&b.hash));
    assert_eq!(bc.fees_by_block.get(&b.hash), Some(&15));
    assert_eq!(bc.fees_by_block.get(&c.hash), Some(&7));
    assert!(!bc.fees_by_block.contains_key(&a.hash));

    // Independent state replay from the identical trusted synthetic prestate.
    let mut expected_a = state.clone();
    apply_transaction(&a.transactions[1], &mut expected_a, 1).unwrap();
    apply_transaction(&c.transactions[1], &mut expected_a, 1).unwrap();
    assert_eq!(
        ac.resulting_utxo_root,
        expected_a.utxo.compute_state_root().unwrap()
    );
    let mut expected_b = state.clone();
    apply_transaction(&b.transactions[1], &mut expected_b, 1).unwrap();
    apply_transaction(&c.transactions[1], &mut expected_b, 1).unwrap();
    assert_eq!(
        bc.resulting_utxo_root,
        expected_b.utxo.compute_state_root().unwrap()
    );

    // All 3! peer arrival permutations contain a valid double spend and must
    // fail closed without paying BOTH 10+15 conflicting fees, or mutating state.
    for order in [
        [a.clone(), b.clone(), c.clone()],
        [a.clone(), c.clone(), b.clone()],
        [b.clone(), a.clone(), c.clone()],
        [b.clone(), c.clone(), a.clone()],
        [c.clone(), a.clone(), b.clone()],
        [c.clone(), b.clone(), a.clone()],
    ] {
        assert!(inspect_dormant_native_v3_explicit_order_trial(&order, &state).is_err());
    }
    assert_eq!(state.utxo.compute_state_root().unwrap(), original_root);
}
