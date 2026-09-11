use std::sync::atomic::Ordering;

use pulsedag_core::{
    genesis::init_chain_state,
    types::{Transaction, TxOutput},
};
use pulsedag_storage::{
    Storage, MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL,
    MEMPOOL_ORPHAN_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL, STORAGE_SCHEMA_VERSION,
};

fn temp_db_path(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("pulsedag-{label}-{}-{nonce}", std::process::id()))
}

#[test]
fn corrupt_optional_admission_height_sidecar_recovers_valid_chain_state_fail_safe() {
    let path = temp_db_path("resource-sidecar-recovery");
    let before = MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL.load(Ordering::Relaxed);
    {
        let storage = Storage::open(path.to_str().expect("temporary path is valid UTF-8")).unwrap();
        let mut state = init_chain_state("resource-sidecar-recovery".to_string());
        let transaction = Transaction {
            txid: "persisted-resource-tx".to_string(),
            version: 1,
            inputs: Vec::new(),
            outputs: vec![TxOutput {
                address: "pulse1persisted000000000000000000000000000000000000000".to_string(),
                amount: 1,
            }],
            fee: 10,
            nonce: 1,
        };
        state
            .mempool
            .transactions
            .insert(transaction.txid.clone(), transaction);
        state
            .mempool
            .admission_height
            .insert("persisted-resource-tx".to_string(), 7);
        storage.persist_chain_state(&state).unwrap();

        let meta = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(meta, b"mempool_admission_height_v1", b"not-bincode")
            .unwrap();

        let restored = storage
            .load_chain_state()
            .expect("corrupt optional age sidecar must not block chain-state load")
            .expect("persisted chain state");
        assert!(restored
            .mempool
            .transactions
            .contains_key("persisted-resource-tx"));
        assert!(restored.mempool.admission_height.is_empty());
    }
    assert!(
        MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL.load(Ordering::Relaxed)
            >= before.saturating_add(1)
    );
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn orphan_admission_height_sidecar_survives_restart_and_filters_stale_txids() {
    let path = temp_db_path("resource-orphan-sidecar");
    {
        let storage = Storage::open(path.to_str().expect("temporary path is valid UTF-8")).unwrap();
        let mut state = init_chain_state("resource-orphan-sidecar".to_string());
        let orphan = Transaction {
            txid: "orphan-live".to_string(),
            version: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            fee: 10,
            nonce: 1,
        };
        state
            .mempool
            .orphan_transactions
            .insert(orphan.txid.clone(), orphan);
        state
            .mempool
            .orphan_admission_height
            .insert("orphan-live".to_string(), 5);
        for block in state.dag.blocks.values() {
            storage.persist_block(block).unwrap();
        }
        storage.persist_chain_state(&state).unwrap();

        let mut sidecar = std::collections::BTreeMap::new();
        sidecar.insert("orphan-live".to_string(), 5_u64);
        sidecar.insert("stale-orphan".to_string(), 1_u64);
        let meta = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(
                meta,
                b"mempool_orphan_admission_height_v1",
                bincode::serialize(&sidecar).unwrap(),
            )
            .unwrap();
    }

    let storage = Storage::open(path.to_str().expect("temporary path is valid UTF-8")).unwrap();
    let restored = storage.load_chain_state().unwrap().unwrap();
    assert_eq!(
        restored.mempool.orphan_admission_height.get("orphan-live"),
        Some(&5)
    );
    assert!(!restored
        .mempool
        .orphan_admission_height
        .contains_key("stale-orphan"));
    assert_eq!(STORAGE_SCHEMA_VERSION, 1);
    drop(storage);
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn corrupt_optional_orphan_age_sidecar_recovers_valid_chain_state_fail_safe() {
    let path = temp_db_path("resource-orphan-sidecar-recovery");
    let before = MEMPOOL_ORPHAN_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL.load(Ordering::Relaxed);
    {
        let storage = Storage::open(path.to_str().expect("temporary path is valid UTF-8")).unwrap();
        let mut state = init_chain_state("resource-orphan-sidecar-recovery".to_string());
        let orphan = Transaction {
            txid: "persisted-orphan".to_string(),
            version: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            fee: 10,
            nonce: 1,
        };
        state
            .mempool
            .orphan_transactions
            .insert(orphan.txid.clone(), orphan);
        state
            .mempool
            .orphan_admission_height
            .insert("persisted-orphan".to_string(), 9);
        storage.persist_chain_state(&state).unwrap();
        let meta = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(meta, b"mempool_orphan_admission_height_v1", b"not-bincode")
            .unwrap();

        let restored = storage
            .load_chain_state()
            .expect("corrupt optional orphan age sidecar must not block chain-state load")
            .expect("persisted chain state");
        assert!(restored
            .mempool
            .orphan_transactions
            .contains_key("persisted-orphan"));
        assert!(restored.mempool.orphan_admission_height.is_empty());
    }
    assert!(
        MEMPOOL_ORPHAN_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL.load(Ordering::Relaxed)
            >= before.saturating_add(1)
    );
    let _ = std::fs::remove_dir_all(path);
}
