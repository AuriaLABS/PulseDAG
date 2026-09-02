use std::fmt;
use std::marker::PhantomData;

use pulsedag_core::types::Hash;
use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};

use super::dag_sync_v2::MAX_SELECTED_CHAIN_LOCATOR_HASHES;
use super::{BlockHeaderAnnouncement, HeaderInventory};
use crate::{MAX_INV_BLOCK_HASHES, MAX_INV_BLOCK_REQUEST_FANOUT};

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

pub(super) fn deserialize_inventory_hashes<'de, D>(deserializer: D) -> Result<Vec<Hash>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, MAX_INV_BLOCK_HASHES, "network inventory hashes")
}

pub(super) fn deserialize_locator_hashes<'de, D>(deserializer: D) -> Result<Vec<Hash>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(
        deserializer,
        MAX_SELECTED_CHAIN_LOCATOR_HASHES,
        "GetHeaders.locator",
    )
}

pub(super) fn deserialize_header_inventory<'de, D>(
    deserializer: D,
) -> Result<Vec<HeaderInventory>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(deserializer, MAX_INV_BLOCK_HASHES, "Headers.headers")
}

pub(super) fn deserialize_request_hashes<'de, D>(deserializer: D) -> Result<Vec<Hash>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(
        deserializer,
        MAX_INV_BLOCK_REQUEST_FANOUT,
        "GetBlockHeaders.hashes",
    )
}

pub(super) fn deserialize_block_header_announcements<'de, D>(
    deserializer: D,
) -> Result<Vec<BlockHeaderAnnouncement>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_bounded_vec(
        deserializer,
        MAX_INV_BLOCK_REQUEST_FANOUT,
        "BlockHeaders.headers",
    )
}

pub(super) fn deserialize_response_limit<'de, D>(deserializer: D) -> Result<usize, D::Error>
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
