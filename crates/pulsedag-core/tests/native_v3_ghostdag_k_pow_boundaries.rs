//! Non-normative independently PoW-mined GHOSTDAG K-boundary regressions.
//! Extracted from #1341, minus all monetary/order preview support dependencies.
//! NEVER production selected-parent authority or an approved K or parent cap.

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3, types::BlockId,
};
use std::collections::BTreeMap;

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
fn k32_k33_blue_anticone_pressure_on_34_actual_pow_valid_siblings() {
    // Diagnostic boundary stress, NOT approval of K=32 or a 34-parent
    // network policy. The native header currently permits more parents than
    // the unapproved 8-parent production proposal. All 34 siblings carry
    // independently mined KHeavyHash-valid compact-target PoW.
    //
    // With exactly 33 incomparable merge-side siblings, the proposed
    // K-cluster must admit 32 and reject the 33rd at K=32. At K=33 the
    // final side sibling crosses the boundary and becomes blue.
    let (g, mut headers) = genesis();
    let mut siblings = Vec::new();
    for i in 0..34u8 {
        let (hash, header) = mine(
            vec![g.clone()],
            1,
            GENESIS_NS + 500_000_000 + u64::from(i) * 100_000,
            1,
            EASY_BITS,
            0xa0 + i,
        );
        assert!(
            headers.insert(hash.clone(), header).is_none(),
            "distinct real-PoW sibling hashes expected"
        );
        siblings.push(hash);
    }
    siblings.sort();
    assert_eq!(siblings.len(), 34);
    let expected_selected = siblings[0].clone();
    let other_siblings = siblings[1..].to_vec();

    for (trial_k, expected_score, expected_work, blue_count) in [
        (0usize, 2u64, "4", 0usize),
        (32, 34, "68", 32),
        (33, 35, "70", 33),
    ] {
        let (join, join_header) = mine(
            siblings.clone(),
            2,
            GENESIS_NS + 1_000_000_000 + trial_k as u64 * 100_000,
            expected_score,
            EASY_BITS,
            0xd0 + trial_k as u8,
        );
        headers.insert(join.clone(), join_header);
        let result = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &headers,
            trial_k,
        )
        .expect("fully authenticated 34-parent candidate K boundary");
        let metadata = &result.blocks[&join];
        assert_eq!(
            metadata.selected_parent.as_deref(),
            Some(expected_selected.as_str())
        );
        assert_eq!(metadata.blue_score, expected_score);
        assert_eq!(metadata.blue_work.decimal(), expected_work);
        assert_eq!(metadata.merge_blues, other_siblings[..blue_count].to_vec());
        assert_eq!(metadata.merge_reds, other_siblings[blue_count..].to_vec());

        // Even though the mined PoW and v3 SHA256 identity are valid,
        // a *separate* candidate committing the wrong blue_score fails.
        let (forged, forged_header) = mine(
            siblings.clone(),
            2,
            GENESIS_NS + 1_100_000_000 + trial_k as u64 * 100_000,
            expected_score + 1,
            EASY_BITS,
            0x60 + trial_k as u8,
        );
        headers.insert(forged.clone(), forged_header);
        assert!(
            derive_dormant_native_v3_ghostdag_trial(
                CHAIN,
                GENESIS_NS,
                &g,
                std::slice::from_ref(&forged),
                &headers,
                trial_k
            )
            .is_err(),
            "valid KHeavyHash cannot excuse an incorrect committed score"
        );
    }
}

