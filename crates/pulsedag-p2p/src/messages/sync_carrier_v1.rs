use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{NetworkMessage, ProtocolSyncWireError, ProtocolSyncWireV1};

pub const PROTOCOL_SYNC_EXTENSION_FIELD_V1: &str = "pulsedag_protocol_sync_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolSyncCarrierV1 {
    pub target_peer_id: String,
    pub wire: ProtocolSyncWireV1,
}

impl ProtocolSyncCarrierV1 {
    pub fn is_targeted_to(&self, peer_id: &str) -> bool {
        self.target_peer_id == peer_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolSyncCarrierErrorV1 {
    Json(String),
    InvalidJsonRoot,
    UnsupportedCarrierKind { kind: String },
    EmptyTargetPeerId,
    ProtocolSync(ProtocolSyncWireError),
}

#[derive(Debug, Clone)]
pub struct DecodedNetworkMessageWithProtocolSyncV1 {
    pub message: NetworkMessage,
    pub protocol_sync: Option<ProtocolSyncCarrierV1>,
}

fn validate_carrier_for_message(
    message: &NetworkMessage,
    carrier: &ProtocolSyncCarrierV1,
) -> Result<(), ProtocolSyncCarrierErrorV1> {
    if !matches!(message, NetworkMessage::Tips { .. }) {
        return Err(ProtocolSyncCarrierErrorV1::UnsupportedCarrierKind {
            kind: message.kind().to_string(),
        });
    }
    if carrier.target_peer_id.trim().is_empty() {
        return Err(ProtocolSyncCarrierErrorV1::EmptyTargetPeerId);
    }
    carrier
        .wire
        .validate_for_chain(message.chain_id())
        .map_err(ProtocolSyncCarrierErrorV1::ProtocolSync)
}

/// Attach one peer-targeted Task 27 protocol-sync payload to already encoded
/// legacy `Tips` bytes while preserving any other unknown top-level extensions.
///
/// `Tips` is used deliberately: legacy peers can decode and process the base
/// message without learning a new `NetworkMessage` variant, and unlike
/// `GetTips` it does not trigger a response from every peer that sees the
/// broadcast carrier.
pub fn attach_protocol_sync_carrier_v1(
    encoded_network_message: &[u8],
    carrier: &ProtocolSyncCarrierV1,
) -> Result<Vec<u8>, ProtocolSyncCarrierErrorV1> {
    let mut value: Value = serde_json::from_slice(encoded_network_message)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    let message: NetworkMessage = serde_json::from_value(value.clone())
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    validate_carrier_for_message(&message, carrier)?;

    let object = value
        .as_object_mut()
        .ok_or(ProtocolSyncCarrierErrorV1::InvalidJsonRoot)?;
    object.insert(
        PROTOCOL_SYNC_EXTENSION_FIELD_V1.to_string(),
        serde_json::to_value(carrier)
            .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?,
    );
    serde_json::to_vec(&value).map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))
}

/// Encode a legacy network message with an optional targeted Task 27 sync
/// extension. With no extension this is byte-identical to the existing JSON
/// encoding.
pub fn encode_network_message_with_protocol_sync_v1(
    message: &NetworkMessage,
    carrier: Option<&ProtocolSyncCarrierV1>,
) -> Result<Vec<u8>, ProtocolSyncCarrierErrorV1> {
    let legacy = serde_json::to_vec(message)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    match carrier {
        Some(carrier) => attach_protocol_sync_carrier_v1(&legacy, carrier),
        None => Ok(legacy),
    }
}

