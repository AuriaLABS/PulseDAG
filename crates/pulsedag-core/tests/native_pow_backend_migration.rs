//! Byte parity between PulseDAG's new canonical native KHeavyHash backend
//! and the pinned upstream Kaspa implementation retained as TEST-only reference.
//! This does not establish GD-08 policy, full consensus correctness or testnet GO.

use kaspa_hashes::{Hash as KaspaHash, PowHash};
use kaspa_pow::matrix::Matrix;
use pulsedag_core::pow::{canonical_pow_engine, PowEngine, PreparedKHeavyHasher};
use sha3::{Digest, Keccak256};

#[test]
fn native_release_hash_matches_pinned_upstream_for_96_seeds_and_nonce_edges() {
    for index in 0..96_u64 {
        let preimage: [u8; 32] = Keccak256::digest(
            format!("PulseDAG:independent-release-pow-parity:20261110:{index}").as_bytes(),
        )
        .into();
        let pre_pow_hash = KaspaHash::from_bytes(Keccak256::digest(preimage).into());
        let upstream_matrix = Matrix::generate(pre_pow_hash);
        let upstream_hasher = PowHash::new(pre_pow_hash, 0);
        let native_prepared = PreparedKHeavyHasher::new(&preimage);

        for nonce in [0_u64, 1, 17, 0xabcdef01_23456789, u64::MAX] {
            let expected = upstream_matrix
                .heavy_hash(upstream_hasher.clone().finalize_with_nonce(nonce))
                .as_bytes();
            let native = canonical_pow_engine().evaluate_pre_pow_bytes_with_nonce(
                &preimage, nonce, 0x207f_ffff,
            );
            assert_eq!(
                native.hash, expected,
                "upstream-vs-release digest mismatch for seed {index}, nonce {nonce}"
            );
            assert_eq!(
                native_prepared.hash_nonce(nonce), expected,
                "prepared miner digest mismatch for seed {index}, nonce {nonce}"
            );
        }
    }
}

#[test]
fn native_consensus_hash_retains_chain_bound_separation_and_exact_target() {
    use pulsedag_core::pow::{compare_pow_hash_to_target, target_from_bits};
    let target = target_from_bits(0x207f_ffff);
    for chain in ["native-v3-testnet-a", "native-v3-testnet-b"] {
        let preimage = format!("PulseDAG:pow:v3:{chain}:forked-header");
        let digest = canonical_pow_engine().evaluate_pre_pow_bytes_with_nonce(
            preimage.as_bytes(), u64::MAX, 0x207f_ffff,
        );
        assert_eq!(digest.accepted, compare_pow_hash_to_target(&digest.hash, &target));
        assert_eq!(digest.hash_hex, hex::encode(digest.hash));
    }
}
