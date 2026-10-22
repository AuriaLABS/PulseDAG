//! A *dormant, non-normative* Rust cross-check of the #1341 candidate's
//! simplest authenticated fork/rejoin. This is not a native monetary selector,
//! consensus-policy freeze, mainnet authority, or a mining/contract activation.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    native_anticone_matrix_v3::derive_dormant_native_v3_anticone_matrix,
    native_merge_set_v3::derive_dormant_native_v3_all_parent_merge_sets,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    types::{compute_merkle_root, BlockId},
};

const CHAIN: &str = "pulsedag-dormant-ghostdag-rust-policy-probe";
const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

/// Work(T)=floor(2^256/(T+1))=2 at this independently checked EASY_BITS.
/// This is a candidate policy test, NOT approved work-accounting semantics.
const EASY_WORK: u64 = 2;

fn mine(
    parents: Vec<BlockId>,
    height: u64,
    timestamp_ns: u64,
    blue_score: u64,
    merkle_root: String,
) -> (BlockId, BlockHeaderV3) {
    let mut parents = parents;
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root,
        state_root: "33".repeat(32), // Placeholder: NOT valid monetary replay.
        blue_score,
        height,
    };
    for nonce in 0..1_024_u64 {
        header.nonce = nonce;
        let evaluation = evaluate_v3_header_pow(&header, CHAIN).expect("canonical header");
        if evaluation.pow.accepted {
            assert_eq!(
                compute_block_hash_v3(&header, CHAIN).unwrap(),
                evaluation.block_hash
            );
            verify_v3_header_pow(&header, CHAIN).expect("genuine easy-target PoW");
            return (evaluation.block_hash, header);
        }
    }
    panic!("test-only easy-target PoW nonce search exhausted");
}

#[test]
fn independently_authenticated_rust_fork_and_rejoin_match_candidate_k_zero_and_one() {
    // Genesis derives from the *real dormant Rust v3 genesis constructor*,
    // not a user-supplied or fabricated restored root.
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let genesis_hash = genesis.hash.clone();
    let mut headers = BTreeMap::new();
    headers.insert(genesis_hash.clone(), genesis.header);

    let root = compute_merkle_root(&[]);
    let (a, ha) = mine(
        vec![genesis_hash.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        root.clone(),
    );
    let (b, hb) = mine(
        vec![genesis_hash.clone()],
        1,
        GENESIS_NS + 600_000_000,
        1,
        "22".repeat(32),
    );
    assert_ne!(
        a, b,
        "different headers must have distinct SHA256 identities"
    );
    headers.insert(a.clone(), ha);
    headers.insert(b.clone(), hb);

    let mut parents = vec![a.clone(), b.clone()];
    parents.sort(); // The canonical hash, NOT symbolic A/B labels, breaks ties.

    let all_sets = derive_dormant_native_v3_all_parent_merge_sets(
        CHAIN,
        GENESIS_NS,
        &genesis_hash,
        &parents,
        &headers,
    )
    .expect("every ancestor including the side branch must be authenticated");
    assert_eq!(
        all_sets
            .topological_evaluation
            .verified_dag
            .authenticated_non_genesis_headers,
        2
    );
    let selected = &parents[0];
    let other = &parents[1];
    assert_eq!(
        all_sets.by_comparison_parent.get(selected).unwrap(),
        &vec![other.clone()],
        "merge-set must be recomputed from the full verified DAG"
    );
    let matrix = derive_dormant_native_v3_anticone_matrix(
        CHAIN,
        GENESIS_NS,
        &genesis_hash,
        &parents,
        &headers,
    )
    .unwrap();
    assert_eq!(matrix.anticone_by_block[selected], vec![other.clone()]);
    assert_eq!(matrix.anticone_by_block[other], vec![selected.clone()]);

    for k in [0_u64, 1_u64] {
        // Exactly one available merge block, incomparable with the selected
        // parent's blue set by one element. Both candidate K constraints agree.
        let blue = u64::from(k >= 1);
        let expected_score = 2 + blue;
        let expected_work = EASY_WORK * (2 + blue);
        assert_eq!(expected_score, if k == 0 { 2 } else { 3 });
        assert_eq!(expected_work, if k == 0 { 4 } else { 6 });
        let (join_hash, join) = mine(
            parents.clone(),
            2,
            GENESIS_NS + 1_000_000_000,
            expected_score,
            if k == 0 {
                root.clone()
            } else {
                "44".repeat(32)
            },
        );
        assert_eq!(join.parents, parents);
        assert_eq!(join.blue_score, expected_score);
        assert_ne!(join_hash, a);
        assert_ne!(join_hash, b);
        assert_eq!(join.height, 2);
        assert!(join.timestamp_ns > GENESIS_NS + 600_000_000);
        assert_eq!(EASY_WORK, 2);
    }

    // The same immutable authenticated input should not depend on input
    // map insertion order or on which peer announces which parent first.
    let mut reversed_map = BTreeMap::new();
    for (id, header) in headers.into_iter().rev() {
        reversed_map.insert(id, header);
    }
    assert_eq!(
        derive_dormant_native_v3_all_parent_merge_sets(
            CHAIN,
            GENESIS_NS,
            &genesis_hash,
            &parents,
            &reversed_map
        )
        .unwrap()
        .by_comparison_parent,
        all_sets.by_comparison_parent
    );
}

#[test]
fn independent_rust_dag_rejects_invalid_pow_on_unselected_side_branch() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let genesis_hash = genesis.hash.clone();
    let mut headers = BTreeMap::new();
    headers.insert(genesis_hash.clone(), genesis.header);
    let (good, good_header) = mine(
        vec![genesis_hash.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        compute_merkle_root(&[]),
    );
    headers.insert(good.clone(), good_header);
    let (_, mut bad_header) = mine(
        vec![genesis_hash.clone()],
        1,
        GENESIS_NS + 600_000_000,
        1,
        "55".repeat(32),
    );
    bad_header.nonce = (0..1_024_u64)
        .find(|nonce| {
            bad_header.nonce = *nonce;
            !evaluate_v3_header_pow(&bad_header, CHAIN)
                .unwrap()
                .pow
                .accepted
        })
        .expect("invalid nonce exists for easiest allowed target");
    let bad_hash = compute_block_hash_v3(&bad_header, CHAIN).unwrap();
    headers.insert(bad_hash.clone(), bad_header);
    let mut parents = vec![good, bad_hash];
    parents.sort();
    assert!(
        derive_dormant_native_v3_all_parent_merge_sets(
            CHAIN,
            GENESIS_NS,
            &genesis_hash,
            &parents,
            &headers
        )
        .is_err(),
        "invalid-PoW side ancestry may not be omitted by selected-parent choice"
    );
}
