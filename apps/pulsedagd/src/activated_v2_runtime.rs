use anyhow::Result;
use pulsedag_core::{ActivatedV2P2pRuntime, ChainState, ProtocolActivationIdentity};
use pulsedag_p2p::messages::ProtocolCapabilitiesV1;
use pulsedag_storage::Storage;

use crate::block_protocol::resolve_activated_v2_runtime_restore_identity;

/// Restore the transient activated-v2 P2P runtime only when the local P2P
/// capabilities explicitly select the exact canonical activated-v2 identity.
///
/// Sidecar presence is deliberately not consulted when capabilities are absent,
/// preserving the historical startup path. Once activated-v2 capabilities are
/// explicit, the storage restore is strict: a missing/corrupt/mismatched sidecar
/// is a startup error rather than an implicit fallback or first activation.
pub fn restore_activated_v2_p2p_runtime_for_startup(
    storage: &Storage,
    capabilities: Option<&ProtocolCapabilitiesV1>,
    state: ChainState,
) -> Result<(
    ChainState,
    ActivatedV2P2pRuntime,
    Option<ProtocolActivationIdentity>,
)> {
    let identity = resolve_activated_v2_runtime_restore_identity(capabilities, &state)
        .map_err(anyhow::Error::msg)?;
    let Some(identity) = identity else {
        return Ok((state, ActivatedV2P2pRuntime::default(), None));
    };

    let (restored_state, restored_runtime) =
        storage.load_activated_v2_p2p_runtime_snapshot(&identity)?;
    Ok((restored_state, restored_runtime, Some(identity)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        finality_v2::GHOSTDAG_V1_FINALITY_POLICY_VERSION, genesis::init_chain_state,
        materialize_authoritative_state_v2, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_ORDERING_VERSION,
    };
    use pulsedag_p2p::messages::P2P_PROTOCOL_CAPABILITIES_VERSION;

    fn temp_db_path(name: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("pulsedagd-task28-v2-runtime-{name}-{nanos}"))
            .to_string_lossy()
            .into_owned()
    }

    fn activated_state() -> (ChainState, ProtocolActivationIdentity) {
        let mut state = init_chain_state("task28-daemon-runtime-restore".to_string());
        let genesis = state.dag.genesis_hash.clone();
        state.dag.merge_set_blues.insert(genesis.clone(), vec![]);
        state.dag.merge_set_reds.insert(genesis, vec![]);
        let state = materialize_authoritative_state_v2(&state).unwrap();
        let identity = ProtocolActivationIdentity::activated_v2(
            state.chain_id.clone(),
            state.dag.genesis_hash.clone(),
            GHOSTDAG_V1_ORDERING_VERSION,
        );
        (state, identity)
    }

    fn activated_capabilities(identity: ProtocolActivationIdentity) -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: identity,
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
            supports_dag_frontier: true,
            supports_consensus_metadata: true,
            high_cadence_allowed: false,
        }
    }

    #[test]
    fn no_capabilities_leave_startup_state_unchanged_and_ignore_missing_sidecar() {
        let path = temp_db_path("legacy-default");
        let storage = Storage::open(&path).unwrap();
        let state = init_chain_state("task28-daemon-legacy-default".to_string());
        let genesis = state.dag.genesis_hash.clone();

        let (restored_state, runtime, identity) =
            restore_activated_v2_p2p_runtime_for_startup(&storage, None, state).unwrap();

        assert_eq!(restored_state.dag.genesis_hash, genesis);
        assert!(runtime.pending_is_empty());
        assert!(runtime.staging().is_empty());
        assert!(identity.is_none());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn exact_capabilities_without_durable_sidecar_fail_closed() {
        let path = temp_db_path("missing-sidecar");
        let storage = Storage::open(&path).unwrap();
        let (state, identity) = activated_state();
        let capabilities = activated_capabilities(identity);

        let error = restore_activated_v2_p2p_runtime_for_startup(
            &storage,
            Some(&capabilities),
            state,
        )
        .expect_err("explicit activated-v2 capability must require a durable sidecar");

        assert!(error.to_string().contains("activation record") || error.to_string().contains("sidecar"));
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn exact_capabilities_restore_matching_durable_runtime() {
        let path = temp_db_path("exact-restore");
        let storage = Storage::open(&path).unwrap();
        let (state, identity) = activated_state();
        let genesis = state.dag.genesis_hash.clone();
        storage
            .persist_block(state.dag.blocks.get(&genesis).unwrap())
            .unwrap();
        let runtime = ActivatedV2P2pRuntime::default();
        storage
            .persist_activated_v2_p2p_runtime_snapshot(&identity, &state, &runtime)
            .unwrap();
        let capabilities = activated_capabilities(identity.clone());

        let (restored_state, restored_runtime, restored_identity) =
            restore_activated_v2_p2p_runtime_for_startup(
                &storage,
                Some(&capabilities),
                state,
            )
            .unwrap();

        assert_eq!(restored_identity, Some(identity));
        assert_eq!(restored_state.dag.genesis_hash, genesis);
        assert!(restored_runtime.pending_is_empty());
        assert!(restored_runtime.staging().is_empty());
        let _ = std::fs::remove_dir_all(path);
    }
}
