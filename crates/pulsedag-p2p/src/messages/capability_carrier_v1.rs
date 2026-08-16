use serde_json::Value;

use super::protocol_v2::{
    ProtocolPeerSessionErrorV1, ProtocolPeerSessionObservationV1, ProtocolPeerSessionV1,
};
use super::{
    NetworkMessage, ProtocolCapabilitiesV1, ProtocolCompatibilityError, ProtocolMessageClassV1,
    ProtocolPeerRouteDecisionV1,
};

pub const PROTOCOL_CAPABILITY_EXTENSION_FIELD_V1: &str = "pulsedag_protocol_capabilities_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolCapabilityCarrierErrorV1 {
    Json(String),
    InvalidJsonRoot,
    UnsupportedCarrierKind { kind: String },
    ProtocolCapability(ProtocolCompatibilityError),
    ChainIdMismatch { message: String, capability: String },
}

#[derive(Debug, Clone)]
pub struct DecodedNetworkMessageWithCapabilitiesV1 {
    pub message: NetworkMessage,
    pub capabilities: Option<ProtocolCapabilitiesV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolCapabilityTransportErrorV1 {
    Carrier(ProtocolCapabilityCarrierErrorV1),
    Session(ProtocolPeerSessionErrorV1),
}

#[derive(Debug, Clone)]
pub struct DecodedProtocolCapabilityTransportV1 {
    pub message: NetworkMessage,
    /// `Some` only for the legacy-compatible capability carriers GetTips/Tips.
    /// Ordinary block/transaction/header traffic must never reset negotiated
    /// peer state merely because it does not carry capability metadata.
    pub observation: Option<ProtocolPeerSessionObservationV1>,
}

/// Transport-facing composition of the backward-compatible capability carrier
/// and runtime-owned per-peer negotiation state.
///
/// This helper deliberately performs no network I/O. The live libp2p loop can
/// keep one defaultable value in its existing runtime state, configure exact
/// local activation capabilities from the node, and then use narrow encode /
/// decode / disconnect calls at the existing GetTips/Tips transport seams.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtocolCapabilityTransportV1 {
    session: ProtocolPeerSessionV1,
}

impl ProtocolCapabilityTransportV1 {
    pub fn configure_local_capabilities(
        &mut self,
        expected_chain_id: &str,
        capabilities: ProtocolCapabilitiesV1,
    ) -> Result<(), ProtocolCapabilityTransportErrorV1> {
        self.session
            .configure_local_capabilities(expected_chain_id, capabilities)
            .map_err(ProtocolCapabilityTransportErrorV1::Session)
    }

    pub fn local_capabilities(&self) -> Option<&ProtocolCapabilitiesV1> {
        self.session.local_capabilities()
    }

    /// Encode an existing legacy-decodable GetTips/Tips message and advertise
    /// local v2 capabilities only after exact local activation identity has been
    /// configured. Before configuration this is byte-identical legacy JSON.
    pub fn encode_tip_message(
        &self,
        message: &NetworkMessage,
    ) -> Result<Vec<u8>, ProtocolCapabilityTransportErrorV1> {
        encode_network_message_with_capabilities_v1(message, self.session.local_capabilities())
            .map_err(ProtocolCapabilityTransportErrorV1::Carrier)
    }

    /// Decode one inbound network message and bind capability-carrier
    /// observations to the authenticated peer id.
    ///
    /// Only GetTips/Tips are negotiation carriers. All other valid legacy
    /// message kinds are decoded and returned without mutating peer capability
    /// state. This is essential because ordinary traffic omits the extension by
    /// design and therefore must not silently revoke an already-compatible peer.
    pub fn decode_from_peer(
        &mut self,
        peer_id: &str,
        bytes: &[u8],
    ) -> Result<DecodedProtocolCapabilityTransportV1, ProtocolCapabilityTransportErrorV1> {
        let decoded = decode_network_message_with_capabilities_v1(bytes)
            .map_err(ProtocolCapabilityTransportErrorV1::Carrier)?;

        if let Some(local) = self.session.local_capabilities() {
            if decoded.message.chain_id() != local.protocol_identity.chain_id {
                return Err(ProtocolCapabilityTransportErrorV1::Carrier(
                    ProtocolCapabilityCarrierErrorV1::ChainIdMismatch {
                        message: decoded.message.chain_id().to_string(),
                        capability: local.protocol_identity.chain_id.clone(),
                    },
                ));
            }
        }

        let observation = if is_legacy_capability_carrier(&decoded.message) {
            Some(
                self.session
                    .observe_remote_capabilities(peer_id, decoded.capabilities)
                    .map_err(ProtocolCapabilityTransportErrorV1::Session)?,
            )
        } else {
            None
        };
        Ok(DecodedProtocolCapabilityTransportV1 {
            message: decoded.message,
            observation,
        })
    }

