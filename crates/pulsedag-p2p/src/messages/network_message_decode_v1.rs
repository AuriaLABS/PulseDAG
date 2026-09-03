use pulsedag_core::types::{Block, Hash, Transaction};
use serde::de;
use serde::{Deserialize, Deserializer};

use super::{
    BlockHeaderAnnouncement, HeaderInventory, NetworkMessage, TipInventoryStatus,
    P2P_WIRE_MAX_REQUEST_ITEMS_V1,
};

#[derive(Debug, Clone, Copy, Deserialize)]
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

/// Streaming wire shape used to preserve legacy field-order tolerance while
/// applying allocation bounds before a complete attacker-controlled vector is
/// materialized. Unknown top-level extension fields remain legacy-compatible:
/// Serde consumes and ignores them without changing `NetworkMessage`.
#[derive(Deserialize)]
struct NetworkMessageWire {
    #[serde(rename = "type")]
    tag: NetworkMessageTag,
    chain_id: String,
    #[serde(default)]
    transaction: Option<Transaction>,
    #[serde(
        default,
        deserialize_with = "super::deserialize_supported_optional_block"
    )]
    block: Option<Block>,
    #[serde(default)]
    hash: Option<Hash>,
    #[serde(
        default,
        deserialize_with = "super::wire_limits_v1::deserialize_optional_inventory_hashes"
    )]
    hashes: Option<Vec<Hash>>,
    #[serde(
        default,
        deserialize_with = "super::wire_limits_v1::deserialize_optional_locator_hashes"
    )]
    locator: Option<Vec<Hash>>,
    #[serde(default)]
    stop_hash: Option<Hash>,
    #[serde(
        default,
        deserialize_with = "super::wire_limits_v1::deserialize_optional_response_limit"
    )]
    limit: Option<usize>,
    #[serde(
        default,
        deserialize_with = "super::wire_limits_v1::deserialize_optional_header_inventory"
    )]
    headers: Option<Vec<HeaderInventory>>,
    #[serde(default)]
    inventory: Option<TipInventoryStatus>,
    #[serde(
        default,
        deserialize_with = "super::wire_limits_v1::deserialize_optional_inventory_hashes"
    )]
    tips: Option<Vec<Hash>>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    requesting_peer_id: Option<String>,
    #[serde(default)]
    requested_peer_id: Option<String>,
    #[serde(default)]
    request_kind: Option<String>,
    #[serde(default)]
    request_hash: Option<Hash>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

