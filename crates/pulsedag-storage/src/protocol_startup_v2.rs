use pulsedag_core::{
    errors::PulseError, genesis_v2::init_chain_state_v2, genesis_v3::init_chain_state_v3,
    ActivatedV2P2pRuntime, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
    PRODUCTION_CADENCE_V3, REWARD_FINALITY_POLICY_VERSION_V3,
};

use super::Storage;

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn derived_v2_identity(state: &pulsedag_core::ChainState) -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

impl Storage {
    /// Load an exact activated-v2 startup snapshot, or initialize one only when
    /// the database is completely empty. Existing schema-1/legacy state is never
    /// promoted implicitly to the v2 protocol identity.
    pub fn load_or_init_activated_v2_p2p_runtime(
        &self,
        expected: &ProtocolActivationIdentity,
    ) -> Result<(pulsedag_core::ChainState, ActivatedV2P2pRuntime), PulseError> {
        expected.validate().map_err(storage_error)?;

        if self.protocol_activation_record()?.is_some() {
            return self.load_activated_v2_p2p_runtime_snapshot(expected);
        }

        if self.activated_v2_p2p_runtime_record()?.is_some() {
            return Err(storage_error(
                "activated-v2 runtime sidecar exists without protocol activation record",
            ));
        }

        if self.load_chain_state()?.is_some() || !self.list_blocks()?.is_empty() {
            return Err(storage_error(
                "non-empty storage is missing an activated-v2 protocol record; refusing implicit migration",
            ));
        }

        let state = init_chain_state_v2(expected.chain_id.clone())?;
        let derived = derived_v2_identity(&state);
        if &derived != expected {
            return Err(storage_error(
                "clean v2 genesis does not match the expected protocol activation identity",
            ));
        }

        let genesis = state
            .dag
            .blocks
            .get(&state.dag.genesis_hash)
            .cloned()
            .ok_or_else(|| {
                storage_error("clean v2 genesis block missing from initialized state")
            })?;
        let runtime = ActivatedV2P2pRuntime::default();
        self.persist_activated_v2_p2p_blocks_and_runtime(
            std::slice::from_ref(&genesis),
            expected,
            &state,
            &runtime,
        )?;

        self.load_activated_v2_p2p_runtime_snapshot(expected)
    }

