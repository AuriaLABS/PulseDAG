use pulsedag_core::pow::{
    pow_accepts, pow_hash_hex, pow_preimage_bytes, target_from_bits, verify_work,
};
use pulsedag_core::types::BlockHeader;
use serde::Deserialize;
use sha3::{Digest, Keccak256};

#[derive(Debug, Deserialize)]
struct Fixture {
    vectors: Vec<Vector>,
}

#[derive(Debug, Deserialize)]
struct Vector {
    header: BlockHeader,
    pre_pow_hash: String,
    pow_hash: String,
    bits: u32,
    target_hex: String,
    expected_passed: bool,
}

#[test]
fn official_v2_2_10_vectors_are_stable() {
    let body = std::fs::read_to_string("../../fixtures/pow/v2_2_10_official_vectors.json").unwrap();
    let fixture: Fixture = serde_json::from_str(&body).unwrap();
    assert!(!fixture.vectors.is_empty());

    for v in fixture.vectors {
        let pre_pow_bytes = pow_preimage_bytes(&v.header);
        assert_eq!(
            hex::encode(Keccak256::digest(&pre_pow_bytes)),
            v.pre_pow_hash
        );
        assert_eq!(pow_hash_hex(&v.header), v.pow_hash);
        assert_eq!(hex::encode(target_from_bits(v.bits)), v.target_hex);
        assert_eq!(pow_accepts(&v.header), v.expected_passed);
        assert_eq!(verify_work(&v.header), v.expected_passed);
        assert!(!pre_pow_bytes.is_empty());
    }
}
