use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::{
    errors::PulseError, snapshot_transfer_chunk_digest_v1,
    snapshot_transfer_commitment_set_digest_v1, snapshot_transfer_payload_digest_v1,
    ProtocolActivationIdentity,
};
use rocksdb::{WriteBatch, WriteOptions};
use serde::{Deserialize, Serialize};

use super::{
    FastSyncPersistedResumeStatusV1, FastSyncSnapshotBundleV1, SnapshotVerificationReport, Storage,
    FAST_SYNC_SNAPSHOT_MANIFEST_VERSION, FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1,
    MAX_FAST_SYNC_SNAPSHOT_CHUNKS, MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES,
    MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES, MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES,
    PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION, STORAGE_SCHEMA_VERSION,
};

pub const FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION: u32 = 1;
const FAST_SYNC_NETWORK_RESUME_KEY_PREFIX_V1: &str = "fast_sync_network_resume_v1:";
const FAST_SYNC_NETWORK_RESUME_PLAN_SUFFIX_V1: &str = ":plan";
const FAST_SYNC_NETWORK_RESUME_CHUNK_MARKER_V1: &str = ":chunk:";
const MAX_FAST_SYNC_NETWORK_RESUME_PLAN_BYTES_V1: usize = 16 * 1024 * 1024;

type FastSyncNetworkResumeScanV1 = (Vec<u32>, BTreeMap<u32, Vec<u8>>);

/// Compact durable transfer authority reconstructed from the live network
/// summary plus the fully verified commitment pages.
///
/// The live wire intentionally advertises only a bounded subset of the full
/// snapshot manifest. Persisting this compact contract lets a downloader make
/// verified chunks restart-safe before the payload has been assembled, while
/// the completed payload still has to pass the full manifest + protocol bundle
/// verifier before durable chain state can change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FastSyncNetworkTransferPlanV1 {
    pub plan_version: u32,
    pub payload_encoding: String,
    pub chain_id: String,
    pub genesis_hash: String,
    pub protocol_fingerprint: String,
    pub manifest_version: u32,
    pub protocol_snapshot_bundle_format_version: u32,
    pub storage_schema_version: u32,
    pub transfer_id: String,
    pub commitment_set_id: String,
    pub payload_len: u64,
    pub chunk_size: u32,
    pub chunk_count: u32,
    pub chunk_commitments: Vec<String>,
    pub best_height: u64,
    pub selected_tip: String,
    pub state_commitment: String,
    pub prune_boundary_height: Option<u64>,
    pub snapshot_generation: u64,
    pub accepted_storage_generation: u64,
    pub delta_start_generation: u64,
    pub delta_end_generation: u64,
}

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn required_chunk_count(payload_len: u64, chunk_size: u32) -> Result<u32, PulseError> {
    if chunk_size == 0 {
        return Err(storage_error(
            "fast-sync network transfer chunk_size must be non-zero",
        ));
    }
    let chunk_size = u64::from(chunk_size);
    let count = payload_len
        .checked_add(chunk_size - 1)
        .ok_or_else(|| storage_error("fast-sync network transfer chunk count overflow"))?
        / chunk_size;
    u32::try_from(count)
        .map_err(|_| storage_error("fast-sync network transfer chunk count exceeds u32"))
}

fn sync_write_options() -> WriteOptions {
    let mut options = WriteOptions::default();
    options.set_sync(true);
    options
}

fn network_resume_plan_key(transfer_id: &str) -> Vec<u8> {
    format!(
        "{FAST_SYNC_NETWORK_RESUME_KEY_PREFIX_V1}{transfer_id}{FAST_SYNC_NETWORK_RESUME_PLAN_SUFFIX_V1}"
    )
    .into_bytes()
}

fn network_resume_chunk_key(transfer_id: &str, chunk_index: u32) -> Vec<u8> {
    format!(
        "{FAST_SYNC_NETWORK_RESUME_KEY_PREFIX_V1}{transfer_id}{FAST_SYNC_NETWORK_RESUME_CHUNK_MARKER_V1}{chunk_index:08x}"
    )
    .into_bytes()
}

