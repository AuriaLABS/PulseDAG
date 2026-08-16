use serde_json::Value;

use super::{NetworkMessage, ProtocolCapabilitiesV1, ProtocolCompatibilityError};

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
    use crate::messages::P2P_PROTOCOL_CAPABILITIES_VERSION;
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
}