    /// Load the exact production-v3 monetary runtime, or initialize it only
    /// from a completely empty database using the caller-supplied frozen
    /// genesis timestamp. No legacy/v2 state is promoted implicitly.
    pub fn load_or_init_production_v3_p2p_runtime(
        &self,
        expected: &ProtocolActivationIdentity,
        frozen_genesis_timestamp: u64,
    ) -> Result<(pulsedag_core::ChainState, ActivatedV2P2pRuntime), PulseError> {
        expected.validate().map_err(storage_error)?;

        let clean_state =
            init_chain_state_v3(expected.chain_id.clone(), frozen_genesis_timestamp)?;
        let derived = derived_v2_identity(&clean_state);
        if &derived != expected {
            return Err(storage_error(
                "clean production-v3 genesis does not match the expected protocol activation identity",
            ));
        }

        if let Some(record) = self.protocol_monetary_activation_record()? {
            record.verify_production_v3(expected).map_err(storage_error)?;
            return self.load_monetary_v3_p2p_runtime_snapshot(
                expected,
                &PRODUCTION_CADENCE_V3,
                REWARD_FINALITY_POLICY_VERSION_V3,
            );
        }

        if self.protocol_activation_record()?.is_some() {
            return Err(storage_error(
                "production-v3 protocol sidecar exists without canonical monetary sidecar",
            ));
        }
        if self.activated_v2_p2p_runtime_record()?.is_some() {
            return Err(storage_error(
                "production-v3 runtime sidecar exists without canonical monetary sidecar",
            ));
        }
        if self.load_chain_state()?.is_some() || !self.list_blocks()?.is_empty() {
            return Err(storage_error(
                "non-empty storage is missing the production-v3 monetary sidecar; refusing implicit migration",
            ));
        }

        let genesis = clean_state
            .dag
            .blocks
            .get(&clean_state.dag.genesis_hash)
            .cloned()
            .ok_or_else(|| {
                storage_error("clean production-v3 genesis block missing from initialized state")
            })?;
        let runtime = ActivatedV2P2pRuntime::default();
        self.persist_production_v3_genesis_and_runtime(
            &genesis,
            expected,
            &clean_state,
            &runtime,
        )?;

        self.load_monetary_v3_p2p_runtime_snapshot(
            expected,
            &PRODUCTION_CADENCE_V3,
            REWARD_FINALITY_POLICY_VERSION_V3,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, genesis_v2::init_chain_state_v2, genesis_v3::init_chain_state_v3,
        BLOCK_HEADER_VERSION_V2, PRODUCTION_CADENCE_FINGERPRINT_V3, TRANSACTION_VERSION_V2,
    };

    fn temp_db_path(test_name: &str) -> String {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedag-storage-startup-v2-{test_name}-{unique}"))
            .to_string_lossy()
            .into_owned()
    }

    fn expected_identity(chain_id: &str) -> ProtocolActivationIdentity {
        let state = init_chain_state_v2(chain_id.to_string()).unwrap();
        derived_v2_identity(&state)
    }

    const PRODUCTION_V3_TS: u64 = 1_800_000_321;

    fn expected_production_v3_identity(chain_id: &str) -> ProtocolActivationIdentity {
        let state = init_chain_state_v3(chain_id.to_string(), PRODUCTION_V3_TS).unwrap();
        derived_v2_identity(&state)
    }

    #[test]
    fn empty_database_bootstraps_exact_production_v3_genesis_and_sidecars() {
        let path = temp_db_path("production-v3-clean-bootstrap");
        let storage = Storage::open(&path).unwrap();
        let expected = expected_production_v3_identity("pulsedag-v3-production-candidate");

        let (state, runtime) = storage
            .load_or_init_production_v3_p2p_runtime(&expected, PRODUCTION_V3_TS)
            .unwrap();
        let genesis = &state.dag.blocks[&state.dag.genesis_hash];

        assert_eq!(derived_v2_identity(&state), expected);
        assert_eq!(genesis.header.version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(genesis.header.timestamp, PRODUCTION_V3_TS);
        assert!(genesis.transactions.is_empty());
        assert!(state.utxo.utxos.is_empty());
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().is_empty());
        assert!(storage
            .monetary_protocol_snapshot_sidecar_complete()
            .unwrap());
        assert!(storage.activated_v2_p2p_runtime_record().unwrap().is_some());
        assert_eq!(storage.list_blocks().unwrap().len(), 1);

        let monetary = storage
            .protocol_monetary_activation_record()
            .unwrap()
            .unwrap();
        monetary.verify_production_v3(&expected).unwrap();
        assert_eq!(
            monetary.monetary_cadence_fingerprint,
            PRODUCTION_CADENCE_FINGERPRINT_V3
        );
        assert_eq!(
            monetary.reward_finality_policy_version,
            REWARD_FINALITY_POLICY_VERSION_V3
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn production_v3_restart_requires_same_identity_and_frozen_timestamp() {
        let path = temp_db_path("production-v3-restart");
        let expected = expected_production_v3_identity("pulsedag-v3-production-restart");

        let storage = Storage::open(&path).unwrap();
        let first = storage
            .load_or_init_production_v3_p2p_runtime(&expected, PRODUCTION_V3_TS)
            .unwrap()
            .0;
        drop(storage);

        let storage = Storage::open(&path).unwrap();
        let second = storage
            .load_or_init_production_v3_p2p_runtime(&expected, PRODUCTION_V3_TS)
            .unwrap()
            .0;
        assert_eq!(second.dag.genesis_hash, first.dag.genesis_hash);
        assert_eq!(
            second.dag.ordered_dag_state_root,
            first.dag.ordered_dag_state_root
        );

        assert!(storage
            .load_or_init_production_v3_p2p_runtime(
                &expected,
                PRODUCTION_V3_TS.saturating_add(1),
            )
            .is_err());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn production_v3_wrong_timestamp_fails_before_any_state_is_persisted() {
        let path = temp_db_path("production-v3-timestamp-mismatch");
        let storage = Storage::open(&path).unwrap();
        let expected = expected_production_v3_identity("pulsedag-v3-production-timestamp");

        assert!(storage
            .load_or_init_production_v3_p2p_runtime(
                &expected,
                PRODUCTION_V3_TS.saturating_add(1),
            )
            .is_err());
        assert!(storage.load_chain_state().unwrap().is_none());
        assert!(storage.protocol_activation_record().unwrap().is_none());
        assert!(storage
            .protocol_monetary_activation_record()
            .unwrap()
            .is_none());
        assert!(storage.activated_v2_p2p_runtime_record().unwrap().is_none());
        assert!(storage.list_blocks().unwrap().is_empty());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn production_v3_refuses_nonempty_storage_without_monetary_sidecar() {
        let path = temp_db_path("production-v3-no-implicit-migration");
        let storage = Storage::open(&path).unwrap();
        let legacy = init_chain_state("pulsedag-v3-no-implicit-migration".to_string());
        storage.persist_chain_state(&legacy).unwrap();
        let expected =
            expected_production_v3_identity("pulsedag-v3-no-implicit-migration");

        let error = storage
            .load_or_init_production_v3_p2p_runtime(&expected, PRODUCTION_V3_TS)
            .expect_err("nonempty legacy state must never be promoted to production-v3");
        assert!(error.to_string().contains("refusing implicit migration"));
        assert!(storage
            .protocol_monetary_activation_record()
            .unwrap()
            .is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn empty_database_bootstraps_exact_chain_bound_v2_genesis() {
        let path = temp_db_path("clean-bootstrap");
        let storage = Storage::open(&path).unwrap();
        let expected = expected_identity("pulsedag-private-v2.4.0");

        let (state, runtime) = storage
            .load_or_init_activated_v2_p2p_runtime(&expected)
            .unwrap();
        let genesis = &state.dag.blocks[&state.dag.genesis_hash];

        assert_eq!(derived_v2_identity(&state), expected);
        assert_eq!(genesis.header.version, BLOCK_HEADER_VERSION_V2);
        assert_eq!(genesis.transactions[0].version, TRANSACTION_VERSION_V2);
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().is_empty());
        assert!(storage.protocol_snapshot_sidecar_complete().unwrap());
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
    fn second_startup_requires_and_restores_the_exact_persisted_identity() {
        let path = temp_db_path("restart");
        let storage = Storage::open(&path).unwrap();
        let expected = expected_identity("pulsedag-private-v2.4.0");
        let first = storage
            .load_or_init_activated_v2_p2p_runtime(&expected)
            .unwrap()
            .0;
        drop(storage);

        let storage = Storage::open(&path).unwrap();
        let second = storage
            .load_or_init_activated_v2_p2p_runtime(&expected)
            .unwrap()
            .0;
        assert_eq!(second.dag.genesis_hash, first.dag.genesis_hash);
        assert_eq!(
            second.dag.ordered_dag_state_root,
            first.dag.ordered_dag_state_root
        );

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn nonempty_legacy_storage_without_v2_sidecar_is_not_promoted() {
        let path = temp_db_path("legacy-refusal");
        let storage = Storage::open(&path).unwrap();
        let legacy = init_chain_state("pulsedag-private-v2.4.0".to_string());
        storage.persist_chain_state(&legacy).unwrap();
        let expected = expected_identity("pulsedag-private-v2.4.0");

        let error = storage
            .load_or_init_activated_v2_p2p_runtime(&expected)
            .expect_err("legacy state without activated sidecar must fail closed");
        assert!(error.to_string().contains("refusing implicit migration"));
        assert!(storage.protocol_activation_record().unwrap().is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn mismatched_expected_genesis_fails_before_any_state_is_persisted() {
        let path = temp_db_path("mismatch");
        let storage = Storage::open(&path).unwrap();
        let mut expected = expected_identity("pulsedag-private-v2.4.0");
        expected.genesis_hash = "00".repeat(32);

        assert!(storage
            .load_or_init_activated_v2_p2p_runtime(&expected)
            .is_err());
        assert!(storage.load_chain_state().unwrap().is_none());
        assert!(storage.protocol_activation_record().unwrap().is_none());

        drop(storage);
        let _ = std::fs::remove_dir_all(path);
    }
}
