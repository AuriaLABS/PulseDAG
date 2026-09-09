use pulsedag_core::{
    genesis::init_chain_state,
    mempool::canonical_mempool_txids,
    mempool_admission_v3::classify_mempool_conflicts_v3,
    types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
    AcceptSource, ChainState,
};
use pulsedag_storage::Storage;

const OWNER_PUBLIC_KEY: &str =
    "c050c5637a44fa8629fff3cccce2300cb362a63d99d95fc54145266f4332445a";
const OWNER_ADDRESS: &str = "pulse187424cbe0e810e23caf67d743f6b26db75186de3";
const CHILD_PUBLIC_KEY: &str =
    "2012cb90ca60e8e5d8daf66e2272d2233e0486d557e8c66141ed8920177d7eb7";
const CHILD_ADDRESS: &str = "pulse175e53614c15cf5f35ed55704de940068319b1496";

const DIRECT_A_TXID: &str =
    "d494a4b8ca2b9be61613d4530c0b3584069ec007a44eb9eab5ee3fc26f85e12d";
const DIRECT_A_SIGNATURE: &str =
    "5fd38da2342f20804511abb11f28942100a6668cc7670de70a6d8d3f6561312f8879530a30c4f22d788984d7f93173b0df04fe5d7a407f565dded4a82c384a05";
const DIRECT_B_TXID: &str =
    "760bbd1668d3392702ad37d0231412022a791e94936acb0c8cfc6c002044ae62";
const DIRECT_B_SIGNATURE: &str =
    "5051d59a22142d54de603174799d761a73b8810b47cefc2a660ab3bac1b8ed71588abadbf6d1c32fe22cf00bf658bf2dba941e8fe4eb52ba583edaaad232ac00";
const SHARED_CHILD_TXID: &str =
    "1a737e8b0698313e41310ebb9cdb1374c103487510697d9ea497782b3eb2fff6";
const SHARED_CHILD_SIGNATURE: &str =
    "4791c090ee43112bce14728dc6a8746e5ceac937bd4785c7e898d8979b9201dd2f828d35ffd12de9ede2fa6cff9a481729aeb54abcde9beb2a6c203ea2af0c07";
const INCOMING_TXID: &str =
    "950f69aee6b186db267bfb62523115ae2875b7158afa18200a9080ede359b9d0";
const INCOMING_SIGNATURE: &str =
    "bca39388be24dc7eddedcfa45d1a22dac9fd4fbb1bd043719a6f03a377634e11a68e3c04aa97049c4a37409fd9ec4b21096351d0d16d0b10b721fc6ca847d309";

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

fn fund_address(state: &mut ChainState, txid: &str, address: &str, amount: u64) -> OutPoint {
    let outpoint = OutPoint {
        txid: txid.to_string(),
        index: 0,
    };
    state.utxo.utxos.insert(
        outpoint.clone(),
        Utxo {
            outpoint: outpoint.clone(),
            address: address.to_string(),
            amount,
            coinbase: false,
            height: 1,
        },
    );
    state
        .utxo
        .address_index
        .entry(address.to_string())
        .or_default()
        .push(outpoint.clone());
    outpoint
}

fn frozen_signed_tx(
    txid: &str,
    previous_outputs: Vec<OutPoint>,
    public_key: &str,
    signature: &str,
    output_address: &str,
    output_amount: u64,
    fee: u64,
    nonce: u64,
) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version: 1,
        inputs: previous_outputs
            .into_iter()
            .map(|previous_output| TxInput {
                previous_output,
                public_key: public_key.to_string(),
                signature: signature.to_string(),
            })
            .collect(),
        outputs: vec![TxOutput {
            address: output_address.to_string(),
            amount: output_amount,
        }],
        fee,
        nonce,
    }
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

    let funding_a = fund_address(
        &mut state,
        "fund-restart-conflict-a",
        OWNER_ADDRESS,
        60,
    );
    let funding_b = fund_address(
        &mut state,
        "fund-restart-conflict-b",
        OWNER_ADDRESS,
        80,
    );

    let direct_a = frozen_signed_tx(
        DIRECT_A_TXID,
        vec![funding_a.clone()],
        OWNER_PUBLIC_KEY,
        DIRECT_A_SIGNATURE,
        CHILD_ADDRESS,
        50,
        10,
        1,
    );
    let direct_b = frozen_signed_tx(
        DIRECT_B_TXID,
        vec![funding_b.clone()],
        OWNER_PUBLIC_KEY,
        DIRECT_B_SIGNATURE,
        CHILD_ADDRESS,
        70,
        10,
        2,
    );

    pulsedag_core::accept_transaction(direct_a.clone(), &mut state, AcceptSource::Rpc).unwrap();
    pulsedag_core::accept_transaction(direct_b.clone(), &mut state, AcceptSource::Rpc).unwrap();

    let shared_child = frozen_signed_tx(
        SHARED_CHILD_TXID,
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
        CHILD_PUBLIC_KEY,
        SHARED_CHILD_SIGNATURE,
        CHILD_ADDRESS,
        110,
        10,
        3,
    );
    pulsedag_core::accept_transaction(
        shared_child.clone(),
        &mut state,
        AcceptSource::Rpc,
    )
    .unwrap();

    let incoming = frozen_signed_tx(
        INCOMING_TXID,
        vec![funding_a, funding_b],
        OWNER_PUBLIC_KEY,
        INCOMING_SIGNATURE,
        OWNER_ADDRESS,
        130,
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
