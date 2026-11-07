//! DORMANT full-unpruned native-v3 retarget trial, NOT consensus authority.
//!
//! Unlike the intentionally strict near-genesis 20-sample guard, this bounded
//! diagnostic first authenticates the COMPLETE ancestor DAG back to the exact
//! independently frozen genesis, THEN derives a proposed K-specific selected
//! parent map and the newest 20-sample retarget window. It can therefore
//! study fully populated windows with genesis outside the sampled timestamps.
//! It never accepts a pruned checkpoint, authenticates money, selects a live
//! tip, or authorizes v3 mining, P2P, contracts, rewards or UTXO changes.
//! GD-01..08 and the trial K are UNAPPROVED, so do not use this in production.

use std::collections::BTreeMap;

use crate::{
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_ghostdag_policy_candidate_v3::derive_dormant_native_v3_ghostdag_trial,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_v3::{expected_difficulty_for_v3_window_ns, V3RetargetDecision},
    types::BlockId,
};

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "UNAPPROVED full-history v3 retarget trial: {}",
        message.into()
    ))
}

/// Derive an offline retarget preview only AFTER authenticating every ancestor.
///
/// `frozen_genesis_hash`, `frozen_genesis_timestamp_ns` and `chain_id`
/// MUST come from an independently frozen network identity. The K parameter
/// is deliberately mandatory: there is NO implied or production default.
/// A missing historical ancestor, including outside the 20-sample window,
/// rejects instead of trusting a partial retained/pruned selected-parent map.
///
/// This is an unpruned, bounded diagnostic (currently <= 128 headers); it
/// cannot replace the checkpoint/finality trust proof in #1367.
pub fn preview_dormant_full_history_candidate_retarget_v3_ns(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    selected_tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    trial_k: usize,
) -> Result<V3RetargetDecision, PulseError> {
    // This verifies all reachable hashes, native KHeavyHash work, edges,
    // exact frozen genesis, candidate blue scores and bounded K calculations.
    // In particular, it does NOT accept a partial/pruned ancestor closure.
    let trial = derive_dormant_native_v3_ghostdag_trial(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        std::slice::from_ref(selected_tip),
        headers,
        trial_k,
    )?;
    // No caller-supplied selected-parent metadata can influence this preview.
    let selected_parents: BTreeMap<BlockId, Option<BlockId>> = trial
        .blocks
        .into_iter()
        .map(|(hash, meta)| (hash, meta.selected_parent))
        .collect();
    let samples =
        selected_parent_retarget_window_v3_ns(chain_id, selected_tip, headers, &selected_parents)
            .map_err(|error| invalid(format!("authenticated selected window: {error}")))?;
    expected_difficulty_for_v3_window_ns(&samples)
        .map_err(|error| invalid(format!("subsecond retarget arithmetic: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis_v3::dormant_native_genesis_envelope_v3_ns,
        header_v3::compute_block_hash_v3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        retarget_context_v3::selected_parent_retarget_window_v3_ns,
        retarget_genesis_guard_v3::verify_dormant_frozen_genesis_retarget_v3_ns,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "TEST-ONLY-full-history-retarget-not-v3-consensus";
    const GENESIS_NS: u64 = 1_800_000_000_750_000_000;
    const BITS: u32 = 0x207f_ffff;
    const INTERVAL_NS: u64 = 500_000_000;

    fn mine(parent: &str, height: u64) -> (BlockId, BlockHeaderV3) {
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![parent.to_owned()],
            timestamp_ns: GENESIS_NS + height * INTERVAL_NS,
            difficulty: BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: "33".repeat(32), // Test-only, NOT a monetary commitment.
            blue_score: height,
            height,
        };
        for nonce in 0..2048_u64 {
            header.nonce = nonce;
            if evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted {
                let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
                return (hash, header);
            }
        }
        panic!("bounded test-only easy-PoW search exhausted");
    }

    type History = (
        BlockId,
        BlockId,
        Vec<BlockId>,
        BTreeMap<BlockId, BlockHeaderV3>,
        BTreeMap<BlockId, Option<BlockId>>,
    );

    fn full_history() -> History {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        assert_eq!(genesis.header.difficulty, BITS);
        let mut headers = BTreeMap::from([(genesis.hash.clone(), genesis.header)]);
        let mut selected = BTreeMap::from([(genesis.hash.clone(), None)]);
        let mut parent = genesis.hash.clone();
        let mut added = Vec::new();
        for height in 1..=21 {
            let (hash, header) = mine(&parent, height);
            selected.insert(hash.clone(), Some(parent));
            headers.insert(hash.clone(), header);
            parent = hash.clone();
            added.push(hash);
        }
        (genesis.hash, parent, added, headers, selected)
    }

    #[test]
    fn complete_unpruned_ancestry_supports_a_full_window_beyond_genesis() {
        let (genesis, tip, _, headers, selected) = full_history();
        let sampled = selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected)
            .unwrap();
        assert_eq!(sampled.len(), 20);
        assert!(verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, GENESIS_NS, &genesis, &tip, &headers, &selected,
        )
        .is_err()); // The near-genesis guard correctly refuses this boundary.
        for k in [0, 1] {
            let decision = preview_dormant_full_history_candidate_retarget_v3_ns(
                CHAIN, GENESIS_NS, &genesis, &tip, &headers, k,
            )
            .unwrap();
            assert_eq!(decision.observed_intervals, 19);
            assert_eq!(decision.average_interval_ns, INTERVAL_NS);
            assert_eq!(decision.expected_bits, BITS);
        }
    }

    #[test]
    fn missing_or_forged_old_ancestor_outside_window_fails_closed() {
        let (genesis, tip, added, headers, selected) = full_history();
        let oldest_mined = &added[0]; // Height 1 is outside the 20 newest samples.
        let mut absent = headers.clone();
        absent.remove(oldest_mined);
        assert!(selected_parent_retarget_window_v3_ns(CHAIN, &tip, &absent, &selected).is_ok());
        assert!(preview_dormant_full_history_candidate_retarget_v3_ns(
            CHAIN, GENESIS_NS, &genesis, &tip, &absent, 0,
        )
        .is_err());

        let mut forged = headers.clone();
        forged.get_mut(oldest_mined).unwrap().nonce ^= 1;
        assert!(selected_parent_retarget_window_v3_ns(CHAIN, &tip, &forged, &selected).is_ok());
        assert!(preview_dormant_full_history_candidate_retarget_v3_ns(
            CHAIN, GENESIS_NS, &genesis, &tip, &forged, 0,
        )
        .is_err());
        assert!(preview_dormant_full_history_candidate_retarget_v3_ns(
            "foreign-network", GENESIS_NS, &genesis, &tip, &headers, 0,
        )
        .is_err());
        assert!(preview_dormant_full_history_candidate_retarget_v3_ns(
            CHAIN, GENESIS_NS + 1, &genesis, &tip, &headers, 0,
        )
        .is_err());
        assert!(preview_dormant_full_history_candidate_retarget_v3_ns(
            CHAIN, GENESIS_NS, &genesis, &tip, &headers, 129,
        )
        .is_err());
    }
}
