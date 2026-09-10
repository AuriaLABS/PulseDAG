from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"guard mismatch {path}: expected 1, got {count}: {old[:120]!r}")
    p.write_text(text.replace(old, new, 1))


# Orphan age is operational metadata like live admission_height: keep it out of
# CHAIN_STATE serialization and persist it independently in RocksDB.
replace_once(
    "crates/pulsedag-core/src/state.rs",
    "    #[serde(skip, default)]\n    pub admission_height: HashMap<Hash, u64>,\n    #[serde(default)]\n    pub orphan_transactions: HashMap<Hash, Transaction>,",
    "    #[serde(skip, default)]\n    pub admission_height: HashMap<Hash, u64>,\n    #[serde(skip, default)]\n    pub orphan_admission_height: HashMap<Hash, u64>,\n    #[serde(default)]\n    pub orphan_transactions: HashMap<Hash, Transaction>,",
)
replace_once(
    "crates/pulsedag-core/src/state.rs",
    "            admission_height: HashMap::new(),\n            orphan_transactions: HashMap::new(),",
    "            admission_height: HashMap::new(),\n            orphan_admission_height: HashMap::new(),\n            orphan_transactions: HashMap::new(),",
)

# Fresh orphan age is bound to the deterministic DAG logical clock. Overflow
# pruning uses fee + txid only, never arrival/hash-map iteration.
old_prune = '''fn prune_orphans(state: &mut ChainState) {\n    if state.mempool.orphan_transactions.len() <= state.mempool.max_orphans {\n        return;\n    }\n    let mut by_age = state\n        .mempool\n        .orphan_received_order\n        .iter()\n        .map(|(txid, order)| (txid.clone(), *order))\n        .collect::<Vec<_>>();\n    by_age.sort_by_key(|(_, order)| *order);\n\n    let overflow = state\n        .mempool\n        .orphan_transactions\n        .len()\n        .saturating_sub(state.mempool.max_orphans);\n    for (txid, _) in by_age.into_iter().take(overflow) {\n        let removed = state.mempool.orphan_transactions.remove(&txid).is_some();\n        state.mempool.orphan_missing_outpoints.remove(&txid);\n        state.mempool.orphan_received_order.remove(&txid);\n        if removed {\n            state.mempool.counters.orphan_dropped_total = state\n                .mempool\n                .counters\n                .orphan_dropped_total\n                .saturating_add(1);\n            state.mempool.counters.orphan_pruned_total =\n                state.mempool.counters.orphan_pruned_total.saturating_add(1);\n        }\n    }\n}\n'''
new_prune = '''fn prune_orphans(state: &mut ChainState) {\n    if state.mempool.orphan_transactions.len() <= state.mempool.max_orphans {\n        return;\n    }\n    let mut candidates = state\n        .mempool\n        .orphan_transactions\n        .values()\n        .map(|tx| (tx.fee, tx.txid.clone()))\n        .collect::<Vec<_>>();\n    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));\n\n    let overflow = state\n        .mempool\n        .orphan_transactions\n        .len()\n        .saturating_sub(state.mempool.max_orphans);\n    for (_, txid) in candidates.into_iter().take(overflow) {\n        let removed = state.mempool.orphan_transactions.remove(&txid).is_some();\n        state.mempool.orphan_missing_outpoints.remove(&txid);\n        state.mempool.orphan_received_order.remove(&txid);\n        state.mempool.orphan_admission_height.remove(&txid);\n        if removed {\n            state.mempool.counters.orphan_dropped_total = state\n                .mempool\n                .counters\n                .orphan_dropped_total\n                .saturating_add(1);\n            state.mempool.counters.orphan_pruned_total =\n                state.mempool.counters.orphan_pruned_total.saturating_add(1);\n        }\n    }\n}\n'''
replace_once("crates/pulsedag-core/src/accept.rs", old_prune, new_prune)
replace_once(
    "crates/pulsedag-core/src/accept.rs",
    "        state.mempool.orphan_received_order.insert(txid, order);\n        state.mempool.counters.orphaned_total =",
    "        state.mempool.orphan_received_order.insert(txid.clone(), order);\n        state\n            .mempool\n            .orphan_admission_height\n            .insert(txid, state.dag.best_height);\n        state.mempool.counters.orphaned_total =",
)
replace_once(
    "crates/pulsedag-core/src/accept.rs",
    "    state.mempool.orphan_received_order.remove(txid);\n}\n\n#[derive(Clone, Copy)]",
    "    state.mempool.orphan_received_order.remove(txid);\n    state.mempool.orphan_admission_height.remove(txid);\n}\n\n#[derive(Clone, Copy)]",
)

