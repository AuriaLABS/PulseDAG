use std::fmt;
use std::marker::PhantomData;

use pulsedag_core::{
    types::{BlockHeader, Hash, Transaction},
    GHOSTDAG_V1_MAX_PARENTS,
};
use serde::de::{self, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{value::RawValue, Value};

use super::{
    validate_compact_block_announcement_v1, validate_compact_transaction_request_v1,
    CompactBlockAnnouncementV1, CompactRelayErrorV1, CompactTransactionRequestV1,
    CompactTransactionResponseV1, NetworkMessage, COMPACT_DAG_RELAY_VERSION_V1,
    P2P_WIRE_MAX_INVENTORY_ITEMS_V1, P2P_WIRE_MAX_REQUEST_ITEMS_V1,
};

pub const COMPACT_RELAY_EXTENSION_FIELD_V1: &str = "pulsedag_compact_relay_v1";
pub const COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1: usize = 60 * 1024;
pub const COMPACT_RELAY_MAX_TARGET_PEER_ID_BYTES_V1: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompactRelayCapabilitiesV1 {
    pub contract_version: u16,
    pub chain_id: String,
    pub max_announcement_txids: u16,
    pub max_request_txids: u16,
    pub max_carrier_bytes: u32,
}

impl CompactRelayCapabilitiesV1 {
    pub fn canonical(chain_id: impl Into<String>) -> Self {
        Self {
            contract_version: COMPACT_DAG_RELAY_VERSION_V1,
            chain_id: chain_id.into(),
            max_announcement_txids: P2P_WIRE_MAX_INVENTORY_ITEMS_V1 as u16,
            max_request_txids: P2P_WIRE_MAX_REQUEST_ITEMS_V1 as u16,
            max_carrier_bytes: COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1 as u32,
        }
    }

    pub fn validate_for_chain(
        &self,
        expected_chain_id: &str,
    ) -> Result<(), CompactRelayCarrierErrorV1> {
        if self.contract_version != COMPACT_DAG_RELAY_VERSION_V1 {
            return Err(CompactRelayCarrierErrorV1::UnsupportedVersion(
                self.contract_version,
            ));
        }
        if self.chain_id != expected_chain_id {
            return Err(CompactRelayCarrierErrorV1::ChainIdMismatch {
                expected: expected_chain_id.to_string(),
                observed: self.chain_id.clone(),
            });
        }
        let expected = Self::canonical(expected_chain_id);
        if self.max_announcement_txids != expected.max_announcement_txids
            || self.max_request_txids != expected.max_request_txids
            || self.max_carrier_bytes != expected.max_carrier_bytes
        {
            return Err(CompactRelayCarrierErrorV1::CapabilityBoundsMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "compact_relay_type",
    content = "payload",
    rename_all = "snake_case"
)]
pub enum CompactRelayWireV1 {
    CapabilityProbe,
    Capabilities(CompactRelayCapabilitiesV1),
    Announce(CompactBlockAnnouncementV1),
    GetTransactions(CompactTransactionRequestV1),
    Transactions(CompactTransactionResponseV1),
}

impl CompactRelayWireV1 {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::CapabilityProbe => "capability_probe",
            Self::Capabilities(_) => "capabilities",
            Self::Announce(_) => "announce",
            Self::GetTransactions(_) => "get_transactions",
            Self::Transactions(_) => "transactions",
        }
    }

    fn validate_for_chain(&self, chain_id: &str) -> Result<(), CompactRelayCarrierErrorV1> {
        match self {
            Self::CapabilityProbe => Ok(()),
            Self::Capabilities(capabilities) => capabilities.validate_for_chain(chain_id),
            Self::Announce(announcement) => validate_compact_block_announcement_v1(announcement)
                .map_err(CompactRelayCarrierErrorV1::Compact),
            Self::GetTransactions(request) => validate_compact_transaction_request_v1(request)
                .map_err(CompactRelayCarrierErrorV1::Compact),
            Self::Transactions(response) => validate_response_shape(response),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CompactRelayCarrierV1 {
    pub target_peer_id: String,
    pub chain_id: String,
    pub wire: CompactRelayWireV1,
}

#[derive(Debug, Clone)]
pub struct DecodedNetworkMessageWithCompactRelayV1 {
    pub message: NetworkMessage,
    pub compact_relay: Option<CompactRelayCarrierV1>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactRelayCarrierErrorV1 {
    Json(String),
    InvalidJsonRoot,
    UnsupportedCarrierKind { kind: String },
    EmptyTargetPeerId,
    TargetPeerIdTooLarge { observed: usize, maximum: usize },
    CarrierTooLarge { observed: usize, maximum: usize },
    ChainIdMismatch { expected: String, observed: String },
    UnsupportedVersion(u16),
    CapabilityBoundsMismatch,
    EmptyTransactionResponse,
    TransactionResponseTooLarge { observed: usize, maximum: usize },
    Compact(CompactRelayErrorV1),
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CompactRelayKindV1 {
    CapabilityProbe,
    Capabilities,
    Announce,
    GetTransactions,
    Transactions,
}

#[derive(Debug, Deserialize)]
struct CompactRelayWireRawV1<'a> {
    compact_relay_type: CompactRelayKindV1,
    #[serde(default, borrow)]
    payload: Option<&'a RawValue>,
}

#[derive(Debug, Deserialize)]
struct CompactRelayCarrierRawV1<'a> {
    target_peer_id: String,
    chain_id: String,
    #[serde(borrow)]
    wire: &'a RawValue,
}

#[derive(Debug, Deserialize)]
struct CompactRelayExtensionRawV1<'a> {
    #[serde(default, borrow, rename = "pulsedag_compact_relay_v1")]
    compact_relay: Option<&'a RawValue>,
}

#[derive(Debug, Deserialize)]
struct CompactRelayTargetV1<'a> {
    #[serde(default, borrow)]
    target_peer_id: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct CompactRelayTargetExtensionV1<'a> {
    #[serde(default, borrow, rename = "pulsedag_compact_relay_v1")]
    compact_relay: Option<CompactRelayTargetV1<'a>>,
}

#[derive(Debug)]
struct BoundedVecV1<T, const MAXIMUM: usize>(Vec<T>);

struct BoundedVecVisitorV1<T, const MAXIMUM: usize> {
    marker: PhantomData<T>,
}

impl<'de, T, const MAXIMUM: usize> Visitor<'de> for BoundedVecVisitorV1<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    type Value = BoundedVecV1<T, MAXIMUM>;

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
                None => return Ok(BoundedVecV1(values)),
            }
        }

        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(format!(
                "sequence exceeds maximum item count {MAXIMUM}"
            )));
        }
        Ok(BoundedVecV1(values))
    }
}

