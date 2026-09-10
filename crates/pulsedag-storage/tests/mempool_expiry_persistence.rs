use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
};

use pulsedag_core::{
    genesis::init_chain_state,
    mempool::prune_expired_mempool,
    state::{
        ChainState, ContractRuntimeState, DagState, MempoolCounters, MissingParentTerminalEntry,
        UtxoState,
    },
    types::{Block, Hash, OutPoint, Transaction},
};
use pulsedag_storage::{Storage, STORAGE_SCHEMA_VERSION};
use serde::Serialize;

const CHAIN_STATE_KEY: &[u8] = b"chain_state";
const MEMPOOL_ADMISSION_HEIGHT_V1_KEY: &[u8] = b"mempool_admission_height_v1";
const MEMPOOL_ORPHAN_ADMISSION_HEIGHT_V1_KEY: &[u8] = b"mempool_orphan_admission_height_v1";
static TEMP_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize)]
struct PreExpiryMempool<'a> {
    transactions: &'a HashMap<Hash, Transaction>,
    spent_outpoints: &'a HashSet<OutPoint>,
    first_seen: &'a HashMap<Hash, u64>,
    next_first_seen: u64,
    orphan_transactions: &'a HashMap<Hash, Transaction>,
    orphan_missing_outpoints: &'a HashMap<Hash, Vec<OutPoint>>,
    orphan_received_order: &'a HashMap<Hash, u64>,
    next_orphan_order: u64,
    counters: &'a MempoolCounters,
    max_transactions: usize,
    max_spent_outpoints: usize,
    max_orphans: usize,
}

#[derive(Serialize)]
struct PreExpiryChainState<'a> {
    chain_id: &'a String,
    dag: &'a DagState,
    utxo: &'a UtxoState,
    mempool: PreExpiryMempool<'a>,
    contracts: &'a ContractRuntimeState,
    orphan_blocks: &'a HashMap<Hash, Block>,
    orphan_missing_parents: &'a HashMap<Hash, Vec<Hash>>,
    orphan_parent_index: &'a HashMap<Hash, BTreeSet<Hash>>,
    orphan_received_at_ms: &'a HashMap<Hash, u64>,
    terminal_missing_parents: &'a HashMap<Hash, MissingParentTerminalEntry>,
    chain_state_generation: u64,
    accepted_commit_generation_conflict_total: u64,
    accepted_commit_reprepare_total: u64,
    accepted_commit_serialized_total: u64,
    accepted_commit_publish_mismatch_total: u64,
    accepted_commit_last_hash: &'a Option<Hash>,
    accepted_commit_last_source: &'a Option<String>,
    chain_state_mutation_generation: u64,
    chain_state_mutation_source: &'a Option<String>,
    chain_state_mutation_conflict_total: u64,
    chain_state_reprepare_total: u64,
    accepted_hash_lost_from_memory_total: u64,
    accepted_hash_terminalization_prevented_total: u64,
    accepted_storage_repair_total: u64,
    last_lost_accepted_hash: &'a Option<Hash>,
    last_lost_accepted_height: Option<u64>,
}

fn pre_expiry_bytes(state: &ChainState) -> Vec<u8> {
    let mempool = PreExpiryMempool {
        transactions: &state.mempool.transactions,
        spent_outpoints: &state.mempool.spent_outpoints,
        first_seen: &state.mempool.first_seen,
        next_first_seen: state.mempool.next_first_seen,
        orphan_transactions: &state.mempool.orphan_transactions,
        orphan_missing_outpoints: &state.mempool.orphan_missing_outpoints,
        orphan_received_order: &state.mempool.orphan_received_order,
        next_orphan_order: state.mempool.next_orphan_order,
        counters: &state.mempool.counters,
        max_transactions: state.mempool.max_transactions,
        max_spent_outpoints: state.mempool.max_spent_outpoints,
        max_orphans: state.mempool.max_orphans,
    };
    bincode::serialize(&PreExpiryChainState {
        chain_id: &state.chain_id,
        dag: &state.dag,
        utxo: &state.utxo,
        mempool,
        contracts: &state.contracts,
        orphan_blocks: &state.orphan_blocks,
        orphan_missing_parents: &state.orphan_missing_parents,
        orphan_parent_index: &state.orphan_parent_index,
        orphan_received_at_ms: &state.orphan_received_at_ms,
        terminal_missing_parents: &state.terminal_missing_parents,
        chain_state_generation: state.chain_state_generation,
        accepted_commit_generation_conflict_total: state.accepted_commit_generation_conflict_total,
        accepted_commit_reprepare_total: state.accepted_commit_reprepare_total,
        accepted_commit_serialized_total: state.accepted_commit_serialized_total,
        accepted_commit_publish_mismatch_total: state.accepted_commit_publish_mismatch_total,
        accepted_commit_last_hash: &state.accepted_commit_last_hash,
        accepted_commit_last_source: &state.accepted_commit_last_source,
        chain_state_mutation_generation: state.chain_state_mutation_generation,
        chain_state_mutation_source: &state.chain_state_mutation_source,
        chain_state_mutation_conflict_total: state.chain_state_mutation_conflict_total,
        chain_state_reprepare_total: state.chain_state_reprepare_total,
        accepted_hash_lost_from_memory_total: state.accepted_hash_lost_from_memory_total,
        accepted_hash_terminalization_prevented_total: state
            .accepted_hash_terminalization_prevented_total,
        accepted_storage_repair_total: state.accepted_storage_repair_total,
        last_lost_accepted_hash: &state.last_lost_accepted_hash,
        last_lost_accepted_height: state.last_lost_accepted_height,
    })
    .unwrap()
}

