//! Dormant versioned native-v3 header-only restore boundary.
//!
//! This module does NOT write/read RocksDB, decode transactions, restore a DAG
//! or allow mining/P2P/startup. It is NOT a full persisted block format.
//! It proves only: an independently trusted network genesis identity,
//! strict JSON shape, native nanosecond header hash and non-genesis KHeavyHash.
//! Parent existence, selected-parent/blue-work authority, height progression,
//! retarget, UTXO state roots, money, future drift and contracts-disabled
//! policy must be checked by a separate authoritative v3 consensus pipeline.

use pulsedag_core::{
    errors::PulseError,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    pow_v3::verify_v3_header_pow,
};
use serde::{Deserialize, Serialize};

pub const NATIVE_V3_HEADER_RESTORE_RECORD_VERSION: u32 = 2;
pub const MAX_NATIVE_V3_HEADER_RESTORE_RECORD_BYTES: usize = 32_768;

/// A verified, non-genesis header record; generic Deserialize is intentionally
/// unavailable so callers cannot bypass the independent freeze/PoW check.
///
/// ```compile_fail
/// use pulsedag_storage::NativeV3HeaderRestoreRecordV2;
/// let _: NativeV3HeaderRestoreRecordV2 = serde_json::from_slice(b"{}").unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct NativeV3HeaderRestoreRecordV2 {
    schema_version: u32,
    chain_id: String,
    frozen_genesis_hash: String,
    block_hash: String,
    header: BlockHeaderV3,
}

// Pin schema v2 independent of the evolving core BlockHeaderV3 shape.
// A future new core header field forces an explicit compile-time conversion
// update rather than silently changing the serialized restore bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageHeaderV3V2 {
    version: u32,
    parents: Vec<String>,
    timestamp_ns: u64,
    difficulty: u32,
    nonce: u64,
    merkle_root: String,
    state_root: String,
    blue_score: u64,
    height: u64,
}

impl From<&BlockHeaderV3> for StorageHeaderV3V2 {
    fn from(header: &BlockHeaderV3) -> Self {
        Self {
            version: header.version,
            parents: header.parents.clone(),
            timestamp_ns: header.timestamp_ns,
            difficulty: header.difficulty,
            nonce: header.nonce,
            merkle_root: header.merkle_root.clone(),
            state_root: header.state_root.clone(),
            blue_score: header.blue_score,
            height: header.height,
        }
    }
}

