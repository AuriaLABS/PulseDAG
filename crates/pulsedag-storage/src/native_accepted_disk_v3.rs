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

    /// Read a coherent committed row from all three independent namespaces.
    /// A missing body is only Missing if the other namespaces are also absent.
    /// This verifies stored consistency, not economic consensus or finality.
    pub fn read_consistent_accepted(
        &self,
        hash: &str,
    ) -> Result<Option<NativeV3AcceptedDiskRowV1>, String> {
        let before = self.current_generation()?;
        let body = self.read_accepted(hash)?;
        let membership = self.has_accepted_membership(hash, before)?;
        let journal = self.independent_journal(hash, before)?;
        if self.current_generation()? != before {
            return Err("native accepted generation changed during read".into());
        }
        match (body, membership, journal) {
            (None, None, None) => Ok(None),
            (Some(row), Some(generation), Some(entry)) => {
                if row.committed_generation != generation
                    || entry.accepted_generation != generation
                    || generation > before
                {
                    return Err("inconsistent native accepted generation".into());
                }
                if row.committed_state_root != row.frame.envelope().header.state_root
                    || row.committed_state_root != entry.monetary_state_root
                    || row.committed_gd08_digest != entry.gd08_order_digest
                {
                    return Err("inconsistent native accepted root or order commitment".into());
                }
                Ok(Some(row))
            }
            _ => Err("incomplete native accepted body/membership/journal".into()),
        }
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
    use rocksdb::{ColumnFamilyDescriptor, WriteBatch, WriteOptions};
    use std::time::{SystemTime, UNIX_EPOCH};

    // Keep the diagnostic entirely out of the compiled production reader.
    impl NativeV3AcceptedRocksDbReadOnlyV1 {
        /// Reconstruct and audit a bounded, complete linear accepted history from
        /// disk, not a caller-supplied corpus. The caller must independently pin
        /// the expected tip and trial cadence. This is read-only evidence, not
        /// approval of a pruned checkpoint or a production monetary policy.
        fn audit_dormant_linear_tip(
            &self,
            expected_tip: &str,
            cadence: &[pulsedag_core::monetary_v3::MonetaryCadenceSegment],
        ) -> Result<
            pulsedag_core::native_gd08_linear_monetary_trial_v3::DormantNativeLinearMonetaryReportV3,
            String,
        > {
            use pulsedag_core::native_gd08_linear_monetary_trial_v3::inspect_dormant_native_linear_monetary_trial;
            use std::collections::BTreeMap;

            const MAX_AUDIT_BLOCKS: u64 = 32;
            let generation = self.current_generation()?;
            if generation > MAX_AUDIT_BLOCKS || !canonical_hash(expected_tip) {
                return Err("unsupported bounded native linear audit tip".into());
            }
            let mut cursor = expected_tip.to_owned();
            let mut descending = Vec::new();
            for expected in (1..=generation).rev() {
                let row = self
                    .read_consistent_accepted(&cursor)?
                    .ok_or_else(|| "missing accepted ancestor in native audit".to_string())?;
                let envelope = row.frame.envelope();
                if row.committed_generation != expected
                    || envelope.header.height != expected
                    || envelope.header.parents.len() != 1
                {
                    return Err("nonlinear or noncontiguous native audit history".into());
                }
                cursor = envelope.header.parents[0].clone();
                descending.push(row);
            }
            if cursor != self.frozen_genesis_hash {
                return Err("native audit did not reach independently frozen genesis".into());
            }
            let genesis =
                dormant_native_genesis_envelope_v3_ns(&self.chain_id, self.genesis_timestamp_ns)
                    .map_err(|e| e.to_string())?;
            let mut corpus = BTreeMap::from([(genesis.hash.clone(), genesis)]);
            let mut last_report = None;
            for row in descending.into_iter().rev() {
                let envelope = row.frame.envelope();
                let hash = envelope.hash.clone();
                if corpus.insert(hash.clone(), envelope.clone()).is_some() {
                    return Err("duplicate native audit ancestor".into());
                }
                let report = inspect_dormant_native_linear_monetary_trial(
                    &self.chain_id,
                    self.genesis_timestamp_ns,
                    &self.frozen_genesis_hash,
                    &hash,
                    &corpus,
                    cadence,
                )
                .map_err(|e| e.to_string())?;
                if report.state_root != row.committed_state_root
                    || report.order_digest != row.committed_gd08_digest
                {
                    return Err("native audit replay differs from persisted commitment".into());
                }
                last_report = Some(report);
            }
            if self.current_generation()? != generation {
                return Err("native accepted generation changed during audit".into());
            }
            last_report.ok_or_else(|| "empty native accepted audit history".to_string())
        }
    }

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
    fn trial_batch_with_digest(
        db: &DB,
        frame: &NativeV3EnvelopeRestoreRecordV1,
        genesis: &str,
        generation: u64,
        include_membership: bool,
        include_journal: bool,
        gd08_digest: &str,
    ) -> WriteBatch {
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
        batch
    }

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
        db.write(trial_batch_with_digest(
            db,
            frame,
            genesis,
            generation,
            include_membership,
            include_journal,
            gd08_digest,
        ))
        .unwrap();
    }

    // Deliberately private, test-only, single-writer linear append. Never
    // expose this as accepted production authority or support fork replacement.
    fn trial_append_verified_linear(
        db: &DB,
        genesis: &str,
        tip: &String,
        corpus: &std::collections::BTreeMap<
            String,
            pulsedag_core::block_envelope_v3::BlockEnvelopeV3,
        >,
        cadence: &[pulsedag_core::monetary_v3::MonetaryCadenceSegment],
    ) -> Result<(), String> {
        use pulsedag_core::native_gd08_linear_monetary_trial_v3::inspect_dormant_native_linear_monetary_trial;

        const TIP_KEY: &[u8] = b"test_linear_tip";
        const CADENCE_KEY: &[u8] = b"test_linear_cadence";
        let report =
            inspect_dormant_native_linear_monetary_trial(CHAIN, NS, genesis, tip, corpus, cadence)
                .map_err(|e| e.to_string())?;
        let body = corpus.get(tip).ok_or("missing trial tip")?;
        let generation = report.monetary_score;
        let meta = db.cf_handle(CF_META).ok_or("missing metadata")?;
        let get = |key: &[u8]| db.get_cf(meta, key).map_err(|e| e.to_string());
        let previous = get(GENERATION_KEY)?;
        let previous_tip = get(TIP_KEY)?;
        let previous_cadence = get(CADENCE_KEY)?;
        let identity = get(IDENTITY_KEY)?;
        let expected_identity = PersistedNativeIdentityV1 {
            schema: JOURNAL_SCHEMA,
            chain_id: CHAIN.to_owned(),
            genesis_timestamp_ns: NS,
            genesis_hash: genesis.to_owned(),
        };
        let parent = body.header.parents.first().ok_or("missing trial parent")?;
        match (previous, previous_tip, previous_cadence, identity) {
            (None, None, None, None) if generation == 1 && parent == genesis => {}
            (Some(raw), Some(old_tip), Some(old_cadence), Some(raw_identity)) => {
                let old = positive_generation(&raw)?;
                let old_identity: PersistedNativeIdentityV1 =
                    serde_json::from_slice(&raw_identity).map_err(|e| e.to_string())?;
                if old.checked_add(1) != Some(generation)
                    || old_tip != parent.as_bytes()
                    || old_cadence != report.cadence_fingerprint.as_bytes()
                    || old_identity != expected_identity
                {
                    return Err("nonsequential or foreign trial append".into());
                }
                let membership = db.cf_handle(CF_MEMBERSHIP).ok_or("missing membership")?;
                let body_cf = db.cf_handle(CF_BODIES).ok_or("missing body namespace")?;
                let journal_cf = db
                    .cf_handle(CF_JOURNAL)
                    .ok_or("missing journal namespace")?;
                // Audit every persisted non-genesis ancestor, not just the
                // previous tip. The initial replay bounds and validates the
                // entire linear corpus. Replaying its decreasing prefixes is
                // deliberately expensive and remains a test-only oracle.
                let mut prefix = corpus.clone();
                prefix.remove(tip);
                let mut cursor = parent.clone();
                for expected_generation in (1..=old).rev() {
                    let previous_report = inspect_dormant_native_linear_monetary_trial(
                        CHAIN, NS, genesis, &cursor, &prefix, cadence,
                    )
                    .map_err(|e| e.to_string())?;
                    let previous_body = prefix.get(&cursor).ok_or("missing ancestor envelope")?;
                    let raw_member = db
                        .get_cf(membership, cursor.as_bytes())
                        .map_err(|e| e.to_string())?
                        .ok_or("missing ancestor membership")?;
                    let raw_body = db
                        .get_cf(body_cf, cursor.as_bytes())
                        .map_err(|e| e.to_string())?
                        .ok_or("missing ancestor body")?;
                    let raw_journal = db
                        .get_cf(journal_cf, cursor.as_bytes())
                        .map_err(|e| e.to_string())?
                        .ok_or("missing ancestor journal")?;
                    if raw_body.len() > MAX_ROW_BYTES || raw_journal.len() > MAX_JOURNAL_BYTES {
                        return Err("oversized ancestor trial record".into());
                    }
                    let row: PersistedNativeRowV1 = native_row_codec()
                        .deserialize(&raw_body)
                        .map_err(|e| e.to_string())?;
                    let entry: NativeV3IndependentJournalV1 =
                        serde_json::from_slice(&raw_journal).map_err(|e| e.to_string())?;
                    let expected_frame = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                        previous_body.clone(),
                        CHAIN,
                        NS,
                        genesis,
                    )
                    .map_err(|e| e.to_string())?
                    .encode_verified(CHAIN, NS, genesis)
                    .map_err(|e| e.to_string())?;
                    if previous_report.monetary_score != expected_generation
                        || positive_generation(&raw_member)? != expected_generation
                        || row.schema != JOURNAL_SCHEMA
                        || row.chain_id != CHAIN
                        || row.genesis_hash != genesis
                        || row.committed_generation != expected_generation
                        || row.committed_state_root != previous_report.state_root
                        || row.committed_gd08_digest != previous_report.order_digest
                        || row.full_native_v3_frame != expected_frame
                        || entry.accepted_generation != expected_generation
                        || entry.monetary_state_root != previous_report.state_root
                        || entry.gd08_order_digest != previous_report.order_digest
                    {
                        return Err("ancestor trial commitment differs from verified replay".into());
                    }
                    let next = previous_body.header.parents[0].clone();
                    prefix.remove(&cursor);
                    cursor = next;
                }
                if cursor != genesis || prefix.len() != 1 || !prefix.contains_key(genesis) {
                    return Err("ancestor trial audit did not reach frozen genesis".into());
                }
            }
            _ => return Err("incomplete or nonsequential trial metadata".into()),
        }
        for name in [CF_BODIES, CF_MEMBERSHIP, CF_JOURNAL] {
            let cf = db.cf_handle(name).ok_or("missing trial namespace")?;
            if db
                .get_cf(cf, tip.as_bytes())
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Err("trial append would overwrite an existing record".into());
            }
        }
        let frame = NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
            body.clone(),
            CHAIN,
            NS,
            genesis,
        )
        .map_err(|e| e.to_string())?;
        let mut batch = trial_batch_with_digest(
            db,
            &frame,
            genesis,
            generation,
            true,
            true,
            &report.order_digest,
        );
        batch.put_cf(meta, TIP_KEY, tip.as_bytes());
        batch.put_cf(meta, CADENCE_KEY, report.cadence_fingerprint.as_bytes());
        let mut options = WriteOptions::default();
        options.set_sync(true);
        db.write_opt(batch, &options).map_err(|e| e.to_string())
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
    fn native_linear_maturity_and_signed_spend_survive_disk_replay() {
        verify_native_linear_disk_replay(3);
    }

    #[test]
    fn native_32_block_monetary_disk_replay_survives_writer_restart() {
        verify_native_linear_disk_replay(32);
    }

    fn verify_native_linear_disk_replay(block_count: u64) {
        use ed25519_dalek::{Signer, SigningKey};
        use pulsedag_core::{
            monetary_v3::{subsidy_atoms_for_score, MonetaryCadenceSegment},
            native_gd08_linear_monetary_trial_v3::inspect_dormant_native_linear_monetary_trial,
            reward_settlement_v3::settlement_outpoint_v3,
            state::UtxoState,
            tx::{
                address_from_public_key, compute_txid_v2, signing_message_v2,
                TRANSACTION_VERSION_V2,
            },
            types::{OutPoint, Transaction, TxInput, TxOutput, Utxo},
        };
        use std::collections::BTreeMap;

        let cadence = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 3_600_000_000_000,
        }];
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let key = SigningKey::from_bytes(&[19u8; 32]);
        let public_key = hex::encode(key.verifying_key().to_bytes());
        let owner = address_from_public_key(&public_key);
        let mut state = UtxoState::default();
        let mut corpus = BTreeMap::from([(g.clone(), genesis.clone())]);
        let mut parent = g.clone();
        let mut expected = Vec::new();
        let mut previous_reward: Option<Utxo> = None;
        let mut spend = None;
        let db_path = path();
        {
            let mut db = open_test_writer(&db_path);
            for height in 1..=block_count {
                let claim = build_reward_claim_transaction_v3(&owner, height, CHAIN).unwrap();
                let mut txs = vec![claim.clone()];
                if height == 3 {
                    let coin = state.utxos.values().next().unwrap().clone();
                    let mut tx = Transaction {
                        txid: String::new(),
                        version: TRANSACTION_VERSION_V2,
                        inputs: vec![TxInput {
                            previous_output: coin.outpoint.clone(),
                            public_key: public_key.clone(),
                            signature: String::new(),
                        }],
                        outputs: vec![TxOutput {
                            address: owner.clone(),
                            amount: coin.amount - 1,
                        }],
                        fee: 1,
                        nonce: 3,
                    };
                    tx.inputs[0].signature = hex::encode(
                        key.sign(&signing_message_v2(&tx, CHAIN).unwrap())
                            .to_bytes(),
                    );
                    tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
                    state.utxos.remove(&coin.outpoint).unwrap();
                    state.address_index.clear();
                    let output = Utxo {
                        outpoint: OutPoint {
                            txid: tx.txid.clone(),
                            index: 0,
                        },
                        address: owner.clone(),
                        amount: tx.outputs[0].amount,
                        coinbase: false,
                        height,
                    };
                    state.utxos.insert(output.outpoint.clone(), output.clone());
                    state
                        .address_index
                        .insert(owner.clone(), vec![output.outpoint]);
                    spend = Some(tx.txid.clone());
                    txs.push(tx);
                }
                if let Some(coin) = previous_reward.take() {
                    state.utxos.insert(coin.outpoint.clone(), coin.clone());
                    state
                        .address_index
                        .entry(owner.clone())
                        .or_default()
                        .push(coin.outpoint);
                }
                let mut header = BlockHeaderV3 {
                    version: BLOCK_HEADER_VERSION_V3,
                    parents: vec![parent.clone()],
                    timestamp_ns: NS + height * 500_000_000,
                    difficulty: 0x207f_ffff,
                    nonce: 0,
                    merkle_root: compute_merkle_root(&txs),
                    state_root: state.compute_state_root().unwrap(),
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
                previous_reward = Some(Utxo {
                    outpoint: settlement_outpoint_v3(CHAIN, &parent, &claim.txid),
                    address: owner.clone(),
                    amount: subsidy_atoms_for_score(height, &cadence).unwrap()
                        + u64::from(height == 3),
                    coinbase: true,
                    height,
                });
                corpus.insert(parent.clone(), body.clone());
                let report = inspect_dormant_native_linear_monetary_trial(
                    CHAIN, NS, &g, &parent, &corpus, &cadence,
                )
                .unwrap();
                assert_eq!(report.state_root, state.compute_state_root().unwrap());
                assert_eq!(report.mature_rewards, (height - 1) as usize);
                assert_eq!(report.applied_transfers, usize::from(height >= 3));
                assert_eq!(
                    report.pending_reward_atoms,
                    subsidy_atoms_for_score(height, &cadence).unwrap() + u64::from(height == 3)
                );
                assert_eq!(
                    report.total_subsidy_atoms,
                    report.spendable_atoms + report.pending_reward_atoms
                );
                if height == 3 {
                    let predecessor = &corpus[&parent].header.parents[0];
                    let ancestor = &corpus[predecessor].header.parents[0];
                    let meta = db.cf_handle(CF_META).unwrap();
                    for prior in [predecessor, ancestor] {
                        for name in [CF_BODIES, CF_MEMBERSHIP, CF_JOURNAL] {
                            let cf = db.cf_handle(name).unwrap();
                            let original = db.get_cf(cf, prior.as_bytes()).unwrap().unwrap();
                            // Delete or replace each independent namespace at
                            // both the immediate tip and a deeper ancestor.
                            for missing in [true, false] {
                                if missing {
                                    db.delete_cf(cf, prior.as_bytes()).unwrap();
                                } else {
                                    db.put_cf(cf, prior.as_bytes(), b"corrupt").unwrap();
                                }
                                assert!(trial_append_verified_linear(
                                    &db, &g, &parent, &corpus, &cadence
                                )
                                .is_err());
                                assert_eq!(
                                    positive_generation(
                                        &db.get_cf(meta, GENERATION_KEY).unwrap().unwrap()
                                    ),
                                    Ok(2)
                                );
                                for new_name in [CF_BODIES, CF_MEMBERSHIP, CF_JOURNAL] {
                                    assert!(db
                                        .get_cf(db.cf_handle(new_name).unwrap(), parent.as_bytes())
                                        .unwrap()
                                        .is_none());
                                }
                                db.put_cf(cf, prior.as_bytes(), &original).unwrap();
                            }
                        }
                    }
                    // Also retain a structurally valid but incorrect digest.
                    let journal_cf = db.cf_handle(CF_JOURNAL).unwrap();
                    let original = db.get_cf(journal_cf, ancestor.as_bytes()).unwrap().unwrap();
                    let mut corrupt: NativeV3IndependentJournalV1 =
                        serde_json::from_slice(&original).unwrap();
                    corrupt.gd08_order_digest = "e".repeat(64);
                    db.put_cf(
                        journal_cf,
                        ancestor.as_bytes(),
                        serde_json::to_vec(&corrupt).unwrap(),
                    )
                    .unwrap();
                    assert!(
                        trial_append_verified_linear(&db, &g, &parent, &corpus, &cadence).is_err()
                    );
                    assert_eq!(
                        positive_generation(&db.get_cf(meta, GENERATION_KEY).unwrap().unwrap()),
                        Ok(2)
                    );
                    for name in [CF_BODIES, CF_MEMBERSHIP, CF_JOURNAL] {
                        assert!(db
                            .get_cf(db.cf_handle(name).unwrap(), parent.as_bytes())
                            .unwrap()
                            .is_none());
                    }
                    db.put_cf(journal_cf, ancestor.as_bytes(), original)
                        .unwrap();
                }
                trial_append_verified_linear(&db, &g, &parent, &corpus, &cadence).unwrap();
                assert!(trial_append_verified_linear(&db, &g, &parent, &corpus, &cadence).is_err());
                let meta = db.cf_handle(CF_META).unwrap();
                assert_eq!(
                    positive_generation(&db.get_cf(meta, GENERATION_KEY).unwrap().unwrap()),
                    Ok(height)
                );
                expected.push((parent.clone(), height, report));
                if block_count == 32 && height == 16 {
                    drop(db);
                    let reader =
                        NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &g).unwrap();
                    assert_eq!(reader.current_generation(), Ok(height));
                    let row = reader.read_consistent_accepted(&parent).unwrap().unwrap();
                    assert_eq!(row.committed_generation, height);
                    assert_eq!(
                        row.committed_state_root,
                        state.compute_state_root().unwrap()
                    );
                    drop(reader);
                    db = open_test_writer(&db_path);
                }
            }
        }

        let reader = NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &g).unwrap();
        assert_eq!(reader.current_generation(), Ok(block_count));
        let (tip, _, final_report) = expected.last().unwrap();
        assert_eq!(
            &reader.audit_dormant_linear_tip(tip, &cadence).unwrap(),
            final_report
        );
        assert!(reader.audit_dormant_linear_tip(&g, &cadence).is_err());
        assert!(reader
            .audit_dormant_linear_tip(&"f".repeat(64), &cadence)
            .is_err());
        let mut restored = BTreeMap::from([(g.clone(), genesis)]);
        for (hash, generation, original) in expected {
            let row = reader.read_consistent_accepted(&hash).unwrap().unwrap();
            let journal = reader
                .independent_journal(&hash, block_count)
                .unwrap()
                .unwrap();
            assert_eq!(
                reader.has_accepted_membership(&hash, block_count),
                Ok(Some(generation))
            );
            assert_eq!(row.committed_generation, generation);
            assert_eq!(journal.accepted_generation, generation);
            if generation == 3 {
                assert_eq!(
                    row.frame.envelope().transactions[1].txid,
                    spend.clone().unwrap()
                );
            }
            restored.insert(hash.clone(), row.frame.envelope().clone());
            let replay = inspect_dormant_native_linear_monetary_trial(
                CHAIN, NS, &g, &hash, &restored, &cadence,
            )
            .unwrap();
            assert_eq!(replay, original);
            assert_eq!(row.committed_state_root, replay.state_root);
            assert_eq!(journal.monetary_state_root, replay.state_root);
            assert_eq!(row.committed_gd08_digest, replay.order_digest);
            assert_eq!(journal.gd08_order_digest, replay.order_digest);
        }
        assert_eq!(restored.len(), block_count as usize + 1);
        drop(reader);
        // Coherently forge BOTH stored copies of an ancestor order digest.
        // Local row consistency is insufficient; full replay must reject it.
        let ancestor = restored
            .values()
            .find(|body| body.header.height == 1)
            .unwrap();
        let tip = restored
            .values()
            .find(|body| body.header.height == block_count)
            .unwrap();
        {
            let db = open_test_writer(&db_path);
            let body_cf = db.cf_handle(CF_BODIES).unwrap();
            let journal_cf = db.cf_handle(CF_JOURNAL).unwrap();
            let key = ancestor.hash.as_bytes();
            let raw = db.get_cf(body_cf, key).unwrap().unwrap();
            let mut row: PersistedNativeRowV1 = native_row_codec().deserialize(&raw).unwrap();
            let mut journal: NativeV3IndependentJournalV1 =
                serde_json::from_slice(&db.get_cf(journal_cf, key).unwrap().unwrap()).unwrap();
            let forged = "f".repeat(64);
            assert_ne!(row.committed_gd08_digest, forged);
            row.committed_gd08_digest = forged.clone();
            journal.gd08_order_digest = forged;
            let mut batch = WriteBatch::default();
            batch.put_cf(body_cf, key, native_row_codec().serialize(&row).unwrap());
            batch.put_cf(journal_cf, key, serde_json::to_vec(&journal).unwrap());
            db.write(batch).unwrap();
        }
        let reader = NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &g).unwrap();
        assert!(reader
            .read_consistent_accepted(&ancestor.hash)
            .unwrap()
            .is_some());
        assert!(reader
            .audit_dormant_linear_tip(&tip.hash, &cadence)
            .is_err());
        assert_eq!(reader.current_generation(), Ok(block_count));
        drop(reader);
        let _ = std::fs::remove_dir_all(db_path);
    }

    // Explicitly invoked by the cross-crate Noise workflow. Do not silently
    // pass when the handoff is missing; ordinary storage CI excludes this
    // external-fixture test by design.
    #[test]
    #[ignore = "requires actual receiver export from the five-peer Noise CI test"]
    fn recovered_noise_linear_bodies_survive_disk_restart_and_replay() {
        use pulsedag_core::{
            block_envelope_v3::BlockEnvelopeV3, monetary_v3::MonetaryCadenceSegment,
            native_gd08_linear_monetary_trial_v3::inspect_dormant_native_linear_monetary_trial,
        };
        use std::{collections::BTreeMap, io::Read};

        const MAX_EXPORT_BYTES: u64 = 8 * 1024 * 1024;
        let source = std::env::var_os("PULSEDAG_TEST_NOISE_RECOVERY_EXPORT")
            .expect("authenticated Noise receiver export is required");
        let mut bytes = Vec::new();
        std::fs::File::open(source)
            .unwrap()
            .take(MAX_EXPORT_BYTES + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.len() as u64 <= MAX_EXPORT_BYTES);
        let received: Vec<BlockEnvelopeV3> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(received.len(), 32);
        let cadence = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 3_600_000_000_000,
        }];
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, NS).unwrap();
        let g = genesis.hash.clone();
        let mut prefix = BTreeMap::from([(g.clone(), genesis.clone())]);
        let mut parent = g.clone();
        let mut expected = Vec::new();
        let db_path = path();
        {
            let mut db = open_test_writer(&db_path);
            for (index, body) in received.into_iter().enumerate() {
                let generation = index as u64 + 1;
                assert_eq!(body.header.parents, vec![parent.clone()]);
                assert_eq!(body.header.height, generation);
                parent = body.hash.clone();
                assert!(prefix.insert(parent.clone(), body).is_none());
                let report = inspect_dormant_native_linear_monetary_trial(
                    CHAIN, NS, &g, &parent, &prefix, &cadence,
                )
                .unwrap();
                assert_eq!(report.monetary_score, generation);
                assert_eq!(report.mature_rewards, index);
                assert_eq!(report.state_root, prefix[&parent].header.state_root);
                assert_eq!(
                    report.total_subsidy_atoms,
                    report.spendable_atoms + report.pending_reward_atoms
                );
                trial_append_verified_linear(&db, &g, &parent, &prefix, &cadence).unwrap();
                expected.push((parent.clone(), generation, report));
                if generation == 16 {
                    drop(db);
                    let reader =
                        NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &g).unwrap();
                    assert_eq!(reader.current_generation(), Ok(generation));
                    let row = reader.read_consistent_accepted(&parent).unwrap().unwrap();
                    assert_eq!(row.committed_state_root, prefix[&parent].header.state_root);
                    drop(reader);
                    db = open_test_writer(&db_path);
                }
            }
        }
        let reader = NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &g).unwrap();
        assert_eq!(reader.current_generation(), Ok(32));
        let mut restored = BTreeMap::from([(g.clone(), genesis)]);
        for (hash, generation, original) in expected {
            let row = reader.read_consistent_accepted(&hash).unwrap().unwrap();
            let journal = reader.independent_journal(&hash, 32).unwrap().unwrap();
            assert_eq!(row.committed_generation, generation);
            assert_eq!(journal.accepted_generation, generation);
            assert_eq!(
                reader.has_accepted_membership(&hash, 32),
                Ok(Some(generation))
            );
            let body = row.frame.envelope();
            assert_eq!(
                serde_json::to_vec(body).unwrap(),
                serde_json::to_vec(&prefix[&hash]).unwrap()
            );
            assert!(restored.insert(hash.clone(), body.clone()).is_none());
            let report = inspect_dormant_native_linear_monetary_trial(
                CHAIN, NS, &g, &hash, &restored, &cadence,
            )
            .unwrap();
            assert_eq!(report, original);
            assert_eq!(row.committed_state_root, report.state_root);
            assert_eq!(journal.monetary_state_root, report.state_root);
            assert_eq!(row.committed_gd08_digest, report.order_digest);
            assert_eq!(journal.gd08_order_digest, report.order_digest);
        }
        assert_eq!(restored.len(), 33);
        drop(reader);
        std::fs::remove_dir_all(db_path).unwrap();
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

    #[test]
    fn consistent_native_read_rejects_tampered_or_incomplete_seals() {
        let db_path = path();
        let (genesis, frame) = fixture();
        let hash = frame.envelope().hash.clone();
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 4, true, true);
        }
        let reader =
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
        assert!(reader.read_consistent_accepted(&hash).unwrap().is_some());
        assert!(reader
            .read_consistent_accepted(&"0".repeat(64))
            .unwrap()
            .is_none());
        drop(reader);
        for (generation, root, digest) in [
            (
                3,
                frame.envelope().header.state_root.clone(),
                "a".repeat(64),
            ),
            (4, "b".repeat(64), "a".repeat(64)),
            (
                4,
                frame.envelope().header.state_root.clone(),
                "c".repeat(64),
            ),
        ] {
            {
                let db = open_test_writer(&db_path);
                let entry = NativeV3IndependentJournalV1 {
                    accepted_generation: generation,
                    monetary_state_root: root,
                    gd08_order_digest: digest,
                };
                db.put_cf(
                    db.cf_handle(CF_JOURNAL).unwrap(),
                    hash.as_bytes(),
                    serde_json::to_vec(&entry).unwrap(),
                )
                .unwrap();
            }
            let reader =
                NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
            assert!(reader.read_consistent_accepted(&hash).is_err());
        }
        {
            let db = open_test_writer(&db_path);
            trial_write(&db, &frame, &genesis, 4, true, true);
            db.delete_cf(db.cf_handle(CF_BODIES).unwrap(), hash.as_bytes())
                .unwrap();
        }
        let reader =
            NativeV3AcceptedRocksDbReadOnlyV1::open(&db_path, CHAIN, NS, &genesis).unwrap();
        assert!(reader.read_consistent_accepted(&hash).is_err());
        drop(reader);
        let _ = std::fs::remove_dir_all(db_path);
    }
}
