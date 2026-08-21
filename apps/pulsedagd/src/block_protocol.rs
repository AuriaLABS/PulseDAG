use pulsedag_core::{
    ChainState, ProtocolActivationIdentity, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_p2p::messages::ProtocolCapabilitiesV1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundP2pBlockProtocol {
    Legacy,
    ActivatedV2(ProtocolActivationIdentity),
}

/// Select the inbound P2P block validation path from the locally configured
/// protocol capability identity.
///
/// No local capabilities means the exact historical v1 path. Once capabilities
/// are configured, the selector is intentionally strict: malformed capabilities
/// or any identity other than the canonical activated-v2 identity fail closed
/// instead of silently falling back to legacy validation.
pub fn resolve_inbound_p2p_block_protocol(
    capabilities: Option<&ProtocolCapabilitiesV1>,
    state: &ChainState,
) -> Result<InboundP2pBlockProtocol, String> {
    let Some(capabilities) = capabilities else {
        return Ok(InboundP2pBlockProtocol::Legacy);
    };

    capabilities
        .validate_shape()
        .map_err(|error| format!("invalid local protocol capabilities: {error:?}"))?;

    let expected = ProtocolActivationIdentity::activated_v2(
        state.chain_id.clone(),
        state.dag.genesis_hash.clone(),
        GHOSTDAG_V1_ORDERING_VERSION,
    );
    if capabilities.protocol_identity != expected {
        return Err(format!(
            "local protocol capability identity mismatch: expected={} actual={}",
            expected
                .fingerprint()
                .unwrap_or_else(|_| "invalid-expected-identity".to_string()),
            capabilities
                .protocol_identity
                .fingerprint()
                .unwrap_or_else(|_| "invalid-configured-identity".to_string())
        ));
    }

    Ok(InboundP2pBlockProtocol::ActivatedV2(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        finality_v2::GHOSTDAG_V1_FINALITY_POLICY_VERSION, CONSENSUS_METADATA_SCHEMA_VERSION,
    };
    use pulsedag_p2p::messages::P2P_PROTOCOL_CAPABILITIES_VERSION;

    fn state() -> ChainState {
        pulsedag_core::genesis::init_chain_state("task28-daemon-selector".to_string())
    }

    fn activated_capabilities(state: &ChainState) -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: ProtocolActivationIdentity::activated_v2(
                state.chain_id.clone(),
                state.dag.genesis_hash.clone(),
                GHOSTDAG_V1_ORDERING_VERSION,
            ),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
            supports_dag_frontier: true,
            supports_consensus_metadata: true,
            high_cadence_allowed: false,
        }
    }

    #[test]
    fn absent_capabilities_preserve_exact_legacy_path() {
        let state = state();
        assert_eq!(
            resolve_inbound_p2p_block_protocol(None, &state),
            Ok(InboundP2pBlockProtocol::Legacy)
        );
    }

    #[test]
    fn exact_activated_identity_selects_v2_path() {
        let state = state();
        let capabilities = activated_capabilities(&state);
        assert_eq!(
            resolve_inbound_p2p_block_protocol(Some(&capabilities), &state),
            Ok(InboundP2pBlockProtocol::ActivatedV2(
                capabilities.protocol_identity
            ))
        );
    }

    #[test]
    fn mismatched_identity_fails_closed() {
        let state = state();
        let mut capabilities = activated_capabilities(&state);
        capabilities.protocol_identity.chain_id = "other-chain".to_string();

        let error = resolve_inbound_p2p_block_protocol(Some(&capabilities), &state)
            .expect_err("mismatched identity must not fall back to legacy");
        assert!(error.contains("identity mismatch"));
    }

    #[test]
    fn malformed_capabilities_fail_closed() {
        let state = state();
        let mut capabilities = activated_capabilities(&state);
        capabilities.capabilities_version = P2P_PROTOCOL_CAPABILITIES_VERSION + 1;

        let error = resolve_inbound_p2p_block_protocol(Some(&capabilities), &state)
            .expect_err("malformed capabilities must fail closed");
        assert!(error.contains("invalid local protocol capabilities"));
    }
}
