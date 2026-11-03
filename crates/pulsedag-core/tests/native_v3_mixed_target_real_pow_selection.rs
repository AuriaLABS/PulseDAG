//! DORMANT native-v3 changing-target parent-choice trial, NOT consensus.
//! Tests candidate PoW-weighted BLUE work, not network activation or money.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3, types::BlockId,
};

const CHAIN: &str = "native-v3-unapproved-mixed-target-real-pow";
const GENESIS_NS: u64 = 1_800_000_028_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff; // Independent Python target-work oracle: 2.
const HARD_BITS: u32 = 0x1f7f_ffff; // Independent Python target-work oracle: 512.

fn mine(
    mut parents: Vec<BlockId>,
    timestamp_ns: u64,
    height: u64,
    score: u64,
    bits: u32,
    tag: u8,
) -> (BlockId, BlockHeaderV3) {
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: bits,
        nonce: 0,
        merkle_root: format!("{tag:02x}").repeat(32),
        state_root: "33".repeat(32), // Monetary root deliberately unauthenticated.
        blue_score: score,
        height,
    };
    for nonce in 0..65_536_u64 {
        header.nonce = nonce;
        let evaluation = evaluate_v3_header_pow(&header, CHAIN).unwrap();
        if evaluation.pow.accepted {
            verify_v3_header_pow(&header, CHAIN).unwrap();
            return (evaluation.block_hash, header);
        }
    }
    panic!("test-only bounded real KHeavyHash search did not find a target");
}

#[test]
fn harder_shorter_branch_wins_over_easy_taller_branch_and_all_k_boundaries_match() {
    let g = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let mut headers = BTreeMap::from([(g.hash.clone(), g.header)]);
    let (easy1, e1) = mine(
        vec![g.hash.clone()],
        GENESIS_NS + 500_000_000,
        1,
        1,
        EASY_BITS,
        0x11,
    );
    headers.insert(easy1.clone(), e1);
    let (easy2, e2) = mine(
        vec![easy1.clone()],
        GENESIS_NS + 1_000_000_000,
        2,
        2,
        EASY_BITS,
        0x12,
    );
    headers.insert(easy2.clone(), e2);
    let (hard, h) = mine(
        vec![g.hash.clone()],
        GENESIS_NS + 600_000_000,
        1,
        1,
        HARD_BITS,
        0x13,
    );
    headers.insert(hard.clone(), h);
    assert_eq!(headers.len(), 4);

    // 0x1f7fffff has 256 times the work of 0x207fffff.
    // Two easy ancestors accumulate 2 + 2 = 4, not 512.
    for (trial_k, expected_score, expected_work, blue_count) in [
        (0_usize, 2_u64, "514", 0_usize),
        (1, 3, "516", 1),
        (2, 4, "518", 2),
    ] {
        let (join, header) = mine(
            vec![easy2.clone(), hard.clone()],
            GENESIS_NS + 1_500_000_000 + trial_k as u64 * 100_000,
            3,
            expected_score,
            EASY_BITS,
            0x21 + trial_k as u8,
        );
        headers.insert(join.clone(), header);
        let trial = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g.hash,
            std::slice::from_ref(&join),
            &headers,
            trial_k,
        )
        .expect("all actual mixed-target KHeavyHash ancestors must authenticate");
        assert_eq!(trial.blocks.len(), 5);
        assert_eq!(trial.blocks[&easy2].blue_work.decimal(), "4");
        assert_eq!(trial.blocks[&hard].blue_work.decimal(), "512");
        assert_eq!(trial.blocks[&join].selected_parent.as_deref(), Some(hard.as_str()));
        assert_eq!(trial.blocks[&join].blue_score, expected_score);
        assert_eq!(trial.blocks[&join].blue_work.decimal(), expected_work);
        assert_eq!(trial.blocks[&join].merge_blues.len(), blue_count);
        assert_eq!(trial.blocks[&join].merge_reds.len(), 2 - blue_count);
        assert_eq!(
            trial.blocks[&join].merge_blues,
            [easy1.clone(), easy2.clone()][..blue_count].to_vec()
        );
        assert_eq!(
            trial.blocks[&join].merge_reds,
            [easy1.clone(), easy2.clone()][blue_count..].to_vec()
        );

        // One missing, unselected ancestor invalidates the WHOLE trial even
        // when the selected heavy parent and valid join PoW are still present.
        let mut incomplete = headers.clone();
        incomplete.remove(&easy1);
        assert!(derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g.hash,
            std::slice::from_ref(&join),
            &incomplete,
            trial_k,
        )
        .is_err());
        headers.remove(&join);
    }
}

#[test]
fn a_pow_valid_join_with_wrong_committed_candidate_score_is_still_rejected() {
    let g = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let mut headers = BTreeMap::from([(g.hash.clone(), g.header)]);
    let (easy, e) = mine(
        vec![g.hash.clone()],
        GENESIS_NS + 500_000_000,
        1,
        1,
        EASY_BITS,
        0x31,
    );
    headers.insert(easy.clone(), e);
    let (hard, h) = mine(
        vec![g.hash.clone()],
        GENESIS_NS + 600_000_000,
        1,
        1,
        HARD_BITS,
        0x32,
    );
    headers.insert(hard.clone(), h);
    // For candidate K=0, expected blue_score=2, not 3.
    let (forged, f) = mine(
        vec![easy, hard],
        GENESIS_NS + 1_000_000_000,
        2,
        3,
        EASY_BITS,
        0x33,
    );
    headers.insert(forged.clone(), f);
    let err = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &g.hash,
        std::slice::from_ref(&forged),
        &headers,
        0,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("committed blue score differs"), "{err}");
}
