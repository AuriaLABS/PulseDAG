//! TEST-ONLY / UNAPPROVED: actual KHeavyHash PoW GHOSTDAG trial K=0/32/33 boundary.
//! 34 direct parent headers are permitted by the current diagnostic shape,
//! NOT an endorsement of any actual native-v3 8-parent or K policy.
//! No runtime consensus, monetary settlement, P2P, wallet, miner or contracts.

use std::collections::BTreeMap;

#[path = "support/native_v3_trial_order_preview.rs"]
#[allow(dead_code)]
mod trial_order_preview;
use trial_order_preview::preview_unapproved_native_v3_order;

use pulsedag_core::{
    genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3, types::BlockId,
};

const CHAIN: &str = "pulsedag-dormant-v3-ghostdag-variable-work";
const GENESIS_NS: u64 = 1_800_000_004_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff; // diagnostic work = 2

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

        let ordered = preview_unapproved_native_v3_order(
            CHAIN,
            GENESIS_NS,
            &g,
            std::slice::from_ref(&join),
            &headers,
            trial_k,
        )
        .expect("parent-first bounded 34-sibling diagnostic preview");
        assert_eq!(ordered.ordered_blocks.len(), 36); // G + 34 siblings + join.
        assert_eq!(ordered.side_blues.len(), blue_count);
        assert_eq!(ordered.side_reds.len(), 33 - blue_count);
        assert_eq!(ordered.ordered_blocks[0], g);
        assert_eq!(ordered.ordered_blocks.last(), Some(&join));

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
