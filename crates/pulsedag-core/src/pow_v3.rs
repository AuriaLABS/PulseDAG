//! Dormant v3 PoW adapter using chain-bound, nanosecond header material.
//!
//! This uses the existing deterministic KHeavyHash engine, but NEVER routes a
//! v3 header through the legacy/v2 seconds-based PoW preimage. The module is
//! deliberately not connected to network/mined block acceptance, mining RPC,
//! persistence, or daemon startup. No v3 protocol is activated by this module.

use crate::{
    errors::PulseError,
    header_v3::{canonical_mining_preimage_bytes_v3, compute_block_hash_v3, BlockHeaderV3},
    pow::{
        bits_from_target, canonical_pow_engine, compare_pow_hash_to_target, target_from_bits,
        PowEngine, PowEvaluation,
    },
    retarget::{consensus_min_target, consensus_pow_limit_target},
    types::BlockId,
};

/// A single chain-bound v3 PoW evaluation. Block hash (SHA-256 of the header)
/// and PoW digest (KHeavyHash of nonce-free v3 preimage + nonce) are distinct
/// commitments. Neither should be substituted for the other.
#[derive(Debug, Clone)]
pub struct V3HeaderPowEvaluation {
    pub block_hash: BlockId,
    pub pow: PowEvaluation,
}

/// Reject noncanonical or out-of-bounds target encoding before hashing so
/// accepted/mined versions cannot disagree about a compact representation.
pub fn validate_v3_pow_target(bits: u32) -> Result<(), PulseError> {
    let target = target_from_bits(bits);
    if bits != bits_from_target(&target)
        || target < consensus_min_target()
        || target > consensus_pow_limit_target()
    {
        return Err(PulseError::InvalidBlock(format!(
            "v3 PoW target bits {bits:#010x} are noncanonical or out of consensus bounds"
        )));
    }
    Ok(())
}

/// The nonce-independent, chain-domain-separated material used for v3 PoW.
/// This is a separate format from all legacy/v2 header encodings.
pub fn v3_pow_preimage_bytes(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    validate_v3_pow_target(header.difficulty)?;
    canonical_mining_preimage_bytes_v3(header, chain_id)
}

