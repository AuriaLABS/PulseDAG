//! DORMANT native-v3 accepted-only RocksDB reader, separate from legacy
//! ChainState<types::Block> and its accepted/pruned columns.
//!
//! Opening this database cannot write accepted blocks. The independent GD-08
//! authority and atomic writer DO NOT exist yet. All write helpers below are
//! gated to unit tests and can only establish *test* membership, not real
//! economic acceptance. This module must never activate a GetBlock route.

use bincode::Options as _;
use rocksdb::{Options, DB};
use serde::{Deserialize, Serialize};

use pulsedag_core::genesis_v3::dormant_native_genesis_envelope_v3_ns;

use crate::{NativeV3EnvelopeRestoreRecordV1, MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES};

const CF_BODIES: &str = "native_v3_accepted_bodies";
const CF_MEMBERSHIP: &str = "native_v3_accepted_membership";
const CF_JOURNAL: &str = "native_v3_consensus_journal";
const CF_META: &str = "native_v3_meta";
const GENERATION_KEY: &[u8] = b"accepted_generation";
const IDENTITY_KEY: &[u8] = b"frozen_native_identity";
const MAX_IDENTITY_BYTES: usize = 1024;
const JOURNAL_SCHEMA: u32 = 1;
const MAX_ROW_BYTES: usize = MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES + 32768;
const MAX_JOURNAL_BYTES: usize = 8192;

fn native_row_codec() -> impl bincode::Options {
    bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .reject_trailing_bytes()
        .with_limit(MAX_ROW_BYTES as u64)
}

/// Immutable database-wide domain seal, separate from individual body rows.
/// The eventual authoritative writer must create it with its initial state.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedNativeIdentityV1 {
    schema: u32,
    chain_id: String,
    genesis_timestamp_ns: u64,
    genesis_hash: String,
}

