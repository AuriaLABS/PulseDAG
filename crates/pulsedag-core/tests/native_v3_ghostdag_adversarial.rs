//! Additional adversarial native-v3 GHOSTDAG trial tests; never authoritative.
//! Mine REAL KHeavyHash-valid headers with heterogeneous PoW targets, then
//! derive blue-work / selected-parent / K-clusters from authenticated ancestry.
//! Trial K=0..2, no production GHOSTDAG freeze, mining or contracts activation.

use std::collections::BTreeMap;

#[path = "support/native_v3_utxo_preview.rs"]
mod detached_utxo;

#[path = "support/native_v3_trial_order_preview.rs"]
mod trial_order_preview;

use trial_order_preview::preview_unapproved_native_v3_order;

use detached_utxo::{
    apply, apply_selected_spine_preview, coin, fingerprint, total_supply, HypotheticalBlockEffects,
    PreviewSet,
};

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::{
        derive_dormant_native_v3_ghostdag_trial, preview_dormant_v3_candidate_selected_chain_delta,
    },
    pow_v3::evaluate_v3_header_pow,
    protocol::BLOCK_HEADER_VERSION_V3,
    types::BlockId,
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
        // IMPORTANT: blue_score != number of blocks in a candidate accepted
        // topological order. Even red side blocks appear in this UNAPPROVED
        // diagnostic ordering, and no monetary replay reads it.
        let order = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&joined),
            &headers,
            trial_k,
        )
        .expect("true mined 3-parent candidate full-ancestor order");
        assert_eq!(
            order.selected_spine,
            vec![g.clone(), hard.clone(), joined.clone()]
        );
        let mut expected = vec![g.clone(), hard.clone()];
        expected.extend(side.iter().cloned());
        expected.push(joined.clone());
        assert_eq!(order.ordered_blocks, expected);
        assert_eq!(order.side_blues, side[..trial_k].to_vec());
        assert_eq!(order.side_reds, side[trial_k..].to_vec());
        assert_eq!(order.ordered_blocks.len(), 5);
        assert_eq!(order.trial_k, trial_k);
        let reversed: BTreeMap<_, _> = headers
            .iter()
            .rev()
            .map(|(key, val)| (key.clone(), val.clone()))
            .collect();
        assert_eq!(
            order,
            preview_unapproved_native_v3_order(
                CHAIN,
                GENESIS_NS,
                &g,
                std::slice::from_ref(&joined),
                &reversed,
                trial_k,
            )
            .unwrap(),
            "diagnostic ordering must be insertion-independent"
        );

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
    let (b2, hb2) = mine(
        vec![b.clone()],
        2,
        GENESIS_NS + 1_100_000_000,
        2,
        EASY_BITS,
        0x85,
    );
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
    let switch = preview_dormant_v3_candidate_selected_chain_delta(&tips, &a3, &b2)
        .expect("authenticated both tips' selected-parent metadata");
    assert_eq!(switch.common_selected_ancestor, g);
    assert_eq!(
        switch.detach_tip_first,
        vec![a3.clone(), a2.clone(), a.clone()]
    );
    assert_eq!(switch.attach_ancestor_first, vec![b.clone(), b2.clone()]);
    assert_eq!(switch.old_tip_blue_work.decimal(), "6");
    assert_eq!(switch.new_tip_blue_work.decimal(), "514");

    let reverse = preview_dormant_v3_candidate_selected_chain_delta(&tips, &b2, &a3)
        .expect("reverse metadata comparison is symmetric");

    // A *detached* UTXO demonstration tied to the REAL mined-header selected
    // spine above. Effects are hypothetical, NOT signed transactions or a
    // protocol-valid native-v3 monetary/reward/state-root implementation.
    let reserve = coin("test-only-reserve", 1_000, "reserve", false, 0);
    let old_a_credit = coin("old-a-credit", 1_000, "alice", false, 1);
    let old_a_reward = coin("old-a-mint", 50, "old-miner", true, 1);
    let old_a2_credit = coin("old-a2-credit", 970, "alice", false, 2);
    let old_a3_credit = coin("old-a3-credit", 960, "alice", false, 3);
    let new_b_credit = coin("new-b-credit", 1_000, "bob", false, 1);
    let new_b_reward = coin("new-b-mint", 80, "new-miner", true, 1);
    let new_b2_credit = coin("new-b2-credit", 940, "bob", false, 2);

    let effects = BTreeMap::from([
        (
            a.clone(),
            HypotheticalBlockEffects {
                block: a.clone(),
                spent: vec![reserve.clone()],
                created: vec![old_a_credit.clone(), old_a_reward.clone()],
                test_issuance: 50,
                test_fee_burn: 0,
            },
        ),
        (
            a2.clone(),
            HypotheticalBlockEffects {
                block: a2.clone(),
                spent: vec![old_a_credit],
                created: vec![old_a2_credit.clone()],
                test_issuance: 0,
                test_fee_burn: 30,
            },
        ),
        (
            a3.clone(),
            HypotheticalBlockEffects {
                block: a3.clone(),
                spent: vec![old_a2_credit],
                created: vec![old_a3_credit.clone()],
                test_issuance: 0,
                test_fee_burn: 10,
            },
        ),
        (
            b.clone(),
            HypotheticalBlockEffects {
                block: b.clone(),
                spent: vec![reserve.clone()],
                created: vec![new_b_credit.clone(), new_b_reward.clone()],
                test_issuance: 80,
                test_fee_burn: 0,
            },
        ),
        (
            b2.clone(),
            HypotheticalBlockEffects {
                block: b2.clone(),
                spent: vec![new_b_credit],
                created: vec![new_b2_credit.clone()],
                test_issuance: 0,
                test_fee_burn: 60,
            },
        ),
    ]);
    let genesis_state = PreviewSet::from([(reserve.outpoint.clone(), reserve)]);
    let after_a = apply(&genesis_state, &effects[&a]).unwrap();
    let after_a2 = apply(&after_a, &effects[&a2]).unwrap();
    let old_snapshot = apply(&after_a2, &effects[&a3]).unwrap();
    assert_eq!(total_supply(&old_snapshot).unwrap(), 1_010);
    let old_snapshot_identity = fingerprint(&old_snapshot);
    let switched = apply_selected_spine_preview(&old_snapshot, &switch, &effects)
        .expect("simulate disconnected old effects and applied new effects");
    assert_eq!(total_supply(&switched).unwrap(), 1_020);
    assert!(switched.contains_key(&new_b2_credit.outpoint));
    assert!(switched.contains_key(&new_b_reward.outpoint));
    assert!(!switched.contains_key(&old_a3_credit.outpoint));
    assert!(!switched.contains_key(&old_a_reward.outpoint));

    let backwards = apply_selected_spine_preview(&switched, &reverse, &effects)
        .expect("reverse hypothetical branch switch");
    assert_eq!(fingerprint(&backwards), old_snapshot_identity);
    assert_eq!(fingerprint(&old_snapshot), old_snapshot_identity);

    let mut bad_effects = effects.clone();
    bad_effects.get_mut(&b2).unwrap().test_fee_burn += 1;
    assert!(apply_selected_spine_preview(&old_snapshot, &switch, &bad_effects).is_err());
    assert_eq!(fingerprint(&old_snapshot), old_snapshot_identity);

    assert_eq!(reverse.common_selected_ancestor, g);
    assert_eq!(reverse.detach_tip_first, vec![b2.clone(), b.clone()]);
    assert_eq!(
        reverse.attach_ancestor_first,
        vec![a.clone(), a2.clone(), a3.clone()]
    );
    assert_eq!(reverse.old_tip_blue_work.decimal(), "514");
    assert_eq!(reverse.new_tip_blue_work.decimal(), "6");

    let unchanged = preview_dormant_v3_candidate_selected_chain_delta(&tips, &a3, &a3)
        .expect("identical tips have no selected-spine delta");
    assert_eq!(unchanged.common_selected_ancestor, a3);
    assert!(unchanged.detach_tip_first.is_empty());
    assert!(unchanged.attach_ancestor_first.is_empty());
    assert!(
        preview_dormant_v3_candidate_selected_chain_delta(&tips, "not-a-known-tip", &b2).is_err(),
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
        let order = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &headers,
            k,
        )
        .expect("multi-height K trial fully authenticated and classified");
        assert_eq!(
            order.selected_spine,
            vec![g.clone(), b.clone(), b2.clone(), join.clone()]
        );
        assert_eq!(
            order.ordered_blocks,
            vec![
                g.clone(),
                b.clone(),
                b2.clone(),
                a.clone(),
                a2.clone(),
                a3.clone(),
                join.clone(),
            ]
        );
        assert_eq!(order.side_blues, side[..blue_count].to_vec());
        assert_eq!(order.side_reds, side[blue_count..].to_vec());
        assert_eq!(order.ordered_blocks.len(), 7);
        assert_ne!(order.digest, "", "trial digest must be nonempty");
        let reversed: BTreeMap<_, _> = headers
            .iter()
            .rev()
            .map(|(key, val)| (key.clone(), val.clone()))
            .collect();
        let another = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &reversed,
            k,
        )
        .unwrap();
        assert_eq!(order, another);
        let mut incomplete = headers.clone();
        incomplete.remove(&a2);
        assert!(
            preview_unapproved_native_v3_order(
                CHAIN,
                GENESIS_NS,
                &g,
                std::slice::from_ref(&join),
                &incomplete,
                k,
            )
            .is_err(),
            "missing side-branch ancestry must never create an incomplete ledger ordering"
        );

        assert_eq!(result.blocks[&a3].blue_work.decimal(), "6");
        assert_eq!(result.blocks[&b2].blue_work.decimal(), "514");
    }
}

