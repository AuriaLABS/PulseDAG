use std::fmt;

use pulsedag_core::types::{Block, Hash, Transaction};
use serde::de::{self, value::MapAccessDeserializer, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::{BlockHeaderAnnouncement, HeaderInventory, NetworkMessage, TipInventoryStatus};

#[derive(Deserialize)]
struct NewTransactionPayload {
    chain_id: String,
    transaction: Transaction,
}

#[derive(Deserialize)]
struct NewBlockPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::deserialize_supported_block")]
    block: Block,
}

#[derive(Deserialize)]
struct HashPayload {
    chain_id: String,
    hash: Hash,
}

#[derive(Deserialize)]
struct InventoryPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_inventory_hashes")]
    hashes: Vec<Hash>,
}

#[derive(Deserialize)]
struct GetHeadersPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_locator_hashes")]
    locator: Vec<Hash>,
    stop_hash: Option<Hash>,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_response_limit")]
    limit: usize,
}

#[derive(Deserialize)]
struct HeadersPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_header_inventory")]
    headers: Vec<HeaderInventory>,
}

#[derive(Deserialize)]
struct GetTipsPayload {
    chain_id: String,
    #[serde(default)]
    inventory: Option<TipInventoryStatus>,
}

#[derive(Deserialize)]
struct TipsPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_inventory_hashes")]
    tips: Vec<Hash>,
    #[serde(default)]
    inventory: Option<TipInventoryStatus>,
}

#[derive(Deserialize)]
struct GetBlockHeadersPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_request_hashes")]
    hashes: Vec<Hash>,
}

#[derive(Deserialize)]
struct BlockHeadersPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::wire_limits_v1::deserialize_block_header_announcements")]
    headers: Vec<BlockHeaderAnnouncement>,
}

#[derive(Deserialize)]
struct GetBlockPayload {
    chain_id: String,
    hash: Hash,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    requesting_peer_id: Option<String>,
    #[serde(default)]
    requested_peer_id: Option<String>,
    #[serde(default)]
    request_kind: Option<String>,
}

#[derive(Deserialize)]
struct BlockDataPayload {
    chain_id: String,
    #[serde(default, deserialize_with = "super::deserialize_supported_optional_block")]
    block: Option<Block>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    request_hash: Option<Hash>,
}

#[derive(Deserialize)]
struct BlockPayload {
    chain_id: String,
    #[serde(deserialize_with = "super::deserialize_supported_block")]
    block: Block,
}

#[derive(Deserialize)]
struct TextPayload {
    chain_id: String,
    reason: String,
}

#[derive(Deserialize)]
struct ErrorPayload {
    chain_id: String,
    message: String,
}

#[derive(Debug, Clone, Copy)]
enum NetworkMessageTag {
    NewTransaction,
    NewBlock,
    BlockAnnounce,
    NewBlockHash,
    InvBlock,
    GetHeaders,
    Headers,
    GetTips,
    Tips,
    GetBlockHeaders,
    BlockHeaders,
    GetBlock,
    BlockData,
    Block,
    Reject,
    Error,
}

impl<'de> Deserialize<'de> for NetworkMessageTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TagVisitor;

        impl Visitor<'_> for TagVisitor {
            type Value = NetworkMessageTag;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a supported PulseDAG network message type")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                match value {
                    "NewTransaction" => Ok(NetworkMessageTag::NewTransaction),
                    "NewBlock" => Ok(NetworkMessageTag::NewBlock),
                    "BlockAnnounce" => Ok(NetworkMessageTag::BlockAnnounce),
                    "NewBlockHash" => Ok(NetworkMessageTag::NewBlockHash),
                    "InvBlock" => Ok(NetworkMessageTag::InvBlock),
                    "GetHeaders" => Ok(NetworkMessageTag::GetHeaders),
                    "Headers" => Ok(NetworkMessageTag::Headers),
                    "GetTips" => Ok(NetworkMessageTag::GetTips),
                    "Tips" => Ok(NetworkMessageTag::Tips),
                    "GetBlockHeaders" => Ok(NetworkMessageTag::GetBlockHeaders),
                    "BlockHeaders" => Ok(NetworkMessageTag::BlockHeaders),
                    "GetBlock" => Ok(NetworkMessageTag::GetBlock),
                    "BlockData" => Ok(NetworkMessageTag::BlockData),
                    "Block" => Ok(NetworkMessageTag::Block),
                    "Reject" => Ok(NetworkMessageTag::Reject),
                    "Error" => Ok(NetworkMessageTag::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &[
                            "NewTransaction",
                            "NewBlock",
                            "BlockAnnounce",
                            "NewBlockHash",
                            "InvBlock",
                            "GetHeaders",
                            "Headers",
                            "GetTips",
                            "Tips",
                            "GetBlockHeaders",
                            "BlockHeaders",
                            "GetBlock",
                            "BlockData",
                            "Block",
                            "Reject",
                            "Error",
                        ],
                    )),
                }
            }
        }

        deserializer.deserialize_str(TagVisitor)
    }
}

struct NetworkMessageVisitor;

impl<'de> Visitor<'de> for NetworkMessageVisitor {
    type Value = NetworkMessage;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a canonical PulseDAG network message object with type first")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let first_key = map
            .next_key::<String>()?
            .ok_or_else(|| de::Error::custom("network message object must not be empty"))?;
        if first_key != "type" {
            return Err(de::Error::custom(
                "network message must encode the type field first",
            ));
        }
        let tag = map.next_value::<NetworkMessageTag>()?;
        let remainder = MapAccessDeserializer::new(map);

