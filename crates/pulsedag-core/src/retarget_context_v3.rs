//! Dormant v3 selected-parent retarget adapter.
//!
//! V3 consensus cannot derive 500ms difficulty from v1/v2 Unix-second headers,
//! or from a caller-invented array of nanosecond samples. This adapter checks
//! the integrity of the v3 header/selected-parent linkage before constructing
//! the bounded newest-first retarget window. The selected-parent map MUST be
//! produced by independently verified consensus metadata; this adapter does
//! not select parents, validate DAG blue work, PoW, or activate a protocol.

use std::collections::{BTreeMap, HashSet};

use thiserror::Error;

use crate::{
    header_v3::{compute_block_hash_v3, BlockHeaderV3},
    pow::{bits_from_target, target_from_bits},
    retarget::{consensus_min_target, consensus_pow_limit_target},
    retarget_v3::{
        expected_difficulty_for_v3_window_ns, V3RetargetDecision, V3RetargetSample,
        V3SubsecondConsensusError, PRODUCTION_V3_RETARGET_WINDOW,
    },
    types::BlockId,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum V3SelectedParentWindowError {
    #[error("v3 selected-parent tip cannot be empty")]
    EmptyTip,
    #[error("v3 selected-parent header missing: {0}")]
    MissingHeader(BlockId),
    #[error("v3 selected-parent metadata missing: {0}")]
    MissingSelection(BlockId),
    #[error("v3 header invalid for {hash}: {reason}")]
    InvalidHeader { hash: BlockId, reason: String },
    #[error("v3 header key {hash} differs from canonical v3 hash {computed}")]
    HeaderHashMismatch { hash: BlockId, computed: BlockId },
    #[error("v3 non-genesis header {0} has no selected-parent identity")]
    MissingNonGenesisSelection(BlockId),
    #[error("v3 header {hash} has invalid/noncanonical compact difficulty {bits:#010x}")]
    InvalidTargetBits { hash: BlockId, bits: u32 },
    #[error("v3 selected parent {parent} is not listed by header {hash}")]
    ParentNotListed { hash: BlockId, parent: BlockId },
    #[error("v3 selected parent height is not lower: header {hash}, parent {parent}")]
    NonIncreasingHeight { hash: BlockId, parent: BlockId },
    #[error("v3 selected-parent metadata contains a cycle at {0}")]
    SelectionCycle(BlockId),
    #[error(transparent)]
    Retarget(#[from] V3SubsecondConsensusError),
}

/// Build the exact newest-first, bounded timestamp window for a *previously
/// verified* selected-parent tip. Entries are authenticated against the
/// chain-bound v3 header hash, and each selected-parent edge is checked against
/// the actual header parent list. No v1/v2 header can enter this path.
///
/// This API verifies linkage integrity, not the authority of a supplied
/// selected-parent map. A future caller must derive that map from validated
/// deterministic selection/blue-work metadata, never directly from P2P/RPC.
pub fn selected_parent_retarget_window_v3_ns(
    chain_id: &str,
    selected_tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    selected_parents: &BTreeMap<BlockId, Option<BlockId>>,
) -> Result<Vec<V3RetargetSample>, V3SelectedParentWindowError> {
    if selected_tip.is_empty() {
        return Err(V3SelectedParentWindowError::EmptyTip);
    }
    let mut current = selected_tip.clone();
    let mut visited = HashSet::with_capacity(PRODUCTION_V3_RETARGET_WINDOW);
    let mut samples = Vec::with_capacity(PRODUCTION_V3_RETARGET_WINDOW);
    let mut previous_child: Option<(BlockId, u64)> = None;

    loop {
        if !visited.insert(current.clone()) {
            return Err(V3SelectedParentWindowError::SelectionCycle(current));
        }
        let header = headers
            .get(&current)
            .ok_or_else(|| V3SelectedParentWindowError::MissingHeader(current.clone()))?;
        let computed = compute_block_hash_v3(header, chain_id).map_err(|error| {
            V3SelectedParentWindowError::InvalidHeader {
                hash: current.clone(),
                reason: error.to_string(),
            }
        })?;
        if computed != current {
            return Err(V3SelectedParentWindowError::HeaderHashMismatch {
                hash: current,
                computed,
            });
        }
        if let Some((child_hash, child_height)) = &previous_child {
            if header.height >= *child_height {
                return Err(V3SelectedParentWindowError::NonIncreasingHeight {
                    hash: child_hash.clone(),
                    parent: current,
                });
            }
        }

        let target = target_from_bits(header.difficulty);
        if header.difficulty != bits_from_target(&target)
            || target < consensus_min_target()
            || target > consensus_pow_limit_target()
        {
            return Err(V3SelectedParentWindowError::InvalidTargetBits {
                hash: current,
                bits: header.difficulty,
            });
        }

        samples.push(V3RetargetSample {
            timestamp_ns: header.timestamp_ns,
            bits: header.difficulty,
        });

        let selected = selected_parents
            .get(&current)
            .ok_or_else(|| V3SelectedParentWindowError::MissingSelection(current.clone()))?;
        let selected_parent = match selected {
            Some(parent) => {
                if !header.parents.contains(parent) {
                    return Err(V3SelectedParentWindowError::ParentNotListed {
                        hash: current,
                        parent: parent.clone(),
                    });
                }
                parent.clone()
            }
            None if header.height == 0 => break,
            None => {
                return Err(V3SelectedParentWindowError::MissingNonGenesisSelection(
                    current,
                ))
            }
        };

        if samples.len() == PRODUCTION_V3_RETARGET_WINDOW {
            break;
        }
        previous_child = Some((current, header.height));
        current = selected_parent;
    }

    // The core retarget math itself enforces strictly decreasing nanosecond
    // timestamps, never a truncation or conversion to Unix seconds.
    expected_difficulty_for_v3_window_ns(&samples)?;
    Ok(samples)
}

/// Run the frozen 500ms arithmetic on authenticated v3 selected-parent samples.
/// Deliberately does not plug into mining templates or mined/P2P acceptance.
pub fn expected_difficulty_for_selected_parent_v3_ns(
    chain_id: &str,
    selected_tip: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
    selected_parents: &BTreeMap<BlockId, Option<BlockId>>,
) -> Result<V3RetargetDecision, V3SelectedParentWindowError> {
    let window =
        selected_parent_retarget_window_v3_ns(chain_id, selected_tip, headers, selected_parents)?;
    Ok(expected_difficulty_for_v3_window_ns(&window)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::BLOCK_HEADER_VERSION_V3;

    const CHAIN: &str = "pulsedag-v3-window-candidate";
    const BASE: u64 = 1_800_000_000_000_000_000;
    const BITS: u32 = 0x1e0f_ffff;

    fn header(parents: Vec<BlockId>, height: u64, timestamp_ns: u64) -> BlockHeaderV3 {
        BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns,
            difficulty: BITS,
            nonce: height,
            merkle_root: "22".repeat(32),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        }
    }

    fn insert(
        headers: &mut BTreeMap<BlockId, BlockHeaderV3>,
        selected: &mut BTreeMap<BlockId, Option<BlockId>>,
        mut parents: Vec<BlockId>,
        chosen: Option<BlockId>,
        height: u64,
        timestamp_ns: u64,
    ) -> BlockId {
        parents.sort();
        let item = header(parents, height, timestamp_ns);
        let hash = compute_block_hash_v3(&item, CHAIN).unwrap();
        headers.insert(hash.clone(), item);
        selected.insert(hash.clone(), chosen);
        hash
    }

    type Fixture = (
        BTreeMap<BlockId, BlockHeaderV3>,
        BTreeMap<BlockId, Option<BlockId>>,
        BlockId,
        BlockId,
        BlockId,
    );

    fn three_block_chain() -> Fixture {
        let mut headers = BTreeMap::new();
        let mut selected = BTreeMap::new();
        let genesis = insert(&mut headers, &mut selected, vec![], None, 0, BASE);
        let first = insert(
            &mut headers,
            &mut selected,
            vec![genesis.clone()],
            Some(genesis.clone()),
            1,
            BASE + 500_000_000,
        );
        let tip = insert(
            &mut headers,
            &mut selected,
            vec![first.clone()],
            Some(first.clone()),
            2,
            BASE + 1_000_000_000,
        );
        (headers, selected, genesis, first, tip)
    }

    #[test]
    fn exact_500ms_window_is_derived_from_chain_bound_v3_headers() {
        let (headers, selected, _, _, tip) = three_block_chain();
        let samples =
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected).unwrap();
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].timestamp_ns, BASE + 1_000_000_000);
        assert_eq!(samples[1].timestamp_ns, BASE + 500_000_000);
        assert_eq!(samples[2].timestamp_ns, BASE);
        let result =
            expected_difficulty_for_selected_parent_v3_ns(CHAIN, &tip, &headers, &selected)
                .unwrap();
        assert_eq!(result.average_interval_ns, 500_000_000);
        assert_eq!(result.expected_bits, BITS);
    }

    #[test]
    fn side_parent_does_not_replace_verified_selected_parent_history() {
        let (mut headers, mut selected, genesis, first, _) = three_block_chain();
        let side = insert(
            &mut headers,
            &mut selected,
            vec![genesis],
            None,
            1,
            BASE + 250_000_000,
        );
        // Selected metadata belongs to verified consensus, not the array order.
        selected.insert(side.clone(), Some(headers[&side].parents[0].clone()));
        let tip = insert(
            &mut headers,
            &mut selected,
            vec![first.clone(), side],
            Some(first),
            2,
            BASE + 1_000_000_000,
        );
        let result =
            expected_difficulty_for_selected_parent_v3_ns(CHAIN, &tip, &headers, &selected)
                .unwrap();
        assert_eq!(result.average_interval_ns, 500_000_000);
    }

    #[test]
    fn wrong_chain_or_forged_hash_fails_before_retarget() {
        let (mut headers, selected, _, _, tip) = three_block_chain();
        assert!(matches!(
            selected_parent_retarget_window_v3_ns("another-chain", &tip, &headers, &selected),
            Err(V3SelectedParentWindowError::HeaderHashMismatch { .. })
        ));
        headers.get_mut(&tip).unwrap().timestamp_ns += 1;
        assert!(matches!(
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected),
            Err(V3SelectedParentWindowError::HeaderHashMismatch { .. })
        ));
    }

    #[test]
    fn authenticated_header_with_noncanonical_pow_bits_cannot_feed_retarget() {
        let mut headers = BTreeMap::new();
        let mut selected = BTreeMap::new();
        let mut invalid = header(vec![], 0, BASE);
        // Hash commitment alone does not imply a canonical compact PoW target.
        invalid.difficulty = 0;
        let hash = compute_block_hash_v3(&invalid, CHAIN).unwrap();
        headers.insert(hash.clone(), invalid);
        selected.insert(hash.clone(), None);
        assert_eq!(
            selected_parent_retarget_window_v3_ns(CHAIN, &hash, &headers, &selected),
            Err(V3SelectedParentWindowError::InvalidTargetBits { hash, bits: 0 })
        );
    }

    #[test]
    fn missing_selection_and_nonparent_selection_fail_closed() {
        let (headers, mut selected, genesis, _, tip) = three_block_chain();
        selected.remove(&tip);
        assert_eq!(
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected),
            Err(V3SelectedParentWindowError::MissingSelection(tip.clone()))
        );
        selected.insert(tip.clone(), Some(genesis.clone()));
        assert_eq!(
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected),
            Err(V3SelectedParentWindowError::ParentNotListed {
                hash: tip,
                parent: genesis
            })
        );
    }

    #[test]
    fn non_genesis_without_selected_parent_fails_closed() {
        let (headers, mut selected, _, first, _) = three_block_chain();
        selected.insert(first.clone(), None);
        assert_eq!(
            selected_parent_retarget_window_v3_ns(CHAIN, &first, &headers, &selected),
            Err(V3SelectedParentWindowError::MissingNonGenesisSelection(
                first
            ))
        );
    }

    #[test]
    fn reordering_nanosecond_time_is_rejected() {
        let mut headers = BTreeMap::new();
        let mut selected = BTreeMap::new();
        let genesis = insert(&mut headers, &mut selected, vec![], None, 0, BASE);
        let first = insert(
            &mut headers,
            &mut selected,
            vec![genesis.clone()],
            Some(genesis),
            1,
            BASE + 700_000_000,
        );
        let tip = insert(
            &mut headers,
            &mut selected,
            vec![first.clone()],
            Some(first),
            2,
            BASE + 600_000_000,
        );
        assert_eq!(
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected),
            Err(V3SelectedParentWindowError::Retarget(
                V3SubsecondConsensusError::NonMonotonicWindow
            ))
        );
    }

    #[test]
    fn long_history_is_bounded_to_20_headers() {
        let mut headers = BTreeMap::new();
        let mut selected = BTreeMap::new();
        let mut tip = insert(&mut headers, &mut selected, vec![], None, 0, BASE);
        for height in 1..=24u64 {
            tip = insert(
                &mut headers,
                &mut selected,
                vec![tip.clone()],
                Some(tip),
                height,
                BASE + height * 500_000_000,
            );
        }
        let samples =
            selected_parent_retarget_window_v3_ns(CHAIN, &tip, &headers, &selected).unwrap();
        assert_eq!(samples.len(), PRODUCTION_V3_RETARGET_WINDOW);
        assert_eq!(samples[0].timestamp_ns, BASE + 24 * 500_000_000);
        assert_eq!(samples[19].timestamp_ns, BASE + 5 * 500_000_000);
    }
}