#[test]
fn detached_utxo_preview_rejects_double_spend_mint_collision_and_undo_tampering() {
    use detached_utxo::{undo, HypotheticalBlockEffects};
    let source = coin("source", 100, "alice", false, 0);
    let next_coin = coin("next-coin", 90, "bob", false, 1);
    let original = PreviewSet::from([(source.outpoint.clone(), source.clone())]);
    let good = HypotheticalBlockEffects {
        block: "dummy-branch".into(),
        spent: vec![source.clone()],
        created: vec![next_coin.clone()],
        test_issuance: 0,
        test_fee_burn: 10,
    };
    let applied = apply(&original, &good).unwrap();
    assert_eq!(total_supply(&applied).unwrap(), 90);
    let restored = undo(&applied, &good).unwrap();
    assert_eq!(fingerprint(&restored), fingerprint(&original));
    assert!(
        apply(&applied, &good).is_err(),
        "spent input cannot be replayed"
    );
    assert!(
        undo(&original, &good).is_err(),
        "missing created coin cannot be undone"
    );
    let before = fingerprint(&original);

    let bad_cases = [
        HypotheticalBlockEffects {
            spent: vec![source.clone(), source.clone()],
            created: vec![coin("too-large", 200, "bob", false, 1)],
            ..good.clone()
        },
        HypotheticalBlockEffects {
            test_fee_burn: 11,
            ..good.clone()
        },
        HypotheticalBlockEffects {
            test_issuance: 50,
            ..good.clone()
        },
        HypotheticalBlockEffects {
            created: vec![source.clone()],
            test_fee_burn: 0,
            ..good.clone()
        },
        HypotheticalBlockEffects {
            created: vec![coin("source", 100, "eve", false, 1)],
            test_fee_burn: 0,
            ..good.clone()
        },
    ];
    for bad in bad_cases {
        assert!(apply(&original, &bad).is_err());
        assert_eq!(fingerprint(&original), before);
    }
    // This simulator intentionally verifies arithmetic/UTXO replay ONLY.
    // A different output address is not invalid until a real transaction
    // signature/ownership checker has validated the original transaction.
    let unverified_recipient = HypotheticalBlockEffects {
        created: vec![coin("next-coin", 90, "eve", false, 1)],
        ..good.clone()
    };
    assert!(apply(&original, &unverified_recipient).is_ok());
    // Never interpret this test-only result as a signed transaction approval.
    let forgery = HypotheticalBlockEffects {
        spent: vec![coin("source", 100, "malicious", false, 0)],
        ..good.clone()
    };
    assert!(apply(&original, &forgery).is_err());
    assert_eq!(fingerprint(&original), before);
    let mutated = PreviewSet::from([(
        next_coin.outpoint.clone(),
        coin("next-coin", 90, "hijacked", false, 1),
    )]);
    assert!(undo(&mutated, &good).is_err());

    let invalid_spine =
        pulsedag_core::native_ghostdag_policy_candidate_v3::DormantCandidateSelectedChainDelta {
            common_selected_ancestor: "common".into(),
            detach_tip_first: vec!["a".into()],
            attach_ancestor_first: vec!["a".into()],
            old_tip_blue_work: Default::default(),
            new_tip_blue_work: Default::default(),
        };
    assert!(
        apply_selected_spine_preview(&original, &invalid_spine, &BTreeMap::new()).is_err(),
        "same block may not be both disconnected and reconnected"
    );
}
