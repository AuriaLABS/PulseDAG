use std::fmt;
use std::marker::PhantomData;

use pulsedag_core::{types::Hash, BlockConsensusMetadataV1, ProtocolActivationIdentity};
use serde::de::{self, DeserializeOwned, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{value::RawValue, Value};

use super::{
    DagFrontierEntryV1, DagFrontierResponseV1, NetworkMessage, ProtocolCapabilityHandshakeV1,
    ProtocolSyncWireError, ProtocolSyncWireV1, SelectedChainLocatorV1, MAX_DAG_FRONTIER_ENTRIES,
    MAX_DAG_FRONTIER_PARENTS, MAX_DAG_FRONTIER_REQUIRED_CONTEXT, MAX_SELECTED_CHAIN_LOCATOR_HASHES,
    MAX_SELECTED_CHAIN_SUFFIX_HASHES,
};

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
    UnsupportedProtocolSyncKind { kind: String },
    EmptyTargetPeerId,
    ProtocolSync(ProtocolSyncWireError),
}

#[derive(Debug, Clone)]
pub struct DecodedNetworkMessageWithProtocolSyncV1 {
    pub message: NetworkMessage,
    pub protocol_sync: Option<ProtocolSyncCarrierV1>,
}

struct BoundedSyncVec<T, const MAXIMUM: usize>(Vec<T>);

struct BoundedSyncVecVisitor<T, const MAXIMUM: usize> {
    marker: PhantomData<T>,
}

impl<'de, T, const MAXIMUM: usize> Visitor<'de> for BoundedSyncVecVisitor<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    type Value = BoundedSyncVec<T, MAXIMUM>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "a sequence with at most {MAXIMUM} items")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let capacity = sequence.size_hint().unwrap_or(0).min(MAXIMUM);
        let mut values = Vec::with_capacity(capacity);
        while values.len() < MAXIMUM {
            match sequence.next_element::<T>()? {
                Some(value) => values.push(value),
                None => return Ok(BoundedSyncVec(values)),
            }
        }

        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(format!(
                "sequence exceeds maximum item count {MAXIMUM}"
            )));
        }
        Ok(BoundedSyncVec(values))
    }
}

impl<'de, T, const MAXIMUM: usize> Deserialize<'de> for BoundedSyncVec<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(BoundedSyncVecVisitor::<T, MAXIMUM> {
            marker: PhantomData,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ProtocolSyncExtensionRawV1<'a> {
    #[serde(default, borrow, rename = "pulsedag_protocol_sync_v1")]
    protocol_sync: Option<&'a RawValue>,
}

#[derive(Debug, Deserialize)]
struct ProtocolSyncCarrierRawV1<'a> {
    target_peer_id: String,
    #[serde(borrow)]
    wire: &'a RawValue,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProtocolSyncKindV1 {
    CapabilityHandshake,
    SelectedChainLocator,
    DagFrontier,
}

#[derive(Debug, Deserialize)]
struct ProtocolSyncWireRawV1<'a> {
    sync_type: ProtocolSyncKindV1,
    #[serde(borrow)]
    payload: &'a RawValue,
}

#[derive(Debug, Deserialize)]
struct SelectedChainLocatorDecodeV1 {
    contract_version: u32,
    protocol_identity: ProtocolActivationIdentity,
    selected_tip: Hash,
    locator: BoundedSyncVec<Hash, MAX_SELECTED_CHAIN_LOCATOR_HASHES>,
}

