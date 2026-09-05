use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::{
    errors::PulseError, snapshot_transfer_chunk_digest_v1, snapshot_transfer_payload_digest_v1,
    ProtocolActivationIdentity,
};
use serde::{Deserialize, Serialize};

use super::{
    FastSyncSnapshotBundleV1, FastSyncSnapshotManifestV1, SnapshotVerificationReport, Storage,
    FAST_SYNC_SNAPSHOT_MANIFEST_VERSION, PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION,
    STORAGE_SCHEMA_VERSION,
};

pub const FAST_SYNC_SNAPSHOT_TRANSFER_VERSION: u32 = 1;
pub const FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1: &str = "bincode-1.3-fast-sync-bundle-v1";
pub const MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES: usize = 256;
pub const DEFAULT_FAST_SYNC_SNAPSHOT_CHUNK_BYTES: usize = 256 * 1024;
pub const MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES: usize = 512 * 1024;
pub const MAX_FAST_SYNC_SNAPSHOT_CHUNKS: u32 = 131_072;
pub const MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncSnapshotTransferPlanV1 {
    pub transfer_version: u32,
    pub payload_encoding: String,
    pub snapshot_manifest: FastSyncSnapshotManifestV1,
    pub transfer_id: String,
    pub payload_len: u64,
    pub chunk_size: u32,
    pub chunk_count: u32,
    pub chunk_commitments: Vec<String>,
}

#[derive(Debug)]
pub struct PreparedFastSyncSnapshotTransferV1 {
    pub plan: FastSyncSnapshotTransferPlanV1,
    payload: Vec<u8>,
}

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn required_chunk_count(payload_len: u64, chunk_size: u32) -> Result<u32, PulseError> {
    if chunk_size == 0 {
        return Err(storage_error("fast-sync transfer chunk_size must be non-zero"));
    }
    let chunk_size = u64::from(chunk_size);
    let count = payload_len
        .checked_add(chunk_size - 1)
        .ok_or_else(|| storage_error("fast-sync transfer chunk count overflow"))?
        / chunk_size;
    u32::try_from(count).map_err(|_| storage_error("fast-sync transfer chunk count exceeds u32"))
}

impl FastSyncSnapshotTransferPlanV1 {
    pub fn validate_for_expected(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        if self.transfer_version != FAST_SYNC_SNAPSHOT_TRANSFER_VERSION {
            return Err(storage_error(format!(
                "unsupported fast-sync transfer version {}; expected {}",
                self.transfer_version, FAST_SYNC_SNAPSHOT_TRANSFER_VERSION
            )));
        }
        if self.payload_encoding != FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1 {
            return Err(storage_error(format!(
                "unsupported fast-sync payload encoding {:?}; expected {:?}",
                self.payload_encoding, FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1
            )));
        }
        if self.snapshot_manifest.manifest_version != FAST_SYNC_SNAPSHOT_MANIFEST_VERSION {
            return Err(storage_error(format!(
                "fast-sync transfer manifest version {} is unsupported",
                self.snapshot_manifest.manifest_version
            )));
        }
        if self.snapshot_manifest.protocol_snapshot_bundle_format_version
            != PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
        {
            return Err(storage_error(format!(
                "fast-sync transfer protocol snapshot format {} is unsupported",
                self.snapshot_manifest.protocol_snapshot_bundle_format_version
            )));
        }
        if self.snapshot_manifest.storage_schema_version != STORAGE_SCHEMA_VERSION {
            return Err(storage_error(format!(
                "fast-sync transfer storage schema {} is incompatible with local schema {}",
                self.snapshot_manifest.storage_schema_version, STORAGE_SCHEMA_VERSION
            )));
        }
        if self.snapshot_manifest.chain_id != expected.chain_id {
            return Err(storage_error(format!(
                "fast-sync transfer chain_id={} does not match expected {}",
                self.snapshot_manifest.chain_id, expected.chain_id
            )));
        }
        if self.snapshot_manifest.genesis_hash != expected.genesis_hash {
            return Err(storage_error(format!(
                "fast-sync transfer genesis={} does not match expected {}",
                self.snapshot_manifest.genesis_hash, expected.genesis_hash
            )));
        }
        let expected_fingerprint = expected.fingerprint().map_err(storage_error)?;
        if self.snapshot_manifest.protocol_fingerprint != expected_fingerprint {
            return Err(storage_error(
                "fast-sync transfer protocol fingerprint does not match expected identity",
            ));
        }
        if self.payload_len == 0 {
            return Err(storage_error("fast-sync transfer payload must be non-empty"));
        }
        if self.payload_len > MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES {
            return Err(storage_error(format!(
                "fast-sync transfer payload {} exceeds maximum {}",
                self.payload_len, MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES
            )));
        }
        let chunk_size = usize::try_from(self.chunk_size)
            .map_err(|_| storage_error("fast-sync transfer chunk_size does not fit usize"))?;
        if !(MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES..=MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES)
            .contains(&chunk_size)
        {
            return Err(storage_error(format!(
                "fast-sync transfer chunk_size {} outside supported range {}..={}",
                chunk_size,
                MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES,
                MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES
            )));
        }
        let required = required_chunk_count(self.payload_len, self.chunk_size)?;
        if self.chunk_count != required {
            return Err(storage_error(format!(
                "fast-sync transfer chunk_count {} does not match required {}",
                self.chunk_count, required
            )));
        }
        if self.chunk_count == 0 || self.chunk_count > MAX_FAST_SYNC_SNAPSHOT_CHUNKS {
            return Err(storage_error(format!(
                "fast-sync transfer chunk_count {} exceeds supported bound {}",
                self.chunk_count, MAX_FAST_SYNC_SNAPSHOT_CHUNKS
            )));
        }
        if self.chunk_commitments.len() != self.chunk_count as usize {
            return Err(storage_error(format!(
                "fast-sync transfer commitment count {} does not match chunk_count {}",
                self.chunk_commitments.len(), self.chunk_count
            )));
        }
        if !is_sha256_hex(&self.transfer_id) {
            return Err(storage_error(
                "fast-sync transfer_id must be a 32-byte SHA-256 hex commitment",
            ));
        }
        if self
            .chunk_commitments
            .iter()
            .any(|commitment| !is_sha256_hex(commitment))
        {
            return Err(storage_error(
                "fast-sync transfer contains an invalid chunk commitment",
            ));
        }
        Ok(())
    }

