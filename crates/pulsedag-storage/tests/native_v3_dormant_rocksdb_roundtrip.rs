//! TEST-ONLY native-v3 frame persistence and restart in an isolated RocksDB.
//!
//! This creates a FRESH disposable RocksDB with an explicitly test-private
//! key prefix; it never opens the node's production database or writes the
//! accepted-block/snapshot/activation keys. Strictly verified v3 header/Pow
//! records persist and are re-decoded after DB close/reopen. A successful
//! roundtrip is NOT an accepted GHOSTDAG block, monetary UTXO checkpoint,
//! finality certificate, authenticated prune boundary, or node-runtime restart.

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use pulsedag_core::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    types::compute_merkle_root,
};
use pulsedag_storage::{
    verify_dormant_v3_genesis_anchored_linear_segment, NativeV3EnvelopeRestoreRecordV1,
};
use rocksdb::{Options, WriteBatch, WriteOptions, DB};

const CHAIN: &str = "pulsedag-dormant-v3-rocksdb-diagnostic-NOT-PRODUCTION";
const GENESIS_NS: u64 = 1_800_000_018_000_000_000;
const FIRST_KEY: &[u8] = b"__TEST_ONLY_DORMANT_V3_FRAME_0001";
const SECOND_KEY: &[u8] = b"__TEST_ONLY_DORMANT_V3_FRAME_0002";

struct DisposableDb(PathBuf);

impl DisposableDb {
    fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "pulsedag-v3-test-only-{label}-{}-{nanos}",
            std::process::id()
        )))
    }

    fn open(&self) -> DB {
        let mut options = Options::default();
        options.create_if_missing(true);
        DB::open(&options, &self.0).unwrap()
    }
}

impl Drop for DisposableDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mine(
    parent: &BlockEnvelopeV3,
    height: u64,
    timestamp_ns: u64,
) -> BlockEnvelopeV3 {
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.hash.clone()],
        timestamp_ns,
        difficulty: parent.header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: "55".repeat(32), // Intentionally UNVERIFIED monetary root.
        blue_score: height,
        height,
    };
    for nonce in 0..4096_u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        }
    }
    panic!("easy target failed to find a test PoW nonce");
}

fn fixture() -> (String, Vec<NativeV3EnvelopeRestoreRecordV1>) {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine(&genesis, 1, GENESIS_NS + 500_000_000);
    let second = mine(&first, 2, GENESIS_NS + 1_000_000_000);
    let records = [first, second]
        .into_iter()
        .map(|block| {
            NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                block,
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
            )
            .unwrap()
        })
        .collect();
    (genesis.hash, records)
}

fn persist_test_only_frames(db: &DB, genesis: &str, records: &[NativeV3EnvelopeRestoreRecordV1]) {
    let mut batch = WriteBatch::default();
    batch.put(
        FIRST_KEY,
        records[0].encode_verified(CHAIN, GENESIS_NS, genesis).unwrap(),
    );
    batch.put(
        SECOND_KEY,
        records[1].encode_verified(CHAIN, GENESIS_NS, genesis).unwrap(),
    );
    let mut write_options = WriteOptions::default();
    write_options.set_sync(true); // Crash-durable WAL for this isolated test.
    db.write_opt(batch, &write_options).unwrap();
}

fn reload_test_only_frames(
    db: &DB,
    genesis: &str,
) -> Result<Vec<NativeV3EnvelopeRestoreRecordV1>, String> {
    [FIRST_KEY, SECOND_KEY]
        .into_iter()
        .map(|key| {
            let raw = db
                .get(key)
                .map_err(|error| format!("temporary test DB read failed: {error}"))?
                .ok_or_else(|| "required test frame is missing".to_owned())?;
            NativeV3EnvelopeRestoreRecordV1::decode_verified(
                &raw,
                CHAIN,
                GENESIS_NS,
                genesis,
            )
            .map_err(|error| format!("strict persisted frame revalidation: {error}"))
        })
        .collect()
}

#[test]
fn real_rocksdb_restart_reverifies_two_pow_valid_native_v3_frames_without_activation() {
    let disk = DisposableDb::new("reopen-success");
    let (genesis, records) = fixture();
    let expected_tip = records[1].envelope().hash.clone();

    {
        let db = disk.open();
        persist_test_only_frames(&db, &genesis, &records);
        assert_eq!(db.get(FIRST_KEY).unwrap().is_some(), true);
    } // Drop real RocksDB instance; no in-memory cache carries frames forward.

    {
        let restarted = disk.open();
        let verified = reload_test_only_frames(&restarted, &genesis).unwrap();
        let outcome = verify_dormant_v3_genesis_anchored_linear_segment(
            &verified, CHAIN, GENESIS_NS, &genesis,
        )
        .unwrap();
        assert_eq!(outcome.linkage_checked_records, 2);
        assert_eq!(outcome.last_hash, expected_tip);
        assert_eq!(outcome.last_height, 2);
        assert_eq!(outcome.last_timestamp_ns, GENESIS_NS + 1_000_000_000);

        // Never infer an authoritative UTXO state from these fake roots.
        assert_eq!(verified[0].envelope().header.state_root, "55".repeat(32));
        assert_eq!(verified[1].envelope().header.state_root, "55".repeat(32));
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &restarted.get(FIRST_KEY).unwrap().unwrap(),
            "a-foreign-network",
            GENESIS_NS,
            &genesis,
        )
        .is_err());
    }
}

#[test]
fn persisted_corruption_or_missing_frame_is_rejected_after_actual_db_reopen() {
    let disk = DisposableDb::new("reopen-fail-closed");
    let (genesis, records) = fixture();
    {
        let db = disk.open();
        persist_test_only_frames(&db, &genesis, &records);
        // Corrupt the second stored JSON *without* changing its committed hash.
        let raw = db.get(SECOND_KEY).unwrap().unwrap();
        let mut untrusted: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        untrusted["envelope"]["header"]["state_root"] = serde_json::json!("66".repeat(32));
        db.put(SECOND_KEY, serde_json::to_vec(&untrusted).unwrap()).unwrap();
        db.flush_wal(true).unwrap();
    }

    {
        let restarted = disk.open();
        assert!(
            reload_test_only_frames(&restarted, &genesis).is_err(),
            "persisted tampered state-root/header hash MUST fail before restore"
        );
    }
    {
        let db = disk.open();
        // Repair only that corrupted frame, then physically remove the first.
        db.put(
            SECOND_KEY,
            records[1].encode_verified(CHAIN, GENESIS_NS, &genesis).unwrap(),
        )
        .unwrap();
        db.delete(FIRST_KEY).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let restarted = disk.open();
        assert!(
            reload_test_only_frames(&restarted, &genesis).is_err(),
            "missing genesis-linked predecessor MUST fail closed after reopen"
        );
        // Individually verified frames cannot substitute for full linear
        // parent-closure validation.
        let surviving = restarted.get(SECOND_KEY).unwrap().unwrap();
        let valid_individual_frame = NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &surviving, CHAIN, GENESIS_NS, &genesis,
        )
        .unwrap();
        assert!(
            verify_dormant_v3_genesis_anchored_linear_segment(
                &[valid_individual_frame], CHAIN, GENESIS_NS, &genesis,
            )
            .is_err(),
            "valid KHeavyHash for an orphan frame does NOT authorize missing ancestry"
        );
    }
}
