//! TEST ONLY: externally pinned checkpoint signer and frozen-genesis boundary.
//!
//! A signed self-declared manifest is not an authenticated v3 checkpoint:
//! without an externally pinned verification key and independently trusted
//! tip/genesis, even a valid signature proves no consensus or finality.
//! No runtime keys or admission paths use these disposable RocksDB fixtures.

use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use pulsedag_core::{
    block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    state::UtxoState,
    types::compute_merkle_root,
};
use pulsedag_storage::NativeV3EnvelopeRestoreRecordV1;
use rocksdb::{Options, WriteBatch, WriteOptions, DB};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CHAIN: &str = "native-v3-TEST-ONLY-externally-pinned-checkpoint";
const GENESIS_NS: u64 = 1_800_000_041_000_000_000;
const MANIFEST_KEY: &[u8] = b"__TEST_ONLY_V3_PINNED_CHECKPOINT_MANIFEST";
const SIGNATURE_KEY: &[u8] = b"__TEST_ONLY_V3_PINNED_CHECKPOINT_SIGNATURE";
const SELF_CLAIMED_PUBKEY: &[u8] = b"__TEST_ONLY_V3_UNTRUSTED_SIGNER_ID";
const DOMAIN: &[u8] = b"PulseDAG:TEST-ONLY:v3-checkpoint-signature:v1";
const FRAMES_DOMAIN: &[u8] = b"PulseDAG:TEST-ONLY:v3-retained-frames:v1";

struct DisposableDb(PathBuf);