/// Decode a current/legacy network message and recover the optional targeted
/// protocol-sync extension. The base message remains independently decodable by
/// older peers because the extension is an unknown top-level JSON field.
pub fn decode_network_message_with_protocol_sync_v1(
    bytes: &[u8],
) -> Result<DecodedNetworkMessageWithProtocolSyncV1, ProtocolSyncCarrierErrorV1> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or(ProtocolSyncCarrierErrorV1::InvalidJsonRoot)?;
    let carrier_value = object.get(PROTOCOL_SYNC_EXTENSION_FIELD_V1).cloned();
    let message: NetworkMessage = serde_json::from_value(value)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;

    let protocol_sync = match carrier_value {
        Some(value) => {
            let carrier: ProtocolSyncCarrierV1 = serde_json::from_value(value)
                .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
            validate_carrier_for_message(&message, &carrier)?;
            Some(carrier)
        }
        None => None,
    };

    Ok(DecodedNetworkMessageWithProtocolSyncV1 {
        message,
        protocol_sync,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{
        decode_network_message_with_capabilities_v1, encode_network_message_with_capabilities_v1,
        ProtocolCapabilitiesV1, SelectedChainLocatorV1, P2P_DAG_SYNC_CONTRACT_VERSION,
        P2P_PROTOCOL_CAPABILITIES_VERSION,
    };
    use pulsedag_core::{
        ProtocolActivationIdentity, CONSENSUS_METADATA_SCHEMA_VERSION,
        GHOSTDAG_V1_FINALITY_POLICY_VERSION, GHOSTDAG_V1_ORDERING_VERSION,
    };

    const CHAIN_ID: &str = "task27-protocol-sync-carrier";

    fn identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            chain_id.to_string(),
            "11".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION.to_string(),
        )
    }

    fn capabilities() -> ProtocolCapabilitiesV1 {
        ProtocolCapabilitiesV1 {
            capabilities_version: P2P_PROTOCOL_CAPABILITIES_VERSION,
            protocol_identity: identity(CHAIN_ID),
            consensus_metadata_schema_version: CONSENSUS_METADATA_SCHEMA_VERSION,
            finality_policy_version: GHOSTDAG_V1_FINALITY_POLICY_VERSION.to_string(),
            supports_dag_frontier: true,
            supports_consensus_metadata: true,
            high_cadence_allowed: false,
        }
    }

    fn tips() -> NetworkMessage {
        NetworkMessage::Tips {
            chain_id: CHAIN_ID.to_string(),
            tips: vec!["tip".to_string()],
            inventory: None,
        }
    }

    fn locator(chain_id: &str) -> ProtocolSyncWireV1 {
        ProtocolSyncWireV1::SelectedChainLocator(SelectedChainLocatorV1 {
            contract_version: P2P_DAG_SYNC_CONTRACT_VERSION,
            protocol_identity: identity(chain_id),
            selected_tip: "tip".to_string(),
            locator: vec!["tip".to_string(), "ancestor".to_string()],
        })
    }

    fn carrier(chain_id: &str) -> ProtocolSyncCarrierV1 {
        ProtocolSyncCarrierV1 {
            target_peer_id: "peer-v2".to_string(),
            wire: locator(chain_id),
        }
    }

    #[test]
    fn no_sync_extension_preserves_legacy_bytes_exactly() {
        let message = tips();
        assert_eq!(
            encode_network_message_with_protocol_sync_v1(&message, None).unwrap(),
            serde_json::to_vec(&message).unwrap()
        );
    }

    #[test]
    fn legacy_decoder_ignores_targeted_sync_extension() {
        let message = tips();
        let encoded =
            encode_network_message_with_protocol_sync_v1(&message, Some(&carrier(CHAIN_ID)))
                .unwrap();
        let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(legacy.kind(), "Tips");
        assert_eq!(legacy.chain_id(), CHAIN_ID);
    }

    #[test]
    fn v2_decoder_recovers_target_and_protocol_sync_wire() {
        let expected = carrier(CHAIN_ID);
        let encoded =
            encode_network_message_with_protocol_sync_v1(&tips(), Some(&expected)).unwrap();
        let decoded = decode_network_message_with_protocol_sync_v1(&encoded).unwrap();

        assert_eq!(decoded.message.kind(), "Tips");
        assert_eq!(decoded.protocol_sync, Some(expected.clone()));
        assert!(expected.is_targeted_to("peer-v2"));
        assert!(!expected.is_targeted_to("different-peer"));
    }

    #[test]
    fn capability_and_protocol_sync_extensions_compose_without_losing_either() {
        let capability_encoded =
            encode_network_message_with_capabilities_v1(&tips(), Some(&capabilities())).unwrap();
        let expected_sync = carrier(CHAIN_ID);
        let combined =
            attach_protocol_sync_carrier_v1(&capability_encoded, &expected_sync).unwrap();

        let legacy: NetworkMessage = serde_json::from_slice(&combined).unwrap();
        assert_eq!(legacy.kind(), "Tips");

        let capability_decoded = decode_network_message_with_capabilities_v1(&combined).unwrap();
        assert_eq!(capability_decoded.capabilities, Some(capabilities()));

        let sync_decoded = decode_network_message_with_protocol_sync_v1(&combined).unwrap();
        assert_eq!(sync_decoded.protocol_sync, Some(expected_sync));
    }

    #[test]
    fn wrong_chain_sync_wire_fails_closed() {
        assert!(matches!(
            encode_network_message_with_protocol_sync_v1(
                &tips(),
                Some(&carrier("different-chain")),
            ),
            Err(ProtocolSyncCarrierErrorV1::ProtocolSync(
                ProtocolSyncWireError::ChainIdMismatch { .. }
            ))
        ));
    }

    #[test]
    fn empty_target_and_non_tips_carrier_are_rejected() {
        let mut empty_target = carrier(CHAIN_ID);
        empty_target.target_peer_id.clear();
        assert_eq!(
            encode_network_message_with_protocol_sync_v1(&tips(), Some(&empty_target)),
            Err(ProtocolSyncCarrierErrorV1::EmptyTargetPeerId)
        );

        let get_tips = NetworkMessage::GetTips {
            chain_id: CHAIN_ID.to_string(),
            inventory: None,
        };
        assert!(matches!(
            encode_network_message_with_protocol_sync_v1(
                &get_tips,
                Some(&carrier(CHAIN_ID)),
            ),
            Err(ProtocolSyncCarrierErrorV1::UnsupportedCarrierKind { kind })
                if kind == "GetTips"
        ));
    }
}
