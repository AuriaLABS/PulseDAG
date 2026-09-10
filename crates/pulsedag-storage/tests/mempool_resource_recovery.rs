use std::sync::atomic::Ordering;

use pulsedag_core::{
    genesis::init_chain_state,
    types::{Transaction, TxOutput},
};
use pulsedag_storage::{Storage, MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL};

fn temp_db_path(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "pulsedag-{label}-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn corrupt_optional_admission_height_sidecar_recovers_valid_chain_state_fail_safe() {
    let path = temp_db_path("resource-sidecar-recovery");
    let before = MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL.load(Ordering::Relaxed);
    {
        let storage = Storage::open(&path).unwrap();
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
