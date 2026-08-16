use pulsedag_core::{
    errors::PulseError, verify_protocol_restore_identity, ProtocolActivationIdentity,
    ProtocolActivationRecordV1, ProtocolConsensusMode, ProtocolRestoreIdentityGate,
};
use rocksdb::WriteBatch;

use super::{Storage, CHAIN_STATE_KEY};

pub const PROTOCOL_ACTIVATION_STORAGE_KEY: &[u8] = b"protocol_activation_record_v1";

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

pub(crate) fn canonical_current_legacy_identity_from_state(
    state: &pulsedag_core::ChainState,
) -> Result<ProtocolActivationIdentity, PulseError> {
    let observed = ProtocolActivationIdentity::legacy_from_state(state);
    let canonical = ProtocolActivationIdentity::legacy_default_for_chain(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
    );
    if observed == canonical {
        return Ok(canonical);
    }

    let mut historical_runtime = canonical.clone();
    historical_runtime.dag_ordering_version = "legacy".to_string();
    if observed == historical_runtime {
        return Ok(canonical);
    }

    Err(storage_error(
        "unsupported current-legacy DAG ordering identity; refusing protocol normalization",
    ))
}

impl Storage {
    /// Read and internally validate the persisted protocol-activation sidecar.
    /// Invalid JSON, record schema, identity shape or fingerprint fails closed.
    pub fn protocol_activation_record(
        &self,
    ) -> Result<Option<ProtocolActivationRecordV1>, PulseError> {
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let Some(bytes) = self
            .db
            .get_cf(&meta_cf, PROTOCOL_ACTIVATION_STORAGE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
        else {
            return Ok(None);
        };

        let record: ProtocolActivationRecordV1 =
            serde_json::from_slice(&bytes).map_err(|error| storage_error(error.to_string()))?;
        record.validate_internal().map_err(storage_error)?;
        Ok(Some(record))
    }

    /// Verify the durable sidecar against an explicit restore expectation.
    /// Missing records are accepted only through the core legacy-schema1 gate;
    /// activated or mixed expectations fail closed before any fallback decision.
    pub fn verify_persisted_protocol_identity(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<ProtocolRestoreIdentityGate, PulseError> {
        let record = self.protocol_activation_record()?;
        verify_protocol_restore_identity(record.as_ref(), expected).map_err(storage_error)
    }

    /// Atomically persist the current historical runtime snapshot and its
    /// protocol-activation sidecar in the same RocksDB batch.
    ///
    /// This helper deliberately derives the identity from the current runtime
    /// state, which today means tx/header v1 with legacy or ghostdag_dev mode.
    /// It cannot be used to manufacture an activated-v2 record for a legacy
    /// `ChainState`.
    pub fn persist_chain_state_with_protocol_record(
        &self,
        state: &pulsedag_core::ChainState,
    ) -> Result<ProtocolActivationRecordV1, PulseError> {
        let observed = ProtocolActivationIdentity::legacy_from_state(state);
        let identity = if observed.consensus_mode == ProtocolConsensusMode::Legacy {
            canonical_current_legacy_identity_from_state(state)?
        } else {
            observed
        };
        let record = ProtocolActivationRecordV1::from_identity(identity).map_err(storage_error)?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let mut batch = WriteBatch::default();

        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        batch.put_cf(
            &meta_cf,
            PROTOCOL_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&record).map_err(|error| storage_error(error.to_string()))?,
        );
        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))?;
        Ok(record)
    }

    /// Return whether the durable chain snapshot and activation sidecar are
    /// present together. This is a diagnostic invariant only; semantic restore
    /// authorization still requires `verify_persisted_protocol_identity`.
    pub fn protocol_snapshot_sidecar_complete(&self) -> Result<bool, PulseError> {
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let snapshot_present = self
            .db
            .get_cf(&meta_cf, CHAIN_STATE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
            .is_some();
        let record_present = self
            .db
            .get_cf(&meta_cf, PROTOCOL_ACTIVATION_STORAGE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
            .is_some();
        Ok(snapshot_present && record_present)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, ordering_v2::GHOSTDAG_V1_ORDERING_VERSION,
        ProtocolActivationIdentity,
    };

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedag-storage-protocol-{test_name}-{unique}"))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn legacy_missing_record_uses_schema1_compatibility_gate() {
        let path = temp_db_path("legacy-missing");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);

        assert_eq!(
            storage
                .verify_persisted_protocol_identity(&expected)
                .unwrap(),
            ProtocolRestoreIdentityGate::LegacySchema1Compatibility
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn snapshot_and_protocol_record_round_trip_together() {
        let path = temp_db_path("round-trip");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);

        let written = storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();
        let loaded = storage.protocol_activation_record().unwrap().unwrap();

        assert_eq!(loaded, written);
        assert_eq!(loaded.identity, expected);
        assert!(storage.protocol_snapshot_sidecar_complete().unwrap());
        assert_eq!(
            storage
                .verify_persisted_protocol_identity(&expected)
                .unwrap(),
            ProtocolRestoreIdentityGate::VerifiedRecordV1
        );
        assert_eq!(
            storage.load_chain_state().unwrap().unwrap().chain_id,
            state.chain_id
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn legacy_runtime_ordering_marker_persists_canonical_identity() {
        let path = temp_db_path("legacy-runtime-ordering");
        let storage = Storage::open(&path).unwrap();
        let mut state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        state.dag.ordering_version = "legacy".to_string();

        let written = storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();

        assert_eq!(written.identity, expected);
        assert_eq!(
            storage
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            expected
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn unknown_legacy_runtime_ordering_marker_fails_closed() {
        let path = temp_db_path("legacy-runtime-ordering-unknown");
        let storage = Storage::open(&path).unwrap();
        let mut state = init_chain_state("pulsedag-testnet".to_string());
        state.dag.ordering_version = "unexpected-ordering".to_string();

        assert!(storage
            .persist_chain_state_with_protocol_record(&state)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn activated_expectation_without_record_fails_closed() {
        let path = temp_db_path("activated-missing");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet-v2".to_string());
        let expected = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert!(storage
            .verify_persisted_protocol_identity(&expected)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn corrupted_persisted_record_fails_closed() {
        let path = temp_db_path("corrupt");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("pulsedag-testnet".to_string());
        let mut record = ProtocolActivationRecordV1::legacy_from_state(&state).unwrap();
        record.fingerprint = "00".repeat(32);
        let meta_cf = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(
                &meta_cf,
                PROTOCOL_ACTIVATION_STORAGE_KEY,
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();

        assert!(storage.protocol_activation_record().is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
