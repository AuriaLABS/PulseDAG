//! Dormant production-v3 header format and nanosecond timestamp commitment.
//!
//! This is a deliberately separate wire/PoW domain from v1/v2, whose
//! BlockHeader.timestamp is defined in Unix *seconds*. This module does not
//! change block admission, mining, P2P, replay, persistence or startup. The
//! v3 network/activation identities must be frozen before enabling it.

use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError, protocol::BLOCK_HEADER_VERSION_V3, selection_v2::GHOSTDAG_V1_MAX_PARENTS,
    types::BlockId,
};

const BLOCK_HEADER_V3_DOMAIN: &[u8] = b"PulseDAG:block-header:v3:nanoseconds";
// Bound header serialization *before* allocating/copying untrusted strings.
const V3_MAX_CHAIN_ID_BYTES: usize = 128;
const V3_HEX_HASH_BYTES: usize = 64;
const V3_MAX_CANONICAL_HEADER_BYTES: usize = 8_192;

/// Distinct from the seconds-based v1/v2 BlockHeader. Do not deserialize
/// existing v2 blocks into this type or reinterpret their timestamp units.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BlockHeaderV3 {
    pub version: u32,
    pub parents: Vec<BlockId>,
    /// Unix nanoseconds. The exact unit is committed by the versioned domain.
    pub timestamp_ns: u64,
    pub difficulty: u32,
    pub nonce: u64,
    pub merkle_root: String,
    pub state_root: String,
    pub blue_score: u64,
    pub height: u64,
}

fn invalid_v3(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(message.into())
}

