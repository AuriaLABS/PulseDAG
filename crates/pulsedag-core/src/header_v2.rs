use sha2::{Digest, Sha256};

use crate::{
    errors::PulseError,
    protocol::BLOCK_HEADER_VERSION_V2,
    selection_v2::GHOSTDAG_V1_MAX_PARENTS,
    types::{BlockHeader, BlockId},
};

const BLOCK_HEADER_V2_DOMAIN: &[u8] = b"PulseDAG:block-header:v2";

fn encode_len_prefixed_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).expect("canonical field length exceeds u32::MAX");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(bytes);
}

fn encode_len_prefixed_str(out: &mut Vec<u8>, value: &str) {
    encode_len_prefixed_bytes(out, value.as_bytes());
}

fn encode_parent_vec(out: &mut Vec<u8>, parents: &[BlockId]) {
    let count = u32::try_from(parents.len()).expect("parent count exceeds u32::MAX");
    out.extend_from_slice(&count.to_le_bytes());
    for parent in parents {
        encode_len_prefixed_str(out, parent);
    }
}

pub fn canonicalize_block_parents_v2(parents: &[BlockId]) -> Result<Vec<BlockId>, PulseError> {
    if parents.len() > GHOSTDAG_V1_MAX_PARENTS {
        return Err(PulseError::InvalidBlock(format!(
            "header v2 parent count {} exceeds maximum {}",
            parents.len(),
            GHOSTDAG_V1_MAX_PARENTS
        )));
    }
    if parents.iter().any(String::is_empty) {
        return Err(PulseError::InvalidBlock(
            "header v2 parent hash must not be empty".to_string(),
        ));
    }

    let mut canonical = parents.to_vec();
    canonical.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    if canonical.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(PulseError::InvalidBlock(
            "header v2 parent hashes must be unique".to_string(),
        ));
    }
    Ok(canonical)
}

pub fn validate_block_header_v2_shape(
    header: &BlockHeader,
    chain_id: &str,
) -> Result<(), PulseError> {
    if header.version != BLOCK_HEADER_VERSION_V2 {
        return Err(PulseError::InvalidBlock(format!(
            "block header v2 serialization requires version {BLOCK_HEADER_VERSION_V2}, got {}",
            header.version
        )));
    }
    if chain_id.is_empty() {
        return Err(PulseError::ChainIdMismatch);
    }

    let canonical_parents = canonicalize_block_parents_v2(&header.parents)?;
    if header.height > 0 && canonical_parents.is_empty() {
        return Err(PulseError::InvalidBlock(
            "non-genesis header v2 requires at least one parent".to_string(),
        ));
    }
    if canonical_parents != header.parents {
        return Err(PulseError::InvalidBlock(
            "header v2 parent vector is not in canonical ascending order".to_string(),
        ));
    }
    Ok(())
}

fn canonical_block_header_material_v2(
    header: &BlockHeader,
    chain_id: &str,
    include_nonce: bool,
) -> Result<Vec<u8>, PulseError> {
    validate_block_header_v2_shape(header, chain_id)?;

    let mut out = Vec::with_capacity(320);
    encode_len_prefixed_bytes(&mut out, BLOCK_HEADER_V2_DOMAIN);
    encode_len_prefixed_str(&mut out, chain_id);
    out.extend_from_slice(&header.version.to_le_bytes());
    encode_parent_vec(&mut out, &header.parents);
    out.extend_from_slice(&header.timestamp.to_le_bytes());
    out.extend_from_slice(&header.difficulty.to_le_bytes());
    if include_nonce {
        out.extend_from_slice(&header.nonce.to_le_bytes());
    }
    encode_len_prefixed_str(&mut out, &header.merkle_root);
    encode_len_prefixed_str(&mut out, &header.state_root);
    out.extend_from_slice(&header.blue_score.to_le_bytes());
    out.extend_from_slice(&header.height.to_le_bytes());
    Ok(out)
}

pub fn canonical_block_header_bytes_v2(
    header: &BlockHeader,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    canonical_block_header_material_v2(header, chain_id, true)
}

pub fn canonical_mining_preimage_bytes_v2(
    header: &BlockHeader,
    chain_id: &str,
) -> Result<Vec<u8>, PulseError> {
    canonical_block_header_material_v2(header, chain_id, false)
}

