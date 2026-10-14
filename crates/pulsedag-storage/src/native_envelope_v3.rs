//! Dormant, bounded, non-genesis native-v3 transaction-bearing restore frame.
//!
//! No production storage key, RocksDB reader/writer, live P2P/RPC route,
//! monetary settlement, or block admission uses this module. Verifying a
//! decoded frame proves only its strict schema, externally frozen genesis
//! identity, chain-bound v3 header and v2 tx commitments, and header KHeavyHash.
//! It does NOT authenticate ancestors, blue-work/selected-parent, height,
//! retarget, future time, transaction signatures, UTXOs, state-root validity,
//! contract execution or canonical monetary ordering.

use pulsedag_core::{
    block_envelope_v3::{validate_block_envelope_v3, BlockEnvelopeV3},
    errors::PulseError,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    pow_v3::verify_v3_header_pow,
    types::{OutPoint, Transaction, TxInput, TxOutput},
};
use serde::{Deserialize, Serialize};

pub const NATIVE_V3_ENVELOPE_RESTORE_RECORD_VERSION: u32 = 1;
/// Decode allocation bound only; NOT a v3 consensus maximum block size.
pub const MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES: usize = 2_097_152;

/// Construct only through verified entrypoints: generic Deserialize is
/// intentionally unavailable for this validated wrapper.
#[derive(Debug, Clone, Serialize)]
pub struct NativeV3EnvelopeRestoreRecordV1 {
    schema_version: u32,
    envelope: BlockEnvelopeV3,
}

