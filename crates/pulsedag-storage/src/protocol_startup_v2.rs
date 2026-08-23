use pulsedag_core::{
    errors::PulseError, genesis_v2::init_chain_state_v2, ActivatedV2P2pRuntime,
    ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};

use super::Storage;

fn storage_error(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(message.into())
}

fn derived_v2_identity(
    state: &pulsedag_core::ChainState,
) -> ProtocolActivationIdentity {
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
            .ok_or_else(|| storage_error("clean v2 genesis block missing from initialized state"))?;
        let runtime = ActivatedV2P2pRuntime::default();
        self.persist_activated_v2_p2p_blocks_and_runtime(
            std::slice::from_ref(&genesis),
            expected,
            &state,
            &runtime,
        )?;

        self.load_activated_v2_p2p_runtime_snapshot(expected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        genesis::init_chain_state, genesis_v2::init_chain_state_v2,
        BLOCK_HEADER_VERSION_V2, TRANSACTION_VERSION_V2,
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
        assert_eq!(second.dag.ordered_dag_state_root, first.dag.ordered_dag_state_root);

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