impl DisposableDb {
    fn new() -> Self {
        let ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        Self(std::env::temp_dir().join(format!(
            "pulsedag-test-only-pinned-v3-{}-{ns}",
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

fn frame_key(slot: usize) -> Vec<u8> {
    format!("__TEST_ONLY_V3_PINNED_FRAME_{slot}").into_bytes()
}

/// This is an informational, signature-bound test manifest, NOT a production
/// consensus state or remotely acceptable checkpoint schema.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrialManifest {
    schema_version: u32,
    chain_id: String,
    frozen_genesis_ns: u64,
    genesis_hash: String,
    externally_pinned_tip: String,
    retained_block_hashes: Vec<String>,
    retained_frame_digest: String,
    claimed_empty_v3_utxo_root: String,
}

fn sign_message(raw_manifest: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(DOMAIN);
    h.update((raw_manifest.len() as u64).to_le_bytes());
    h.update(raw_manifest);
    h.finalize().to_vec()
}

fn digest_frames(encoded_frames: &[Vec<u8>]) -> String {
    let mut h = Sha256::new();
    h.update(FRAMES_DOMAIN);
    h.update((encoded_frames.len() as u64).to_le_bytes());
    for raw in encoded_frames {
        h.update((raw.len() as u64).to_le_bytes());
        h.update(raw);
    }
    hex::encode(h.finalize())
}

fn mine_child(parent: &BlockEnvelopeV3, height: u64) -> BlockEnvelopeV3 {
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents: vec![parent.hash.clone()],
        timestamp_ns: GENESIS_NS + height * 500_000_000,
        difficulty: parent.header.difficulty,
        nonce: 0,
        merkle_root: compute_merkle_root(&[]),
        state_root: "ab".repeat(32), // Not a real economic commitment.
        blue_score: height,
        height,
    };
    for nonce in 0..4096u64 {
        header.nonce = nonce;
        if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
            return build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        }
    }
    panic!("test-only easy PoW nonce budget exhausted");
}

/// Pin comes from trusted test configuration, NEVER from the stored manifest,
/// stored advertised pubkey or a key supplied by an untrusted peer.
/// Without this pin there is deliberately no best-effort decode or fallback.
fn read_with_external_trust(
    db: &DB,
    externally_pinned_key: Option<&VerifyingKey>,
    trusted_genesis_hash: &str,
    trusted_tip_hash: &str,
) -> Result<(), String> {
    let pinned = externally_pinned_key.ok_or("no independently trusted signer key")?;
    let raw_manifest = db
        .get(MANIFEST_KEY)
        .map_err(|e| e.to_string())?
        .ok_or("missing test-only manifest")?;
    let raw_sig = db
        .get(SIGNATURE_KEY)
        .map_err(|e| e.to_string())?
        .ok_or("missing test-only signature")?;
    let bytes: [u8; 64] = raw_sig
        .as_slice()
        .try_into()
        .map_err(|_| "invalid pinned signature length")?;
    pinned
        .verify(&sign_message(&raw_manifest), &Signature::from_bytes(&bytes))
        .map_err(|_| "signature not issued by externally pinned test signer")?;

    let manifest: TrialManifest =
        serde_json::from_slice(&raw_manifest).map_err(|e| e.to_string())?;
    let independently_rebuilt_genesis =
        dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).map_err(|e| e.to_string())?;
    if manifest.schema_version != 1
        || manifest.chain_id != CHAIN
        || manifest.frozen_genesis_ns != GENESIS_NS
        || manifest.genesis_hash != trusted_genesis_hash
        || independently_rebuilt_genesis.hash != trusted_genesis_hash
        || manifest.externally_pinned_tip != trusted_tip_hash
        || manifest.retained_block_hashes.is_empty()
        || manifest.retained_block_hashes.len() > 2
        || manifest.retained_block_hashes.last().map(String::as_str) != Some(trusted_tip_hash)
        || manifest.claimed_empty_v3_utxo_root
            != UtxoState::default()
                .compute_state_root()
                .map_err(|e| e.to_string())?
    {
        return Err("manifest conflicts with externally trusted test identity".into());
    }

    let mut encoded = Vec::new();
    let mut prior_hash = trusted_genesis_hash.to_owned();
    for slot in 0..manifest.retained_block_hashes.len() {
        let raw = db
            .get(frame_key(slot))
            .map_err(|e| e.to_string())?
            .ok_or("signed retained envelope missing")?;
        let verified = NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &raw,
            CHAIN,
            GENESIS_NS,
            trusted_genesis_hash,
        )
        .map_err(|e| e.to_string())?;
        let block = verified.envelope();
        if block.hash != manifest.retained_block_hashes[slot]
            || block.header.parents != [prior_hash]
            || block.header.height != slot as u64 + 1
            || !block.transactions.is_empty()
        {
            return Err("retained PoW envelope identity/linkage mismatch".into());
        }
        prior_hash = block.hash.clone();
        encoded.push(raw);
    }
    if digest_frames(&encoded) != manifest.retained_frame_digest {
        return Err("retained frame bytes differ from pinned signature commitment".into());
    }
    Ok(())
}