# Extend the already-staged resource normalizer without changing the frozen
# resource-policy field vector/fingerprint. The same 1440-height TTL applies to
# live and orphan pools. Missing historical orphan age is seeded at current
# best_height once, so migration is conservative but future expiry is enabled.
replace_once(
    "crates/pulsedag-core/src/mempool_resource_v1.rs",
    "pub struct MempoolResourceNormalizationV1 {\n    pub removed_live_txids: Vec<Hash>,\n    pub expired_live_txids: Vec<Hash>,\n    pub dropped_orphan_txids: Vec<Hash>,\n}",
    "pub struct MempoolResourceNormalizationV1 {\n    pub removed_live_txids: Vec<Hash>,\n    pub expired_live_txids: Vec<Hash>,\n    pub expired_orphan_txids: Vec<Hash>,\n    pub dropped_orphan_txids: Vec<Hash>,\n}",
)
replace_once(
    "crates/pulsedag-core/src/mempool_resource_v1.rs",
    "    state.mempool.orphan_received_order.remove(txid);\n    if removed {",
    "    state.mempool.orphan_received_order.remove(txid);\n    state.mempool.orphan_admission_height.remove(txid);\n    if removed {",
)
old_orphans = '''    let orphan_txids = state\n        .mempool\n        .orphan_transactions\n        .keys()\n        .cloned()\n        .collect::<BTreeSet<_>>();\n    let mut dropped_orphans = BTreeSet::new();\n    for txid in orphan_txids {\n        let oversized = state\n            .mempool\n            .orphan_transactions\n            .get(&txid)\n            .is_some_and(|tx| {\n                matches!(\n                    assess_production_transaction_resources_v1(tx, state),\n                    Err(MempoolResourceAssessmentErrorV1::TransactionTooLarge { .. })\n                )\n            });\n        if oversized && remove_orphan(state, &txid) {\n            dropped_orphans.insert(txid);\n        }\n    }\n\n    if state.mempool.orphan_transactions.len() > state.mempool.max_orphans {\n        let mut ordered = state\n            .mempool\n            .orphan_transactions\n            .keys()\n            .map(|txid| {\n                (\n                    state\n                        .mempool\n                        .orphan_received_order\n                        .get(txid)\n                        .copied()\n                        .unwrap_or(u64::MAX),\n                    txid.clone(),\n                )\n            })\n            .collect::<Vec<_>>();\n        ordered.sort();\n        let overflow = state\n            .mempool\n            .orphan_transactions\n            .len()\n            .saturating_sub(state.mempool.max_orphans);\n        for (_, txid) in ordered.into_iter().take(overflow) {\n            if remove_orphan(state, &txid) {\n                dropped_orphans.insert(txid);\n            }\n        }\n    }\n\n    let live_orphans = state\n        .mempool\n        .orphan_transactions\n        .keys()\n        .cloned()\n        .collect::<BTreeSet<_>>();\n    state\n        .mempool\n        .orphan_missing_outpoints\n        .retain(|txid, _| live_orphans.contains(txid));\n    state\n        .mempool\n        .orphan_received_order\n        .retain(|txid, _| live_orphans.contains(txid));\n\n    removed_live.sort();\n    removed_live.dedup();\n    MempoolResourceNormalizationV1 {\n        removed_live_txids: removed_live,\n        expired_live_txids,\n        dropped_orphan_txids: dropped_orphans.into_iter().collect(),\n    }'''
new_orphans = '''    let orphan_txids = state\n        .mempool\n        .orphan_transactions\n        .keys()\n        .cloned()\n        .collect::<BTreeSet<_>>();\n    let current_height = state.dag.best_height;\n    for txid in &orphan_txids {\n        if !state.mempool.orphan_admission_height.contains_key(txid) {\n            state\n                .mempool\n                .orphan_admission_height\n                .insert(txid.clone(), current_height);\n        }\n    }\n\n    let mut expired_orphans = BTreeSet::new();\n    let mut dropped_orphans = BTreeSet::new();\n    for txid in &orphan_txids {\n        let expired = state\n            .mempool\n            .orphan_admission_height\n            .get(txid)\n            .copied()\n            .filter(|height| *height <= current_height)\n            .and_then(|height| height.checked_add(policy.max_age_blocks))\n            .is_some_and(|deadline| current_height >= deadline);\n        if expired && remove_orphan(state, txid) {\n            expired_orphans.insert(txid.clone());\n            dropped_orphans.insert(txid.clone());\n        }\n    }\n\n    for txid in orphan_txids {\n        let oversized = state\n            .mempool\n            .orphan_transactions\n            .get(&txid)\n            .is_some_and(|tx| {\n                matches!(\n                    assess_production_transaction_resources_v1(tx, state),\n                    Err(MempoolResourceAssessmentErrorV1::TransactionTooLarge { .. })\n                )\n            });\n        if oversized && remove_orphan(state, &txid) {\n            dropped_orphans.insert(txid);\n        }\n    }\n\n    if state.mempool.orphan_transactions.len() > state.mempool.max_orphans {\n        let mut ordered = state\n            .mempool\n            .orphan_transactions\n            .values()\n            .map(|tx| (tx.fee, tx.txid.clone()))\n            .collect::<Vec<_>>();\n        ordered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));\n        let overflow = state\n            .mempool\n            .orphan_transactions\n            .len()\n            .saturating_sub(state.mempool.max_orphans);\n        for (_, txid) in ordered.into_iter().take(overflow) {\n            if remove_orphan(state, &txid) {\n                dropped_orphans.insert(txid);\n            }\n        }\n    }\n\n    let live_orphans = state\n        .mempool\n        .orphan_transactions\n        .keys()\n        .cloned()\n        .collect::<BTreeSet<_>>();\n    state\n        .mempool\n        .orphan_missing_outpoints\n        .retain(|txid, _| live_orphans.contains(txid));\n    state\n        .mempool\n        .orphan_received_order\n        .retain(|txid, _| live_orphans.contains(txid));\n    state\n        .mempool\n        .orphan_admission_height\n        .retain(|txid, _| live_orphans.contains(txid));\n\n    removed_live.sort();\n    removed_live.dedup();\n    MempoolResourceNormalizationV1 {\n        removed_live_txids: removed_live,\n        expired_live_txids,\n        expired_orphan_txids: expired_orphans.into_iter().collect(),\n        dropped_orphan_txids: dropped_orphans.into_iter().collect(),\n    }'''
replace_once("crates/pulsedag-core/src/mempool_resource_v1.rs", old_orphans, new_orphans)