// Unlike legacy types::Transaction, the untrusted JSON boundary rejects
// unknown fields at EVERY nested transaction/input/output/outpoint level.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictOutPoint {
    txid: String,
    index: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictInput {
    previous_output: StrictOutPoint,
    public_key: String,
    signature: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictOutput {
    address: String,
    amount: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictTransaction {
    txid: String,
    version: u32,
    inputs: Vec<StrictInput>,
    outputs: Vec<StrictOutput>,
    fee: u64,
    nonce: u64,
}

impl From<StrictTransaction> for Transaction {
    fn from(tx: StrictTransaction) -> Self {
        Self {
            txid: tx.txid,
            version: tx.version,
            inputs: tx
                .inputs
                .into_iter()
                .map(|input| TxInput {
                    previous_output: OutPoint {
                        txid: input.previous_output.txid,
                        index: input.previous_output.index,
                    },
                    public_key: input.public_key,
                    signature: input.signature,
                })
                .collect(),
            outputs: tx
                .outputs
                .into_iter()
                .map(|output| TxOutput {
                    address: output.address,
                    amount: output.amount,
                })
                .collect(),
            fee: tx.fee,
            nonce: tx.nonce,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictEnvelope {
    chain_id: String,
    hash: String,
    header: BlockHeaderV3,
    transactions: Vec<StrictTransaction>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictRecord {
    schema_version: u32,
    envelope: StrictEnvelope,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(format!(
        "native-v3 envelope restore record: {}",
        message.into()
    ))
}

fn check_size(bytes: &[u8]) -> Result<(), PulseError> {
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES {
        return Err(invalid(format!(
            "record length {} outside 1..={MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES}",
            bytes.len()
        )));
    }
    Ok(())
}

impl NativeV3EnvelopeRestoreRecordV1 {
    pub fn envelope(&self) -> &BlockEnvelopeV3 {
        &self.envelope
    }

    /// This is a verified frame, NOT a canonical accepted block. The frozen
    /// genesis timestamp and hash MUST come from a trusted external source.
    pub fn from_verified_envelope(
        envelope: BlockEnvelopeV3,
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, PulseError> {
        let record = Self {
            schema_version: NATIVE_V3_ENVELOPE_RESTORE_RECORD_VERSION,
            envelope,
        };
        record.verify(
            trusted_chain_id,
            frozen_genesis_timestamp_ns,
            frozen_genesis_hash,
        )?;
        Ok(record)
    }

    pub fn verify(
        &self,
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<(), PulseError> {
        if self.schema_version != NATIVE_V3_ENVELOPE_RESTORE_RECORD_VERSION {
            return Err(invalid("unsupported native-v3 envelope schema version"));
        }
        let frozen =
            dormant_native_genesis_envelope_v3_ns(trusted_chain_id, frozen_genesis_timestamp_ns)
                .map_err(|error| invalid(format!("invalid frozen genesis: {error}")))?;
        if frozen.hash != frozen_genesis_hash {
            return Err(invalid(
                "expected genesis hash differs from independently reconstructed genesis",
            ));
        }
        if self.envelope.header.height == 0 {
            return Err(invalid(
                "genesis must be restored by its distinct frozen, unmined record",
            ));
        }
        if self.envelope.header.state_root == "00".repeat(32) {
            return Err(invalid(
                "unsealed all-zero state-root candidate cannot enter restore frame",
            ));
        }
        validate_block_envelope_v3(&self.envelope, trusted_chain_id)
            .map_err(|error| invalid(format!("invalid v3 envelope or tx commitment: {error}")))?;
        verify_v3_header_pow(&self.envelope.header, trusted_chain_id)
            .map_err(|error| invalid(format!("invalid canonical v3 KHeavyHash: {error}")))?;
        Ok(())
    }

    /// Independently verify and bind the header-only and transaction-bearing
    /// restore records to the same frozen genesis and canonical v3 header.
    ///
    /// This equality check is not an assertion of canonical DAG membership,
    /// authenticated blue work, state-root replay, or persisted block authority.
    pub fn verify_matching_header_record(
        &self,
        header_record: &crate::NativeV3HeaderRestoreRecordV1,
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<(), PulseError> {
        self.verify(
            trusted_chain_id,
            frozen_genesis_timestamp_ns,
            frozen_genesis_hash,
        )?;
        header_record
            .verify(
                trusted_chain_id,
                frozen_genesis_timestamp_ns,
                frozen_genesis_hash,
            )
            .map_err(|error| invalid(format!("companion v3 header record: {error}")))?;
        if header_record.block_hash() != self.envelope.hash.as_str()
            || header_record.header() != &self.envelope.header
        {
            return Err(invalid(
                "companion v3 header record does not match full envelope",
            ));
        }
        Ok(())
    }

    /// Encode a validated envelope without writing to the active storage DB.
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
        let encoded =
            serde_json::to_vec(self).map_err(|error| invalid(format!("encode failed: {error}")))?;
        check_size(&encoded)?;
        Ok(encoded)
    }

    /// Accept strict JSON only, never legacy v2 or an opaque bincode fallback.
    pub fn decode_verified(
        bytes: &[u8],
        trusted_chain_id: &str,
        frozen_genesis_timestamp_ns: u64,
        frozen_genesis_hash: &str,
    ) -> Result<Self, PulseError> {
        check_size(bytes)?;
        let strict: StrictRecord = serde_json::from_slice(bytes)
            .map_err(|error| invalid(format!("strict JSON decode failed: {error}")))?;
        // Reject unsupported schemas BEFORE converting transactions or
        // performing expensive KHeavyHash and frozen-genesis checks.
        if strict.schema_version != NATIVE_V3_ENVELOPE_RESTORE_RECORD_VERSION {
            return Err(invalid("unsupported native-v3 envelope schema version"));
        }
        let envelope = BlockEnvelopeV3 {
            chain_id: strict.envelope.chain_id,
            hash: strict.envelope.hash,
            header: strict.envelope.header,
            transactions: strict
                .envelope
                .transactions
                .into_iter()
                .map(Transaction::from)
                .collect(),
        };
        Self::from_verified_envelope(
            envelope,
            trusted_chain_id,
            frozen_genesis_timestamp_ns,
            frozen_genesis_hash,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        block_envelope_v3::build_block_envelope_v3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        tx::{compute_txid_v2, TRANSACTION_VERSION_V2},
        types::compute_merkle_root,
    };

    const CHAIN: &str = "dormant-v3-full-envelope-restore";
    const FROZEN_NS: u64 = 1_800_000_000_500_000_000;

    fn fixture() -> (NativeV3EnvelopeRestoreRecordV1, String) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS).unwrap();
        let mut tx = Transaction {
            txid: String::new(),
            version: TRANSACTION_VERSION_V2,
            inputs: vec![],
            outputs: vec![],
            fee: 0,
            nonce: 7,
        };
        tx.txid = compute_txid_v2(&tx, CHAIN).unwrap();
        let transactions = vec![tx];
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![genesis.hash.clone()],
            timestamp_ns: FROZEN_NS + 500_000_000,
            difficulty: genesis.header.difficulty,
            nonce: 0,
            merkle_root: compute_merkle_root(&transactions),
            state_root: "33".repeat(32),
            blue_score: 1,
            height: 1,
        };
        let mut found = false;
        for nonce in 0..4096 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                found = true;
                break;
            }
        }
        assert!(found, "must find a valid low-difficulty v3 fixture");
        let envelope = build_block_envelope_v3(CHAIN, header, transactions).unwrap();
        (
            NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
                envelope,
                CHAIN,
                FROZEN_NS,
                &genesis.hash,
            )
            .unwrap(),
            genesis.hash,
        )
    }

    #[test]
    fn verified_native_ns_envelope_and_transaction_roundtrip() {
        let (record, frozen_hash) = fixture();
        let data = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        let restored =
            NativeV3EnvelopeRestoreRecordV1::decode_verified(&data, CHAIN, FROZEN_NS, &frozen_hash)
                .unwrap();
        assert_eq!(restored.envelope().hash, record.envelope().hash);
        assert_eq!(restored.envelope().transactions.len(), 1);
        assert_eq!(
            restored.envelope().header.timestamp_ns,
            FROZEN_NS + 500_000_000
        );
    }

    #[test]
    fn rejects_wrong_freeze_network_schema_genesis_and_unsealed_root() {
        let (record, frozen_hash) = fixture();
        let raw = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &raw,
            "wrong-network",
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &raw,
            CHAIN,
            FROZEN_NS + 1,
            &frozen_hash,
        )
        .is_err());
        let mut json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        json["schema_version"] = serde_json::json!(2);
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&json).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let mut unsealed = record.envelope().clone();
        unsealed.header.state_root = "00".repeat(32);
        assert!(NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
            unsealed,
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());

        let mut genesis_frame = record.envelope().clone();
        genesis_frame.header.height = 0;
        assert!(NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
            genesis_frame,
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
    }

    #[test]
    fn rejects_unknown_schema_before_rehashing_or_pow() {
        let (record, frozen_hash) = fixture();
        let raw = record.encode_verified(CHAIN, FROZEN_NS, &frozen_hash).unwrap();
        let mut forged: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        forged["schema_version"] = serde_json::json!(9);
        forged["envelope"]["hash"] = serde_json::json!("aa".repeat(32));
        let error = NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&forged).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("unsupported native-v3 envelope schema version"),
            "schema check must take priority over cryptographic work: {error}"
        );
    }

    #[test]
    fn matching_header_and_envelope_restore_records_reject_valid_but_different_pow() {
        let (record, frozen_hash) = fixture();
        let companion = crate::NativeV3HeaderRestoreRecordV1::from_verified_header(
            CHAIN,
            record.envelope().header.clone(),
            FROZEN_NS,
            &frozen_hash,
        )
        .unwrap();
        record
            .verify_matching_header_record(&companion, CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();

        // A second valid PoW header on the SAME chain is not an acceptable
        // companion if it commits to a different block hash/timestamp.
        let mut different = record.envelope().header.clone();
        different.timestamp_ns += 500_000_000;
        let mut mined = false;
        for nonce in 0..4096 {
            different.nonce = nonce;
            if evaluate_v3_header_pow(&different, CHAIN).unwrap().pow.accepted {
                mined = true;
                break;
            }
        }
        assert!(mined, "valid alternate PoW fixture must exist");
        let different_companion = crate::NativeV3HeaderRestoreRecordV1::from_verified_header(
            CHAIN,
            different,
            FROZEN_NS,
            &frozen_hash,
        )
        .unwrap();
        let error = record
            .verify_matching_header_record(&different_companion, CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap_err();
        assert!(
            error.to_string().contains("does not match full envelope"),
            "two independently valid records must match: {error}"
        );
    }

    #[test]
    fn rejects_legacy_fields_unknown_nested_fields_hash_and_tx_tampering() {
        let (record, frozen_hash) = fixture();
        let raw = record
            .encode_verified(CHAIN, FROZEN_NS, &frozen_hash)
            .unwrap();
        let canonical: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        for field in ["timestamp", "future_header_field"] {
            let mut wrong = canonical.clone();
            wrong["envelope"]["header"][field] = serde_json::json!(42);
            assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
                &serde_json::to_vec(&wrong).unwrap(),
                CHAIN,
                FROZEN_NS,
                &frozen_hash,
            )
            .is_err());
        }
        for pointer in [
            "/future_record_field",
            "/envelope/future_field",
            "/envelope/transactions/0/future_field",
        ] {
            let mut wrong = canonical.clone();
            let object = if pointer == "/future_record_field" {
                wrong.as_object_mut().unwrap()
            } else if pointer == "/envelope/future_field" {
                wrong["envelope"].as_object_mut().unwrap()
            } else {
                wrong["envelope"]["transactions"][0]
                    .as_object_mut()
                    .unwrap()
            };
            object.insert("future_field".to_string(), serde_json::json!(true));
            assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
                &serde_json::to_vec(&wrong).unwrap(),
                CHAIN,
                FROZEN_NS,
                &frozen_hash,
            )
            .is_err());
        }
        let mut forged_hash = canonical.clone();
        forged_hash["envelope"]["hash"] = serde_json::json!("aa".repeat(32));
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&forged_hash).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
        let mut forged_tx = canonical;
        forged_tx["envelope"]["transactions"][0]["nonce"] = serde_json::json!(99);
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &serde_json::to_vec(&forged_tx).unwrap(),
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
        assert!(NativeV3EnvelopeRestoreRecordV1::decode_verified(
            &vec![b' '; MAX_NATIVE_V3_ENVELOPE_RESTORE_RECORD_BYTES + 1],
            CHAIN,
            FROZEN_NS,
            &frozen_hash,
        )
        .is_err());
    }
}
