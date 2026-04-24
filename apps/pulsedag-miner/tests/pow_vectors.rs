use pulsedag_core::pow::{
    mine_header, pow_accepts, pow_hash_hex, pow_hash_score_u64, pow_preimage_bytes, pow_target_u64,
};
use pulsedag_miner::{
    miner_pow_accepts, miner_pow_hash_hex, miner_pow_preimage_bytes, miner_pow_score_u64,
};

#[path = "../../../tests/support/pow_fixture.rs"]
mod pow_fixture;

#[test]
fn official_vectors_pass_in_miner() {
    let fixture = pow_fixture::load_official_fixture(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.algorithm, "kHeavyHash");

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
    let fixture = pow_fixture::load_official_fixture(env!("CARGO_MANIFEST_DIR"));

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
    let fixture = pow_fixture::load_official_fixture(env!("CARGO_MANIFEST_DIR"));

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
    let fixture = pow_fixture::load_official_fixture(env!("CARGO_MANIFEST_DIR"));

    for vector in fixture.invalid_vectors {
        let actual_preimage_hex = hex::encode(miner_pow_preimage_bytes(&vector.header));
        let actual_hash = miner_pow_hash_hex(&vector.header);
        let actual_score = miner_pow_score_u64(&vector.header);
        let actual_target = pow_target_u64(vector.header.difficulty as u64);
        let actual_accepts = miner_pow_accepts(&vector.header);

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
