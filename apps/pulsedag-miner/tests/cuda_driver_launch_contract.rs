#![cfg(feature = "cuda")]

use pulsedag_miner::cuda_driver_launch;

use pulsedag_core::types::BlockHeader;
use pulsedag_core::{
    ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2,
    GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_miner::protocol_pow::{build_protocol_pow_work, ProtocolPowWork};
use sha3::{Digest, Keccak256};

fn header(version: u32, target_bits: u32) -> BlockHeader {
    BlockHeader {
        version,
        parents: vec!["11".repeat(32), "22".repeat(32)],
        timestamp: 1_700_000_000,
        difficulty: target_bits,
        nonce: 0,
        merkle_root: "33".repeat(32),
        state_root: "44".repeat(32),
        blue_score: 12,
        height: 13,
    }
}

fn activated_identity() -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        "pulsedag-testnet-v2",
        "55".repeat(32),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn canonical_pre_pow_hash(work: &ProtocolPowWork) -> [u8; 32] {
    let digest = Keccak256::digest(&work.material.pre_pow_bytes);
    let mut pre_pow_hash = [0u8; 32];
    pre_pow_hash.copy_from_slice(&digest);
    pre_pow_hash
}

fn assert_launch_matches(work: &ProtocolPowWork) {
    let nonces = [0u64, 1, 7, 42, 255, 1_024];
    let hashes = cuda_driver_launch::launch_kheavyhash_batch(
        b"mock-module-image",
        0,
        canonical_pre_pow_hash(work),
        &nonces,
        64,
    )
    .expect("mock CUDA Driver launch must succeed");

    assert_eq!(hashes.len(), nonces.len());
    for (&nonce, &hash) in nonces.iter().zip(&hashes) {
        let canonical = work.evaluate_nonce(nonce);
        assert_eq!(hash, canonical.final_hash.hash);
        assert_eq!(
            work.reverify_accelerator_hash(nonce, hash).unwrap(),
            canonical.comparison.accepted()
        );
    }
}

#[test]
fn cuda_driver_launch_matches_legacy_v1_canonical_pow() {
    let target_bits = 0x207f_ffff;
    let header = header(BLOCK_HEADER_VERSION_V1, target_bits);
    let work = build_protocol_pow_work(&header, target_bits, None).unwrap();
    assert_launch_matches(&work);
}

#[test]
fn cuda_driver_launch_matches_activated_v2_canonical_pow() {
    let target_bits = 0x207f_ffff;
    let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
    let identity = activated_identity();
    let work = build_protocol_pow_work(&header, target_bits, Some(&identity)).unwrap();
    assert_launch_matches(&work);
}

#[test]
fn wrong_cuda_pre_pow_hash_fails_closed_at_canonical_reverification() {
    let target_bits = 0x207f_ffff;
    let header = header(BLOCK_HEADER_VERSION_V2, target_bits);
    let identity = activated_identity();
    let work = build_protocol_pow_work(&header, target_bits, Some(&identity)).unwrap();
    let nonce = 7u64;
    let mut wrong_pre_pow_hash = canonical_pre_pow_hash(&work);
    wrong_pre_pow_hash[0] ^= 1;

    let hash = cuda_driver_launch::launch_kheavyhash_batch(
        b"mock-module-image",
        0,
        wrong_pre_pow_hash,
        &[nonce],
        64,
    )
    .unwrap()
    .remove(0);

    let error = work.reverify_accelerator_hash(nonce, hash).unwrap_err();
    assert!(error.to_string().contains("accelerator hash mismatch"));
}