#[test]
fn only_independently_pinned_signer_can_approve_tiny_trial_manifest_after_rocksdb_reopen() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine_child(&genesis, 1);
    let second = mine_child(&first, 2);
    let frames: Vec<Vec<u8>> = [&first, &second]
        .iter()
        .map(|envelope| {
            let record = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                (*envelope).clone(),
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
            )
            .unwrap();
            record
                .encode_verified(CHAIN, GENESIS_NS, &genesis.hash)
                .unwrap()
        })
        .collect();
    let manifest = TrialManifest {
        schema_version: 1,
        chain_id: CHAIN.to_owned(),
        frozen_genesis_ns: GENESIS_NS,
        genesis_hash: genesis.hash.clone(),
        externally_pinned_tip: second.hash.clone(),
        retained_block_hashes: vec![first.hash.clone(), second.hash.clone()],
        retained_frame_digest: digest_frames(&frames),
        claimed_empty_v3_utxo_root: UtxoState::default().compute_state_root().unwrap(),
    };
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let trusted_signing_key = SigningKey::from_bytes(&[73; 32]);
    let pinned_key = trusted_signing_key.verifying_key();
    let trusted_sig = trusted_signing_key.sign(&sign_message(&bytes));
    let self_appointed_key = SigningKey::from_bytes(&[74; 32]);
    let disk = DisposableDb::new();

    {
        let db = disk.open();
        let mut batch = WriteBatch::default();
        batch.put(frame_key(0), &frames[0]);
        batch.put(frame_key(1), &frames[1]);
        batch.put(MANIFEST_KEY, &bytes);
        batch.put(SIGNATURE_KEY, trusted_sig.to_bytes());
        // Untrusted self-asserted pubkey is intentionally ignored by reader.
        batch.put(
            SELF_CLAIMED_PUBKEY,
            self_appointed_key.verifying_key().to_bytes(),
        );
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(batch, &sync).unwrap();

        let mut abandoned = WriteBatch::default();
        abandoned.delete(frame_key(0));
        abandoned.delete(MANIFEST_KEY);
        drop(abandoned);
    }
    {
        let db = disk.open();
        assert!(
            read_with_external_trust(&db, None, &genesis.hash, &second.hash).is_err(),
            "self-declared public key must never bootstrap trust"
        );
        read_with_external_trust(&db, Some(&pinned_key), &genesis.hash, &second.hash).unwrap();
        assert!(
            read_with_external_trust(&db, Some(&pinned_key), &genesis.hash, &first.hash).is_err(),
            "a signed but unexpected tip cannot be accepted without trusted identity"
        );
        assert!(
            read_with_external_trust(&db, Some(&pinned_key), &"00".repeat(32), &second.hash,)
                .is_err(),
            "frozen genesis mismatch must fail"
        );
    }

    // An attacker controls the stored manifest and its advertised signer and
    // can make a mathematically VALID new signature with their own key. That
    // signature must still fail when the external pinned trust key is used.
    let mut counterfeit = manifest.clone();
    counterfeit.claimed_empty_v3_utxo_root = "ff".repeat(32);
    let counterfeit_bytes = serde_json::to_vec(&counterfeit).unwrap();
    let counterfeit_sig = self_appointed_key.sign(&sign_message(&counterfeit_bytes));
    {
        let db = disk.open();
        db.put(MANIFEST_KEY, &counterfeit_bytes).unwrap();
        db.put(SIGNATURE_KEY, counterfeit_sig.to_bytes()).unwrap();
        db.put(
            SELF_CLAIMED_PUBKEY,
            self_appointed_key.verifying_key().to_bytes(),
        )
        .unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_with_external_trust(&db, Some(&pinned_key), &genesis.hash, &second.hash).is_err(),
            "valid self-issued signature cannot replace externally pinned authority"
        );
        let mut batch = WriteBatch::default();
        batch.put(MANIFEST_KEY, &bytes);
        batch.put(SIGNATURE_KEY, trusted_sig.to_bytes());
        db.write(batch).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        read_with_external_trust(&db, Some(&pinned_key), &genesis.hash, &second.hash).unwrap();
        db.put(frame_key(1), &frames[0]).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_with_external_trust(&db, Some(&pinned_key), &genesis.hash, &second.hash).is_err(),
            "legitimate signature cannot bless substituted persisted PoW frame"
        );
        db.put(frame_key(1), &frames[1]).unwrap();
        db.delete(frame_key(0)).unwrap();
        db.flush_wal(true).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_with_external_trust(&db, Some(&pinned_key), &genesis.hash, &second.hash).is_err(),
            "missing required signed PoW predecessor must fail closed"
        );
    }
}


