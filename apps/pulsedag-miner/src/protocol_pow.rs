use anyhow::{anyhow, Result};
use pulsedag_core::pow::{canonical_pow_adapter, CanonicalPowAttempt, CanonicalPowMaterial};
use pulsedag_core::types::{compute_block_hash, BlockHeader};
use pulsedag_core::{
    canonical_pow_v2_adapter, compute_block_hash_v2, resolve_pow_identity_path, PowValidationPath,
    ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V1,
};

fn protocol_error(message: impl Into<String>) -> anyhow::Error {
    anyhow!("protocol-bound miner PoW: {}", message.into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolPowWork {
    pub path: PowValidationPath,
    pub material: CanonicalPowMaterial,
}

impl ProtocolPowWork {
    pub fn evaluate_nonce(&self, nonce: u64) -> CanonicalPowAttempt {
        match self.path {
            PowValidationPath::LegacyV1 => {
                canonical_pow_adapter().evaluate_material_with_nonce(&self.material, nonce)
            }
            PowValidationPath::ActivatedV2 => {
                canonical_pow_v2_adapter().evaluate_material_with_nonce(&self.material, nonce)
            }
        }
    }
}

/// Resolve the exact PoW domain an external miner must use for one header.
///
/// Historical templates do not carry a protocol identity, so absence remains
/// an explicit legacy-v1 compatibility path. A v2 header without an identity
/// fails closed instead of silently falling back to v1 hashing.
pub fn resolve_mining_pow_path(
    header: &BlockHeader,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<PowValidationPath> {
    let path = match identity {
        Some(identity) => resolve_pow_identity_path(identity)
            .map_err(|error| protocol_error(error.to_string()))?,
        None => PowValidationPath::LegacyV1,
    };

    let required_header_version = identity
        .map(|identity| identity.block_header_protocol_version)
        .unwrap_or(BLOCK_HEADER_VERSION_V1);
    if header.version != required_header_version {
        return Err(protocol_error(format!(
            "PoW path {} requires block header version {}, got {}",
            path.as_str(),
            required_header_version,
            header.version
        )));
    }

    Ok(path)
}

/// Freeze the nonce-independent canonical work unit at the target supplied by
/// the mining template. CPU workers and future GPU kernels must consume this
/// exact path/material pair and vary only the nonce.
pub fn build_protocol_pow_work(
    header: &BlockHeader,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<ProtocolPowWork> {
    if target_bits == 0 {
        return Err(protocol_error("invalid target bits: 0"));
    }

    let path = resolve_mining_pow_path(header, identity)?;
    let mut canonical_header = header.clone();
    canonical_header.difficulty = target_bits;

    let material = match path {
        PowValidationPath::LegacyV1 => canonical_pow_adapter()
            .pre_pow_material(&canonical_header)
            .map_err(|reason| {
                protocol_error(format!("invalid v1 PoW material: {}", reason.code()))
            })?,
        PowValidationPath::ActivatedV2 => {
            let identity = identity.ok_or_else(|| {
                protocol_error("activated-v2 PoW requires an explicit protocol identity")
            })?;
            canonical_pow_v2_adapter()
                .pre_pow_material(&canonical_header, &identity.chain_id)
                .map_err(|error| protocol_error(error.to_string()))?
        }
    };

    Ok(ProtocolPowWork { path, material })
}

pub fn canonical_mining_pow_material(
    header: &BlockHeader,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<CanonicalPowMaterial> {
    Ok(build_protocol_pow_work(header, target_bits, identity)?.material)
}

/// Evaluate one nonce through a freshly frozen canonical work unit.
/// Hot nonce-search loops should build the work once and call
/// [`ProtocolPowWork::evaluate_nonce`] directly.
pub fn evaluate_mining_pow_nonce(
    header: &BlockHeader,
    target_bits: u32,
    identity: Option<&ProtocolActivationIdentity>,
    nonce: u64,
) -> Result<CanonicalPowAttempt> {
    Ok(build_protocol_pow_work(header, target_bits, identity)?.evaluate_nonce(nonce))
}

/// Compute the final block hash after a backend has selected a nonce. This keeps
/// the submit payload in the same protocol domain used during nonce search.
pub fn compute_mined_block_hash(
    header: &BlockHeader,
    identity: Option<&ProtocolActivationIdentity>,
) -> Result<String> {
    match resolve_mining_pow_path(header, identity)? {
        PowValidationPath::LegacyV1 => Ok(compute_block_hash(header)),
        PowValidationPath::ActivatedV2 => {
            let identity = identity.ok_or_else(|| {
                protocol_error("activated-v2 block hashing requires an explicit protocol identity")
            })?;
            compute_block_hash_v2(header, &identity.chain_id)
                .map_err(|error| protocol_error(error.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulsedag_core::{
        ProtocolConsensusMode, BLOCK_HEADER_VERSION_V2, GHOSTDAG_V1_ORDERING_VERSION,
    };

    fn header(version: u32) -> BlockHeader {
        BlockHeader {
            version,
            parents: vec!["11".repeat(32), "22".repeat(32)],
            timestamp: 1_700_000_000,
            difficulty: 0x207f_ffff,
            nonce: 7,
            merkle_root: "33".repeat(32),
            state_root: "44".repeat(32),
            blue_score: 12,
            height: 13,
        }
    }

    fn activated_identity(chain_id: &str) -> ProtocolActivationIdentity {
        ProtocolActivationIdentity::activated_v2(
            chain_id,
            "55".repeat(32),
            GHOSTDAG_V1_ORDERING_VERSION,
        )
    }

    #[test]
    fn identity_absence_preserves_only_legacy_v1() {
        assert_eq!(
            resolve_mining_pow_path(&header(BLOCK_HEADER_VERSION_V1), None).unwrap(),
            PowValidationPath::LegacyV1
        );
        assert!(resolve_mining_pow_path(&header(BLOCK_HEADER_VERSION_V2), None).is_err());
    }

    #[test]
    fn activated_v2_work_is_chain_bound() {
        let header = header(BLOCK_HEADER_VERSION_V2);
        let testnet = activated_identity("pulsedag-testnet-v2");
        let private = activated_identity("pulsedag-private-v2");

        let testnet_work =
            build_protocol_pow_work(&header, header.difficulty, Some(&testnet)).unwrap();
        let private_work =
            build_protocol_pow_work(&header, header.difficulty, Some(&private)).unwrap();

        assert_eq!(testnet_work.path, PowValidationPath::ActivatedV2);
        assert_ne!(
            testnet_work.material.pre_pow_bytes,
            private_work.material.pre_pow_bytes
        );
        assert_eq!(testnet_work.material.target, private_work.material.target);
    }

    #[test]
    fn frozen_work_varies_only_nonce() {
        let header = header(BLOCK_HEADER_VERSION_V2);
        let identity = activated_identity("pulsedag-testnet-v2");
        let work = build_protocol_pow_work(&header, header.difficulty, Some(&identity)).unwrap();
        let first = work.evaluate_nonce(7);
        let second = work.evaluate_nonce(8);

        assert_eq!(first.material.pre_pow_bytes, second.material.pre_pow_bytes);
        assert_eq!(first.material.target, second.material.target);
        assert_ne!(first.final_hash.hash, second.final_hash.hash);
    }

    #[test]
    fn nonce_evaluation_matches_core_v2_adapter() {
        let header = header(BLOCK_HEADER_VERSION_V2);
        let identity = activated_identity("pulsedag-testnet-v2");
        let evaluated =
            evaluate_mining_pow_nonce(&header, header.difficulty, Some(&identity), 99).unwrap();
        let material = canonical_pow_v2_adapter()
            .pre_pow_material(&header, &identity.chain_id)
            .unwrap();
        let expected = canonical_pow_v2_adapter().evaluate_material_with_nonce(&material, 99);

        assert_eq!(evaluated.final_hash, expected.final_hash);
        assert_eq!(evaluated.comparison, expected.comparison);
    }

    #[test]
    fn v2_block_hash_is_chain_bound() {
        let header = header(BLOCK_HEADER_VERSION_V2);
        let testnet = activated_identity("pulsedag-testnet-v2");
        let private = activated_identity("pulsedag-private-v2");

        assert_ne!(
            compute_mined_block_hash(&header, Some(&testnet)).unwrap(),
            compute_mined_block_hash(&header, Some(&private)).unwrap()
        );
    }

    #[test]
    fn mixed_protocol_identity_fails_closed() {
        let header = header(BLOCK_HEADER_VERSION_V2);
        let mut identity = activated_identity("pulsedag-testnet-v2");
        identity.consensus_mode = ProtocolConsensusMode::Legacy;

        assert!(resolve_mining_pow_path(&header, Some(&identity)).is_err());
    }

    #[test]
    fn zero_target_is_rejected_before_material_creation() {
        let header = header(BLOCK_HEADER_VERSION_V1);
        let error = build_protocol_pow_work(&header, 0, None).unwrap_err();
        assert!(error.to_string().contains("invalid target bits"));
    }
}