fn serialize_plan(plan: &FastSyncNetworkTransferPlanV1) -> Result<Vec<u8>, PulseError> {
    let bytes = bincode::serialize(plan).map_err(|error| storage_error(error.to_string()))?;
    if bytes.len() > MAX_FAST_SYNC_NETWORK_RESUME_PLAN_BYTES_V1 {
        return Err(storage_error(format!(
            "fast-sync network resume plan is {} bytes; maximum is {}",
            bytes.len(), MAX_FAST_SYNC_NETWORK_RESUME_PLAN_BYTES_V1
        )));
    }
    Ok(bytes)
}

fn deserialize_plan(bytes: &[u8]) -> Result<FastSyncNetworkTransferPlanV1, PulseError> {
    if bytes.len() > MAX_FAST_SYNC_NETWORK_RESUME_PLAN_BYTES_V1 {
        return Err(storage_error(format!(
            "persisted fast-sync network resume plan is {} bytes; maximum is {}",
            bytes.len(), MAX_FAST_SYNC_NETWORK_RESUME_PLAN_BYTES_V1
        )));
    }
    bincode::deserialize(bytes).map_err(|error| {
        storage_error(format!(
            "persisted fast-sync network resume plan failed to decode: {error}"
        ))
    })
}

fn require_manifest_field_match<T: std::fmt::Debug + PartialEq>(
    field: &str,
    advertised: &T,
    actual: &T,
) -> Result<(), PulseError> {
    if advertised != actual {
        return Err(storage_error(format!(
            "fast-sync network transfer advertised {field} does not match completed snapshot: advertised={advertised:?} actual={actual:?}"
        )));
    }
    Ok(())
}

