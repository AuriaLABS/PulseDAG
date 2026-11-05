//! TEST ONLY: isolated RocksDB generation consistency across a native-v3
//! checkpoint-shaped, two-frame diagnostic. It does not derive native GHOSTDAG,
//! authorize pruning, validate monetary state roots, or accept peer checkpoints.
//! Both external witnesses and frames are fabricated by this disposable test.
//! A real launch needs independently authenticated policy/finality/monetary
//! commitments and full-history equivalence (#1309, #1367, #781).

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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CHAIN: &str = "TEST-ONLY-v3-checkpoint-generation-never-authoritative";
const GENESIS_NS: u64 = 1_800_000_090_000_000_000;
const FIRST_KEY: &[u8] = b"__TEST_ONLY_V3_GENERATION_FRAME_01";
const SECOND_KEY: &[u8] = b"__TEST_ONLY_V3_GENERATION_FRAME_02";
const MANIFEST_KEY: &[u8] = b"__TEST_ONLY_V3_GENERATION_MANIFEST";
const FRAMES_DOMAIN: &[u8] = b"PulseDAG:TEST-ONLY:v3-generation-frames:v1";

struct DisposableDb(PathBuf);

impl DisposableDb {
    fn new(label: &str) -> Self {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "pulsedag-v3-generation-test-{label}-{}-{time}",
            std::process::id()
        )))
    }

    fn open(&self) -> DB {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        DB::open(&opts, &self.0).unwrap()
    }
}

impl Drop for DisposableDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateManifest {
    schema_version: u8,
    generation: u64,
    chain_id: String,
    frozen_genesis_ns: u64,
    genesis_hash: String,
    tip_hash: String,
    frames_digest: String,
}

struct ExternalTestWitness {
    generation: u64,
    genesis_hash: String,
    tip_hash: String,
    manifest_sha256: String,
}

struct PrivateBatch {
    frames: [Vec<u8>; 2],
    manifest: Vec<u8>,
    witness: ExternalTestWitness,
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn frame_digest(frames: &[Vec<u8>; 2]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(FRAMES_DOMAIN);
    for frame in frames {
        hasher.update((frame.len() as u64).to_le_bytes());
        hasher.update(frame);
    }
    hex::encode(hasher.finalize())
}

fn mine(parent: &BlockEnvelopeV3, timestamp_ns: u64) -> BlockEnvelopeV3 {
    let height = parent.header.height + 1;
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.hash.clone()],
        timestamp_ns,
        difficulty: parent.header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: "88".repeat(32), // Deliberately NOT a verified UTXO root.
        blue_score: height,          // NOT derived native-v3 blue score.
        height,
    };
    for nonce in 0..4096_u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        }
    }
    panic!("failed to mine easy-target test-only native-v3 diagnostic");
}

fn create_batch(
    generation: u64,
    frozen_genesis_hash: &str,
    first: &BlockEnvelopeV3,
    second: &BlockEnvelopeV3,
) -> PrivateBatch {
    let frames = [first.clone(), second.clone()].map(|envelope| {
        NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
            envelope,
            CHAIN,
            GENESIS_NS,
            frozen_genesis_hash,
        )
        .unwrap()
        .encode_verified(CHAIN, GENESIS_NS, frozen_genesis_hash)
        .unwrap()
    });
    let manifest = serde_json::to_vec(&PrivateManifest {
        schema_version: 1,
        generation,
        chain_id: CHAIN.to_owned(),
        frozen_genesis_ns: GENESIS_NS,
        genesis_hash: frozen_genesis_hash.to_owned(),
        tip_hash: second.hash.clone(),
        frames_digest: frame_digest(&frames),
    })
    .unwrap();
    let witness = ExternalTestWitness {
        generation,
        genesis_hash: frozen_genesis_hash.to_owned(),
        tip_hash: second.hash.clone(),
        manifest_sha256: digest(&manifest),
    };
    PrivateBatch {
        frames,
        manifest,
        witness,
    }
}

fn write_atomically(db: &DB, batch: &PrivateBatch) {
    let mut changes = WriteBatch::default();
    changes.put(FIRST_KEY, &batch.frames[0]);
    changes.put(SECOND_KEY, &batch.frames[1]);
    changes.put(MANIFEST_KEY, &batch.manifest);
    let mut opts = WriteOptions::default();
    opts.set_sync(true);
    db.write_opt(changes, &opts).unwrap();
}