    fn chunk_range(&self, chunk_index: u32) -> Result<std::ops::Range<usize>, PulseError> {
        if chunk_index >= self.chunk_count {
            return Err(storage_error(format!(
                "fast-sync chunk index {} is outside chunk_count {}",
                chunk_index, self.chunk_count
            )));
        }
        let chunk_size = u64::from(self.chunk_size);
        let start = u64::from(chunk_index)
            .checked_mul(chunk_size)
            .ok_or_else(|| storage_error("fast-sync chunk offset overflow"))?;
        let end = start
            .checked_add(chunk_size)
            .ok_or_else(|| storage_error("fast-sync chunk end overflow"))?
            .min(self.payload_len);
        let start = usize::try_from(start)
            .map_err(|_| storage_error("fast-sync chunk start does not fit usize"))?;
        let end = usize::try_from(end)
            .map_err(|_| storage_error("fast-sync chunk end does not fit usize"))?;
        Ok(start..end)
    }

    pub fn verify_chunk(&self, chunk_index: u32, chunk: &[u8]) -> Result<(), PulseError> {
        let range = self.chunk_range(chunk_index)?;
        let expected_len = range.end - range.start;
        if chunk.len() != expected_len {
            return Err(storage_error(format!(
                "fast-sync chunk {} length {} does not match expected {}",
                chunk_index,
                chunk.len(),
                expected_len
            )));
        }
        let expected_commitment = &self.chunk_commitments[chunk_index as usize];
        let actual_commitment =
            snapshot_transfer_chunk_digest_v1(&self.transfer_id, chunk_index, chunk);
        if &actual_commitment != expected_commitment {
            return Err(storage_error(format!(
                "fast-sync chunk {} commitment mismatch",
                chunk_index
            )));
        }
        Ok(())
    }

    pub fn missing_chunk_indices(
        &self,
        received: impl IntoIterator<Item = u32>,
    ) -> Result<Vec<u32>, PulseError> {
        let mut received_set = BTreeSet::new();
        for index in received {
            if index >= self.chunk_count {
                return Err(storage_error(format!(
                    "fast-sync received chunk index {} is outside chunk_count {}",
                    index, self.chunk_count
                )));
            }
            received_set.insert(index);
        }
        Ok((0..self.chunk_count)
            .filter(|index| !received_set.contains(index))
            .collect())
    }
}

impl PreparedFastSyncSnapshotTransferV1 {
    pub fn chunk(&self, chunk_index: u32) -> Result<&[u8], PulseError> {
        let range = self.plan.chunk_range(chunk_index)?;
        Ok(&self.payload[range])
    }

    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }
}

