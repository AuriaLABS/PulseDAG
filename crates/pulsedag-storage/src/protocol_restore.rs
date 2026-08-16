use pulsedag_core::{
    errors::PulseError, genesis::init_chain_state, ProtocolActivationIdentity,
    ProtocolRestoreIdentityGate,
};

use super::{protocol_identity::canonical_current_legacy_identity_from_state, Storage};

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn verify_current_legacy_runtime_identity(
    expected: &ProtocolActivationIdentity,
) -> Result<(), PulseError> {
    expected.validate().map_err(storage_error)?;
    let canonical_state = init_chain_state(expected.chain_id.clone());
    let canonical = canonical_current_legacy_identity_from_state(&canonical_state)?;
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

    /// Protocol-bound startup restore for the canonical current Legacy runtime.
    ///
    /// Any snapshot decode fallback is rebuilt in memory and checked against
    /// the expected activation identity before a chain snapshot is published.
    /// Historical schema-1 state is promoted to an explicit activation record
    /// only after that check succeeds.
    pub fn load_or_init_genesis_for_protocol(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<pulsedag_core::ChainState, PulseError> {
        self.verify_protocol_restore_preflight(expected)?;

        let mut initialized_genesis = false;
        let state = match self.load_chain_state() {
            Ok(Some(state)) => state,
            Ok(None) => {
                let blocks = self.list_blocks()?;
                if blocks.is_empty() {
                    initialized_genesis = true;
                    init_chain_state(expected.chain_id.clone())
                } else {
                    pulsedag_core::rebuild_state_from_blocks(expected.chain_id.clone(), blocks)?
                }
            }
            Err(snapshot_err) => {
                let blocks = self.list_blocks()?;
                if blocks.is_empty() {
                    return Err(snapshot_err);
                }
                let _ = self.append_runtime_event(
                    "warn",
                    "startup_snapshot_decode_failed_fallback_full",
                    &format!(
                        "startup snapshot decode failed and full rebuild fallback engaged: {}",
                        snapshot_err
                    ),
                );
                pulsedag_core::rebuild_state_from_blocks(expected.chain_id.clone(), blocks)?
            }
        };

        let actual = canonical_current_legacy_identity_from_state(&state)?;
        if &actual != expected {
            return Err(storage_error(
                "restored chain state does not match expected protocol activation identity",
            ));
        }

        if initialized_genesis {
            for block in state.dag.blocks.values() {
                self.persist_block_and_chain_state(block, &state)?;
            }
        }
        self.persist_chain_state_with_protocol_record(&state)?;
        Ok(state)
    }

    /// Protocol-bound replay/rebuild for the canonical current Legacy runtime.
    ///
    /// Snapshot+delta replay and full-rebuild fallback both remain in-memory
    /// until the rebuilt state matches the expected protocol identity. This
    /// prevents a decodable but semantically mismatched snapshot from replacing
    /// durable state before the protocol gate can reject it.
    pub fn replay_blocks_or_init_for_protocol(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<pulsedag_core::ChainState, PulseError> {
        self.verify_protocol_restore_preflight(expected)?;

        let blocks = self.list_blocks()?;
        let state = match self.load_chain_state() {
            Ok(Some(snapshot)) => {
                match pulsedag_core::rebuild_state_from_snapshot_and_blocks(
                    snapshot,
                    blocks.clone(),
                ) {
                    Ok(state) => state,
                    Err(snapshot_delta_err) => {
                        if blocks.is_empty() {
                            return Err(snapshot_delta_err);
                        }
                        let _ = self.append_runtime_event(
                            "warn",
                            "snapshot_delta_replay_failed_fallback_full",
                            &format!(
                                "snapshot+delta replay failed and full rebuild fallback engaged: {}",
                                snapshot_delta_err
                            ),
                        );
                        pulsedag_core::rebuild_state_from_blocks(expected.chain_id.clone(), blocks)?
                    }
                }
            }
            Ok(None) => {
                if blocks.is_empty() {
                    return self.load_or_init_genesis_for_protocol(expected);
                }
                pulsedag_core::rebuild_state_from_blocks(expected.chain_id.clone(), blocks)?
            }
            Err(snapshot_err) => {
                if blocks.is_empty() {
                    return Err(snapshot_err);
                }
                let _ = self.append_runtime_event(
                    "warn",
                    "snapshot_decode_failed_fallback_full",
                    &format!(
                        "snapshot decode failed and full rebuild fallback engaged: {}",
                        snapshot_err
                    ),
                );
                pulsedag_core::rebuild_state_from_blocks(expected.chain_id.clone(), blocks)?
            }
        };

        let actual = canonical_current_legacy_identity_from_state(&state)?;
        if &actual != expected {
            return Err(storage_error(
                "replayed chain state does not match expected protocol activation identity",
            ));
        }
        self.persist_chain_state_with_protocol_record(&state)?;
        Ok(state)
    }

    /// Protocol-bound wrapper around validated snapshot+delta replay.
    ///
    /// The durable identity is checked before restore inputs are consumed. The
    /// snapshot+delta state is reconstructed in memory, validated against the
    /// expected protocol identity, and only then atomically persisted together
    /// with its activation record. Identity failure therefore cannot publish a
    /// rebuilt snapshot after an auto-prune operation.
    pub fn replay_from_validated_snapshot_and_delta_for_protocol(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<pulsedag_core::ChainState, PulseError> {
        self.verify_protocol_restore_preflight(expected)?;
        let (snapshot, blocks) = self.validate_restore_inputs(Some(&expected.chain_id))?;
        let state = pulsedag_core::rebuild_state_from_snapshot_and_blocks(snapshot, blocks)?;
        let actual = canonical_current_legacy_identity_from_state(&state)?;
        if &actual != expected {
            return Err(storage_error(
                "snapshot+delta replay state does not match expected protocol activation identity",
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
    fn post_genesis_legacy_ordering_marker_restores_under_canonical_identity() {
        let path = temp_db_path("legacy-post-genesis-ordering");
        let storage = Storage::open(&path).unwrap();
        let mut state = init_chain_state("pulsedag-testnet".to_string());
        let expected = ProtocolActivationIdentity::legacy_from_state(&state);
        state.dag.ordering_version = "legacy".to_string();
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();

        let restored = storage
            .load_or_init_genesis_for_protocol(&expected)
            .unwrap();

        assert_eq!(restored.dag.ordering_version, "legacy");
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
    fn replay_blocks_identity_failure_does_not_publish_rebuilt_state() {
        let path = temp_db_path("replay-blocks-prepublish-identity-gate");
        let storage = Storage::open(&path).unwrap();
        let canonical = init_chain_state("pulsedag-testnet".to_string());
        let expected = canonical_current_legacy_identity_from_state(&canonical).unwrap();
        storage
            .persist_chain_state_with_protocol_record(&canonical)
            .unwrap();
        let record_before = storage.protocol_activation_record().unwrap().unwrap();

        let mut mismatched_snapshot = canonical.clone();
        mismatched_snapshot.dag.ordering_version = "unexpected-ordering".to_string();
        storage.persist_chain_state(&mismatched_snapshot).unwrap();

        let error = storage
            .replay_blocks_or_init_for_protocol(&expected)
            .expect_err("unsupported rebuilt identity must fail before publication");
        assert!(error
            .to_string()
            .contains("unsupported current-legacy DAG ordering identity"));
        assert_eq!(
            storage.protocol_activation_record().unwrap().unwrap(),
            record_before,
            "failed rebuild must not replace the verified activation sidecar"
        );
        assert_eq!(
            storage
                .load_chain_state()
                .unwrap()
                .unwrap()
                .dag
                .ordering_version,
            "unexpected-ordering",
            "failed rebuild must not publish a replacement snapshot"
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn validated_snapshot_delta_replay_accepts_historical_legacy_ordering_marker() {
        let path = temp_db_path("legacy-snapshot-delta-ordering");
        let storage = Storage::open(&path).unwrap();
        let mut state = init_chain_state("pulsedag-testnet".to_string());
        let expected = canonical_current_legacy_identity_from_state(&state).unwrap();
        state.dag.ordering_version = "legacy".to_string();
        storage
            .persist_chain_state_with_protocol_record(&state)
            .unwrap();

        let restored = storage
            .replay_from_validated_snapshot_and_delta_for_protocol(&expected)
            .unwrap();

        assert_eq!(restored.dag.ordering_version, "legacy");
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
    fn snapshot_delta_identity_failure_does_not_publish_rebuilt_state() {
        let path = temp_db_path("snapshot-delta-prepublish-identity-gate");
        let storage = Storage::open(&path).unwrap();
        let canonical = init_chain_state("pulsedag-testnet".to_string());
        let expected = canonical_current_legacy_identity_from_state(&canonical).unwrap();
        storage
            .persist_chain_state_with_protocol_record(&canonical)
            .unwrap();
        let record_before = storage.protocol_activation_record().unwrap().unwrap();

        let mut mismatched_snapshot = canonical.clone();
        mismatched_snapshot.dag.ordering_version = "unexpected-ordering".to_string();
        storage.persist_chain_state(&mismatched_snapshot).unwrap();

        let error = storage
            .replay_from_validated_snapshot_and_delta_for_protocol(&expected)
            .expect_err("unsupported rebuilt identity must fail before publication");
        assert!(error
            .to_string()
            .contains("unsupported current-legacy DAG ordering identity"));

        assert_eq!(
            storage.protocol_activation_record().unwrap().unwrap(),
            record_before,
            "failed replay must not replace the verified activation sidecar"
        );
        assert_eq!(
            storage
                .load_chain_state()
                .unwrap()
                .unwrap()
                .dag
                .ordering_version,
            "unexpected-ordering",
            "failed replay must not publish a reconstructed replacement snapshot"
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