impl From<StorageHeaderV3V2> for BlockHeaderV3 {
    fn from(header: StorageHeaderV3V2) -> Self {
        Self {
            version: header.version,
            parents: header.parents,
            timestamp_ns: header.timestamp_ns,
            difficulty: header.difficulty,
            nonce: header.nonce,
            merkle_root: header.merkle_root,
            state_root: header.state_root,
            blue_score: header.blue_score,
            height: header.height,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictNativeV3HeaderRestoreRecordV2 {
    schema_version: u32,
    chain_id: String,
    frozen_genesis_hash: String,
    block_hash: String,
    header: StorageHeaderV3V2,
}

fn invalid_record(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(format!(
        "native-v3 header restore record: {}",
        message.into()
    ))
}

fn validate_record_size(bytes: &[u8]) -> Result<(), PulseError> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_V3_HEADER_RESTORE_RECORD_BYTES {
        return Err(invalid_record(format!(
            "invalid record length {}; expected 1..={MAX_NATIVE_V3_HEADER_RESTORE_RECORD_BYTES}",
            bytes.len()
        )));
    }
    Ok(())
}

impl NativeV3HeaderRestoreRecordV2 {
    /// A header can be observed after validation without granting access to
    /// the old seconds-based `types::Block` or production storage.
    pub fn header(&self) -> &BlockHeaderV3 {
        &self.header
    }

    pub fn block_hash(&self) -> &str {
        &self.block_hash
    }

    /// Store only a non-genesis header that proves its own canonical v3 PoW.
    /// The caller supplies the frozen genesis identity from a trusted source,
    /// never from this record or another unverified persisted field.
    pub fn from_verified_header(
        chain_id: &str,
        header: BlockHeaderV3,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, PulseError> {
        let block_hash = compute_block_hash_v3(&header, chain_id)?;
        let record = Self {
            schema_version: NATIVE_V3_HEADER_RESTORE_RECORD_VERSION,
            chain_id: chain_id.to_owned(),
            frozen_genesis_hash: frozen_genesis_hash.to_owned(),
            block_hash,
            header,
        };
        record.verify(chain_id, frozen_genesis_timestamp_ns, frozen_genesis_hash)?;
        Ok(record)
    }

    pub fn verify(
        &self,
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<(), PulseError> {
        if self.schema_version != NATIVE_V3_HEADER_RESTORE_RECORD_VERSION {
            return Err(invalid_record("unsupported native-v3 header record schema"));
        }
        if self.chain_id != trusted_chain_id {
            return Err(invalid_record(
                "native-v3 header record chain identity mismatch",
            ));
        }
        // Unlike dormant schema v1, the bytes themselves now commit to the
        // independently frozen genesis. Reconstructing two caller arguments
        // against each other without this comparison was not sufficient.
        if self.frozen_genesis_hash != frozen_genesis_hash {
            return Err(invalid_record(
                "native-v3 header record persisted genesis commitment mismatch",
            ));
        }
        let genesis =
            dormant_native_genesis_envelope_v3_ns(trusted_chain_id, frozen_genesis_timestamp_ns)
                .map_err(|error| invalid_record(format!("invalid frozen genesis: {error}")))?;
        if genesis.hash != frozen_genesis_hash {
            return Err(invalid_record(
                "expected genesis hash differs from independently reconstructed genesis",
            ));
        }
        // Genesis uses its own stricter record and is never mined.
        if self.header.height == 0 {
            return Err(invalid_record(
                "non-genesis header restore cannot accept an unmined genesis",
            ));
        }
        let canonical_hash = compute_block_hash_v3(&self.header, trusted_chain_id)
            .map_err(|error| invalid_record(format!("invalid v3 header: {error}")))?;
        if self.block_hash != canonical_hash {
            return Err(invalid_record("native-v3 committed header hash mismatch"));
        }
        verify_v3_header_pow(&self.header, trusted_chain_id)
            .map_err(|error| invalid_record(format!("invalid canonical v3 KHeavyHash: {error}")))?;
        Ok(())
    }

    /// Serialize a *verified header only*. No storage key or network acceptance
    /// is created, and no parent/state-root authority is inferred.
    pub fn encode_verified(
        &self,
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Vec<u8>, PulseError> {
        self.verify(
            trusted_chain_id,
            frozen_genesis_timestamp_ns,
            frozen_genesis_hash,
        )?;
        let strict = StrictNativeV3HeaderRestoreRecordV2 {
            schema_version: self.schema_version,
            chain_id: self.chain_id.clone(),
            frozen_genesis_hash: self.frozen_genesis_hash.clone(),
            block_hash: self.block_hash.clone(),
            header: StorageHeaderV3V2::from(&self.header),
        };
        let bytes = serde_json::to_vec(&strict)
            .map_err(|error| invalid_record(format!("encode failed: {error}")))?;
        validate_record_size(&bytes)?;
        Ok(bytes)
    }

    /// Decode only this strict JSON schema. No legacy bincode/v2 fallbacks.
    pub fn decode_verified(
        bytes: &[u8],
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, PulseError> {
        validate_record_size(bytes)?;
        let decoded: StrictNativeV3HeaderRestoreRecordV2 = serde_json::from_slice(bytes)
            .map_err(|error| invalid_record(format!("decode failed: {error}")))?;
        if decoded.schema_version != NATIVE_V3_HEADER_RESTORE_RECORD_VERSION {
            return Err(invalid_record("unsupported native-v3 header record schema"));
        }
        let record = Self {
            schema_version: decoded.schema_version,
            chain_id: decoded.chain_id,
            frozen_genesis_hash: decoded.frozen_genesis_hash,
            block_hash: decoded.block_hash,
            header: decoded.header.into(),
        };
        record.verify(
            trusted_chain_id,
            frozen_genesis_timestamp_ns,
            frozen_genesis_hash,
        )?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        header_v3::compute_block_hash_v3, pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
    };

    const CHAIN: &str = "candidate-v3-header-restore";
    const FROZEN_NS: u64 = 1_800_000_000_500_000_000;

    fn fixture() -> (NativeV3HeaderRestoreRecordV2, String) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS).unwrap();
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash.clone()],
            timestamp_ns: FROZEN_NS + 500_000_000,
            difficulty: genesis.header.difficulty,
            nonce: 0,
            merkle_root: genesis.header.merkle_root.clone(),
            state_root: genesis.header.state_root.clone(),
            blue_score: 1,
            height: 1,
        };
        let mut valid = false;
        for nonce in 0..4096 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                valid = true;
                break;
            }
        }
        assert!(valid, "fixture must mine canonical non-genesis v3 header");
        let record = NativeV3HeaderRestoreRecordV2::from_verified_header(
            CHAIN,
            header,
            FROZEN_NS,
            &genesis.hash,
        )
        .unwrap();
        (record, genesis.hash)
    }

