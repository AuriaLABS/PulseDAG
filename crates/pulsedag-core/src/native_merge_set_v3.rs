//! Dormant, policy-neutral native-v3 merge-set set-algebra diagnostic.
//!
//! With complete PoW-authenticated native-v3 multi-parent ancestry, the
//! merge-set relative to a *specified* parent is:
//! union(past(other parents)) minus past(specified selected parent),
//! with inclusive past sets. This does NOT determine WHICH parent is selected,
//! classify blues/reds, derive blue-work or establish GHOSTDAG consensus.
//! The return value must never serve as network admission/replay authority.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    errors::PulseError,
    header_v3::BlockHeaderV3,
    native_parent_dag_context_v3::{
        verify_dormant_native_v3_parent_dag, DormantV3ParentDagCheck, MAX_DORMANT_V3_DAG_HEADERS,
    },
    types::BlockId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DormantV3MergeSetCheck {
    pub supplied_selected_parent: BlockId,
    /// Lexically sorted set-algebra diagnostic only, NOT canonical blue order.
    pub unordered_merge_set: Vec<BlockId>,
    pub selected_parent_past_size: usize,
    pub verified_dag: DormantV3ParentDagCheck,
}

fn invalid(message: impl Into<String>) -> PulseError {
    PulseError::InvalidBlock(format!("dormant native v3 merge set: {}", message.into()))
}

/// Reconstruct the inclusive parent past only from the already verified
/// complete DAG. Traversal remains bounded even if future callers change
/// the graph-validation implementation or offer a cyclic/pruned map.
fn inclusive_past(
    root: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Result<BTreeSet<BlockId>, PulseError> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.clone()];
    while let Some(hash) = pending.pop() {
        if !seen.insert(hash.clone()) {
            continue;
        }
        if seen.len() > MAX_DORMANT_V3_DAG_HEADERS {
            return Err(invalid("ancestor visit budget exceeded"));
        }
        let header = headers
            .get(&hash)
            .ok_or_else(|| invalid(format!("missing verified ancestor {hash}")))?;
        pending.extend(header.parents.iter().cloned());
    }
    Ok(seen)
}

