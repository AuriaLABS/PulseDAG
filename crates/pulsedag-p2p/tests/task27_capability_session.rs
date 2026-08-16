use pulsedag_core::{
    ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
    GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_p2p::messages::{
    decode_network_message_with_capabilities_v1, encode_network_message_with_capabilities_v1,
    NetworkMessage, ProtocolCapabilitiesV1, ProtocolCompatibilityError, ProtocolMessageClassV1,
    ProtocolPeerCompatibilityV1, ProtocolPeerRouteActionV1, ProtocolPeerRouterV1,
    P2P_PROTOCOL_CAPABILITIES_VERSION,
};

const CHAIN_ID: &str = "pulsedag-testnet";

fn capabilities(chain_id: &str, genesis_byte: &str) -> ProtocolCapabilitiesV1 {
    ProtocolCapabilitiesV1 {
        capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
        protocol_identity: ProtocolActivationIdentity::activated_v2(
            chain_id.to_string(),
            genesis_byte.repeat(64),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        ),
        consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
        finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
        supports_dag_frontier: true,
        supports_consensus_metadata: true,
        high_cadence_allowed: false,
    }
}

fn get_tips() -> NetworkMessage {
    NetworkMessage::GetTips {
        chain_id: CHAIN_ID.to_string(),
        inventory: None,
    }
}

#[test]
fn compatible_capability_carrier_authorizes_v2_after_legacy_decodable_bootstrap() {
    let local = capabilities(CHAIN_ID, "1");
    let wire = encode_network_message_with_capabilities_v1(&get_tips(), Some(&local)).unwrap();

    let legacy: NetworkMessage = serde_json::from_slice(&wire).unwrap();
    assert_eq!(legacy.kind(), "GetTips");
    assert_eq!(legacy.chain_id(), CHAIN_ID);

    let decoded = decode_network_message_with_capabilities_v1(&wire).unwrap();
    let remote = decoded.capabilities.expect("capabilities carried");
    let mut router = ProtocolPeerRouterV1::default();
    assert_eq!(
        router.observe_remote_capabilities("peer-v2", &local, remote),
        ProtocolPeerCompatibilityV1::Compatible
    );
    let route = router.route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync);
    assert_eq!(route.action, ProtocolPeerRouteActionV1::SendProtocolV2);
    assert!(!route.penalize_peer);
}

#[test]
fn plain_legacy_peer_remains_usable_but_cannot_feed_v2_sync() {
    let wire = serde_json::to_vec(&get_tips()).unwrap();
    let decoded = decode_network_message_with_capabilities_v1(&wire).unwrap();
    assert!(decoded.capabilities.is_none());

    let router = ProtocolPeerRouterV1::default();
    let v2_route = router.route("peer-legacy", ProtocolMessageClassV1::ProtocolV2Sync);
    assert_eq!(
        v2_route.action,
        ProtocolPeerRouteActionV1::HoldForCapabilities
    );
    assert!(!v2_route.penalize_peer);

    let legacy_route = router.route("peer-legacy", ProtocolMessageClassV1::LegacySafe);
    assert_eq!(
        legacy_route.action,
        ProtocolPeerRouteActionV1::SendLegacySafe
    );
    assert!(!legacy_route.penalize_peer);
}

#[test]
fn same_chain_but_different_protocol_identity_falls_back_without_penalty() {
    let local = capabilities(CHAIN_ID, "1");
    let remote = capabilities(CHAIN_ID, "2");
    let wire = encode_network_message_with_capabilities_v1(&get_tips(), Some(&remote)).unwrap();
    let decoded = decode_network_message_with_capabilities_v1(&wire).unwrap();

    let mut router = ProtocolPeerRouterV1::default();
    assert_eq!(
        router.observe_remote_capabilities(
            "peer-different-v2",
            &local,
            decoded.capabilities.unwrap(),
        ),
        ProtocolPeerCompatibilityV1::Incompatible(
            ProtocolCompatibilityError::ProtocolIdentityMismatch
        )
    );
    let route = router.route("peer-different-v2", ProtocolMessageClassV1::ProtocolV2Sync);
    assert_eq!(route.action, ProtocolPeerRouteActionV1::UseLegacyFallback);
    assert!(!route.penalize_peer);
}

#[test]
fn disconnect_resets_authorization_before_next_session() {
    let local = capabilities(CHAIN_ID, "1");
    let wire = encode_network_message_with_capabilities_v1(&get_tips(), Some(&local)).unwrap();
    let decoded = decode_network_message_with_capabilities_v1(&wire).unwrap();

    let mut router = ProtocolPeerRouterV1::default();
    router.observe_remote_capabilities("peer-v2", &local, decoded.capabilities.unwrap());
    assert_eq!(
        router
            .route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync)
            .action,
        ProtocolPeerRouteActionV1::SendProtocolV2
    );

    router.mark_peer_unknown("peer-v2");
    assert_eq!(
        router.compatibility("peer-v2"),
        ProtocolPeerCompatibilityV1::Unknown
    );
    assert_eq!(
        router
            .route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync)
            .action,
        ProtocolPeerRouteActionV1::HoldForCapabilities
    );
}
