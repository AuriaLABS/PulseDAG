//! Dormant, genesis-anchored native-v3 LINEAR restore segment integrity.
//!
//! This deliberately supports only a single-parent chain of at most 20
//! non-genesis records starting immediately after an independently frozen
//! genesis. It is NOT a multi-parent DAG algorithm, an authoritative restore
//! executor, or a full snapshot validator. There is no RocksDB, P2P, mining,
//! reward/UTXO replay, selected-parent/blue-work computation, or activation.
//! Success certifies only bounded cryptographic frame and parent/height/time
//! linkage. It does NOT establish whether the blocks belong to canonical DAG
//! order, whether committed state roots are true, or whether 500ms retarget
//! rules were followed. Do not feed the result to production admission.

use pulsedag_core::{errors::PulseError, genesis_v3::dormant_native_genesis_envelope_v3_ns};

use crate::NativeV3EnvelopeRestoreRecordV1;

/// Local verification work cap; NOT a network/consensus block limit.
pub const MAX_DORMANT_V3_LINEAR_RESTORE_RECORDS: usize = 20;

/// Diagnostic output only. Not an accepted-block token or state checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantV3LinearRestoreCheck {
    pub linkage_checked_records: usize,
    pub last_hash: String,
    pub last_height: u64,
    pub last_timestamp_ns: u64,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::StorageError(format!(
        "dormant native-v3 linear restore: {}",
        message.into()
    ))
}