/// Compute the bounded graph-theoretic merge set for a caller-specified parent.
///
/// The inputs must describe all immediate candidate parents and complete
/// native v3 ancestry back to the *externally trusted* exact unmined genesis.
/// We first validate the entire reachable parent DAG, including side-branch
/// KHeavyHash/target, before trusting any graph relation or doing set algebra.
///
/// The caller-specified selected parent is only a comparison parameter.
/// THIS DOES NOT authenticate that choice, any blue/red assignment, blue score
/// or PoW-weighted accumulated blue-work. It never activates mining or P2P.
pub fn derive_dormant_native_v3_merge_set_for_parent(
    chain_id: &str,
    frozen_genesis_timestamp_ns: u64,
    frozen_genesis_hash: &str,
    candidate_parents: &[BlockId],
    supplied_selected_parent: &BlockId,
    headers: &BTreeMap<BlockId, BlockHeaderV3>,
) -> Result<DormantV3MergeSetCheck, PulseError> {
    if !candidate_parents.contains(supplied_selected_parent) {
        return Err(invalid(
            "supplied comparison parent not listed in candidate parents",
        ));
    }
    let verified_dag = verify_dormant_native_v3_parent_dag(
        chain_id,
        frozen_genesis_timestamp_ns,
        frozen_genesis_hash,
        candidate_parents,
        headers,
    )?;
    let selected_past = inclusive_past(supplied_selected_parent, headers)?;
    let mut unordered_merge_set = BTreeSet::new();
    for parent in candidate_parents {
        if parent == supplied_selected_parent {
            continue;
        }
        let other_past = inclusive_past(parent, headers)?;
        for ancestor in other_past {
            if !selected_past.contains(&ancestor) {
                unordered_merge_set.insert(ancestor);
                if unordered_merge_set.len() > MAX_DORMANT_V3_DAG_HEADERS {
                    return Err(invalid("merge-set size exceeds verified DAG budget"));
                }
            }
        }
    }

    Ok(DormantV3MergeSetCheck {
        supplied_selected_parent: supplied_selected_parent.clone(),
        unordered_merge_set: unordered_merge_set.into_iter().collect(),
        selected_parent_past_size: selected_past.len(),
        verified_dag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        genesis_v3::dormant_native_genesis_envelope_v3_ns, header_v3::compute_block_hash_v3,
        pow_v3::evaluate_v3_header_pow, protocol::BLOCK_HEADER_VERSION_V3,
        types::compute_merkle_root,
    };

    const CHAIN: &str = "dormant-native-v3-merge-set";
    const GENESIS_NS: u64 = 1_800_000_000_500_000_000;
    const EASY_BITS: u32 = 0x207f_ffff;

    fn fixture() -> (BlockId, BTreeMap<BlockId, BlockHeaderV3>) {
        let genesis = dormant_native_genesis_envelope_v3_ns(CHAIN, GENESIS_NS).unwrap();
        let mut headers = BTreeMap::new();
        headers.insert(genesis.hash.clone(), genesis.header);
        (genesis.hash, headers)
    }

    fn append(
        headers: &mut BTreeMap<BlockId, BlockHeaderV3>,
        mut parents: Vec<BlockId>,
        height: u64,
        timestamp_ns: u64,
        valid_pow: bool,
    ) -> BlockId {
        parents.sort();
        let mut header = BlockHeaderV3 {
            version: BLOCK_HEADER_VERSION_V3,
            parents,
            timestamp_ns,
            difficulty: EASY_BITS,
            nonce: 0,
            merkle_root: compute_merkle_root(&[]),
            state_root: "33".repeat(32),
            blue_score: height,
            height,
        };
        header.nonce = (0..1_024_u64)
            .find(|nonce| {
                header.nonce = *nonce;
                evaluate_v3_header_pow(&header, CHAIN).unwrap().pow.accepted == valid_pow
            })
            .expect("easy target fixture nonce must exist");
        let hash = compute_block_hash_v3(&header, CHAIN).unwrap();
        headers.insert(hash.clone(), header);
        hash
    }

    fn examine(
        genesis: &str,
        parents: &[BlockId],
        selected: &BlockId,
        headers: &BTreeMap<BlockId, BlockHeaderV3>,
    ) -> Result<DormantV3MergeSetCheck, PulseError> {
        derive_dormant_native_v3_merge_set_for_parent(
            CHAIN, GENESIS_NS, genesis, parents, selected, headers,
        )
    }

    #[test]
    fn excludes_selected_parent_past_and_retains_parallel_branch() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let b = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            true,
        );
        let mut parents = vec![a.clone(), b.clone()];
        parents.sort();
        let result = examine(&genesis, &parents, &a, &headers).unwrap();
        assert_eq!(result.unordered_merge_set, vec![b.clone()]);
        assert_eq!(result.selected_parent_past_size, 2);
        assert_eq!(result.verified_dag.authenticated_non_genesis_headers, 2);

        let other = examine(&genesis, &parents, &b, &headers).unwrap();
        assert_eq!(other.unordered_merge_set, vec![a]);
    }

    #[test]
    fn includes_complete_side_history_but_never_shared_selected_past() {
        let (genesis, mut headers) = fixture();
        let common = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let selected = append(
            &mut headers,
            vec![common.clone()],
            2,
            GENESIS_NS + 1_000_000_000,
            true,
        );
        let side = append(
            &mut headers,
            vec![common.clone()],
            2,
            GENESIS_NS + 1_100_000_000,
            true,
        );
        let side_tip = append(
            &mut headers,
            vec![side.clone()],
            3,
            GENESIS_NS + 1_600_000_000,
            true,
        );
        let mut parents = vec![selected.clone(), side_tip.clone()];
        parents.sort();
        let result = examine(&genesis, &parents, &selected, &headers).unwrap();
        let mut expected = vec![side, side_tip];
        expected.sort();
        assert_eq!(result.unordered_merge_set, expected);
        assert_eq!(result.selected_parent_past_size, 3);
        assert_eq!(result.verified_dag.authenticated_non_genesis_headers, 4);
    }

    #[test]
    fn three_parent_merge_uses_union_of_all_other_inclusive_pasts() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let b = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            true,
        );
        let joined = append(
            &mut headers,
            vec![a.clone(), b.clone()],
            2,
            GENESIS_NS + 1_000_000_000,
            true,
        );
        let a_tip = append(
            &mut headers,
            vec![a.clone()],
            2,
            GENESIS_NS + 1_100_000_000,
            true,
        );
        let b_tip = append(
            &mut headers,
            vec![b.clone()],
            2,
            GENESIS_NS + 1_200_000_000,
            true,
        );
        let mut parents = vec![joined.clone(), a_tip.clone(), b_tip.clone()];
        parents.sort();

        // Both old branches are already in the joined parent's inclusive
        // past. Their new tips, but not the shared ancestors, remain.
        let for_joined = examine(&genesis, &parents, &joined, &headers).unwrap();
        let mut expected = vec![a_tip.clone(), b_tip.clone()];
        expected.sort();
        assert_eq!(for_joined.unordered_merge_set, expected);
        assert_eq!(for_joined.selected_parent_past_size, 4);
        assert_eq!(for_joined.verified_dag.authenticated_non_genesis_headers, 5);

        // Changing only the comparison parent must include the other branch's
        // shared ancestor once, plus the join and the other branch's tip.
        let for_a = examine(&genesis, &parents, &a_tip, &headers).unwrap();
        let mut expected = vec![b.clone(), joined.clone(), b_tip.clone()];
        expected.sort();
        assert_eq!(for_a.unordered_merge_set, expected);

        let for_b = examine(&genesis, &parents, &b_tip, &headers).unwrap();
        let mut expected = vec![a, joined, a_tip];
        expected.sort();
        assert_eq!(for_b.unordered_merge_set, expected);
    }

    #[test]
    fn ancestor_parent_is_not_a_duplicate_merge_set_entry() {
        let (genesis, mut headers) = fixture();
        let ancestor = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let descendant = append(
            &mut headers,
            vec![ancestor.clone()],
            2,
            GENESIS_NS + 1_000_000_000,
            true,
        );
        let mut parents = vec![ancestor.clone(), descendant.clone()];
        parents.sort();

        // A redundant parent already belongs to the chosen descendant's
        // inclusive past, so no merge-set member remains.
        let newer = examine(&genesis, &parents, &descendant, &headers).unwrap();
        assert!(newer.unordered_merge_set.is_empty());
        assert_eq!(newer.selected_parent_past_size, 3);

        // The reverse comparison is not symmetric: the newer parent's
        // inclusive past contains exactly one block absent from the older.
        let older = examine(&genesis, &parents, &ancestor, &headers).unwrap();
        assert_eq!(older.unordered_merge_set, vec![descendant]);
    }

    #[test]
    fn single_parent_has_no_merge_set() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let result = examine(&genesis, std::slice::from_ref(&a), &a, &headers).unwrap();
        assert!(result.unordered_merge_set.is_empty());
    }

    #[test]
    fn rejects_missing_selected_parent_duplicate_parents_or_missing_side_header() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let b = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            true,
        );
        let mut parents = vec![a.clone(), b.clone()];
        parents.sort();
        assert!(examine(&genesis, &parents, &genesis, &headers).is_err());
        assert!(examine(&genesis, &[a.clone(), a.clone()], &a, &headers).is_err());
        headers.remove(&b);
        assert!(examine(&genesis, &parents, &b, &headers).is_err());
    }

    #[test]
    fn bad_side_branch_pow_and_forged_genesis_fail_before_set_algebra() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let b = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            false,
        );
        let mut parents = vec![a.clone(), b.clone()];
        parents.sort();
        let error = examine(&genesis, &parents, &a, &headers).unwrap_err();
        assert!(error.to_string().contains("invalid PoW"));

        assert!(examine("00", std::slice::from_ref(&a), &a, &headers).is_err());
        headers.remove(&genesis);
        assert!(examine(&genesis, std::slice::from_ref(&a), &a, &headers).is_err());
    }

    #[test]
    fn cannot_promote_noncanonical_order_or_cross_network_input() {
        let (genesis, mut headers) = fixture();
        let a = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 500_000_000,
            true,
        );
        let b = append(
            &mut headers,
            vec![genesis.clone()],
            1,
            GENESIS_NS + 600_000_000,
            true,
        );
        let mut parents = vec![a.clone(), b];
        parents.sort();
        parents.reverse();
        assert!(examine(&genesis, &parents, &a, &headers).is_err());
        assert!(derive_dormant_native_v3_merge_set_for_parent(
            "other-network",
            GENESIS_NS,
            &genesis,
            std::slice::from_ref(&a),
            &a,
            &headers,
        )
        .is_err());
    }
}
