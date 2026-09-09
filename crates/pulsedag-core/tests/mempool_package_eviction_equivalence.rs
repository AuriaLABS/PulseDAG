use std::collections::{BTreeSet, HashMap, HashSet};

use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    accept::{accept_transaction, AcceptSource},
    genesis::init_chain_state,
    mempool::canonical_mempool_txids,
    tx::{address_from_public_key, compute_txid, signing_message},
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
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
    index: u32,
    address: String,
    amount: u64,
) -> OutPoint {
    let outpoint = OutPoint {
        txid: txid.to_string(),
        index,
    };
    let utxo = Utxo {
        outpoint: outpoint.clone(),
        address: address.clone(),
        amount,
        coinbase: false,
        height: 1,
    };
    state.utxo.utxos.insert(outpoint.clone(), utxo);
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

fn reverse_internal_insertion_order(state: &mut pulsedag_core::ChainState) {
    let mut transactions = std::mem::take(&mut state.mempool.transactions)
        .into_iter()
        .collect::<Vec<_>>();
    transactions.sort_by(|a, b| b.0.cmp(&a.0));
    state.mempool.transactions = transactions.into_iter().collect::<HashMap<_, _>>();

    let mut first_seen = std::mem::take(&mut state.mempool.first_seen)
        .into_iter()
        .collect::<Vec<_>>();
    first_seen.sort_by(|a, b| b.0.cmp(&a.0));
    state.mempool.first_seen = first_seen.into_iter().collect::<HashMap<_, _>>();

    let mut admission_height = std::mem::take(&mut state.mempool.admission_height)
        .into_iter()
        .collect::<Vec<_>>();
    admission_height.sort_by(|a, b| b.0.cmp(&a.0));
    state.mempool.admission_height = admission_height.into_iter().collect::<HashMap<_, _>>();

    let mut spent = std::mem::take(&mut state.mempool.spent_outpoints)
        .into_iter()
        .collect::<Vec<_>>();
    spent.reverse();
    state.mempool.spent_outpoints = spent.into_iter().collect::<HashSet<_>>();
}

#[test]
fn equivalent_hashmap_orders_select_identical_package_eviction_and_metadata() {
    let mut baseline = init_chain_state("mempool-package-eviction-equivalence".to_string());
    baseline.mempool.max_transactions = 2;

    let parent_key = signing_key(172);
    let child_key = signing_key(173);
    let outsider_key = signing_key(174);
    let parent_input = fund_address(
        &mut baseline,
        "fund-equivalent-package-parent",
        0,
        address_from_public_key(&public_key_hex(&parent_key)),
        100,
    );
    let outsider_input = fund_address(
        &mut baseline,
        "fund-equivalent-package-outsider",
        0,
        address_from_public_key(&public_key_hex(&outsider_key)),
        100,
    );

    let parent = signed_tx(
        &parent_key,
        vec![parent_input],
        vec![TxOutput {
            address: address_from_public_key(&public_key_hex(&child_key)),
            amount: 98,
        }],
        2,
        1,
    );
    let child = signed_tx(
        &child_key,
        vec![OutPoint {
            txid: parent.txid.clone(),
            index: 0,
        }],
        vec![TxOutput {
            address: "pulse1equivalent-package-child".to_string(),
            amount: 83,
        }],
        15,
        2,
    );
    let outsider = signed_tx(
        &outsider_key,
        vec![outsider_input],
        vec![TxOutput {
            address: "pulse1equivalent-package-outsider".to_string(),
            amount: 90,
        }],
        10,
        3,
    );

    accept_transaction(parent.clone(), &mut baseline, AcceptSource::Rpc).unwrap();
    accept_transaction(child.clone(), &mut baseline, AcceptSource::Rpc).unwrap();

    let mut reordered = baseline.clone();
    reverse_internal_insertion_order(&mut reordered);

    accept_transaction(outsider.clone(), &mut baseline, AcceptSource::Rpc)
        .expect("baseline state should admit outsider by evicting the dependency package");
    accept_transaction(outsider.clone(), &mut reordered, AcceptSource::Rpc)
        .expect("reordered equivalent state must make the same eviction decision");

    let baseline_txids = baseline
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let reordered_txids = reordered
        .mempool
        .transactions
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(baseline_txids, reordered_txids);
    assert_eq!(baseline_txids, BTreeSet::from([outsider.txid.clone()]));
    assert!(!baseline_txids.contains(&parent.txid));
    assert!(!baseline_txids.contains(&child.txid));

    assert_eq!(
        canonical_mempool_txids(&baseline),
        canonical_mempool_txids(&reordered)
    );
    assert_eq!(
        baseline.mempool.spent_outpoints,
        reordered.mempool.spent_outpoints
    );
    assert_eq!(baseline.mempool.first_seen, reordered.mempool.first_seen);
    assert_eq!(
        baseline.mempool.admission_height,
        reordered.mempool.admission_height
    );
    assert_eq!(
        baseline.mempool.next_first_seen,
        reordered.mempool.next_first_seen
    );
    assert_eq!(baseline.mempool.counters.evicted_total, 2);
    assert_eq!(
        baseline.mempool.counters.evicted_total,
        reordered.mempool.counters.evicted_total
    );

    let live_keys = baseline_txids;
    assert_eq!(
        baseline
            .mempool
            .first_seen
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        live_keys
    );
    assert_eq!(
        baseline
            .mempool
            .admission_height
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        live_keys
    );
}