#[test]
fn eight_parent_proposed_cap_k_cluster_boundary_requires_canonical_tips() {
    // Eight real PoW-valid genesis children: this targets the PROPOSED eight
    // direct-parent cap, NOT approval of its production value. Seven true
    // side-anticone candidates test K=6 vs K=7. K=32 is also checked as
    // an unapproved mathematical upper-range diagnostic.
    let (genesis_id, mut headers) = genesis();
    let mut siblings = Vec::new();
    for i in 0..8_u8 {
        let (id, header) = mine(
            vec![genesis_id.clone()],
            1,
            GENESIS_NS + 500_000_000 + u64::from(i) * 100_000,
            1,
            EASY_BITS,
            0x40 + i,
        );
        assert!(headers.insert(id.clone(), header).is_none());
        siblings.push(id);
    }
    siblings.sort();
    assert_eq!(siblings.len(), 8);
    let expected_selected = siblings[0].clone();

    // The existing native DAG verifier deliberately requires canonical
    // sorted/unique tip frontiers. Raw reversed peer arrival is INVALID, not
    // an alternative canonical representation. Normalize at the boundary.
    let mut reverse_tips = siblings.clone();
    reverse_tips.reverse();
    let forward = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &genesis_id,
        &siblings,
        &headers,
        32,
    )
    .expect("eight independent sorted tips should be a verified DAG");
    assert!(
        derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis_id,
            &reverse_tips,
            &headers,
            32,
        )
        .is_err(),
        "untrusted unsorted tip frontier must fail closed"
    );
    reverse_tips.sort_unstable();
    let canonicalized = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        &genesis_id,
        &reverse_tips,
        &headers,
        32,
    )
    .expect("canonicalizing a reversed arrival order yields the same metadata");
    assert_eq!(forward, canonicalized);
    assert_eq!(forward.blocks.len(), 9);

    for (k, expected_score, expected_work, blue_count) in [
        (0_usize, 2_u64, "4", 0_usize),
        (6, 8, "16", 6),
        (7, 9, "18", 7),
        (32, 9, "18", 7),
    ] {
        let (join, header) = mine(
            siblings.clone(),
            2,
            GENESIS_NS + 1_000_000_000 + (k as u64) * 100_000,
            expected_score,
            EASY_BITS,
            0x80 + k as u8,
        );
        headers.insert(join.clone(), header);
        let trial = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &genesis_id,
            std::slice::from_ref(&join),
            &headers,
            k,
        )
        .expect("canonical eight-parent join must pass independent native DAG checks");
        let meta = &trial.blocks[&join];
        assert_eq!(
            meta.selected_parent.as_deref(),
            Some(expected_selected.as_str())
        );
        assert_eq!(meta.blue_score, expected_score);
        assert_eq!(meta.blue_work.decimal(), expected_work);
        assert_eq!(meta.merge_blues, siblings[1..1 + blue_count].to_vec());
        assert_eq!(meta.merge_reds, siblings[1 + blue_count..].to_vec());

        let mut damaged = headers.clone();
        damaged.remove(&siblings[7]);
        assert!(
            derive_dormant_native_v3_ghostdag_trial(
                CHAIN,
                GENESIS_NS,
                &genesis_id,
                std::slice::from_ref(&join),
                &damaged,
                k
            )
            .is_err(),
            "a valid PoW join must not excuse a missing side parent at any K"
        );
    }
}

#[test]
fn inherited_blue_anticone_boundary_rejects_new_merge_when_existing_blue_is_saturated() {
    // Two joins with a reused selected-parent blue past. At K=2, the NEW
    // merge candidate has exactly two incomparable blues (within K), but one
    // existing blue already has two incomparable blues. The second GHOSTDAG
    // K-cluster condition therefore MUST reject the otherwise PoW-valid
    // merge. At K=3, it is accepted. This catches stale/incomplete counters
    // inherited across selected-parent metadata, not just a shallow fork.
    let (g, mut headers) = genesis();
    let mut siblings = Vec::new();
    for i in 0..3_u8 {
        let (id, header) = mine(
            vec![g.clone()],
            1,
            GENESIS_NS + 500_000_000 + u64::from(i) * 100_000,
            1,
            EASY_BITS,
            0x91 + i,
        );
        headers.insert(id.clone(), header);
        siblings.push(id);
    }
    siblings.sort();
    let (d, h_d) = mine(
        siblings[..2].to_vec(),
        2,
        GENESIS_NS + 800_000_000,
        3,
        EASY_BITS,
        0x95,
    );
    headers.insert(d.clone(), h_d);

    let (first_join, h_first) = mine(
        siblings.clone(),
        2,
        GENESIS_NS + 1_000_000_000,
        4,
        EASY_BITS,
        0x96,
    );
    headers.insert(first_join.clone(), h_first);
    // Full first-join blue past: G, all three siblings, first_join. Every
    // sibling is incomparable to the other two, so has anticone count 2.
    for k in [2_usize, 3] {
        let (second_join, h_second) = mine(
            vec![first_join.clone(), d.clone()],
            3,
            GENESIS_NS + 1_500_000_000 + (k as u64) * 100_000,
            if k == 2 { 5 } else { 6 },
            EASY_BITS,
            0x98 + k as u8,
        );
        headers.insert(second_join.clone(), h_second);

        let result = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&second_join),
            &headers,
            k,
        )
        .expect("both real-PoW joins must remain valid under candidate K");
        assert_eq!(result.blocks.len(), 7);
        let first = &result.blocks[&first_join];
        assert_eq!(first.selected_parent.as_deref(), Some(siblings[0].as_str()));
        assert_eq!(first.merge_blues, siblings[1..].to_vec());
        assert!(first.merge_reds.is_empty());
        assert_eq!(first.blue_score, 4);
        assert_eq!(first.blue_work.decimal(), "8");

        let last = &result.blocks[&second_join];
        assert_eq!(last.selected_parent.as_deref(), Some(first_join.as_str()));
        assert_eq!(last.blue_score, if k == 2 { 5 } else { 6 });
        assert_eq!(last.blue_work.decimal(), if k == 2 { "10" } else { "12" });
        if k == 2 {
            assert!(last.merge_blues.is_empty());
            assert_eq!(last.merge_reds, vec![d.clone()]);
        } else {
            assert_eq!(last.merge_blues, vec![d.clone()]);
            assert!(last.merge_reds.is_empty());
        }

        let reversed: BTreeMap<_, _> = headers
            .iter()
            .rev()
            .map(|(hash, header)| (hash.clone(), header.clone()))
            .collect();
        assert_eq!(
            result,
            derive_dormant_native_v3_ghostdag_trial(
                CHAIN,
                GENESIS_NS,
                &g,
                std::slice::from_ref(&second_join),
                &reversed,
                k,
            )
            .unwrap(),
            "map arrival order must not change inherited K-cluster checks"
        );
    }
}

