//! DORMANT TEST-ONLY native v3 selection-to-retarget trust boundary.
//!
//! The existing retarget adapter accepts externally supplied selected-parent
//! metadata. This test independently derives *candidate* GHOSTDAG selection
//! over complete, real-PoW ancestry before calling the frozen-genesis guard.
//! No approved K, production admission, mining, monetary replay, or pruning.

use std::collections::BTreeMap;

use pulsedag_core::{
    errors::PulseError,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    pow_v3::{evaluate_v3_header_pow, verify_v3_header_pow},
    protocol::BLOCK_HEADER_VERSION_V3,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_genesis_guard_v3::verify_dormant_frozen_genesis_retarget_v3_ns,
    retarget_v3::V3RetargetDecision,
    types::BlockId,
};

const CHAIN: &str = "native-v3-unapproved-derived-selection-genesis-retarget";
const GENESIS_NS: u64 = 1_800_000_029_000_000_000;
const EASY_BITS: u32 = 0x207f_ffff;
const HARD_BITS: u32 = 0x1f7f_ffff;

fn mine(
    mut parents: Vec<BlockId>,
    height: u64,
    score: u64,
    timestamp_ns: u64,
    bits: u32,
    marker: u8,
) -> (BlockId, BlockHeaderV3) {
    parents.sort();
    let mut header = BlockHeaderV3 {
        version: BLOCK_HEADER_VERSION_V3,
        parents,
        timestamp_ns,
        difficulty: bits,
        nonce: 0,
        merkle_root: format!("{marker:02x}").repeat(32),
        state_root: "33".repeat(32), // Not a UTXO commitment.
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
    panic!("bounded test-only KHeavyHash nonce search exhausted");
}

/// One strictly local proof composition, NOT an admission or policy-freeze API.
/// A caller-selected map must never substitute for the candidate's re-derivation.
fn preview_candidate_derived_retarget(
    frozen_genesis_hash: &str,
    selected_tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    trial_k: usize,
) -> Result<(V3RetargetDecision, BTreeMap<BlockId, Option<BlockId>>), PulseError> {
    let trial = derive_dormant_native_v3_ghostdag_trial(
        CHAIN,
        GENESIS_NS,
        frozen_genesis_hash,
        std::slice::from_ref(selected_tip),
        headers,
        trial_k,
    )?;
    let derived: BTreeMap<BlockId, Option<BlockId>> = trial
        .blocks
        .iter()
        .map(|(hash, metadata)| (hash.clone(), metadata.selected_parent.clone()))
        .collect();
    let decision = verify_dormant_frozen_genesis_retarget_v3_ns(
        CHAIN,
        GENESIS_NS,
        frozen_genesis_hash,
        selected_tip,
        headers,
        &derived,
    )?;
    Ok((decision, derived))
}

#[test]
fn authenticated_heavy_parent_candidate_drives_near_genesis_retarget_not_caller_map() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header)]);
    let (easy, easy_header) = mine(
        vec![genesis.hash.clone()],
        1,
        1,
        GENESIS_NS + 500_000_000,
        EASY_BITS,
        0x11,
    );
    headers.insert(easy.clone(), easy_header);
    let (hard, hard_header) = mine(
        vec![genesis.hash.clone()],
        1,
        1,
        GENESIS_NS + 600_000_000,
        HARD_BITS,
        0x12,
    );
    headers.insert(hard.clone(), hard_header);
    let (join, join_header) = mine(
        vec![easy.clone(), hard.clone()],
        2,
        2, // Experimental K=0: the other sibling is red.
        GENESIS_NS + 1_500_000_000,
        EASY_BITS,
        0x13,
    );
    headers.insert(join.clone(), join_header);
    let (decision, derived) =
        preview_candidate_derived_retarget(&genesis.hash, &join, &headers, 0).unwrap();
    assert_eq!(derived[&join], Some(hard.clone()));
    assert_eq!(derived[&genesis.hash], None);
    assert_eq!(decision.observed_intervals, 2);
    assert_eq!(decision.average_interval_ns, 750_000_000);
    let trusted_samples =
        selected_parent_retarget_window_v3_ns(CHAIN, &join, &headers, &derived).unwrap();
    assert_eq!(trusted_samples.len(), 3);
    assert_eq!(trusted_samples[1].timestamp_ns, GENESIS_NS + 600_000_000);
    assert_eq!(trusted_samples[2].timestamp_ns, GENESIS_NS);

    // The historical retarget adapter can verify a PoW-valid *wrong* parent
    // map supplied externally; it does NOT derive candidate GHOSTDAG authority.
    // Never permit this caller map to replace the independently derived one.
    let mut caller_forgery = derived.clone();
    caller_forgery.insert(join.clone(), Some(easy.clone()));
    let forged_samples =
        selected_parent_retarget_window_v3_ns(CHAIN, &join, &headers, &caller_forgery).unwrap();
    assert_eq!(forged_samples[1].timestamp_ns, GENESIS_NS + 500_000_000);
    assert_ne!(forged_samples[1], trusted_samples[1]);

    let mut incomplete = headers.clone();
    incomplete.remove(&easy);
    assert!(preview_candidate_derived_retarget(&genesis.hash, &join, &incomplete, 0).is_err());
    assert!(preview_candidate_derived_retarget("00", &join, &headers, 0).is_err());
    assert_eq!(headers.len(), 4);
}

#[test]
fn verified_trial_tip_beyond_genesis_window_still_fails_without_prune_proof() {
    let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
    let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header)]);
    let mut tip = genesis.hash.clone();
    for height in 1..=20_u64 {
        let (next, header) = mine(
            vec![tip],
            height,
            height,
            GENESIS_NS + height * 500_000_000,
            EASY_BITS,
            height as u8,
        );
        headers.insert(next.clone(), header);
        tip = next;
    }
    // Trial K=0 can authenticate the complete native parent DAG and derive
    // a selected spine, but the retarget guard deliberately refuses to
    // promote a window without its own trusted genesis/checkpoint boundary.
    let err = preview_candidate_derived_retarget(&genesis.hash, &tip, &headers, 0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("outside the verified retarget window"), "{err}");
}
