use pulsedag_core::pow::{compare_pow_hash_to_target, target_from_bits};
use pulsedag_core::types::BlockHeader;
use pulsedag_core::{
    ProtocolActivationIdentity, BLOCK_HEADER_VERSION_V1, BLOCK_HEADER_VERSION_V2,
    GHOSTDAG_V1_ORDERING_VERSION,
};
use pulsedag_miner::protocol_backend::{
    verify_backend_result_for_protocol, ProtocolNoncePartition,
};
use pulsedag_miner::protocol_pow::{build_protocol_pow_work, evaluate_mining_pow_nonce};

fn header(version: u32, target_bits: u32) -> BlockHeader {
    BlockHeader {
        version,
        parents: vec!["11".repeat(32)],
        timestamp: 1_700_000_000,
        difficulty: target_bits,
        nonce: 0,
        merkle_root: "33".repeat(32),
        state_root: "44".repeat(32),
        blue_score: 1,
        height: 2,
    }
}

fn legacy_identity() -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::legacy_default_for_chain("pulsedag-testnet", "55".repeat(32))
}

fn activated_identity() -> ProtocolActivationIdentity {
    ProtocolActivationIdentity::activated_v2(
        "pulsedag-testnet-v2",
        "55".repeat(32),
        GHOSTDAG_V1_ORDERING_VERSION,
    )
}

fn assert_nonce_edges(
    header: BlockHeader,
    target_bits: u32,
    identity: &ProtocolActivationIdentity,
) {
    let work = build_protocol_pow_work(&header, target_bits, Some(identity))
        .expect("protocol work must build");
    let frozen_material = work.material.clone();

    for nonce in [0, 1, u64::MAX - 1, u64::MAX] {
        let direct = work.evaluate_nonce(nonce);
        let via_public = evaluate_mining_pow_nonce(&header, target_bits, Some(identity), nonce)
            .expect("public protocol nonce evaluation");
        assert_eq!(direct, via_public);
        assert_eq!(direct.final_hash.nonce, nonce);
        assert_eq!(direct.material, frozen_material);
        assert_eq!(
            direct.comparison.accepted(),
            compare_pow_hash_to_target(&direct.final_hash.hash, &direct.material.target.target,)
        );

        let mut candidate = header.clone();
        candidate.nonce = nonce;
        let verification = verify_backend_result_for_protocol(&candidate, target_bits, identity)
            .expect("protocol backend verification");
        assert_eq!(verification.accepted, direct.comparison.accepted());
        assert_eq!(verification.final_hash_hex, direct.final_hash.hash_hex);
        assert_eq!(verification.target_hex, direct.material.target.target_hex);
        assert_eq!(
            work.reverify_accelerator_hash(nonce, direct.final_hash.hash)
                .expect("canonical accelerator re-verification"),
            direct.comparison.accepted()
        );
    }
}

#[test]
fn legacy_and_v2_protocol_paths_cover_full_u64_nonce_edges() {
    let bits = 0x207f_ffff;
    let legacy = legacy_identity();
    assert_nonce_edges(header(BLOCK_HEADER_VERSION_V1, bits), bits, &legacy);

    let activated = activated_identity();
    assert_nonce_edges(header(BLOCK_HEADER_VERSION_V2, bits), bits, &activated);
}

#[test]
fn protocol_target_extremes_match_canonical_target_rule() {
    for (version, identity) in [
        (BLOCK_HEADER_VERSION_V1, legacy_identity()),
        (BLOCK_HEADER_VERSION_V2, activated_identity()),
    ] {
        let max_bits = 1;
        let max_work =
            build_protocol_pow_work(&header(version, max_bits), max_bits, Some(&identity))
                .expect("max-target work");
        assert_eq!(max_work.material.target.target, [0xffu8; 32]);
        for nonce in [0, u64::MAX] {
            let attempt = max_work.evaluate_nonce(nonce);
            assert!(attempt.comparison.accepted());
            assert!(compare_pow_hash_to_target(
                &attempt.final_hash.hash,
                &max_work.material.target.target,
            ));
        }

        let zero_bits = 0x0100_0000;
        let zero_work =
            build_protocol_pow_work(&header(version, zero_bits), zero_bits, Some(&identity))
                .expect("zero-target work");
        assert_eq!(zero_work.material.target.target, [0u8; 32]);
        for nonce in [0, u64::MAX] {
            let attempt = zero_work.evaluate_nonce(nonce);
            assert_eq!(
                attempt.comparison.accepted(),
                compare_pow_hash_to_target(
                    &attempt.final_hash.hash,
                    &zero_work.material.target.target,
                )
            );
        }

        assert_eq!(
            zero_work.material.target.target,
            target_from_bits(zero_bits)
        );
    }
}

#[test]
fn zero_template_target_bits_fail_closed_before_nonce_evaluation() {
    let legacy = legacy_identity();
    let legacy_error =
        build_protocol_pow_work(&header(BLOCK_HEADER_VERSION_V1, 1), 0, Some(&legacy)).unwrap_err();
    assert!(legacy_error.to_string().contains("invalid target bits: 0"));

    let activated = activated_identity();
    let activated_error =
        build_protocol_pow_work(&header(BLOCK_HEADER_VERSION_V2, 1), 0, Some(&activated))
            .unwrap_err();
    assert!(activated_error
        .to_string()
        .contains("invalid target bits: 0"));
}

#[test]
fn nonce_partition_u64_edges_fail_closed_without_wrapping() {
    let addition_overflow =
        ProtocolNoncePartition::new(u64::MAX - 2, u64::MAX - 1, u64::MAX).unwrap();
    assert_eq!(addition_overflow.nonce_at(0).unwrap(), Some(u64::MAX - 2));
    assert!(addition_overflow.nonce_at(1).is_err());

    let multiplication_overflow = ProtocolNoncePartition::new(0, u64::MAX, u64::MAX).unwrap();
    assert_eq!(multiplication_overflow.nonce_at(0).unwrap(), Some(0));
    assert_eq!(multiplication_overflow.nonce_at(1).unwrap(), None);
    assert!(multiplication_overflow.nonce_at(2).is_err());
}
