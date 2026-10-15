//! Dormant, genesis-anchored subset of native-v3 selected-parent retarget.
//!
//! This high-assurance DIAGNOSTIC accepts only the initial selected-parent
//! history for which the frozen network genesis itself appears within the
//! 20-sample retarget window. It refuses pruned/older windows that do not reach
//! genesis rather than pretending to authenticate their historical boundary.
//! It does NOT derive GHOSTDAG selected-parent choice or blue-work, verify
//! monetary replay/state roots, operate on RocksDB, or enable P2P/mining.
//! Never treat its result as an admission token or a mainnet readiness gate.

use std::collections::BTreeMap;

use crate::{
    errors::PulseError,
    genesis_v3::dormant_native_genesis_envelope_v3_ns,
    header_v3::BlockHeaderV3,
    retarget_context_v3::selected_parent_retarget_window_v3_ns,
    retarget_v3::{expected_difficulty_for_v3_window_ns, V3RetargetDecision},
    types::BlockId,
};

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!(
        "dormant frozen-genesis v3 retarget: {}",
        message.into()
    ))
}

/// Check a near-genesis v3 selected-parent window against an *external* freeze.
///
/// `frozen_genesis_hash` and timestamp MUST be read from a distinct trusted
/// network config, never from `headers`, `selected_parents` or P2P traffic.
/// Existing bounded retarget validation authenticates header hashes, KHeavyHash
/// for every non-genesis sample, selected-parent linkage, and nanosecond order.
/// This extra guard then requires the exact canonical, unmined genesis header
/// among those samples. The window cannot prove selection or blue-work.
pub fn verify_dormant_frozen_genesis_retarget_v3_ns(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    selected_tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    selected_parents: &BTreeMap<BlockId, Option<BlockId>>,
) -> Result<V3RetargetDecision, PulseError> {
    let genesis = dormant_native_genesis_envelope_v3_ns(chain_id, frozen_genesis_timestamp_ns)
        .map_err(|error| invalid(format!("invalid frozen genesis inputs: {error}")))?;
    if genesis.hash != frozen_genesis_hash {
        return Err(invalid(
            "external genesis hash differs from canonical chain-bound nanosecond genesis",
        ));
    }

    let samples = selected_parent_retarget_window_v3_ns(
        chain_id,
        selected_tip,
        headers,
        selected_parents,
    )
    .map_err(|error| invalid(format!("selected-parent PoW/window: {error}")))?;
    let mut current = selected_tip.clone();
    let mut anchored = false;
    for index in 0..samples.len() {
        let header = headers
            .get(&current)
            .ok_or_else(|| invalid(format!("selected ancestor {current} is missing")))?;
        if header.height == 0 {
            if current != genesis.hash
                || header != &genesis.header
                || selected_parents.get(&current) != Some(&None)
            {
                return Err(invalid(
                    "height-zero selected ancestor is not the exact externally frozen genesis",
                ));
            }
            anchored = true;
            break;
        }
        // A fully populated window need not store the 21st selected-parent
        // edge. That is a deliberately unproven historical boundary here.
        if index + 1 == samples.len() {
            break;
        }
        current = selected_parents
            .get(&current)
            .and_then(Option::as_ref)
            .cloned()
            .ok_or_else(|| invalid(format!("selected ancestor {current} has no next parent")))?;
    }
    if !anchored {
        return Err(invalid(
            "frozen genesis is outside the verified retarget window; separately authenticate any pruned history boundary",
        ));
    }

    expected_difficulty_for_v3_window_ns(&samples)
        .map_err(|error| invalid(format!("v3 retarget arithmetic: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block_envelope_v3::BlockEnvelopeV3,
        header_v3::compute_block_hash_v3,
        pow_v3::evaluate_v3_header_pow,
        protocol::BLOCK_HEADER_VERSION_V3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "dormant-frozen-selected-retarget-v3";
    const FROZEN_NS: u64 = 1_800_000_000_750_000_000;

    fn append_mined(
        headers: &mut BTreeMap<BlockId, BlockHeaderV3>,
        selection: &mut BTreeMap<BlockId, Option<BlockId>>,
        parent: &str,
        height: u64,
        difficulty: u32,
    ) -> String {
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents: vec![parent.to_string()],
            timestamp_ns: FROZEN_NS + height * 500_000_000,
            difficulty,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        };
        let nonce = (0..1_024_u64)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted
            })
            .expect("easy-target native-v3 selected-parent fixture mines");
        header.nonce = nonce;
        let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
        headers.insert(hash.clone(), header);
        selection.insert(hash.clone(), Some(parent.to_string()));
        hash
    }

    fn frozen_fixture() -> (
        BlockEnvelopeV3,
        BTreeMap<BlockId, BlockHeaderV3>,
        BTreeMap<BlockId, Option<BlockId>>,
    ) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, FROZEN_NS).unwrap();
        let mut headers = BTreeMap::new();
        let mut selection = BTreeMap::new();
        headers.insert(genesis.hash.clone(), genesis.header.clone());
        selection.insert(genesis.hash.clone(), None);
        (genesis, headers, selection)
    }

    #[test]
    fn validates_only_a_genuinely_frozen_near_genesis_window() {
        let (genesis, mut headers, mut selected) = frozen_fixture();
        let first = append_mined(
            &mut headers,
            &mut selected,
            &genesis.hash,
            1,
            genesis.header.difficulty,
        );
        let tip = append_mined(
            &mut headers,
            &mut selected,
            &first,
            2,
            genesis.header.difficulty,
        );
        let result = verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN,
            FROZEN_NS,
            &genesis.hash,
            &tip,
            &headers,
            &selected,
        )
        .unwrap();
        assert_eq!(result.average_interval_ns, 500_000_000);
        assert_eq!(result.observed_intervals, 2);
        assert_eq!(result.expected_bits, genesis.header.difficulty);
    }

    #[test]
    fn refuses_a_hash_valid_but_unfrozen_height_zero_ancestor() {
        let (genesis, mut headers, mut selected) = frozen_fixture();
        let mut forged = genesis.header.clone();
        forged.nonce += 1;
        let forged_hash = compute_block_hash_v3(&forged, CHAIN).unwrap();
        headers.insert(forged_hash.clone(), forged);
        selected.insert(forged_hash.clone(), None);
        let tip = append_mined(
            &mut headers,
            &mut selected,
            &forged_hash,
            1,
            genesis.header.difficulty,
        );
        // The unbound window helper considers arbitrary height-zero headers
        // non-mined anchors; this new subset guard intentionally rejects one.
        assert!(selected_parent_retarget_window_v3_ns(
            CHAIN, &tip, &headers, &selected,
        )
        .is_ok());
        let error = verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, FROZEN_NS, &genesis.hash, &tip, &headers, &selected,
        )
        .unwrap_err();
        assert!(error.to_string().contains("externally frozen genesis"));
    }

    #[test]
    fn refuses_cross_freeze_and_wrong_genesis_metadata() {
        let (genesis, mut headers, mut selected) = frozen_fixture();
        let tip = append_mined(
            &mut headers,
            &mut selected,
            &genesis.hash,
            1,
            genesis.header.difficulty,
        );
        assert!(verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, FROZEN_NS + 1, &genesis.hash, &tip, &headers, &selected,
        )
        .is_err());
        assert!(verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, FROZEN_NS, "00", &tip, &headers, &selected,
        )
        .is_err());
        assert!(verify_dormant_frozen_genesis_retarget_v3_ns(
            "other-network", FROZEN_NS, &genesis.hash, &tip, &headers, &selected,
        )
        .is_err());
        selected.insert(genesis.hash.clone(), Some("aa".repeat(32)));
        assert!(verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, FROZEN_NS, &genesis.hash, &tip, &headers, &selected,
        )
        .is_err());
    }

    #[test]
    fn refuses_truncated_twenty_sample_window_without_genesis_anchor() {
        let (genesis, mut headers, mut selected) = frozen_fixture();
        let mut tip = genesis.hash.clone();
        for height in 1..=20 {
            tip = append_mined(
                &mut headers,
                &mut selected,
                &tip,
                height,
                genesis.header.difficulty,
            );
        }
        // The existing retarget helper correctly verifies these 20 mined
        // samples without demanding the 21st/older genesis.
        assert_eq!(
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected)
                .unwrap()
                .len(),
            20
        );
        let error = verify_dormant_frozen_genesis_retarget_v3_ns(
            CHAIN, FROZEN_NS, &genesis.hash, &tip, &headers, &selected,
        )
        .unwrap_err();
        assert!(error.to_string().contains("outside the verified retarget window"));
    }
}