fn temp_db_path(name: &str) -> String {
    let sequence = TEMP_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!(
            "pulsedag-expiry-{name}-pid{}-{sequence}",
            std::process::id()
        ))
        .to_string_lossy()
        .into_owned()
}

fn dummy_tx(txid: &str) -> Transaction {
    Transaction {
        txid: txid.to_string(),
        version: 1,
        inputs: Vec::new(),
        outputs: Vec::new(),
        fee: 0,
        nonce: 0,
    }
}

#[test]
fn exact_pre_expiry_chain_state_bytes_load_at_real_rocksdb_boundary() {
    let path = temp_db_path("legacy-layout");
    let storage = Storage::open(&path).unwrap();
    let mut state = init_chain_state("expiry-legacy-layout".to_string());
    let transaction = dummy_tx("legacy-live");
    state
        .mempool
        .transactions
        .insert(transaction.txid.clone(), transaction.clone());
    state.mempool.first_seen.insert(transaction.txid.clone(), 0);
    state.mempool.next_first_seen = 1;
    state
        .mempool
        .admission_height
        .insert(transaction.txid.clone(), 42);

    let legacy = pre_expiry_bytes(&state);
    assert_eq!(
        bincode::serialize(&state).unwrap(),
        legacy,
        "serde-skipped admission metadata must not change the pre-expiry bincode layout"
    );

    let meta_cf = storage.db.cf_handle("meta").unwrap();
    storage
        .db
        .put_cf(&meta_cf, CHAIN_STATE_KEY, legacy)
        .unwrap();
    storage
        .db
        .delete_cf(&meta_cf, MEMPOOL_ADMISSION_HEIGHT_V1_KEY)
        .unwrap();

    let loaded = storage.load_chain_state().unwrap().unwrap();
    assert!(loaded.mempool.transactions.contains_key("legacy-live"));
    assert!(loaded.mempool.admission_height.is_empty());
    assert_eq!(STORAGE_SCHEMA_VERSION, 1);

    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn admission_height_sidecar_survives_real_restart_and_filters_stale_txids() {
    let path = temp_db_path("restart");
    let mut state = init_chain_state("expiry-restart".to_string());
    let transaction = dummy_tx("live");
    state
        .mempool
        .transactions
        .insert(transaction.txid.clone(), transaction.clone());
    state.mempool.first_seen.insert(transaction.txid.clone(), 0);
    state.mempool.next_first_seen = 1;
    state
        .mempool
        .admission_height
        .insert(transaction.txid.clone(), 0);
    state
        .mempool
        .admission_height
        .insert("stale".to_string(), 0);

    let mut before_restart = state.clone();
    let expected = prune_expired_mempool(&mut before_restart, 2, 2);
    assert_eq!(expected.expired_txids, vec!["live".to_string()]);

    {
        let storage = Storage::open(&path).unwrap();
        for block in state.dag.blocks.values() {
            storage.persist_block(block).unwrap();
        }
        storage.persist_chain_state(&state).unwrap();

        // Deliberately inject a stale sidecar row to prove load never revives it.
        let mut raw_sidecar = BTreeMap::new();
        raw_sidecar.insert("live".to_string(), 0_u64);
        raw_sidecar.insert("stale".to_string(), 0_u64);
        let meta_cf = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(
                &meta_cf,
                MEMPOOL_ADMISSION_HEIGHT_V1_KEY,
                bincode::serialize(&raw_sidecar).unwrap(),
            )
            .unwrap();
    }

    let reopened = Storage::open(&path).unwrap();
    let mut loaded = reopened.load_chain_state().unwrap().unwrap();
    assert_eq!(loaded.mempool.admission_height.get("live"), Some(&0));
    assert!(!loaded.mempool.admission_height.contains_key("stale"));
    let after_restart = prune_expired_mempool(&mut loaded, 2, 2);
    assert_eq!(after_restart, expected);
    assert_eq!(
        reopened.storage_schema_metadata().unwrap().schema_version,
        STORAGE_SCHEMA_VERSION
    );

    drop(reopened);
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn orphan_admission_height_sidecar_survives_restart_and_filters_stale_txids() {
    let path = temp_db_path("orphan-restart");
    let mut state = init_chain_state("expiry-orphan-restart".to_string());
    let orphan = dummy_tx("orphan-live");
    state
        .mempool
        .orphan_transactions
        .insert(orphan.txid.clone(), orphan);
    state
        .mempool
        .orphan_admission_height
        .insert("orphan-live".to_string(), 5);
    state
        .mempool
        .orphan_admission_height
        .insert("stale-orphan".to_string(), 1);

    {
        let storage = Storage::open(&path).unwrap();
        for block in state.dag.blocks.values() {
            storage.persist_block(block).unwrap();
        }
        storage.persist_chain_state(&state).unwrap();
        let mut raw = BTreeMap::new();
        raw.insert("orphan-live".to_string(), 5_u64);
        raw.insert("stale-orphan".to_string(), 1_u64);
        let meta_cf = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(
                &meta_cf,
                MEMPOOL_ORPHAN_ADMISSION_HEIGHT_V1_KEY,
                bincode::serialize(&raw).unwrap(),
            )
            .unwrap();
    }

    let storage = Storage::open(&path).unwrap();
    let loaded = storage.load_chain_state().unwrap().unwrap();
    assert_eq!(
        loaded.mempool.orphan_admission_height.get("orphan-live"),
        Some(&5)
    );
    assert!(!loaded
        .mempool
        .orphan_admission_height
        .contains_key("stale-orphan"));
    assert_eq!(STORAGE_SCHEMA_VERSION, 1);
    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn corrupt_optional_age_sidecars_recover_valid_chain_state_and_surface_events() {
    let path = temp_db_path("corrupt-sidecars");
    let mut state = init_chain_state("expiry-corrupt-sidecars".to_string());
    let live = dummy_tx("live");
    state.mempool.transactions.insert(live.txid.clone(), live);
    state.mempool.first_seen.insert("live".to_string(), 0);
    state.mempool.admission_height.insert("live".to_string(), 7);
    let orphan = dummy_tx("orphan");
    state
        .mempool
        .orphan_transactions
        .insert(orphan.txid.clone(), orphan);
    state
        .mempool
        .orphan_admission_height
        .insert("orphan".to_string(), 9);

    let storage = Storage::open(&path).unwrap();
    for block in state.dag.blocks.values() {
        storage.persist_block(block).unwrap();
    }
    storage.persist_chain_state(&state).unwrap();
    let meta_cf = storage.db.cf_handle("meta").unwrap();
    storage
        .db
        .put_cf(&meta_cf, MEMPOOL_ADMISSION_HEIGHT_V1_KEY, b"corrupt")
        .unwrap();
    storage
        .db
        .put_cf(
            &meta_cf,
            MEMPOOL_ORPHAN_ADMISSION_HEIGHT_V1_KEY,
            b"corrupt-orphan",
        )
        .unwrap();

    let loaded = storage.load_chain_state().unwrap().unwrap();
    assert!(loaded.mempool.transactions.contains_key("live"));
    assert!(loaded.mempool.orphan_transactions.contains_key("orphan"));
    assert!(loaded.mempool.admission_height.is_empty());
    assert!(loaded.mempool.orphan_admission_height.is_empty());
    let events = storage.list_runtime_events(20).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "mempool_admission_height_sidecar_corrupt_recovered"));
    assert!(events.iter().any(|event| {
        event.kind == "mempool_orphan_admission_height_sidecar_corrupt_recovered"
    }));
    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}
