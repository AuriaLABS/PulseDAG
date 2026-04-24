use pulsedag_core::pow::{
    pow_accepts, pow_hash_hex, pow_hash_score_u64, pow_preimage_bytes, pow_target_u64,
};

#[path = "../../../tests/support/pow_fixture.rs"]
mod pow_fixture;

#[test]
fn official_vectors_pass_in_core() {
    let fixture = pow_fixture::load_official_fixture(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.algorithm, "kHeavyHash");

    for vector in fixture.valid_vectors {
        let preimage = pow_preimage_bytes(&vector.header);
        let preimage_hex = hex::encode(&preimage);
        let pow_hash = pow_hash_hex(&vector.header);
        let pow_score = pow_hash_score_u64(&vector.header);
        let target = pow_target_u64(vector.header.difficulty as u64);
        let accepts = pow_accepts(&vector.header);

        assert_eq!(
            preimage_hex, vector.expected.preimage_hex,
            "{} preimage mismatch",
            vector.id
        );
        assert_eq!(
            pow_hash, vector.expected.pow_hash_hex,
            "{} hash mismatch",
            vector.id
        );
        assert_eq!(
            pow_score, vector.expected.pow_score_u64,
            "{} score mismatch",
            vector.id
        );
        assert_eq!(
            target, vector.expected.target_u64,
            "{} target mismatch",
            vector.id
        );
        assert_eq!(
            accepts, vector.expected.accepts,
            "{} acceptance mismatch",
            vector.id
        );
    }
}

#[test]
fn invalid_vectors_fail_in_core() {
    let fixture = pow_fixture::load_official_fixture(env!("CARGO_MANIFEST_DIR"));

    for vector in fixture.invalid_vectors {
        let actual_preimage_hex = hex::encode(pow_preimage_bytes(&vector.header));
        let actual_hash = pow_hash_hex(&vector.header);
        let actual_score = pow_hash_score_u64(&vector.header);
        let actual_target = pow_target_u64(vector.header.difficulty as u64);
        let actual_accepts = pow_accepts(&vector.header);

        assert!(
            !vector.must_fail_fields.is_empty(),
            "{} should include at least one must_fail field",
            vector.id
        );

        for must_fail in &vector.must_fail_fields {
            let failed = match pow_fixture::PowVectorField::parse(must_fail) {
                Some(pow_fixture::PowVectorField::PreimageHex) => {
                    actual_preimage_hex != vector.expected.preimage_hex
                }
                Some(pow_fixture::PowVectorField::PowHashHex) => {
                    actual_hash != vector.expected.pow_hash_hex
                }
                Some(pow_fixture::PowVectorField::PowScoreU64) => {
                    actual_score != vector.expected.pow_score_u64
                }
                Some(pow_fixture::PowVectorField::TargetU64) => {
                    actual_target != vector.expected.target_u64
                }
                Some(pow_fixture::PowVectorField::Accepts) => {
                    actual_accepts != vector.expected.accepts
                }
                None => panic!(
                    "{} contains unknown must_fail field: {must_fail}",
                    vector.id
                ),
            };
            assert!(failed, "{} should fail on field {must_fail}", vector.id);
        }
    }
}