    pub fn route(
        &self,
        peer_id: &str,
        message_class: ProtocolMessageClassV1,
    ) -> ProtocolPeerRouteDecisionV1 {
        self.session.route(peer_id, message_class)
    }

    pub fn eligible_v2_peers(&self) -> Vec<String> {
        self.session.eligible_v2_peers()
    }

    pub fn peer_disconnected(&mut self, peer_id: &str) {
        self.session.peer_disconnected(peer_id);
    }

    pub fn reset_local_capabilities(&mut self) {
        self.session.reset_local_capabilities();
    }
}

fn is_legacy_capability_carrier(message: &NetworkMessage) -> bool {
    matches!(
        message,
        NetworkMessage::GetTips { .. } | NetworkMessage::Tips { .. }
    )
}

fn validate_capabilities_for_message(
    message: &NetworkMessage,
    capabilities: &ProtocolCapabilitiesV1,
) -> Result<(), ProtocolCapabilityCarrierErrorV1> {
    if !is_legacy_capability_carrier(message) {
        return Err(ProtocolCapabilityCarrierErrorV1::UnsupportedCarrierKind {
            kind: message.kind().to_string(),
        });
    }
    capabilities
        .validate_shape()
        .map_err(ProtocolCapabilityCarrierErrorV1::ProtocolCapability)?;
    if capabilities.protocol_identity.chain_id != message.chain_id() {
        return Err(ProtocolCapabilityCarrierErrorV1::ChainIdMismatch {
            message: message.chain_id().to_string(),
            capability: capabilities.protocol_identity.chain_id.clone(),
        });
    }
    Ok(())
}

/// Encode one legacy-decodable network message with an optional Task 27
/// capability extension.
///
/// The extension is intentionally an unknown top-level JSON field on the
/// existing `GetTips`/`Tips` variants. Legacy Serde decoders ignore that field,
/// while v2.4 peers can extract and validate the capability identity before
/// authorizing protocol-v2 sync. No new `NetworkMessage` variant is required for
/// capability bootstrap.
pub fn encode_network_message_with_capabilities_v1(
    message: &NetworkMessage,
    capabilities: Option<&ProtocolCapabilitiesV1>,
) -> Result<Vec<u8>, ProtocolCapabilityCarrierErrorV1> {
    let Some(capabilities) = capabilities else {
        return serde_json::to_vec(message)
            .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()));
    };

    validate_capabilities_for_message(message, capabilities)?;
    let mut value = serde_json::to_value(message)
        .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or(ProtocolCapabilityCarrierErrorV1::InvalidJsonRoot)?;
    object.insert(
        PROTOCOL_CAPABILITY_EXTENSION_FIELD_V1.to_string(),
        serde_json::to_value(capabilities)
            .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()))?,
    );
    serde_json::to_vec(&value)
        .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()))
}