impl Storage {
    /// Serialize an already verified fast-sync snapshot into a bounded binary
    /// payload and publish independently verifiable chunk commitments.
    pub fn prepare_fast_sync_snapshot_transfer_v1(
        &self,
        bundle: &FastSyncSnapshotBundleV1,
        expected: &ProtocolActivationIdentity,
        chunk_size: usize,
    ) -> Result<(PreparedFastSyncSnapshotTransferV1, SnapshotVerificationReport), PulseError> {
        let report = self.verify_fast_sync_snapshot_bundle_v1(bundle, expected)?;
        if !(MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES..=MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES)
            .contains(&chunk_size)
        {
            return Err(storage_error(format!(
                "fast-sync chunk_size {} outside supported range {}..={}",
                chunk_size,
                MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES,
                MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES
            )));
        }
        let payload = bincode::serialize(bundle).map_err(|error| storage_error(error.to_string()))?;
        let payload_len = u64::try_from(payload.len())
            .map_err(|_| storage_error("fast-sync payload length exceeds u64"))?;
        if payload_len == 0 || payload_len > MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES {
            return Err(storage_error(format!(
                "fast-sync serialized payload length {} outside supported range 1..={}",
                payload_len, MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES
            )));
        }
        let chunk_size_u32 = u32::try_from(chunk_size)
            .map_err(|_| storage_error("fast-sync chunk_size exceeds u32"))?;
        let chunk_count = required_chunk_count(payload_len, chunk_size_u32)?;
        if chunk_count > MAX_FAST_SYNC_SNAPSHOT_CHUNKS {
            return Err(storage_error(format!(
                "fast-sync transfer requires {} chunks, exceeding maximum {}",
                chunk_count, MAX_FAST_SYNC_SNAPSHOT_CHUNKS
            )));
        }
        let transfer_id = snapshot_transfer_payload_digest_v1(&payload);
        let mut chunk_commitments = Vec::with_capacity(chunk_count as usize);
        for chunk_index in 0..chunk_count {
            let start = chunk_index as usize * chunk_size;
            let end = start.saturating_add(chunk_size).min(payload.len());
            chunk_commitments.push(snapshot_transfer_chunk_digest_v1(
                &transfer_id,
                chunk_index,
                &payload[start..end],
            ));
        }
        let plan = FastSyncSnapshotTransferPlanV1 {
            transfer_version: FAST_SYNC_SNAPSHOT_TRANSFER_VERSION,
            payload_encoding: FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1.to_string(),
            snapshot_manifest: bundle.manifest.clone(),
            transfer_id,
            payload_len,
            chunk_size: chunk_size_u32,
            chunk_count,
            chunk_commitments,
        };
        plan.validate_for_expected(expected)?;
        Ok((PreparedFastSyncSnapshotTransferV1 { plan, payload }, report))
    }

    /// Reassemble only a complete set of individually verified chunks. The
    /// whole payload digest, transfer manifest and enclosed snapshot contract are
    /// all verified again before a bundle is returned to the caller.
    pub fn decode_complete_fast_sync_snapshot_transfer_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        chunks: &BTreeMap<u32, Vec<u8>>,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(FastSyncSnapshotBundleV1, SnapshotVerificationReport), PulseError> {
        plan.validate_for_expected(expected)?;
        let missing = plan.missing_chunk_indices(chunks.keys().copied())?;
        if !missing.is_empty() {
            return Err(storage_error(format!(
                "fast-sync transfer incomplete; {} chunks missing",
                missing.len()
            )));
        }
        let capacity = usize::try_from(plan.payload_len)
            .map_err(|_| storage_error("fast-sync payload length does not fit usize"))?;
        let mut payload = Vec::with_capacity(capacity);
        for chunk_index in 0..plan.chunk_count {
            let chunk = chunks.get(&chunk_index).ok_or_else(|| {
                storage_error(format!("fast-sync chunk {} disappeared", chunk_index))
            })?;
            plan.verify_chunk(chunk_index, chunk)?;
            payload.extend_from_slice(chunk);
        }
        if payload.len() != capacity {
            return Err(storage_error(format!(
                "fast-sync reassembled payload length {} does not match expected {}",
                payload.len(), capacity
            )));
        }
        let payload_commitment = snapshot_transfer_payload_digest_v1(&payload);
        if payload_commitment != plan.transfer_id {
            return Err(storage_error(
                "fast-sync reassembled payload commitment mismatch",
            ));
        }
        let bundle: FastSyncSnapshotBundleV1 =
            bincode::deserialize(&payload).map_err(|error| storage_error(error.to_string()))?;
        if bundle.manifest != plan.snapshot_manifest {
            return Err(storage_error(
                "fast-sync transfer plan manifest does not match enclosed snapshot manifest",
            ));
        }
        let report = self.verify_fast_sync_snapshot_bundle_v1(&bundle, expected)?;
        Ok((bundle, report))
    }