/// A checkpoint signed by the REAL test fixture authority can STILL be stale.
/// Its Ed25519 signature is mathematically genuine; it becomes unacceptable
/// only because an independently trusted current-tip reference moved forward.
/// If a caller voluntarily trusts the older tip again, the signature alone
/// cannot prevent rollback. This is NOT v3 economic finality/freshness proof.
#[test]
fn genuine_signed_checkpoint_rollback_is_rejected_only_with_external_fresh_tip_pin() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let first = mine_child(&genesis, 1);
    let second = mine_child(&first, 2);
    let frames: Vec<Vec<u8>> = [&first, &second]
        .iter()
        .map(|envelope| {
            NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                (*envelope).clone(),
                CHAIN,
                GENESIS_NS,
                &genesis.hash,
            )
            .unwrap()
            .encode_verified(CHAIN, GENESIS_NS, &genesis.hash)
            .unwrap()
        })
        .collect();

    let signer = SigningKey::from_bytes(&[73; 32]);
    let pinned = signer.verifying_key();
    let old_manifest = TrialManifest {
        schema_version: 1,
        chain_id: CHAIN.to_owned(),
        frozen_genesis_ns: GENESIS_NS,
        genesis_hash: genesis.hash.clone(),
        externally_pinned_tip: first.hash.clone(),
        retained_block_hashes: vec![first.hash.clone()],
        retained_frame_digest: digest_frames(&frames[..1]),
        claimed_empty_v3_utxo_root: UtxoState::default().compute_state_root().unwrap(),
    };
    let new_manifest = TrialManifest {
        externally_pinned_tip: second.hash.clone(),
        retained_block_hashes: vec![first.hash.clone(), second.hash.clone()],
        retained_frame_digest: digest_frames(&frames),
        ..old_manifest.clone()
    };
    let old_bytes = serde_json::to_vec(&old_manifest).unwrap();
    let new_bytes = serde_json::to_vec(&new_manifest).unwrap();
    let old_sig = signer.sign(&sign_message(&old_bytes)).to_bytes();
    let new_sig = signer.sign(&sign_message(&new_bytes)).to_bytes();
    let disk = DisposableDb::new();

    {
        let db = disk.open();
        let mut batch = WriteBatch::default();
        batch.put(frame_key(0), &frames[0]);
        batch.put(MANIFEST_KEY, &old_bytes);
        batch.put(SIGNATURE_KEY, old_sig);
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(batch, &sync).unwrap();
    }
    {
        let db = disk.open();
        // A historically correct checkpoint passes against its *then-current*
        // externally trusted first tip, never against an unseen later tip.
        read_with_external_trust(&db, Some(&pinned), &genesis.hash, &first.hash).unwrap();
        assert!(
            read_with_external_trust(&db, Some(&pinned), &genesis.hash, &second.hash).is_err()
        );
        let mut batch = WriteBatch::default();
        batch.put(frame_key(1), &frames[1]);
        batch.put(MANIFEST_KEY, &new_bytes);
        batch.put(SIGNATURE_KEY, new_sig);
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(batch, &sync).unwrap();
    }
    {
        let db = disk.open();
        // External reference now advances and explicitly refuses older tips.
        read_with_external_trust(&db, Some(&pinned), &genesis.hash, &second.hash).unwrap();
        assert!(read_with_external_trust(&db, Some(&pinned), &genesis.hash, &first.hash).is_err());
        let mut batch = WriteBatch::default();
        batch.put(MANIFEST_KEY, &old_bytes);
        batch.put(SIGNATURE_KEY, old_sig);
        batch.delete(frame_key(1));
        let mut sync = WriteOptions::default();
        sync.set_sync(true);
        db.write_opt(batch, &sync).unwrap();
    }
    {
        let db = disk.open();
        assert!(
            read_with_external_trust(&db, Some(&pinned), &genesis.hash, &second.hash).is_err(),
            "a genuinely signed old checkpoint must not override a newer externally trusted tip"
        );
        // This intentionally succeeds to demonstrate that a signature and
        // genesis match ALONE provide no durable high-watermark or finality.
        read_with_external_trust(&db, Some(&pinned), &genesis.hash, &first.hash).unwrap();
    }
}
