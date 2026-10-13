//! Dormant, typed native-v3 genesis restore-record boundary.
//!
//! The production storage schema still persists a seconds-based v1/v2 Block.
//! This does NOT install a RocksDB key, read existing databases, fall back to
//! legacy bincode, or authorize v3 startup. It defines a bounded, versioned,
//! separately verified byte boundary for a future native-v3 restore path.
//!
//! Trusted freeze parameters MUST come from externally frozen chain/network
//! configuration, never the untrusted persisted record itself.

use pulsedag_core::{
    block_envelope_v3::BlockEnvelopeV3, errors::PulseError, header_v3::BlockHeaderV3,
    types::Transaction, verify_dormant_native_genesis_freeze_v3_ns,
};
use serde::{Deserialize, Serialize};

pub const NATIVE_V3_GENESIS_RESTORE_RECORD_VERSION: u32 = 1;
pub const MAX_NATIVE_V3_GENESIS_RESTORE_RECORD_BYTES: usize = 4096;

/// Restored v3 genesis record with an authenticated identity.
///
/// Deliberately does NOT implement `Deserialize`. Generic Serde decoding of
/// `BlockEnvelopeV3` would ignore unknown nested header fields, silently
/// bypassing the strict versioned `decode_verified` schema. Callers must use
/// `decode_verified` with independently trusted freeze parameters instead.
#[derive(Debug, Clone, Serialize)]
pub struct NativeV3GenesisRestoreRecordV1 {
    schema_version: u32,
    genesis: BlockEnvelopeV3,
}

// Storage v3 schema must stay strict at *every* JSON nesting level.
/// BlockHeaderV3 intentionally has a general-purpose deserializer, which
/// ignores unknown members. Never use it directly to decode a persisted
/// versioned restore record: a mixed `timestamp` + `timestamp_ns` header
/// could otherwise pass validation while silently losing the old field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictRestoreHeaderV3 {
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