pub fn compute_block_hash_v2(header: &BlockHeader, chain_id: &str) -> Result<BlockId, PulseError> {
    let digest = Sha256::digest(canonical_block_header_bytes_v2(header, chain_id)?);
    Ok(hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::compute_block_hash;

    fn parent(byte: &str) -> String {
        byte.repeat(64)
    }

    fn sample_header(version: u32, parents: Vec<String>) -> BlockHeader {
        BlockHeader {
            version,
            parents,
            timestamp: 1_700_000_000,
            difficulty: 42,
            nonce: 7,
            merkle_root: "33".repeat(32),
            state_root: "44".repeat(32),
            blue_score: 123,
            height: 456,
        }
    }

    #[test]
    fn v1_block_hash_golden_vector_remains_unchanged() {
        let header = sample_header(1, vec![parent("1"), parent("2")]);
        assert_eq!(
            compute_block_hash(&header),
            "f21a0c2acc704f35ae511c43e1b9edb53b049b5c593302cfc0a6a35961981278"
        );
    }

    #[test]
    fn v2_block_hash_golden_vectors_are_chain_bound() {
        let header = sample_header(
            BLOCK_HEADER_VERSION_V2,
            vec!["11".repeat(32), "22".repeat(32)],
        );
        assert_eq!(
            compute_block_hash_v2(&header, "pulsedag-testnet").unwrap(),
            "619150aadab3746865bb4ffa54d5431675b12e765c7d1658a5a8fd2ca32a385b"
        );
        assert_eq!(
            compute_block_hash_v2(&header, "pulsedag-private").unwrap(),
            "4b8e3107055bbcc2a0aee83f8ec2030d406c4f5ad206e3b6a990f4d37f7d1496"
        );
    }

    #[test]
    fn v2_mining_preimage_golden_vectors_are_chain_bound() {
        let header = sample_header(
            BLOCK_HEADER_VERSION_V2,
            vec!["11".repeat(32), "22".repeat(32)],
        );
        let testnet = canonical_mining_preimage_bytes_v2(&header, "pulsedag-testnet").unwrap();
        let private = canonical_mining_preimage_bytes_v2(&header, "pulsedag-private").unwrap();

        assert_eq!(testnet.len(), 356);
        assert_eq!(private.len(), 356);
        assert_eq!(
            hex::encode(Sha256::digest(&testnet)),
            "7d10b1c9c93a444c9424b34c028ccc4d95f8feeaa82769da42304636892a5c1e"
        );
        assert_eq!(
            hex::encode(Sha256::digest(&private)),
            "3bc47ac6314674695f81f5bc10f11eedebb814c979276c22782421f65d84bc8a"
        );
        assert_ne!(testnet, private);
    }

    #[test]
    fn v2_mining_preimage_excludes_only_nonce_material() {
        let header = sample_header(
            BLOCK_HEADER_VERSION_V2,
            vec!["11".repeat(32), "22".repeat(32)],
        );
        let mut different_nonce = header.clone();
        different_nonce.nonce = header.nonce + 1;

        assert_eq!(
            canonical_mining_preimage_bytes_v2(&header, "pulsedag-testnet").unwrap(),
            canonical_mining_preimage_bytes_v2(&different_nonce, "pulsedag-testnet").unwrap()
        );
        assert_ne!(
            canonical_block_header_bytes_v2(&header, "pulsedag-testnet").unwrap(),
            canonical_block_header_bytes_v2(&different_nonce, "pulsedag-testnet").unwrap()
        );
    }

    #[test]
    fn local_parent_permutations_canonicalize_to_one_vector() {
        let forward = vec!["11".repeat(32), "22".repeat(32)];
        let reverse = vec!["22".repeat(32), "11".repeat(32)];
        assert_eq!(
            canonicalize_block_parents_v2(&forward).unwrap(),
            canonicalize_block_parents_v2(&reverse).unwrap()
        );
    }

    #[test]
    fn canonicalized_parent_permutations_produce_one_v2_preimage() {
        let forward = canonicalize_block_parents_v2(&["11".repeat(32), "22".repeat(32)]).unwrap();
        let reverse = canonicalize_block_parents_v2(&["22".repeat(32), "11".repeat(32)]).unwrap();
        let a = sample_header(BLOCK_HEADER_VERSION_V2, forward);
        let b = sample_header(BLOCK_HEADER_VERSION_V2, reverse);

        assert_eq!(
            canonical_mining_preimage_bytes_v2(&a, "pulsedag-testnet").unwrap(),
            canonical_mining_preimage_bytes_v2(&b, "pulsedag-testnet").unwrap()
        );
    }

    #[test]
    fn received_non_canonical_parent_order_fails_closed() {
        let header = sample_header(
            BLOCK_HEADER_VERSION_V2,
            vec!["22".repeat(32), "11".repeat(32)],
        );
        assert!(matches!(
            canonical_block_header_bytes_v2(&header, "pulsedag-testnet"),
            Err(PulseError::InvalidBlock(message)) if message.contains("canonical ascending order")
        ));
        assert!(canonical_mining_preimage_bytes_v2(&header, "pulsedag-testnet").is_err());
    }

    #[test]
    fn duplicate_empty_and_over_limit_parents_fail_closed() {
        assert!(canonicalize_block_parents_v2(&["aa".to_string(), "aa".to_string()]).is_err());
        assert!(canonicalize_block_parents_v2(&[String::new()]).is_err());
        let too_many = (0..=GHOSTDAG_V1_MAX_PARENTS)
            .map(|index| format!("{index:064x}"))
            .collect::<Vec<_>>();
        assert!(canonicalize_block_parents_v2(&too_many).is_err());
    }

    #[test]
    fn wrong_version_empty_chain_and_parentless_non_genesis_fail_closed() {
        let v1 = sample_header(1, vec!["11".repeat(32)]);
        assert!(canonical_block_header_bytes_v2(&v1, "pulsedag-testnet").is_err());
        assert!(canonical_mining_preimage_bytes_v2(&v1, "pulsedag-testnet").is_err());

        let v2 = sample_header(BLOCK_HEADER_VERSION_V2, vec!["11".repeat(32)]);
        assert!(matches!(
            canonical_block_header_bytes_v2(&v2, ""),
            Err(PulseError::ChainIdMismatch)
        ));
        assert!(matches!(
            canonical_mining_preimage_bytes_v2(&v2, ""),
            Err(PulseError::ChainIdMismatch)
        ));

        let parentless = sample_header(BLOCK_HEADER_VERSION_V2, vec![]);
        assert!(canonical_block_header_bytes_v2(&parentless, "pulsedag-testnet").is_err());
        assert!(canonical_mining_preimage_bytes_v2(&parentless, "pulsedag-testnet").is_err());
    }
}
