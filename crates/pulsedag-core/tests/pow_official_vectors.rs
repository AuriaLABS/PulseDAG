use pulsedag_core::pow::{
    pow_accepts, pow_hash_hex, pow_hash_score_u64, pow_preimage_bytes, pow_target_u64,
};
use pulsedag_core::types::BlockHeader;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct VectorFixture {
    schema_version: u32,
    algorithm: String,
    valid_vectors: Vec<FixtureVector>,
    invalid_vectors: Vec<InvalidFixtureVector>,
}

#[derive(Debug, Deserialize)]
struct FixtureVector {
    id: String,
    header: BlockHeader,
    expected: ExpectedPow,
}

#[derive(Debug, Deserialize)]
struct InvalidFixtureVector {
    id: String,
    header: BlockHeader,
    expected: ExpectedPow,
    must_fail_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ExpectedPow {
    preimage_hex: String,
    pow_hash_hex: String,
    pow_score_u64: u64,
    target_u64: u64,
    accepts: bool,
}

fn load_fixture() -> VectorFixture {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_path = manifest_dir.join("../../fixtures/pow/official_vectors.json");
    let body = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture_path.display()));
    serde_json::from_str::<VectorFixture>(&body).expect("official pow fixture must parse")
}

#[test]
fn official_vectors_pass_in_core() {
    let fixture = load_fixture();
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
    let fixture = load_fixture();

    for vector in fixture.invalid_vectors {
        let actual_preimage_hex = hex::encode(pow_preimage_bytes(&vector.header));
        let actual_hash = pow_hash_hex(&vector.header);
        let actual_score = pow_hash_score_u64(&vector.header);
        let actual_target = pow_target_u64(vector.header.difficulty as u64);
        let actual_accepts = pow_accepts(&vector.header);

        for must_fail in &vector.must_fail_fields {
            let failed = match must_fail.as_str() {
                "preimage_hex" => actual_preimage_hex != vector.expected.preimage_hex,
                "pow_hash_hex" => actual_hash != vector.expected.pow_hash_hex,
                "pow_score_u64" => actual_score != vector.expected.pow_score_u64,
                "target_u64" => actual_target != vector.expected.target_u64,
                "accepts" => actual_accepts != vector.expected.accepts,
                other => panic!("{} contains unknown must_fail field: {other}", vector.id),
            };
            assert!(failed, "{} should fail on field {must_fail}", vector.id);
        }
    }
}