# RocksDB: independent optional orphan-age sidecar. Missing/corrupt optional age
# metadata never invalidates an otherwise valid CHAIN_STATE.
replace_once(
    "crates/pulsedag-storage/src/lib.rs",
    'const MEMPOOL_ADMISSION_HEIGHT_V1_KEY: &[u8] = b"mempool_admission_height_v1";\n',
    'const MEMPOOL_ADMISSION_HEIGHT_V1_KEY: &[u8] = b"mempool_admission_height_v1";\nconst MEMPOOL_ORPHAN_ADMISSION_HEIGHT_V1_KEY: &[u8] =\n    b"mempool_orphan_admission_height_v1";\n',
)
replace_once(
    "crates/pulsedag-storage/src/lib.rs",
    "pub static MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL: AtomicU64 = AtomicU64::new(0);\npub static SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL: AtomicU64 = AtomicU64::new(0);",
    "pub static MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL: AtomicU64 = AtomicU64::new(0);\npub static MEMPOOL_ORPHAN_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL: AtomicU64 = AtomicU64::new(0);\npub static SNAPSHOT_VERIFICATION_GENERATION_CHANGED_TOTAL: AtomicU64 = AtomicU64::new(0);",
)
replace_once(
    "crates/pulsedag-storage/src/lib.rs",
    '''        batch.put_cf(\n            meta_cf,\n            MEMPOOL_ADMISSION_HEIGHT_V1_KEY,\n            bincode::serialize(&admission_height)\n                .map_err(|e| PulseError::StorageError(e.to_string()))?,\n        );\n        batch.put_cf(\n            meta_cf,\n            STORAGE_SCHEMA_VERSION_KEY,''',
    '''        batch.put_cf(\n            meta_cf,\n            MEMPOOL_ADMISSION_HEIGHT_V1_KEY,\n            bincode::serialize(&admission_height)\n                .map_err(|e| PulseError::StorageError(e.to_string()))?,\n        );\n        let mut orphan_admission_height = BTreeMap::<Hash, u64>::new();\n        for (txid, height) in &state.mempool.orphan_admission_height {\n            if state.mempool.orphan_transactions.contains_key(txid) {\n                orphan_admission_height.insert(txid.clone(), *height);\n            }\n        }\n        batch.put_cf(\n            meta_cf,\n            MEMPOOL_ORPHAN_ADMISSION_HEIGHT_V1_KEY,\n            bincode::serialize(&orphan_admission_height)\n                .map_err(|e| PulseError::StorageError(e.to_string()))?,\n        );\n        batch.put_cf(\n            meta_cf,\n            STORAGE_SCHEMA_VERSION_KEY,''',
)
load_tail = '''        if let Some(sidecar) = self\n            .db\n            .get_cf(&cf, MEMPOOL_ADMISSION_HEIGHT_V1_KEY)\n            .map_err(|e| PulseError::StorageError(e.to_string()))?\n        {\n            match bincode::deserialize::<BTreeMap<Hash, u64>>(&sidecar) {\n                Ok(persisted) => {\n                    for (txid, height) in persisted {\n                        if state.mempool.transactions.contains_key(&txid) {\n                            state.mempool.admission_height.insert(txid, height);\n                        }\n                    }\n                }\n                Err(_) => {\n                    // Optional policy-age metadata must not invalidate an otherwise\n                    // valid positional chain state. Empty age is the legacy fail-safe\n                    // retain behavior frozen by #1081.\n                    MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL\n                        .fetch_add(1, Ordering::Relaxed);\n                }\n            }\n        }\n        Ok(Some(state))'''
load_with_orphans = '''        if let Some(sidecar) = self\n            .db\n            .get_cf(&cf, MEMPOOL_ADMISSION_HEIGHT_V1_KEY)\n            .map_err(|e| PulseError::StorageError(e.to_string()))?\n        {\n            match bincode::deserialize::<BTreeMap<Hash, u64>>(&sidecar) {\n                Ok(persisted) => {\n                    for (txid, height) in persisted {\n                        if state.mempool.transactions.contains_key(&txid) {\n                            state.mempool.admission_height.insert(txid, height);\n                        }\n                    }\n                }\n                Err(_) => {\n                    // Optional policy-age metadata must not invalidate an otherwise\n                    // valid positional chain state. Empty age is the legacy fail-safe\n                    // retain behavior frozen by #1081.\n                    MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL\n                        .fetch_add(1, Ordering::Relaxed);\n                }\n            }\n        }\n        state.mempool.orphan_admission_height.clear();\n        if let Some(sidecar) = self\n            .db\n            .get_cf(&cf, MEMPOOL_ORPHAN_ADMISSION_HEIGHT_V1_KEY)\n            .map_err(|e| PulseError::StorageError(e.to_string()))?\n        {\n            match bincode::deserialize::<BTreeMap<Hash, u64>>(&sidecar) {\n                Ok(persisted) => {\n                    for (txid, height) in persisted {\n                        if state.mempool.orphan_transactions.contains_key(&txid) {\n                            state.mempool.orphan_admission_height.insert(txid, height);\n                        }\n                    }\n                }\n                Err(_) => {\n                    MEMPOOL_ORPHAN_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL\n                        .fetch_add(1, Ordering::Relaxed);\n                }\n            }\n        }\n        Ok(Some(state))'''
replace_once("crates/pulsedag-storage/src/lib.rs", load_tail, load_with_orphans)

