use std::fmt;
use std::marker::PhantomData;

use pulsedag_core::types::Hash;
use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::dag_sync_v2::MAX_SELECTED_CHAIN_LOCATOR_HASHES;
use super::HeaderInventory;
use crate::{MAX_INV_BLOCK_HASHES, MAX_INV_BLOCK_REQUEST_FANOUT};

/// Public wire view of the live inventory budget. The value is deliberately
/// sourced from the runtime constant so decoder/runtime limits cannot drift.
pub const P2P_WIRE_MAX_INVENTORY_ITEMS_V1: usize = MAX_INV_BLOCK_HASHES;
/// Public wire view of the live request-fanout budget.
pub const P2P_WIRE_MAX_REQUEST_ITEMS_V1: usize = MAX_INV_BLOCK_REQUEST_FANOUT;

struct BoundedVecVisitor<T> {
    maximum: usize,
    field: &'static str,
    marker: PhantomData<T>,
}

impl<T> BoundedVecVisitor<T> {
    fn new(maximum: usize, field: &'static str) -> Self {
        Self {
            maximum,
            field,
            marker: PhantomData,
        }
    }
}

impl<'de, T> Visitor<'de> for BoundedVecVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} with at most {} items",
            self.field, self.maximum
        )
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let capacity = sequence.size_hint().unwrap_or(0).min(self.maximum);
        let mut values = Vec::with_capacity(capacity);

        while values.len() < self.maximum {
            match sequence.next_element::<T>()? {
                Some(value) => values.push(value),
                None => return Ok(values),
            }
        }

        if sequence.next_element::<IgnoredAny>()?.is_some() {
            return Err(A::Error::custom(format!(
                "{} exceeds maximum item count {}",
                self.field, self.maximum
            )));
        }

        Ok(values)
    }
}

fn deserialize_bounded_vec<'de, D, T>(
    deserializer: D,
    maximum: usize,
    field: &'static str,
) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::<T>::new(maximum, field))
}

fn deserialize_inventory_hashes<'de, D>(deserializer: D) -> Result<Vec<Hash>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(
        deserializer,
        MAX_INV_BLOCK_HASHES,
        "network inventory hashes",
    )
}

fn deserialize_locator_hashes<'de, D>(deserializer: D) -> Result<Vec<Hash>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(
        deserializer,
        MAX_SELECTED_CHAIN_LOCATOR_HASHES,
        "GetHeaders.locator",
    )
}

fn deserialize_header_inventory<'de, D>(
    deserializer: D,
) -> Result<Vec<HeaderInventory>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, MAX_INV_BLOCK_HASHES, "Headers.headers")
}

fn deserialize_response_limit<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = usize::deserialize(deserializer)?;
    if limit > MAX_INV_BLOCK_HASHES {
        return Err(D::Error::custom(format!(
            "GetHeaders.limit exceeds maximum {MAX_INV_BLOCK_HASHES}"
        )));
    }
    Ok(limit)
}

pub(super) fn deserialize_optional_inventory_hashes<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<Hash>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_inventory_hashes(deserializer).map(Some)
}

pub(super) fn deserialize_optional_locator_hashes<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<Hash>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_locator_hashes(deserializer).map(Some)
}

pub(super) fn deserialize_optional_header_inventory<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<HeaderInventory>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_header_inventory(deserializer).map(Some)
}

pub(super) fn deserialize_optional_response_limit<'de, D>(
    deserializer: D,
) -> Result<Option<usize>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_response_limit(deserializer).map(Some)
}

/// Keep outbound `Tips` serialization byte-shape compatible with the existing
/// protocol. Resource limits in this slice are inbound admission limits; live
/// sender-side budgets remain enforced by the runtime.
pub(super) fn serialize_bounded_tips<S>(tips: &[Hash], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    tips.serialize(serializer)
}

#[cfg(test)]
mod tests {
    use pulsedag_core::types::BlockHeader;

    use super::*;
    use crate::messages::{BlockHeaderAnnouncement, NetworkMessage};

    fn header() -> BlockHeader {
        BlockHeader {
            version: 1,
            parents: vec!["parent".into()],
            timestamp: 1,
            difficulty: 1,
            nonce: 1,
            merkle_root: "merkle".into(),
            state_root: "state".into(),
            blue_score: 1,
            height: 1,
        }
    }

    fn decode(message: &NetworkMessage) -> Result<NetworkMessage, serde_json::Error> {
        let encoded = serde_json::to_vec(message).expect("wire fixture serializes");
        serde_json::from_slice(&encoded)
    }

    #[test]
    fn exact_wire_boundaries_are_accepted() {
        assert!(decode(&NetworkMessage::GetHeaders {
            chain_id: "testnet".into(),
            locator: vec!["hash".into(); MAX_SELECTED_CHAIN_LOCATOR_HASHES],
            stop_hash: None,
            limit: MAX_INV_BLOCK_HASHES,
        })
        .is_ok());

        assert!(decode(&NetworkMessage::GetBlockHeaders {
            chain_id: "testnet".into(),
            hashes: vec!["hash".into(); MAX_INV_BLOCK_REQUEST_FANOUT],
        })
        .is_ok());

        let header_inventory = HeaderInventory {
            hash: "header".into(),
            header: header(),
        };
        assert!(decode(&NetworkMessage::Headers {
            chain_id: "testnet".into(),
            headers: vec![header_inventory; MAX_INV_BLOCK_HASHES],
        })
        .is_ok());

        let announcement = BlockHeaderAnnouncement {
            hash: "header".into(),
            header: header(),
        };
        assert!(decode(&NetworkMessage::BlockHeaders {
            chain_id: "testnet".into(),
            headers: vec![announcement; MAX_INV_BLOCK_REQUEST_FANOUT],
        })
        .is_ok());
    }

    #[test]
    fn oversized_response_vectors_are_rejected_during_decode() {
        let header_inventory = HeaderInventory {
            hash: "header".into(),
            header: header(),
        };
        let error = decode(&NetworkMessage::Headers {
            chain_id: "testnet".into(),
            headers: vec![header_inventory; MAX_INV_BLOCK_HASHES + 1],
        })
        .unwrap_err();
        assert!(error.to_string().contains("Headers.headers"));

        let announcement = BlockHeaderAnnouncement {
            hash: "header".into(),
            header: header(),
        };
        let error = decode(&NetworkMessage::BlockHeaders {
            chain_id: "testnet".into(),
            headers: vec![announcement; MAX_INV_BLOCK_REQUEST_FANOUT + 1],
        })
        .unwrap_err();
        assert!(error.to_string().contains("BlockHeaders.headers"));
    }

    #[test]
    fn outbound_tips_serialization_preserves_order_and_duplicates() {
        let tips = vec!["tip-z".to_string(), "tip-a".to_string(), "tip-a".to_string()];
        let value = serde_json::to_value(NetworkMessage::Tips {
            chain_id: "testnet".into(),
            tips: tips.clone(),
            inventory: None,
        })
        .expect("tips serialize");
        let observed = value["tips"]
            .as_array()
            .expect("tips remain an array")
            .iter()
            .map(|value| value.as_str().expect("tip string").to_string())
            .collect::<Vec<_>>();
        assert_eq!(observed, tips);
    }
}
