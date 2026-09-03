use std::fmt;
use std::marker::PhantomData;

use pulsedag_core::types::{Block, Hash, Transaction};
use serde::de::{self, DeserializeOwned, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use super::{
    BlockHeaderAnnouncement, HeaderInventory, NetworkMessage, TipInventoryStatus,
    P2P_WIRE_MAX_INVENTORY_ITEMS_V1, P2P_WIRE_MAX_REQUEST_ITEMS_V1,
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

struct BoundedRawVec<T, const MAXIMUM: usize>(Vec<T>);

struct BoundedRawVecVisitor<T, const MAXIMUM: usize> {
    marker: PhantomData<T>,
}

impl<'de, T, const MAXIMUM: usize> Visitor<'de> for BoundedRawVecVisitor<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    type Value = BoundedRawVec<T, MAXIMUM>;

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
                None => return Ok(BoundedRawVec(values)),
            }
        }

        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(format!(
                "sequence exceeds maximum item count {MAXIMUM}"
            )));
        }
        Ok(BoundedRawVec(values))
    }
}

impl<'de, T, const MAXIMUM: usize> Deserialize<'de> for BoundedRawVec<T, MAXIMUM>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(BoundedRawVecVisitor::<T, MAXIMUM> {
            marker: PhantomData,
        })
    }
}

/// Streaming wire shape used to preserve legacy field-order tolerance while
/// applying allocation bounds before a complete attacker-controlled vector is
/// materialized. The two fields shared by differently bounded variants are kept
/// as borrowed raw JSON slices; after `type` is known, only that slice is parsed
/// with the correct live budget (512 for inventory, 64 for request fanout).
/// Unknown top-level extension fields are consumed with Serde's streaming
/// `IgnoredAny` path and never materialized as a complete `Value`.
#[derive(Deserialize)]
struct NetworkMessageWire<'a> {
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
    #[serde(default, borrow)]
    hashes: Option<&'a RawValue>,
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
    #[serde(default, borrow)]
    headers: Option<&'a RawValue>,
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

fn parse_bounded_raw_vec<T, E, const MAXIMUM: usize>(
    raw: Option<&RawValue>,
    field: &'static str,
) -> Result<Vec<T>, E>
where
    T: DeserializeOwned,
    E: de::Error,
{
    let raw = required_field(raw, field)?;
    serde_json::from_str::<BoundedRawVec<T, MAXIMUM>>(raw.get())
        .map(|values| values.0)
        .map_err(|error| E::custom(format!("{field}: {error}")))
}

impl NetworkMessageWire<'_> {
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
                hashes: parse_bounded_raw_vec::<Hash, E, { P2P_WIRE_MAX_INVENTORY_ITEMS_V1 }>(
                    self.hashes,
                    "network inventory hashes",
                )?,
            }),
            NetworkMessageTag::GetHeaders => Ok(NetworkMessage::GetHeaders {
                chain_id: self.chain_id,
                locator: required_field(self.locator, "locator")?,
                stop_hash: self.stop_hash,
                limit: required_field(self.limit, "limit")?,
            }),
            NetworkMessageTag::Headers => Ok(NetworkMessage::Headers {
                chain_id: self.chain_id,
                headers: parse_bounded_raw_vec::<
                    HeaderInventory,
                    E,
                    { P2P_WIRE_MAX_INVENTORY_ITEMS_V1 },
                >(self.headers, "Headers.headers")?,
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
            NetworkMessageTag::GetBlockHeaders => Ok(NetworkMessage::GetBlockHeaders {
                chain_id: self.chain_id,
                hashes: parse_bounded_raw_vec::<Hash, E, { P2P_WIRE_MAX_REQUEST_ITEMS_V1 }>(
                    self.hashes,
                    "GetBlockHeaders.hashes",
                )?,
            }),
            NetworkMessageTag::BlockHeaders => Ok(NetworkMessage::BlockHeaders {
                chain_id: self.chain_id,
                headers: parse_bounded_raw_vec::<
                    BlockHeaderAnnouncement,
                    E,
                    { P2P_WIRE_MAX_REQUEST_ITEMS_V1 },
                >(self.headers, "BlockHeaders.headers")?,
            }),
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
    fn oversized_inventory_is_rejected_even_when_it_precedes_type() {
        let hashes = serde_json::to_string(&vec!["a"; P2P_WIRE_MAX_INVENTORY_ITEMS_V1 + 1])
            .expect("serialize hashes");
        let wire = format!(r#"{{"hashes":{hashes},"chain_id":"testnet","type":"InvBlock"}}"#);
        let error = serde_json::from_str::<NetworkMessage>(&wire).unwrap_err();
        assert!(error.to_string().contains("network inventory hashes"));
        assert!(error
            .to_string()
            .contains(&P2P_WIRE_MAX_INVENTORY_ITEMS_V1.to_string()));
    }

    #[test]
    fn inventory_budget_stays_at_512_when_type_is_last() {
        let hashes = serde_json::to_string(&vec!["a"; P2P_WIRE_MAX_INVENTORY_ITEMS_V1])
            .expect("serialize hashes");
        let wire = format!(r#"{{"hashes":{hashes},"chain_id":"testnet","type":"InvBlock"}}"#);
        assert!(serde_json::from_str::<NetworkMessage>(&wire).is_ok());
    }

    #[test]
    fn request_fanout_is_bounded_during_decode_when_type_is_last() {
        let hashes = serde_json::to_string(&vec!["a"; P2P_WIRE_MAX_REQUEST_ITEMS_V1 + 1])
            .expect("serialize hashes");
        let wire =
            format!(r#"{{"hashes":{hashes},"chain_id":"testnet","type":"GetBlockHeaders"}}"#);
        let error = serde_json::from_str::<NetworkMessage>(&wire).unwrap_err();
        assert!(error.to_string().contains("GetBlockHeaders.hashes"));
        assert!(error
            .to_string()
            .contains(&P2P_WIRE_MAX_REQUEST_ITEMS_V1.to_string()));
    }

    #[test]
    fn block_header_response_fanout_is_bounded_during_decode_when_type_is_last() {
        let header = serde_json::json!({
            "version": 1,
            "parents": ["parent"],
            "timestamp": 1,
            "difficulty": 1,
            "nonce": 1,
            "merkle_root": "merkle",
            "state_root": "state",
            "blue_score": 1,
            "height": 1
        });
        let announcements = (0..P2P_WIRE_MAX_REQUEST_ITEMS_V1 + 1)
            .map(|index| serde_json::json!({"hash": format!("header-{index}"), "header": header.clone()}))
            .collect::<Vec<_>>();
        let headers = serde_json::to_string(&announcements).expect("serialize headers");
        let wire = format!(r#"{{"headers":{headers},"chain_id":"testnet","type":"BlockHeaders"}}"#);
        let error = serde_json::from_str::<NetworkMessage>(&wire).unwrap_err();
        assert!(error.to_string().contains("BlockHeaders.headers"));
        assert!(error
            .to_string()
            .contains(&P2P_WIRE_MAX_REQUEST_ITEMS_V1.to_string()));
    }
}
