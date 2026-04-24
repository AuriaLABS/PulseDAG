use pulsedag_core::pow::{
    mine_header, pow_accepts, pow_hash_hex, pow_hash_score_u64, pow_preimage_bytes, pow_target_u64,
};
use pulsedag_core::types::BlockHeader;
use pulsedag_miner::{
    miner_pow_accepts, miner_pow_hash_hex, miner_pow_preimage_bytes, miner_pow_score_u64,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct VectorFixture {
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
fn official_vectors_pass_in_miner() {
    let fixture = load_fixture();

    for vector in fixture.valid_vectors {
        let preimage_hex = hex::encode(miner_pow_preimage_bytes(&vector.header));
        let pow_hash = miner_pow_hash_hex(&vector.header);
        let pow_score = miner_pow_score_u64(&vector.header);
        let target = pow_target_u64(vector.header.difficulty as u64);
        let accepts = miner_pow_accepts(&vector.header);

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
fn node_and_miner_cross_check_matches_exactly() {
    let fixture = load_fixture();

    for vector in fixture.valid_vectors {
        let node_preimage = pow_preimage_bytes(&vector.header);
        let miner_preimage = miner_pow_preimage_bytes(&vector.header);
        assert_eq!(
            node_preimage, miner_preimage,
            "{} preimage divergence",
            vector.id
        );

        assert_eq!(
            pow_hash_hex(&vector.header),
            miner_pow_hash_hex(&vector.header),
            "{} hash divergence",
            vector.id
        );
        assert_eq!(
            pow_hash_score_u64(&vector.header),
            miner_pow_score_u64(&vector.header),
            "{} score divergence",
            vector.id
        );
        assert_eq!(
            pow_accepts(&vector.header),
            miner_pow_accepts(&vector.header),
            "{} accepts divergence",
            vector.id
        );
    }
}

#[test]
fn node_and_miner_nonce_search_determinism_matches() {
    let fixture = load_fixture();

    for vector in fixture.valid_vectors {
        let max_tries = 8192;
        let (node_header, node_accepted, node_tries, node_hash) =
            mine_header(vector.header.clone(), max_tries);
        let miner_result = pulsedag_miner::mine_header_strided(vector.header.clone(), max_tries, 1)
            .expect("miner strided search must succeed");

        assert_eq!(
            miner_result.header.nonce, node_header.nonce,
            "{} nonce divergence",
            vector.id
        );
        assert_eq!(
            miner_result.accepted, node_accepted,
            "{} accepted divergence",
            vector.id
        );
        assert_eq!(
            miner_result.tries, node_tries,
            "{} tries divergence",
            vector.id
        );
        assert_eq!(
            miner_result.final_hash_hex, node_hash,
            "{} final hash divergence",
            vector.id
        );
    }
}

#[test]
fn invalid_vectors_fail_in_miner() {
    let fixture = load_fixture();

    for vector in fixture.invalid_vectors {
        let actual_preimage_hex = hex::encode(miner_pow_preimage_bytes(&vector.header));
        let actual_hash = miner_pow_hash_hex(&vector.header);
        let actual_score = miner_pow_score_u64(&vector.header);
        let actual_target = pow_target_u64(vector.header.difficulty as u64);
        let actual_accepts = miner_pow_accepts(&vector.header);

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