/// Decode a current/legacy `NetworkMessage` and, when present, extract the
/// backward-compatible Task 27 capability extension.
///
/// Capability data is validated independently from the legacy message. A wrong
/// chain, malformed capability object, or extension attached to a non-tip
/// carrier fails closed for v2 routing even though an older decoder would still
/// be able to ignore the unknown field.
pub fn decode_network_message_with_capabilities_v1(
    bytes: &[u8],
) -> Result<DecodedNetworkMessageWithCapabilitiesV1, ProtocolCapabilityCarrierErrorV1> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or(ProtocolCapabilityCarrierErrorV1::InvalidJsonRoot)?;
    let capability_value = object.get(PROTOCOL_CAPABILITY_EXTENSION_FIELD_V1).cloned();
    let message: NetworkMessage = serde_json::from_value(value)
        .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()))?;

    let capabilities = match capability_value {
        Some(value) => {
            let capabilities: ProtocolCapabilitiesV1 = serde_json::from_value(value)
                .map_err(|error| ProtocolCapabilityCarrierErrorV1::Json(error.to_string()))?;
            validate_capabilities_for_message(&message, &capabilities)?;
            Some(capabilities)
        }
        None => None,
    };

    Ok(DecodedNetworkMessageWithCapabilitiesV1 {
        message,
        capabilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{ProtocolPeerRouteActionV1, P2P_PROTOCOL_CAPABILITIES_VERSION};
    use pulsedag_core::{
        ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "pulsedag-testnet";

    fn capabilities(chain_id: &str) -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: ProtocolActivationIdentity::activated_v2(
                chain_id.to_string(),
                "11".repeat(32),
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

    fn tips() -> NetworkMessage {
        NetworkMessage::Tips {
            chain_id: CHAIN_ID.to_string(),
            tips: vec!["tip".to_string()],
            inventory: None,
        }
    }

    #[test]
    fn no_extension_preserves_legacy_wire_bytes_exactly() {
        let message = get_tips();
        let legacy = serde_json::to_vec(&message).unwrap();
        let encoded = encode_network_message_with_capabilities_v1(&message, None).unwrap();
        assert_eq!(encoded, legacy);
    }

    #[test]
    fn legacy_decoder_ignores_capability_extension() {
        for message in [get_tips(), tips()] {
            let encoded = encode_network_message_with_capabilities_v1(
                &message,
                Some(&capabilities(CHAIN_ID)),
            )
            .unwrap();
            let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(legacy.kind(), message.kind());
            assert_eq!(legacy.chain_id(), CHAIN_ID);
        }
    }

    #[test]
    fn v2_decoder_recovers_and_validates_capability_extension() {
        let expected = capabilities(CHAIN_ID);
        let encoded =
            encode_network_message_with_capabilities_v1(&tips(), Some(&expected)).unwrap();
        let decoded = decode_network_message_with_capabilities_v1(&encoded).unwrap();

        assert_eq!(decoded.message.kind(), "Tips");
        assert_eq!(decoded.message.chain_id(), CHAIN_ID);
        assert_eq!(decoded.capabilities, Some(expected));
    }

    #[test]
    fn v2_decoder_accepts_plain_legacy_tip_wire_without_capabilities() {
        let encoded = serde_json::to_vec(&get_tips()).unwrap();
        let decoded = decode_network_message_with_capabilities_v1(&encoded).unwrap();

        assert_eq!(decoded.message.kind(), "GetTips");
        assert_eq!(decoded.message.chain_id(), CHAIN_ID);
        assert!(decoded.capabilities.is_none());
    }

    #[test]
    fn wrong_chain_capability_extension_fails_closed() {
        let message = get_tips();
        assert_eq!(
            encode_network_message_with_capabilities_v1(
                &message,
                Some(&capabilities("different-chain")),
            ),
            Err(ProtocolCapabilityCarrierErrorV1::ChainIdMismatch {
                message: CHAIN_ID.to_string(),
                capability: "different-chain".to_string(),
            })
        );
    }

    #[test]
    fn capability_extension_is_rejected_on_non_tip_message() {
        let message = NetworkMessage::InvBlock {
            chain_id: CHAIN_ID.to_string(),
            hashes: vec!["block".to_string()],
        };
        assert_eq!(
            encode_network_message_with_capabilities_v1(&message, Some(&capabilities(CHAIN_ID)),),
            Err(ProtocolCapabilityCarrierErrorV1::UnsupportedCarrierKind {
                kind: "InvBlock".to_string(),
            })
        );
    }

    #[test]
    fn malformed_capability_extension_fails_for_v2_decoder_but_not_legacy_decoder() {
        let message = get_tips();
        let mut value = serde_json::to_value(&message).unwrap();
        value.as_object_mut().unwrap().insert(
            PROTOCOL_CAPABILITY_EXTENSION_FIELD_V1.to_string(),
            serde_json::json!({"capabilities_version": "not-a-number"}),
        );
        let encoded = serde_json::to_vec(&value).unwrap();

        let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(legacy.kind(), "GetTips");
        assert!(matches!(
            decode_network_message_with_capabilities_v1(&encoded),
            Err(ProtocolCapabilityCarrierErrorV1::Json(_))
        ));
    }

    #[test]
    fn transport_is_legacy_byte_identical_until_local_identity_is_configured() {
        let transport = ProtocolCapabilityTransportV1::default();
        let message = get_tips();
        assert_eq!(
            transport.encode_tip_message(&message).unwrap(),
            serde_json::to_vec(&message).unwrap()
        );
    }

    #[test]
    fn transport_configured_identity_advertises_and_observes_compatible_peer() {
        let local = capabilities(CHAIN_ID);
        let mut sender = ProtocolCapabilityTransportV1::default();
        sender
            .configure_local_capabilities(CHAIN_ID, local.clone())
            .unwrap();
        let wire = sender.encode_tip_message(&get_tips()).unwrap();

        let legacy: NetworkMessage = serde_json::from_slice(&wire).unwrap();
        assert_eq!(legacy.kind(), "GetTips");

        let mut receiver = ProtocolCapabilityTransportV1::default();
        receiver
            .configure_local_capabilities(CHAIN_ID, local)
            .unwrap();
        let decoded = receiver.decode_from_peer("peer-v2", &wire).unwrap();
        assert_eq!(decoded.message.kind(), "GetTips");
        assert_eq!(
            decoded.observation,
            Some(ProtocolPeerSessionObservationV1::Compatible)
        );
        assert_eq!(
            receiver
                .route("peer-v2", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::SendProtocolV2
        );
        assert_eq!(receiver.eligible_v2_peers(), vec!["peer-v2".to_string()]);
    }

    #[test]
    fn transport_plain_legacy_tip_revokes_previous_v2_authorization() {
        let local = capabilities(CHAIN_ID);
        let mut transport = ProtocolCapabilityTransportV1::default();
        transport
            .configure_local_capabilities(CHAIN_ID, local.clone())
            .unwrap();
        let v2_wire = encode_network_message_with_capabilities_v1(&get_tips(), Some(&local)).unwrap();
        transport.decode_from_peer("peer", &v2_wire).unwrap();
        assert_eq!(
            transport
                .route("peer", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::SendProtocolV2
        );

        let legacy_wire = serde_json::to_vec(&get_tips()).unwrap();
        let decoded = transport.decode_from_peer("peer", &legacy_wire).unwrap();
        assert_eq!(
            decoded.observation,
            Some(ProtocolPeerSessionObservationV1::LegacyNoCapabilities)
        );
        assert_eq!(
            transport
                .route("peer", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::HoldForCapabilities
        );
    }

    #[test]
    fn transport_non_carrier_traffic_preserves_v2_authorization() {
        let local = capabilities(CHAIN_ID);
        let mut transport = ProtocolCapabilityTransportV1::default();
        transport
            .configure_local_capabilities(CHAIN_ID, local.clone())
            .unwrap();
        let v2_wire = encode_network_message_with_capabilities_v1(&get_tips(), Some(&local)).unwrap();
        transport.decode_from_peer("peer", &v2_wire).unwrap();

        let ordinary = NetworkMessage::InvBlock {
            chain_id: CHAIN_ID.to_string(),
            hashes: vec!["block".to_string()],
        };
        let decoded = transport
            .decode_from_peer("peer", &serde_json::to_vec(&ordinary).unwrap())
            .unwrap();
        assert_eq!(decoded.message.kind(), "InvBlock");
        assert!(decoded.observation.is_none());
        assert_eq!(
            transport
                .route("peer", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::SendProtocolV2
        );
    }

    #[test]
    fn transport_rejects_wrong_chain_before_peer_state_changes() {
        let local = capabilities(CHAIN_ID);
        let mut transport = ProtocolCapabilityTransportV1::default();
        transport
            .configure_local_capabilities(CHAIN_ID, local)
            .unwrap();
        let wrong = NetworkMessage::GetTips {
            chain_id: "different-chain".to_string(),
            inventory: None,
        };
        let bytes = serde_json::to_vec(&wrong).unwrap();
        assert!(matches!(
            transport.decode_from_peer("peer", &bytes),
            Err(ProtocolCapabilityTransportErrorV1::Carrier(
                ProtocolCapabilityCarrierErrorV1::ChainIdMismatch { .. }
            ))
        ));
        assert_eq!(
            transport
                .route("peer", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::HoldForCapabilities
        );
    }

    #[test]
    fn transport_capability_decode_fails_until_local_identity_is_configured() {
        let local = capabilities(CHAIN_ID);
        let bytes = encode_network_message_with_capabilities_v1(&get_tips(), Some(&local)).unwrap();
        let mut transport = ProtocolCapabilityTransportV1::default();
        assert!(matches!(
            transport.decode_from_peer("peer", &bytes),
            Err(ProtocolCapabilityTransportErrorV1::Session(
                ProtocolPeerSessionErrorV1::LocalCapabilitiesUnavailable
            ))
        ));
    }

    #[test]
    fn transport_disconnect_and_local_reset_revoke_authorization() {
        let local = capabilities(CHAIN_ID);
        let mut transport = ProtocolCapabilityTransportV1::default();
        transport
            .configure_local_capabilities(CHAIN_ID, local.clone())
            .unwrap();
        let bytes = encode_network_message_with_capabilities_v1(&get_tips(), Some(&local)).unwrap();
        transport.decode_from_peer("peer", &bytes).unwrap();

        transport.peer_disconnected("peer");
        assert_eq!(
            transport
                .route("peer", ProtocolMessageClassV1::ProtocolV2Sync)
                .action,
            ProtocolPeerRouteActionV1::HoldForCapabilities
        );

        transport.decode_from_peer("peer", &bytes).unwrap();
        transport.reset_local_capabilities();
        assert!(transport.local_capabilities().is_none());
        assert!(transport.eligible_v2_peers().is_empty());
    }
}