impl From<StrictRestoreHeaderV3> for BlockHeaderV3 {
    fn from(header: StrictRestoreHeaderV3) -> Self {
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictRestoreEnvelopeV3 {
    chain_id: String,
    hash: String,
    header: StrictRestoreHeaderV3,
    transactions: Vec<Transaction>,
}

impl From<StrictRestoreEnvelopeV3> for BlockEnvelopeV3 {
    fn from(envelope: StrictRestoreEnvelopeV3) -> Self {
        Self {
            chain_id: envelope.chain_id,
            hash: envelope.hash,
            header: envelope.header.into(),
            transactions: envelope.transactions,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictRestoreRecordV1 {
    schema_version: u32,
    genesis: StrictRestoreEnvelopeV3,
}

fn invalid_record(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(format!(
        "native-v3 genesis restore record: {}",
        message.into()
    ))
}

impl NativeV3GenesisRestoreRecordV1 {
    /// Access only a previously verified genesis; no mutable access is exposed.
    pub fn genesis(&self) -> &BlockEnvelopeV3 {
        &self.genesis
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Construct only from a v3 envelope matching an independent network freeze.
    pub fn from_verified_genesis(
        genesis: BlockEnvelopeV3,
        frozen_chain_id: &str,
        frozen_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, PulseError> {
        verify_dormant_native_genesis_freeze_v3_ns(
            &genesis,
            frozen_chain_id,
            frozen_timestamp_ns,
            frozen_genesis_hash,
        )?;
        Ok(Self {
            schema_version: NATIVE_V3_GENESIS_RESTORE_RECORD_VERSION,
            genesis,
        })
    }

    /// Serialize an already-created record after verifying its identity again.
    ///
    /// No database write is performed. This is a separate schema from the
    /// current bincode ChainState and legacy persisted Block record.
    pub fn encode_verified(
        &self,
        frozen_chain_id: &str,
        frozen_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Vec<u8>, PulseError> {
        self.verify(frozen_chain_id, frozen_timestamp_ns, frozen_genesis_hash)?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| invalid_record(format!("encode failed: {error}")))?;
        check_record_size(&bytes)?;
        Ok(bytes)
    }

    /// Decode only this JSON envelope; malformed and legacy records fail closed.
    ///
    /// The externally frozen identity is checked BEFORE any decoded genesis
    /// could be handed to a future v3 replay or storage restoration path.
    pub fn decode_verified(
        bytes: &[u8],
        frozen_chain_id: &str,
        frozen_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, PulseError> {
        check_record_size(bytes)?;
        let decoded: StrictRestoreRecordV1 = serde_json::from_slice(bytes)
            .map_err(|error| invalid_record(format!("decode failed: {error}")))?;
        let record = Self {
            schema_version: decoded.schema_version,
            genesis: decoded.genesis.into(),
        };
        record.verify(frozen_chain_id, frozen_timestamp_ns, frozen_genesis_hash)?;
        Ok(record)
    }

    pub fn verify(
        &self,
        frozen_chain_id: &str,
        frozen_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<(), PulseError> {
        if self.schema_version != NATIVE_V3_GENESIS_RESTORE_RECORD_VERSION {
            return Err(invalid_record(format!(
                "unsupported schema version {}, expected {}",
                self.schema_version, NATIVE_V3_GENESIS_RESTORE_RECORD_VERSION
            )));
        }
        verify_dormant_native_genesis_freeze_v3_ns(
            &self.genesis,
            frozen_chain_id,
            frozen_timestamp_ns,
            frozen_genesis_hash,
        )
        .map_err(|error| invalid_record(format!("frozen identity mismatch: {error}")))
    }
}

fn check_record_size(bytes: &[u8]) -> Result<(), PulseError> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_V3_GENESIS_RESTORE_RECORD_BYTES {
        return Err(invalid_record(format!(
            "invalid record size {}, permitted 1..={MAX_NATIVE_V3_GENESIS_RESTORE_RECORD_BYTES}",
            bytes.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        dormant_native_genesis_envelope_v3_ns, genesis_v3::genesis_block_v3,
        header_v3::compute_block_hash_v3,
    };

    const CHAIN: &str = "native-v3-genesis-storage-restore-candidate";
    const FROZEN_NS: u64 = 1_800_000_000_500_000_000;

    fn record() -> NativeV3GenesisRestoreRecordV1 {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS).unwrap();
        let frozen_hash = genesis.hash.clone();
        NativeV3GenesisRestoreRecordV1::from_verified_genesis(
            genesis,
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .unwrap()
    }

    #[test]
    fn roundtrip_is_bounded_and_external_freeze_authenticated() {
        let initial = record();
        let frozen_hash = initial.genesis.hash.clone();
        let bytes = initial
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        assert!(bytes.len() < MAX_NATIVE_V3_GENESIS_RESTORE_RECORD_BYTES);
        let restored =
            NativeV3GenesisRestoreRecordV1::decode_verified(&bytes, CHAIN, FROZEN_NS, &frozen_hash)
                .unwrap();
        assert_eq!(restored.genesis.hash, frozen_hash);
        assert_eq!(restored.genesis.header.timestamp_ns, FROZEN_NS);
        assert!(restored.genesis.transactions.is_empty());
        assert_eq!(restored.schema_version(), 1);
        assert_eq!(restored.genesis().hash, frozen_hash);
    }

    #[test]
    fn wrong_chain_timestamp_hash_and_schema_are_rejected() {
        let initial = record();
        let frozen_hash = initial.genesis.hash.clone();
        let bytes = initial
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &bytes,
            "other-network",
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
        // One nanosecond apart: the v1/v2 whole-second timestamp cannot
        // express this difference, but native v3 MUST reject it.
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &bytes,
            CHAIN,
            FROZEN_NS + 1,
            &frozen_hash,
        )
        .is_err());
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &bytes,
            CHAIN,
            FROZEN_NS,
            &"ab".repeat(32),
        )
        .is_err());

        let mut wrong_schema = initial;
        wrong_schema.schema_version += 1;
        let raw = serde_json::to_vec(&wrong_schema).unwrap();
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &raw,
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
        assert!(wrong_schema
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .is_err());
    }

    #[test]
    fn rehashed_alternative_genesis_and_v2_records_cannot_restore() {
        let initial = record();
        let frozen_hash = initial.genesis.hash.clone();
        let mut forged = initial.clone();
        forged.genesis.header.nonce += 1;
        forged.genesis.hash = compute_block_hash_v3(&forged.genesis.header, CHAIN).unwrap();
        let raw = serde_json::to_vec(&forged).unwrap();
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &raw,
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let legacy = genesis_block_v3(CHAIN, FROZEN_NS / 1_000_000_000).unwrap();
        let legacy_json = serde_json::to_vec(&legacy).unwrap();
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &legacy_json,
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
    }

    #[test]
    fn nested_unknown_header_fields_and_mixed_second_nanosecond_schema_fail_closed() {
        let original = record();
        let hash = original.genesis.hash.clone();
        let canonical = original.encode_verified(CHAIN, FROZEN_NS, &hash).unwrap();
        let mut mixed: serde_json::Value = serde_json::from_slice(&canonical).unwrap();

        // This used to pass: generic BlockHeaderV3 deserialization silently
        // ignored a legacy whole-second timestamp alongside timestamp_ns.
        mixed["genesis"]["header"]["timestamp"] =
            serde_json::Value::Number((FROZEN_NS / 1_000_000_000).into());
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&mixed).unwrap(),
            CHAIN,
            FROZEN_NS,
            &hash,
        )
        .is_err());

        mixed["genesis"]["header"]
            .as_object_mut()
            .unwrap()
            .remove("timestamp");
        mixed["genesis"]["header"]["future_header_field"] = serde_json::Value::Bool(true);
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&mixed).unwrap(),
            CHAIN,
            FROZEN_NS,
            &hash,
        )
        .is_err());

        let mut missing: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
        missing["genesis"]["header"]
            .as_object_mut()
            .unwrap()
            .remove("timestamp_ns");
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&missing).unwrap(),
            CHAIN,
            FROZEN_NS,
            &hash,
        )
        .is_err());
    }

    #[test]
    fn corrupted_truncated_unknown_and_oversized_records_fail_closed() {
        let initial = record();
        let frozen_hash = initial.genesis.hash.clone();
        let bytes = initial
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();

        let mut trailing = bytes.clone();
        trailing.extend_from_slice(b"malicious suffix");
        let oversize = vec![b' '; MAX_NATIVE_V3_GENESIS_RESTORE_RECORD_BYTES + 1];
        for bad in [
            Vec::new(),
            bytes[..bytes.len() - 1].to_vec(),
            trailing,
            oversize,
        ] {
            assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
                &bad,
                CHAIN,
                FROZEN_NS,
                &frozen_hash,
            )
            .is_err());
        }

        let mut unexpected: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        unexpected["unexpected"] = serde_json::Value::Bool(true);
        assert!(NativeV3GenesisRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&unexpected).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
    }
}