fn canonical_v3_hash(value: &str) -> bool {
    value.len() == V3_HEX_HASH_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn encode_len_prefixed(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), PulseError> {
    let len = u32::try_from(bytes.len())
        .map_err(|_| invalid_v3("v3 header canonical field exceeds u32::MAX"))?;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn encode_string(out: &mut Vec<u8>, value: &str) -> Result<(), PulseError> {
    encode_len_prefixed(out, value.as_bytes())
}

pub fn validate_block_header_v3_shape(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<(), PulseError> {
    if header.version != BLOCK_HEADER_VERSION_V3 {
        return Err(invalid_v3(format!(
            "header v3 requires version {}, got {}",
            BLOCK_HEADER_VERSION_V3, header.version
        )));
    }
    if chain_id.is_empty()
        || chain_id.len() > V3_MAX_CHAIN_ID_BYTES
        || !chain_id.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(PulseError::ChainIdMismatch);
    }
    if header.timestamp_ns == 0 {
        return Err(invalid_v3("header v3 Unix nanoseconds must be positive"));
    }
    if header.parents.len() > GHOSTDAG_V1_MAX_PARENTS {
        return Err(invalid_v3(format!(
            "header v3 parents {} exceed the limit {}",
            header.parents.len(),
            GHOSTDAG_V1_MAX_PARENTS
        )));
    }
    if header.height == 0 && !header.parents.is_empty() {
        return Err(invalid_v3("header v3 genesis height must not reference parents"));
    }
    if header.height > 0 && header.parents.is_empty() {
        return Err(invalid_v3("header v3 non-genesis block requires a parent"));
    }
    if header.parents.iter().any(|hash| !canonical_v3_hash(hash)) {
        return Err(invalid_v3("header v3 parents require 64 lowercase hex digits"));
    }
    if !canonical_v3_hash(&header.merkle_root) || !canonical_v3_hash(&header.state_root) {
        return Err(invalid_v3("header v3 commitment roots require 64 lowercase hex digits"));
    }
    if header
        .parents
        .windows(2)
        .any(|pair| pair[0].as_bytes() >= pair[1].as_bytes())
    {
        return Err(invalid_v3(
            "header v3 parents must be unique and canonically ordered",
        ));
    }
    Ok(())
}

fn canonical_header_material_v3(
    header: &BlockHeaderV3,
    chain_id: &str,
    include_nonce: bool,
) -> Result<Vec<u8>, PulseError> {
    validate_block_header_v3_shape(header, chain_id)?;
    let mut out = Vec::with_capacity(320);
    encode_len_prefixed(&mut out, BLOCK_HEADER_V3_DOMAIN)?;
    encode_string(&mut out, chain_id)?;
    out.extend_from_slice(&header.version.to_le_bytes());
    out.extend_from_slice(&(header.parents.len() as u32).to_le_bytes());
    for parent in &header.parents {
        encode_string(&mut out, parent)?;
    }
    out.extend_from_slice(&header.timestamp_ns.to_le_bytes());
    out.extend_from_slice(&header.difficulty.to_le_bytes());
    if include_nonce {
        out.extend_from_slice(&header.nonce.to_le_bytes());
    }
    encode_string(&mut out, &header.merkle_root)?;
    encode_string(&mut out, &header.state_root)?;
    out.extend_from_slice(&header.blue_score.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    if out.len() > V3_MAX_CANONICAL_HEADER_BYTES {
        return Err(invalid_v3("header v3 canonical bytes exceed the fixed limit"));
    }
    Ok(out)
}

/// Canonical v3 hash commitment, including nonce.
pub fn canonical_block_header_bytes_v3(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    canonical_header_material_v3(header, chain_id, true)
}

/// Nonce-free v3 PoW seed material. No mining engine uses this yet.
pub fn canonical_mining_preimage_bytes_v3(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    canonical_header_material_v3(header, chain_id, false)
}

pub fn compute_block_hash_v3(
    header: &BlockHeaderV3,
    chain_id: &str,
) -> Result<BlockId, PulseError> {
    let digest = Sha256::digest(canonical_block_header_bytes_v3(header, chain_id)?);
    Ok(hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        header_v2::canonical_block_header_bytes_v2, protocol::BLOCK_HEADER_VERSION_V2,
        types::BlockHeader,
    };

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
    fn v3_commits_independent_nanosecond_precision_and_chain_domain() {
        let a = header(BASE_NS + 100_000_000);
        let b = header(BASE_NS + 600_000_000);
        assert_eq!(
            a.timestamp_ns / 1_000_000_000,
            b.timestamp_ns / 1_000_000_000
        );
        assert_ne!(
            canonical_block_header_bytes_v3(&a, "pulsedag-v3-testnet").unwrap(),
            canonical_block_header_bytes_v3(&b, "pulsedag-v3-testnet").unwrap()
        );
        assert_ne!(
            compute_block_hash_v3(&a, "pulsedag-v3-testnet").unwrap(),
            compute_block_hash_v3(&a, "pulsedag-v3-mainnet").unwrap()
        );
    }

    #[test]
    fn v3_header_and_pow_domain_are_distinct_from_v2_seconds() {
        let v3 = header(BASE_NS);
        let v2 = BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: v3.parents.clone(),
            timestamp: BASE_NS / 1_000_000_000,
            difficulty: v3.difficulty,
            nonce: v3.nonce,
            merkle_root: v3.merkle_root.clone(),
            state_root: v3.state_root.clone(),
            blue_score: v3.blue_score,
            height: v3.height,
        };
        let chain = "pulsedag-candidate-test";
        let v3_bytes = canonical_block_header_bytes_v3(&v3, chain).unwrap();
        let v2_bytes = canonical_block_header_bytes_v2(&v2, chain).unwrap();
        assert_ne!(v3_bytes, v2_bytes);
        assert_ne!(
            hex::encode(Sha256::digest(&v3_bytes)),
            hex::encode(Sha256::digest(&v2_bytes))
        );
    }

    #[test]
    fn nonce_affects_header_hash_but_not_pow_seed() {
        let a = header(BASE_NS);
        let mut b = a.clone();
        b.nonce += 1;
        assert_ne!(
            canonical_block_header_bytes_v3(&a, "v3").unwrap(),
            canonical_block_header_bytes_v3(&b, "v3").unwrap()
        );
        assert_eq!(
            canonical_mining_preimage_bytes_v3(&a, "v3").unwrap(),
            canonical_mining_preimage_bytes_v3(&b, "v3").unwrap()
        );
    }

    #[test]
    fn wrong_version_empty_identity_timestamp_and_parent_order_fail_closed() {
        let mut candidate = header(BASE_NS);
        candidate.version = BLOCK_HEADER_VERSION_V2;
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.version = BLOCK_HEADER_VERSION_V3;
        assert!(canonical_block_header_bytes_v3(&candidate, "").is_err());
        candidate.timestamp_ns = 0;
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.timestamp_ns = BASE_NS;
        candidate.parents = vec!["22".repeat(32), "11".repeat(32)];
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.parents = vec!["11".repeat(32), "11".repeat(32)];
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.parents.clear();
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
    }

    #[test]
    fn height_zero_requires_parentless_genesis_and_non_genesis_requires_parents() {
        let mut candidate = header(BASE_NS);
        candidate.height = 0;
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.parents.clear();
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_ok());
        candidate.height = 1;
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
    }

    #[test]
    fn oversized_and_noncanonical_fields_reject_before_encoding() {
        let mut candidate = header(BASE_NS);
        assert!(canonical_block_header_bytes_v3(
            &candidate,
            &"x".repeat(V3_MAX_CHAIN_ID_BYTES + 1),
        )
        .is_err());
        assert!(canonical_block_header_bytes_v3(&candidate, "v3\ninvalid").is_err());
        candidate.parents = vec!["11".repeat(32) + "00"];
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.parents = vec!["GG".repeat(32)];
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.parents = vec!["AA".repeat(32)];
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.parents = vec!["11".repeat(32)];
        candidate.merkle_root = "22".repeat(33);
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
        candidate.merkle_root = "22".repeat(32);
        candidate.state_root = "3".repeat(63);
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
    }

    #[test]
    fn maximum_parent_window_has_a_bounded_canonical_envelope() {
        let mut candidate = header(BASE_NS);
        candidate.parents = (0..GHOSTDAG_V1_MAX_PARENTS)
            .map(|i| format!("{i:064x}"))
            .collect();
        let full = canonical_block_header_bytes_v3(
            &candidate,
            &"n".repeat(V3_MAX_CHAIN_ID_BYTES),
        )
        .unwrap();
        let mining = canonical_mining_preimage_bytes_v3(
            &candidate,
            &"n".repeat(V3_MAX_CHAIN_ID_BYTES),
        )
        .unwrap();
        assert!(full.len() <= V3_MAX_CANONICAL_HEADER_BYTES);
        assert!(mining.len() < full.len());
        candidate.parents.push(format!("{:064x}", GHOSTDAG_V1_MAX_PARENTS));
        assert!(canonical_block_header_bytes_v3(&candidate, "v3").is_err());
    }

    #[test]
    fn same_header_is_byte_identical_across_repeated_encodings() {
        let candidate = header(BASE_NS);
        let bytes = canonical_block_header_bytes_v3(&candidate, "pulsedag-test").unwrap();
        assert_eq!(
            bytes,
            canonical_block_header_bytes_v3(&candidate, "pulsedag-test").unwrap()
        );
        assert_eq!(
            bytes,
            canonical_block_header_bytes_v3(&candidate, "pulsedag-test").unwrap()
        );
    }
}