    /// Complete transfer verification first, then cross the existing atomic
    /// fast-sync import boundary. Corrupt or incomplete transfers cannot mutate
    /// durable storage.
    pub fn import_complete_fast_sync_snapshot_transfer_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        chunks: &BTreeMap<u32, Vec<u8>>,
        expected: &ProtocolActivationIdentity,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        let (bundle, _) =
            self.decode_complete_fast_sync_snapshot_transfer_v1(plan, chunks, expected)?;
        self.import_fast_sync_snapshot_bundle_v1(bundle, expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::genesis::init_chain_state;

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-fast-sync-transfer-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn source_bundle(
        storage: &Storage,
    ) -> (FastSyncSnapshotBundleV1, ProtocolActivationIdentity) {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();
        let (bundle, report) = storage
            .export_fast_sync_snapshot_bundle_v1(&expected)
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        (bundle, expected)
    }

    fn all_chunks(prepared: &PreparedFastSyncSnapshotTransferV1) -> BTreeMap<u32, Vec<u8>> {
        (0..prepared.plan.chunk_count)
            .map(|index| (index, prepared.chunk(index).unwrap().to_vec()))
            .collect()
    }

    #[test]
    fn chunked_transfer_round_trip_exposes_resume_gaps() {
        let path = temp_db_path("round-trip");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let (prepared, report) = storage
            .prepare_fast_sync_snapshot_transfer_v1(&bundle, &expected, 256)
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        assert!(prepared.plan.chunk_count > 1);
        assert_eq!(prepared.payload_len() as u64, prepared.plan.payload_len);

        let mut chunks = all_chunks(&prepared);
        let missing_index = prepared.plan.chunk_count / 2;
        let removed = chunks.remove(&missing_index).unwrap();
        assert_eq!(
            prepared
                .plan
                .missing_chunk_indices(chunks.keys().copied())
                .unwrap(),
            vec![missing_index]
        );
        chunks.insert(missing_index, removed);

        let (decoded, decoded_report) = storage
            .decode_complete_fast_sync_snapshot_transfer_v1(&prepared.plan, &chunks, &expected)
            .unwrap();
        assert_eq!(decoded.manifest, bundle.manifest);
        assert!(decoded_report.restore_guarantees_explicit);

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn tampered_chunk_is_rejected_before_decode() {
        let path = temp_db_path("chunk-tamper");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let (prepared, _) = storage
            .prepare_fast_sync_snapshot_transfer_v1(&bundle, &expected, 256)
            .unwrap();
        let mut chunk = prepared.chunk(0).unwrap().to_vec();
        chunk[0] ^= 0x01;

        let error = prepared.plan.verify_chunk(0, &chunk).unwrap_err();
        assert!(error.to_string().contains("commitment mismatch"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn transfer_plan_rejects_wrong_network_before_payload_use() {
        let path = temp_db_path("wrong-network");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let (prepared, _) = storage
            .prepare_fast_sync_snapshot_transfer_v1(&bundle, &expected, 256)
            .unwrap();
        let other_state = init_chain_state("pulsedag-private".to_string());
        let other = ProtocolActivationIdentity::legacy_from_state(&other_state);

        let error = prepared.plan.validate_for_expected(&other).unwrap_err();
        assert!(error.to_string().contains("chain_id"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn incomplete_transfer_fails_without_mutating_target() {
        let source_path = temp_db_path("import-source");
        let target_path = temp_db_path("import-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let (bundle, expected) = source_bundle(&source);
        let (prepared, _) = source
            .prepare_fast_sync_snapshot_transfer_v1(&bundle, &expected, 256)
            .unwrap();
        let mut chunks = all_chunks(&prepared);
        chunks.remove(&(prepared.plan.chunk_count - 1));

        let target_state = init_chain_state("pulsedag-private".to_string());
        let target_identity = ProtocolActivationIdentity::legacy_from_state(&target_state);
        target
            .persist_chain_state_with_protocol_record(&target_state)
            .unwrap();

        assert!(target
            .import_complete_fast_sync_snapshot_transfer_v1(&prepared.plan, &chunks, &expected)
            .is_err());
        assert_eq!(
            target.load_chain_state().unwrap().unwrap().chain_id,
            target_state.chain_id
        );
        assert_eq!(
            target
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            target_identity
        );

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }
}
