use pulsedag_core::pow::{
    bits_from_target, canonical_pow_adapter, compare_pow_hash_to_target, target_from_bits,
    PowTargetComparison,
};
use pulsedag_core::types::BlockHeader;

fn sample_header(target_bits: u32) -> BlockHeader {
    BlockHeader {
        version: 1,
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

fn decrement_be(mut value: [u8; 32]) -> [u8; 32] {
    for byte in value.iter_mut().rev() {
        if *byte != 0 {
            *byte -= 1;
            return value;
        }
        *byte = 0xff;
    }
    panic!("cannot decrement an all-zero 256-bit value");
}

fn increment_be(mut value: [u8; 32]) -> [u8; 32] {
    for byte in value.iter_mut().rev() {
        if *byte != 0xff {
            *byte += 1;
            return value;
        }
        *byte = 0;
    }
    panic!("cannot increment an all-0xff 256-bit value");
}

#[test]
fn canonical_target_comparison_accepts_below_and_equal_and_rejects_above() {
    let bits = 0x1d00_ffff;
    let target = target_from_bits(bits);
    assert_ne!(target, [0u8; 32]);
    assert_ne!(target, [0xffu8; 32]);
    assert_eq!(bits_from_target(&target), bits);

    let below = decrement_be(target);
    let above = increment_be(target);
    let adapter = canonical_pow_adapter();

    assert!(compare_pow_hash_to_target(&below, &target));
    assert!(compare_pow_hash_to_target(&target, &target));
    assert!(!compare_pow_hash_to_target(&above, &target));

    assert_eq!(
        adapter.compare_hash_to_target_bits(&below, bits),
        PowTargetComparison::MeetsTarget
    );
    assert_eq!(
        adapter.compare_hash_to_target_bits(&target, bits),
        PowTargetComparison::MeetsTarget
    );
    assert_eq!(
        adapter.compare_hash_to_target_bits(&above, bits),
        PowTargetComparison::AboveTarget
    );
}

#[test]
fn compact_target_extremes_are_deterministic() {
    let adapter = canonical_pow_adapter();

    let zero = adapter.target_from_compact_bits(0x0100_0000);
    assert_eq!(zero.target, [0u8; 32]);
    assert!(zero.is_zero);

    // Legacy low-byte difficulty semantics normalize 0 to difficulty 1.
    assert_eq!(target_from_bits(0), [0xffu8; 32]);
    assert_eq!(target_from_bits(1), [0xffu8; 32]);

    let mut tiny_expected = [0u8; 32];
    tiny_expected[31] = 1;
    assert_eq!(target_from_bits(0x0101_0000), tiny_expected);

    let mut legacy_two_expected = [0xffu8; 32];
    legacy_two_expected[..8].copy_from_slice(&(u64::MAX / 2).to_be_bytes());
    assert_eq!(target_from_bits(2), legacy_two_expected);
}

#[test]
fn zero_and_max_targets_preserve_inclusive_boundary_rule() {
    let adapter = canonical_pow_adapter();
    let zero_bits = 0x0100_0000;
    let max_bits = 1;
    let zero_hash = [0u8; 32];
    let mut smallest_nonzero_hash = [0u8; 32];
    smallest_nonzero_hash[31] = 1;
    let max_hash = [0xffu8; 32];

    assert_eq!(target_from_bits(zero_bits), zero_hash);
    assert_eq!(
        adapter.compare_hash_to_target_bits(&zero_hash, zero_bits),
        PowTargetComparison::MeetsTarget
    );
    assert_eq!(
        adapter.compare_hash_to_target_bits(&smallest_nonzero_hash, zero_bits),
        PowTargetComparison::AboveTarget
    );

    assert_eq!(target_from_bits(max_bits), max_hash);
    for hash in [zero_hash, smallest_nonzero_hash, max_hash] {
        assert_eq!(
            adapter.compare_hash_to_target_bits(&hash, max_bits),
            PowTargetComparison::MeetsTarget
        );
    }
}

#[test]
fn nonce_u64_edges_are_deterministic_and_match_header_evaluation() {
    let bits = 0x207f_ffff;
    let header = sample_header(bits);
    let adapter = canonical_pow_adapter();
    let material = adapter
        .pre_pow_material(&header)
        .expect("canonical material");

    for nonce in [0, 1, u64::MAX - 1, u64::MAX] {
        let first = adapter.evaluate_material_with_nonce(&material, nonce);
        let second = adapter.evaluate_material_with_nonce(&material, nonce);
        assert_eq!(first, second, "nonce {nonce} must be deterministic");
        assert_eq!(first.final_hash.nonce, nonce);
        assert_eq!(first.material, material);
        assert_eq!(
            first.comparison.accepted(),
            compare_pow_hash_to_target(&first.final_hash.hash, &material.target.target)
        );

        let mut nonce_header = header.clone();
        nonce_header.nonce = nonce;
        let from_header = adapter
            .evaluate_header(&nonce_header)
            .expect("header boundary evaluation");
        assert_eq!(
            &from_header.material.pre_pow_bytes, &material.pre_pow_bytes,
            "canonical pre-PoW bytes must remain nonce-independent"
        );
        assert_eq!(
            &from_header.material.sorted_parents, &material.sorted_parents,
            "parent ordering must remain nonce-independent"
        );
        assert_eq!(
            &from_header.material.target, &material.target,
            "target material must remain nonce-independent"
        );
        assert_eq!(from_header.material.header_nonce, nonce);
        assert_eq!(from_header.final_hash, first.final_hash);
        assert_eq!(from_header.comparison, first.comparison);
    }
}
