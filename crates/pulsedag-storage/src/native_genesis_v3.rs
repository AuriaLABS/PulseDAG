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
    block_envelope_v3::BlockEnvelopeV3, errors::PulseError,
    verify_dormant_native_genesis_freeze_v3_ns,
};
use serde::{Deserialize, Serialize};

pub const NATIVE_V3_GENESIS_RESTORE_RECORD_VERSION: u32 = 1;
pub const MAX_NATIVE_V3_GENESIS_RESTORE_RECORD_BYTES: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeV3GenesisRestoreRecordV1 {
    pub schema_version: u32,
    pub genesis: BlockEnvelopeV3,
}

fn invalid_record(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(format!(
        "native-v3 genesis restore record: {}",
        message.into()
    ))
}

impl NativeV3GenesisRestoreRecordV1 {
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
        let record: Self = serde_json::from_slice(bytes)
            .map_err(|error| invalid_record(format!("decode failed: {error}")))?;
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
        assert_eq!(restored.schema_version, 1);
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