#[derive(Debug, Clone)]
pub struct NativeV3AcceptedDiskRowV1 {
    pub frame: NativeV3EnvelopeRestoreRecordV1,
    pub chain_id: String,
    pub genesis_hash: String,
    pub committed_generation: u64,
    pub committed_state_root: String,
    pub committed_gd08_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeV3IndependentJournalV1 {
    pub accepted_generation: u64,
    pub monetary_state_root: String,
    pub gd08_order_digest: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedNativeRowV1 {
    schema: u32,
    chain_id: String,
    genesis_hash: String,
    committed_generation: u64,
    committed_state_root: String,
    committed_gd08_digest: String,
    full_native_v3_frame: Vec<u8>,
}

pub struct NativeV3AcceptedRocksDbReadOnlyV1 {
    db: DB,
    chain_id: String,
    genesis_timestamp_ns: u64,
    frozen_genesis_hash: String,
}

fn canonical_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn positive_generation(bytes: &[u8]) -> Result<u64, String> {
    let arr: [u8; 8] = bytes
        .try_into()
        .map_err(|_| "native accepted generation is not an 8-byte integer".to_string())?;
    let result = u64::from_le_bytes(arr);
    if result == 0 {
        return Err("uninitialized or zero native accepted generation".to_string());
    }
    Ok(result)
}

impl NativeV3AcceptedRocksDbReadOnlyV1 {
    /// Opens a separate, already created native-v3 journal in read-only mode.
    /// NO create-if-missing, NO compatibility search, NO writer or public
    /// service registration. The caller supplies independently frozen identity.
    pub fn open(
        path: &str,
        chain_id: &str,
        genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, String> {
        if chain_id.is_empty() || chain_id.len() > 96 || !canonical_hash(frozen_genesis_hash) {
            return Err("invalid frozen native-v3 identity".into());
        }
        let genesis = dormant_native_genesis_envelope_v3_ns(chain_id, genesis_timestamp_ns)
            .map_err(|e| format!("invalid native-v3 frozen genesis: {e}"))?;
        if genesis.hash != frozen_genesis_hash {
            return Err("native-v3 genesis hash mismatch".into());
        }
        let opts = Options::default();
        let db = DB::open_cf_for_read_only(
            &opts,
            path,
            [CF_BODIES, CF_MEMBERSHIP, CF_JOURNAL, CF_META],
            false,
        )
        .map_err(|e| format!("open separate native-v3 accepted journal read-only: {e}"))?;
        let reader = Self {
            db,
            chain_id: chain_id.to_owned(),
            genesis_timestamp_ns,
            frozen_genesis_hash: frozen_genesis_hash.to_owned(),
        };
        reader.verify_frozen_identity()?;
        reader.current_generation()?;
        Ok(reader)
    }

    fn checked_get(
        &self,
        cf_name: &str,
        key: &[u8],
        limit: usize,
    ) -> Result<Option<Vec<u8>>, String> {
        let cf = self
            .db
            .cf_handle(cf_name)
            .ok_or_else(|| format!("missing {cf_name}"))?;
        let value = self
            .db
            .get_cf(cf, key)
            .map_err(|e| format!("read {cf_name}: {e}"))?;
        if let Some(bytes) = &value {
            if bytes.is_empty() || bytes.len() > limit {
                return Err(format!("{cf_name}: empty or oversized stored value"));
            }
        }
        Ok(value)
    }

    fn verify_frozen_identity(&self) -> Result<(), String> {
        let raw = self
            .checked_get(CF_META, IDENTITY_KEY, MAX_IDENTITY_BYTES)?
            .ok_or_else(|| "native accepted database identity missing".to_string())?;
        let identity: PersistedNativeIdentityV1 = serde_json::from_slice(&raw)
            .map_err(|e| format!("invalid native accepted database identity: {e}"))?;
        if identity.schema != JOURNAL_SCHEMA
            || identity.chain_id != self.chain_id
            || identity.genesis_timestamp_ns != self.genesis_timestamp_ns
            || identity.genesis_hash != self.frozen_genesis_hash
        {
            return Err("foreign native accepted database identity".into());
        }
        Ok(())
    }

    pub fn current_generation(&self) -> Result<u64, String> {
        let value = self
            .checked_get(CF_META, GENERATION_KEY, 8)?
            .ok_or_else(|| "native accepted generation missing".to_string())?;
        positive_generation(&value)
    }

    pub fn read_accepted(&self, hash: &str) -> Result<Option<NativeV3AcceptedDiskRowV1>, String> {
        if !canonical_hash(hash) {
            return Err("invalid requested native-v3 hash".into());
        }
        let Some(raw) = self.checked_get(CF_BODIES, hash.as_bytes(), MAX_ROW_BYTES)? else {
            return Ok(None);
        };
        let row: PersistedNativeRowV1 = native_row_codec()
            .deserialize(&raw)
            .map_err(|e| format!("strict bounded native body row: {e}"))?;
        if row.schema != JOURNAL_SCHEMA
            || row.chain_id != self.chain_id
            || row.genesis_hash != self.frozen_genesis_hash
            || row.committed_generation == 0
        {
            return Err("foreign, invalid or uncommitted native body row".into());
        }
        let frame = NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &row.full_native_v3_frame,
            &self.chain_id,
            self.genesis_timestamp_ns,
            &self.frozen_genesis_hash,
        )
        .map_err(|e| format!("invalid full native-v3 frame: {e}"))?;
        if frame.envelope().hash != hash {
            return Err("requested hash not bound to native body record".into());
        }
        Ok(Some(NativeV3AcceptedDiskRowV1 {
            frame,
            chain_id: row.chain_id,
            genesis_hash: row.genesis_hash,
            committed_generation: row.committed_generation,
            committed_state_root: row.committed_state_root,
            committed_gd08_digest: row.committed_gd08_digest,
        }))
    }

    pub fn has_accepted_membership(
        &self,
        hash: &str,
        snapshot_generation: u64,
    ) -> Result<Option<u64>, String> {
        if !canonical_hash(hash) || snapshot_generation == 0 {
            return Err("invalid native-v3 accepted membership lookup".into());
        }
        let Some(raw) = self.checked_get(CF_MEMBERSHIP, hash.as_bytes(), 8)? else {
            return Ok(None);
        };
        let committed = positive_generation(&raw)?;
        if committed > snapshot_generation {
            return Err("future native accepted membership generation".into());
        }
        Ok(Some(committed))
    }

    pub fn independent_journal(
        &self,
        hash: &str,
        snapshot_generation: u64,
    ) -> Result<Option<NativeV3IndependentJournalV1>, String> {
        if !canonical_hash(hash) || snapshot_generation == 0 {
            return Err("invalid independent consensus-journal lookup".into());
        }
        let Some(raw) = self.checked_get(CF_JOURNAL, hash.as_bytes(), MAX_JOURNAL_BYTES)? else {
            return Ok(None);
        };
        let entry: NativeV3IndependentJournalV1 = serde_json::from_slice(&raw)
            .map_err(|e| format!("strict independent GD08 entry: {e}"))?;
        if entry.accepted_generation == 0
            || entry.accepted_generation > snapshot_generation
            || !canonical_hash(&entry.monetary_state_root)
            || !canonical_hash(&entry.gd08_order_digest)
        {
            return Err("invalid independent GD08/UTXO/generation commitment".into());
        }
        Ok(Some(entry))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3, header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3,
        reward_settlement_v3::build_reward_claim_transaction_v3, types::compute_merkle_root,
    };
    use rocksdb::{ColumnFamilyDescriptor, WriteBatch};
    use std::time::{SystemTime, UNIX_EPOCH};

    const CHAIN: &str = "first-native-v3-readonly-journal-test";
    const NS: u64 = 1_800_000_000_500_000_000;

    fn path() -> String {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("native-v3-journal-{}-{n}", std::process::id()))
            .to_string_lossy()
            .into_owned()
    }

    fn open_test_writer(path: &str) -> DB {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        DB::open_cf_descriptors(
            &opts,
            path,
            [CF_BODIES, CF_MEMBERSHIP, CF_JOURNAL, CF_META]
                .map(|name| ColumnFamilyDescriptor::new(name, Options::default())),
        )
        .unwrap()
    }

    fn fixture() -> (String, NativeV3EnvelopeRestoreRecordV1) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let txs =
            vec![build_reward_claim_transaction_v3("pulse1nativejournal", 12, CHAIN).unwrap()];
        let mut h = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash.clone()],
            timestamp_ns: NS + 500_000_000,
            difficulty: 0x207f_ffff,
            nonce: 0,
            merkle_root: compute_merkle_root(&txs),
            state_root: "5".repeat(64),
            blue_score: 1,
            height: 1,
        };
        h.nonce = (0..4096)
            .find(|n| {
                h.nonce = *n;
                evaluate_v3_header_pow(&h, CHAIN).unwrap().pow.accepted
            })
            .unwrap();
        let body = build_block_envelope_v3(CHAIN, h, txs).unwrap();
        let frame =
            NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(body, CHAIN, NS, &genesis.hash)
                .unwrap();
        (genesis.hash, frame)
    }

    // No production API accepts or persists an economic block. This test
    // helper imitates the future consensus writer solely to exercise RocksDB
    // power-loss atomicity and strict read logic.
    #[allow(clippy::too_many_arguments)]
    fn trial_write_with_digest(
        db: &DB,
        frame: &NativeV3EnvelopeRestoreRecordV1,
        genesis: &str,
        generation: u64,
        include_membership: bool,
        include_journal: bool,
        gd08_digest: &str,
    ) {
        let hash = frame.envelope().hash.as_bytes();
        let row = PersistedNativeRowV1 {
            schema: JOURNAL_SCHEMA,
            chain_id: CHAIN.to_owned(),
            genesis_hash: genesis.to_owned(),
            committed_generation: generation,
            committed_state_root: frame.envelope().header.state_root.clone(),
            committed_gd08_digest: gd08_digest.to_owned(),
            full_native_v3_frame: frame.encode_verified(CHAIN, NS, genesis).unwrap(),
        };
        let entry = NativeV3IndependentJournalV1 {
            accepted_generation: generation,
            monetary_state_root: row.committed_state_root.clone(),
            gd08_order_digest: gd08_digest.to_owned(),
        };
        let mut batch = WriteBatch::default();
        let meta = db.cf_handle(CF_META).unwrap();
        let body_cf = db.cf_handle(CF_BODIES).unwrap();
        let membership = db.cf_handle(CF_MEMBERSHIP).unwrap();
        let journal = db.cf_handle(CF_JOURNAL).unwrap();
        if db.get_cf(meta, IDENTITY_KEY).unwrap().is_none() {
            let identity = PersistedNativeIdentityV1 {
                schema: JOURNAL_SCHEMA,
                chain_id: CHAIN.to_owned(),
                genesis_timestamp_ns: NS,
                genesis_hash: genesis.to_owned(),
            };
            batch.put_cf(meta, IDENTITY_KEY, serde_json::to_vec(&identity).unwrap());
        }
        batch.put_cf(meta, GENERATION_KEY, generation.to_le_bytes());
        batch.put_cf(body_cf, hash, native_row_codec().serialize(&row).unwrap());
        if include_membership {
            batch.put_cf(membership, hash, generation.to_le_bytes());
        }
        if include_journal {
            batch.put_cf(journal, hash, serde_json::to_vec(&entry).unwrap());
        }
        db.write(batch).unwrap();
    }

    fn trial_write(
        db: &DB,
        frame: &NativeV3EnvelopeRestoreRecordV1,
        genesis: &str,
        generation: u64,
        include_membership: bool,
        include_journal: bool,
    ) {
        trial_write_with_digest(
            db,
            frame,
            genesis,
            generation,
            include_membership,
            include_journal,
            &"a".repeat(64),
        );
    }

    #[test]
    fn atomic_native_fixture_reopens_read_only_with_all_three_independent_keys() {
        let db_path = path();
        let (genesis, frame) = fixture();
        let hash = frame.envelope().hash.clone();
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 7, true, true);
        }
        let reader =
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
        assert_eq!(reader.current_generation(), Ok(7));
        assert_eq!(
            reader
                .read_accepted(&hash)
                .unwrap()
                .unwrap()
                .frame
                .envelope()
                .hash,
            hash
        );
        assert_eq!(reader.has_accepted_membership(&hash, 7), Ok(Some(7)));
        let journal = reader.independent_journal(&hash, 7).unwrap().unwrap();
        assert_eq!(journal.gd08_order_digest, "a".repeat(64));
        assert_eq!(
            journal.monetary_state_root,
            frame.envelope().header.state_root
        );
        assert!(
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS + 1, &genesis).is_err()
        );
        drop(reader);
        let _ = std::fs::remove_dir_all(db_path);
    }

    #[test]
    fn foreign_database_is_rejected_even_with_valid_requested_genesis() {
        let db_path = path();
        let (genesis, frame) = fixture();
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 1, true, true);
        }
        let other_chain = "foreign-native-v3-journal";
        let other_genesis = dormant_native_genesis_envelope_v3_ns(other_chain, NS)
            .unwrap()
            .hash;
        assert!(
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, other_chain, NS, &other_genesis,)
                .is_err()
        );
        let other_time = NS + 1;
        let other_genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, other_time)
            .unwrap()
            .hash;
        assert!(NativeV3AcceptedRocksDbReadOnlyV1::open(
            &db_path,
            CHAIN,
            other_time,
            &other_genesis,
        )
        .is_err());
        let _ = std::fs::remove_dir_all(db_path);
    }

    #[test]
    fn missing_or_invalid_database_seal_fails_before_serving_any_hash() {
        let db_path = path();
        let (genesis, frame) = fixture();
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 1, true, true);
            db.delete_cf(db.cf_handle(CF_META).unwrap(), IDENTITY_KEY)
                .unwrap();
        }
        assert!(NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).is_err());
        for invalid in [
            b"{}".to_vec(),
            vec![b'x'; MAX_IDENTITY_BYTES + 1],
            serde_json::to_vec(&PersistedNativeIdentityV1 {
                schema: JOURNAL_SCHEMA + 1,
                chain_id: CHAIN.to_owned(),
                genesis_timestamp_ns: NS,
                genesis_hash: genesis.clone(),
            })
            .unwrap(),
        ] {
            {
                let db = open_test_writer(&db_path);
                db.put_cf(db.cf_handle(CF_META).unwrap(), IDENTITY_KEY, invalid)
                    .unwrap();
            }
            assert!(
                NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).is_err()
            );
        }
        let _ = std::fs::remove_dir_all(db_path);
    }

    #[test]
    fn replay_derived_zero_issuance_prefix_survives_atomic_fixture_reopen() {
        use pulsedag_core::{
            native_gd08_signed_utxo_trial_v3::inspect_dormant_native_gd08_signed_prefix_with_tip_root_trial,
            state::UtxoState,
        };
        use std::collections::BTreeMap;

        let db_path = path();
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let opening = UtxoState::default();
        let empty_root = opening.compute_state_root().unwrap();
        assert_eq!(empty_root, genesis.header.state_root);
        let mut corpus = BTreeMap::from([(g.clone(), genesis.clone())]);
        let mut parent = g.clone();
        let mut expected = Vec::new();
        {
            let db = open_test_writer(&db_path);
            for height in 1..=2_u64 {
                let txs =
                    vec![
                        build_reward_claim_transaction_v3("pulse1nativejournal", height, CHAIN)
                            .unwrap(),
                    ];
                let mut header = BlockHeaderV3 {
                    version: BLOCK_HEADER_VERSION_V3,
                    parents: vec![parent.clone()],
                    timestamp_ns: NS + height * 500_000_000,
                    difficulty: 0x207f_ffff,
                    nonce: 0,
                    merkle_root: compute_merkle_root(&txs),
                    state_root: empty_root.clone(),
                    blue_score: height,
                    height,
                };
                header.nonce = (0..4096)
                    .find(|nonce| {
                        header.nonce = *nonce;
                        evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
                    })
                    .unwrap();
                let body = build_block_envelope_v3(CHAIN, header, txs).unwrap();
                parent = body.hash.clone();
                corpus.insert(parent.clone(), body.clone());
                let report = inspect_dormant_native_gd08_signed_prefix_with_tip_root_trial(
                    CHAIN, NS, &g, &parent, &corpus, 0, &opening,
                )
                .unwrap();
                assert!(report.applied_txids.is_empty());
                assert!(report
                    .eligible_fees_by_block
                    .iter()
                    .all(|(_, fee)| *fee == 0));
                assert_eq!(report.resulting_state_root, empty_root);
                let frame =
                    NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(body, CHAIN, NS, &g)
                        .unwrap();
                trial_write_with_digest(&db, &frame, &g, height, true, true, &report.order_digest);
                expected.push((parent.clone(), height, report));
            }
        }
        let reader = NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &g).unwrap();
        assert_eq!(reader.current_generation(), Ok(2));
        let mut restored = BTreeMap::from([(g.clone(), genesis)]);
        for (hash, generation, original) in expected {
            let row = reader.read_accepted(&hash).unwrap().unwrap();
            assert_eq!(
                reader.has_accepted_membership(&hash, 2),
                Ok(Some(generation))
            );
            let journal = reader.independent_journal(&hash, 2).unwrap().unwrap();
            assert_eq!(journal.accepted_generation, generation);
            assert_eq!(row.committed_generation, generation);
            restored.insert(hash.clone(), row.frame.envelope().clone());
            let replay = inspect_dormant_native_gd08_signed_prefix_with_tip_root_trial(
                CHAIN, NS, &g, &hash, &restored, 0, &opening,
            )
            .unwrap();
            assert_eq!(replay, original);
            assert_eq!(journal.gd08_order_digest, replay.order_digest);
            assert_eq!(row.committed_gd08_digest, replay.order_digest);
            assert_eq!(journal.monetary_state_root, replay.resulting_state_root);
            assert_eq!(row.committed_state_root, replay.resulting_state_root);
        }
        drop(reader);
        let _ = std::fs::remove_dir_all(db_path);
    }

    #[test]
    fn near_limit_full_native_frame_has_compact_bounded_row_round_trip() {
        // JSON Vec<u8> would expand a valid ~2 MiB block into >6 MiB.
        // The binary envelope remains byte-preserving with bounded overhead.
        let row = PersistedNativeRowV1 {
            schema: JOURNAL_SCHEMA,
            chain_id: CHAIN.into(),
            genesis_hash: "a".repeat(64),
            committed_generation: 1,
            committed_state_root: "b".repeat(64),
            committed_gd08_digest: "c".repeat(64),
            full_native_v3_frame: vec![42u8; MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES - 16],
        };
        let encoded = native_row_codec().serialize(&row).unwrap();
        assert!(encoded.len() <= MAX_ROW_BYTES);
        let decoded: PersistedNativeRowV1 = native_row_codec().deserialize(&encoded).unwrap();
        assert_eq!(decoded.full_native_v3_frame, row.full_native_v3_frame);
        assert_eq!(decoded.committed_generation, 1);
        let mut malformed = encoded;
        malformed.push(0);
        assert!(native_row_codec()
            .deserialize::<PersistedNativeRowV1>(&malformed)
            .is_err());
    }

    #[test]
    fn partial_seal_update_keeps_the_old_membership_generation_visible() {
        let db_path = path();
        let (genesis, frame) = fixture();
        let hash = frame.envelope().hash.clone();
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 4, true, true);
            // Simulate a buggy future writer updating body and independent
            // journal to generation 5 while leaving membership at 4.
            trial_write(&db, &frame, &genesis, 5, false, true);
        }
        let reader =
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
        assert_eq!(reader.current_generation(), Ok(5));
        assert_eq!(
            reader
                .read_accepted(&hash)
                .unwrap()
                .unwrap()
                .committed_generation,
            5
        );
        assert_eq!(reader.has_accepted_membership(&hash, 5), Ok(Some(4)));
        assert_eq!(
            reader
                .independent_journal(&hash, 5)
                .unwrap()
                .unwrap()
                .accepted_generation,
            5
        );
        // The daemon read adapter must reject 4 != 5 != 5; old bool
        // membership API discarded the evidence of this inconsistent seal.
        drop(reader);
        let _ = std::fs::remove_dir_all(db_path);
    }

    #[test]
    fn body_only_or_accepted_index_without_journal_never_becomes_accepted() {
        let db_path = path();
        let (genesis, frame) = fixture();
        let hash = frame.envelope().hash.clone();
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 3, false, false);
        }
        let reader =
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
        assert!(reader.read_accepted(&hash).unwrap().is_some());
        assert_eq!(reader.has_accepted_membership(&hash, 3), Ok(None));
        assert_eq!(reader.independent_journal(&hash, 3), Ok(None));
        drop(reader);
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 4, true, false);
        }
        let reader =
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
        assert_eq!(reader.has_accepted_membership(&hash, 4), Ok(Some(4)));
        assert_eq!(reader.independent_journal(&hash, 4), Ok(None));
        assert!(reader.independent_journal(&hash, 0).is_err());
        assert!(reader.has_accepted_membership(&hash, 3).is_err());
        drop(reader);
        let _ = std::fs::remove_dir_all(db_path);
    }
}