# Extend focused integration evidence.
core_test = Path("crates/pulsedag-core/tests/mempool_resource_freeze.rs")
core_test.write_text(
    core_test.read_text().rstrip()
    + r'''

#[test]
fn orphan_expiry_uses_same_1440_height_boundary_and_missing_age_migrates_conservatively() {
    let mut state = init_chain_state("resource-orphan-expiry".to_string());
    let aged = tx("aged-orphan", 10);
    let migrated = tx("migrated-orphan", 11);
    state
        .mempool
        .orphan_transactions
        .insert(aged.txid.clone(), aged);
    state
        .mempool
        .orphan_transactions
        .insert(migrated.txid.clone(), migrated);
    state
        .mempool
        .orphan_admission_height
        .insert("aged-orphan".to_string(), 0);

    state.dag.best_height = 1_439;
    let before = normalize_production_mempool_resources_v1(&mut state);
    assert!(before.expired_orphan_txids.is_empty());
    assert_eq!(
        state.mempool.orphan_admission_height.get("migrated-orphan"),
        Some(&1_439)
    );

    state.dag.best_height = 1_440;
    let boundary = normalize_production_mempool_resources_v1(&mut state);
    assert_eq!(boundary.expired_orphan_txids, vec!["aged-orphan".to_string()]);
    assert!(!state.mempool.orphan_transactions.contains_key("aged-orphan"));
    assert!(state.mempool.orphan_transactions.contains_key("migrated-orphan"));
}

#[test]
fn restored_orphan_capacity_is_insertion_order_deterministic() {
    fn survivors(reverse: bool) -> std::collections::BTreeSet<String> {
        let mut state = init_chain_state("resource-orphan-capacity".to_string());
        let mut indices = (0..520usize).collect::<Vec<_>>();
        if reverse {
            indices.reverse();
        }
        for i in indices {
            let transaction = tx(&format!("orphan-{i:04}"), (i % 7) as u64);
            let txid = transaction.txid.clone();
            state.mempool.orphan_transactions.insert(txid.clone(), transaction);
            state.mempool.orphan_missing_outpoints.insert(txid.clone(), Vec::new());
            state.mempool.orphan_received_order.insert(txid, i as u64);
        }
        normalize_production_mempool_resources_v1(&mut state);
        state.mempool.orphan_transactions.keys().cloned().collect()
    }

    let forward = survivors(false);
    let reverse = survivors(true);
    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 512);
}
'''
    + "\n"
)