impl From<SelectedChainLocatorDecodeV1> for SelectedChainLocatorV1 {
    fn from(value: SelectedChainLocatorDecodeV1) -> Self {
        Self {
            contract_version: value.contract_version,
            protocol_identity: value.protocol_identity,
            selected_tip: value.selected_tip,
            locator: value.locator.0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct DagFrontierEntryDecodeV1 {
    hash: Hash,
    parents: BoundedSyncVec<Hash, MAX_DAG_FRONTIER_PARENTS>,
    consensus: BlockConsensusMetadataV1,
}

impl From<DagFrontierEntryDecodeV1> for DagFrontierEntryV1 {
    fn from(value: DagFrontierEntryDecodeV1) -> Self {
        Self {
            hash: value.hash,
            parents: value.parents.0,
            consensus: value.consensus,
        }
    }
}

#[derive(Debug, Deserialize)]
struct DagFrontierResponseDecodeV1 {
    contract_version: u32,
    protocol_identity: ProtocolActivationIdentity,
    consensus_metadata_schema_version: u32,
    ordering_version: String,
    common_ancestor: Hash,
    selected_tip: Hash,
    selected_chain_suffix: BoundedSyncVec<Hash, MAX_SELECTED_CHAIN_SUFFIX_HASHES>,
    required_context: BoundedSyncVec<Hash, MAX_DAG_FRONTIER_REQUIRED_CONTEXT>,
    frontier: BoundedSyncVec<DagFrontierEntryDecodeV1, MAX_DAG_FRONTIER_ENTRIES>,
}

impl From<DagFrontierResponseDecodeV1> for DagFrontierResponseV1 {
    fn from(value: DagFrontierResponseDecodeV1) -> Self {
        Self {
            contract_version: value.contract_version,
            protocol_identity: value.protocol_identity,
            consensus_metadata_schema_version: value.consensus_metadata_schema_version,
            ordering_version: value.ordering_version,
            common_ancestor: value.common_ancestor,
            selected_tip: value.selected_tip,
            selected_chain_suffix: value.selected_chain_suffix.0,
            required_context: value.required_context.0,
            frontier: value.frontier.0.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProtocolSyncTargetV1 {
    #[serde(default)]
    target_peer_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProtocolSyncTargetExtensionWireV1 {
    #[serde(default, rename = "pulsedag_protocol_sync_v1")]
    protocol_sync: Option<ProtocolSyncTargetV1>,
}

fn parse_sync_payload<T>(
    raw: &RawValue,
    field: &'static str,
) -> Result<T, ProtocolSyncCarrierErrorV1>
where
    T: DeserializeOwned,
{
    serde_json::from_str(raw.get()).map_err(|error| {
        ProtocolSyncCarrierErrorV1::Json(format!("protocol sync {field}: {error}"))
    })
}

fn decode_protocol_sync_wire_v1(
    raw: &RawValue,
) -> Result<ProtocolSyncWireV1, ProtocolSyncCarrierErrorV1> {
    let wire: ProtocolSyncWireRawV1<'_> = parse_sync_payload(raw, "wire")?;
    match wire.sync_type {
        ProtocolSyncKindV1::CapabilityHandshake => Ok(ProtocolSyncWireV1::CapabilityHandshake(
            parse_sync_payload::<ProtocolCapabilityHandshakeV1>(wire.payload, "handshake payload")?,
        )),
        ProtocolSyncKindV1::SelectedChainLocator => {
            let locator = parse_sync_payload::<SelectedChainLocatorDecodeV1>(
                wire.payload,
                "selected-chain locator payload",
            )?;
            Ok(ProtocolSyncWireV1::SelectedChainLocator(locator.into()))
        }
        ProtocolSyncKindV1::DagFrontier => {
            let frontier = parse_sync_payload::<DagFrontierResponseDecodeV1>(
                wire.payload,
                "DAG frontier payload",
            )?;
            Ok(ProtocolSyncWireV1::DagFrontier(frontier.into()))
        }
    }
}

fn decode_protocol_sync_carrier_v1(
    raw: &RawValue,
) -> Result<ProtocolSyncCarrierV1, ProtocolSyncCarrierErrorV1> {
    let carrier: ProtocolSyncCarrierRawV1<'_> = parse_sync_payload(raw, "carrier")?;
    Ok(ProtocolSyncCarrierV1 {
        target_peer_id: carrier.target_peer_id,
        wire: decode_protocol_sync_wire_v1(carrier.wire)?,
    })
}

fn decode_protocol_sync_extension_v1(
    bytes: &[u8],
) -> Result<Option<ProtocolSyncCarrierV1>, ProtocolSyncCarrierErrorV1> {
    let extension: ProtocolSyncExtensionRawV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    extension
        .protocol_sync
        .map(decode_protocol_sync_carrier_v1)
        .transpose()
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
    if matches!(carrier.wire, ProtocolSyncWireV1::CapabilityHandshake(_)) {
        return Err(ProtocolSyncCarrierErrorV1::UnsupportedProtocolSyncKind {
            kind: carrier.wire.kind().to_string(),
        });
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
    let message: NetworkMessage = serde_json::from_slice(encoded_network_message)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    validate_carrier_for_message(&message, carrier)?;

    let mut value: Value = serde_json::from_slice(encoded_network_message)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
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
/// protocol-sync extension. Both passes are streaming Serde decodes: unrelated
/// top-level fields are skipped instead of materialized into a complete JSON
/// `Value`, and the selected sync payload is decoded only after its raw tag is
/// known with its live vector budgets applied during sequence consumption.
pub fn decode_network_message_with_protocol_sync_v1(
    bytes: &[u8],
) -> Result<DecodedNetworkMessageWithProtocolSyncV1, ProtocolSyncCarrierErrorV1> {
    let message: NetworkMessage = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    let protocol_sync = decode_protocol_sync_extension_v1(bytes)?;

    if let Some(carrier) = protocol_sync.as_ref() {
        validate_carrier_for_message(&message, carrier)?;
    }

    Ok(DecodedNetworkMessageWithProtocolSyncV1 {
        message,
        protocol_sync,
    })
}

/// Decode the base legacy message for every peer, but only deserialize and
/// validate the complete protocol-sync extension when it is explicitly
/// addressed to `local_peer_id`. The first extension pass reads only the target
/// id and streams over the rest, preserving bystander behavior for malformed
/// payloads addressed to another peer without materializing a JSON `Value`.
pub fn decode_network_message_with_protocol_sync_for_peer_v1(
    bytes: &[u8],
    local_peer_id: &str,
) -> Result<DecodedNetworkMessageWithProtocolSyncV1, ProtocolSyncCarrierErrorV1> {
    let message: NetworkMessage = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;
    let target_extension: ProtocolSyncTargetExtensionWireV1 = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolSyncCarrierErrorV1::Json(error.to_string()))?;

    let protocol_sync = match target_extension
        .protocol_sync
        .and_then(|target| target.target_peer_id)
    {
        Some(target_peer_id) if target_peer_id == local_peer_id => {
            let carrier = decode_protocol_sync_extension_v1(bytes)?.ok_or_else(|| {
                ProtocolSyncCarrierErrorV1::Json(
                    "protocol sync target present without full carrier".to_string(),
                )
            })?;
            validate_carrier_for_message(&message, &carrier)?;
            Some(carrier)
        }
        _ => None,
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
        ProtocolCapabilitiesV1, ProtocolCapabilityHandshakeV1, SelectedChainLocatorV1,
        P2P_DAG_SYNC_CONTRACT_VERSION, P2P_PROTOCOL_CAPABILITIES_VERSION,
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

    fn encoded_with_payload_before_sync_type(
        sync_type: &str,
        payload: &Value,
        target_peer_id: &str,
    ) -> Vec<u8> {
        let mut base = serde_json::to_string(&tips()).expect("serialize base Tips");
        assert_eq!(base.pop(), Some('}'));
        let target = serde_json::to_string(target_peer_id).expect("serialize target");
        let sync_type = serde_json::to_string(sync_type).expect("serialize sync type");
        let payload = serde_json::to_string(payload).expect("serialize sync payload");
        format!(
            "{base},\"{PROTOCOL_SYNC_EXTENSION_FIELD_V1}\":{{\"target_peer_id\":{target},\"wire\":{{\"payload\":{payload},\"sync_type\":{sync_type}}}}}}}}"
        )
        .into_bytes()
    }

    fn consensus_metadata() -> BlockConsensusMetadataV1 {
        BlockConsensusMetadataV1 {
            selected_parent: None,
            blue_score: 1,
            blue_work_decimal: "1".to_string(),
            merge_set_blues: Vec::new(),
            merge_set_reds: Vec::new(),
        }
    }

    fn frontier_payload(frontier: Vec<Value>) -> Value {
        serde_json::json!({
            "contract_version": P2P_DAG_SYNC_CONTRACT_VERSION,
            "protocol_identity": identity(CHAIN_ID),
            "consensus_metadata_schema_version": CONSENSUS_METADATA_SCHEMA_VERSION,
            "ordering_version": GHOSTDAG_V1_ORDERING_VERSION,
            "common_ancestor": "ancestor",
            "selected_tip": "tip",
            "selected_chain_suffix": ["ancestor", "tip"],
            "required_context": [],
            "frontier": frontier,
        })
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
    fn payload_before_sync_type_decodes_after_raw_tag_selection() {
        let payload = match locator(CHAIN_ID) {
            ProtocolSyncWireV1::SelectedChainLocator(locator) => {
                serde_json::to_value(locator).expect("serialize locator")
            }
            other => panic!("unexpected sync wire: {other:?}"),
        };
        let encoded =
            encoded_with_payload_before_sync_type("selected_chain_locator", &payload, "peer-v2");
        let decoded =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, "peer-v2").unwrap();
        assert_eq!(decoded.protocol_sync, Some(carrier(CHAIN_ID)));
    }

    #[test]
    fn oversized_targeted_locator_is_rejected_during_bounded_decode() {
        let mut payload = match locator(CHAIN_ID) {
            ProtocolSyncWireV1::SelectedChainLocator(locator) => {
                serde_json::to_value(locator).expect("serialize locator")
            }
            other => panic!("unexpected sync wire: {other:?}"),
        };
        payload["locator"] = Value::Array(
            (0..=MAX_SELECTED_CHAIN_LOCATOR_HASHES)
                .map(|index| Value::String(format!("hash-{index}")))
                .collect(),
        );
        let encoded =
            encoded_with_payload_before_sync_type("selected_chain_locator", &payload, "peer-v2");
        let error =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, "peer-v2").unwrap_err();
        assert!(matches!(error, ProtocolSyncCarrierErrorV1::Json(_)));
        assert!(format!("{error:?}").contains(&MAX_SELECTED_CHAIN_LOCATOR_HASHES.to_string()));
    }

    #[test]
    fn oversized_targeted_frontier_is_rejected_during_bounded_decode() {
        let entry = serde_json::json!({
            "hash": "frontier",
            "parents": [],
            "consensus": consensus_metadata(),
        });
        let payload = frontier_payload(vec![entry; MAX_DAG_FRONTIER_ENTRIES + 1]);
        let encoded = encoded_with_payload_before_sync_type("dag_frontier", &payload, "peer-v2");
        let error =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, "peer-v2").unwrap_err();
        assert!(matches!(error, ProtocolSyncCarrierErrorV1::Json(_)));
        assert!(format!("{error:?}").contains(&MAX_DAG_FRONTIER_ENTRIES.to_string()));
    }

    #[test]
    fn oversized_targeted_frontier_parents_are_rejected_during_bounded_decode() {
        let entry = serde_json::json!({
            "hash": "frontier",
            "parents": (0..=MAX_DAG_FRONTIER_PARENTS)
                .map(|index| format!("parent-{index}"))
                .collect::<Vec<_>>(),
            "consensus": consensus_metadata(),
        });
        let payload = frontier_payload(vec![entry]);
        let encoded = encoded_with_payload_before_sync_type("dag_frontier", &payload, "peer-v2");
        let error =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, "peer-v2").unwrap_err();
        assert!(matches!(error, ProtocolSyncCarrierErrorV1::Json(_)));
        assert!(format!("{error:?}").contains(&MAX_DAG_FRONTIER_PARENTS.to_string()));
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
    fn target_aware_decoder_admits_only_the_local_target() {
        let expected = carrier(CHAIN_ID);
        let encoded =
            encode_network_message_with_protocol_sync_v1(&tips(), Some(&expected)).unwrap();

        let local =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, "peer-v2").unwrap();
        assert_eq!(local.protocol_sync, Some(expected));

        let bystander =
            decode_network_message_with_protocol_sync_for_peer_v1(&encoded, "different-peer")
                .unwrap();
        assert!(bystander.protocol_sync.is_none());
        assert_eq!(bystander.message.kind(), "Tips");
    }

    #[test]
    fn malformed_other_target_is_ignored_before_v2_payload_decode() {
        let encoded =
            encode_network_message_with_protocol_sync_v1(&tips(), Some(&carrier(CHAIN_ID)))
                .unwrap();
        let mut value: Value = serde_json::from_slice(&encoded).unwrap();
        value[PROTOCOL_SYNC_EXTENSION_FIELD_V1]["wire"] =
            Value::String("malformed-wire".to_string());
        let malformed = serde_json::to_vec(&value).unwrap();

        let bystander =
            decode_network_message_with_protocol_sync_for_peer_v1(&malformed, "different-peer")
                .unwrap();
        assert!(bystander.protocol_sync.is_none());
        assert_eq!(bystander.message.kind(), "Tips");

        assert!(
            decode_network_message_with_protocol_sync_for_peer_v1(&malformed, "peer-v2",).is_err()
        );
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
    fn capability_handshake_is_rejected_from_protocol_sync_carrier() {
        let handshake = ProtocolSyncCarrierV1 {
            target_peer_id: "peer-v2".to_string(),
            wire: ProtocolSyncWireV1::CapabilityHandshake(
                ProtocolCapabilityHandshakeV1::GetProtocolCapabilities {
                    chain_id: CHAIN_ID.to_string(),
                },
            ),
        };

        assert!(matches!(
            encode_network_message_with_protocol_sync_v1(&tips(), Some(&handshake)),
            Err(ProtocolSyncCarrierErrorV1::UnsupportedProtocolSyncKind { kind })
                if kind == "CapabilityHandshake"
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
