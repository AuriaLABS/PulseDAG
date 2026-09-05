use std::collections::BTreeMap;

use pulsedag_core::{errors::PulseError, ProtocolActivationIdentity};
use rocksdb::{WriteBatch, WriteOptions};

use super::{FastSyncSnapshotTransferPlanV1, Storage};

const FAST_SYNC_RESUME_KEY_PREFIX_V1: &str = "fast_sync_resume_v1:";
const FAST_SYNC_RESUME_PLAN_SUFFIX_V1: &str = ":plan";
const FAST_SYNC_RESUME_CHUNK_MARKER_V1: &str = ":chunk:";
const MAX_FAST_SYNC_RESUME_PLAN_BYTES_V1: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastSyncPersistedResumeStatusV1 {
    pub transfer_id: String,
    pub received_chunk_count: u32,
    pub missing_chunk_indices: Vec<u32>,
    pub complete: bool,
}

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn resume_plan_key(transfer_id: &str) -> Vec<u8> {
    format!("{FAST_SYNC_RESUME_KEY_PREFIX_V1}{transfer_id}{FAST_SYNC_RESUME_PLAN_SUFFIX_V1}")
        .into_bytes()
}

fn resume_chunk_key(transfer_id: &str, chunk_index: u32) -> Vec<u8> {
    format!(
        "{FAST_SYNC_RESUME_KEY_PREFIX_V1}{transfer_id}{FAST_SYNC_RESUME_CHUNK_MARKER_V1}{chunk_index:08x}"
    )
    .into_bytes()
}

fn sync_write_options() -> WriteOptions {
    let mut options = WriteOptions::default();
    options.set_sync(true);
    options
}

fn serialize_plan(plan: &FastSyncSnapshotTransferPlanV1) -> Result<Vec<u8>, PulseError> {
    let bytes = bincode::serialize(plan).map_err(|error| storage_error(error.to_string()))?;
    if bytes.len() > MAX_FAST_SYNC_RESUME_PLAN_BYTES_V1 {
        return Err(storage_error(format!(
            "fast-sync persisted resume plan is {} bytes; maximum is {}",
            bytes.len(),
            MAX_FAST_SYNC_RESUME_PLAN_BYTES_V1
        )));
    }
    Ok(bytes)
}

fn deserialize_plan(bytes: &[u8]) -> Result<FastSyncSnapshotTransferPlanV1, PulseError> {
    if bytes.len() > MAX_FAST_SYNC_RESUME_PLAN_BYTES_V1 {
        return Err(storage_error(format!(
            "persisted fast-sync resume plan is {} bytes; maximum is {}",
            bytes.len(),
            MAX_FAST_SYNC_RESUME_PLAN_BYTES_V1
        )));
    }
    bincode::deserialize(bytes).map_err(|error| {
        storage_error(format!(
            "persisted fast-sync resume plan failed to decode: {error}"
        ))
    })
}

impl Storage {
    fn persisted_fast_sync_resume_plan_v1(
        &self,
        transfer_id: &str,
        expected: &ProtocolActivationIdentity,
    ) -> Result<Option<FastSyncSnapshotTransferPlanV1>, PulseError> {
        let Some(bytes) = self
            .db
            .get(resume_plan_key(transfer_id))
            .map_err(|error| storage_error(error.to_string()))?
        else {
            return Ok(None);
        };
        let plan = deserialize_plan(&bytes)?;
        plan.validate_for_expected(expected)?;
        if plan.transfer_id != transfer_id {
            return Err(storage_error(
                "persisted fast-sync resume plan transfer_id does not match its quarantine key",
            ));
        }
        Ok(Some(plan))
    }