    #[test]
    fn strict_header_record_roundtrip_preserves_nanosecond_hash_and_pow() {
        let (record, frozen_hash) = fixture();
        let bytes = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["schema_version"], serde_json::json!(2));
        assert_eq!(body["frozen_genesis_hash"], serde_json::json!(frozen_hash));
        let decoded =
            NativeV3HeaderRestoreRecordV2::decode_verified(&bytes, CHAIN, FROZEN_NS, &frozen_hash)
                .unwrap();
        assert_eq!(decoded.block_hash(), record.block_hash());
        assert_eq!(decoded.header(), record.header());
        assert_eq!(decoded.header().timestamp_ns, FROZEN_NS + 500_000_000);
    }

    #[test]
    fn wrong_network_freeze_schema_and_rehashed_invalid_pow_reject() {
        let (record, frozen_hash) = fixture();
        let raw = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        for (chain, timestamp, hash) in [
            ("different-network", FROZEN_NS, frozen_hash.as_str()),
            (CHAIN, FROZEN_NS + 1, frozen_hash.as_str()),
            (CHAIN, FROZEN_NS, "00"),
        ] {
            assert!(
                NativeV3HeaderRestoreRecordV2::decode_verified(&raw, chain, timestamp, hash,)
                    .is_err()
            );
        }
        let mut wrong_schema: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        wrong_schema["schema_version"] = serde_json::json!(1);
        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &serde_json::to_vec(&wrong_schema).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let mut invalid = record.clone();
        for nonce in (invalid.header.nonce + 1)..(invalid.header.nonce + 4096) {
            invalid.header.nonce = nonce;
            if !evaluate_v3_header_pow(&invalid.header, CHAIN)
                .unwrap()
                .pow
                .accepted
            {
                invalid.block_hash = compute_block_hash_v3(&invalid.header, CHAIN).unwrap();
                break;
            }
        }
        assert!(invalid.verify(CHAIN, FROZEN_NS, &frozen_hash).is_err());
    }

    #[test]
    fn v1_without_genesis_commitment_and_cross_genesis_replay_reject() {
        let (record, frozen_hash) = fixture();
        let bytes = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();

        // A record valid under A cannot be reinterpreted under an unrelated,
        // correctly reconstructed network freeze B, even with the same chain.
        let different = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS + 1).unwrap();
        let error = NativeV3HeaderRestoreRecordV2::decode_verified(
            &bytes,
            CHAIN,
            FROZEN_NS + 1,
            &different.hash,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("persisted genesis commitment mismatch"));

        // Schema-v1 bytes have no genesis commitment and are never silently
        // accepted as secure v2 records.
        let mut old: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        old["schema_version"] = serde_json::json!(1);
        old.as_object_mut().unwrap().remove("frozen_genesis_hash");
        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &serde_json::to_vec(&old).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
    }

    #[test]
    fn legacy_unknown_fields_mutated_hash_and_genesis_fail_closed() {
        let (record, frozen_hash) = fixture();
        let encoded = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        let canonical: serde_json::Value = serde_json::from_slice(&encoded).unwrap();

        let mut wrong = canonical.clone();
        wrong["header"]["timestamp"] = serde_json::json!((FROZEN_NS + 500_000_000) / 1_000_000_000);
        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &serde_json::to_vec(&wrong).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let mut unknown = canonical.clone();
        unknown["future_record_field"] = serde_json::json!(true);
        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &serde_json::to_vec(&unknown).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let mut tampered = canonical.clone();
        tampered["block_hash"] = serde_json::json!("aa".repeat(32));
        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &serde_json::to_vec(&tampered).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let mut no_pow_genesis = canonical;
        no_pow_genesis["header"]["height"] = serde_json::json!(0);
        no_pow_genesis["header"]["parents"] = serde_json::json!([]);
        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &serde_json::to_vec(&no_pow_genesis).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        assert!(NativeV3HeaderRestoreRecordV2::decode_verified(
            &vec![b' '; MAX_NATIVE_V3_HEADER_RESTORE_RECORD_BYTES + 1],
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
    }
}