impl FastSyncNetworkTransferPlanV1 {
    pub fn validate_for_expected(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        if self.plan_version != FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION {
            return Err(storage_error(format!(
                "unsupported fast-sync network transfer plan version {}; expected {}",
                self.plan_version, FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION
            )));
        }
        if self.payload_encoding != FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1 {
            return Err(storage_error(format!(
                "unsupported fast-sync network payload encoding {:?}; expected {:?}",
                self.payload_encoding, FAST_SYNC_SNAPSHOT_PAYLOAD_ENCODING_V1
            )));
        }
        if self.manifest_version != FAST_SYNC_SNAPSHOT_MANIFEST_VERSION {
            return Err(storage_error(format!(
                "fast-sync network manifest version {} is unsupported",
                self.manifest_version
            )));
        }
        if self.protocol_snapshot_bundle_format_version
            != PROTOCOL_SNAPSHOT_BUNDLE_FORMAT_VERSION
        {
            return Err(storage_error(format!(
                "fast-sync network protocol snapshot format {} is unsupported",
                self.protocol_snapshot_bundle_format_version
            )));
        }
        if self.storage_schema_version != STORAGE_SCHEMA_VERSION {
            return Err(storage_error(format!(
                "fast-sync network storage schema {} is incompatible with local schema {}",
                self.storage_schema_version, STORAGE_SCHEMA_VERSION
            )));
        }
        if self.chain_id != expected.chain_id {
            return Err(storage_error(format!(
                "fast-sync network chain_id={} does not match expected {}",
                self.chain_id, expected.chain_id
            )));
        }
        if self.genesis_hash != expected.genesis_hash {
            return Err(storage_error(format!(
                "fast-sync network genesis={} does not match expected {}",
                self.genesis_hash, expected.genesis_hash
            )));
        }
        let expected_fingerprint = expected.fingerprint().map_err(storage_error)?;
        if self.protocol_fingerprint != expected_fingerprint {
            return Err(storage_error(
                "fast-sync network protocol fingerprint does not match expected identity",
            ));
        }
        if self.payload_len == 0 || self.payload_len > MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES {
            return Err(storage_error(format!(
                "fast-sync network payload length {} outside supported range 1..={}",
                self.payload_len, MAX_FAST_SYNC_SNAPSHOT_TRANSFER_BYTES
            )));
        }
        let chunk_size = usize::try_from(self.chunk_size)
            .map_err(|_| storage_error("fast-sync network chunk_size does not fit usize"))?;
        if !(MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES..=MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES)
            .contains(&chunk_size)
        {
            return Err(storage_error(format!(
                "fast-sync network chunk_size {} outside storage-supported range {}..={}",
                chunk_size, MIN_FAST_SYNC_SNAPSHOT_CHUNK_BYTES, MAX_FAST_SYNC_SNAPSHOT_CHUNK_BYTES
            )));
        }
        let required = required_chunk_count(self.payload_len, self.chunk_size)?;
        if self.chunk_count != required {
            return Err(storage_error(format!(
                "fast-sync network chunk_count {} does not match required {}",
                self.chunk_count, required
            )));
        }
        if self.chunk_count == 0 || self.chunk_count > MAX_FAST_SYNC_SNAPSHOT_CHUNKS {
            return Err(storage_error(format!(
                "fast-sync network chunk_count {} exceeds supported bound {}",
                self.chunk_count, MAX_FAST_SYNC_SNAPSHOT_CHUNKS
            )));
        }
        if self.chunk_commitments.len() != self.chunk_count as usize {
            return Err(storage_error(format!(
                "fast-sync network commitment count {} does not match chunk_count {}",
                self.chunk_commitments.len(), self.chunk_count
            )));
        }
        if !is_sha256_hex(&self.transfer_id) {
            return Err(storage_error(
                "fast-sync network transfer_id must be a 32-byte SHA-256 hex commitment",
            ));
        }
        if !is_sha256_hex(&self.commitment_set_id) {
            return Err(storage_error(
                "fast-sync network commitment_set_id must be a 32-byte SHA-256 hex commitment",
            ));
        }
        if self
            .chunk_commitments
            .iter()
            .any(|commitment| !is_sha256_hex(commitment))
        {
            return Err(storage_error(
                "fast-sync network transfer contains an invalid chunk commitment",
            ));
        }
        let commitment_set_id =
            snapshot_transfer_commitment_set_digest_v1(&self.transfer_id, &self.chunk_commitments);
        if commitment_set_id != self.commitment_set_id {
            return Err(storage_error(
                "fast-sync network commitment set root does not match the ordered commitment set",
            ));
        }
        if self.selected_tip.is_empty() {
            return Err(storage_error(
                "fast-sync network selected_tip must not be empty",
            ));
        }
        if !is_sha256_hex(&self.state_commitment) {
            return Err(storage_error(
                "fast-sync network state_commitment must be a 32-byte SHA-256 hex commitment",
            ));
        }
        if self.delta_start_generation > self.delta_end_generation {
            return Err(storage_error(
                "fast-sync network delta generation range is inverted",
            ));
        }
        Ok(())
    }

    fn chunk_range(&self, chunk_index: u32) -> Result<std::ops::Range<usize>, PulseError> {
        if chunk_index >= self.chunk_count {
            return Err(storage_error(format!(
                "fast-sync network chunk index {} is outside chunk_count {}",
                chunk_index, self.chunk_count
            )));
        }
        let chunk_size = u64::from(self.chunk_size);
        let start = u64::from(chunk_index)
            .checked_mul(chunk_size)
            .ok_or_else(|| storage_error("fast-sync network chunk offset overflow"))?;
        let end = start
            .checked_add(chunk_size)
            .ok_or_else(|| storage_error("fast-sync network chunk end overflow"))?
            .min(self.payload_len);
        let start = usize::try_from(start)
            .map_err(|_| storage_error("fast-sync network chunk start does not fit usize"))?;
        let end = usize::try_from(end)
            .map_err(|_| storage_error("fast-sync network chunk end does not fit usize"))?;
        Ok(start..end)
    }

