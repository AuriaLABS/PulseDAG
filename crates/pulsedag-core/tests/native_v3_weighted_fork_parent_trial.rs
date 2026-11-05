//! TEST ONLY. Real KHeavyHash native-v3 siblings with deliberately unequal
//! compact targets (candidate work 2 versus 512) and a rejoin. This verifies
//! the *unapproved* #1309 weighted-parent and inclusive blue-work proposal,
//! not a frozen selected-parent rule, monetary consensus or live mining.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    types::{compute_merkle_root, BlockId},
};

const CHAIN: &str = "TEST-ONLY-v3-weighted-fork-not-production-consensus";
const GENESIS_NS: u64 = 1_800_000_099_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff; // Proposed floor(2^256 / (T+1)) = 2.
const HARD_BITS: u32 = 0x1f7f_ffff; // Proposed floor(2^256 / (T+1)) = 512.

fn mine(
    mut parents: Vec<BlockId>,
    height: u64,
    timestamp_ns: u64,
    difficulty: u32,
    score: u64,
    marker: u8,
) -> (BlockId, BlockHeaderV3) {
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty,
        nonce: 0,
        merkle_root: if marker == 0 {
            compute_merkle_root(&[])
        } else {
            format!("{marker:02x}").repeat(32)
        },
        state_root: "33".repeat(32), // Intentionally not an authenticated monetary root.
        blue_score: score,
        height,
    };
    for nonce in 0..65_536 {
        header.nonce = nonce;
        let evaluated = evaluate_v3_header_pow(&header, CHAIN).unwrap();
        if evaluated.pow.accepted {
            assert_eq!(
                compute_block_hash_v3(&header, CHAIN).unwrap(),
                evaluated.block_hash
            );
            verify_v3_header_pow(&header, CHAIN).unwrap();
            return (evaluated.block_hash, header);
        }
    }
    panic!("PoW-valid bounded, fixed test header not found");
}

#[test]
fn real_pow_unequal_siblings_select_heavier_parent_and_charge_merge_once() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let g = genesis.hash;
    let mut headers = BTreeMap::from([(g.clone(), genesis.header)]);
    let (a, easy) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        EASY_BITS,
        1,
        0,
    );
    let (b, hard) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 600_000_000,
        HARD_BITS,
        1,
        1,
    );
    assert_ne!(a, b);
    headers.insert(a.clone(), easy);
    headers.insert(b.clone(), hard);

    // The per-target work is independently checked by the existing Python
    // integer oracle scripts/native_v3_work_policy_candidates.py; these
    // values deliberately differ despite equal genesis-relative blue scores.
    for (k, expected_score, expected_work) in [(0_usize, 2_u64, "514"), (1_usize, 3_u64, "516")] {
        let (joined, header) = mine(
            vec![a.clone(), b.clone()],
            2,
            GENESIS_NS + 1_000_000_000,
            EASY_BITS,
            expected_score,
            10 + k as u8,
        );
        let mut complete = headers.clone();
        complete.insert(joined.clone(), header);
        let trial = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            &[joined.clone()],
            &complete,
            k,
        )
        .unwrap();
        assert_eq!(trial.blocks[&a].blue_work.decimal(), "2");
        assert_eq!(trial.blocks[&b].blue_work.decimal(), "512");
        let choice = &trial.blocks[&joined];
        assert_eq!(choice.selected_parent.as_deref(), Some(b.as_str()));
        assert_eq!(choice.blue_score, expected_score);
        assert_eq!(choice.blue_work.decimal(), expected_work);
        if k == 0 {
            assert!(choice.merge_blues.is_empty());
            assert_eq!(choice.merge_reds, vec![a.clone()]);
        } else {
            assert_eq!(choice.merge_blues, vec![a.clone()]);
            assert!(choice.merge_reds.is_empty());
        }

        // A new PoW-valid header with an invented committed blue_score must
        // NOT become authority just by recomputing SHA256/KHeavyHash.
        let (forged, wrong_score_header) = mine(
            vec![a.clone(), b.clone()],
            2,
            GENESIS_NS + 1_200_000_000,
            EASY_BITS,
            expected_score + 1,
            20 + k as u8,
        );
        assert_ne!(forged, joined);
        let mut forged_complete = headers.clone();
        forged_complete.insert(forged.clone(), wrong_score_header);
        assert!(
            derive_dormant_native_v3_ghostdag_trial(
                CHAIN,
                GENESIS_NS,
                &g,
                &[forged],
                &forged_complete,
                k
            )
            .is_err(),
            "fully PoW-valid rejoin must reject forged blue_score at K={k}"
        );
    }
}
