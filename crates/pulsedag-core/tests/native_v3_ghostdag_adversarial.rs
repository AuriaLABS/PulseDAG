//! Additional adversarial native-v3 GHOSTDAG trial tests; never authoritative.
//! Mine REAL KHeavyHash-valid headers with heterogeneous PoW targets, then
//! derive blue-work / selected-parent / K-clusters from authenticated ancestry.
//! Trial K=0..2, no production GHOSTDAG freeze, mining or contracts activation.

use std::collections::BTreeMap;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial,
        preview_dormant_v3_candidate_selected_chain_delta,
    },
    pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3, types::BlockId,
};

const CHAIN: &str = "pulsedag-dormant-v3-ghostdag-variable-work";
const GENESIS_NS: u64 = 1_800_000_004_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff; // candidate work = 2
const HARD_BITS: u32 = 0x1f7f_ffff; // candidate work = 512

fn mine(
    mut parents: Vec<BlockId>,
    height: u64,
    timestamp_ns: u64,
    score: u64,
    bits: u32,
    marker: u8,
) -> (BlockId, BlockHeaderV3) {
    parents.sort();
    let mut h = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: bits,
        nonce: 0,
        merkle_root: format!("{marker:02x}").repeat(32),
        state_root: "33".repeat(32), // Diagnostic only, not monetary UTXO replay.
        blue_score: score,
        height,
    };
    for nonce in 0..65_536u64 {
        h.nonce = nonce;
        let result = evaluate_v3_header_pow(&h, CHAIN).expect("valid frozen trial header");
        if result.pow.accepted {
            return (result.block_hash, h);
        }
    }
    panic!("PoW trial could not find an easy/medium target within 65536 nonces");
}

fn genesis() -> (BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
    let g = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    (g.hash.clone(), BTreeMap::from([(g.hash, g.header)]))
}

#[test]
fn target_weighted_hard_parent_wins_and_k_changes_actual_merge_colors() {
    let (g, mut headers) = genesis();
    let (easy_a, h_a) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        EASY_BITS,
        0x11,
    );
    let (hard, h_hard) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 600_000_000,
        1,
        HARD_BITS,
        0x22,
    );
    let (easy_b, h_b) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 700_000_000,
        1,
        EASY_BITS,
        0x33,
    );
    headers.insert(easy_a.clone(), h_a);
    headers.insert(hard.clone(), h_hard);
    headers.insert(easy_b.clone(), h_b);

    let mut side = [easy_a.clone(), easy_b.clone()];
    side.sort();
    let mut frontier = vec![easy_a.clone(), hard.clone(), easy_b.clone()];
    frontier.sort();

    for trial_k in 0..=2usize {
        let expected_score = 2 + trial_k as u64;
        let (joined, h_join) = mine(
            frontier.clone(),
            2,
            GENESIS_NS + 1_000_000_000,
            expected_score,
            EASY_BITS,
            0x50 + trial_k as u8,
        );
        headers.insert(joined.clone(), h_join);
        let result = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&joined),
            &headers,
            trial_k,
        )
        .expect("authentic candidate score/selection/work and K cluster");
        let metadata = &result.blocks[&joined];
        assert_eq!(metadata.selected_parent.as_deref(), Some(hard.as_str()));
        assert_eq!(metadata.blue_score, expected_score);
        assert_eq!(
            metadata.blue_work.decimal(),
            (514 + 2 * trial_k).to_string()
        );
        assert_eq!(metadata.merge_blues, side[..trial_k].to_vec());
        assert_eq!(metadata.merge_reds, side[trial_k..].to_vec());
        assert_eq!(result.k_trial, trial_k);
        assert_eq!(result.blocks[&hard].blue_work.decimal(), "512");
        assert_eq!(result.blocks[&easy_a].blue_work.decimal(), "2");
        assert_eq!(result.blocks[&easy_b].blue_work.decimal(), "2");
    }
}

#[test]
fn malformed_committed_score_rejected_even_if_kheavyhash_is_valid() {
    let (g, mut headers) = genesis();
    let (a, ha) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        EASY_BITS,
        0x61,
    );
    let (b, hb) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 600_000_000,
        1,
        EASY_BITS,
        0x62,
    );
    headers.insert(a.clone(), ha);
    headers.insert(b.clone(), hb);
    let (invalid, hi) = mine(
        vec![a, b],
        2,
        GENESIS_NS + 1_000_000_000,
        42,
        EASY_BITS,
        0x63,
    );
    headers.insert(invalid.clone(), hi);
    for k in [0, 1, 2] {
        let verdict = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&invalid),
            &headers,
            k,
        );
        let error = verdict
            .expect_err("PoW-valid but forged score must fail closed")
            .to_string();
        assert!(error.contains("committed blue score differs"), "{error}");
    }
}

#[test]
fn redundant_direct_parent_frontier_fails_closed_without_live_consensus() {
    let (g, mut headers) = genesis();
    let (a, ha) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        EASY_BITS,
        0x71,
    );
    headers.insert(a.clone(), ha);
    let (child, hc) = mine(
        vec![a.clone()],
        2,
        GENESIS_NS + 1_000_000_000,
        2,
        EASY_BITS,
        0x72,
    );
    headers.insert(child.clone(), hc);
    let (invalid, hi) = mine(
        vec![a, child],
        3,
        GENESIS_NS + 1_500_000_000,
        3,
        EASY_BITS,
        0x73,
    );
    headers.insert(invalid.clone(), hi);
    let result =
        derive_dormant_native_v3_ghostdag_trial(CHAIN, GENESIS_NS, &g, &[invalid], &headers, 2);
    let error = result
        .expect_err("redundant ancestor/direct parent")
        .to_string();
    assert!(error.contains("antichain"), "{error}");
}

