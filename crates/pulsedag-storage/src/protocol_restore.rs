use pulsedag_core::{
    errors::PulseError, genesis::init_chain_state, ProtocolActivationIdentity,
    ProtocolRestoreIdentityGate,
};

use super::Storage;

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn verify_current_legacy_runtime_identity(
    expected: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    expected.validate().map_err(storage_error)?;
    let canonical_state = init_chain_state(expected.chain_id.clone());
    let canonical = ProtocolActivationIdentity::legacy_from_state(&canonical_state);
    if &canonical != expected {
        return Err(storage_error(
            "protocol-bound restore execution is only available for the canonical current legacy runtime identity; refusing legacy fallback",
        ));
    }
    Ok(())
}

impl Storage {
    /// Preflight durable identity and runtime compatibility before delegating to
    /// any historical restore/fallback implementation.
    ///
    /// Exact activated-v2 records are still rejected here because the current
    /// replay engine is legacy-v1. This prevents a valid future identity from
    /// accidentally selecting legacy rebuild semantics merely because its
    /// sidecar was well formed.
    pub fn verify_protocol_restore_preflight(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<ProtocolRestoreIdentityGate, PulseError> {
        let gate = self.verify_persisted_protocol_identity(expected)?;
        verify_current_legacy_runtime_identity(expected)?;
        Ok(gate)
    }

    /// Protocol-bound wrapper around historical startup restore.
    ///
    /// Only the canonical current legacy runtime identity may delegate to the
    /// existing fallback path. A successful restore is promoted to an explicit
    /// snapshot+activation-record pair before returning.
    pub fn load_or_init_genesis_for_protocol(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<pulsedag_core::ChainState, PulseError> {
        self.verify_protocol_restore_preflight(expected)?;
        let state = self.load_or_init_genesis(expected.chain_id.clone())?;
        let actual = ProtocolActivationIdentity::legacy_from_state(&state);
        if &actual != expected {
            return Err(storage_error(
                "restored chain state does not match expected protocol activation identity",
            ));
        }
        self.persist_chain_state_with_protocol_record(&state)?;
        Ok(state)
    }

    /// Protocol-bound wrapper around historical replay/rebuild.
    ///
    /// The durable activation identity is checked before the legacy method can
    /// inspect a snapshot or engage full-rebuild fallback. Activated-v2 and any
    /// non-canonical identity therefore fail before mutation.
    pub fn replay_blocks_or_init_for_protocol(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<pulsedag_core::ChainState, PulseError> {
        self.verify_protocol_restore_preflight(expected)?;
        let state = self.replay_blocks_or_init(expected.chain_id.clone())?;
        let actual = ProtocolActivationIdentity::legacy_from_state(&state);
        if &actual != expected {
            return Err(storage_error(
                "replayed chain state does not match expected protocol activation identity",
            ));
        }
        self.persist_chain_state_with_protocol_record(&state)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{ordering_v2::GHOSTDAG_V1_ORDERING_VERSION, ProtocolActivationRecordV1};

    use crate::protocol_identity::PROTOCOL_ACTIVATION_STORAGE_KEY;

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!(
                "pulsedag-storage-protocol-restore-{test_name}-{unique}"
            ))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn canonical_legacy_startup_promotes_schema1_to_explicit_record() {
        let path = temp_db_path("legacy-startup");
        let storage = Storage::open(&path).unwrap();
        let canonical = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&canonical);

        assert_eq!(
            storage
                .verify_protocol_restore_preflight(&expected)
                .unwrap(),
            ProtocolRestoreIdentityGate::LegacySchema1Compatibility
        );
        let restored = storage
            .load_or_init_genesis_for_protocol(&expected)
            .unwrap();

        assert_eq!(restored.chain_id, canonical.chain_id);
        assert_eq!(
            storage
                .verify_persisted_protocol_identity(&expected)
                .unwrap(),
            ProtocolRestoreIdentityGate::VerifiedRecordV1
        );
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
    fn activated_v2_missing_record_fails_before_genesis_initialization() {
        let path = temp_db_path("activated-missing");
        let storage = Storage::open(&path).unwrap();
        let canonical = init_chain_state("pulsedag-testnet-v2".to_string());
        let expected = ProtocolActivationIdentity::activated_v2(
            canonical.chain_id.clone(),
            canonical.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );

        assert!(storage
            .load_or_init_genesis_for_protocol(&expected)
            .is_err());
        assert!(!storage.snapshot_exists().unwrap());
        assert!(storage.protocol_activation_record().unwrap().is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn exact_activated_v2_record_still_cannot_delegate_to_legacy_replay() {
        let path = temp_db_path("activated-record");
        let storage = Storage::open(&path).unwrap();
        let canonical = init_chain_state("pulsedag-testnet-v2".to_string());
        let expected = ProtocolActivationIdentity::activated_v2(
            canonical.chain_id.clone(),
            canonical.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        let record = ProtocolActivationRecordV1::from_identity(expected.clone()).unwrap();
        let meta_cf = storage.db.cf_handle("meta").unwrap();
        storage
            .db
            .put_cf(
                &meta_cf,
                PROTOCOL_ACTIVATION_STORAGE_KEY,
                serde_json::to_vec(&record).unwrap(),
            )
            .unwrap();

        assert_eq!(
            storage
                .verify_persisted_protocol_identity(&expected)
                .unwrap(),
            ProtocolRestoreIdentityGate::VerifiedRecordV1
        );
        assert!(storage
            .replay_blocks_or_init_for_protocol(&expected)
            .is_err());
        assert!(!storage.snapshot_exists().unwrap());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn mismatched_durable_identity_fails_before_existing_target_is_replaced() {
        let path = temp_db_path("mismatch");
        let storage = Storage::open(&path).unwrap();
        let target_state = init_chain_state("pulsedag-private".to_string());
        storage
            .persist_chain_state_with_protocol_record(&target_state)
            .unwrap();
        let source_state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&source_state);

        assert!(storage
            .replay_blocks_or_init_for_protocol(&expected)
            .is_err());
        assert_eq!(
            storage.load_chain_state().unwrap().unwrap().chain_id,
            target_state.chain_id
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
