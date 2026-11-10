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
                &preimage,
                nonce,
                0x207f_ffff,
            );
            assert_eq!(
                native.hash, expected,
                "upstream-vs-release digest mismatch for seed {index}, nonce {nonce}"
            );
            assert_eq!(
                native_prepared.hash_nonce(nonce),
                expected,
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
            preimage.as_bytes(),
            u64::MAX,
            0x207f_ffff,
        );
        assert_eq!(
            digest.accepted,
            compare_pow_hash_to_target(&digest.hash, &target)
        );
        assert_eq!(digest.hash_hex, hex::encode(digest.hash));
    }
}

/// Exact-head release-profile performance *gate* against the pinned original.
/// Compare both paths on the SAME CI runner with the SAME cached matrix. Any
/// large loss of CPU miner throughput blocks promotion of the new backend.
#[test]
#[ignore = "run explicitly in release profile on CI"]
fn prepared_native_mining_throughput_must_remain_close_to_upstream() {
    use std::{hint::black_box, time::Instant};

    let seed: [u8; 32] = Keccak256::digest(b"PulseDAG:prepared-miner-throughput:20261110").into();
    let native = PreparedKHeavyHasher::new(&seed);
    let upstream_prehash = KaspaHash::from_bytes(Keccak256::digest(seed).into());
    let upstream_matrix = Matrix::generate(upstream_prehash);
    let upstream = PowHash::new(upstream_prehash, 0);

    const COUNT: u64 = 200_000;
    let start = Instant::now();
    for nonce in 0..COUNT {
        black_box(upstream_matrix.heavy_hash(upstream.clone().finalize_with_nonce(black_box(nonce))));
    }
    let baseline = start.elapsed().as_secs_f64();

    let start = Instant::now();
    for nonce in 0..COUNT {
        black_box(native.hash_nonce(black_box(nonce)));
    }
    let alternative = start.elapsed().as_secs_f64();

    eprintln!(
        "prepared PoW throughput on same release runner: upstream={baseline:.4}s native={alternative:.4}s ratio={:.3}x",
        alternative / baseline
    );
    assert!(baseline > 0.0);
    assert!(
        alternative <= baseline * 1.50,
        "native prepared CPU mining throughput regressed more than 50%"
    );
}