/// Verify a small, genesis-anchored single-parent sequence without touching DB.
///
/// The chain ID, genesis timestamp and genesis hash must come from a trusted
/// independent network freeze, NEVER from the presented records. Every record
/// verifies its native-v3 PoW and transaction commitments independently.
///
/// Requiring exactly one parent is intentional: this does not define selection
/// for a multi-parent DAG. There is no consensus timestamp cadence rule here;
/// strict monotonicity is necessary, but not enough, for future 500ms consensus.
pub fn verify_dormant_v3_genesis_anchored_linear_segment(
    records: &[NativeV3EnvelopeRestoreRecordV1],
    trusted_chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
) -> Result<DormantV3LinearRestoreCheck, PulseError> {
    if records.is_empty() || records.len() > MAX_DORMANT_V3_LINEAR_RESTORE_RECORDS {
        return Err(invalid(format!(
            "record count {} outside 1..={MAX_DORMANT_V3_LINEAR_RESTORE_RECORDS}",
            records.len()
        )));
    }
    let genesis =
        dormant_native_genesis_envelope_v3_ns(trusted_chain_id, frozen_genesis_timestamp_ns)
            .map_err(|error| invalid(format!("invalid externally frozen genesis: {error}")))?;
    if genesis.hash != frozen_genesis_hash {
        return Err(invalid(
            "frozen genesis identity does not match independently derived hash",
        ));
    }

    let mut preceding_hash = genesis.hash;
    let mut preceding_height = genesis.header.height;
    let mut preceding_time = genesis.header.timestamp_ns;
    for (index, record) in records.iter().enumerate() {
        record
            .verify(
                trusted_chain_id,
                frozen_genesis_timestamp_ns,
                frozen_genesis_hash,
            )
            .map_err(|error| {
                invalid(format!("record {index} failed frame verification: {error}"))
            })?;

        let envelope = record.envelope();
        if envelope.header.parents.len() != 1 || envelope.header.parents[0] != preceding_hash {
            return Err(invalid(format!(
                "record {index} is not linked by its unique parent to the preceding frame"
            )));
        }
        let expected_height = preceding_height
            .checked_add(1)
            .ok_or_else(|| invalid("genesis-anchored linear height overflow"))?;
        if envelope.header.height != expected_height {
            return Err(invalid(format!(
                "record {index} height {} differs from expected {expected_height}",
                envelope.header.height
            )));
        }
        if envelope.header.timestamp_ns <= preceding_time {
            return Err(invalid(format!(
                "record {index} nanosecond timestamp is not strictly newer than its parent"
            )));
        }

        preceding_hash = envelope.hash.clone();
        preceding_height = envelope.header.height;
        preceding_time = envelope.header.timestamp_ns;
    }

    Ok(DormantV3LinearRestoreCheck {
        linkage_checked_records: records.len(),
        last_hash: preceding_hash,
        last_height: preceding_height,
        last_timestamp_ns: preceding_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        block_envelope_v3::{build_block_envelope_v3, BlockEnvelopeV3},
        header_v3::BlockHeaderV3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "dormant-v3-linear-restore-test";
    const FROZEN_NS: u64 = 1_800_000_000_500_000_000;

    fn mine_record(
        parent: &BlockEnvelopeV3,
        height: u64,
        timestamp_ns: u64,
        extra_parent: Option<String>,
        frozen_hash: &str,
    ) -> NativeV3EnvelopeRestoreRecordV1 {
        let mut parents = vec![parent.hash.clone()];
        if let Some(extra) = extra_parent {
            parents.push(extra);
            parents.sort();
        }
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns,
            difficulty: parent.header.difficulty,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        };
        let mut found = false;
        for nonce in 0..4096 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                found = true;
                break;
            }
        }
        assert!(found, "easy-target v3 header fixture must find a nonce");
        let envelope = build_block_envelope_v3(CHAIN, header, vec![]).unwrap();
        NativeV3EnvelopeRestoreRecordV1::from_verified_envelope(
            envelope,
            CHAIN,
            FROZEN_NS,
            frozen_hash,
        )
        .unwrap()
    }

    fn sequence() -> (String, Vec<NativeV3EnvelopeRestoreRecordV1>) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS).unwrap();
        let first = mine_record(&genesis, 1, FROZEN_NS + 500_000_000, None, &genesis.hash);
        let second = mine_record(
            first.envelope(),
            2,
            FROZEN_NS + 1_000_000_000,
            None,
            &genesis.hash,
        );
        (genesis.hash, vec![first, second])
    }

    #[test]
    fn validates_two_canonical_ns_frames_from_independent_frozen_genesis() {
        let (genesis, records) = sequence();
        let report =
            verify_dormant_v3_genesis_anchored_linear_segment(&records, CHAIN, FROZEN_NS, &genesis)
                .unwrap();
        assert_eq!(report.linkage_checked_records, 2);
        assert_eq!(report.last_hash, records[1].envelope().hash);
        assert_eq!(report.last_height, 2);
        assert_eq!(report.last_timestamp_ns, FROZEN_NS + 1_000_000_000);
    }

    #[test]
    fn refuses_empty_excessive_and_wrong_network_freeze() {
        let (genesis, records) = sequence();
        assert!(
            verify_dormant_v3_genesis_anchored_linear_segment(&[], CHAIN, FROZEN_NS, &genesis,)
                .is_err()
        );
        let too_many = vec![records[0].clone(); MAX_DORMANT_V3_LINEAR_RESTORE_RECORDS + 1];
        assert!(verify_dormant_v3_genesis_anchored_linear_segment(
            &too_many, CHAIN, FROZEN_NS, &genesis,
        )
        .is_err());
        for (chain, time, hash) in [
            ("wrong-network", FROZEN_NS, genesis.as_str()),
            (CHAIN, FROZEN_NS + 1, genesis.as_str()),
            (CHAIN, FROZEN_NS, "00"),
        ] {
            assert!(
                verify_dormant_v3_genesis_anchored_linear_segment(&records, chain, time, hash,)
                    .is_err()
            );
        }
    }

    #[test]
    fn refuses_out_of_order_duplicate_and_missing_parent_records() {
        let (genesis, records) = sequence();
        for attempted in [
            vec![records[1].clone(), records[0].clone()],
            vec![records[0].clone(), records[0].clone()],
            vec![records[1].clone()],
        ] {
            assert!(verify_dormant_v3_genesis_anchored_linear_segment(
                &attempted, CHAIN, FROZEN_NS, &genesis,
            )
            .is_err());
        }
    }

    #[test]
    fn refuses_valid_pow_with_wrong_height_stale_time_or_extra_parent() {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS).unwrap();
        let invalids = [
            mine_record(&genesis, 2, FROZEN_NS + 500_000_000, None, &genesis.hash),
            mine_record(&genesis, 1, FROZEN_NS, None, &genesis.hash),
            mine_record(
                &genesis,
                1,
                FROZEN_NS + 500_000_000,
                Some("ff".repeat(32)),
                &genesis.hash,
            ),
        ];
        for invalid in invalids {
            assert!(verify_dormant_v3_genesis_anchored_linear_segment(
                &[invalid],
                CHAIN,
                FROZEN_NS,
                &genesis.hash,
            )
            .is_err());
        }
    }
}