        match tag {
            NetworkMessageTag::NewTransaction => {
                let payload = NewTransactionPayload::deserialize(remainder)?;
                Ok(NetworkMessage::NewTransaction {
                    chain_id: payload.chain_id,
                    transaction: payload.transaction,
                })
            }
            NetworkMessageTag::NewBlock => {
                let payload = NewBlockPayload::deserialize(remainder)?;
                Ok(NetworkMessage::NewBlock {
                    chain_id: payload.chain_id,
                    block: payload.block,
                })
            }
            NetworkMessageTag::BlockAnnounce => {
                let payload = HashPayload::deserialize(remainder)?;
                Ok(NetworkMessage::BlockAnnounce {
                    chain_id: payload.chain_id,
                    hash: payload.hash,
                })
            }
            NetworkMessageTag::NewBlockHash => {
                let payload = HashPayload::deserialize(remainder)?;
                Ok(NetworkMessage::NewBlockHash {
                    chain_id: payload.chain_id,
                    hash: payload.hash,
                })
            }
            NetworkMessageTag::InvBlock => {
                let payload = InventoryPayload::deserialize(remainder)?;
                Ok(NetworkMessage::InvBlock {
                    chain_id: payload.chain_id,
                    hashes: payload.hashes,
                })
            }
            NetworkMessageTag::GetHeaders => {
                let payload = GetHeadersPayload::deserialize(remainder)?;
                Ok(NetworkMessage::GetHeaders {
                    chain_id: payload.chain_id,
                    locator: payload.locator,
                    stop_hash: payload.stop_hash,
                    limit: payload.limit,
                })
            }
            NetworkMessageTag::Headers => {
                let payload = HeadersPayload::deserialize(remainder)?;
                Ok(NetworkMessage::Headers {
                    chain_id: payload.chain_id,
                    headers: payload.headers,
                })
            }
            NetworkMessageTag::GetTips => {
                let payload = GetTipsPayload::deserialize(remainder)?;
                Ok(NetworkMessage::GetTips {
                    chain_id: payload.chain_id,
                    inventory: payload.inventory,
                })
            }
            NetworkMessageTag::Tips => {
                let payload = TipsPayload::deserialize(remainder)?;
                Ok(NetworkMessage::Tips {
                    chain_id: payload.chain_id,
                    tips: payload.tips,
                    inventory: payload.inventory,
                })
            }
            NetworkMessageTag::GetBlockHeaders => {
                let payload = GetBlockHeadersPayload::deserialize(remainder)?;
                Ok(NetworkMessage::GetBlockHeaders {
                    chain_id: payload.chain_id,
                    hashes: payload.hashes,
                })
            }
            NetworkMessageTag::BlockHeaders => {
                let payload = BlockHeadersPayload::deserialize(remainder)?;
                Ok(NetworkMessage::BlockHeaders {
                    chain_id: payload.chain_id,
                    headers: payload.headers,
                })
            }
            NetworkMessageTag::GetBlock => {
                let payload = GetBlockPayload::deserialize(remainder)?;
                Ok(NetworkMessage::GetBlock {
                    chain_id: payload.chain_id,
                    hash: payload.hash,
                    request_id: payload.request_id,
                    requesting_peer_id: payload.requesting_peer_id,
                    requested_peer_id: payload.requested_peer_id,
                    request_kind: payload.request_kind,
                })
            }
            NetworkMessageTag::BlockData => {
                let payload = BlockDataPayload::deserialize(remainder)?;
                Ok(NetworkMessage::BlockData {
                    chain_id: payload.chain_id,
                    block: payload.block,
                    request_id: payload.request_id,
                    request_hash: payload.request_hash,
                })
            }
            NetworkMessageTag::Block => {
                let payload = BlockPayload::deserialize(remainder)?;
                Ok(NetworkMessage::Block {
                    chain_id: payload.chain_id,
                    block: payload.block,
                })
            }
            NetworkMessageTag::Reject => {
                let payload = TextPayload::deserialize(remainder)?;
                Ok(NetworkMessage::Reject {
                    chain_id: payload.chain_id,
                    reason: payload.reason,
                })
            }
            NetworkMessageTag::Error => {
                let payload = ErrorPayload::deserialize(remainder)?;
                Ok(NetworkMessage::Error {
                    chain_id: payload.chain_id,
                    message: payload.message,
                })
            }
        }
    }
}

impl<'de> Deserialize<'de> for NetworkMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(NetworkMessageVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_noncanonical_type_order_before_payload_decode() {
        let wire = br#"{"chain_id":"testnet","type":"InvBlock","hashes":[]}"#;
        let error = serde_json::from_slice::<NetworkMessage>(wire).unwrap_err();
        assert!(error.to_string().contains("type field first"));
    }

    #[test]
    fn canonical_type_first_inventory_decodes() {
        let wire = br#"{"type":"InvBlock","chain_id":"testnet","hashes":["a"]}"#;
        let decoded = serde_json::from_slice::<NetworkMessage>(wire).expect("decode canonical wire");
        match decoded {
            NetworkMessage::InvBlock { chain_id, hashes } => {
                assert_eq!(chain_id, "testnet");
                assert_eq!(hashes, vec!["a".to_string()]);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }
}
