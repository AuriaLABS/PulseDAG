use ed25519_dalek::{Signer, SigningKey};
use pulsedag_core::{
    genesis::init_chain_state,
    mempool::canonical_mempool_txids,
    mempool_admission_v3::classify_mempool_conflicts_v3,
    tx::{address_from_public_key, compute_txid, signing_message},
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    AcceptSource, ChainState,
};
use pulsedag_storage::Storage;

fn temp_db_path(name: &str) -> String {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir()
        .join(format!("pulsedag-conflict-vector-{name}-{unique}"))
        .to_string_lossy()
        .into_owned()
}

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn public_key_hex(signing_key: &SigningKey) -> String {
    hex::encode(signing_key.verifying_key().to_bytes())
}

fn fund_address(state: &mut ChainState, txid: &str, address: String, amount: u64) -> OutPoint {
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

fn persist_snapshot(path: &str, state: &ChainState) {
    let storage = Storage::open(path).unwrap();
    for block in state.dag.blocks.values() {
        storage.persist_block(block).unwrap();
    }
    storage.persist_chain_state(state).unwrap();
}

#[test]
fn conflict_package_golden_vectors_survive_real_restart_and_hashmap_reordering() {
    let path_a = temp_db_path("a");
    let path_b = temp_db_path("b");
    let mut state = init_chain_state("mempool-conflict-golden-restart".to_string());

    let owner_key = signing_key(81);
    let child_key = signing_key(82);
    let owner_address = address_from_public_key(&public_key_hex(&owner_key));
    let child_address = address_from_public_key(&public_key_hex(&child_key));

    let funding_a = fund_address(&mut state, "fund-restart-conflict-a", owner_address.clone(), 60);
    let funding_b = fund_address(&mut state, "fund-restart-conflict-b", owner_address.clone(), 80);

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
    let expected_first_seen = state.mempool.first_seen.clone();
    let expected_admission_height = state.mempool.admission_height.clone();

    let before = classify_mempool_conflicts_v3(&incoming, &state);
    assert_eq!(before.direct_conflict_txids, expected_direct);
    assert_eq!(before.conflict_package_txids, expected_package);
    assert_eq!(canonical_mempool_txids(&state), expected_order);

    let mut reordered = state.clone();
    reordered.mempool.transactions.clear();
    for tx in [&shared_child, &direct_b, &direct_a] {
        reordered
            .mempool
            .transactions
            .insert(tx.txid.clone(), tx.clone());
    }
    assert_eq!(classify_mempool_conflicts_v3(&incoming, &reordered), before);
    assert_eq!(canonical_mempool_txids(&reordered), expected_order);

    persist_snapshot(&path_a, &state);
    persist_snapshot(&path_b, &reordered);

    let storage_a = Storage::open(&path_a).unwrap();
    let restored_a = storage_a.load_chain_state().unwrap().unwrap();
    let storage_b = Storage::open(&path_b).unwrap();
    let restored_b = storage_b.load_chain_state().unwrap().unwrap();

    let after_a = classify_mempool_conflicts_v3(&incoming, &restored_a);
    let after_b = classify_mempool_conflicts_v3(&incoming, &restored_b);
    assert_eq!(after_a, before);
    assert_eq!(after_b, before);
    assert_eq!(after_a.direct_conflict_txids, expected_direct);
    assert_eq!(after_a.conflict_package_txids, expected_package);
    assert_eq!(canonical_mempool_txids(&restored_a), expected_order);
    assert_eq!(canonical_mempool_txids(&restored_b), expected_order);
    assert_eq!(restored_a.mempool.first_seen, expected_first_seen);
    assert_eq!(restored_b.mempool.first_seen, expected_first_seen);
    assert_eq!(
        restored_a.mempool.admission_height,
        expected_admission_height
    );
    assert_eq!(
        restored_b.mempool.admission_height,
        expected_admission_height
    );

    drop(storage_a);
    drop(storage_b);
    let _ = std::fs::remove_dir_all(path_a);
    let _ = std::fs::remove_dir_all(path_b);
}