    pub fn verify_chunk(&self, chunk_index: u32, chunk: &[u8]) -> Result<(), PulseError> {
        let range = self.chunk_range(chunk_index)?;
        let expected_len = range.end - range.start;
        if chunk.len() != expected_len {
            return Err(storage_error(format!(
                "fast-sync network chunk {} length {} does not match expected {}",
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
                "fast-sync network chunk {} commitment mismatch",
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
                    "fast-sync network received chunk index {} is outside chunk_count {}",
                    index, self.chunk_count
                )));
            }
            received_set.insert(index);
        }
        Ok((0..self.chunk_count)
            .filter(|index| !received_set.contains(index))
            .collect())
    }

    fn verify_completed_manifest(
        &self,
        bundle: &FastSyncSnapshotBundleV1,
    ) -> Result<(), PulseError> {
        let manifest = &bundle.manifest;
        require_manifest_field_match(
            "manifest_version",
            &self.manifest_version,
            &manifest.manifest_version,
        )?;
        require_manifest_field_match(
            "protocol_snapshot_bundle_format_version",
            &self.protocol_snapshot_bundle_format_version,
            &manifest.protocol_snapshot_bundle_format_version,
        )?;
        require_manifest_field_match(
            "storage_schema_version",
            &self.storage_schema_version,
            &manifest.storage_schema_version,
        )?;
        require_manifest_field_match("chain_id", &self.chain_id, &manifest.chain_id)?;
        require_manifest_field_match("genesis_hash", &self.genesis_hash, &manifest.genesis_hash)?;
        require_manifest_field_match(
            "protocol_fingerprint",
            &self.protocol_fingerprint,
            &manifest.protocol_fingerprint,
        )?;
        require_manifest_field_match("best_height", &self.best_height, &manifest.best_height)?;
        require_manifest_field_match("selected_tip", &self.selected_tip, &manifest.selected_tip)?;
        require_manifest_field_match(
            "state_commitment",
            &self.state_commitment,
            &manifest.state_commitment,
        )?;
        require_manifest_field_match(
            "prune_boundary_height",
            &self.prune_boundary_height,
            &manifest.prune_boundary_height,
        )?;
        require_manifest_field_match(
            "snapshot_generation",
            &self.snapshot_generation,
            &manifest.snapshot_generation,
        )?;
        require_manifest_field_match(
            "accepted_storage_generation",
            &self.accepted_storage_generation,
            &manifest.accepted_storage_generation,
        )?;
        require_manifest_field_match(
            "delta_start_generation",
            &self.delta_start_generation,
            &manifest.delta_start_generation,
        )?;
        require_manifest_field_match(
            "delta_end_generation",
            &self.delta_end_generation,
            &manifest.delta_end_generation,
        )?;
        Ok(())
    }
}

impl Storage {
    fn persisted_fast_sync_network_resume_plan_v1(
        &self,
        transfer_id: &str,
        expected: &ProtocolActivationIdentity,
    ) -> Result<Option<FastSyncNetworkTransferPlanV1>, PulseError> {
        let Some(bytes) = self
            .db
            .get(network_resume_plan_key(transfer_id))
            .map_err(|error| storage_error(error.to_string()))?
        else {
            return Ok(None);
        };
        let plan = deserialize_plan(&bytes)?;
        plan.validate_for_expected(expected)?;
        if plan.transfer_id != transfer_id {
            return Err(storage_error(
                "persisted fast-sync network resume plan transfer_id does not match its quarantine key",
            ));
        }
        Ok(Some(plan))
    }