fn verify_after_reopen(db: &DB, expected: &ExternalTestWitness) -> Result<(), String> {
    // A self-declared manifest/signature is NEVER a consensus trust anchor:
    // the digest and generation below are supplied by this independent test.
    let independently_frozen = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS)
        .map_err(|err| format!("frozen diagnostic genesis: {err}"))?;
    if expected.genesis_hash != independently_frozen.hash {
        return Err("independent genesis identity mismatch".to_owned());
    }
    let manifest_raw = db
        .get(MANIFEST_KEY)
        .map_err(|err| format!("manifest read: {err}"))?
        .ok_or_else(|| "missing private manifest".to_owned())?;
    if digest(&manifest_raw) != expected.manifest_sha256 {
        return Err("manifest differs from externally pinned test witness".to_owned());
    }
    let manifest: PrivateManifest =
        serde_json::from_slice(&manifest_raw).map_err(|err| format!("manifest: {err}"))?;
    if manifest.schema_version != 1
        || manifest.generation != expected.generation
        || manifest.chain_id != CHAIN
        || manifest.frozen_genesis_ns != GENESIS_NS
        || manifest.genesis_hash != independently_frozen.hash
        || manifest.tip_hash != expected.tip_hash
    {
        return Err("manifest generation or network mismatch".to_owned());
    }

    let frames = [FIRST_KEY, SECOND_KEY].map(|key| {
        db.get(key)
            .map_err(|err| format!("frame read: {err}"))?
            .ok_or_else(|| "missing checkpoint-shaped frame".to_owned())
    });
    let first = frames[0].as_ref().map_err(Clone::clone)?;
    let second = frames[1].as_ref().map_err(Clone::clone)?;
    let bytes = [first.clone(), second.clone()];
    if frame_digest(&bytes) != manifest.frames_digest {
        return Err("mixed or tampered frame generation".to_owned());
    }
    let records: Vec<_> = bytes
        .iter()
        .map(|raw| {
            NativeV3EnvelopeRestoreRecordV1::decode_verified(
                raw,
                CHAIN,
                GENESIS_NS,
                &independently_frozen.hash,
            )
            .map_err(|err| format!("strict frame validation: {err}"))
        })
        .collect::<Result<_, _>>()?;
    let checked = verify_dormant_v3_genesis_anchored_linear_segment(
        &records,
        CHAIN,
        GENESIS_NS,
        &independently_frozen.hash,
    )
    .map_err(|err| format!("genesis-anchored linear closure: {err}"))?;
    if checked.last_hash != expected.tip_hash || checked.linkage_checked_records != 2 {
        return Err("authenticated test-only tip mismatch".to_owned());
    }
    Ok(())
}

#[test]
fn atomic_generation_switch_requires_fresh_independent_witness_after_reopen() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine(&genesis, GENESIS_NS + 500_000_000);
    let second_a = mine(&first, GENESIS_NS + 1_000_000_000);
    let second_b = mine(&first, GENESIS_NS + 1_200_000_000);
    assert_ne!(second_a.hash, second_b.hash);

    let a = create_batch(1, &genesis.hash, &first, &second_a);
    let b = create_batch(2, &genesis.hash, &first, &second_b);
    let disk = DisposableDb::new("atomic-rollforward-rollback");
    {
        let db = disk.open();
        write_atomically(&db, &a);
    }
    {
        let db = disk.open();
        verify_after_reopen(&db, &a.witness).unwrap();

        // Deliberately write only one B frame, simulating a broken application
        // outside an atomic manifest+frames batch. It must never be accepted.
        db.put(SECOND_KEY, &b.frames[1]).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(verify_after_reopen(&db, &a.witness).is_err());
        assert!(verify_after_reopen(&db, &b.witness).is_err());
        // A true synced batch restores a single coherent B generation.
        write_atomically(&db, &b);
    }
    {
        let db = disk.open();
        verify_after_reopen(&db, &b.witness).unwrap();
        assert!(verify_after_reopen(&db, &a.witness).is_err());
        // Replacing the database with a formerly valid generation is still
        // a rollback unless the external up-to-date witness changes.
        write_atomically(&db, &a);
    }
    {
        let db = disk.open();
        assert!(verify_after_reopen(&db, &b.witness).is_err());
    }
}

#[test]
fn unknown_manifest_or_missing_private_frame_never_passes_reopen() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine(&genesis, GENESIS_NS + 500_000_000);
    let second = mine(&first, GENESIS_NS + 1_000_000_000);
    let a = create_batch(1, &genesis.hash, &first, &second);
    let disk = DisposableDb::new("missing-frame-or-manifest");
    {
        let db = disk.open();
        write_atomically(&db, &a);
        db.delete(FIRST_KEY).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(verify_after_reopen(&db, &a.witness).is_err());
        write_atomically(&db, &a);
        db.delete(MANIFEST_KEY).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(verify_after_reopen(&db, &a.witness).is_err());
    }
}