impl<'de, T, const MAXIMUM: usize> Deserialize<'de> for BoundedVecV1<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(BoundedVecVisitorV1::<T, MAXIMUM> {
            marker: PhantomData,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CompactBlockHeaderDecodeV1 {
    version: u32,
    parents: BoundedVecV1<Hash, { GHOSTDAG_V1_MAX_PARENTS }>,
    timestamp: u64,
    difficulty: u32,
    nonce: u64,
    merkle_root: Hash,
    state_root: Hash,
    blue_score: u64,
    height: u64,
}

impl CompactBlockHeaderDecodeV1 {
    fn into_header(self) -> BlockHeader {
        BlockHeader {
            version: self.version,
            parents: self.parents.0,
            timestamp: self.timestamp,
            difficulty: self.difficulty,
            nonce: self.nonce,
            merkle_root: self.merkle_root,
            state_root: self.state_root,
            blue_score: self.blue_score,
            height: self.height,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CompactBlockAnnouncementDecodeV1 {
    version: u16,
    block_hash: Hash,
    header: CompactBlockHeaderDecodeV1,
    txids: BoundedVecV1<Hash, { P2P_WIRE_MAX_INVENTORY_ITEMS_V1 }>,
}

#[derive(Debug, Deserialize)]
struct CompactTransactionRequestDecodeV1 {
    version: u16,
    block_hash: Hash,
    txids: BoundedVecV1<Hash, { P2P_WIRE_MAX_REQUEST_ITEMS_V1 }>,
}

#[derive(Debug, Deserialize)]
struct CompactTransactionResponseDecodeV1 {
    version: u16,
    block_hash: Hash,
    transactions: BoundedVecV1<Transaction, { P2P_WIRE_MAX_REQUEST_ITEMS_V1 }>,
}

fn parse_payload<'de, T>(raw: &'de RawValue, field: &str) -> Result<T, CompactRelayCarrierErrorV1>
where
    T: Deserialize<'de>,
{
    serde_json::from_str(raw.get()).map_err(|error| {
        CompactRelayCarrierErrorV1::Json(format!("compact-relay {field}: {error}"))
    })
}

fn validate_response_shape(
    response: &CompactTransactionResponseV1,
) -> Result<(), CompactRelayCarrierErrorV1> {
    if response.version != COMPACT_DAG_RELAY_VERSION_V1 {
        return Err(CompactRelayCarrierErrorV1::UnsupportedVersion(
            response.version,
        ));
    }
    if response.transactions.is_empty() {
        return Err(CompactRelayCarrierErrorV1::EmptyTransactionResponse);
    }
    if response.transactions.len() > P2P_WIRE_MAX_REQUEST_ITEMS_V1 {
        return Err(CompactRelayCarrierErrorV1::TransactionResponseTooLarge {
            observed: response.transactions.len(),
            maximum: P2P_WIRE_MAX_REQUEST_ITEMS_V1,
        });
    }
    Ok(())
}

fn decode_wire(
    raw: &RawValue,
    chain_id: &str,
) -> Result<CompactRelayWireV1, CompactRelayCarrierErrorV1> {
    let raw: CompactRelayWireRawV1<'_> = parse_payload(raw, "wire")?;
    let decoded = match raw.compact_relay_type {
        CompactRelayKindV1::CapabilityProbe => {
            if raw.payload.is_some() {
                return Err(CompactRelayCarrierErrorV1::Json(
                    "compact-relay capability probe must not contain payload".to_string(),
                ));
            }
            CompactRelayWireV1::CapabilityProbe
        }
        CompactRelayKindV1::Capabilities => {
            let payload = raw.payload.ok_or_else(|| {
                CompactRelayCarrierErrorV1::Json(
                    "compact-relay capabilities payload is missing".to_string(),
                )
            })?;
            CompactRelayWireV1::Capabilities(parse_payload(payload, "capabilities")?)
        }
        CompactRelayKindV1::Announce => {
            let payload = raw.payload.ok_or_else(|| {
                CompactRelayCarrierErrorV1::Json(
                    "compact-relay announcement payload is missing".to_string(),
                )
            })?;
            let decoded: CompactBlockAnnouncementDecodeV1 = parse_payload(payload, "announcement")?;
            CompactRelayWireV1::Announce(CompactBlockAnnouncementV1 {
                version: decoded.version,
                block_hash: decoded.block_hash,
                header: decoded.header.into_header(),
                txids: decoded.txids.0,
            })
        }
        CompactRelayKindV1::GetTransactions => {
            let payload = raw.payload.ok_or_else(|| {
                CompactRelayCarrierErrorV1::Json(
                    "compact-relay transaction request payload is missing".to_string(),
                )
            })?;
            let decoded: CompactTransactionRequestDecodeV1 =
                parse_payload(payload, "transaction request")?;
            CompactRelayWireV1::GetTransactions(CompactTransactionRequestV1 {
                version: decoded.version,
                block_hash: decoded.block_hash,
                txids: decoded.txids.0,
            })
        }
        CompactRelayKindV1::Transactions => {
            let payload = raw.payload.ok_or_else(|| {
                CompactRelayCarrierErrorV1::Json(
                    "compact-relay transaction response payload is missing".to_string(),
                )
            })?;
            let decoded: CompactTransactionResponseDecodeV1 =
                parse_payload(payload, "transaction response")?;
            CompactRelayWireV1::Transactions(CompactTransactionResponseV1 {
                version: decoded.version,
                block_hash: decoded.block_hash,
                transactions: decoded.transactions.0,
            })
        }
    };
    decoded.validate_for_chain(chain_id)?;
    Ok(decoded)
}

fn decode_extension(
    bytes: &[u8],
) -> Result<Option<CompactRelayCarrierV1>, CompactRelayCarrierErrorV1> {
    let extension: CompactRelayExtensionRawV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    extension
        .compact_relay
        .map(|raw| {
            let carrier: CompactRelayCarrierRawV1<'_> = parse_payload(raw, "carrier")?;
            let wire = decode_wire(carrier.wire, &carrier.chain_id)?;
            Ok(CompactRelayCarrierV1 {
                target_peer_id: carrier.target_peer_id,
                chain_id: carrier.chain_id,
                wire,
            })
        })
        .transpose()
}

fn validate_carrier_for_message(
    message: &NetworkMessage,
    carrier: &CompactRelayCarrierV1,
) -> Result<(), CompactRelayCarrierErrorV1> {
    if !matches!(message, NetworkMessage::Tips { .. }) {
        return Err(CompactRelayCarrierErrorV1::UnsupportedCarrierKind {
            kind: message.kind().to_string(),
        });
    }
    if carrier.target_peer_id.trim().is_empty() {
        return Err(CompactRelayCarrierErrorV1::EmptyTargetPeerId);
    }
    if carrier.target_peer_id.len() > COMPACT_RELAY_MAX_TARGET_PEER_ID_BYTES_V1 {
        return Err(CompactRelayCarrierErrorV1::TargetPeerIdTooLarge {
            observed: carrier.target_peer_id.len(),
            maximum: COMPACT_RELAY_MAX_TARGET_PEER_ID_BYTES_V1,
        });
    }
    if carrier.chain_id != message.chain_id() {
        return Err(CompactRelayCarrierErrorV1::ChainIdMismatch {
            expected: message.chain_id().to_string(),
            observed: carrier.chain_id.clone(),
        });
    }
    carrier.wire.validate_for_chain(&carrier.chain_id)
}

fn extension_present_and_bounded(bytes: &[u8]) -> Result<bool, CompactRelayCarrierErrorV1> {
    let extension: CompactRelayExtensionRawV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    let present = extension.compact_relay.is_some();
    if present && bytes.len() > COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1 {
        return Err(CompactRelayCarrierErrorV1::CarrierTooLarge {
            observed: bytes.len(),
            maximum: COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1,
        });
    }
    Ok(present)
}

pub fn attach_compact_relay_carrier_v1(
    encoded_network_message: &[u8],
    carrier: &CompactRelayCarrierV1,
) -> Result<Vec<u8>, CompactRelayCarrierErrorV1> {
    let message: NetworkMessage = serde_json::from_slice(encoded_network_message)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    validate_carrier_for_message(&message, carrier)?;

    let mut value: Value = serde_json::from_slice(encoded_network_message)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or(CompactRelayCarrierErrorV1::InvalidJsonRoot)?;
    object.insert(
        COMPACT_RELAY_EXTENSION_FIELD_V1.to_string(),
        serde_json::to_value(carrier)
            .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?,
    );
    let encoded = serde_json::to_vec(&value)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    if encoded.len() > COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1 {
        return Err(CompactRelayCarrierErrorV1::CarrierTooLarge {
            observed: encoded.len(),
            maximum: COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1,
        });
    }
    Ok(encoded)
}

pub fn compact_relay_wire_fits_minimal_transport_v1(
    target_peer_id: &str,
    chain_id: &str,
    wire: &CompactRelayWireV1,
) -> bool {
    let message = NetworkMessage::Tips {
        chain_id: chain_id.to_string(),
        tips: Vec::new(),
        inventory: None,
    };
    let carrier = CompactRelayCarrierV1 {
        target_peer_id: target_peer_id.to_string(),
        chain_id: chain_id.to_string(),
        wire: wire.clone(),
    };
    encode_network_message_with_compact_relay_v1(&message, Some(&carrier)).is_ok()
}

pub fn encode_network_message_with_compact_relay_v1(
    message: &NetworkMessage,
    carrier: Option<&CompactRelayCarrierV1>,
) -> Result<Vec<u8>, CompactRelayCarrierErrorV1> {
    let legacy = serde_json::to_vec(message)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    match carrier {
        Some(carrier) => attach_compact_relay_carrier_v1(&legacy, carrier),
        None => Ok(legacy),
    }
}

pub fn decode_network_message_with_compact_relay_v1(
    bytes: &[u8],
) -> Result<DecodedNetworkMessageWithCompactRelayV1, CompactRelayCarrierErrorV1> {
    let present = extension_present_and_bounded(bytes)?;
    let message: NetworkMessage = serde_json::from_slice(bytes)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    let compact_relay = if present {
        decode_extension(bytes)?
    } else {
        None
    };
    if let Some(carrier) = compact_relay.as_ref() {
        validate_carrier_for_message(&message, carrier)?;
    }
    Ok(DecodedNetworkMessageWithCompactRelayV1 {
        message,
        compact_relay,
    })
}

pub fn decode_network_message_with_compact_relay_for_peer_v1(
    bytes: &[u8],
    local_peer_id: &str,
) -> Result<DecodedNetworkMessageWithCompactRelayV1, CompactRelayCarrierErrorV1> {
    let target_extension: CompactRelayTargetExtensionV1<'_> = serde_json::from_slice(bytes)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    let has_extension = target_extension.compact_relay.is_some();
    let addressed = target_extension
        .compact_relay
        .as_ref()
        .and_then(|target| target.target_peer_id)
        .is_some_and(|target| target == local_peer_id);

    if has_extension && bytes.len() > COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1 {
        return Err(CompactRelayCarrierErrorV1::CarrierTooLarge {
            observed: bytes.len(),
            maximum: COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1,
        });
    }

    let message: NetworkMessage = serde_json::from_slice(bytes)
        .map_err(|error| CompactRelayCarrierErrorV1::Json(error.to_string()))?;
    let compact_relay = if addressed {
        let carrier = decode_extension(bytes)?.ok_or_else(|| {
            CompactRelayCarrierErrorV1::Json(
                "compact-relay target present without full carrier".to_string(),
            )
        })?;
        validate_carrier_for_message(&message, &carrier)?;
        Some(carrier)
    } else {
        None
    };

    Ok(DecodedNetworkMessageWithCompactRelayV1 {
        message,
        compact_relay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::build_compact_block_announcement_v1;
    use pulsedag_core::types::{compute_merkle_root, Block, TxOutput};

    const CHAIN_ID: &str = "compact-relay-testnet";
    const LOCAL_PEER: &str = "peer-compact-local";

    fn transaction(txid: &str) -> Transaction {
        Transaction {
            txid: txid.into(),
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                address: "recipient".into(),
                amount: 1,
            }],
            fee: 0,
            nonce: 0,
        }
    }

    fn block() -> Block {
        let transactions = vec![
            transaction("coinbase"),
            transaction("tx-a"),
            transaction("tx-b"),
        ];
        Block {
            hash: "block-hash".into(),
            header: BlockHeader {
                version: 1,
                parents: vec!["parent-a".into(), "parent-b".into()],
                timestamp: 1,
                difficulty: 1,
                nonce: 1,
                merkle_root: compute_merkle_root(&transactions),
                state_root: "state".into(),
                blue_score: 2,
                height: 2,
            },
            transactions,
        }
    }

    fn tips() -> NetworkMessage {
        NetworkMessage::Tips {
            chain_id: CHAIN_ID.into(),
            tips: vec!["tip".into()],
            inventory: None,
        }
    }

    fn carrier(wire: CompactRelayWireV1) -> CompactRelayCarrierV1 {
        CompactRelayCarrierV1 {
            target_peer_id: LOCAL_PEER.into(),
            chain_id: CHAIN_ID.into(),
            wire,
        }
    }

    #[test]
    fn no_extension_preserves_legacy_wire_bytes_exactly() {
        let message = tips();
        assert_eq!(
            encode_network_message_with_compact_relay_v1(&message, None).unwrap(),
            serde_json::to_vec(&message).unwrap()
        );
    }

    #[test]
    fn legacy_decoder_ignores_compact_relay_extension() {
        let announcement = build_compact_block_announcement_v1(&block()).unwrap();
        let encoded = encode_network_message_with_compact_relay_v1(
            &tips(),
            Some(&carrier(CompactRelayWireV1::Announce(announcement))),
        )
        .unwrap();

        let legacy: NetworkMessage = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(legacy.kind(), "Tips");
        assert_eq!(legacy.chain_id(), CHAIN_ID);
    }

    #[test]
    fn addressed_peer_decodes_bounded_announcement() {
        let announcement = build_compact_block_announcement_v1(&block()).unwrap();
        let encoded = encode_network_message_with_compact_relay_v1(
            &tips(),
            Some(&carrier(CompactRelayWireV1::Announce(announcement))),
        )
        .unwrap();

        let decoded =
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, LOCAL_PEER).unwrap();
        let compact = decoded.compact_relay.expect("addressed carrier");
        assert_eq!(compact.chain_id, CHAIN_ID);
        match compact.wire {
            CompactRelayWireV1::Announce(announcement) => {
                assert_eq!(announcement.block_hash, "block-hash");
                assert_eq!(announcement.txids, vec!["coinbase", "tx-a", "tx-b"]);
            }
            other => panic!("unexpected compact wire: {}", other.kind()),
        }
    }

    #[test]
    fn unaddressed_peer_ignores_extension_payload() {
        let announcement = build_compact_block_announcement_v1(&block()).unwrap();
        let encoded = encode_network_message_with_compact_relay_v1(
            &tips(),
            Some(&carrier(CompactRelayWireV1::Announce(announcement))),
        )
        .unwrap();

        let decoded =
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, "different-peer")
                .unwrap();
        assert!(decoded.compact_relay.is_none());
        assert_eq!(decoded.message.kind(), "Tips");
    }

    #[test]
    fn compact_relay_is_rejected_on_non_tip_carrier() {
        let base = NetworkMessage::GetBlock {
            chain_id: CHAIN_ID.into(),
            hash: "block-hash".into(),
            request_id: None,
            requesting_peer_id: None,
            requested_peer_id: None,
            request_kind: None,
        };
        let result = encode_network_message_with_compact_relay_v1(
            &base,
            Some(&carrier(CompactRelayWireV1::CapabilityProbe)),
        );
        assert!(matches!(
            result,
            Err(CompactRelayCarrierErrorV1::UnsupportedCarrierKind { .. })
        ));
    }

    #[test]
    fn carrier_chain_must_match_legacy_envelope() {
        let mut wrong = carrier(CompactRelayWireV1::CapabilityProbe);
        wrong.chain_id = "different-chain".into();
        assert_eq!(
            encode_network_message_with_compact_relay_v1(&tips(), Some(&wrong)),
            Err(CompactRelayCarrierErrorV1::ChainIdMismatch {
                expected: CHAIN_ID.into(),
                observed: "different-chain".into(),
            })
        );
    }

    #[test]
    fn capability_bounds_are_exact_and_fail_closed() {
        let mut capabilities = CompactRelayCapabilitiesV1::canonical(CHAIN_ID);
        capabilities.max_request_txids += 1;
        assert_eq!(
            encode_network_message_with_compact_relay_v1(
                &tips(),
                Some(&carrier(CompactRelayWireV1::Capabilities(capabilities))),
            ),
            Err(CompactRelayCarrierErrorV1::CapabilityBoundsMismatch)
        );
    }

    #[test]
    fn oversized_request_is_rejected_during_decode() {
        let hashes = (0..=P2P_WIRE_MAX_REQUEST_ITEMS_V1)
            .map(|index| format!("tx-{index}"))
            .collect::<Vec<_>>();
        let mut value = serde_json::to_value(tips()).unwrap();
        value.as_object_mut().unwrap().insert(
            COMPACT_RELAY_EXTENSION_FIELD_V1.to_string(),
            serde_json::json!({
                "target_peer_id": LOCAL_PEER,
                "chain_id": CHAIN_ID,
                "wire": {
                    "compact_relay_type": "get_transactions",
                    "payload": {
                        "version": COMPACT_DAG_RELAY_VERSION_V1,
                        "block_hash": "block-hash",
                        "txids": hashes
                    }
                }
            }),
        );
        let encoded = serde_json::to_vec(&value).unwrap();

        assert!(matches!(
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, LOCAL_PEER),
            Err(CompactRelayCarrierErrorV1::Json(message))
                if message.contains("sequence exceeds maximum item count")
        ));
    }

    #[test]
    fn oversized_announcement_parent_set_is_rejected_while_streaming() {
        let mut value = serde_json::to_value(tips()).unwrap();
        let mut header = serde_json::to_value(block().header).unwrap();
        header.as_object_mut().unwrap().insert(
            "parents".to_string(),
            serde_json::json!(vec![""; GHOSTDAG_V1_MAX_PARENTS + 1]),
        );
        value.as_object_mut().unwrap().insert(
            COMPACT_RELAY_EXTENSION_FIELD_V1.to_string(),
            serde_json::json!({
                "target_peer_id": LOCAL_PEER,
                "chain_id": CHAIN_ID,
                "wire": {
                    "compact_relay_type": "announce",
                    "payload": {
                        "version": COMPACT_DAG_RELAY_VERSION_V1,
                        "block_hash": "block-hash",
                        "header": header,
                        "txids": ["coinbase"]
                    }
                }
            }),
        );
        let encoded = serde_json::to_vec(&value).unwrap();
        assert!(encoded.len() < COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1);

        assert!(matches!(
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, LOCAL_PEER),
            Err(CompactRelayCarrierErrorV1::Json(message))
                if message.contains("sequence exceeds maximum item count")
        ));
    }

    #[test]
    fn oversized_announcement_inventory_is_rejected_while_streaming() {
        let mut value = serde_json::to_value(tips()).unwrap();
        value.as_object_mut().unwrap().insert(
            COMPACT_RELAY_EXTENSION_FIELD_V1.to_string(),
            serde_json::json!({
                "target_peer_id": LOCAL_PEER,
                "chain_id": CHAIN_ID,
                "wire": {
                    "compact_relay_type": "announce",
                    "payload": {
                        "version": COMPACT_DAG_RELAY_VERSION_V1,
                        "block_hash": "block-hash",
                        "header": block().header,
                        "txids": vec![""; P2P_WIRE_MAX_INVENTORY_ITEMS_V1 + 1]
                    }
                }
            }),
        );
        let encoded = serde_json::to_vec(&value).unwrap();
        assert!(encoded.len() < COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1);

        assert!(matches!(
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, LOCAL_PEER),
            Err(CompactRelayCarrierErrorV1::Json(message))
                if message.contains("sequence exceeds maximum item count")
        ));
    }

    #[test]
    fn oversized_transaction_response_is_rejected_while_streaming() {
        let minimal = transaction("x");
        let mut value = serde_json::to_value(tips()).unwrap();
        value.as_object_mut().unwrap().insert(
            COMPACT_RELAY_EXTENSION_FIELD_V1.to_string(),
            serde_json::json!({
                "target_peer_id": LOCAL_PEER,
                "chain_id": CHAIN_ID,
                "wire": {
                    "compact_relay_type": "transactions",
                    "payload": {
                        "version": COMPACT_DAG_RELAY_VERSION_V1,
                        "block_hash": "block-hash",
                        "transactions": vec![minimal; P2P_WIRE_MAX_REQUEST_ITEMS_V1 + 1]
                    }
                }
            }),
        );
        let encoded = serde_json::to_vec(&value).unwrap();
        assert!(encoded.len() < COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1);

        assert!(matches!(
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, LOCAL_PEER),
            Err(CompactRelayCarrierErrorV1::Json(message))
                if message.contains("sequence exceeds maximum item count")
        ));
    }

    #[test]
    fn oversized_carrier_is_rejected_before_full_extension_decode() {
        let mut value = serde_json::to_value(tips()).unwrap();
        value.as_object_mut().unwrap().insert(
            COMPACT_RELAY_EXTENSION_FIELD_V1.to_string(),
            serde_json::json!({
                "target_peer_id": "x".repeat(COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1),
                "chain_id": CHAIN_ID,
                "wire": {"compact_relay_type": "capability_probe"}
            }),
        );
        let encoded = serde_json::to_vec(&value).unwrap();
        assert!(encoded.len() > COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1);

        assert!(matches!(
            decode_network_message_with_compact_relay_for_peer_v1(&encoded, LOCAL_PEER),
            Err(CompactRelayCarrierErrorV1::CarrierTooLarge { .. })
        ));
    }

    #[test]
    fn transaction_response_count_is_bounded() {
        let response = CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "block-hash".into(),
            transactions: (0..=P2P_WIRE_MAX_REQUEST_ITEMS_V1)
                .map(|index| transaction(&format!("tx-{index}")))
                .collect(),
        };
        assert!(matches!(
            encode_network_message_with_compact_relay_v1(
                &tips(),
                Some(&carrier(CompactRelayWireV1::Transactions(response))),
            ),
            Err(CompactRelayCarrierErrorV1::TransactionResponseTooLarge { .. })
        ));
    }

    #[test]
    fn carrier_transport_size_is_bounded_on_encode() {
        let response = CompactTransactionResponseV1 {
            version: COMPACT_DAG_RELAY_VERSION_V1,
            block_hash: "block-hash".into(),
            transactions: vec![Transaction {
                txid: "tx-large".into(),
                version: 1,
                inputs: vec![],
                outputs: vec![TxOutput {
                    address: "a".repeat(COMPACT_RELAY_TRANSPORT_MAX_BYTES_V1),
                    amount: 1,
                }],
                fee: 0,
                nonce: 0,
            }],
        };
        assert!(matches!(
            encode_network_message_with_compact_relay_v1(
                &tips(),
                Some(&carrier(CompactRelayWireV1::Transactions(response))),
            ),
            Err(CompactRelayCarrierErrorV1::CarrierTooLarge { .. })
        ));
    }
}