    fn require_matching_fast_sync_network_resume_plan_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        plan.validate_for_expected(expected)?;
        let persisted = self
            .persisted_fast_sync_network_resume_plan_v1(&plan.transfer_id, expected)?
            .ok_or_else(|| {
                storage_error("fast-sync network persisted resume session is not initialized")
            })?;
        if &persisted != plan {
            return Err(storage_error(
                "fast-sync network persisted resume plan mismatch for existing transfer_id",
            ));
        }
        Ok(())
    }

    fn ensure_fast_sync_network_resume_plan_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        plan.validate_for_expected(expected)?;
        if let Some(persisted) =
            self.persisted_fast_sync_network_resume_plan_v1(&plan.transfer_id, expected)?
        {
            if &persisted != plan {
                return Err(storage_error(
                    "fast-sync network persisted resume plan mismatch for existing transfer_id",
                ));
            }
            return Ok(());
        }

        let serialized = serialize_plan(plan)?;
        self.db
            .put_opt(
                network_resume_plan_key(&plan.transfer_id),
                &serialized,
                &sync_write_options(),
            )
            .map_err(|error| storage_error(error.to_string()))?;

        let persisted = self
            .persisted_fast_sync_network_resume_plan_v1(&plan.transfer_id, expected)?
            .ok_or_else(|| {
                storage_error(
                    "fast-sync network persisted resume plan disappeared after synchronous write",
                )
            })?;
        if &persisted != plan {
            return Err(storage_error(
                "fast-sync network persisted resume plan changed during initialization",
            ));
        }
        Ok(())
    }

    fn scan_fast_sync_network_resume_chunks_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
        collect: bool,
    ) -> Result<FastSyncNetworkResumeScanV1, PulseError> {
        self.require_matching_fast_sync_network_resume_plan_v1(plan, expected)?;
        let maximum_chunk_len = usize::try_from(plan.chunk_size)
            .map_err(|_| storage_error("fast-sync network resume chunk_size does not fit usize"))?;
        let mut received = Vec::new();
        let mut chunks = BTreeMap::new();

        for chunk_index in 0..plan.chunk_count {
            let Some(bytes) = self
                .db
                .get(network_resume_chunk_key(&plan.transfer_id, chunk_index))
                .map_err(|error| storage_error(error.to_string()))?
            else {
                continue;
            };
            if bytes.len() > maximum_chunk_len {
                return Err(storage_error(format!(
                    "persisted fast-sync network chunk {chunk_index} length {} exceeds negotiated chunk_size {}",
                    bytes.len(), maximum_chunk_len
                )));
            }
            plan.verify_chunk(chunk_index, &bytes)?;
            received.push(chunk_index);
            if collect {
                chunks.insert(chunk_index, bytes.to_vec());
            }
        }
        Ok((received, chunks))
    }

    /// Create or reopen a restart-safe quarantine session from a fully verified
    /// live transfer summary + commitment set.
    pub fn begin_fast_sync_network_persisted_resume_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<FastSyncPersistedResumeStatusV1, PulseError> {
        self.ensure_fast_sync_network_resume_plan_v1(plan, expected)?;
        self.fast_sync_network_resume_status_v1(plan, expected)
    }

    /// Persist one already network-correlated chunk only after verifying its
    /// exact transfer/index commitment. A successful return is WAL-sync durable.
    pub fn persist_fast_sync_network_resume_chunk_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
        chunk_index: u32,
        chunk: &[u8],
    ) -> Result<(), PulseError> {
        self.ensure_fast_sync_network_resume_plan_v1(plan, expected)?;
        plan.verify_chunk(chunk_index, chunk)?;
        let key = network_resume_chunk_key(&plan.transfer_id, chunk_index);

        if let Some(existing) = self
            .db
            .get(&key)
            .map_err(|error| storage_error(error.to_string()))?
        {
            plan.verify_chunk(chunk_index, &existing)?;
            if existing.as_slice() != chunk {
                return Err(storage_error(format!(
                    "persisted fast-sync network chunk {chunk_index} differs from the supplied verified bytes"
                )));
            }
            return Ok(());
        }

        self.db
            .put_opt(key, chunk, &sync_write_options())
            .map_err(|error| storage_error(error.to_string()))?;

        let persisted = self
            .db
            .get(network_resume_chunk_key(&plan.transfer_id, chunk_index))
            .map_err(|error| storage_error(error.to_string()))?
            .ok_or_else(|| {
                storage_error(format!(
                    "persisted fast-sync network chunk {chunk_index} disappeared after synchronous write"
                ))
            })?;
        plan.verify_chunk(chunk_index, &persisted)?;
        if persisted.as_slice() != chunk {
            return Err(storage_error(format!(
                "persisted fast-sync network chunk {chunk_index} changed during synchronous write"
            )));
        }
        Ok(())
    }

    pub fn fast_sync_network_resume_status_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<FastSyncPersistedResumeStatusV1, PulseError> {
        let (received, _) = self.scan_fast_sync_network_resume_chunks_v1(plan, expected, false)?;
        let missing_chunk_indices = plan.missing_chunk_indices(received.iter().copied())?;
        let received_chunk_count = u32::try_from(received.len())
            .map_err(|_| storage_error("fast-sync network received chunk count exceeds u32"))?;
        Ok(FastSyncPersistedResumeStatusV1 {
            transfer_id: plan.transfer_id.clone(),
            received_chunk_count,
            complete: missing_chunk_indices.is_empty(),
            missing_chunk_indices,
        })
    }

    pub fn load_fast_sync_network_resume_chunks_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<BTreeMap<u32, Vec<u8>>, PulseError> {
        let (_, chunks) = self.scan_fast_sync_network_resume_chunks_v1(plan, expected, true)?;
        Ok(chunks)
    }

    /// Reassemble a completed compact-network session, bind the advertised
    /// summary to the payload's full manifest, then run the existing complete
    /// manifest/protocol verification. This method never mutates chain state.
    pub fn decode_complete_fast_sync_network_transfer_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        chunks: &BTreeMap<u32, Vec<u8>>,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(FastSyncSnapshotBundleV1, SnapshotVerificationReport), PulseError> {
        plan.validate_for_expected(expected)?;
        let missing = plan.missing_chunk_indices(chunks.keys().copied())?;
        if !missing.is_empty() {
            return Err(storage_error(format!(
                "fast-sync network transfer incomplete; {} chunks missing",
                missing.len()
            )));
        }
        let capacity = usize::try_from(plan.payload_len)
            .map_err(|_| storage_error("fast-sync network payload length does not fit usize"))?;
        let mut payload = Vec::with_capacity(capacity);
        for chunk_index in 0..plan.chunk_count {
            let chunk = chunks.get(&chunk_index).ok_or_else(|| {
                storage_error(format!(
                    "fast-sync network chunk {} disappeared",
                    chunk_index
                ))
            })?;
            plan.verify_chunk(chunk_index, chunk)?;
            payload.extend_from_slice(chunk);
        }
        if payload.len() != capacity {
            return Err(storage_error(format!(
                "fast-sync network reassembled payload length {} does not match expected {}",
                payload.len(), capacity
            )));
        }
        let payload_commitment = snapshot_transfer_payload_digest_v1(&payload);
        if payload_commitment != plan.transfer_id {
            return Err(storage_error(
                "fast-sync network reassembled payload commitment mismatch",
            ));
        }
        let bundle: FastSyncSnapshotBundleV1 =
            bincode::deserialize(&payload).map_err(|error| storage_error(error.to_string()))?;
        plan.verify_completed_manifest(&bundle)?;
        let report = self.verify_fast_sync_snapshot_bundle_v1(&bundle, expected)?;
        Ok((bundle, report))
    }

    /// Cross the existing atomic fast-sync import boundary only after the
    /// compact network contract, every chunk, the whole payload, the advertised
    /// manifest subset and the full enclosed manifest have all verified.
    pub fn import_complete_fast_sync_network_transfer_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        chunks: &BTreeMap<u32, Vec<u8>>,
        expected: &ProtocolActivationIdentity,
    ) -> Result<SnapshotVerificationReport, PulseError> {
        let (bundle, _) =
            self.decode_complete_fast_sync_network_transfer_v1(plan, chunks, expected)?;
        self.import_fast_sync_snapshot_bundle_v1(bundle, expected)
    }

    /// Atomically remove one compact network quarantine session after import or
    /// explicit abandonment.
    pub fn clear_fast_sync_network_resume_v1(
        &self,
        plan: &FastSyncNetworkTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        self.require_matching_fast_sync_network_resume_plan_v1(plan, expected)?;
        let mut batch = WriteBatch::default();
        for chunk_index in 0..plan.chunk_count {
            batch.delete(network_resume_chunk_key(&plan.transfer_id, chunk_index));
        }
        batch.delete(network_resume_plan_key(&plan.transfer_id));
        self.db
            .write_opt(batch, &sync_write_options())
            .map_err(|error| storage_error(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PreparedFastSyncSnapshotTransferV1;
    use pulsedag_core::genesis::init_chain_state;

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-fast-sync-network-resume-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn source_bundle(storage: &Storage) -> (FastSyncSnapshotBundleV1, ProtocolActivationIdentity) {
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        let persisted_blocks = state.dag.blocks.values().cloned().collect::<Vec<_>>();
        storage
            .persist_blocks_and_chain_state(&persisted_blocks, &state)
            .unwrap();
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();
        let (bundle, report) = storage
            .export_fast_sync_snapshot_bundle_v1(&expected)
            .unwrap();
        assert!(report.restore_guarantees_explicit);
        (bundle, expected)
    }

    fn prepared_transfer(
        storage: &Storage,
        bundle: &FastSyncSnapshotBundleV1,
        expected: &ProtocolActivationIdentity,
    ) -> PreparedFastSyncSnapshotTransferV1 {
        storage
            .prepare_fast_sync_snapshot_transfer_v1(bundle, expected, 256)
            .unwrap()
            .0
    }

    fn network_plan(prepared: &PreparedFastSyncSnapshotTransferV1) -> FastSyncNetworkTransferPlanV1 {
        let transfer = &prepared.plan;
        let manifest = &transfer.snapshot_manifest;
        FastSyncNetworkTransferPlanV1 {
            plan_version: FAST_SYNC_NETWORK_TRANSFER_PLAN_VERSION,
            payload_encoding: transfer.payload_encoding.clone(),
            chain_id: manifest.chain_id.clone(),
            genesis_hash: manifest.genesis_hash.clone(),
            protocol_fingerprint: manifest.protocol_fingerprint.clone(),
            manifest_version: manifest.manifest_version,
            protocol_snapshot_bundle_format_version: manifest.protocol_snapshot_bundle_format_version,
            storage_schema_version: manifest.storage_schema_version,
            transfer_id: transfer.transfer_id.clone(),
            commitment_set_id: snapshot_transfer_commitment_set_digest_v1(
                &transfer.transfer_id,
                &transfer.chunk_commitments,
            ),
            payload_len: transfer.payload_len,
            chunk_size: transfer.chunk_size,
            chunk_count: transfer.chunk_count,
            chunk_commitments: transfer.chunk_commitments.clone(),
            best_height: manifest.best_height,
            selected_tip: manifest.selected_tip.clone(),
            state_commitment: manifest.state_commitment.clone(),
            prune_boundary_height: manifest.prune_boundary_height,
            snapshot_generation: manifest.snapshot_generation,
            accepted_storage_generation: manifest.accepted_storage_generation,
            delta_start_generation: manifest.delta_start_generation,
            delta_end_generation: manifest.delta_end_generation,
        }
    }

    fn all_chunks(
        prepared: &PreparedFastSyncSnapshotTransferV1,
    ) -> BTreeMap<u32, Vec<u8>> {
        (0..prepared.plan.chunk_count)
            .map(|index| (index, prepared.chunk(index).unwrap().to_vec()))
            .collect()
    }

    #[test]
    fn compact_network_plan_validates_commitment_root_and_identity() {
        let path = temp_db_path("plan");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected);
        let plan = network_plan(&prepared);

        assert!(plan.validate_for_expected(&expected).is_ok());
        let mut tampered = plan.clone();
        tampered.commitment_set_id = "ff".repeat(32);
        assert!(tampered.validate_for_expected(&expected).is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn verified_network_chunks_survive_restart_and_expose_only_gaps() {
        let path = temp_db_path("restart");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected);
        let plan = network_plan(&prepared);
        assert!(plan.chunk_count > 2);

        storage
            .begin_fast_sync_network_persisted_resume_v1(&plan, &expected)
            .unwrap();
        for index in [0, plan.chunk_count - 1] {
            storage
                .persist_fast_sync_network_resume_chunk_v1(
                    &plan,
                    &expected,
                    index,
                    prepared.chunk(index).unwrap(),
                )
                .unwrap();
        }
        drop(storage);

        let reopened = Storage::open(&path).unwrap();
        let status = reopened
            .begin_fast_sync_network_persisted_resume_v1(&plan, &expected)
            .unwrap();
        assert_eq!(status.received_chunk_count, 2);
        assert!(!status.complete);
        assert!(!status.missing_chunk_indices.contains(&0));
        assert!(!status.missing_chunk_indices.contains(&(plan.chunk_count - 1)));
        assert_eq!(
            status.missing_chunk_indices.len(),
            plan.chunk_count as usize - 2
        );

        drop(reopened);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn invalid_network_chunk_never_crosses_durable_quarantine_boundary() {
        let path = temp_db_path("reject-before-write");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected);
        let plan = network_plan(&prepared);
        let mut tampered = prepared.chunk(0).unwrap().to_vec();
        tampered[0] ^= 0x01;

        assert!(storage
            .persist_fast_sync_network_resume_chunk_v1(&plan, &expected, 0, &tampered)
            .is_err());
        assert!(storage
            .db
            .get(network_resume_chunk_key(&plan.transfer_id, 0))
            .unwrap()
            .is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn alternate_network_plan_for_same_transfer_id_is_rejected() {
        let path = temp_db_path("plan-mismatch");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected);
        let plan = network_plan(&prepared);
        storage
            .begin_fast_sync_network_persisted_resume_v1(&plan, &expected)
            .unwrap();

        let mut alternate = plan.clone();
        alternate.best_height = alternate.best_height.saturating_add(1);
        let error = storage
            .begin_fast_sync_network_persisted_resume_v1(&alternate, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("plan mismatch"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn complete_network_transfer_binds_advertised_summary_to_full_manifest() {
        let path = temp_db_path("complete");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected);
        let plan = network_plan(&prepared);
        let chunks = all_chunks(&prepared);

        let (decoded, report) = storage
            .decode_complete_fast_sync_network_transfer_v1(&plan, &chunks, &expected)
            .unwrap();
        assert_eq!(decoded.manifest, bundle.manifest);
        assert!(report.restore_guarantees_explicit);

        let mut lied = plan.clone();
        lied.best_height = lied.best_height.saturating_add(1);
        let error = storage
            .decode_complete_fast_sync_network_transfer_v1(&lied, &chunks, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("advertised best_height"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn advertised_manifest_mismatch_cannot_mutate_import_target() {
        let source_path = temp_db_path("import-source");
        let target_path = temp_db_path("import-target");
        let source = Storage::open(&source_path).unwrap();
        let target = Storage::open(&target_path).unwrap();
        let (bundle, expected) = source_bundle(&source);
        let prepared = prepared_transfer(&source, &bundle, &expected);
        let mut plan = network_plan(&prepared);
        let chunks = all_chunks(&prepared);

        let target_state = init_chain_state(expected.chain_id.clone());
        target
            .persist_chain_state_with_protocol_record(&target_state)
            .unwrap();
        let before = target.load_chain_state().unwrap().unwrap();

        plan.state_commitment = "11".repeat(32);
        assert!(target
            .import_complete_fast_sync_network_transfer_v1(&plan, &chunks, &expected)
            .is_err());
        let after = target.load_chain_state().unwrap().unwrap();
        assert_eq!(after.dag.best_height, before.dag.best_height);
        assert_eq!(after.dag.genesis_hash, before.dag.genesis_hash);
        assert_eq!(after.chain_state_generation, before.chain_state_generation);

        drop(source);
        drop(target);
        let _ = std::fs::remove_dir_all(source_path);
        let _ = std::fs::remove_dir_all(target_path);
    }

    #[test]
    fn clear_network_resume_removes_plan_and_chunks_atomically() {
        let path = temp_db_path("clear");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected);
        let plan = network_plan(&prepared);
        storage
            .persist_fast_sync_network_resume_chunk_v1(
                &plan,
                &expected,
                0,
                prepared.chunk(0).unwrap(),
            )
            .unwrap();

        storage
            .clear_fast_sync_network_resume_v1(&plan, &expected)
            .unwrap();
        assert!(storage
            .db
            .get(network_resume_plan_key(&plan.transfer_id))
            .unwrap()
            .is_none());
        assert!(storage
            .db
            .get(network_resume_chunk_key(&plan.transfer_id, 0))
            .unwrap()
            .is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