    fn require_matching_fast_sync_resume_plan_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        plan.validate_for_expected(expected)?;
        let persisted = self
            .persisted_fast_sync_resume_plan_v1(&plan.transfer_id, expected)?
            .ok_or_else(|| {
                storage_error("fast-sync persisted resume session is not initialized")
            })?;
        if &persisted != plan {
            return Err(storage_error(
                "fast-sync persisted resume plan mismatch for existing transfer_id",
            ));
        }
        Ok(())
    }

    fn ensure_fast_sync_resume_plan_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        plan.validate_for_expected(expected)?;
        if let Some(persisted) =
            self.persisted_fast_sync_resume_plan_v1(&plan.transfer_id, expected)?
        {
            if &persisted != plan {
                return Err(storage_error(
                    "fast-sync persisted resume plan mismatch for existing transfer_id",
                ));
            }
            return Ok(());
        }

        let serialized = serialize_plan(plan)?;
        self.db
            .put_opt(
                resume_plan_key(&plan.transfer_id),
                &serialized,
                &sync_write_options(),
            )
            .map_err(|error| storage_error(error.to_string()))?;

        // Re-read after the synchronous publish. Besides detecting local durable
        // corruption, this also fails closed if a cooperating process raced this
        // session with a different valid plan for the same payload id.
        let persisted = self
            .persisted_fast_sync_resume_plan_v1(&plan.transfer_id, expected)?
            .ok_or_else(|| {
                storage_error("fast-sync persisted resume plan disappeared after synchronous write")
            })?;
        if &persisted != plan {
            return Err(storage_error(
                "fast-sync persisted resume plan changed during initialization",
            ));
        }
        Ok(())
    }

    fn scan_fast_sync_resume_chunks_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
        collect: bool,
    ) -> Result<(Vec<u32>, BTreeMap<u32, Vec<u8>>), PulseError> {
        self.require_matching_fast_sync_resume_plan_v1(plan, expected)?;
        let maximum_chunk_len = usize::try_from(plan.chunk_size)
            .map_err(|_| storage_error("fast-sync resume chunk_size does not fit usize"))?;
        let mut received = Vec::new();
        let mut chunks = BTreeMap::new();

        for chunk_index in 0..plan.chunk_count {
            let Some(bytes) = self
                .db
                .get(resume_chunk_key(&plan.transfer_id, chunk_index))
                .map_err(|error| storage_error(error.to_string()))?
            else {
                continue;
            };
            if bytes.len() > maximum_chunk_len {
                return Err(storage_error(format!(
                    "persisted fast-sync chunk {chunk_index} length {} exceeds negotiated chunk_size {}",
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

    /// Create or reopen a durable quarantine session for one exact transfer
    /// plan. The complete plan is persisted with a synchronous RocksDB write;
    /// a different valid plan using the same payload transfer id is rejected.
    pub fn begin_fast_sync_persisted_resume_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<FastSyncPersistedResumeStatusV1, PulseError> {
        self.ensure_fast_sync_resume_plan_v1(plan, expected)?;
        self.fast_sync_resume_status_v1(plan, expected)
    }

    /// Verify one chunk against the exact persisted plan before crossing the
    /// durable quarantine boundary. The RocksDB WAL is synchronously flushed
    /// for the chunk write, making successfully returned chunks restart-safe.
    pub fn persist_fast_sync_resume_chunk_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
        chunk_index: u32,
        chunk: &[u8],
    ) -> Result<(), PulseError> {
        self.ensure_fast_sync_resume_plan_v1(plan, expected)?;
        plan.verify_chunk(chunk_index, chunk)?;
        let key = resume_chunk_key(&plan.transfer_id, chunk_index);

        if let Some(existing) = self
            .db
            .get(&key)
            .map_err(|error| storage_error(error.to_string()))?
        {
            plan.verify_chunk(chunk_index, &existing)?;
            if existing.as_slice() != chunk {
                return Err(storage_error(format!(
                    "persisted fast-sync chunk {chunk_index} differs from the supplied verified bytes"
                )));
            }
            return Ok(());
        }

        self.db
            .put_opt(key, chunk, &sync_write_options())
            .map_err(|error| storage_error(error.to_string()))?;

        let persisted = self
            .db
            .get(resume_chunk_key(&plan.transfer_id, chunk_index))
            .map_err(|error| storage_error(error.to_string()))?
            .ok_or_else(|| {
                storage_error(format!(
                    "persisted fast-sync chunk {chunk_index} disappeared after synchronous write"
                ))
            })?;
        plan.verify_chunk(chunk_index, &persisted)?;
        if persisted.as_slice() != chunk {
            return Err(storage_error(format!(
                "persisted fast-sync chunk {chunk_index} changed during synchronous write"
            )));
        }
        Ok(())
    }

    /// Re-scan and cryptographically verify every durable chunk before exposing
    /// restart state to the downloader.
    pub fn fast_sync_resume_status_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<FastSyncPersistedResumeStatusV1, PulseError> {
        let (received, _) = self.scan_fast_sync_resume_chunks_v1(plan, expected, false)?;
        let missing_chunk_indices = plan.missing_chunk_indices(received.iter().copied())?;
        let received_chunk_count = u32::try_from(received.len())
            .map_err(|_| storage_error("fast-sync received chunk count exceeds u32"))?;
        Ok(FastSyncPersistedResumeStatusV1 {
            transfer_id: plan.transfer_id.clone(),
            received_chunk_count,
            complete: missing_chunk_indices.is_empty(),
            missing_chunk_indices,
        })
    }

    /// Load only chunks that still verify against the exact durable session
    /// plan. The caller can pass the returned map to the existing complete
    /// transfer verifier/import boundary; this method itself never mutates chain
    /// state or accepted block storage.
    pub fn load_fast_sync_resume_chunks_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<BTreeMap<u32, Vec<u8>>, PulseError> {
        let (_, chunks) = self.scan_fast_sync_resume_chunks_v1(plan, expected, true)?;
        Ok(chunks)
    }

    /// Remove one exact quarantine session atomically. Chunks and the plan are
    /// deleted in the same synchronous RocksDB batch so a crash cannot publish a
    /// partially cleared session as a new identity.
    pub fn clear_fast_sync_resume_v1(
        &self,
        plan: &FastSyncSnapshotTransferPlanV1,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(), PulseError> {
        self.require_matching_fast_sync_resume_plan_v1(plan, expected)?;
        let mut batch = WriteBatch::default();
        for chunk_index in 0..plan.chunk_count {
            batch.delete(resume_chunk_key(&plan.transfer_id, chunk_index));
        }
        batch.delete(resume_plan_key(&plan.transfer_id));
        self.db
            .write_opt(batch, &sync_write_options())
            .map_err(|error| storage_error(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FastSyncSnapshotBundleV1, PreparedFastSyncSnapshotTransferV1};
    use pulsedag_core::genesis::init_chain_state;

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-fast-sync-resume-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn source_bundle(storage: &Storage) -> (FastSyncSnapshotBundleV1, ProtocolActivationIdentity) {
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

    fn prepared_transfer(
        storage: &Storage,
        bundle: &FastSyncSnapshotBundleV1,
        expected: &ProtocolActivationIdentity,
        chunk_size: usize,
    ) -> PreparedFastSyncSnapshotTransferV1 {
        storage
            .prepare_fast_sync_snapshot_transfer_v1(bundle, expected, chunk_size)
            .unwrap()
            .0
    }

    #[test]
    fn verified_chunks_survive_restart_and_expose_only_resume_gaps() {
        let path = temp_db_path("restart");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected, 256);
        assert!(prepared.plan.chunk_count > 2);
        storage
            .begin_fast_sync_persisted_resume_v1(&prepared.plan, &expected)
            .unwrap();
        let persisted = [0, prepared.plan.chunk_count - 1];
        for index in persisted {
            storage
                .persist_fast_sync_resume_chunk_v1(
                    &prepared.plan,
                    &expected,
                    index,
                    prepared.chunk(index).unwrap(),
                )
                .unwrap();
        }
        drop(storage);

        let reopened = Storage::open(&path).unwrap();
        let status = reopened
            .begin_fast_sync_persisted_resume_v1(&prepared.plan, &expected)
            .unwrap();
        assert_eq!(status.received_chunk_count, 2);
        assert!(!status.complete);
        assert!(!status.missing_chunk_indices.contains(&0));
        assert!(!status
            .missing_chunk_indices
            .contains(&(prepared.plan.chunk_count - 1)));
        assert_eq!(
            status.missing_chunk_indices.len(),
            prepared.plan.chunk_count as usize - 2
        );

        drop(reopened);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn invalid_chunk_never_crosses_durable_quarantine_boundary() {
        let path = temp_db_path("reject-before-write");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected, 256);
        let mut tampered = prepared.chunk(0).unwrap().to_vec();
        tampered[0] ^= 0x01;

        assert!(storage
            .persist_fast_sync_resume_chunk_v1(&prepared.plan, &expected, 0, &tampered)
            .is_err());
        assert!(storage
            .db
            .get(resume_chunk_key(&prepared.plan.transfer_id, 0))
            .unwrap()
            .is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn tampered_durable_chunk_fails_closed_on_reopen_scan() {
        let path = temp_db_path("durable-tamper");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected, 256);
        storage
            .persist_fast_sync_resume_chunk_v1(
                &prepared.plan,
                &expected,
                0,
                prepared.chunk(0).unwrap(),
            )
            .unwrap();
        let mut tampered = prepared.chunk(0).unwrap().to_vec();
        tampered[0] ^= 0x01;
        storage
            .db
            .put(resume_chunk_key(&prepared.plan.transfer_id, 0), tampered)
            .unwrap();
        drop(storage);

        let reopened = Storage::open(&path).unwrap();
        let error = reopened
            .fast_sync_resume_status_v1(&prepared.plan, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("commitment mismatch"));

        drop(reopened);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn same_payload_with_different_chunk_plan_cannot_rebind_session() {
        let path = temp_db_path("plan-rebind");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let small_chunks = prepared_transfer(&storage, &bundle, &expected, 256);
        let larger_chunks = prepared_transfer(&storage, &bundle, &expected, 512);
        assert_eq!(
            small_chunks.plan.transfer_id,
            larger_chunks.plan.transfer_id
        );
        assert_ne!(small_chunks.plan, larger_chunks.plan);

        storage
            .begin_fast_sync_persisted_resume_v1(&small_chunks.plan, &expected)
            .unwrap();
        let error = storage
            .begin_fast_sync_persisted_resume_v1(&larger_chunks.plan, &expected)
            .unwrap_err();
        assert!(error.to_string().contains("plan mismatch"));

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn complete_restart_resume_still_crosses_existing_full_verification_boundary() {
        let path = temp_db_path("complete-restart");
        let storage = Storage::open(&path).unwrap();
        let (bundle, expected) = source_bundle(&storage);
        let prepared = prepared_transfer(&storage, &bundle, &expected, 256);
        for index in 0..prepared.plan.chunk_count {
            storage
                .persist_fast_sync_resume_chunk_v1(
                    &prepared.plan,
                    &expected,
                    index,
                    prepared.chunk(index).unwrap(),
                )
                .unwrap();
        }
        drop(storage);

        let reopened = Storage::open(&path).unwrap();
        let status = reopened
            .fast_sync_resume_status_v1(&prepared.plan, &expected)
            .unwrap();
        assert!(status.complete);
        let chunks = reopened
            .load_fast_sync_resume_chunks_v1(&prepared.plan, &expected)
            .unwrap();
        let (decoded, report) = reopened
            .decode_complete_fast_sync_snapshot_transfer_v1(&prepared.plan, &chunks, &expected)
            .unwrap();
        assert_eq!(decoded.manifest, bundle.manifest);
        assert!(report.restore_guarantees_explicit);

        reopened
            .clear_fast_sync_resume_v1(&prepared.plan, &expected)
            .unwrap();
        assert!(reopened
            .fast_sync_resume_status_v1(&prepared.plan, &expected)
            .is_err());

        drop(reopened);
        let _ = std::fs::remove_dir_all(path);
    }
}
