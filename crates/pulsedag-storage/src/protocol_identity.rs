use pulsedag_core::{
    errors::PulseError, verify_protocol_restore_identity, ProtocolActivationIdentity,
    MonetaryCadenceSegment, ProtocolActivationRecordV1, ProtocolConsensusMode,
    ProtocolMonetaryActivationRecordV2, ProtocolRestoreIdentityGate,
};
use rocksdb::WriteBatch;

use super::{Storage, CHAIN_STATE_KEY};

pub const PROTOCOL_ACTIVATION_STORAGE_KEY: &[u8] = b"protocol_activation_record_v1";
pub const PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY: &[u8] =
    b"protocol_monetary_activation_record_v2";

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

    /// Read and internally validate the v3 monetary-activation sidecar.
    ///
    /// This record is additive to the existing v1 protocol sidecar. Keeping
    /// the two records separate preserves historical restore compatibility
    /// while allowing v3 restore to require the exact policy + cadence binding.
    pub fn protocol_monetary_activation_record(
        &self,
    ) -> Result<Option<ProtocolMonetaryActivationRecordV2>, PulseError> {
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let Some(bytes) = self
            .db
            .get_cf(&meta_cf, PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
        else {
            return Ok(None);
        };

        let record: ProtocolMonetaryActivationRecordV2 =
            serde_json::from_slice(&bytes).map_err(|error| storage_error(error.to_string()))?;
        record.validate_internal().map_err(storage_error)?;
        Ok(Some(record))
    }

    /// Verify a persisted v3 monetary identity. There is deliberately no
    /// legacy fallback: a v3 restore without this exact sidecar fails closed.
    pub fn verify_persisted_monetary_identity(
        &self,
        expected: &ProtocolActivationIdentity,
        expected_cadence: &[MonetaryCadenceSegment],
    ) -> Result<(), PulseError> {
        let record = self
            .protocol_monetary_activation_record()?
            .ok_or_else(|| storage_error("missing v3 monetary activation sidecar"))?;
        record
            .verify_expected(expected, expected_cadence)
            .map_err(storage_error)
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

    /// Atomically persist a candidate v3 chain snapshot together with both the
    /// historical protocol sidecar and the policy+cadence monetary sidecar.
    ///
    /// The supplied identity must already match the state's chain/genesis/DAG
    /// ordering. This helper does not activate consensus; it only makes a future
    /// v3 restore fail closed on identity, policy, or cadence substitution.
    pub fn persist_chain_state_with_monetary_protocol_record(
        &self,
        state: &pulsedag_core::ChainState,
        identity: &ProtocolActivationIdentity,
        cadence_segments: &[MonetaryCadenceSegment],
    ) -> Result<ProtocolMonetaryActivationRecordV2, PulseError> {
        identity.validate().map_err(storage_error)?;
        if identity.chain_id != state.chain_id {
            return Err(PulseError::ChainIdMismatch);
        }
        if identity.genesis_hash != state.dag.genesis_hash {
            return Err(storage_error(
                "v3 monetary activation identity genesis does not match chain state",
            ));
        }
        if identity.dag_ordering_version != state.dag.ordering_version {
            return Err(storage_error(
                "v3 monetary activation identity DAG ordering does not match chain state",
            ));
        }

        let protocol_record =
            ProtocolActivationRecordV1::from_identity(identity.clone()).map_err(storage_error)?;
        let monetary_record = ProtocolMonetaryActivationRecordV2::from_identity_and_cadence(
            identity.clone(),
            cadence_segments,
        )
        .map_err(storage_error)?;
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let mut batch = WriteBatch::default();

        self.stage_chain_state_snapshot(&mut batch, &meta_cf, state)?;
        batch.put_cf(
            &meta_cf,
            PROTOCOL_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&protocol_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        batch.put_cf(
            &meta_cf,
            PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY,
            serde_json::to_vec(&monetary_record)
                .map_err(|error| storage_error(error.to_string()))?,
        );
        self.db
            .write(batch)
            .map_err(|error| storage_error(error.to_string()))?;
        Ok(monetary_record)
    }

    /// Return whether snapshot + protocol sidecar + monetary sidecar are
    /// durably present together. Semantic authorization still requires
    /// verify_persisted_monetary_identity.
    pub fn monetary_protocol_snapshot_sidecar_complete(&self) -> Result<bool, PulseError> {
        let meta_cf = self
            .db
            .cf_handle("meta")
            .ok_or_else(|| storage_error("missing cf meta"))?;
        let snapshot_present = self
            .db
            .get_cf(&meta_cf, CHAIN_STATE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
            .is_some();
        let protocol_present = self
            .db
            .get_cf(&meta_cf, PROTOCOL_ACTIVATION_STORAGE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
            .is_some();
        let monetary_present = self
            .db
            .get_cf(&meta_cf, PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY)
            .map_err(|error| storage_error(error.to_string()))?
            .is_some();
        Ok(snapshot_present && protocol_present && monetary_present)
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
        genesis::init_chain_state, init_chain_state_v3,
        ordering_v2::GHOSTDAG_V1_ORDERING_VERSION, MonetaryCadenceSegment,
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

    const V3_TEST_CADENCE: [MonetaryCadenceSegment; 1] = [MonetaryCadenceSegment {
        activation_score: 0,
        target_interval_ns: 1_000_000_000,
    }];

    #[test]
    fn monetary_snapshot_and_both_sidecars_round_trip_atomically() {
        let path = temp_db_path("monetary-round-trip");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state_v3(
            "pulsedag-v3-storage-candidate".to_string(),
            1_800_000_000,
        )
        .unwrap();
        let expected = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        let written = storage
            .persist_chain_state_with_monetary_protocol_record(
                &state,
                &expected,
                &V3_TEST_CADENCE,
            )
            .unwrap();
        let loaded = storage
            .protocol_monetary_activation_record()
            .unwrap()
            .unwrap();

        assert_eq!(loaded, written);
        assert_eq!(
            storage
                .protocol_activation_record()
                .unwrap()
                .unwrap()
                .identity,
            expected
        );
        assert!(storage.monetary_protocol_snapshot_sidecar_complete().unwrap());
        storage
            .verify_persisted_monetary_identity(&expected, &V3_TEST_CADENCE)
            .unwrap();

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn missing_v3_monetary_sidecar_fails_closed() {
        let path = temp_db_path("monetary-missing");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state_v3(
            "pulsedag-v3-storage-missing".to_string(),
            1_800_000_001,
        )
        .unwrap();
        let expected = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert!(storage
            .verify_persisted_monetary_identity(&expected, &V3_TEST_CADENCE)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn persisted_monetary_cadence_substitution_fails_closed() {
        let path = temp_db_path("monetary-cadence-drift");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state_v3(
            "pulsedag-v3-storage-cadence".to_string(),
            1_800_000_002,
        )
        .unwrap();
        let expected = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        storage
            .persist_chain_state_with_monetary_protocol_record(
                &state,
                &expected,
                &V3_TEST_CADENCE,
            )
            .unwrap();

        let alternate = [MonetaryCadenceSegment {
            activation_score: 0,
            target_interval_ns: 500_000_000,
        }];
        assert!(storage
            .verify_persisted_monetary_identity(&expected, &alternate)
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn corrupted_monetary_sidecar_fails_closed() {
        let path = temp_db_path("monetary-corrupt");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state_v3(
            "pulsedag-v3-storage-corrupt".to_string(),
            1_800_000_003,
        )
        .unwrap();
        let expected = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        storage
            .persist_chain_state_with_monetary_protocol_record(
                &state,
                &expected,
                &V3_TEST_CADENCE,
            )
            .unwrap();

        let mut record = storage
            .protocol_monetary_activation_record()
            .unwrap()
            .unwrap();
        record.monetary_cadence_segments[0].target_interval_ns = 250_000_000;
        let meta_cf = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(
                &meta_cf,
                PROTOCOL_MONETARY_ACTIVATION_STORAGE_KEY,
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();

        assert!(storage.protocol_monetary_activation_record().is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

}