storage_test = Path("crates/pulsedag-storage/tests/mempool_resource_recovery.rs")
replace_once(
    "crates/pulsedag-storage/tests/mempool_resource_recovery.rs",
    "use pulsedag_storage::{Storage, MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL};",
    "use pulsedag_storage::{\n    Storage, MEMPOOL_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL,\n    MEMPOOL_ORPHAN_ADMISSION_HEIGHT_SIDECAR_RECOVERY_TOTAL, STORAGE_SCHEMA_VERSION,\n};",
)
storage_test.write_text(
    storage_test.read_text().rstrip()
    + r'''

#[test]
fn orphan_admission_height_sidecar_survives_restart_and_filters_stale_txids() {
    let path = temp_db_path("resource-orphan-sidecar");
    {
        let storage = Storage::open(&path).unwrap();
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

    let storage = Storage::open(&path).unwrap();
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
        let storage = Storage::open(&path).unwrap();
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
'''
    + "\n"
)

# Final docs: clarify that 1440 applies to both pools and that missing historical
# orphan ages are seeded at current best_height without changing the frozen
# resource-policy identity.
doc = Path("docs/MEMPOOL_POLICY_V3.md")
text = doc.read_text()
needle = "- live transaction maximum age: `1440` accepted-height steps."
if needle not in text:
    raise SystemExit("resource docs marker missing")
text = text.replace(
    needle,
    needle + "\n- orphan transaction maximum age: the same `1440` accepted-height steps.",
    1,
)
text = text.replace(
    "Missing legacy age metadata, future admission heights and checked-add overflow retain fail-safe rather than inventing age.",
    "Missing legacy live age metadata, future admission heights and checked-add overflow retain fail-safe rather than inventing live age. Historical orphan entries have no old age field, so their missing orphan age is seeded once at the current best height and they can expire only after a full future 1440-height window.",
    1,
)
doc.write_text(text)