fn required_field<T, E>(value: Option<T>, field: &'static str) -> Result<T, E>
where
    E: de::Error,
{
    value.ok_or_else(|| E::missing_field(field))
}

fn enforce_request_fanout<E>(count: usize, field: &'static str) -> Result<(), E>
where
    E: de::Error,
{
    if count > P2P_WIRE_MAX_REQUEST_ITEMS_V1 {
        return Err(E::custom(format!(
            "{field} exceeds maximum item count {P2P_WIRE_MAX_REQUEST_ITEMS_V1}"
        )));
    }
    Ok(())
}

impl NetworkMessageWire {
    fn into_message<E>(self) -> Result<NetworkMessage, E>
    where
        E: de::Error,
    {
        match self.tag {
            NetworkMessageTag::NewTransaction => Ok(NetworkMessage::NewTransaction {
                chain_id: self.chain_id,
                transaction: required_field(self.transaction, "transaction")?,
            }),
            NetworkMessageTag::NewBlock => Ok(NetworkMessage::NewBlock {
                chain_id: self.chain_id,
                block: required_field(self.block, "block")?,
            }),
            NetworkMessageTag::BlockAnnounce => Ok(NetworkMessage::BlockAnnounce {
                chain_id: self.chain_id,
                hash: required_field(self.hash, "hash")?,
            }),
            NetworkMessageTag::NewBlockHash => Ok(NetworkMessage::NewBlockHash {
                chain_id: self.chain_id,
                hash: required_field(self.hash, "hash")?,
            }),
            NetworkMessageTag::InvBlock => Ok(NetworkMessage::InvBlock {
                chain_id: self.chain_id,
                hashes: required_field(self.hashes, "hashes")?,
            }),
            NetworkMessageTag::GetHeaders => Ok(NetworkMessage::GetHeaders {
                chain_id: self.chain_id,
                locator: required_field(self.locator, "locator")?,
                stop_hash: self.stop_hash,
                limit: required_field(self.limit, "limit")?,
            }),
            NetworkMessageTag::Headers => Ok(NetworkMessage::Headers {
                chain_id: self.chain_id,
                headers: required_field(self.headers, "headers")?,
            }),
            NetworkMessageTag::GetTips => Ok(NetworkMessage::GetTips {
                chain_id: self.chain_id,
                inventory: self.inventory,
            }),
            NetworkMessageTag::Tips => Ok(NetworkMessage::Tips {
                chain_id: self.chain_id,
                tips: required_field(self.tips, "tips")?,
                inventory: self.inventory,
            }),
            NetworkMessageTag::GetBlockHeaders => {
                let hashes = required_field(self.hashes, "hashes")?;
                enforce_request_fanout::<E>(hashes.len(), "GetBlockHeaders.hashes")?;
                Ok(NetworkMessage::GetBlockHeaders {
                    chain_id: self.chain_id,
                    hashes,
                })
            }
            NetworkMessageTag::BlockHeaders => {
                let headers = required_field(self.headers, "headers")?;
                enforce_request_fanout::<E>(headers.len(), "BlockHeaders.headers")?;
                Ok(NetworkMessage::BlockHeaders {
                    chain_id: self.chain_id,
                    headers: headers
                        .into_iter()
                        .map(|entry| BlockHeaderAnnouncement {
                            hash: entry.hash,
                            header: entry.header,
                        })
                        .collect(),
                })
            }
            NetworkMessageTag::GetBlock => Ok(NetworkMessage::GetBlock {
                chain_id: self.chain_id,
                hash: required_field(self.hash, "hash")?,
                request_id: self.request_id,
                requesting_peer_id: self.requesting_peer_id,
                requested_peer_id: self.requested_peer_id,
                request_kind: self.request_kind,
            }),
            NetworkMessageTag::BlockData => Ok(NetworkMessage::BlockData {
                chain_id: self.chain_id,
                block: self.block,
                request_id: self.request_id,
                request_hash: self.request_hash,
            }),
            NetworkMessageTag::Block => Ok(NetworkMessage::Block {
                chain_id: self.chain_id,
                block: required_field(self.block, "block")?,
            }),
            NetworkMessageTag::Reject => Ok(NetworkMessage::Reject {
                chain_id: self.chain_id,
                reason: required_field(self.reason, "reason")?,
            }),
            NetworkMessageTag::Error => Ok(NetworkMessage::Error {
                chain_id: self.chain_id,
                message: required_field(self.message, "message")?,
            }),
        }
    }
}

impl<'de> Deserialize<'de> for NetworkMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        NetworkMessageWire::deserialize(deserializer)?.into_message::<D::Error>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::P2P_WIRE_MAX_INVENTORY_ITEMS_V1;

    #[test]
    fn field_order_and_unknown_extensions_preserve_legacy_decode() {
        let wire = br#"{"chain_id":"testnet","pulsedag_protocol_capabilities_v1":{"ignored":true},"hashes":["a"],"type":"InvBlock"}"#;
        let decoded = serde_json::from_slice::<NetworkMessage>(wire).expect("decode legacy wire");
        match decoded {
            NetworkMessage::InvBlock { chain_id, hashes } => {
                assert_eq!(chain_id, "testnet");
                assert_eq!(hashes, vec!["a".to_string()]);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn oversized_vector_is_rejected_even_when_it_precedes_type() {
        let hashes = serde_json::to_string(&vec!["a"; P2P_WIRE_MAX_INVENTORY_ITEMS_V1 + 1])
            .expect("serialize hashes");
        let wire = format!(r#"{{"hashes":{hashes},"chain_id":"testnet","type":"InvBlock"}}"#);
        let error = serde_json::from_str::<NetworkMessage>(&wire).unwrap_err();
        assert!(error.to_string().contains("network inventory hashes"));
        assert!(error
            .to_string()
            .contains(&P2P_WIRE_MAX_INVENTORY_ITEMS_V1.to_string()));
    }
}