/// Evaluate one v3 header using the exact canonical KHeavyHash implementation
/// from legacy/v2, but on the **v3** nanosecond preimage/domain. This helper
/// only evaluates the header; it does not authorize a mined/P2P block.
pub fn evaluate_v3_header_pow(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<V3HeaderPowEvaluation, PulseError> {
    let preimage = v3_pow_preimage_bytes(header, chain_id)?;
    let block_hash = compute_block_hash_v3(header, chain_id)?;
    let pow = canonical_pow_engine().evaluate_pre_pow_bytes_with_nonce(
        &preimage,
        header.nonce,
        header.difficulty,
    );
    debug_assert_eq!(
        pow.accepted,
        compare_pow_hash_to_target(&pow.hash, &target_from_bits(header.difficulty))
    );
    Ok(V3HeaderPowEvaluation { block_hash, pow })
}

/// Validate a standalone v3 PoW digest, not a consensus DAG admission. Future
/// mined/P2P admission MUST also validate selected parent, timestamp drift,
/// difficulty, state root, monetary reward and protocol activation identity.
pub fn verify_v3_header_pow(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<V3HeaderPowEvaluation, PulseError> {
    let evaluation = evaluate_v3_header_pow(header, chain_id)?;
    if !evaluation.pow.accepted {
        return Err(PulseError::InvalidBlock(
            "v3 PoW hash exceeds the canonical compact target".into(),
        ));
    }
    Ok(evaluation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::{BLOCK_HEADER_VERSION_V2, BLOCK_HEADER_VERSION_V3},
        types::{canonical_mining_preimage_bytes, BlockHeader},
    };

    const CHAIN: &str = "pulsedag-v3-pow-candidate";
    const BASE_NS: u64 = 1_800_000_000_000_000_000;

    fn header(timestamp_ns: u64) -> BlockHeaderV3 {
        BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec!["11".repeat(32)],
            timestamp_ns,
            difficulty: 0x207f_ffff,
            nonce: 14,
            merkle_root: "22".repeat(32),
            state_root: "33".repeat(32),
            blue_score: 12,
            height: 5,
        }
    }

    #[test]
    fn same_second_distinct_subsecond_headers_have_distinct_v3_pow() {
        let first = header(BASE_NS + 100_000_000);
        let second = header(BASE_NS + 600_000_000);
        assert_eq!(
            first.timestamp_ns / 1_000_000_000,
            second.timestamp_ns / 1_000_000_000
        );
        assert_ne!(
            v3_pow_preimage_bytes(&first, CHAIN).unwrap(),
            v3_pow_preimage_bytes(&second, CHAIN).unwrap()
        );
        let first = evaluate_v3_header_pow(&first, CHAIN).unwrap();
        let second = evaluate_v3_header_pow(&second, CHAIN).unwrap();
        assert_ne!(first.block_hash, second.block_hash);
        assert_ne!(first.pow.hash, second.pow.hash);
    }

    #[test]
    fn chain_identity_is_committed_by_header_and_pow_domains() {
        let candidate = header(BASE_NS);
        let a = evaluate_v3_header_pow(&candidate, "candidate-mainnet").unwrap();
        let b = evaluate_v3_header_pow(&candidate, "candidate-testnet").unwrap();
        assert_ne!(a.block_hash, b.block_hash);
        assert_ne!(a.pow.hash, b.pow.hash);
    }

    #[test]
    fn nonce_is_excluded_from_pow_seed_but_bound_to_both_hashes() {
        let first = header(BASE_NS);
        let mut second = first.clone();
        second.nonce = second.nonce.saturating_add(1);
        assert_eq!(
            v3_pow_preimage_bytes(&first, CHAIN).unwrap(),
            v3_pow_preimage_bytes(&second, CHAIN).unwrap()
        );
        let a = evaluate_v3_header_pow(&first, CHAIN).unwrap();
        let b = evaluate_v3_header_pow(&second, CHAIN).unwrap();
        assert_ne!(a.block_hash, b.block_hash);
        assert_ne!(a.pow.hash, b.pow.hash);
    }

    #[test]
    fn v2_seconds_pow_seed_cannot_alias_v3_nanosecond_domain() {
        let candidate = header(BASE_NS);
        let v2 = BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: candidate.parents.clone(),
            timestamp: BASE_NS / 1_000_000_000,
            difficulty: candidate.difficulty,
            nonce: candidate.nonce,
            merkle_root: candidate.merkle_root.clone(),
            state_root: candidate.state_root.clone(),
            blue_score: candidate.blue_score,
            height: candidate.height,
        };
        assert_ne!(
            v3_pow_preimage_bytes(&candidate, CHAIN).unwrap(),
            canonical_mining_preimage_bytes(&v2)
        );
    }

    #[test]
    fn fail_closed_on_invalid_version_header_shape_or_compact_bits() {
        let mut candidate = header(BASE_NS);
        candidate.version = BLOCK_HEADER_VERSION_V2;
        assert!(evaluate_v3_header_pow(&candidate, CHAIN).is_err());
        candidate.version = BLOCK_HEADER_VERSION_V3;
        candidate.timestamp_ns = 0;
        assert!(evaluate_v3_header_pow(&candidate, CHAIN).is_err());
        candidate.timestamp_ns = BASE_NS;
        candidate.difficulty = 0;
        assert!(evaluate_v3_header_pow(&candidate, CHAIN).is_err());
        candidate.difficulty = 0x2100_0000;
        assert!(evaluate_v3_header_pow(&candidate, CHAIN).is_err());
        assert!(evaluate_v3_header_pow(&header(BASE_NS), "").is_err());
    }

    #[test]
    fn pow_decision_uses_full_256_bit_compact_target() {
        let candidate = header(BASE_NS);
        let eval = evaluate_v3_header_pow(&candidate, CHAIN).unwrap();
        assert_eq!(
            eval.pow.accepted,
            compare_pow_hash_to_target(&eval.pow.hash, &target_from_bits(candidate.difficulty))
        );
        assert_eq!(eval.pow.hash_hex, hex::encode(eval.pow.hash));
        if eval.pow.accepted {
            assert!(verify_v3_header_pow(&candidate, CHAIN).is_ok());
        } else {
            assert!(verify_v3_header_pow(&candidate, CHAIN).is_err());
        }
    }
}