#[test]
fn multi_height_heterogeneous_work_selected_parent_switch_and_k_boundary() {
    // A longer easy-target chain (6 total trial work) competes with a
    // shorter branch carrying one harder PoW block (514 total trial work).
    // The merge uses real mined headers but NEVER triggers an actual reorg.
    let (g, mut headers) = genesis();
    let (a, ha) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 500_000_000,
        1,
        EASY_BITS,
        0x81,
    );
    headers.insert(a.clone(), ha);
    let (a2, ha2) = mine(
        vec![a.clone()],
        2,
        GENESIS_NS + 1_000_000_000,
        2,
        EASY_BITS,
        0x82,
    );
    headers.insert(a2.clone(), ha2);
    let (a3, ha3) = mine(
        vec![a2.clone()],
        3,
        GENESIS_NS + 1_500_000_000,
        3,
        EASY_BITS,
        0x83,
    );
    headers.insert(a3.clone(), ha3);
    let (b, hb) = mine(
        vec![g.clone()],
        1,
        GENESIS_NS + 600_000_000,
        1,
        HARD_BITS,
        0x84,
    );
    headers.insert(b.clone(), hb);
    let (b2, hb2) = mine(vec![b.clone()], 2, GENESIS_NS + 1_100_000_000, 2, EASY_BITS, 0x85);
    headers.insert(b2.clone(), hb2);

    let tips = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &g,
        &[a3.clone(), b2.clone()],
        &headers,
        1,
    )
    .expect("both competing tips must have valid PoW and provenance");
    assert_eq!(tips.blocks[&a3].blue_work.decimal(), "6");
    assert_eq!(tips.blocks[&b2].blue_work.decimal(), "514");
    assert!(tips.blocks[&a3].blue_work.0[0] < tips.blocks[&b2].blue_work.0[0]);

    // Pure selected-SPINE preview: no UTXO rollback, no accepted-order
    // decision, no finality, no on-chain activation. Full merge-set blocks
    // must be separately processed under a future frozen monetary policy.
    let switch = preview_dormant_v3_candidate_selected_chain_delta(
        &tips, &a3, &b2,
    ).expect("authenticated both tips' selected-parent metadata");
    assert_eq!(switch.common_selected_ancestor, g);
    assert_eq!(switch.detach_tip_first, vec![a3.clone(), a2.clone(), a.clone()]);
    assert_eq!(switch.attach_ancestor_first, vec![b.clone(), b2.clone()]);
    assert_eq!(switch.old_tip_blue_work.decimal(), "6");
    assert_eq!(switch.new_tip_blue_work.decimal(), "514");

    let reverse = preview_dormant_v3_candidate_selected_chain_delta(&tips, &b2, &a3)
        .expect("reverse metadata comparison is symmetric");
    assert_eq!(reverse.common_selected_ancestor, g);
    assert_eq!(reverse.detach_tip_first, vec![b2.clone(), b.clone()]);
    assert_eq!(reverse.attach_ancestor_first, vec![a.clone(), a2.clone(), a3.clone()]);
    assert_eq!(reverse.old_tip_blue_work.decimal(), "514");
    assert_eq!(reverse.new_tip_blue_work.decimal(), "6");

    let unchanged = preview_dormant_v3_candidate_selected_chain_delta(&tips, &a3, &a3)
        .expect("identical tips have no selected-spine delta");
    assert_eq!(unchanged.common_selected_ancestor, a3);
    assert!(unchanged.detach_tip_first.is_empty());
    assert!(unchanged.attach_ancestor_first.is_empty());
    assert!(
        preview_dormant_v3_candidate_selected_chain_delta(&tips, "not-a-known-tip", &b2)
            .is_err(),
        "unknown disconnected tip must fail closed"
    );


    let mut parents = vec![a3.clone(), b2.clone()];
    parents.sort();
    for (k, score, work, blue_count) in [
        (0usize, 3u64, "516", 0usize),
        (1, 3, "516", 0),
        (2, 5, "520", 2),
        (3, 6, "522", 3),
    ] {
        let (join, h_join) = mine(
            parents.clone(),
            4,
            GENESIS_NS + 2_000_000_000,
            score,
            EASY_BITS,
            0x90 + k as u8,
        );
        headers.insert(join.clone(), h_join);
        let result = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &headers,
            k,
        )
        .expect("independently derived trial K decision on authenticated full DAG");
        let record = &result.blocks[&join];
        assert_eq!(record.selected_parent.as_deref(), Some(b2.as_str()));
        assert_eq!(record.blue_score, score);
        assert_eq!(record.blue_work.decimal(), work);
        let side = [a.clone(), a2.clone(), a3.clone()];
        assert_eq!(record.merge_blues, side[..blue_count].to_vec());
        assert_eq!(record.merge_reds, side[blue_count..].to_vec());
        assert_eq!(result.blocks[&a3].blue_work.decimal(), "6");
        assert_eq!(result.blocks[&b2].blue_work.decimal(), "514");
    }
}