#[test]
fn multi_depth_anticone_k_threshold_matrix_mined_headers_preserves_exact_cluster_bounds() {
    // Twenty-four full-PoW, eight-parent-or-smaller DAG evaluations. A chain
    // of D blocks is comparable within itself but incomparable to every block
    // on other chains. To accept an additional FULL side chain under this
    // candidate greedy K rule, K must cover D * (accepted_side_chains + 1).
    //
    // The tests independently derive the expected counts from this graph
    // structure; no cached counter, candidate blue_work, or peer metadata is
    // used as expected authority. Trial K values are NOT protocol approval.
    for (branches, depth) in [(3_u8, 2_u64), (5, 4), (8, 7), (8, 12)] {
        for k in [
            0_usize,
            depth as usize - 1,
            depth as usize,
            (2 * depth) as usize - 1,
            (2 * depth) as usize,
            32,
        ] {
            let (g, mut headers) = genesis();
            let mut tips = Vec::new();
            for branch in 0..branches {
                let mut parent = g.clone();
                for height in 1..=depth {
                    let marker = branch * 12 + (height - 1) as u8;
                    let (id, header) = mine(
                        vec![parent],
                        height,
                        GENESIS_NS + height * 500_000_000 + u64::from(branch) * 100_000,
                        height,
                        EASY_BITS,
                        marker,
                    );
                    assert!(headers.insert(id.clone(), header).is_none());
                    parent = id;
                }
                tips.push(parent);
            }
            tips.sort();
            let side_chains_accepted = usize::min(usize::from(branches) - 1, k / depth as usize);
            let expected_blues = side_chains_accepted * depth as usize;
            let expected_score = depth + 1 + expected_blues as u64;
            let expected_work = expected_score * 2; // Every target work=2.
            let (join, header) = mine(
                tips.clone(),
                depth + 1,
                GENESIS_NS + (depth + 1) * 500_000_000,
                expected_score,
                EASY_BITS,
                0xb0 + k as u8,
            );
            assert!(headers.insert(join.clone(), header).is_none());
            let result = derive_dormant_native_v3_ghostdag_trial(
                CHAIN,
                GENESIS_NS,
                &g,
                std::slice::from_ref(&join),
                &headers,
                k,
            )
            .expect("full authenticated matrix DAG must satisfy independent K/score invariant");
            let expected_vertices = 2 + usize::from(branches) * depth as usize;
            assert_eq!(result.blocks.len(), expected_vertices);
            let meta = &result.blocks[&join];
            assert_eq!(
                meta.selected_parent.as_deref(),
                Some(tips[0].as_str()),
                "heavier/earlier fork selection at branches={branches}, depth={depth}, K={k}"
            );
            assert_eq!(meta.blue_score, expected_score);
            assert_eq!(meta.blue_work.decimal(), expected_work.to_string());
            assert_eq!(
                meta.merge_blues.len(),
                expected_blues,
                "inherited anticone count admitted wrong side-branch length at branches={branches}, depth={depth}, K={k}"
            );
            assert_eq!(
                meta.merge_reds.len(),
                (usize::from(branches) - 1) * depth as usize - expected_blues,
            );
            let repeated: BTreeMap<_, _> = headers
                .iter()
                .rev()
                .map(|(id, header)| (id.clone(), header.clone()))
                .collect();
            assert_eq!(
                result,
                derive_dormant_native_v3_ghostdag_trial(
                    CHAIN,
                    GENESIS_NS,
                    &g,
                    std::slice::from_ref(&join),
                    &repeated,
                    k
                )
                .expect("authenticated DAG should not depend on map insertion"),
                "candidate metadata changed with arrival order at K={k}"
            );
        }
    }
}
