//! Non-normative native-v3 PoW authenticated full-DAG block-order experiment.
//! Trial blue/red ordering is NOT consensus, finality, monetary replay or GO.

use std::collections::{BTreeMap, BTreeSet};

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3, types::BlockId,
};

#[path = "support/native_v3_trial_order_preview.rs"]
mod trial_order_preview;

use trial_order_preview::preview_unapproved_native_v3_order;

const CHAIN: &str = "pulsedag-unapproved-native-v3-full-order-real-pow";
const GENESIS_NS: u64 = 1_800_000_020_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;

fn genesis() -> (BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
    let block = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    (
        block.hash.clone(),
        BTreeMap::from([(block.hash, block.header)]),
    )
}

fn mine(
    mut parents: Vec<BlockId>,
    timestamp_ns: u64,
    height: u64,
    blue_score: u64,
    marker: u8,
) -> (BlockId, BlockHeaderV3) {
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: EASY_BITS,
        nonce: 0,
        merkle_root: format!("{marker:02x}").repeat(32),
        state_root: "33".repeat(32), // Intentional placeholder; no UTXO authority.
        blue_score,
        height,
    };
    for nonce in 0..4_096_u64 {
        header.nonce = nonce;
        let candidate = evaluate_v3_header_pow(&header, CHAIN).unwrap();
        if candidate.pow.accepted {
            return (candidate.block_hash, header);
        }
    }
    panic!("real KHeavyHash test target was not found within nonce budget");
}

fn assert_parent_before_child(
    genesis: &str,
    tip: &str,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    ordered: &[BlockId],
) {
    assert_eq!(ordered.len(), headers.len());
    assert_eq!(ordered.first().map(String::as_str), Some(genesis));
    assert_eq!(ordered.last().map(String::as_str), Some(tip));
    let mut visited = BTreeSet::new();
    for hash in ordered {
        let h = &headers[hash];
        assert!(
            h.parents.iter().all(|parent| visited.contains(parent)),
            "the trial ordering may not emit a child before authenticated ancestry"
        );
        assert!(
            visited.insert(hash.clone()),
            "no vertex may be emitted twice"
        );
    }
    assert_eq!(visited.len(), headers.len());
}

#[test]
fn real_pow_three_way_join_k0_k1_k2_has_total_parent_first_order_without_dropping_red_blocks() {
    let (g, mut headers) = genesis();
    let mut siblings = Vec::new();
    for i in 0..3_u8 {
        let (id, header) = mine(
            vec![g.clone()],
            GENESIS_NS + 500_000_000 + u64::from(i) * 100_000,
            1,
            1,
            0x21 + i,
        );
        assert!(headers.insert(id.clone(), header).is_none());
        siblings.push(id);
    }
    siblings.sort();
    let selected = siblings[0].clone();
    for trial_k in 0..=2_usize {
        let (join, join_header) = mine(
            siblings.clone(),
            GENESIS_NS + 1_000_000_000 + trial_k as u64 * 100_000,
            2,
            2 + trial_k as u64,
            0x41 + trial_k as u8,
        );
        assert!(headers.insert(join.clone(), join_header).is_none());

        let preview = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &headers,
            trial_k,
        )
        .expect("unapproved complete parent-first PoW trial");
        assert_parent_before_child(&g, &join, &headers, &preview.ordered_blocks);
        assert_eq!(
            preview.selected_spine,
            vec![g.clone(), selected.clone(), join.clone()]
        );
        assert_eq!(preview.side_blues, siblings[1..1 + trial_k].to_vec());
        assert_eq!(preview.side_reds, siblings[1 + trial_k..].to_vec());
        assert_eq!(preview.ordered_blocks.len(), 5);
        assert_eq!(preview.ordered_blocks[1], selected);
        assert_eq!(preview.digest.len(), 64);
        assert_eq!(preview.trial_k, trial_k);

        let trial = derive_dormant_native_v3_ghostdag_trial(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &headers,
            trial_k,
        )
        .unwrap();
        assert_eq!(trial.blocks[&join].blue_score, 2 + trial_k as u64);
        assert_eq!(
            trial.blocks[&join].blue_work.decimal(),
            (4 + 2 * trial_k).to_string()
        );

        // A rebuilt ordered map is not an independent proof of network
        // ordering, but the input iteration order may not affect the result.
        let permuted: BTreeMap<_, _> = headers
            .iter()
            .rev()
            .map(|(id, h)| (id.clone(), h.clone()))
            .collect();
        assert_eq!(
            preview,
            preview_unapproved_native_v3_order(
                CHAIN,
                GENESIS_NS,
                &g,
                std::slice::from_ref(&join),
                &permuted,
                trial_k
            )
            .unwrap()
        );
        headers.remove(&join);
    }
}

#[test]
fn two_depth_fork_requires_complete_side_ancestry_and_rejects_forged_score() {
    let (g, mut headers) = genesis();
    let (a1, h_a1) = mine(vec![g.clone()], GENESIS_NS + 500_000_000, 1, 1, 0x51);
    let (a2, h_a2) = mine(vec![a1.clone()], GENESIS_NS + 1_000_000_000, 2, 2, 0x52);
    let (b1, h_b1) = mine(vec![g.clone()], GENESIS_NS + 600_000_000, 1, 1, 0x53);
    let (b2, h_b2) = mine(vec![b1.clone()], GENESIS_NS + 1_100_000_000, 2, 2, 0x54);
    headers.extend([
        (a1.clone(), h_a1),
        (a2.clone(), h_a2),
        (b1.clone(), h_b1),
        (b2.clone(), h_b2),
    ]);
    let (join, join_header) = mine(
        vec![a2.clone(), b2.clone()],
        GENESIS_NS + 1_500_000_000,
        3,
        5,
        0x55,
    );
    headers.insert(join.clone(), join_header);
    let ordered = preview_unapproved_native_v3_order(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&join),
        &headers,
        2,
    )
    .unwrap();
    assert_parent_before_child(&g, &join, &headers, &ordered.ordered_blocks);
    assert_eq!(ordered.ordered_blocks.len(), 6);
    assert_eq!(ordered.side_blues.len(), 2);
    assert!(ordered.side_reds.is_empty());

    let mut missing_side = headers.clone();
    missing_side.remove(&b1);
    assert!(preview_unapproved_native_v3_order(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&join),
        &missing_side,
        2,
    )
    .is_err());

    let (forged, wrong_score_header) = mine(
        vec![a2, b2],
        GENESIS_NS + 1_600_000_000,
        3,
        6, // Valid PoW but incorrect derived trial K=2 score.
        0x57,
    );
    let mut forged_headers = headers;
    forged_headers.remove(&join);
    forged_headers.insert(forged.clone(), wrong_score_header);
    assert!(preview_unapproved_native_v3_order(
        CHAIN,
        GENESIS_NS,
        &g,
        std::slice::from_ref(&forged),
        &forged_headers,
        2,
    )
    .is_err());
}

#[test]
fn a_complete_header_corpus_with_two_tips_must_not_claim_one_canonical_order() {
    let (g, mut headers) = genesis();
    let (a, h_a) = mine(vec![g.clone()], GENESIS_NS + 500_000_000, 1, 1, 0x71);
    let (b, h_b) = mine(vec![g.clone()], GENESIS_NS + 600_000_000, 1, 1, 0x72);
    headers.insert(a.clone(), h_a);
    headers.insert(b.clone(), h_b);
    let mut tips = vec![a, b];
    tips.sort();
    assert!(preview_unapproved_native_v3_order(CHAIN, GENESIS_NS, &g, &tips, &headers, 1).is_err());
}
