use crate::{
    errors::PulseError,
    header_v2::canonical_mining_preimage_bytes_v2,
    pow::{
        canonical_pow_adapter, canonical_pow_engine, CanonicalPowAttempt, CanonicalPowEngine,
        CanonicalPowHash, CanonicalPowMaterial, PowAlgorithm, PowEngine, PowTargetComparison,
    },
    types::BlockHeader,
};

/// Non-activating PoW adapter for the frozen chain-bound block-header v2 domain.
///
/// This adapter deliberately reuses the canonical kHeavyHash engine and target
/// comparison semantics from the historical PoW implementation while sourcing
/// nonce-independent bytes exclusively from `canonical_mining_preimage_bytes_v2`.
/// It does not alter `PowHeaderPreimage`, `CanonicalPowAdapter`, node validation,
/// mining templates, or any live miner path.
#[derive(Debug, Clone, Copy)]
pub struct CanonicalPowV2Adapter {
    engine: CanonicalPowEngine,
}

impl Default for CanonicalPowV2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl CanonicalPowV2Adapter {
    pub fn new() -> Self {
        Self {
            engine: canonical_pow_engine(),
        }
    }

    pub fn algorithm(&self) -> PowAlgorithm {
        self.engine.algorithm()
    }

    pub fn algorithm_name(&self) -> &'static str {
        self.engine.algorithm_name()
    }

    pub fn engine_name(&self) -> &'static str {
        self.engine.engine_name()
    }

    /// Return chain-bound, nonce-independent canonical PoW material for a v2 header.
    pub fn pre_pow_material(
        &self,
        header: &BlockHeader,
        chain_id: &str,
    ) -> Result<CanonicalPowMaterial, PulseError> {
        let pre_pow_bytes = canonical_mining_preimage_bytes_v2(header, chain_id)?;
        Ok(CanonicalPowMaterial {
            pre_pow_bytes,
            sorted_parents: header.parents.clone(),
            header_nonce: header.nonce,
            target: canonical_pow_adapter().target_from_compact_bits(header.difficulty),
        })
    }

    /// Evaluate already-canonical v2 PoW material with an explicit nonce using
    /// exactly the same kHeavyHash and compact-target comparison as v1.
    pub fn evaluate_material_with_nonce(
        &self,
        material: &CanonicalPowMaterial,
        nonce: u64,
    ) -> CanonicalPowAttempt {
        let evaluation = self.engine.evaluate_pre_pow_bytes_with_nonce(
            &material.pre_pow_bytes,
            nonce,
            material.target.bits,
        );
        let comparison = if evaluation.accepted {
            PowTargetComparison::MeetsTarget
        } else {
            PowTargetComparison::AboveTarget
        };
        CanonicalPowAttempt {
            algorithm: evaluation.algorithm,
            material: material.clone(),
            final_hash: CanonicalPowHash {
                nonce,
                hash: evaluation.hash,
                hash_hex: evaluation.hash_hex,
                score_u64: evaluation.score_u64,
            },
            comparison,
        }
    }

    /// Evaluate the header's own nonce after v2 chain/header-shape validation.
    pub fn evaluate_header(
        &self,
        header: &BlockHeader,
        chain_id: &str,
    ) -> Result<CanonicalPowAttempt, PulseError> {
        let material = self.pre_pow_material(header, chain_id)?;
        Ok(self.evaluate_material_with_nonce(&material, header.nonce))
    }
}

pub fn canonical_pow_v2_adapter() -> CanonicalPowV2Adapter {
    CanonicalPowV2Adapter::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2};

    fn sample_header() -> BlockHeader {
        BlockHeader {
            version: BLOCK_HEADER_VERSION_V2,
            parents: vec!["11".repeat(32), "22".repeat(32)],
            timestamp: 1_700_000_000,
            difficulty: 0x1e00_ffff,
            nonce: 7,
            merkle_root: "33".repeat(32),
            state_root: "44".repeat(32),
            blue_score: 123,
            height: 456,
        }
    }

    #[test]
    fn v2_adapter_uses_chain_bound_pre_pow_material() {
        let header = sample_header();
        let adapter = canonical_pow_v2_adapter();
        let testnet = adapter
            .pre_pow_material(&header, "pulsedag-testnet")
            .unwrap();
        let private = adapter
            .pre_pow_material(&header, "pulsedag-private")
            .unwrap();

        assert_ne!(testnet.pre_pow_bytes, private.pre_pow_bytes);
        assert_eq!(testnet.target, private.target);
        assert_eq!(testnet.sorted_parents, header.parents);
    }

    #[test]
    fn v2_adapter_reuses_canonical_target_and_kheavyhash_semantics() {
        let header = sample_header();
        let v2 = canonical_pow_v2_adapter();
        let material = v2.pre_pow_material(&header, "pulsedag-testnet").unwrap();
        let attempt = v2.evaluate_material_with_nonce(&material, header.nonce);

        assert_eq!(v2.algorithm(), canonical_pow_adapter().algorithm());
        assert_eq!(
            v2.algorithm_name(),
            canonical_pow_adapter().algorithm_name()
        );
        assert_eq!(v2.engine_name(), canonical_pow_adapter().engine_name());
        assert_eq!(
            material.target,
            canonical_pow_adapter().target_from_compact_bits(header.difficulty)
        );
        assert_eq!(attempt.final_hash.nonce, header.nonce);
        assert_eq!(
            attempt.final_hash.hash_hex,
            hex::encode(attempt.final_hash.hash)
        );
    }

    #[test]
    fn nonce_is_finalized_outside_v2_pre_pow_material() {
        let header = sample_header();
        let adapter = canonical_pow_v2_adapter();
        let material = adapter
            .pre_pow_material(&header, "pulsedag-testnet")
            .unwrap();
        let first = adapter.evaluate_material_with_nonce(&material, 7);
        let second = adapter.evaluate_material_with_nonce(&material, 8);

        assert_eq!(first.material.pre_pow_bytes, second.material.pre_pow_bytes);
        assert_ne!(first.final_hash.hash, second.final_hash.hash);
    }

    #[test]
    fn different_chain_ids_produce_different_final_pow_hashes() {
        let header = sample_header();
        let adapter = canonical_pow_v2_adapter();
        let testnet = adapter
            .evaluate_header(&header, "pulsedag-testnet")
            .unwrap();
        let private = adapter
            .evaluate_header(&header, "pulsedag-private")
            .unwrap();

        assert_ne!(testnet.final_hash.hash, private.final_hash.hash);
    }

    #[test]
    fn wrong_header_version_and_empty_chain_fail_closed() {
        let adapter = canonical_pow_v2_adapter();
        let mut legacy = sample_header();
        legacy.version = BLOCK_HEADER_VERSION_V1;
        assert!(adapter
            .pre_pow_material(&legacy, "pulsedag-testnet")
            .is_err());

        let v2 = sample_header();
        assert!(adapter.pre_pow_material(&v2, "").is_err());
    }
}
